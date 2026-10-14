# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Rust/Cargo wire output stays UTF-8 under a legacy parent text decoder."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import notices
import release
import windows_runtime
import package


class ProtocolTests(unittest.TestCase):
    def test_known_program_protocol_uses_utf8_while_native_tool_default_is_unchanged(self):
        wire = 'saved C:\\Users\\Łódź\\Ervīns\\target\n'.encode('utf-8')
        producer = [sys.executable, '-c', 'import sys; sys.stdout.buffer.write(bytes.fromhex(' + repr(wire.hex()) + '))']
        original = subprocess.Popen
        def legacy_parent(command, *args, **kwargs):
            if kwargs.get('encoding') is None:
                kwargs['encoding'] = 'cp1252'
            return original(command, *args, **kwargs)
        with mock.patch.object(subprocess, 'Popen', side_effect=legacy_parent):
            self.assertEqual(package.run(*producer, encoding='utf-8'), wire.decode('utf-8'))
            with self.assertRaises(UnicodeDecodeError):
                package.run(*producer)

    def test_real_utf8_children_preserve_nonascii_metadata_and_sysroot_with_ansi_default(self):
        with tempfile.TemporaryDirectory() as directory:
            path = str((Path(directory) / 'Łódź' / 'Ervīns').resolve())
            metadata = {'packages': [{'name': 'auditory-lab', 'version': '0.1.0', 'manifest_path': path}]}
            payloads = {
                'cargo': json.dumps(metadata, ensure_ascii=False),
                '-vV': 'release: 1.99.0\nhost: x86_64-pc-windows-gnullvm\n',
                '--print': path + '\n',
            }
            original = subprocess.Popen
            def legacy_parent(command, *args, **kwargs):
                if command[0] in ('cargo', 'rustc'):
                    text = payloads['cargo' if command[0] == 'cargo' else command[1]]
                    wire = text.encode('utf-8')
                    command = [sys.executable, '-c', 'import sys; sys.stdout.buffer.write(bytes.fromhex(' + repr(wire.hex()) + '))']
                kwargs.setdefault('encoding', 'cp1252')
                return original(command, *args, **kwargs)
            with mock.patch.object(subprocess, 'Popen', side_effect=legacy_parent):
                # Negative control: a real UTF-8 producer fails with the old
                # implicit ANSI text decoder; no system locale is changed.
                with self.assertRaises(UnicodeDecodeError):
                    subprocess.check_output(['cargo', 'metadata'], text=True)
                self.assertEqual(notices.metadata('x86_64-pc-windows-gnullvm'), metadata)
                self.assertEqual(release.workspace_version(), '0.1.0')
                version, host, sysroot = windows_runtime.toolchain()
                self.assertEqual(version, '1.99.0')
                self.assertEqual(host, 'x86_64-pc-windows-gnullvm')
                self.assertEqual(str(sysroot), path)


if __name__ == '__main__':
    unittest.main()
