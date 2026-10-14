# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Check runnable package examples and required platform/runtime context."""
from pathlib import Path
import sys
import tomllib
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from release_archive import install_text, matrix_for, usage_text


class ArchiveGuideTests(unittest.TestCase):
    def setUp(self):
        self.version = "3.2.1"
        self.repository = "owner/project"
        self.entries = matrix_for(self.version)
        self.microsoft = {"terms": {"license_file": "selected-runtime.txt",
                                     "runtime": "https://example.invalid/selected-runtime"}}

    def test_each_archive_has_matching_identity_and_source(self):
        self.assertEqual(len(self.entries), 6)
        for entry in self.entries:
            with self.subTest(target=entry["target"]):
                text = install_text(entry, self.version, self.repository, self.microsoft)
                self.assertIn(entry["archive"], text)
                self.assertIn(entry["target"], text)
                self.assertIn("https://github.com/owner/project/releases/tag/v3.2.1", text)
                self.assertIn("https://github.com/owner/project/tree/v3.2.1", text)
                self.assertIn("SHA256SUMS", text)
                self.assertIn("Keep LICENSE and", text)

    def test_windows_prerequisites_and_selected_terms_precede_first_run(self):
        for entry in self.entries:
            if not entry["windows"]:
                continue
            with self.subTest(target=entry["target"]):
                text = install_text(entry, self.version, self.repository, self.microsoft)
                launch = text.index("  .\\auditory-lab.exe --version")
                for required in ("PSVersionTable.PSVersion.Major -lt 7",
                                 "third-party/selected-runtime.txt",
                                 "https://example.invalid/selected-runtime"):
                    self.assertLess(text.index(required), launch)
                self.assertIn("not an executable runtime dependency", text)
                self.assertIn("Authenticode", text)
                self.assertIn("Managed policy", text)
                self.assertNotIn("powershell.exe", text)
                self.assertNotIn("5.1", text)
                with self.assertRaisesRegex(ValueError, "selected Microsoft runtime"):
                    install_text(entry, self.version, self.repository)

    def test_mac_and_linux_retain_actual_runtime_and_security_distinctions(self):
        for entry in self.entries:
            if entry["windows"]:
                continue
            text = install_text(entry, self.version, self.repository)
            if entry["macos"]:
                self.assertIn("macOS 15.0", text)
                self.assertIn("not Developer ID signed or Apple notarized", text)
                self.assertIn("codesign --verify --strict", text)
                self.assertIn("not publisher identity or Gatekeeper approval", text)
                self.assertIn("Open Anyway", text)
                self.assertIn("Do not disable Gatekeeper/SIP globally", text)
            elif not entry["windows"]:
                self.assertIn("Ubuntu 22.04", text)
                self.assertIn("libasound.so.2", text)
                self.assertIn("libgcc_s.so.1", text)
                self.assertIn("sudo apt-get install libasound2 libgcc-s1", text)
                self.assertIn("uname -m", text)
                self.assertNotIn("ad hoc", text)

    def test_usage_commands_run_from_the_extracted_folder(self):
        for executable, prefix in ((Path("auditory-lab"), "./auditory-lab"),
                                   (Path("auditory-lab.exe"), ".\\auditory-lab.exe")):
            with self.subTest(executable=executable):
                text = usage_text(executable, self.version, self.repository)
                commands = [line.strip() for line in text.splitlines()
                            if line.startswith("  ") and "auditory-lab" in line]
                self.assertTrue(commands)
                self.assertTrue(all(command.startswith(prefix + " ") for command in commands))
                exports = [command for command in commands
                           if command.startswith(prefix + " export ") and "--help" not in command]
                self.assertEqual(len(exports), 2)
                self.assertTrue(all(command.endswith(("./silence.toml", "./short-clicks.toml"))
                                    for command in exports))
                self.assertNotRegex(text, r"(?m)^Usage:")
                self.assertIn("/blob/v3.2.1/docs/usage.md", text)
                self.assertIn("/blob/v3.2.1/docs/presets.md", text)

    def test_windows_recipe_saves_are_explicit_utf8(self):
        text = usage_text(Path("auditory-lab.exe"), self.version, self.repository)
        saves = [line for line in text.splitlines() if "Out-File" in line]
        self.assertEqual(len(saves), 2)
        self.assertTrue(all("-Encoding utf8" in line for line in saves))
        self.assertIn("PowerShell 7 or later", text)
        self.assertNotRegex(text, r"auditory-lab\.exe show .* >")

    def test_short_examples_keep_adaptation_and_opt_in_playback_clear(self):
        catalog = Path(__file__).resolve().parents[1] / "crates" / "auditory-core" / "catalog" / "chan-2022-clicks-40-hz.toml"
        preset = tomllib.loads(catalog.read_text(encoding="utf-8"))
        self.assertEqual(preset["duration_seconds"], 180.0)
        self.assertEqual(preset["fade_seconds"], 0.1)
        self.assertIn("for 3 minutes", preset["provenance"]["adaptation"])
        text = usage_text(Path("auditory-lab"), self.version, self.repository)
        self.assertIn("duration_seconds = 0.01", text)
        self.assertIn("fade_seconds = 0.0", text)
        self.assertIn("duration_seconds = 1.0", text)
        self.assertIn("provenance.adaptation", text)
        self.assertIn('from "for 3 minutes" to "for 1 second"', text)
        self.assertIn("keep the citation and study description", text)
        self.assertIn("shell save commands can overwrite preset files", text)
        silence = text.index("Silent starter")
        edit = text.index("Copy and edit a catalog preset")
        playback = text.index("Optional playback")
        warning = text.index("Lower the output volume")
        play_command = text.index("  ./auditory-lab play ./short-clicks.toml")
        self.assertLess(silence, edit)
        self.assertLess(edit, playback)
        self.assertLess(playback, warning)
        self.assertLess(warning, play_command)


if __name__ == "__main__":
    unittest.main()
