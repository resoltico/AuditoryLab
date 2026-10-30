# Onboarding artwork source and exports

The selected **Spectral ribbon / A spectrum of rhythms** and five matched individual sine illustrations are editable source artwork, not screenshots of app state. Their representative rates are 2, 6, 10, 20 and 32 cycles/s. Each illustration covers one second, starts at zero phase and uses normalized amplitude. Actual audio pulse envelopes remain separate and must retain their correct rectangular/shaped forms.

## Ownership and editing inputs

| Path | Responsibility and license |
| --- | --- |
| `generate_art.py` | MPL-2.0 software: template expansion, bounded sine geometry, glyph placement and output checks. |
| `source/*.svg.in` | Proprietary preferred editing masters: composition, lettering placements, labels and original design. |
| `source/lettering.json` | Proprietary original geometric glyph outlines. No external font assets. |
| `source/palette.json` | Proprietary paired light/dark palettes and appearance parameters. |
| `source/layout.json` | Proprietary waveform-slot coordinates and label placement for the fixed masters. Both stroke and filled paths derive from the same slot. |
| `../../crates/app/src/learning/band_examples.json` | MPL-covered application learning data: one factual representative-rate definition used by both overview and details. Core `Band` remains the owner of band membership and interval semantics. |
| `assets/*.svg` | Proprietary generated runtime artwork. Two overview files and ten individual files. |
| `reference/` | Retained prior art directions and art-bearing original generators are proprietary; identifiable preview software remains MPL-covered as its scope notice explains. Historical/reference only. |

The root LICENSE identifies proprietary artwork scope and permissions. It permits unchanged art to accompany Auditory Lab and copying source for builds, like the icon; modification or independent reuse requires the owner's permission. The MPL exporter grants no rights to the separate artwork. Scientific facts, standard mathematical definitions and legally required rights are not claimed exclusively. Existing icon terms and previously granted rights are unchanged.

The preserved original `reference/rhythm-spectrum-studies/draw_spectra.py` mixes creative glyph/layout data with exporting code and remains historical proprietary drawing source, like the icon's source generator. It was not silently given an MPL header. The new, separated generator has the requested MPL header. Earlier `reference/rhythm-context-options/` SVGs used ordinary installed font references and their PNGs reflect those fonts; they are not falsely described as font-independent original-path lettering. No font file, stock icon or supplied Gemini JPEG is included. The selected canonical artwork uses original glyph paths only.

## Regenerate and inspect

Requires Python 3.9+ standard library only. The generation tool is optional for ordinary app builds; those consume tracked SVG exports and must not acquire Python, browser or vectorcraft dependencies.

From the repository root:

```sh
python3 artwork/onboarding/generate_art.py target/onboarding-art-review
```

Outside a complete checkout, supply the factual examples file explicitly:

```sh
python3 generate_art.py OUTPUT --examples PATH_TO_BAND_EXAMPLES_JSON
```

Compare the regenerated SVG set with `artwork/onboarding/assets/` and review visual and semantic differences before replacing tracked exports. The exporter rejects output paths inside/above its source tree. It writes only the named generated files, does not delete unrelated files, and never overwrites its masters or the app icon. Pre-existing symbolic-link and nonregular output entries are rejected before any export is written. Each SVG is written to a fresh sibling temporary file and atomically replaces its directory entry; hard-linked source inodes are never truncated. Existing regular-output permissions are retained, while new files use mode 0644; temporary files are removed on failure. Publication is atomic per SVG, not a transaction across all 12 files. Input types, missing glyphs/references, nonfinite values, external assets and excessive sampling work fail actionably.

The SVG master supplies the creative composition; its wave tokens are expanded from `layout.json` and shared example rates. Filled wave paths close to the **same current baseline endpoints**, not copied coordinates. Editing palettes regenerates both appearances coherently. Editing a representative rate updates the overview path, individual path and individual cycles/s label; native application consistency tests must also verify core membership and displayed intervals. The rendering-work cap is not a second EEG-band classifier.

The independent `verify_art.py` check exercises actual exported paths, independent sine values, zero crossings/quarter cycles, a retired-envelope negative control, moved-fill geometry and malformed input controls. PNGs in reference are review snapshots; no cross-platform bit-identical PNG claim is made. The handoff also records Python 3.9/3.12 generation checks; they do not qualify later edits. Compare outputs on other toolchains: floating-point/libm or rasterizer differences require inspection rather than blind snapshot updates.

Run the independent repository checks after generation/source edits:

```sh
python3 artwork/onboarding/verify_art.py --scratch target/onboarding-art-qa
```

This verifies the entire checksum inventory (including unexpected files), all 12 exported files, analytic sine/quarter-cycle values, theme geometry equality, rejection of the retired pulse envelope, modified rate/slot controls, missing/external references, invalid numerical input and source overwrite refusal, hard-linked source preservation and legitimate repeat regeneration with preserved output permissions. It reads the actual repository sources and shared app rate file. It does not establish native SVG decoding or app behavior.

## Production integration requirements

Select the light/dark bundled asset using the resolved native application palette, not SVG browser media queries or a network request. Native accessible text must convey the five names, exact core intervals, chosen rates and qualified contexts: path lettering inside an image is not a substitute for native text accessibility. Narrow/large-text layouts need readable text plus a deliberate enlarged/compact graphic view; avoiding overflow alone is insufficient.

The checksum inventory includes all intended source/runtime/reference files. Source distributions retain these editing inputs and historical references. Runtime UI references only `assets/*.svg`; reference art is not embedded. Existing About/legal embedding and package equality checks consume the root LICENSE. Native resource and platform-package checks remain separate from this mathematical verifier.

The art-bearing `reference/prepare-onboarding-artwork.py` preparation source is retained for complete construction history; canonical preferred masters and the separated exporter are the active editing/rebuild path. Historical environment-specific scripts are not app build dependencies. `reference/retained-art-inventory.json` retains their original-path/handoff-path provenance and hashes; remove the `implementation-materials/artwork/onboarding/` prefix from those historical package paths to locate their current repository copies. The independent verifier checks those retained original bytes as well as the current complete manifest.

The supplied `SHA256SUMS` inventories every intended file in this artwork tree except itself. Verify it with `(cd artwork/onboarding && shasum -a 256 -c SHA256SUMS)` and compare its path set to the actual tree to detect unexpected additions. Update it only after reviewing intentional source/export changes.
