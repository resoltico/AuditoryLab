# Onboarding artwork

**Spectral ribbon / A spectrum of rhythms** and five individual band illustrations share representative rates of 2, 6, 10, 20 and 32 cycles/s. Each overview section and individual illustration covers one second, starts at zero phase and uses normalized signed amplitude. These are teaching schematics, distinct from recorded EEG, rendered audio and actual pulse envelopes.

## Editing inputs and ownership

| Path | Responsibility |
| --- | --- |
| `source/*.svg.in` | Preferred composition and lettering-placement masters. |
| `source/lettering.json` | Original geometric glyph paths; standalone figures need no external font. |
| `source/palette.json` | Paired light/dark foreground palettes for transparent figures. |
| `source/layout.json` | Waveform slots, shared stroke/fill endpoints and dynamic label placement. |
| `../../crates/app/src/learning/band_examples.json` | Application-owned representative rates consumed by the app and exporter. Core `Band` owns membership and interval semantics. |
| `generate_art.py` | Bounded sine geometry, template expansion, glyph placement and output publication. |
| `assets/*.svg` | Twelve standalone exports: two overviews and ten individual diagrams. |
| `reference/` | Drawing inputs, reference compositions, generators and design previews; not runtime assets. |
| `verify_art.py` | Independent geometry, edit, rejection, source-preservation and inventory checks. |

The [root LICENSE](../../LICENSE) defines proprietary scope for creative masters, glyph/layout/palette data, generated images and identified reference drawing source. Exporter/verifier software and identified preview/build software use MPL-2.0. Copying artwork source for builds and distributing unchanged artwork with Auditory Lab are permitted under those terms; modifying or independently reusing artwork requires the owner’s permission. Standard mathematics and scientific facts are not claimed exclusively.

The masters use original geometric path lettering without imported image or font assets. Uniform strokes, minimum glyph sizes and contrasting foregrounds define the standalone lettering. Figures have transparent backgrounds; pair light artwork with a light neutral surface and dark artwork with a dark neutral surface. `reference/rhythm-context-options/` contains font-referencing SVG designs and corresponding rendered PNGs. `reference/rhythm-spectrum-studies/` contains path-lettered reference compositions. Art-bearing reference scripts mix creative drawing data with exporting logic and have the proprietary scope identified by LICENSE. The separated exporter does not embed their creative payloads in MPL software.

## Generate and review exports

Python 3.9+ standard library is sufficient. Ordinary Rust builds do not run Python or depend on browser/vector-authoring tools. The desktop renders its learning schematics and labels natively; the SVG exports are independently editable figures.

From the repository root:

```sh
python3 artwork/onboarding/generate_art.py target/onboarding-art-review
```

For a standalone source tree, supply its application data explicitly:

```sh
python3 generate_art.py OUTPUT --examples PATH_TO_BAND_EXAMPLES_JSON
```

Compare all generated files with `assets/` and review their meaning and appearance before updating tracked exports. SVG masters own composition; layout slots and representative rates own generated wave geometry and rate labels. Stroke and fill close at the same slot endpoints. Palette edits produce both appearances. A representative-rate edit changes both overview/detail curves and the individual caption; compiled checks compare rates and range lettering with core.

Output must be outside the artwork source tree and cannot be an ancestor of it. Existing symbolic-link and nonregular destination entries are rejected before publication. Each SVG is written to a fresh sibling temporary file and atomically replaces its directory entry, preserving hard-linked source inodes. Existing regular-output permissions are retained; new files use mode 0644. Temporary files are cleaned on failure. Publication is atomic per SVG, not across the twelve-file set. The exporter writes only its named outputs and does not delete unrelated files.

Invalid numerical types, nonfinite/nonpositive rates, excessive sampling work, missing glyphs and external references produce errors. The sampling bound is an authoring resource limit, not an EEG-band classifier.

## Verification and inventories

```sh
python3 artwork/onboarding/verify_art.py --scratch target/onboarding-art-qa
```

The verifier checks actual path coordinates against analytic sine/quarter-cycle expectations, matching theme geometry, transparent/no-glow compositions, lettering weight/size/bounds, pulse-envelope rejection controls, rate/slot/fill edits, malformed inputs, source aliases, repeated regeneration and output modes. It also checks the complete current manifest and immutable reference-source records. Mathematical checks do not establish native decoding, accessibility or platform-package qualification.

`SHA256SUMS` covers every current file in this tree except itself, including maintained Markdown notes. `reference/retained-art-inventory.json` identifies immutable reference artwork, drawing generators and their source data; maintained documentation and checksum indexes are covered by the current-tree manifest instead. Review intentional changes before refreshing inventories. Floating-point libraries and rasterizers can affect exported bytes; inspect differences rather than blindly updating snapshots. Reference PNGs are appearance examples, not a cross-platform raster reproducibility contract.

## Desktop integration

The desktop presents the same one-second normalized teaching curves using application-owned example rates, with native scalable band names, core intervals, representative rates and qualified contexts. It does not shrink path lettering into a reading screen. Native surfaces and palette-aware curve colors handle appearance changes. The overview and individual diagrams retain the same zero-phase sinusoidal meaning; real compiled audio/envelope plots retain their actual pulse shapes.

The SVGs in `assets/` are standalone authoring/export material; reference compositions and mockups remain source-only. Root LICENSE is embedded in source-built desktops and copied into native packages. [Packaging](../../docs/releasing.md) defines resource, identity, legal-equality and extracted-package checks.
