# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Reviewed native inputs, selected members, and binary-bound link receipts."""
import hashlib
import io
import json
from pathlib import Path, PureWindowsPath
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import windows_runtime as runtime


class RuntimeTests(unittest.TestCase):
    def setUp(self):
        (runtime.ROOT / "target").mkdir(exist_ok=True)
        self.directory = tempfile.TemporaryDirectory(dir=runtime.ROOT / "target")
        self.addCleanup(self.directory.cleanup)
        self.base = Path(self.directory.name)
        self.target = "x86_64-pc-windows-gnullvm"
        self.sysroot = self.base / "Rust toolchain"
        self.bundled = self.sysroot / "lib/rustlib" / self.target / "lib/self-contained"
        self.bundled.mkdir(parents=True)
        self.executable = self.base / "release/auditory-lab.exe"
        self.executable.parent.mkdir()
        self.executable.write_bytes(b"fixture executable")
        self.inputs = {}
        for name in ("crt2.o", "libmingw32.a", "libmingwex.a", "libmsvcrt.a", "libunwind.a"):
            data = ("reviewed " + name).encode()
            (self.bundled / name).write_bytes(data)
            self.inputs[name] = {"sha256": hashlib.sha256(data).hexdigest(),
                                 "members": [] if name == "crt2.o" else ["reviewed.o"]}
        self.trace_text = "\n".join(
            f"         rust-lld: Reading {self.bundled / name}" for name in self.inputs)
        self.trace_text += "\n" + "\n".join(
            f"rust-lld: Loaded {name}(reviewed.o) for symbol" for name in self.inputs if name != "crt2.o")
        self.receipt = self.base / "receipt.json"

    def check(self, text=None):
        runtime.check_trace(self.trace_text if text is None else text, self.executable,
                            self.target, self.sysroot, self.inputs)

    def toolchain(self):
        return mock.patch.object(runtime, "toolchain", return_value=("1.99.0", "test-host", self.sysroot))

    def policy(self):
        return mock.patch.object(runtime, "policy", return_value=("1.99.0", self.inputs))

    def test_accepts_qualified_reviewed_inputs_and_members(self):
        self.check()
        # Paths with spaces and either rust-lld executable spelling remain intact.
        text = self.trace_text.replace("rust-lld:", "rust-lld.exe:")
        text = text.replace("libunwind.a(reviewed.o)", f"{self.bundled / 'libunwind.a'}(reviewed.o)")
        self.check(text)

    def test_windows_drive_backslash_space_and_parenthesis_spellings(self):
        # Exercise the actual parser with native Windows tokens on every host.
        # Translate only the filesystem domain to owned fixture files; this does
        # not claim native Windows filesystem or linker execution was exercised.
        native_root = PureWindowsPath(r"C:\Build for release (reviewed)")
        def native_path(value):
            text = str(value)
            windows = PureWindowsPath(text)
            if windows.drive:
                try:
                    parts = windows.relative_to(native_root).parts
                except ValueError:
                    parts = windows.parts[1:]
                return self.base.joinpath(*parts)
            return Path(value)
        text = self.trace_text.replace(str(self.base), str(native_root)).replace("/", "\\")
        qualified = native_root / self.bundled.relative_to(self.base).as_posix() / "libunwind.a"
        text = text.replace("libunwind.a(reviewed.o)", str(qualified) + "(reviewed.o)")
        with mock.patch.object(runtime, "Path", side_effect=native_path):
            self.check(text)
            with self.assertRaisesRegex(ValueError, "unexpected.*native link input"):
                self.check(text + r"\nrust-lld: Reading C:\Program Files (x86)\Windows Kits\10\Lib\ucrt.lib".replace(r"\n", "\n"))

    def test_rejects_missing_static_runtime_or_empty_trace(self):
        for text in ("", "\n".join(line for line in self.trace_text.splitlines() if "libunwind.a" not in line)):
            with self.assertRaisesRegex(ValueError, "lacks.*startup"):
                self.check(text)

    def test_rejects_an_altered_native_input(self):
        (self.bundled / "crt2.o").write_bytes(b"altered startup implementation")
        with self.assertRaisesRegex(ValueError, "differs from its reviewed component"):
            self.check()

    def test_rejects_system_sdk_and_unreviewed_native_inputs(self):
        for name in ("libcmt.lib", "ucrt.lib", "private.a", "unreviewed.obj"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "unexpected.*native link input"):
                self.check(self.trace_text + f"\nrust-lld: Reading {self.base / name}")

    def test_rejects_native_archive_shadowing_and_unqualified_members(self):
        with self.assertRaisesRegex(ValueError, "outside the bundled runtime"):
            self.check(self.trace_text.replace(str(self.bundled / "libunwind.a"), str(self.base / "libunwind.a")))
        with self.assertRaisesRegex(ValueError, "no qualified input"):
            self.check(self.trace_text + "\nrust-lld: Loaded unknown.a(member.o) for symbol")

    def test_rejects_unreviewed_member_even_in_unchanged_reviewed_archive(self):
        with self.assertRaisesRegex(ValueError, "unreviewed.*member"):
            self.check(self.trace_text.replace("libunwind.a(reviewed.o)", "libunwind.a(new-source.o)"))

    def test_rejects_native_objects_hidden_in_rust_archives(self):
        archive = self.executable.parent / "deps/libdependency-123.rlib"
        text = self.trace_text + f"\nrust-lld: Reading {archive}"
        self.check(text + "\nrust-lld: Reading libdependency-123.rlib(dependency.123.rcgu.o)")
        with self.assertRaisesRegex(ValueError, "unexpected native object"):
            self.check(text + "\nrust-lld: Reading libdependency-123.rlib(native-library.o)")

    def fake_cargo(self, command, **kwargs):
        map_flag = next(arg for arg in command if arg.startswith("link-arg=-Map="))
        Path(map_flag.split("=", 2)[2]).write_text(
            "Address  Size     Align Out     In      Symbol\n"
            f"00001000 000003d1    16         {self.bundled / 'crt2.o'}:(.text)\n", encoding="utf-8")
        artifact = {"reason": "compiler-artifact", "target": {"name": "auditory-lab"},
                    "executable": str(self.executable)}
        message = {"reason": "compiler-message", "message": {"rendered": self.trace_text}}
        return subprocess.CompletedProcess(command, 0, json.dumps(artifact) + "\n" + json.dumps(message) + "\n", "")

    def test_cargo_json_utf8_survives_an_ansi_parent_decoder(self):
        original = subprocess.Popen
        diagnostic = 'UTF-8 protocol path: C:\\Users\\Łódź\\Ervīns\\target'
        def legacy_parent(command, *args, **kwargs):
            result = self.fake_cargo(command)
            messages = [json.loads(line) for line in result.stdout.splitlines()]
            messages[-1]['message']['rendered'] += '\n' + diagnostic + '\n'
            wire = ('\n'.join(json.dumps(message, ensure_ascii=False) for message in messages) + '\n').encode('utf-8')
            producer = [sys.executable, '-c', 'import sys; sys.stdout.buffer.write(bytes.fromhex(' + repr(wire.hex()) + '))']
            kwargs.setdefault('encoding', 'cp1252')
            return original(producer, *args, **kwargs)
        with self.toolchain(), self.policy(), mock.patch.object(subprocess, 'Popen', side_effect=legacy_parent):
            runtime.build(self.target, self.receipt)
            recorded = json.loads(self.receipt.read_text(encoding='utf-8'))
            self.assertIn(diagnostic, recorded['linker_output'])

    def test_success_status_handles_unicode_paths_on_legacy_and_ascii_writers(self):
        directory = self.base / 'Łódź-Ж' / 'release'
        directory.mkdir(parents=True)
        executable = directory / 'auditory-lab.exe'
        shutil.copyfile(self.executable, executable)
        self.executable = executable
        self.receipt = directory.parent / 'receipt.json'
        for encoding in ('ascii', 'cp1252', 'utf-8'):
            buffer = io.BytesIO()
            writer = io.TextIOWrapper(buffer, encoding=encoding)
            with self.toolchain(), self.policy(), mock.patch.object(runtime.subprocess, 'run', side_effect=self.fake_cargo), mock.patch.object(runtime.sys, 'stdout', writer):
                runtime.build(self.target, self.receipt)
                writer.flush()
            self.assertTrue(self.receipt.is_file())
            self.assertIn('verified Windows native inputs:', buffer.getvalue().decode(encoding))
            self.assertIn('\\u0141', buffer.getvalue().decode(encoding))
            writer.detach()

    def test_build_receipt_binds_binary_and_map_and_forces_final_relink(self):
        with self.toolchain(), self.policy(), mock.patch.object(runtime.subprocess, "run", side_effect=self.fake_cargo):
            runtime.build(self.target, self.receipt)
            first = json.loads(self.receipt.read_text())["map"]
            runtime.verify(self.executable, self.target, self.receipt)
            copied = self.base / "staged.exe"
            shutil.copyfile(self.executable, copied)
            runtime.verify(copied, self.target, self.receipt)
            runtime.build(self.target, self.receipt)
            self.assertNotEqual(first, json.loads(self.receipt.read_text())["map"])
            self.executable.write_bytes(b"another binary")
            with self.assertRaisesRegex(ValueError, "executable bytes"):
                runtime.verify(self.executable, self.target, self.receipt)

    def test_receipt_rejects_changed_target_map_and_toolchain(self):
        with self.toolchain(), self.policy(), mock.patch.object(runtime.subprocess, "run", side_effect=self.fake_cargo):
            runtime.build(self.target, self.receipt)
            original = json.loads(self.receipt.read_text())
            changed = dict(original, target="aarch64-pc-windows-gnullvm")
            self.receipt.write_text(json.dumps(changed))
            with self.assertRaisesRegex(ValueError, "target/toolchain"):
                runtime.verify(self.executable, self.target, self.receipt)
            self.receipt.write_text(json.dumps(original))
            Path(original["map"]).write_text("unrelated map")
            with self.assertRaisesRegex(ValueError, "link map differs"):
                runtime.verify(self.executable, self.target, self.receipt)
            with mock.patch.object(runtime, "toolchain", return_value=("1.100.0", "host", self.sysroot)):
                with self.assertRaisesRegex(ValueError, "requires Rust"):
                    runtime.verify(self.executable, self.target, self.receipt)

    def test_malformed_receipt_is_reported_as_invalid_input(self):
        for value in (None, [], {}, {"target": []}):
            self.receipt.write_text(json.dumps(value))
            with self.toolchain(), self.policy(), self.assertRaisesRegex(ValueError, "receipt is incomplete"):
                runtime.verify(self.executable, self.target, self.receipt)

    def test_empty_or_unknown_map_is_rejected_even_with_matching_hash(self):
        with self.toolchain(), self.policy(), mock.patch.object(runtime.subprocess, "run", side_effect=self.fake_cargo):
            runtime.build(self.target, self.receipt)
            receipt = json.loads(self.receipt.read_text())
            map_path = Path(receipt["map"])
            for text in ("", "unknown map\n", "Address Size Align Out In Symbol\n"):
                map_path.write_text(text)
                receipt["map_sha256"] = runtime.sha256(map_path)
                self.receipt.write_text(json.dumps(receipt))
                with self.subTest(text=text), self.assertRaisesRegex(ValueError, "link map lacks"):
                    runtime.verify(self.executable, self.target, self.receipt)

    def test_unicode_failed_cargo_output_keeps_logs_and_actionable_ascii_diagnostic(self):
        self.receipt = self.base / 'Łódź-Ж' / 'receipt.json'
        stdout = 'Cargo output: Łódź\nnext line\n'
        stderr = 'fatal: Ж build failure\n'
        result = subprocess.CompletedProcess([], 1, stdout, stderr)
        buffer = io.BytesIO()
        writer = io.TextIOWrapper(buffer, encoding='ascii')
        with self.toolchain(), self.policy(), mock.patch.object(runtime.subprocess, 'run', return_value=result), mock.patch.object(runtime.sys, 'stderr', writer):
            with self.assertRaisesRegex(ValueError, 'Windows build failed; evidence retained') as failure:
                runtime.build(self.target, self.receipt)
            writer.flush()
        message = buffer.getvalue().decode('ascii')
        self.assertIn('\\u0141', message)
        self.assertIn('next line', message.splitlines())
        self.assertIn('fatal: \\u0416', message)
        self.assertTrue(str(failure.exception).isascii())
        self.assertFalse(self.receipt.exists())
        evidence = next(self.receipt.parent.glob(self.target + '-*'))
        self.assertEqual((evidence / 'cargo.stdout').read_text(encoding='utf-8'), stdout)
        self.assertEqual((evidence / 'cargo.stderr').read_text(encoding='utf-8'), stderr)
        writer.detach()

    def test_failed_build_or_untraced_cached_artifact_cannot_leave_stale_receipt(self):
        results = [subprocess.CompletedProcess([], 1, "", "fixture build error\n"),
                   subprocess.CompletedProcess([], 0, json.dumps({"reason": "compiler-artifact",
                       "target": {"name": "auditory-lab"}, "executable": str(self.executable)}) + "\n", "")]
        for result in results:
            self.receipt.write_text("stale success")
            with self.toolchain(), self.policy(), mock.patch.object(runtime.subprocess, "run", return_value=result):
                with self.assertRaises((ValueError, OSError)):
                    runtime.build(self.target, self.receipt)
                self.assertFalse(self.receipt.exists())


if __name__ == "__main__":
    unittest.main()
