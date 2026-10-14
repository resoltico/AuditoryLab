# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Independent rejection controls for public version and note selection."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import types
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from release import prepare, release_notes, validate_repository, validate_source, validate_version


class NotesTests(unittest.TestCase):
    text = "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-10-14\n\n- First release.\n"

    def test_exact_owner_entry(self):
        self.assertEqual(release_notes(self.text, "0.1.0"), "- First release.\n")

    def test_other_sections_excluded(self):
        text = self.text + "\n## [0.0.1] - 2026-10-01\n\n- Different body.\n"
        self.assertEqual(release_notes(text, "0.1.0"), "- First release.\n")

    def test_markdown_indentation_and_hard_break_are_preserved(self):
        text = self.text.replace("- First release.", "    printf hello\n\nA line with a hard break.  ")
        self.assertEqual(release_notes(text, "0.1.0"), "    printf hello\n\nA line with a hard break.  \n")

    def test_example_heading_is_not_release(self):
        text = self.text.replace("- First release.", "```md\n## [0.1.0] - 2026-10-14\n```\n\n- First release.")
        self.assertEqual(release_notes(text, "0.1.0"), "```md\n## [0.1.0] - 2026-10-14\n```\n\n- First release.\n")

    def test_reject_bad_sections(self):
        for text in (self.text.replace("0.1.0", "0.2.0"), self.text + self.text,
                     self.text.replace("- First release.", ""),
                     self.text.replace(" - 2026-10-14", ""),
                     self.text.replace("2026-10-14", "2026-02-30"),
                     self.text.replace("2026-10-14", "9999-01-01"),
                     self.text.replace("Unreleased", "Pending"),
                     self.text + "\n```\n"):
            with self.subTest(text=text), self.assertRaises(ValueError):
                release_notes(text, "0.1.0")

    def test_canonical_version(self):
        self.assertEqual(validate_version("0.1.0"), "0.1.0")
        for value in ("v0.1.0", "01.1.0", "0.1", "0.1.0-beta", "0.1.0\n", "0.1.0;echo x"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate_version(value)

    def test_identity_rejection(self):
        self.assertEqual(validate_repository("resoltico/AuditoryLab"), "resoltico/AuditoryLab")
        self.assertEqual(validate_source("a" * 40), "a" * 40)
        for repo in ("owner/repo/extra", "https://github.com/owner/repo", "owner/repo\n"):
            with self.assertRaises(ValueError):
                validate_repository(repo)
        for sha in ("a" * 39, "A" * 40, "a" * 40 + "\n", "main"):
            with self.assertRaises(ValueError):
                validate_source(sha)


class SourceTests(unittest.TestCase):
    def setUp(self):
        parent = Path(__file__).resolve().parents[1] / "target" / "release-tool-tests"
        parent.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(dir=parent)
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = dict(os.environ)
        self.env.update(GIT_AUTHOR_NAME="Release fixture", GIT_COMMITTER_NAME="Release fixture",
                        GIT_AUTHOR_EMAIL="fixture@example.invalid", GIT_COMMITTER_EMAIL="fixture@example.invalid")
        self.git("init", "--quiet", "--initial-branch=main")
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.1.0"\n', encoding="utf-8")
        (self.root / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.99.0"\n', encoding="utf-8")
        (self.root / "CHANGELOG.md").write_text(NotesTests.text, encoding="utf-8")
        self.git("add", ".")
        self.git("-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "Qualification fixture")
        self.source = self.git("rev-parse", "HEAD").strip()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, env=self.env, text=True)

    def prepare(self, **environment):
        # Only the matrix is isolated; source identity exercises real Git boundaries.
        fixture = types.SimpleNamespace(matrix_for=lambda version: [])
        with mock.patch.dict(sys.modules, {"release_archive": fixture}), mock.patch.dict(os.environ, {
                "GITHUB_ACTIONS": "false", **environment}):
            return prepare("0.1.0", "owner/repository", self.root)

    def test_clean_source_then_dirty_and_untracked_rejection(self):
        self.assertEqual(self.prepare()["source"], self.source)
        changelog = self.root / "CHANGELOG.md"
        changelog.write_text(NotesTests.text + "dirty notes\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "clean committed"):
            self.prepare()
        changelog.write_text(NotesTests.text, encoding="utf-8")
        (self.root / "unexpected.txt").write_text("untracked source", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "clean committed"):
            self.prepare()

    def test_detached_ci_checkout_requires_frozen_dispatch_identity(self):
        self.git("checkout", "--quiet", "--detach", self.source)
        environment = {"GITHUB_ACTIONS": "true", "GITHUB_REF": "refs/heads/main", "GITHUB_SHA": self.source}
        self.assertEqual(self.prepare(**environment)["source"], self.source)
        with self.assertRaisesRegex(ValueError, "dispatched source"):
            self.prepare(**{**environment, "GITHUB_SHA": "b" * 40})
        with self.assertRaisesRegex(ValueError, "main"):
            self.prepare(**{**environment, "GITHUB_REF": "refs/heads/unqualified"})


if __name__ == "__main__":
    unittest.main()
