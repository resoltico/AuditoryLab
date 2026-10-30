#!/usr/bin/env python3
# Copyright (c) 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Independent artifact checks. These do not qualify native app integration."""
import argparse
import hashlib
import json
import math
import os
import stat
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

ART = Path(__file__).resolve().parent
ROOT = ART.parents[1]
EXAMPLES = ROOT / 'crates/app/src/learning/band_examples.json'
NS = {'s': 'http://www.w3.org/2000/svg'}
EXPECTED_RATES = {'delta': 2, 'theta': 6, 'alpha': 10, 'beta': 20, 'gamma': 32}


def run_export(art, output, examples=EXAMPLES, success=True):
    result = subprocess.run([sys.executable, str(art / 'generate_art.py'), str(output),
                             '--examples', str(examples)], capture_output=True, text=True)
    if success:
        assert result.returncode == 0, result.stdout + result.stderr
    else:
        assert result.returncode != 0, 'Negative control was incorrectly accepted'
        assert 'Cannot export onboarding artwork:' in result.stderr, result.stderr
    return result


def points(d):
    values = re.findall(r'[ML](-?\d+(?:\.\d+)?) (-?\d+(?:\.\d+)?)', d)
    return [(float(x), float(y)) for x, y in values]


def check_sine(curve, slot, hz):
    rows = points(curve)
    assert len(rows) >= 81
    span = slot['x1'] - slot['x0']
    bound = 0.005 / slot['amplitude'] + 2 * math.pi * hz * 0.005 / span + 1e-11
    normalized = []
    for x, y in rows:
        t = (x - slot['x0']) / span
        z = (slot['center_y'] - y) / slot['amplitude']
        assert abs(z - math.sin(2 * math.pi * hz * t)) <= bound, (hz, t, z, bound)
        normalized.append(z)
    assert abs(normalized[0]) < 1e-10 and abs(normalized[-1]) < 1e-10
    # These literal quarter-cycle values do not reuse the renderer's sine formula.
    for fraction, expected in [(0, 0), (0.25, 1), (0.5, 0), (0.75, -1), (1, 0)]:
        wanted = fraction / hz
        i = min(range(len(rows)), key=lambda j: abs((rows[j][0] - slot['x0']) / span - wanted))
        assert abs((rows[i][0] - slot['x0']) / span - wanted) <= .005 / span + 1e-12
        assert abs(normalized[i] - expected) <= .005 / slot['amplitude'] + 1e-11
    positive = sum(normalized[i] > normalized[i-1] and normalized[i] > normalized[i+1]
                   for i in range(1, len(rows)-1))
    negative = sum(normalized[i] < normalized[i-1] and normalized[i] < normalized[i+1]
                   for i in range(1, len(rows)-1))
    assert positive == hz and negative == hz, (hz, positive, negative)
    return len(rows)


