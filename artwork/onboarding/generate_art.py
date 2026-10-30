#!/usr/bin/env python3
# Copyright (c) 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Export separately licensed SVG masters using shared palettes and example rates.

Code is MPL-covered. Creative SVG masters, glyph paths, palettes and layouts in
source/, and generated artwork, have proprietary artwork terms in root LICENSE.
No rights to those inputs are granted by this script's software license.

Python 3.9+ standard library only. Ordinary app builds use tracked exports.
Run from a checkout: python3 artwork/onboarding/generate_art.py target/onboarding-art
The default examples file is crates/app/src/learning/band_examples.json. An explicit
--examples path permits inspecting this material outside the implementation tree.
"""
import argparse
from html import escape
import json
import math
import os
import stat
import tempfile
import re
import xml.etree.ElementTree as ET
from pathlib import Path
from string import Template

HERE = Path(__file__).resolve().parent
BAND_KEYS = ('delta', 'theta', 'alpha', 'beta', 'gamma')


def load_json(path):
    def reject_constant(value):
        raise ValueError('Nonfinite JSON number: ' + value)
    result = json.loads(path.read_text(encoding='utf-8'), parse_constant=reject_constant)
    if not isinstance(result, dict):
        raise ValueError('Expected a JSON object in ' + str(path))
    return result


def load_examples(path):
    data = load_json(path)
    numeric = lambda value: type(value) in (int, float)
    if (type(data.get('schema')) is not int or data.get('schema') != 1
            or not numeric(data.get('window_seconds')) or data.get('window_seconds') != 1
            or not numeric(data.get('normalized_amplitude')) or data.get('normalized_amplitude') != 1
            or not numeric(data.get('initial_phase_cycles')) or data.get('initial_phase_cycles') != 0):
        raise ValueError('Expected schema 1, one second, normalized amplitude 1 and zero phase')
    entries = data.get('examples', [])
    if (not isinstance(entries, list) or not all(isinstance(entry, dict) for entry in entries)
            or len(entries) != 5 or tuple(entry.get('band') for entry in entries) != BAND_KEYS):
        raise ValueError('Expected exactly Delta, Theta, Alpha, Beta and Gamma in order')
    for entry in entries:
        value = entry['example_hz']
        if type(value) not in (int, float) or not 0 < value <= 128:
            raise ValueError('Example frequencies must be finite JSON numbers in (0,128]; '
                             'this bounds each path to at most 4097 points')
    rates = {entry['band']: float(entry['example_hz']) for entry in entries}
    return rates


def sine_path(slot, rate):
    coordinates = [slot[key] for key in ('x0', 'x1', 'center_y', 'amplitude')]
    if not all(type(value) in (int, float) and math.isfinite(value) for value in coordinates):
        raise ValueError('Wave slot coordinates must be finite JSON numbers')
    if slot['x1'] <= slot['x0'] or slot['amplitude'] <= 0 or not 0 < rate <= 128:
        raise ValueError('Wave slots need increasing x, positive amplitude and bounded rate')
    count = max(80, int(rate * 32))
    values = []
    for i in range(count + 1):
        t = i / count
        x = slot['x0'] + (slot['x1'] - slot['x0']) * t
        y = slot['center_y'] - slot['amplitude'] * math.sin(2 * math.pi * rate * t)
        values.append(('M' if i == 0 else 'L') + f'{x:.2f} {y:.2f}')
    return ' '.join(values)


def glyph_definitions(lettering):
    return '\n'.join('<path id="g' + str(ord(char)) + '" d="' + escape(path, quote=True) + '"/>'
                     for char, path in lettering['glyphs'].items())


def lettering_label(text, placement, lettering, palette):
    advance, space = lettering['advance'], lettering['space_advance']
    width = (sum(space if c == ' ' else advance for c in text) - 2) * placement['size'] / 10
    x = placement['x']
    if placement['align'] == 'end':
        x -= width
    elif placement['align'] == 'middle':
        x -= width / 2
    scale = placement['size'] / 10
    escaped = escape(text, quote=True)
    parts = [f'<g data-lettering="{escaped}" aria-label="{escaped}" '
             f'transform="translate({x:.3f} {placement["y"]}) scale({scale:.5f})" '
             f'fill="none" stroke="{palette[placement["color"]]}" stroke-width=".8" '
             'stroke-linecap="round" stroke-linejoin="round">']
    cursor = 0
    for char in text:
        if char == ' ':
            cursor += space
            continue
        if char not in lettering['glyphs']:
            raise ValueError('Missing original glyph for: ' + repr(char))
        parts.append(f'<use xlink:href="#g{ord(char)}" x="{cursor}"/>')
        cursor += advance
    return '\n'.join(parts + ['</g>'])


def validate_svg(svg):
    tree = ET.fromstring(svg)
    ids = {element.attrib['id'] for element in tree.iter() if 'id' in element.attrib}
    for element in tree.iter():
        kind = element.tag.rsplit('}', 1)[-1]
        if kind in ('image', 'text', 'script', 'foreignObject'):
            raise ValueError('Font-free authored artwork may not contain ' + kind)
        for key, value in element.attrib.items():
            if key.rsplit('}', 1)[-1] == 'href':
                if not value.startswith('#') or value[1:] not in ids:
                    raise ValueError('Missing or external SVG reference: ' + value)
    if 'font-family' in svg or '@font-face' in svg:
        raise ValueError('The selected artwork must not depend on fonts')
    for target in re.findall(r'url\(([^)]+)\)', svg):
        if not target.startswith('#') or target[1:] not in ids:
            raise ValueError('Missing or external SVG paint reference: ' + target)


def replace_svg(destination, svg, mode):
    """Publish a fresh inode, so an existing hard link cannot truncate a source."""
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', newline='\n',
                                         prefix='.' + destination.name + '-', suffix='.tmp',
                                         dir=destination.parent, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(svg)
        temporary.chmod(mode)
        os.replace(temporary, destination)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def export(output, examples_path):
    output = output.resolve()
    if output == HERE or HERE in output.parents or output in HERE.parents:
        raise ValueError('Choose an explicit output directory outside the artwork source tree')
    rates = load_examples(examples_path)
    palette_data = load_json(HERE / 'source/palette.json')
    if (not isinstance(palette_data.get('themes'), dict)
            or set(palette_data['themes']) != {'light', 'dark'}):
        raise ValueError('The app consumes exactly the light and dark artwork pair')
    lettering = load_json(HERE / 'source/lettering.json')
    layout = load_json(HERE / 'source/layout.json')
    definitions = glyph_definitions(lettering)
    rendered = {}
    for stem, item in layout['masters'].items():
        template = Template((HERE / 'source' / item['template']).read_text(encoding='utf-8'))
        for appearance, palette in palette_data['themes'].items():
            values = dict(palette)
            values['GLYPH_DEFINITIONS'] = definitions
            for token, slot in item['wave_slots'].items():
                curve = sine_path(slot, rates[slot['band']])
                values[token] = curve
                values[token + '_FILL'] = (curve + f' L{slot["x1"]} {slot["center_y"]}'
                                           + f' L{slot["x0"]} {slot["center_y"]}Z')
            if 'band' in item:
                rate_text = format(rates[item['band']], '.12g')
                values['EXAMPLE_HZ'] = rate_text
                values['RATE_LABEL'] = lettering_label(rate_text + ' CYCLES/S',
                                                        item['rate_label'], lettering, palette)
            svg = template.substitute(values)
            validate_svg(svg)
            rendered[stem + '-' + appearance + '.svg'] = svg
    # Validate every existing directory entry before publication. Never open an
    # output inode for writing: it may also be a source through a hard link.
    modes = {}
    for name in rendered:
        destination = output / name
        try:
            existing = destination.lstat()
        except FileNotFoundError:
            modes[name] = 0o644
            continue
        if stat.S_ISLNK(existing.st_mode):
            raise ValueError('Generated output may not be a symbolic link: ' + str(destination))
        if not stat.S_ISREG(existing.st_mode):
            raise ValueError('Generated output must be a regular file: ' + str(destination))
        modes[name] = stat.S_IMODE(existing.st_mode)
    output.mkdir(parents=True, exist_ok=True)
    for name, svg in rendered.items():
        replace_svg(output / name, svg, modes[name])
    return sorted(rendered)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--examples', type=Path,
                        default=HERE.parents[1] / 'crates/app/src/learning/band_examples.json')
    args = parser.parse_args()
    try:
        files = export(args.output, args.examples)
    except (OSError, ValueError, KeyError, TypeError, OverflowError, ET.ParseError) as error:
        parser.exit(1, 'Cannot export onboarding artwork: ' + str(error) + '\n')
    print(f'Exported {len(files)} SVG files to {args.output}')


if __name__ == '__main__':
    main()
