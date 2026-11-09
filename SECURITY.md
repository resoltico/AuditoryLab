# Security

Report suspected vulnerabilities through [GitHub private vulnerability reporting](https://github.com/resoltico/AuditoryLab/security/advisories/new). Include the affected version, operating system, minimal input and reproducible steps. Keep credentials and private data out of reports. Use public issues for ordinary defects and questions. Security fixes target the current public release and may require upgrading.

## Recipe input

The shared parser bounds input size, requires UTF-8, checks the schema version and rejects unknown fields. Signal validation bounds duration, component count, rates, pulse widths and per-channel digital peak sums. File loading checks the opened handle and admits only regular files, including symlinks to them. Unix opens are nonblocking so FIFOs can be rejected before reading. Intentional streams use the bounded reader or CLI `--stdin`.

Recipe metadata is descriptive text. Human-readable CLI reports escape terminal controls and bidi characters. Portable TOML output retains its bytes, and JSON preserves decoded string data using JSON escaping; treat these as machine-readable formats when displaying untrusted metadata. Parsing recipe metadata does not fetch source URLs; explicit desktop link actions open HTTP(S) source addresses in the system browser. WAV export stages a file beside its destination and publishes only completed output. Cancellation and failures remove the partial file, and cleanup errors name any leftover. Native Save dialogs confirm replacement; CLI output paths preserve existing files.

## Recording input

Recordings are untrusted local media. The app admits one regular file handle, rejects unsupported codecs/channel layouts and nonfinite decoded samples, and streams an explicit interval without a whole-file counting pass or reopen. A bounded prefix selects a supported FFT plan; completed results disclose the actual selected frames and keep unknown source duration distinct. Owned buffering and application-controlled frame, packet and skip work have the [documented limits](docs/formats.md#input-and-selected-window); exhaustion refuses the result instead of silently shortening it. There is no file-size ceiling.

The maintained AAC patch refuses unsupported profiles and SBR rather than returning an incomplete decode. The RF64/BW64 reader validates 64-bit lengths and bounds its retained size-table metadata. See [supported formats and limits](docs/formats.md#recording-measurements) and [dependency integration](third_party/README.md). Cancellation is cooperative and cannot interrupt a blocked read, codec allocation/decode call or FFT call. Container/decoder code still processes untrusted bytes; the application budgets are not a hard wall-clock or total third-party allocation limit. Measurements do not establish that unexamined packets are valid. Analysis itself does not open an audio output device.

Dependency policy is in [deny.toml](deny.toml). Keep system audio, windowing and graphics components updated. Listening safety is described in the [README](README.md#listening-safely).
