# Security

Report suspected vulnerabilities through [GitHub private vulnerability reporting](https://github.com/resoltico/AuditoryLab/security/advisories/new). Include the affected version, operating system, minimal input and reproducible steps. Keep credentials and private data out of reports. Use public issues for ordinary defects and questions. Security fixes target the current public release and may require upgrading.

## Recipe input

The shared parser bounds input size, requires UTF-8, checks the schema version and rejects unknown fields. Signal validation bounds duration, component count, rates, pulse widths and per-ear peak sums. File loading checks the opened handle and admits only regular files, including symlinks to them. Unix opens are nonblocking so FIFOs can be rejected before reading. Intentional streams use the bounded reader or CLI `--stdin`.

Recipe metadata is descriptive text. The CLI escapes terminal controls and bidi characters. Parsing recipe metadata does not fetch source URLs; explicit desktop link actions open HTTP(S) source addresses in the system browser. WAV export stages a file beside its destination and publishes only completed output. Cancellation and failures remove the partial file, and cleanup errors name any leftover. Native Save confirms replacement; CLI output paths preserve existing files.

Dependency policy is in [deny.toml](deny.toml). Keep system audio, windowing and graphics components updated. Listening safety is described in the [README](README.md#listening-safely).
