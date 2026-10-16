# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Native product isolation and the publication-free qualification boundary."""
from pathlib import Path
import os
import json
import tempfile
import subprocess
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import qualify
from release import MACOS_MINIMUM, TARGETS, expected_identity, check_build_identity, validate_build_request, archive_name


class QualificationTests(unittest.TestCase):
    def test_native_failure_retains_full_log_status_command_and_cause(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'native.log'
            command = [sys.executable, '-c', "import sys; print('native output'); print('original native failure',file=sys.stderr); sys.exit(7)"]
            with self.assertRaises(ValueError) as caught:
                qualify.run_logged(command, os.environ.copy(), log)
            for evidence in ('original native failure', 'native output', '7', str(log)):
                self.assertIn(evidence, str(caught.exception))
            self.assertIn('original native failure', log.read_text())
            self.assertIsInstance(caught.exception.__cause__, subprocess.CalledProcessError)

    def test_source_requires_clean_exact_revision(self):
        for values in [('a' * 40, ''), ('b' * 40, ''), ('a' * 40, ' M source.rs')]:
            with mock.patch.object(qualify, 'run', side_effect=values):
                if values == ('a' * 40, ''):
                    qualify.check_source('a' * 40)
                else:
                    with self.assertRaises(ValueError):
                        qualify.check_source('a' * 40)

    def test_all_macos_products_share_floor_and_reset_ambient_deployment_target(self):
        self.assertEqual(MACOS_MINIMUM, '15.0')
        for entry in TARGETS:
            environments = []
            for product in ('cli', 'gui'):
                with mock.patch.dict(os.environ, {'MACOSX_DEPLOYMENT_TARGET': '99',
                                                 'AUDITORY_BUILD_KIND': 'release',
                                                 'AUDITORY_SOURCE_REVISION': 'b' * 40}, clear=True):
                    environment = qualify.controlled_environment(product, entry, 'local', None)
                    qualified = qualify.controlled_environment(product, entry, 'candidate', 'a' * 40)
                if 'apple' in entry['target']:
                    self.assertEqual(environment['MACOSX_DEPLOYMENT_TARGET'], '15.0')
                else:
                    self.assertNotIn('MACOSX_DEPLOYMENT_TARGET', environment)
                self.assertNotIn('AUDITORY_SOURCE_REVISION', environment)
                self.assertNotIn('AUDITORY_BUILD_KIND', environment)
                self.assertEqual(qualified['AUDITORY_BUILD_KIND'], 'candidate')
                self.assertEqual(qualified['AUDITORY_SOURCE_REVISION'], 'a' * 40)
                if 'apple' in entry['target']:
                    self.assertEqual(qualified['MACOSX_DEPLOYMENT_TARGET'], '15.0')
                else:
                    self.assertNotIn('MACOSX_DEPLOYMENT_TARGET', qualified)
                environments.append(environment)
            self.assertNotEqual(environments[0]['CARGO_TARGET_DIR'], environments[1]['CARGO_TARGET_DIR'])


    def test_compiler_override_checks_repeat_even_when_cargo_would_use_cache(self):
        entry = TARGETS[0]
        with mock.patch.dict(os.environ, {}, clear=True):
            qualify.controlled_environment('gui', entry, 'local', None)
            for name, value in [('SLINT_BUNDLE_TRANSLATIONS', '/external/tree'),
                                ('SLINT_EMBED_RESOURCES', 'false'), ('SLINT_UNKNOWN_OVERRIDE', '1'),
                                ('SLINT_COMPILER_DENY_WARNINGS', '0'), ('DEP_MCU_BOARD_SUPPORT_PATH', '/external')]:
                with self.subTest(name=name), mock.patch.dict(os.environ, {name: value}), self.assertRaisesRegex(ValueError, name):
                    qualify.controlled_environment('gui', entry, 'local', None)
            with mock.patch.dict(os.environ, {'SLINT_COMPILER_DENY_WARNINGS': '1'}):
                qualify.controlled_environment('gui', entry, 'local', None)

    def test_identity_inputs_and_names_distinguish_local_candidate_release(self):
        for kind, revision in [('local', 'a' * 40), ('candidate', None), ('release', ''), ('unknown', None), ('candidate', 'g' * 40)]:
            with self.subTest(kind=kind, revision=revision), self.assertRaises(ValueError):
                validate_build_request(kind, revision)
        local = expected_identity('gui', '0.2.0', TARGETS[0]['target'], 'local', None)
        self.assertIsNone(local['source_revision'])
        self.assertIn('-local-', archive_name('macos-aarch64', '0.2.0', 'gui', 'local'))
        self.assertIn('-dev-' + 'a' * 12, archive_name('macos-aarch64', '0.2.0', 'gui', 'candidate', 'a' * 40))
        with self.assertRaises(ValueError):
            check_build_identity(local, 'gui', '0.2.0', TARGETS[0]['target'], 'release', 'a' * 40)

    def test_shipping_profile_rejects_renderer_or_version_drift(self):
        baseline = "slint v1.18.1|std,compat-1-18,backend-winit,renderer-software,renderer-femtovg-wgpu,i-slint-renderer-femtovg,accessibility,raw-window-handle-06,unstable-winit-030\nslint-build v1.18.1|compat-1-18\n"
        with mock.patch.object(qualify, 'run', return_value=baseline):
            qualify.check_gui_profile(TARGETS[0]['target'])
        for changed in (baseline.replace('slint-build v1.18.1', 'slint-build v1.18.0'),
                        baseline.replace(',accessibility', ''), baseline.replace(',unstable-winit-030', ''), baseline.replace('compat-1-18\n', 'compat-1-18,default\n'),
                        baseline + 'i-slint-core v1.18.1|std,system-testing\n',
                        baseline + 'i-slint-backend-winit v1.18.1|renderer-software,mcp\n',
                        baseline + 'i-slint-renderer-skia v1.18.1|default\n', baseline + 'wgpu v30.0.1|static-dxc\n',
                        baseline + 'i-slint-core v1.18.1|unstable-wgpu-29\n', baseline + 'libm v0.2.16|arch,default\n'):
            with mock.patch.object(qualify, 'run', return_value=changed), self.assertRaises(ValueError):
                qualify.check_gui_profile(TARGETS[0]['target'])

    def test_linux_profile_preserves_csd_titles_with_system_font_producers(self):
        baseline = "slint v1.18.1|std,compat-1-18,backend-winit,renderer-software,renderer-femtovg-wgpu,i-slint-renderer-femtovg,accessibility,raw-window-handle-06,unstable-winit-030\nslint-build v1.18.1|compat-1-18\nsctk-adwaita v0.10.1|crossfont\ncrossfont v0.9.0|\nfreetype-sys v0.20.1|\nyeslogic-fontconfig-sys v6.0.1|\nwinit v0.30.13|wayland-csd-adwaita-crossfont\n"
        with mock.patch.object(qualify, 'run', return_value=baseline):
            qualify.check_gui_profile('aarch64-unknown-linux-gnu')
        for changed in (baseline.replace('wayland-csd-adwaita-crossfont', 'wayland-csd-adwaita-notitle'),
                        baseline.replace('sctk-adwaita v0.10.1|crossfont', 'sctk-adwaita v0.10.1|ab_glyph,crossfont'),
                        baseline.replace('yeslogic-fontconfig-sys v6.0.1|', 'yeslogic-fontconfig-sys v6.0.1|dlopen'),
                        baseline.replace('crossfont v0.9.0|', 'crossfont v0.8.0|'), baseline + 'ttf-parser v0.25.1|\n'):
            with mock.patch.object(qualify, 'run', return_value=changed), self.assertRaises(ValueError):
                qualify.check_gui_profile('aarch64-unknown-linux-gnu')

    def test_renderer_claim_requires_actual_first_frame_and_labels_cpu_only_fallback(self):
        gpu = 'NATIVE_WINDOW_STARTED\nSlint: Build config: release; Backend: FemtoVG renderer with WGPU backend\naverage frames per second: 1\n'
        software = 'NATIVE_WINDOW_STARTED\nBackend: software\naverage frames per second: 1 details from last frame: (dirty: [])\n'
        self.assertTrue(qualify.renderer_evidence(gpu, 'gpu')['wgpu_first_frame'])
        self.assertEqual(qualify.renderer_evidence(gpu, 'gpu')['hardware_adapter'], 'unverified')
        self.assertTrue(qualify.renderer_evidence(gpu.replace('per second: 1', 'per second: 0'), 'gpu')['wgpu_first_frame'])
        fallback = qualify.renderer_evidence(software, 'gpu')
        self.assertTrue(fallback['wgpu_unavailable'])
        self.assertEqual(fallback['actual'], 'software')
        self.assertFalse(qualify.renderer_evidence(software, 'software')['wgpu_first_frame'])
        for text, requested in [('NATIVE_WINDOW_STARTED', 'gpu'),
                                ('Backend: FemtoVG renderer with WGPU backend', 'gpu'),
                                ('NATIVE_WINDOW_STARTED\nBackend: FemtoVG renderer with WGPU backend\n', 'gpu'),
                                ('NATIVE_WINDOW_STARTED\nBackend: software', 'software'),
                                (gpu, 'software'), (gpu + software, 'gpu')]:
            with self.subTest(requested=requested), self.assertRaises(ValueError):
                qualify.renderer_evidence(text, requested)

    def test_runtime_gpu_overrides_never_leak_into_build_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            environment = {'SLINT_WGPU_CPU': '1', 'WGPU_ADAPTER_NAME': 'cpu', 'WGPU_BACKEND': 'gl'}
            def launch(command, **kwargs):
                child = kwargs['env']
                self.assertTrue(all(name not in child for name in environment))
                self.assertEqual(child['SLINT_BACKEND'], 'winit-femtovg-wgpu')
                self.assertEqual(child['SLINT_DEBUG_PERFORMANCE'], 'refresh_lazy,console')
                kwargs['stdout'].write('NATIVE_WINDOW_STARTED\nBackend: FemtoVG renderer with WGPU backend\naverage frames per second: 1\n')
                return mock.Mock(wait=mock.Mock(return_value=0))
            with mock.patch.object(qualify.subprocess, 'Popen', side_effect=launch):
                result = qualify.run_renderer(Path('fixture'), 'aarch64-apple-darwin', environment, 'gpu', Path(directory) / 'window.log')
            self.assertTrue(result['wgpu_first_frame'])
            self.assertEqual(environment['SLINT_WGPU_CPU'], '1')
            self.assertNotIn('SLINT_DEBUG_PERFORMANCE', environment)

    def test_native_window_artifact_selection_is_exact_before_runtime_flags(self):
        artifact = {'reason': 'compiler-artifact', 'package_id': 'gui-id',
                    'target': {'name': 'native_window', 'kind': ['test']}, 'executable': '/owned/test-window'}
        for mode in ('zero', 'multiple', 'wrong-package', 'wrong-target'):
            wrong = json.loads(json.dumps(artifact))
            if mode == 'wrong-package':
                wrong['package_id'] = 'cli-id'
            elif mode == 'wrong-target':
                wrong['target']['kind'] = ['bin']
            records = [] if mode == 'zero' else [wrong, wrong] if mode == 'multiple' else [wrong]
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory, \
                 mock.patch.object(qualify, 'run_logged', return_value='\n'.join(json.dumps(record) for record in records)), \
                 mock.patch.object(qualify, 'run_renderer') as renderer, self.assertRaises(ValueError):
                qualify.native_window_checks('aarch64-apple-darwin', {}, Path(directory), 'gui-id')
            renderer.assert_not_called()
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(qualify, 'run_logged', return_value=json.dumps(artifact)) as build, \
             mock.patch.object(qualify, 'run_renderer', return_value={'actual': 'gpu'}) as renderer:
            qualify.native_window_checks('aarch64-apple-darwin', {}, Path(directory), 'gui-id')
            self.assertIn('--no-run', build.call_args.args[0])
            self.assertEqual(build.call_args.args[1], {})
            self.assertEqual(renderer.call_count, 2)

    def test_geometry_matrix_uses_fresh_empty_capture_dirs_and_runtime_only_scale(self):
        with tempfile.TemporaryDirectory() as temporary:
            results = Path(temporary)
            directories = []
            def capture(binary, target, environment, log, timeout):
                self.assertEqual(binary, Path('/owned/geometry'))
                self.assertEqual(timeout, 45)
                self.assertEqual(environment['AUDITORY_GUI_GEOMETRY_TEST'], '1')
                self.assertTrue(all(name not in environment for name in ('SLINT_WGPU_CPU', 'WGPU_ADAPTER_NAME', 'WGPU_BACKEND')))
                directory = Path(environment['AUDITORY_GUI_GEOMETRY_DIR'])
                self.assertTrue(directory.is_absolute())
                self.assertEqual(list(directory.iterdir()), [])
                self.assertNotEqual(log.parent, directory)
                directories.append(directory)
                (directory / 'manifest.json').write_text(json.dumps({'cases': [{'scale_factor': float(environment['SLINT_SCALE_FACTOR'])}] * 9}))
                return 'NATIVE_WINDOW_STARTED\nBackend: software\naverage frames per second: 1\n' + 'NATIVE_SIGNAL_GEOMETRY_CAPTURED case\n' * 9 + 'NATIVE_SIGNAL_GEOMETRY_COMPLETE cases=9\n'
            def oracle(command, environment, log):
                self.assertNotIn('SLINT_SCALE_FACTOR', environment)
                directory = Path(command[-1])
                (directory / 'pixels.json').write_text(json.dumps({'status': 'passed', 'cases': [{}] * 9}))
                return 'NATIVE_SIGNAL_GEOMETRY_PIXELS_OK cases=9\n'
            environment = {'SLINT_WGPU_CPU': '1', 'WGPU_ADAPTER_NAME': 'cpu', 'WGPU_BACKEND': 'gl'}
            with mock.patch.object(qualify, 'build_native_test', return_value=Path('/owned/geometry')) as build, \
                 mock.patch.object(qualify, 'run_native_test', side_effect=capture), \
                 mock.patch.object(qualify, 'run_logged', side_effect=oracle):
                records = qualify.native_signal_geometry_checks('aarch64-apple-darwin', environment, results, 'gui-id')
            self.assertEqual(build.call_args.args[-1], 'native_signal_geometry')
            self.assertEqual(len(set(directories)), 4)
            self.assertEqual([record['requested_scale'] for record in records], [1, 1.25, 1, 1.25])
            self.assertTrue(records[0]['renderer']['wgpu_unavailable'])
            self.assertEqual(environment['SLINT_WGPU_CPU'], '1')
            self.assertTrue((results / 'signal-geometry.json').is_file())

    def test_geometry_cannot_accept_skipped_capture_or_missing_oracle_report(self):
        for issue in ('skipped', 'scale', 'oracle', 'missing-report'):
            with self.subTest(issue=issue), tempfile.TemporaryDirectory() as temporary:
                results = Path(temporary)
                def capture(binary, target, environment, log, timeout):
                    directory = Path(environment['AUDITORY_GUI_GEOMETRY_DIR'])
                    (directory / 'manifest.json').write_text(json.dumps({'cases': [{'scale_factor': 9 if issue == 'scale' else float(environment['SLINT_SCALE_FACTOR'])}] * 9}))
                    markers = '' if issue == 'skipped' else 'NATIVE_SIGNAL_GEOMETRY_CAPTURED case\n' * 9 + 'NATIVE_SIGNAL_GEOMETRY_COMPLETE cases=9\n'
                    return 'NATIVE_WINDOW_STARTED\nBackend: software\naverage frames per second: 1\n' + markers
                def oracle(command, environment, log):
                    if issue == 'oracle':
                        return ''
                    return 'NATIVE_SIGNAL_GEOMETRY_PIXELS_OK cases=9\n'
                with mock.patch.object(qualify, 'build_native_test', return_value=Path('/owned/geometry')), \
                     mock.patch.object(qualify, 'run_native_test', side_effect=capture), \
                     mock.patch.object(qualify, 'run_logged', side_effect=oracle), self.assertRaises((ValueError, OSError)):
                    qualify.native_signal_geometry_checks('aarch64-apple-darwin', {}, results, 'gui-id')

    def test_ambient_preflight_rejects_before_any_cargo_or_host_query(self):
        with mock.patch.dict(os.environ, {'RUSTC': 'unreviewed-compiler'}, clear=True), \
             mock.patch.object(qualify, 'cargo_identities') as identities, \
             mock.patch.object(qualify, 'check_gui_profile') as profile, \
             mock.patch.object(qualify, 'run') as host, self.assertRaises(ValueError):
            qualify.qualify('aarch64-apple-darwin', 'local', None, Path('unused'), 'owner/repo')
        identities.assert_not_called()
        profile.assert_not_called()
        host.assert_not_called()

    def test_native_orchestration_qualifies_both_products_with_source_checks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            identities = {'cli': {'id':'cli-id', 'name': 'auditory-cli', 'bin': 'auditorylab', 'version': '0.2.0'},
                          'gui': {'id':'gui-id', 'name': 'auditory-gui', 'bin': 'auditorylab-gui', 'version': '0.2.0'}}
            def logged(command, environment, log):
                log.parent.mkdir(parents=True, exist_ok=True)
                text = 'test result: ok. 1 passed; 0 failed; 0 ignored;\nBENCH,active\nNATIVE_WINDOW_STARTED\n' + ''.join('native_termination ' + mode + ': passed\n' for mode in ('idle','blocked','repeated','dialog'))
                log.write_text(text)
                return text
            def packaged(target, version, repository, binary, output, receipt, product, kind, revision):
                output.mkdir(parents=True, exist_ok=True)
                archive = output / (product + '.zip')
                archive.write_bytes(product.encode())
                return archive
            with mock.patch.object(qualify, 'ROOT', root), mock.patch.object(qualify, 'run', return_value='host: aarch64-apple-darwin\n'), \
                 mock.patch.object(qualify, 'cargo_identities', return_value=identities), \
                 mock.patch.object(qualify, 'freestanding_core_check') as freestanding, mock.patch.object(qualify, 'architecture_checks') as architecture, mock.patch.object(qualify, 'check_ambient_cargo_configuration'), mock.patch.object(qualify, 'validate_offline_records'), mock.patch.object(qualify, 'check_gui_profile'), mock.patch.object(qualify, 'native_window_checks') as windows, mock.patch.object(qualify, 'native_signal_geometry_checks') as geometry, mock.patch.object(qualify, 'check_source') as source, \
                 mock.patch.object(qualify, 'run_logged', side_effect=logged) as commands, \
                 mock.patch.object(qualify, 'package', side_effect=packaged):
                archives = qualify.qualify('aarch64-apple-darwin', 'candidate', 'a' * 40, root / 'assets', 'owner/repo')
            self.assertEqual(len(archives), 2)
            self.assertEqual(source.call_count, 5)
            calls = [call.args[0] for call in commands.call_args_list]
            self.assertTrue(any('clippy' in command and '--all-targets' in command for command in calls))
            freestanding.assert_called_once()
            architecture.assert_called_once()
            windows.assert_called_once()
            geometry.assert_called_once()
            explicit_ignored = [command for command in calls if '--ignored' in command]
            self.assertEqual(len(explicit_ignored), 5)
            self.assertTrue(all('--lib' in command and '-p' in command for command in explicit_ignored))
            self.assertTrue((root / 'target/qualification-results/candidate/aarch64-apple-darwin/packages.json').is_file())

    def test_observation_measurements_require_all_active_modes_and_maximum_rate(self):
        from itertools import product
        rows = []
        for mode, hz, block, voices, shape, channels, fmt in product(
                ('disabled', 'drained', 'full', 'disconnected'), (44100, 48000, 96000, 768000),
                (64, 256, 1024), (1, 4, 16), ('plain', 'sam', 'gated'), (2, 6),
                ('f64', 'f32', 'i32', 'i24', 'i16')):
            rows.append(f'OBS_BENCH,{mode},{hz},{block},{voices},{shape},{channels},{fmt},128,{block * 128},128,1,1,1,1,1,999')
        qualify.validate_offline_records('observed_production_callback_budget', '\n'.join(rows))
        for changed in (rows[:-1], rows + rows[:1], [row.replace(',128,8192,128,', ',128,0,0,') for row in rows]):
            with self.assertRaises(ValueError):
                qualify.validate_offline_records('observed_production_callback_budget', '\n'.join(changed))
        maximum = 'OBSERVATION_MAX_RATE,hz=768000,polls=60,active_frames=1536000,capacity_bytes=7000000,owned_capacity_bytes=7000000,presentation_reserved_bytes=0,packets_per_poll=188,min_ns=1,median_ns=1,p95_ns=1,max_ns=999999999'
        qualify.validate_offline_records('maximum_rate_observation_and_snapshot_budget', maximum)
        for changed in ('', maximum + '\n' + maximum, maximum.replace('1536000', '0'), maximum.replace('7000000', '999999999'), maximum.replace('owned_capacity_bytes=7000000', 'owned_capacity_bytes=6999999'), maximum.replace('presentation_reserved_bytes=0', 'presentation_reserved_bytes=1')):
            with self.assertRaises(ValueError):
                qualify.validate_offline_records('maximum_rate_observation_and_snapshot_budget', changed)

    def test_development_wrapper_contains_no_publication_or_write_permissions(self):
        root = Path(__file__).resolve().parents[1]
        candidate = (root / '.github/workflows/candidate.yml').read_text()
        native = (root / '.github/workflows/native.yml').read_text()
        for text in (candidate, native):
            self.assertNotIn('contents: write', text)
            self.assertNotIn('gh release', text)
            self.assertNotIn('git push', text)
            self.assertNotIn('publish:', text)
        self.assertIn('ref: ${{ github.sha }}', native)
        self.assertNotIn('source-revision:', native)


if __name__ == '__main__':
    unittest.main()
