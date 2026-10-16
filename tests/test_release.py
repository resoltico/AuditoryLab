# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Release targets and release-note selection."""
import copy
import json
import io
import tarfile
import zipfile
import stat
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from release import TARGETS, archive_name, release_notes, windows_toolchain, cargo_identity, validate_artifacts, PRODUCTS, expected_identity, check_build_identity, artifact_name

CHANGELOG = """# Changelog

## [Unreleased]

- Pending.

## [0.2.0] - 2026-10-10

### Changed

- Second.

## [0.1.0] - 2026-10-01

- First release.
"""


class NotesTests(unittest.TestCase):
    def test_the_selected_dated_section_body_is_returned_exactly(self):
        self.assertEqual(release_notes(CHANGELOG, "0.2.0"), "### Changed\n\n- Second.\n")
        self.assertEqual(release_notes(CHANGELOG, "0.1.0"), "- First release.\n")

    def test_missing_undated_empty_and_future_sections_are_rejected(self):
        for text, version in [
            (CHANGELOG, "0.3.0"),
            (CHANGELOG.replace("## [0.2.0] - 2026-10-10", "## [0.2.0]"), "0.2.0"),
            (CHANGELOG.replace("- First release.\n", ""), "0.1.0"),
            (CHANGELOG.replace("2026-10-10", "2999-01-01"), "0.2.0"),
            (CHANGELOG, "0.2"),
        ]:
            with self.assertRaises(ValueError, msg=version):
                release_notes(text, version)

    def test_fenced_headings_are_not_release_sections(self):
        for marker in ["```", "~~~~"]:
            with self.assertRaises(ValueError):
                release_notes(marker + "markdown\n## [0.2.0] - 2026-10-10\n- Example.\n" + marker + "\n", "0.2.0")
            text = CHANGELOG.replace("- Second.", marker + "\n## [0.9.0] - 2026-10-10\n" + marker + "\n- Second.")
            self.assertIn("- Second.", release_notes(text, "0.2.0"))

    def test_unclosed_and_malformed_closing_fences_are_rejected(self):
        for fence in ["```\n", "```\n~~~\n", "```\n``` extra\n"]:
            with self.assertRaises(ValueError):
                release_notes(CHANGELOG.replace("- Second.", "- Second.\n" + fence), "0.2.0")

    def test_duplicate_live_sections_are_rejected(self):
        with self.assertRaises(ValueError):
            release_notes(CHANGELOG + "\n## [0.2.0] - 2026-10-10\n- Duplicate.\n", "0.2.0")

    def test_indentation_and_hard_break_spaces_are_preserved(self):
        text = "## [0.2.0] - 2026-10-10\n\n    code example\n\nLast line.  \n\n## [0.1.0] - 2026-10-01\n- Earlier.\n"
        self.assertEqual(release_notes(text, "0.2.0"), "    code example\n\nLast line.  \n")


class TargetTests(unittest.TestCase):
    def test_windows_targets_use_the_native_gnu_llvm_hosts(self):
        self.assertEqual({entry["platform"]: entry["target"] for entry in TARGETS if entry["platform"].startswith("windows-")}, {
            "windows-x86_64": "x86_64-pc-windows-gnullvm",
            "windows-aarch64": "aarch64-pc-windows-gnullvm",
        })
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.99.0"\n', encoding="utf-8")
            self.assertEqual(windows_toolchain("x86_64-pc-windows-gnullvm", root), "1.99.0-x86_64-pc-windows-gnullvm")
            self.assertEqual(windows_toolchain("aarch64-pc-windows-gnullvm", root), "1.99.0-aarch64-pc-windows-gnullvm")
            for target in ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"]:
                with self.assertRaises(ValueError):
                    windows_toolchain(target, root)
            (root / "rust-toolchain.toml").write_text('[toolchain]\nprofile = "minimal"\n', encoding="utf-8")
            with self.assertRaises(ValueError):
                windows_toolchain("x86_64-pc-windows-gnullvm", root)

    def test_six_native_targets_have_distinct_archives(self):
        names = {archive_name(entry["platform"], "1.2.3") for entry in TARGETS}
        self.assertEqual(len(names), 6)
        self.assertIn("auditorylab-1.2.3-linux-x86_64.tar.gz", names)
        self.assertIn("auditorylab-1.2.3-windows-aarch64.zip", names)
        self.assertIn("auditorylab-1.2.3-macos-aarch64.zip", names)