def verify_assets(folder, layout, rates):
    checks = []
    for stem, item in layout['masters'].items():
        theme_curves = []
        for appearance in ['light', 'dark']:
            path = folder / (stem + '-' + appearance + '.svg')
            text = path.read_text()
            assert 'SPDX-License-Identifier: LicenseRef-Proprietary-Artwork' in text
            tree = ET.fromstring(text)
            assert not tree.findall('.//s:text', NS) and not tree.findall('.//s:image', NS)
            curves = []
            for token, slot in item['wave_slots'].items():
                hz = rates[slot['band']]
                if 'band' in item:
                    curve = tree.find('.//s:path[@data-wave="' + item['band'] + '"]', NS).attrib['d']
                    assert str(hz) + ' CYCLES/S' in text
                    assert str(hz) in tree.find('s:title', NS).text
                else:
                    # Main source stroke is distinct from glow/fill and lettering paths.
                    main = [p.attrib['d'] for p in tree.findall('.//s:path', NS)
                            if p.attrib.get('stroke-width') == '2.6' and len(p.attrib.get('d', '')) > 1000]
                    curve = main[list(item['wave_slots']).index(token)]
                count = check_sine(curve, slot, hz)
                curves.append(curve)
                checks.append({'asset': path.name, 'band': slot['band'], 'hz': hz,
                               'independent_sine_and_quarters': True, 'points': count})
            theme_curves.append(curves)
        assert theme_curves[0] == theme_curves[1], 'Theme change altered wave geometry'
    return checks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--scratch', required=True, type=Path)
    parser.add_argument('--result', type=Path)
    args = parser.parse_args()
    args.scratch.mkdir(parents=True, exist_ok=True)
    data = json.loads(EXAMPLES.read_text())
    rates = {entry['band']: entry['example_hz'] for entry in data['examples']}
    assert rates == EXPECTED_RATES
    layout = json.loads((ART / 'source/layout.json').read_text())
    manifest = {}
    for line in (ART / 'SHA256SUMS').read_text().splitlines():
        digest, name = line.split('  ', 1)
        assert name not in manifest
        manifest[name] = digest
    actual_paths = {str(path.relative_to(ART)) for path in ART.rglob('*')
                    if path.is_file() and path != ART / 'SHA256SUMS'}
    assert set(manifest) == actual_paths, 'Artwork inventory has missing/unexpected files'
    for name, digest in manifest.items():
        assert hashlib.sha256((ART / name).read_bytes()).hexdigest() == digest, name
    history = json.loads((ART / 'reference/retained-art-inventory.json').read_text())
    for entry in history:
        path = entry['package'].removeprefix('implementation-materials/artwork/onboarding/')
        retained = ART / path
        assert hashlib.sha256(retained.read_bytes()).hexdigest() == entry['sha256'], path
    with tempfile.TemporaryDirectory(prefix='onboarding-qa-', dir=args.scratch) as temp:
        work = Path(temp)
        output = work / 'exports'
        run_export(ART, output)
        expected_files = {stem + '-' + theme + '.svg'
                          for stem in layout['masters'] for theme in ['light', 'dark']}
        assert {p.name for p in output.iterdir()} == expected_files
        for name in expected_files:
            assert (output / name).read_bytes() == (ART / 'assets' / name).read_bytes(), name
        checks = verify_assets(output, layout, rates)
        # Old pulse envelopes must fail the same sine check, even at the same rate.
        slot = {'x0': 0, 'x1': 1000, 'center_y': 100, 'amplitude': 50}
        wrong = ' '.join(('M' if i == 0 else 'L') + str(i * 10) + ' '
                         + str(50 if (i * 0.1) % 1 < .5 else 150) for i in range(101))
        caught = False
        try:
            check_sine(wrong, slot, 10)
        except AssertionError:
            caught = True
        assert caught, 'The retired envelope negative control must be detected'
        # Reuse a small isolated art copy for human-edit failure/consistency controls.
        changed = work / 'editable-art'
        shutil.copytree(ART, changed, ignore=shutil.ignore_patterns('reference', 'assets', '__pycache__'))
        moved = json.loads((changed / 'source/layout.json').read_text())
        wave = moved['masters']['spectral-ribbon']['wave_slots']['WAVE_DELTA']
        wave.update(x0=61.5, x1=310.5, center_y=350)
        (changed / 'source/layout.json').write_text(json.dumps(moved))
        moved_output = work / 'moved'
        run_export(changed, moved_output)
        moved_svg = (moved_output / 'spectral-ribbon-light.svg').read_text()
        assert ' L310.5 350 L61.5 350Z' in moved_svg
        assert ' L302.5 344 L37.5 344Z' not in moved_svg
        revised = work / 'revised-examples.json'
        revised_data = json.loads(EXAMPLES.read_text())
        revised_data['examples'][-1]['example_hz'] = 36
        revised.write_text(json.dumps(revised_data))
        changed_output = work / 'changed-frequency'
        run_export(ART, changed_output, revised)
        revised_rates = dict(rates, gamma=36)
        verify_assets(changed_output, layout, revised_rates)
        assert '36 CYCLES/S' in (changed_output / 'gamma-rhythm-dark.svg').read_text()
        for i, bad in enumerate([True, '32', 0, -1, 1e200, float('nan'), float('inf'), -float('inf')]):
            invalid_data = json.loads(EXAMPLES.read_text())
            invalid_data['examples'][-1]['example_hz'] = bad
            invalid = work / ('bad-' + str(i) + '.json')
            invalid.write_text(json.dumps(invalid_data))
            run_export(ART, work / ('bad-output-' + str(i)), invalid, success=False)
        external = changed / 'source/delta-rhythm.svg.in'
        original = external.read_text()
        external.write_text(original.replace('xlink:href="#g', 'xlink:href="https://invalid.example/g', 1))
        rejected = run_export(changed, work / 'external-reference', success=False)
        assert 'Missing or external SVG reference' in rejected.stderr
        external.write_text(original)
        missing = json.loads((changed / 'source/lettering.json').read_text())
        del missing['glyphs']['Z']
        (changed / 'source/lettering.json').write_text(json.dumps(missing))
        result = run_export(changed, work / 'missing-glyph', success=False)
        assert 'Missing or external SVG reference' in result.stderr
        redirected = work / 'redirected'
        redirected.mkdir()
        master = changed / 'source/gamma-rhythm.svg.in'
        preserved = master.read_bytes()
        (redirected / 'gamma-rhythm-light.svg').symlink_to(master)
        refusal = run_export(ART, redirected, success=False)
        assert 'may not be a symbolic link' in refusal.stderr
        assert master.read_bytes() == preserved
        assert {p.name for p in redirected.iterdir()} == {'gamma-rhythm-light.svg'}
        # A source hard-linked at an otherwise legitimate output path must keep
        # its bytes and inode. Export replaces the output entry with a fresh file.
        hardlinked = work / 'hardlinked'
        hardlinked.mkdir()
        source = changed / 'source/delta-rhythm.svg.in'
        source_bytes = source.read_bytes()
        source_identity = (source.stat().st_dev, source.stat().st_ino)
        linked_output = hardlinked / 'delta-rhythm-light.svg'
        os.link(source, linked_output)
        assert linked_output.samefile(source)
        run_export(ART, hardlinked)
        assert source.read_bytes() == source_bytes
        assert (source.stat().st_dev, source.stat().st_ino) == source_identity
        assert not linked_output.samefile(source)
        for name in expected_files:
            assert (hardlinked / name).read_bytes() == (ART / 'assets' / name).read_bytes()
        # Normal repeated regeneration remains supported and preserves a chosen
        # regular output mode. No sibling temporary files may survive publication.
        regular = hardlinked / 'gamma-rhythm-dark.svg'
        regular.chmod(0o640)
        mode = stat.S_IMODE(regular.stat().st_mode)
        run_export(ART, hardlinked)
        assert stat.S_IMODE(regular.stat().st_mode) == mode
        assert {p.name for p in hardlinked.iterdir()} == expected_files
        for name in expected_files:
            assert (hardlinked / name).read_bytes() == (ART / 'assets' / name).read_bytes()
        nonregular = work / 'nonregular'
        nonregular.mkdir()
        (nonregular / 'delta-rhythm-light.svg').mkdir()
        refusal = run_export(ART, nonregular, success=False)
        assert 'must be a regular file' in refusal.stderr
        assert {p.name for p in nonregular.iterdir()} == {'delta-rhythm-light.svg'}
        unsafe = run_export(ART, ART / 'source/forbidden-output', success=False)
        assert 'outside the artwork source tree' in unsafe.stderr
    result = {'kind': 'artifact math/generation/licensing QA; not native implementation qualification',
              'runtime': sys.version, 'assets_reproduced': 12, 'wave_checks': checks,
              'retired_envelope_detected': True, 'edited_fill_uses_current_slot': True,
              'gamma_edit_updates_both_views_and_caption': True,
              'invalid_numeric_types_and_cost_rejected': True, 'missing_glyph_rejected': True,
              'source_overwrite_rejected': True, 'hardlinked_source_preserved': True,
              'repeat_regeneration_and_output_mode_preserved': True, 'nonregular_output_rejected': True, 'redirected_output_rejected_before_writing': True, 'complete_artwork_manifest_verified': True, 'historical_source_bytes_preserved': len(history), 'native_integration_verified': False}
    if args.result:
        args.result.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: value for key, value in result.items() if key != 'wave_checks'}, indent=2))


if __name__ == '__main__':
    main()
