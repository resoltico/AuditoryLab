#!/usr/bin/env python3
# Copyright 2026 Ervins Strauhmanis
# SPDX-License-Identifier: MPL-2.0
"""Independent pixel oracle for the explicit production-UI geometry fixture."""
import argparse
import json
import math
from pathlib import Path

SIZES = ((600, 640), (900, 800), (1200, 900))
SCENES = ("anchors", "asymmetric", "gap")


def groups(values, step=1):
    result = []
    for value in values:
        if not result or value > result[-1][-1] + step:
            result.append([value])
        else:
            result[-1].append(value)
    return result


def ppm(path):
    magic, dimensions, maximum, data = path.read_bytes().split(b"\n", 3)
    width, height = map(int, dimensions.split())
    if magic != b"P6" or maximum != b"255" or not (0 < width <= 2500 and 0 < height <= 2000) or len(data) != width * height * 3:
        raise ValueError("invalid bounded native PPM capture")
    return width, height, data


def classify(width, height, data):
    axes, trace, labels = {}, [], []
    for y in range(height):
        row = []
        for x in range(width):
            offset = (y * width + x) * 3
            red, green, blue = data[offset], data[offset + 1], data[offset + 2]
            # Alpha-tolerant matches against actual dark-lane palette. No OCR,
            # font-specific golden image, or screenshot-derived expected trace.
            r, g, b = red - 19, green - 29, blue - 43
            # Cyan dominance also accepts antialiased trace over the gray zero
            # reference, rather than assuming every trace blends over dark fill.
            if red <= 125 and 50 <= green <= 218 and blue <= 245 and 22 <= green - red <= 105 and 8 <= blue - green <= 40:
                trace.append((x + .5, y + .5))
            elif 25 <= red <= 71 and 36 <= green <= 88 and 49 <= blue <= 112 and abs(54 * r - 48 * g) <= 54 * 4 and abs(54 * b - 64 * g) <= 54 * 4:
                row.append(x)
            elif 85 <= red <= 180 and 100 <= green <= 200 and 115 <= blue <= 220 and abs(160 * r - 155 * g) <= 160 * 10 and abs(160 * b - 163 * g) <= 160 * 10:
                labels.append((x + .5, y + .5))
        if len(row) > width * .55:
            axes[y] = row
    return axes, trace, labels


def references(width, height, data, scale, clean_reference=None):
    axes, trace, labels = classify(width, height, data)
    lines = groups(sorted(axes))
    if len(lines) != 2:
        raise ValueError(f"expected two independent full-width zero references, got {len(lines)}")
    plots = []
    for ordinal, line in enumerate(lines):
        zero = sum(y + .5 for y in line) / len(line)
        if clean_reference is None:
            # Anchor scenes leave long clean axis segments. Reject short tick
            # and label ink runs instead of folding them into source bounds.
            runs = groups(sorted({x for y in line for x in axes[y]}))
            runs = [run for run in runs if run[-1] - run[0] > width * .1]
            if not runs:
                raise ValueError("zero reference has no long source-span segment")
            left, right = min(run[0] for run in runs) + .5, max(run[-1] for run in runs) + .5
        else:
            left, right = clean_reference[ordinal][:2]
            # Other scenes can cover the reference with real zero-amplitude
            # samples; reuse the clean anchor's bounds at this exact size/scale.
            if abs(zero - clean_reference[ordinal][4]) > 2 * scale:
                raise ValueError("scene data unexpectedly moved its reference axis")
        rows = sorted({int(y - .5) for x, y in labels if left - 52 * scale <= x <= left - 2 * scale and abs(y - zero) < 38 * scale})
        ticks = [sum(y + .5 for y in group) / len(group) for group in groups(rows)]
        above = [y for y in ticks if y < zero - 3 * scale]
        below = [y for y in ticks if y > zero + 3 * scale]
        if not above or not below:
            raise ValueError("amplitude labels do not straddle the zero reference")
        top, bottom = max(above), min(below)
        if abs((top + bottom) / 2 - zero) > 3 * scale or bottom - top < 30 * scale:
            raise ValueError(f"amplitude labels are not centered on common ±1/0 axes: {top}, {zero}, {bottom}")
        plots.append((left, right, top, bottom, zero))
    # Both ears represent the same source extent. The anchor's edge markers can
    # overpaint a reference endpoint while its peer exposes a short tick there;
    # their shared clean intersection removes both without using trace bounds.
    common_left = max(plot[0] for plot in plots)
    common_right = min(plot[1] for plot in plots)
    if common_right - common_left < min(plot[1] - plot[0] for plot in plots) - 10 * scale:
        raise ValueError("per-ear source reference extents disagree")
    plots = [(common_left, common_right, top, bottom, zero) for _, _, top, bottom, zero in plots]
    # The two actual frame/time endpoint labels must share the same horizontal
    # source extent. Their text values are supplied by the explicit fixture.
    for left, right, top, bottom, zero in plots:
        ink = [(x, y) for x, y in labels if left - 6 * scale <= x <= right + 6 * scale and bottom + 5 * scale <= y <= bottom + 38 * scale]
        first = [x for x, _ in ink if x < left + (right - left) / 3]
        last = [x for x, _ in ink if x > right - (right - left) / 3]
        if len(first) < 10 or len(last) < 10 or abs(min(first) - left) > 3 * scale or abs(max(last) - right) > 3 * scale:
            raise ValueError("source endpoint label ink does not share the plotted horizontal extent")
    return plots, trace


