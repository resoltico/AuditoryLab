# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Required architecture tests cannot disappear behind successful Cargo commands."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import qualify


class ArchitectureQualificationTests(unittest.TestCase):
    def test_actual_cargo_autotest_exclusion_is_rejected(self):
        with tempfile.TemporaryDirectory(prefix='auditorylab-discovery-', dir=qualify.ROOT.parent) as temporary:
            root = Path(temporary)
            package = root / 'crates/auditory-app'
            (package / 'src').mkdir(parents=True)
            (package / 'tests').mkdir()
            (package / 'src/lib.rs').write_text('', encoding='utf-8')
            driver = package / 'tests/frontend_boundary.rs'
            source = """
#[test] fn delivered_frontends_obey_boundary() {}
mod roles { mod tests {
    #[test] fn declared_dependencies_reject_aliases_targets_and_builds() {}
    #[test] fn whole_workspace_roles_reject_wrong_directions_and_unclassified_members() {}
    #[test] fn resolved_gpu_features_and_math_override_survive_feature_unification() {}
} }
mod source { mod tests {
    #[test] fn canonical_source_containment_is_checked() {}
    #[test] fn workspace_package_and_source_roots_cannot_relocate() {}
    #[test] fn internal_production_inclusions_cannot_escape_admitted_inputs() {}
} }
"""
            driver.write_text(source, encoding='utf-8')
            for name in qualify.ESSENTIAL_APP_TESTS:
                if name != 'frontend_boundary':
                    (package / ('tests/' + name + '.rs')).write_text('#[test] fn behavior() {}', encoding='utf-8')
            (root / 'Cargo.toml').write_text('[workspace]\nmembers=["crates/auditory-app"]\nresolver="3"\n', encoding='utf-8')
            manifest = package / 'Cargo.toml'
            base = '[package]\nname="auditory-app"\nversion="0.0.0"\nedition="2024"\n'
            environment = os.environ.copy()
            environment['CARGO_HOME'] = str(root / 'cargo-home')
            environment['CARGO_TARGET_DIR'] = str(root / 'artifacts')
            environment['RUSTUP_TOOLCHAIN'] = subprocess.check_output(['rustup', 'show', 'active-toolchain'], cwd=qualify.ROOT, text=True, encoding='utf-8').split()[0]
            for exclusion in ('', 'autotests=false\n', 'autotests=false\n[[test]]\nname="frontend_boundary"\npath="tests/frontend_boundary.rs"\n', '[lib]\ntest=false\n'):
                manifest.write_text(base + exclusion, encoding='utf-8')
                result = subprocess.run(['cargo', 'metadata', '--offline', '--format-version', '1', '--no-deps'], cwd=root,
                                        env=environment, capture_output=True, text=True, encoding='utf-8', timeout=30)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                metadata = json.loads(result.stdout)
                if exclusion:
                    with self.assertRaises(ValueError):
                        qualify.require_application_test_targets(metadata, root)
                else:
                    qualify.require_application_test_targets(metadata, root)
                    command = ['cargo', 'test', '--offline', '-p', 'auditory-app', '--test', 'frontend_boundary']
                    listed = subprocess.run([*command, '--', '--list'], cwd=root, env=environment, capture_output=True, text=True, encoding='utf-8', timeout=30)
                    self.assertEqual(listed.returncode, 0, listed.stdout + listed.stderr)
                    discovered = qualify.architecture_test_names(listed.stdout, qualify.ARCHITECTURE_CONTROLS)
                    executed = subprocess.run([*command, '--', '--test-threads=1'], cwd=root, env=environment, capture_output=True, text=True, encoding='utf-8', timeout=30)
                    self.assertEqual(executed.returncode, 0, executed.stdout + executed.stderr)
                    qualify.require_architecture_execution(executed.stdout, discovered)
                    driver.write_text(source.replace('#[test]', '#[test]\n#[ignore]', 1), encoding='utf-8')
                    ignored = subprocess.run([*command, '--', '--test-threads=1'], cwd=root, env=environment, capture_output=True, text=True, encoding='utf-8', timeout=30)
                    self.assertEqual(ignored.returncode, 0, ignored.stdout + ignored.stderr)
                    with self.assertRaises(ValueError):
                        qualify.require_architecture_execution(ignored.stdout, discovered)
                    for field, value in [('kind', ['bin']), ('test', False), ('src_path', str(root / 'elsewhere.rs'))]:
                        changed = json.loads(result.stdout)
                        target = next(item for item in changed['packages'][0]['targets'] if item['name'] == 'frontend_boundary')
                        target[field] = value
                        with self.subTest(field=field), self.assertRaises(ValueError):
                            qualify.require_application_test_targets(changed, root)

    def test_actual_cargo_bin_test_false_cannot_exclude_gui_capacity_controls(self):
        with tempfile.TemporaryDirectory(prefix='auditorylab-gui-discovery-', dir=qualify.ROOT.parent) as temporary:
            root = Path(temporary)
            package = root / 'crates/auditory-gui'
            (package / 'src').mkdir(parents=True)
            (package / 'src/main.rs').write_text('fn main() {}', encoding='utf-8')
            (root / 'Cargo.toml').write_text('[workspace]\nmembers=["crates/auditory-gui"]\nresolver="3"\n', encoding='utf-8')
            manifest = package / 'Cargo.toml'
            base = '[package]\nname="auditory-gui"\nversion="0.0.0"\nedition="2024"\n[[bin]]\nname="auditorylab-gui"\npath="src/main.rs"\n'
            environment = os.environ.copy()
            environment['CARGO_HOME'] = str(root / 'cargo-home')
            environment['RUSTUP_TOOLCHAIN'] = subprocess.check_output(['rustup', 'show', 'active-toolchain'], cwd=qualify.ROOT, text=True, encoding='utf-8').split()[0]
            for setting in ('test=true\n', 'test=false\n'):
                manifest.write_text(base + setting, encoding='utf-8')
                result = subprocess.run(['cargo', 'metadata', '--offline', '--format-version', '1', '--no-deps'], cwd=root,
                                        env=environment, capture_output=True, text=True, encoding='utf-8', timeout=30)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                if setting == 'test=true\n':
                    qualify.require_gui_unit_target(json.loads(result.stdout), root)
                else:
                    with self.assertRaisesRegex(ValueError, 'excluded'):
                        qualify.require_gui_unit_target(json.loads(result.stdout), root)
        listing = '\n'.join(name + ': test' for name in qualify.gui_unit_controls('aarch64-apple-darwin'))
        qualify.architecture_test_names(listing, qualify.gui_unit_controls('aarch64-apple-darwin'))
        with self.assertRaises(ValueError):
            qualify.architecture_test_names(listing.replace('short_overallocated_string_and_growth_are_rejected', 'unrelated'),
                                            qualify.gui_unit_controls('aarch64-apple-darwin'))

    def test_required_controls_must_be_discovered_and_every_discovered_test_executed(self):
        names = sorted(qualify.ARCHITECTURE_CONTROLS | {'additional_meaningful_control'})
        listing = '\n'.join(name + ': test' for name in names)
        discovered = qualify.architecture_test_names(listing, qualify.ARCHITECTURE_CONTROLS)
        evidence = '\n'.join('test ' + name + ' ... ok' for name in names)
        summary = f'\ntest result: ok. {len(names)} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n'
        qualify.require_architecture_execution(evidence + summary, discovered)
        for rejected in ('', listing.replace(sorted(qualify.ARCHITECTURE_CONTROLS)[0] + ': test', ''), listing + '\n' + names[0] + ': test'):
            with self.assertRaises(ValueError):
                qualify.architecture_test_names(rejected, qualify.ARCHITECTURE_CONTROLS)
        for rejected in ('test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;',
                         evidence.replace('test ' + names[0] + ' ... ok', 'test ' + names[0] + ' ... ignored') + summary,
                         evidence + summary.replace('0 filtered out', '1 filtered out'),
                         evidence + '\n' + 'test ' + names[0] + ' ... ok' + summary,
                         evidence.replace('test ' + names[0], 'test unexpected') + summary):
            with self.assertRaises(ValueError):
                qualify.require_architecture_execution(rejected, discovered)

    def test_exact_required_app_capacity_control_cannot_be_ignored(self):
        name = sorted(qualify.APP_CAPACITY_CONTROLS)[0]
        evidence = f'test {name} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out;'
        qualify.require_architecture_execution(evidence, {name}, filtered=4)
        for rejected in (evidence.replace('... ok', '... ignored').replace('1 passed', '0 passed').replace('0 ignored', '1 ignored'),
                         evidence.replace('4 filtered out', '3 filtered out')):
            with self.assertRaises(ValueError):
                qualify.require_architecture_execution(rejected, {name}, filtered=4)

    def test_explicit_architecture_target_discovery_and_execution_are_authoritative(self):
        def output(command, environment, log):
            if 'metadata' in command:
                return '{}'
            if '--lib' in command:
                names = sorted(qualify.APP_CAPACITY_CONTROLS | {'intentionally_ignored_benchmark'})
                if '--list' in command:
                    return '\n'.join(name + ': test' for name in names)
                name = command[command.index('--') - 1]
                return f'test {name} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; {len(names) - 1} filtered out;'
            name = command[command.index('--test') + 1] if '--test' in command else None
            names = sorted(qualify.essential_app_tests('native-target')[name] if name else qualify.gui_unit_controls('native-target'))
            if '--list' in command:
                return '\n'.join(name + ': test' for name in names)
            return '\n'.join('test ' + name + ' ... ok' for name in names) + f'\ntest result: ok. {len(names)} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;'
        with mock.patch.object(qualify, 'require_gui_unit_target') as gui, \
             mock.patch.object(qualify, 'require_application_test_targets') as target, \
             mock.patch.object(qualify, 'run_logged', side_effect=output) as command:
            qualify.architecture_checks('native-target', {'controlled': 'environment'}, Path('/owned/results'))
        target.assert_called_once_with({})
        gui.assert_called_once_with({})
        calls = [call.args[0] for call in command.call_args_list]
        self.assertIn('--no-deps', calls[0])
        self.assertEqual(len(calls), 4 + 2 * (len(qualify.ESSENTIAL_APP_TESTS) + 1))
        self.assertIn('--lib', calls[1])
        self.assertTrue(all('--exact' in call for call in calls[2:4]))
        for index in range(4, len(calls), 2):
            self.assertEqual(calls[index][-2:], ['--', '--list'])
            self.assertEqual(calls[index + 1][-2:], ['--', '--test-threads=1'])
            self.assertEqual(calls[index][:-2], calls[index + 1][:-2])



if __name__ == '__main__':
    unittest.main()
