# System-font title rendering

Exact sctk-adwaita 0.10.1 registry source is retained, including every original feature, source, bundled font and legal file. Only Cargo.toml changes: optional crossfont 0.8 becomes 0.9 so its Fontconfig links producer matches Slint 1.18.1. Winit selects its supported crossfont CSD route, preserving title text and controls. No source algorithm changes.

Supported Linux root builds use Cargo links overrides for system FreeType; freetype-sys build.rs and its bundled native fallback must not execute. Security-updated system FreeType/Fontconfig are prerequisites. The original inactive ab_glyph/Cantarell source remains intact; the font copyright and full SIL OFL terms are retained in scripts/licenses/sctk-adwaita-font-notices.txt and generated notices.

The full upstream MIT grant remains LICENSE. auditorylab-provenance.json identifies every original file and exact producer; auditorylab.patch identifies the sole manifest delta. Native Linux linking and actual Wayland CSD behavior require separate qualification; source review and Cargo audit do not establish them.
