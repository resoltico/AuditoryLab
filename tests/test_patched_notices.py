# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""The narrow reviewed backend fork remains visible and source/license bound."""
import io
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import notices


class PatchedNoticeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.source = notices.ROOT / 'vendor/slint-winit'

    def test_exact_copy_reconstructs_upstream_and_retains_every_legal_file(self):
        with tempfile.TemporaryDirectory(dir=notices.ROOT / 'target') as directory:
            root = Path(directory)
            destination = root / 'vendor/slint-winit'
            shutil.copytree(self.source, destination)
            package = {'name': 'i-slint-backend-winit', 'version': '1.18.1',
                       'manifest_path': str(destination / 'Cargo.toml')}
            actual_run = subprocess.run
            def byte_transport(*args, **kwargs):
                self.assertIsInstance(kwargs['input'], bytes)
                self.assertNotIn('text', kwargs)
                self.assertNotIn('encoding', kwargs)
                return actual_run(*args, **kwargs)
            with mock.patch.object(notices, 'ROOT', root), mock.patch.object(notices.subprocess, 'run', side_effect=byte_transport):
                notices.validate_patched_dependency(package)
                files = notices.published_files(package)
                self.assertTrue(any('Royalty-free-2.0' in path.name for path in files))
                self.assertTrue(any('GPL-3.0' in path.name for path in files))
                failure = subprocess.CompletedProcess(['git', 'apply'], 1, b'', b'original patch failure: \xff')
                with mock.patch.object(notices.subprocess, 'run', return_value=failure), self.assertRaises(ValueError) as caught:
                    notices.validate_patched_dependency(package)
                self.assertIn('original patch failure: \\xff', str(caught.exception))

    def test_git_patch_bytes_preserve_lf_and_reject_text_newline_translation(self):
        # Model Windows TextIOWrapper newline translation at the actual Git
        # boundary; the checked patch and source bytes themselves stay unchanged.
        patch = b"--- a/source.txt\n+++ b/source.txt\n@@ -1,3 +1,3 @@\n first\n-old\n+new\n last\n"
        stream = io.BytesIO()
        writer = io.TextIOWrapper(stream, encoding='utf-8', newline='\r\n')
        writer.write(patch.decode('utf-8'))
        writer.flush()
        translated = stream.getvalue()
        self.assertNotEqual(patch, translated)
        for accepted, payload in ((True, patch), (False, translated)):
            with self.subTest(translated=not accepted), tempfile.TemporaryDirectory(dir=notices.ROOT / 'target') as directory:
                root = Path(directory)
                source = root / 'source.txt'
                source.write_bytes(b'first\nnew\nlast\n')
                result = subprocess.run(['git', 'apply', '--reverse', '--whitespace=nowarn', '-'], cwd=root,
                                        input=payload, capture_output=True)
                if accepted:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(source.read_bytes(), b'first\nold\nlast\n')
                else:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(source.read_bytes(), b'first\nnew\nlast\n')

    def test_source_patch_provenance_license_and_corpus_changes_fail(self):
        for change in ('source', 'patch', 'provenance', 'license', 'added', 'missing'):
            with self.subTest(change=change), tempfile.TemporaryDirectory(dir=notices.ROOT / 'target') as directory:
                root = Path(directory)
                destination = root / 'vendor/slint-winit'
                shutil.copytree(self.source, destination)
                if change == 'source':
                    with (destination / 'lib.rs').open('a') as file:
                        file.write('\n// unreviewed change\n')
                elif change == 'patch':
                    with (destination / 'auditorylab.patch').open('a') as file:
                        file.write('\n')
                elif change == 'provenance':
                    (destination / 'auditorylab-provenance.json').write_text('{}')
                elif change == 'license':
                    (destination / 'LICENSES/LicenseRef-Slint-Royalty-free-2.0.md').write_text('changed terms')
                elif change == 'added':
                    (destination / 'unreviewed.rs').write_text('')
                else:
                    (destination / 'LICENSES/GPL-3.0-only.txt').unlink()
                package = {'name': 'i-slint-backend-winit', 'version': '1.18.1',
                           'manifest_path': str(destination / 'Cargo.toml')}
                with mock.patch.object(notices, 'ROOT', root), self.assertRaises(ValueError):
                    notices.validate_patched_dependency(package)

    def test_sctk_manifest_only_patch_retains_source_font_and_terms(self):
        source = notices.ROOT / 'vendor/sctk-adwaita'
        for change in (None, 'font', 'source', 'license'):
            with self.subTest(change=change), tempfile.TemporaryDirectory(dir=notices.ROOT / 'target') as directory:
                root = Path(directory)
                destination = root / 'vendor/sctk-adwaita'
                shutil.copytree(source, destination)
                package = {'name': 'sctk-adwaita', 'version': '0.10.1', 'manifest_path': str(destination / 'Cargo.toml')}
                if change:
                    file = {'font': 'src/title/Cantarell-Regular.ttf', 'source': 'src/title/crossfont_renderer.rs', 'license': 'LICENSE'}[change]
                    (destination / file).write_bytes(b'changed')
                with mock.patch.object(notices, 'ROOT', root):
                    if change:
                        with self.assertRaises(ValueError):
                            notices.validate_patched_dependency(package)
                    else:
                        notices.validate_patched_dependency(package)
                        self.assertTrue(any(p.name == 'LICENSE' for p in notices.published_files(package)))

    def test_unknown_path_dependency_needs_explicit_review(self):
        with self.assertRaisesRegex(ValueError, 'needs reviewed patch provenance'):
            notices.validate_patched_dependency({'name': 'unreviewed', 'version': '1.0.0', 'manifest_path': '/unreviewed/Cargo.toml'})


if __name__ == '__main__':
    unittest.main()
