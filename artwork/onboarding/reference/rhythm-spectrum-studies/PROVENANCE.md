# Spectrum reference compositions

This directory contains the `draw_spectra.py` drawing source and paired light/dark SVG/PNG reference compositions: Luminous spectrum, Spectral ribbon and Wave medallions. Runtime exports use the [active masters and exporter](../../README.md); these reference files are not embedded by the application.

The generator defines geometry, wave paths, gradients, composition and geometric glyph outlines. SVGs use internal paths rather than external font or image assets. Their metadata supports source inspection; native accessibility relies on the application’s separate text equivalents.

The compositions use the application’s reference-band convention: Delta 0.1–<4, Theta 4–<8, Alpha 8–13, Beta >13–30 and Gamma >30–80 Hz. Context associations are selected observations, not a causal sound-to-state map or an alertness/intelligence ranking. The spectrum compositions show equal-amplitude schematic waves over one second per section; medallions use half-second windows. These drawings do not compare measured EEG amplitudes.

Scientific context sources are the [IFCN glossary](https://pmc.ncbi.nlm.nih.gov/articles/PMC6123891/), [human memory theta/gamma MEG study](https://pmc.ncbi.nlm.nih.gov/articles/PMC6674196/) and [NIH sleep-stage overview](https://www.nhlbi.nih.gov/health/sleep/stages-of-sleep). The [root LICENSE](../../../../LICENSE) governs creative artwork and art-bearing source; it makes no guarantee of exclusive rights or copyrightability. This maintained note is covered by the current artwork manifest.
