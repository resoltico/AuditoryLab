# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Cargo itself must suppress the native producer's fallback build script."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import qualify


class SystemFontTests(unittest.TestCase):
    def test_exact_supported_linux_overrides_reject_missing_static_or_extra_inputs(self):
        config = (qualify.ROOT / '.cargo/config.toml').read_text(encoding='utf-8')
        qualify.check_linux_font_overrides(config)
        for changed in (config.replace('dylib=freetype', 'static=freetype', 1),
                        config.replace('[target.x86_64-unknown-linux-gnu.freetype]', '[target.other.freetype]'),
                        config + '\n[target.aarch64-unknown-linux-gnu.freetype]\nrustc-link-lib=["dylib=freetype"]\n',
                        config.replace('["dylib=freetype"]', '["dylib=freetype", "other"]', 1),
                        config + 'rustc-link-search=["/unreviewed"]\n',
                        'note="""\n' + config + '\n[end]\n"""\n',
                        'include=["/unreviewed.toml"]\n' + config,
                        '"note#key" = """\n' + config + '\n[end]\n#"""\n',
                        '"\\u0069nclude"=["/unreviewed.toml"]\n' + config):
            with self.assertRaises(ValueError):
                qualify.check_linux_font_overrides(changed)

    def test_qualification_rejects_ambient_producer_flags_and_external_cargo_configs(self):
        for name in ('RUSTC', 'RUSTDOC', 'RUSTFLAGS', 'RUSTC_WRAPPER', 'CARGO_ENCODED_RUSTFLAGS',
                     'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER', 'CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER'):
            with mock.patch.dict(os.environ, {name: 'unreviewed'}, clear=True), self.assertRaises(ValueError):
                qualify.controlled_environment('gui', qualify.TARGETS[0], 'local', None)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ('config', 'config.toml'):
                path = root / name
                path.write_text('settings deliberately not read', encoding='utf-8')
                with self.assertRaises(ValueError):
                    qualify.check_ambient_cargo_configuration({'CARGO_HOME': str(root)})
                self.assertEqual(path.read_text(), 'settings deliberately not read')
                path.unlink()

    def test_actual_cargo_links_override_skips_panic_script_and_removal_detects_it(self):
        host = next(line[6:] for line in subprocess.check_output(['rustc', '-vV'], text=True, encoding='utf-8').splitlines() if line.startswith('host: '))
        # Outside the checkout so removal cannot inherit its Linux override.
        with tempfile.TemporaryDirectory(prefix='auditorylab-font-', dir=qualify.ROOT.parent) as directory:
            root = Path(directory)
            (root / 'src').mkdir()
            (root / '.cargo').mkdir()
            (root / 'Cargo.toml').write_text('[package]\nname="system-font-override-probe"\nversion="0.0.0"\nedition="2024"\nlinks="freetype"\nbuild="build.rs"\n[workspace]\n', encoding='utf-8')
            (root / 'src/lib.rs').write_text('pub fn identity() -> u8 { 1 }\n', encoding='utf-8')
            (root / 'build.rs').write_text('fn main() { panic!("SYSTEM_FREETYPE_OVERRIDE_MISSING"); }\n', encoding='utf-8')
            config = root / '.cargo/config.toml'
            config.write_text('[target.' + host + '.freetype]\nrustc-link-lib=["dylib=freetype"]\n', encoding='utf-8')
            environment = os.environ.copy()
            environment['CARGO_TARGET_DIR'] = str(root / 'artifacts')
            environment['CARGO_HOME'] = str(root / 'cargo-home')
            environment['RUSTUP_TOOLCHAIN'] = subprocess.check_output(['rustup', 'show', 'active-toolchain'], cwd=qualify.ROOT, text=True, encoding='utf-8').split()[0]
            command = ['cargo', 'check', '--offline', '--target', host]
            passed = subprocess.run(command, cwd=root, env=environment, capture_output=True, text=True, encoding='utf-8', timeout=60)
            self.assertEqual(passed.returncode, 0, passed.stdout + passed.stderr)
            config.unlink()
            rejected = subprocess.run(command, cwd=root, env=environment, capture_output=True, text=True, encoding='utf-8', timeout=60)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn('SYSTEM_FREETYPE_OVERRIDE_MISSING', rejected.stderr)


if __name__ == '__main__':
    unittest.main()
