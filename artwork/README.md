# Application artwork

Auditory Lab’s “Woven” icon shows two sine waves woven across each other. Project-owned artwork, editable drawing source, generated images and software use [AGPL-3.0-only](../LICENSE); see [project terms](../PROJECT-NOTICE.md). Third-party components retain their own terms. No trademark rights are granted.

## Sources and attribution

[`auditorylab-icon/source/make_icon.py`](auditorylab-icon/source/make_icon.py) and its [palette](auditorylab-icon/source/palette.json) define the drawing. [Icon geometry and use](auditorylab-icon/GEOMETRY.md) describes the drawing inputs and their limits. Editable Icon Composer layers, platform exports and a preview are retained beside them. The [asset guide](auditorylab-icon/README.md) describes the files and construction tools. Its checksum manifest checks the listed current files; hashes do not certify originality or ownership.

## Onboarding illustrations

[Onboarding artwork](onboarding/README.md) supplies **Spectral ribbon / A spectrum of rhythms**, matched sinusoidal band illustrations and their SVG masters, glyphs, palettes and layout inputs. Drawing generators, reference compositions and design mockups are available under `onboarding/reference/`. The exporter, verifier, preview/build software, creative inputs, generated images and art-bearing source use AGPL-3.0-only.

The shared application learning data defines representative rates; core owns band intervals. Illustrations are normalized one-second sine examples, distinct from real pulse-amplitude envelopes. Regeneration and independent semantic checks are optional authoring operations. The native guide uses application-owned example rates and scalable native labels; the transparent SVG figures remain editable standalone exports.

## Runtime and packaging

Slint embeds the runtime PNG in the main and About windows. Windows builds embed the ICO into the desktop executable. Linux packages include the desktop Icon identifier and its hicolor exports. macOS packaging compiles the editable `AppIcon.icon` with the selected Xcode `actool`, producing icon resources before signing; the separate flat ICNS is a reference export, not a packaging fallback. The desktop renders the onboarding schematics with native labels and palette-aware paths; standalone SVGs and reference compositions remain authoring material. CLI downloads contain no unused GUI images.

See [packaging prerequisites and resource checks](../docs/releasing.md#formats-and-installed-resources). Artwork construction tools are optional for application builds, which use the tracked assets; native macOS packaging still compiles the editable icon package.
