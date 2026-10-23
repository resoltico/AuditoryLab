# Statement of originality: Auditory Lab application icon

**Work:** the Auditory Lab application icon "Woven". Two sine waves of slightly different frequency, one woven over the other, with a clear gap where they cross. It has a light (main) palette and a dark companion palette.
**Author of the drawing:** Claude (Anthropic AI model), working to the art direction and selections of Ervins Strauhmanis
**Date:** 21 October 2026

## How it was made

Ervins chose the design in rounds:
1. Five original concepts; he picked "Binaural braid".
2. Five legibility variations; he picked "Woven".
3. Five accessible palettes; he picked "Daylight" as the main palette, and asked for a matching dark companion.

The artwork is defined entirely by `source/make_icon.py` and `source/palette.json`:
- **Front wave:** y = 512 − 180·sin(2π·1.5·t)
- **Back wave:** y = 512 + 180·sin(2π·2.0·t)
- **Both waves:** x runs from 256 to 768 for t from 0 to 1, with a stroke thickness of 56
- **Crossing gap:** 14 units are cut from the back wave either side of the front wave
- **Tile:** 824 × 824, corner radius 185, on a 1024 canvas
- **Colours:** listed in `palette.json`

Running the script reproduces every SVG, PNG, ICO, ICNS and Icon Composer layer in this package.

## Statement

I drew this icon independently, from the formulas and numbers above, which I wrote for this project.

It was **not** traced from, adapted from, reproduced from memory of, or derived from any of the following:
- Apple SF Symbols or any other Apple artwork
- Microsoft Fluent / Segoe icons
- Google Material Symbols
- Feather, Lucide, Font Awesome or Heroicons
- any other icon set, stock image, logo or third-party artwork

No external image, font or vector file was downloaded or used to create it.

## Limits of this statement

- **Generic motifs:** sine waves and rounded-square app tiles are generic motifs that anyone may use, and many icons show waves. What is specific to this work is the particular composition: two waves at 1.5 and 2 cycles, the over-and-under weave with cut crossings, the proportions and the palette.
- **Not legal certification:** this records how the work was made. It is not a legal certification of copyright ownership, non-infringement or trademark clearance, and it is not legal advice.
- **AI authorship:** copyright protection for AI-generated material is limited or uncertain in many jurisdictions, including the EU and the US. The owner's documented creative direction and selection supports a claim, but cannot guarantee one. For exclusive rights in the icon as a brand mark, registering a trademark (for example as an EU trade mark) is the dependable route.
- **Licence of the surrounding project:** Auditory Lab's code is licensed under MPL 2.0. Unless separate artwork terms are added (see `ARTWORK-LICENSE.md`), committing the icon to the repository could be read as releasing it under MPL 2.0 as well.