def check_capture(scene, width, height, data, scale, clean_reference=None):
    plots, points = references(width, height, data, scale, clean_reference)
    xtol, ytol = 2 * scale, 2.5 * scale
    def at(plot, fraction, amplitude):
        left, right, top, bottom, _ = plot
        return left + fraction * (right - left), top + (1 - amplitude) * (bottom - top) / 2
    def present(plot, fraction, amplitude):
        x, y = at(plot, fraction, amplitude)
        if not any(abs(px - x) <= xtol and abs(py - y) <= ytol for px, py in points):
            raise ValueError(f"{scene}: trace missing at independent source/amplitude coordinate {fraction}, {amplitude}")
    if scene == "anchors":
        for amplitude in (1, .5, -.5, -1):
            for fraction in (0, .5, 1):
                present(plots[0], fraction, amplitude)
        for fraction in (0, .25, .75, 1):
            for amplitude in (.8, 0, -.8):
                present(plots[1], fraction, amplitude)
    elif scene == "asymmetric":
        for band, (minimum, maximum) in enumerate(((.25, .75), (-.75, -.25), (-.5, 1), (0, .5))):
            for index in (2, 8, 14):
                fraction = (band * 16 + index + .5) / 64
                present(plots[0], fraction, minimum)
                present(plots[0], fraction, maximum)
        left, right, top, bottom, zero = plots[1]
        silent = [(x, y) for x, y in points if left <= x <= right and top - ytol <= y <= bottom + ytol]
        if len(silent) < 8 or any(abs(y - zero) > ytol for _, y in silent):
            raise ValueError("the independently silent right ear is absent or shows nonzero amplitude")
    elif scene == "gap":
        for plot, first, last in ((plots[0], .5, -.5), (plots[1], -.5, .5)):
            present(plot, .2, first)
            present(plot, .8, last)
            left, right, top, bottom, _ = plot
            if any(left + .43 * (right - left) <= x <= left + .57 * (right - left) and top <= y <= bottom for x, y in points):
                raise ValueError("a trace bridges the explicitly missing central interval")
    else:
        raise ValueError("unknown geometry scene")
    return {"plots": plots, "trace_pixels": len(points)}


def capture_path(directory, case):
    width, height = case["requested_logical"]
    filename = f'{case["scene"]}-{width}x{height}.ppm'
    path = directory / filename
    if case["file"] != filename or path.is_symlink():
        raise ValueError("capture path differs from fixed fixture vocabulary")
    return path


def verify(directory):
    manifest = json.loads((directory / "manifest.json").read_text(encoding="utf-8"))
    if manifest.get("format_version") != 1 or manifest.get("producer") != "production MainWindow/SignalLane via include_modules":
        raise ValueError("unrecognized native geometry producer")
    cases = manifest.get("cases", [])
    expected = {(scene, size) for scene in SCENES for size in SIZES}
    actual = [(case.get("scene"), tuple(case.get("requested_logical", []))) for case in cases]
    if len(actual) != 9 or set(actual) != expected:
        raise ValueError("native geometry case matrix is missing, repeated or unexpected")
    results = []
    clean = {}
    # Establish independent axis bounds from anchors before data can overpaint
    # them. No trace pixel is used to calculate the expected coordinate system.
    for case in cases:
        if case["scene"] == "anchors":
            width, height, data = ppm(capture_path(directory, case))
            clean[tuple(case["requested_logical"])] = references(width, height, data, case["scale_factor"])[0]
    for case in cases:
        scene, (logical_width, logical_height) = case["scene"], case["requested_logical"]
        width, height, data = ppm(capture_path(directory, case))
        if case["snapshot_physical"] != [width, height] or case["window_physical"] != [width, height]:
            raise ValueError("actual native dimensions disagree with capture metadata")
        scale = case["scale_factor"]
        if not math.isfinite(scale) or min(abs(scale - target) for target in (1, 1.25)) > .001:
            raise ValueError("unexpected native scale factor")
        if abs(width - logical_width * scale) > 2 or abs(height - logical_height * scale) > 2:
            raise ValueError("capture was clamped or did not exercise requested logical size")
        result = check_capture(scene, width, height, data, scale, clean[tuple(case["requested_logical"])] if scene != "anchors" else None)
        results.append({"scene": scene, "logical": [logical_width, logical_height], "physical": [width, height], "scale": scale, **result})
    record = {"status": "passed", "cases": results, "scope": "actual production-UI pixel geometry; sentinels are not playback data; no physical-adapter or audio claim"}
    (directory / "pixels.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    verify(args.directory)
    print("NATIVE_SIGNAL_GEOMETRY_PIXELS_OK cases=9")


if __name__ == "__main__":
    main()
