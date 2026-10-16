# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""System engine terms remain distinct from Rust bindings and bundled code."""
from pathlib import Path
import json
import shutil
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import notices


class SystemLibraryNoticeTests(unittest.TestCase):
    def test_reference_engine_acknowledgment_and_entire_original_grants_are_retained(self):
        output = notices.system_library_notices()
        self.assertIn('This software is based in part on the work of the FreeType Team.', output)
        self.assertIn('not the\nMIT Rust binding crates or bundled native implementations', output)
        reviewed = json.loads((notices.LICENSES / 'system-library-notices.json').read_text(encoding='utf-8'))
        for item in reviewed['materials']:
            self.assertIn((notices.LICENSES / item['file']).read_text(encoding='utf-8'), output)
            self.assertIn(item['package'] + ' ' + item['version'], output)
        self.assertIn('linking it  to form a', output)
        self.assertIn('KEITH PACKARD DISCLAIMS ALL WARRANTIES', output)

    def test_missing_engine_acknowledgment_material_or_changed_grant_rejects(self):
        for change in ('acknowledgment', 'missing', 'terms'):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                reviewed = json.loads((notices.LICENSES / 'system-library-notices.json').read_text(encoding='utf-8'))
                for item in reviewed['materials']:
                    shutil.copyfile(notices.LICENSES / item['file'], root / item['file'])
                if change == 'acknowledgment':
                    reviewed['acknowledgment'] = ''
                elif change == 'missing':
                    reviewed['materials'].pop()
                else:
                    (root / reviewed['materials'][0]['file']).write_text('altered engine terms', encoding='utf-8')
                (root / 'system-library-notices.json').write_text(json.dumps(reviewed), encoding='utf-8')
                with mock.patch.object(notices, 'LICENSES', root), self.assertRaises(ValueError):
                    notices.system_library_notices()


if __name__ == '__main__':
    unittest.main()
