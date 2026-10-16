# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Complete reviewed MinGW/LLVM material reaches the consolidated notices."""
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import notices


class NativeNoticeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.reviewed = json.loads((notices.LICENSES / 'windows-runtime.json').read_text())
        cls.channel = cls.reviewed['producer']['rust']
        cls.native = notices.windows_runtime_notices(cls.channel)
        cls.generated = notices.generate()

    def test_all_required_grants_and_source_specific_notice_reach_output_complete(self):
        expected = {
            'mingw-COPYING': 'Zope Public License (ZPL) Version 2.1',
            'mingw-COPYING-runtime.txt': 'MinGW-w64 runtime licensing',
            'mingw-DISCLAIMER.PD': 'This file has no copyright assigned',
            'llvm-libunwind-LICENSE.TXT': 'LLVM Exceptions to the Apache 2.0 License',
            'mingw-pseudo-reloc-notice.txt': 'Contributed by Egor Duda',
            'mingw-complex-internal-notice.txt': 'Copyright (c) 2009, 2010 by the mingw-w64 project',
        }
        self.assertEqual({item['file'] for item in self.reviewed['materials']}, set(expected))
        for name, grant in expected.items():
            text = (notices.LICENSES / name).read_bytes().decode('utf-8')
            self.assertIn(grant, text)
            self.assertIn(text, self.native)
            self.assertIn(text, self.generated)
        self.assertIn(self.native, self.generated)
        self.assertNotIn('== Microsoft Visual C++ runtime (Windows executables) ==', self.generated)
        self.assertIn('Copyright (c) Microsoft Corporation.', self.generated)

    def test_missing_unreviewed_altered_or_wrong_version_material_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            licenses = Path(directory)
            for entry in self.reviewed['materials']:
                shutil.copyfile(notices.LICENSES / entry['file'], licenses / entry['file'])
            manifest = licenses / 'windows-runtime.json'
            for kind in ('missing', 'duplicate', 'altered', 'version'):
                reviewed = json.loads(json.dumps(self.reviewed))
                if kind == 'missing':
                    reviewed['materials'].pop()
                elif kind == 'duplicate':
                    reviewed['materials'][-1] = reviewed['materials'][0]
                elif kind == 'altered':
                    reviewed['materials'][-1]['sha256'] = '0' * 64
                else:
                    reviewed['producer']['rust'] = '0.0.0'
                manifest.write_text(json.dumps(reviewed))
                with self.subTest(kind=kind), mock.patch.object(notices, 'LICENSES', licenses), self.assertRaises(ValueError):
                    notices.windows_runtime_notices(self.channel)


if __name__ == '__main__':
    unittest.main()
