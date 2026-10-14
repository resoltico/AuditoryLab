# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Check the reviewed libm notice spans against the locked crate's complete source.

This is a bounded audit of one pinned source version, not a Rust/license parser.
Marker coverage and exact spans supplement the manual review of all source files.
The locked libm crate must already be cached by a Cargo build or fetch. The test
does not invoke Cargo, require off-target crates, or use the network.
"""

import copy
import json
import os
from pathlib import Path
import re
import sys
import tarfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import release_archive as archive


MARKER = re.compile(rb"copyright|permission|SPDX-License-Identifier|redistribution|all\s+rights\s+reserved|licensed\s+under|\xc2\xa9", re.IGNORECASE)


def check_source_notice_coverage(sources, bundle, coverage):
    """Every bound raw notice must be present; every legal marker must be bound."""
    assert coverage.get("format") == 1 and coverage.get("files"), "Empty or unknown notice coverage"
    spans = {}
    for record in coverage["files"]:
        path = record["path"]
        assert path.startswith("src/") and ".." not in Path(path).parts and path not in spans, "Invalid coverage path"
        content = sources[path]
        assert archive.digest(content) == record["sha256"], f"Source identity changed: {path}"
        assert record["notices"], f"Empty notice selector: {path}"
        spans[path] = []
        for notice in record["notices"]:
            start, length = notice["offset"], notice["length"]
            assert start >= 0 and length > 0 and start + length <= len(content), f"Invalid notice span: {path}"
            raw = content[start:start + length]
            assert archive.digest(raw) == notice["sha256"], f"Notice span identity changed: {path}"
            assert raw in bundle, f"Complete raw legal notice missing from bundle: {path}"
            spans[path].append((start, start + length))
    markers = 0
    source_files = sorted(sources)
    assert source_files, "Empty source selector"
    for path in source_files:
        for match in MARKER.finditer(sources[path]):
            markers += 1
            assert any(start <= match.start() and match.end() <= end for start, end in spans.get(path, [])), f"Uncovered legal marker: {path}:{match.start()}"
    assert markers, "Empty legal-marker selector"
    return len(source_files), len(spans), markers


class LibmNoticeCoverage(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.policy = archive.libm_notice_policy({"name": "libm", "version": "0.2.16"})
        cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo"))).resolve()
        candidates = sorted((cargo_home / "registry" / "cache").glob("*/libm-0.2.16.crate"))
        crates = [path for path in candidates if archive.digest(path.read_bytes()) == cls.policy["crate_checksum"]]
        assert crates, "Run cargo build --workspace --locked (or cargo fetch --locked) to cache the reviewed libm crate before the notice check"
        cls.sources = {}
        prefix = "libm-0.2.16/"
        with tarfile.open(crates[0], "r:gz") as crate:
            vcs = crate.extractfile(prefix + ".cargo_vcs_info.json").read()
            assert json.loads(vcs) == cls.policy["vcs"], "Crate VCS differs from reviewed source"
            cls.upstream_license = crate.extractfile(prefix + "LICENSE.txt").read()
            for member in crate.getmembers():
                if member.name.startswith(prefix + "src/") and not member.isdir():
                    assert member.isfile(), "Published libm source must be regular files"
                    cls.sources[member.name[len(prefix):]] = crate.extractfile(member).read()
        legal = archive.ROOT / "scripts" / "licenses"
        raw_coverage = (legal / cls.policy["coverage_file"]).read_bytes()
        assert archive.digest(raw_coverage) == cls.policy["coverage_sha256"], "Reviewed coverage material changed"
        cls.coverage = json.loads(raw_coverage)
        notice = next(item for item in cls.policy["files"] if item["archive_file"] == "SOURCE-NOTICES.txt")
        cls.bundle = (legal / notice["source_file"]).read_bytes()
        assert archive.digest(cls.bundle) == notice["sha256"], "Reviewed source-notice bundle changed"

    def test_all_source_markers_and_complete_verbatim_notices_are_covered(self):
        check_source_notice_coverage(self.sources, self.bundle, self.coverage)

    def test_complete_upstream_license_is_retained_verbatim(self):
        item = next(item for item in self.policy["files"] if item["archive_file"] == "LICENSE.txt")
        content = (archive.ROOT / "scripts" / "licenses" / item["source_file"]).read_bytes()
        self.assertEqual(content, self.upstream_license)
        self.assertEqual(archive.digest(content), item["sha256"])

    def test_missing_bsd_binary_redistribution_condition_is_rejected(self):
        record = next(item for item in self.coverage["files"] if item["path"] == "src/math/exp2.rs")
        notice = record["notices"][0]
        content = self.sources[record["path"]]
        raw = content[notice["offset"]:notice["offset"] + notice["length"]]
        self.assertIn(b"Redistributions in binary form", raw)
        altered = self.bundle.replace(raw, raw.replace(b"Redistributions in binary form", b"Removed condition"), 1)
        with self.assertRaisesRegex(AssertionError, "Complete raw legal notice missing"):
            check_source_notice_coverage(self.sources, altered, self.coverage)

    def test_new_uncovered_copyright_is_rejected_even_with_refreshed_file_hash(self):
        changed = dict(self.sources)
        path = "src/math/exp2.rs"
        changed[path] += b"\n// Copyright 2026 additional contributor\n"
        coverage = copy.deepcopy(self.coverage)
        next(item for item in coverage["files"] if item["path"] == path)["sha256"] = archive.digest(changed[path])
        with self.assertRaisesRegex(AssertionError, "Uncovered legal marker"):
            check_source_notice_coverage(changed, self.bundle, coverage)

    def test_empty_coverage_and_notice_selectors_are_rejected(self):
        with self.assertRaisesRegex(AssertionError, "Empty or unknown notice coverage"):
            check_source_notice_coverage(self.sources, self.bundle, {"format": 1, "files": []})
        coverage = copy.deepcopy(self.coverage)
        coverage["files"][0]["notices"] = []
        with self.assertRaisesRegex(AssertionError, "Empty notice selector"):
            check_source_notice_coverage(self.sources, self.bundle, coverage)


if __name__ == "__main__":
    unittest.main()
