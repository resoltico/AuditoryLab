# Icon geometry and use

The Woven icon is defined by explicit coordinates, sine formulas and the tracked palette. Its drawing uses no external artwork, icon libraries, fonts or system symbols. The [root LICENSE](../../LICENSE) governs software and artwork permissions.

[`source/make_icon.py`](source/make_icon.py) and [`source/palette.json`](source/palette.json) define the drawing:

- Front wave: `y = 512 − 180 × sin(2π × 1.5 × t)`.
- Back wave: `y = 512 + 180 × sin(2π × 2 × t)`.
- Both waves span `x = 256..768` for `t = 0..1`, with stroke thickness 56 and a 14-unit crossing gap around the front wave.
- Flat exports use an 824 × 824 tile with corner radius 185 on a 1024 canvas. Icon Composer foreground layers scale the drawing by `1024 / 824`; system tooling supplies the enclosure.
- The palette file specifies light/dark colors and tinted back-wave opacity.

Sine waves and rounded app tiles are generic motifs. The geometry and checksum manifest do not establish exclusive rights.
