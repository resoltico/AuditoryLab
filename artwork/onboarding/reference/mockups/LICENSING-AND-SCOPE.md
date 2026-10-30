# Design references and software scope

Sound and storage interactions in the HTML design previews are simulated. The previews are not application implementations or a source of sound defaults, catalog data, save policy or envelope formulas. Production behavior belongs to the application services and core model.

The four mixed HTML files contain MPL-covered UI software and separate proprietary embedded artwork payloads. A software notice on an HTML file does not grant MPL rights to its embedded artwork. The [root LICENSE](../../../../LICENSE) names the exact files and payload boundary.

`portable-preview.html` is MPL-covered preview software referencing separate SVG assets. `incorporate-spectrum.py` and `prepare-reviewed-preview.py` are MPL-covered preview-building helpers. Their environment-specific inputs are not application build dependencies. Documentation, provenance metadata and verification records use MPL-2.0.

The reference drawing generators, creative drawing data, SVGs and PNGs have the proprietary scope identified by LICENSE. The active editing path is the separated [exporter and masters](../../README.md). Application builds do not execute the reference scripts or design previews.