class IdentityTests(unittest.TestCase):
    def metadata(self):
        packages = [{"id": f"path+file:///workspace#{item['package']}@1.2.3",
                     "name": item["package"], "version": "1.2.3",
                     "targets": [{"name": item["bin"], "kind": ["bin"]},
                                 {"name": "integration", "kind": ["test"]},
                                 {"name": "worker", "kind": ["example"]}]}
                    for item in PRODUCTS.values()]
        return {"workspace_members": [item["id"] for item in packages], "packages": packages}

    def test_workspace_identity_ignores_tests_examples_and_nonmembers(self):
        meta = self.metadata()
        outsider = copy.deepcopy(meta["packages"][0])
        outsider["id"] = "path+file:///other#auditory-cli@1.2.3"
        meta["packages"].append(outsider)
        self.assertEqual(cargo_identity(meta), {"id": meta["workspace_members"][0],
                         "name": "auditory-cli", "version": "1.2.3", "bin": "auditorylab"})

    def test_missing_and_ambiguous_workspace_roots_fail(self):
        for change in ("absent", "outsider", "ambiguous"):
            meta = self.metadata()
            if change == "absent":
                meta["packages"][0]["name"] = "other"
            elif change == "outsider":
                meta["workspace_members"] = []
            else:
                other = copy.deepcopy(meta["packages"][0])
                other["id"] = "path+file:///other#auditory-cli@1.2.3"
                meta["packages"].append(other)
                meta["workspace_members"].append(other["id"])
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, "exactly one auditory-cli"):
                cargo_identity(meta)

    def test_missing_renamed_and_alias_bins_fail(self):
        for bins in ([], [{"name": "auditory-lab", "kind": ["bin"]}],
                     [{"name": name, "kind": ["bin"]} for name in ("auditorylab", "alias")]):
            meta = self.metadata()
            meta["packages"][0]["targets"] = bins
            with self.subTest(bins=bins), self.assertRaisesRegex(ValueError, "exactly one bin named auditorylab"):
                cargo_identity(meta)


class ArtifactTests(unittest.TestCase):
    source = "a" * 40

    def populate(self, directory, build_kind="release"):
        from package import expected_members, bundle_metadata
        import plistlib
        for product in PRODUCTS:
            for target in TARGETS:
                folder = directory / artifact_name(product, target["platform"], build_kind)
                folder.mkdir()
                name = archive_name(target["platform"], "1.2.3", product, build_kind, self.source)
                archive = folder / name
                root = name.removesuffix(".zip").removesuffix(".tar.gz")
                identity = expected_identity(product, "1.2.3", target["target"], build_kind, self.source)
                records = {root + "/" + path: b"fixture" for path in expected_members(product, target["target"])}
                if product == "gui" and "apple" in target["target"]:
                    records[root + "/Auditory Lab.app/Contents/Info.plist"] = plistlib.dumps(bundle_metadata("1.2.3"))
                records[root + "/README.txt"] = ("Build identity: " + json.dumps(identity) + "\n").encode()
                if archive.suffix == ".zip":
                    with zipfile.ZipFile(archive, "w") as zipped:
                        for member, value in records.items():
                            info = zipfile.ZipInfo(member)
                            info.external_attr = (stat.S_IFREG | 0o644) << 16
                            zipped.writestr(info, value)
                else:
                    with tarfile.open(archive, "w:gz") as tarred:
                        for member, value in records.items():
                            info = tarfile.TarInfo(member)
                            info.size = len(value)
                            tarred.addfile(info, io.BytesIO(value))

    def test_exact_assignment_passes_with_real_product_archives(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.populate(root)
            self.assertEqual(len(validate_artifacts(root, "1.2.3", source_revision=self.source)), 12)

    def test_missing_extra_nested_duplicate_misassigned_and_nonregular_reject(self):
        for case in ("missing-directory", "extra-directory", "missing-archive", "extra", "nested", "duplicate", "misassigned", "symlink"):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.populate(root)
                folder = root / artifact_name("cli", TARGETS[0]["platform"], "release")
                archive = next(folder.iterdir())
                if case in ("missing-directory", "missing-archive", "nested", "misassigned", "symlink"):
                    archive.unlink()
                if case == "missing-directory":
                    folder.rmdir()
                elif case == "extra-directory":
                    (root / "release-unexpected").mkdir()
                elif case == "extra":
                    (folder / "extra.txt").write_bytes(b"extra")
                elif case == "nested":
                    archive.mkdir()
                    (archive / archive.name).write_bytes(b"nested")
                elif case in ("duplicate", "misassigned"):
                    (folder / archive_name(TARGETS[1]["platform"], "1.2.3")).write_bytes(b"wrong assignment")
                elif case == "symlink":
                    archive.symlink_to(next((root / artifact_name("cli", TARGETS[1]["platform"], "release")).iterdir()))
                with self.assertRaises(ValueError):
                    validate_artifacts(root, "1.2.3", source_revision=self.source)

    def test_candidate_artifacts_cannot_pass_release_even_at_same_semver(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.populate(root, "candidate")
            self.assertEqual(len(validate_artifacts(root, "1.2.3", "candidate", self.source)), 12)
            with self.assertRaises(ValueError):
                validate_artifacts(root, "1.2.3", "release", self.source)

    def test_wrong_source_product_target_kind_and_incomplete_identity_reject(self):
        correct = expected_identity("cli", "1.2.3", TARGETS[0]["target"], "release", self.source)
        for field, value in (("source_revision", "b" * 40), ("product", "gui"), ("bin", "auditorylab-gui"),
                             ("target", TARGETS[1]["target"]), ("build_kind", "candidate"), ("format_version", 2), ("format_version", True), ("format_version", 1.0)):
            wrong = dict(correct, **{field: value})
            with self.subTest(field=field), self.assertRaises(ValueError):
                check_build_identity(wrong, "cli", "1.2.3", TARGETS[0]["target"], "release", self.source)
        del correct["source_revision"]
        with self.assertRaises(ValueError):
            check_build_identity(correct, "cli", "1.2.3", TARGETS[0]["target"], "release", self.source)


if __name__ == "__main__":
    unittest.main()
