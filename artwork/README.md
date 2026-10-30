# Application artwork

Auditory Lab's “Woven” icon shows two sine waves woven across each other. The [root LICENSE](../LICENSE) governs the artwork, its drawing source and generated images separately from MPL-covered software. Unchanged art may accompany original or modified Auditory Lab source/binary distributions. Copying the drawing source for builds is permitted; modifying the art or reusing it independently requires permission. Normal application, preview and build code remains MPL-covered.

## Sources and attribution

[`auditorylab-icon/source/make_icon.py`](auditorylab-icon/source/make_icon.py) and its [palette](auditorylab-icon/source/palette.json) define the drawing. [Icon geometry and use](auditorylab-icon/GEOMETRY.md) describes the drawing inputs and their limits. Editable Icon Composer layers, platform exports and a preview are retained beside them. The [asset guide](auditorylab-icon/README.md) describes the files and construction tools. Its checksum manifest checks the listed current files; hashes do not certify originality or ownership.

## Onboarding illustrations

[Onboarding artwork](onboarding/README.md) retains the selected **Spectral ribbon / A spectrum of rhythms**, matched sinusoidal band examples, editable masters, palette/layout/glyph sources and original generators and prior concepts. The separated Python exporter and verification software are MPL-covered; creative inputs, generated images and historical art-bearing sources use the root proprietary artwork terms. The root LICENSE names the proprietary reference payloads and explicitly retains MPL for mixed-mockup UI software and the software-only preview/build helpers; reference documentation and provenance records remain MPL-covered. The existing icon is unchanged.

The shared application learning data defines representative rates; core owns band intervals. Illustrations are normalized one-second sine examples, distinct from real pulse-amplitude envelopes. Regeneration and independent semantic checks are optional authoring operations; Rust builds use tracked exports.

## Runtime and packaging

Slint embeds the runtime PNG in the main and About windows. Windows builds embed the ICO into the desktop executable. Linux packages include the desktop Icon identifier and its hicolor exports. macOS packaging compiles the editable `AppIcon.icon` with the selected Xcode `actool`, producing icon resources before signing; the separate flat ICNS is a reference export, not a packaging fallback. The desktop embeds the selected onboarding SVG pairs; unselected references remain source-only. CLI downloads contain no unused GUI images.

See [packaging prerequisites and resource checks](../docs/releasing.md#formats-and-installed-resources). Artwork construction tools are optional for application builds, which use the tracked assets; native macOS packaging still compiles the editable icon package.
