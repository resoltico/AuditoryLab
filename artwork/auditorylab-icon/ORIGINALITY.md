# Artwork attribution and geometry

The Auditory Lab “Woven” icon is supplied as coordinate-defined artwork by Claude (Anthropic), under Ervins Strauhmanis's art direction and selection. The supplied authorship account states that the drawing is independently constructed without tracing, importing or adapting external artwork, icon libraries, fonts or system symbols. This records that account, not independent certification of originality, ownership or trademark clearance.

[`source/make_icon.py`](source/make_icon.py) and [`source/palette.json`](source/palette.json) define the drawing:

- Front wave: `y = 512 − 180 × sin(2π × 1.5 × t)`.
- Back wave: `y = 512 + 180 × sin(2π × 2 × t)`.
- Both waves span `x = 256..768` for `t = 0..1`, with stroke thickness 56 and a 14-unit crossing gap around the front wave.
- Flat exports use an 824 × 824 tile with corner radius 185 on a 1024 canvas. Icon Composer foreground layers scale the drawing by `1024 / 824`; system tooling supplies the enclosure.
- The palette file specifies light/dark colors and tinted back-wave opacity.

Sine waves and rounded app tiles are generic motifs. Attribution and checksums do not establish exclusive rights. The governing software/artwork permissions are in the [root LICENSE](../../LICENSE).
