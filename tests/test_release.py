# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Release targets and release-note selection."""
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from release import TARGETS, archive_name, release_notes, windows_toolchain

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
        self.assertIn("auditory-lab-1.2.3-linux-x86_64.tar.gz", names)
        self.assertIn("auditory-lab-1.2.3-windows-aarch64.zip", names)
        self.assertIn("auditory-lab-1.2.3-macos-aarch64.zip", names)


if __name__ == "__main__":
    unittest.main()
