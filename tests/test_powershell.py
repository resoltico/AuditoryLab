# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Controls for the Windows verification shell prerequisite."""

from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import release_archive as archive


class PowerShellChecks(unittest.TestCase):
    def test_major_seven_and_later_are_accepted(self):
        for major in ("7\n", "8\r\n"):
            with self.subTest(major=major), patch.object(archive, "run", return_value=major) as execute:
                archive.require_powershell()
                self.assertEqual(execute.call_args.args[0][0], "pwsh")

    def test_older_missing_and_invalid_shells_fail_actionably(self):
        for major in ("5\n", "6\n", "", "unexpected", "７", "7\nextra"):
            with self.subTest(major=major), patch.object(archive, "run", return_value=major):
                with self.assertRaisesRegex(ValueError, "PowerShell 7 or later.*Install"):
                    archive.require_powershell()
        for error in (FileNotFoundError("pwsh"), ValueError("execution failed"), subprocess.TimeoutExpired("pwsh", 30)):
            with self.subTest(error=error), patch.object(archive, "run", side_effect=error):
                with self.assertRaisesRegex(ValueError, "PowerShell 7 or later.*Install"):
                    archive.require_powershell()

    def test_windows_host_fails_before_compiler_or_runtime_probes(self):
        with patch.object(archive.platform, "system", return_value="Windows"), patch.object(archive, "require_powershell", side_effect=ValueError("PowerShell 7 required")), patch.object(archive, "compiler_info") as compiler:
            with self.assertRaisesRegex(ValueError, "PowerShell 7"):
                archive.assert_host("x86_64-pc-windows-msvc")
            compiler.assert_not_called()

    def test_windows_extraction_fails_before_creating_output(self):
        with patch.object(archive, "require_powershell", side_effect=ValueError("PowerShell 7 required")), patch.object(archive.Path, "mkdir") as create:
            with self.assertRaisesRegex(ValueError, "PowerShell 7"):
                archive.extract_native(Path("archive.zip"), {"windows": True}, Path("unused"))
            create.assert_not_called()


if __name__ == "__main__":
    unittest.main()
