# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Negative controls for the independent native chart pixel oracle."""
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import signal_pixels


def raster(scene, fault=None):
    width, height = 600, 420
    data = bytearray(bytes((19, 29, 43)) * (width * height))
    def pixel(x, y, color):
        offset = (int(y) * width + int(x)) * 3
        data[offset:offset + 3] = bytes(color)
    def line(x0, y0, x1, y1, color):
        steps = int(max(abs(x1 - x0), abs(y1 - y0))) + 1
        for step in range(steps + 1):
            pixel(round(x0 + (x1 - x0) * step / steps), round(y0 + (y1 - y0) * step / steps), color)
    for zero in (100, 300):
        line(70, zero, 580, zero, (67, 83, 107))
        for amplitude in (1, 0, -1):
            center = zero - 24 * amplitude + (8 if fault == 'labels' else 0)
            for y in range(center - 4, center + 4):
                for x in range(30, 38):pixel(x, y, (174, 189, 206))
        for y in range(zero + 40, zero + 46):
            for x in (*range(71, 111), *range(540, 581)):pixel(x, y, (174, 189, 206))
    def point(lane, fraction, amplitude):
        x = 70 + fraction * 510
        if fault == 'contained':x = 325 + (x - 325) * .65
        if fault == 'horizontal':x += 4
        y = (100, 300)[lane] - 24 * amplitude + (8 if fault == 'vertical' else 0)
        return x, y
    def trace(lane, x0, a0, x1, a1):
        line(*point(lane, x0, a0), *point(lane, x1, a1), (119, 212, 239))
    if scene == 'anchors':
        for amplitude in (1, .5, -.5, -1):trace(0, 0, amplitude, 1, amplitude)
        for fraction in (0, .25, .75, 1):trace(1, fraction, 1, fraction, -1)
    elif scene == 'asymmetric':
        for band, (minimum, maximum) in enumerate(((.25, .75), (-.75, -.25), (-.5, 1), (0, .5))):
            for index in range(16):
                fraction = (band * 16 + index + .5) / 64
                trace(0, fraction, minimum, fraction, maximum)
                trace(1, fraction, 0, fraction, 0)
        if fault == 'silent-ear':trace(1, .5, .5, .5, .5)
    else:
        for lane, first, last in ((0, .5, -.5), (1, -.5, .5)):
            trace(lane, 0, first, .375, first)
            trace(lane, .625, last, 1, last)
            if fault == 'bridge':trace(lane, .375, first, .625, last)
    return width, height, data


class SignalPixelTests(unittest.TestCase):
    def test_known_axes_asymmetric_extrema_silent_ear_and_gap_are_accepted(self):
        clean = signal_pixels.references(*raster("anchors"), 1)[0]
        for scene in signal_pixels.SCENES:
            signal_pixels.check_capture(scene, *raster(scene), 1, clean if scene != "anchors" else None)

    def test_contained_horizontal_extent_and_shifted_amplitude_axes_reject(self):
        for fault in ('contained', 'horizontal', 'vertical', 'labels'):
            with self.assertRaises(ValueError):
                signal_pixels.check_capture('anchors', *raster('anchors', fault), 1)

    def test_a_bridged_gap_and_leaking_silent_ear_reject(self):
        for scene, fault in (('gap', 'bridge'), ('asymmetric', 'silent-ear')):
            with self.assertRaises(ValueError):
                signal_pixels.check_capture(scene, *raster(scene, fault), 1, signal_pixels.references(*raster("anchors"), 1)[0])
