# Retained mockups and helpers

These are preserved design/source references, not production application code. Historical files retain their recorded bytes and hashes.

The HTML mockups combine preview UI software with embedded proprietary SVG data. The UI/preview software is MPL-covered; embedded original artwork retains LicenseRef-Proprietary-Artwork terms under the owner's instruction. An HTML file containing a base64 illustration is not a blanket MPL grant to that illustration. Do not paste the mixed file into MPL production source or treat the prototype's local saves, recipes, parameter limits or approximate previews as application policy.

The software-only `incorporate-spectrum.py` helper reads the art rather than defining its glyph shapes; its preview-building logic is MPL-covered. Its historical environment-specific paths are not a production build dependency. The retained `portable-preview.html` is MPL-noticed software referencing separate proprietary SVGs. Its paths are adjusted for this repository. Historical mockups retained here have fake storage/audio and are reference only.

The actual original art-bearing generators, original glyph source, palettes, SVGs and PNGs are retained beside these mockups under `artwork/onboarding/reference/`. The root LICENSE explicitly identifies the SVG/PNG artwork, creative drawing data, original art-bearing generators and embedded artwork payloads covered by the proprietary terms, with the same unchanged-art distribution permission and retained limitations as the existing icon. It does not exclude this whole reference directory from MPL. The new separated `generate_art.py` is MPL-2.0 software; its creative inputs and generated artwork are separate files.

The supplied Gemini reference JPEG is not generated/owned artwork from this task and is excluded. It was used as visual direction only, without imported pixels or traced humans/icons.

The root LICENSE explicitly retains MPL coverage for the UI software portions of the four mixed HTML mockups, and for `portable-preview.html`, `incorporate-spectrum.py` and `prepare-reviewed-preview.py`. Documentation and provenance/verification records also remain MPL-covered. Current native production code does not execute these files.
