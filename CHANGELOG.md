# Changelog

Notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking

- Desktop drawing requires an OpenGL-capable graphics driver (OpenGL 2.0 or newer on Windows); software-renderer recovery is removed. Fatal Windows application errors appear in a native dialog.
- Integer-phase oscillators and exact frame-quantized pulse timing replace the synthesis model. Click and rectangular-gate pulses last exactly `round(width_seconds × sample rate)` frames and start on the first frame at or after each onset; edges can move by one frame against 0.3.0. Recipes that leave no closed frame in a period, or whose pulse widths exceed 2^36 frames at the selected rate, are rejected. Reduce such widths to the supported frame limit. Tone and modulation sample bits also change.
- Rust packages are `auditorylab-core`, `auditorylab`, `auditorylab-cli` and `auditorylab-gui`, under `crates/core`, `crates/lib`, `crates/cli` and `crates/gui`. WAV and device APIs move into the library. Callers use `Stimulus::signal`, immutable seekable `Signal`, and the library's loading/export/playback API; update Rust imports and exhaustive error matches. Install the CLI with `cargo install --locked --path crates/cli`.
- Windows source builds require MSVC Visual Studio Build Tools. Downloads statically link the Microsoft C runtime. Linux builds use distribution fontconfig without the former font-library patch or root-directory build requirement.
- Source builds require Rust 1.99.1 or newer; the repository toolchain pins 1.99.1.

### Added

- A desktop Workbench builds and changes up to 16 tone/click components live, with sliders, exact numeric fields, ear routing, level and modulation controls. Smooth ramps, glides and crossfades connect valid settings; drafts save as schema-2 recipes with title/summary text or export as finite WAV files. The Rust library supplies draft editing, shared control ranges and live playback.
- An optional bounded output-monitor drawer sits above compact persistent transport, with fixed navigation/Save and a scrolling sound document. Large text/small space can use a focused monitor window; full selectable diagnostics open separately.
- CLI `-V` as an alias for `--version`.
- Positive frequencies previously rejected as too small to advance the phase are accepted.

### Changed

- Native desktop fonts follow San Francisco on macOS, Windows UI font/text scaling and configured Linux desktop family/size. About is available through the application/Help menu, with identity and licenses retained.
- Library prioritizes sound meaning and attributed study context; Workbench groups directly editable layers with explicit routing/modulation disclosures and document-local static Pattern explained. Exact fields preserve precision, recover valid pending corrections without an extra blur, and report setting-specific errors. Slider gestures use three significant digits, 0.1 dB levels and whole-percent depths.
- Bounded core timing/envelope projections distinguish static recipe/accepted-target models from generated waveforms. Click/gate/AM summaries retain independent rhythms and steady companions; narrow interaural and monaural pairs retain their physical meaning.
- One desktop player retains its session across Library browsing and Workbench edits. Generated rates, crossfade roles, actual stereo traces and window sample peak/RMS distinguish accepted controls from generated output; Fixed/Fit share a truthful stereo scale, and Hold retains complete source/session/rate/span/scale/meter context across playback changes. Live/Held/Stale/Waiting/Unavailable/Last states and receipt age distinguish current data from history; cumulative gaps remain in diagnostics. Coverage/display-range advice stays readable without repeated automatic speech; stale/unavailable monitoring, capture errors and independent backend warnings remain announced. Native motion preferences use explicit enabled/disabled/unknown states with user override and diagnosed live fallback.
- Library search covers titles, IDs, research and sound parameters. Wrapped results, compact component controls and collapsible notes/saved-audio settings keep sound actions accessible. Edited source research stays attributed separately from custom notes.
- New, draft replacement, window close and File/app-menu Quit protect unsaved content with Save/Discard/Cancel and wait for worker cleanup, including native macOS Cmd-Q. Sound snapshots save while audio continues; audio Stop and file-task cancellation are independent, both lifetimes are joined on quit, and late/obsolete save results preserve newer or replacement drafts. WAV export requires idle audio.
- CLI/GUI version output and About distinguish product version, numeric native build counter and optional source revision. CLI Play renders start, generated status and final results on stderr; redirected text stays static.

- Stop fades finite playback over 30 ms, shortened at the end; live Stop finishes any current transition before its 30 ms fade. CLI Ctrl-C retains exit 130.
- CLI and desktop durations read as hours, minutes and seconds; desktop peak bounds use one decimal and silence reads −∞.
- CLI help and usage use clap. Commands, options, explicit source selection and exit codes stay the same. Version output is `auditorylab VERSION`, with a short commit ID for CI builds.
- Desktop Open and Export use native dialogs. Save replaces an existing file only after native confirmation; CLI output keeps existing files untouched. Closing the window waits for worker cleanup; forced termination can leave a hidden `.NAME.RANDOM.partial` export file beside the destination.
- Library waveform previews use the same seekable signal as finite playback and export; Workbench previews show the current valid settings.
- Preset files retain `schema_version = 2`; existing catalog parameters are unchanged.
- Packaging uses `cargo xtask`; reusable CI builds the release archives. Release dispatch defaults to non-publishing verification with downloadable checksums, notes and provenance; publication requires `publish: true` on main. cargo-deny gates dependency policy, and packaging checks dependency and license-text coverage.

### Fixed

- macOS keyboard shortcuts honor native modifier flags when accessibility input supplies no separate modifier-key events.
- Linux playback checks missed ALSA fault notifications soon enough to recover before the existing stall deadline.

### Removed

- `--build-info` output, build kinds, the sample observation transport and display memory budgets.
- Python build/release tooling and committed generated third-party notices.

### Internal

- Authored Rust lint exceptions require an exact source scope and written rationale, checked in CI across inactive platform code, macro attributes and opaque expression tokens. Package lint inheritance and production lint-weakening flags are checked; numerical and native unsafe exceptions are confined to their specific operations.
- macOS CLI and GUI artifacts are separately built for Apple silicon and Intel, with extracted thin-architecture, identity, plist, signature and executable checks. Product/target notices are generated inside each archive. Release asset collection rejects incomplete, duplicate or unexpected archive sets before checksums and attestation.
- Public library live generation/control/observation and bounded mixed-output monitoring share the device callback's generator boundary. Core rendering and generated-state observation allocate nothing; device monitoring uses a preallocated nonblocking lock attempt per buffer.

## [0.3.0] - 2026-10-16

### Breaking

- CLI and desktop packages require macOS 15 or later on both Apple silicon and Intel. CLI users on older macOS versions must upgrade the OS to use these packages.
- The CLI executable is `auditorylab` (`auditorylab.exe` on Windows), installed from package `auditory-cli`; replace `auditory-lab` command invocations and install from `crates/auditory-cli`. Release CLI archives/folders use `auditorylab-VERSION-PLATFORM`; desktop archives use `auditorylab-gui-VERSION-PLATFORM`. No old executable alias is provided.
- CLI commands `describe` and `recipe` replace `about` and `show`. `describe`, `inspect`, `play` and `export` accept exactly one positional built-in ID, `--file PATH`, or `--stdin`; `recipe` accepts only a built-in ID. Punctuation, existing paths and piped input no longer select sources implicitly. Update file/stdin invocations and consumers of changed human reports. No command aliases or compatibility modes are provided; schema 2's `[about]` table is unchanged.
- Recipe path loading accepts only regular files and symlinks to regular files; use CLI `--stdin` or `auditory_app::load_reader` for intentional streams. FIFOs, devices and directories reject before reading. This does not impose deadlines on stalled regular-file filesystems.
- Rust callers must replace `auditory_wav::export(session, path, cancel)` with `auditory_wav::encode(session, writer, cancel)` or use `auditory-app` export preparation/execution. The encoder no longer owns file creation, syncing, publication or cleanup, and `ExportError::DestinationExists` and `ExportError::IncompleteFileLeft` are removed. Update exhaustive matches.
- Rust `Renderer::render` returns `RenderedSpan` instead of an active-frame count; use `.frames()` for the count and `.start()`/`.end()` for source coverage. `ComponentError` variants with field identifiers use `parameter: Parameter` instead of free-form `field` strings. Direct device callers must pass the observation argument to `Output::play(session, cancel, observation)` (`None` for ordinary playback). `auditory_device::ListeningAdvice` is removed; shared application descriptions/playback views supply typed `Notice` values. Update callers and exhaustive matches.

### Added

- A native `auditorylab-gui` desktop frontend with a Compact session layout, selectable research/diagnostic text, native/manual recipe paths, validation, WAV export and explicit Play/Stop. Close and ordinary macOS Quit wait for the actual operation and worker. Packages provide an ad hoc signed, unnotarized macOS `.app`, a console-free Windows application and an optional Linux desktop launcher. Slint enables native accessibility and GPU-capable drawing with software recovery; enabled accessibility is not screen-reader qualification. GPU drawing does not offload audio, use every installed GPU or promise recovery from every later graphics failure. See the [desktop guide](https://github.com/resoltico/AuditoryLab/blob/v0.3.0/docs/desktop.md) for platform and source-build prerequisites, including Windows import-library preparation and Linux system FreeType/Fontconfig requirements, and `SLINT_BACKEND=winit-software` recovery.
- Optional Generated signal observation and separate-ear waveform detail/two-second recent Peak range views, with actual source coverage, gaps, freshness, bounded live/frozen displays and reduced-motion presentation. Charts show mixed/faded generated samples before output conversion, not microphone input, sound pressure or an audible playhead. Freeze display leaves audio running; observed samples cannot establish playback completion.
- `auditory-app` supplies both frontends' input/catalog workflows, immutable declared descriptions and resolved facts, typed diagnostics, bound preparation views, listening notices and contextual completion reports. Synthesis/validation and signal analysis remain shared; WAV encoding accepts a caller-owned writer and application workflows own publication and cancellation.
- Shared product/build identity and a narrow standalone `--build-info` artifact-verification response on both products. Local and candidate archives carry explicit local/development markers; candidate identities retain the full source revision. Read-only development qualification uses the release workflow's native checks and isolated final builds without publishing or inventing a next version.

### Changed

- Running the CLI with no arguments shows help successfully. Commands share concise help/error rules and accept native equals-form paths. `list` shows human durations and readable Sound records; `describe` adds declared routes/advice; inspection retains exact frames/rates, round-trip technical peak bounds and declared/resolved duration and fade. Preparation/completion reports retain their bound context, with explicit stream-rate labels and stop cues.
- Three van Eeckhoutte AM titles state the configured 39.0625 Hz modulation instead of nominal 40 Hz. The Lahijanian adaptation retains the source's approximately -40 dB statement with its unspecified reference and participant comfort adjustment. Preset IDs, numeric parameters and generated samples are unchanged.
- Recipe saving and WAV export stage complete output in the destination folder, sync it and publish without replacement. These operations create no final file before commit and never replace an existing destination; cancellation and failure clean only the owned stage, and cleanup failures retain both causes. Successful publication remains successful if cancellation or reporting fails afterward. Abnormal termination can leave a stage; no-overwrite publication is not universally atomic and file sync does not guarantee directory-entry persistence after power loss.

### Internal

- Sample callback terminal publication uses a fixed atomic outcome; asynchronous backend diagnostics are stored separately, and synchronous setup errors retain their original cause without waiting for a diagnostic writer. This covers application sample processing, not every native backend callback. Optional preallocated SPSC observation drops visual data without blocking audio; shared analysis/history and polling have explicit frame/work bounds, with caller-owned presentation capacity checked against the 8 MiB product-owned monitoring-buffer limit. This is not a process-RSS or native-graphics memory bound.
- Required architecture tests check exhaustive workspace roles, dependency directions, anchored source/resource roots and frontend syntax boundaries. Qualification checks nonempty positive/rejection-control execution and a freestanding core build with a std-dependency rejection control. Product/platform/archive/source/kind assignments and product-bound Windows native evidence are verified before release assets are combined; package checks inspect the extracted copy. Windows CLI tests exercise native Ctrl-C/Ctrl-Break during silent export and pre-input waits, plus forced exit on a second interrupt; these are Cargo test binaries rather than packaged-interruption or acoustic qualification.
- Portable `libm` arithmetic is forced across GUI dependency feature unification. Independent numerical, conversion, long-stream and callback characterization checks retain their own evidence limits. Third-party notices distinguish runtime dependencies, retained host code-generation material, generated resources and source-bound patches.

## [0.2.0] - 2026-10-15

### Breaking

- Windows source builds use the native Rust GNU LLVM host (`x86_64-pc-windows-gnullvm` or `aarch64-pc-windows-gnullvm`) with bundled startup libraries and linker, replacing the MSVC build route. Install the host and `rust-mingw` component using the [source setup](https://github.com/resoltico/AuditoryLab/blob/v0.2.0/README.md#install-from-source), and pass the native `--target` when building. Windows x86_64 executables require SSE3, CMPXCHG16B and LAHF/SAHF instructions.
- Presets use `schema_version = 2`. Component levels are named `peak_dbfs`, and the optional `[provenance]` table is replaced by `[about]`, in which only `title` is required and `description` is renamed `study`. Schema 1 and the old names are rejected. Built-in schema 2 recipes can be saved with `show NAME --output FILE` and edited.
- Rust callers must replace `Provenance` and `Preset.provenance` with `About` and `Preset.about`, rename `description` to `study`, and handle the optional descriptive fields. `Component` fields and its `level_dbfs()` accessor become `peak_dbfs` and `peak_dbfs()`. `ComponentError::Level`, `ClickRateTooSmall` and `ClickWidth` become `PeakDbfs`, `PulseRateTooSmall` and `PulseWidth`; the pulse errors include a `field` name. Update exhaustive matches for the added modulation and preset-error variants.
- `inspect` retains the preset title but no longer prints citation, source URL, study description or adaptation. Use `about PRESET` for that text, and update consumers of the inspection output.

### Added

- `rectangular` modulation switches a component on for `width_seconds` at the start of each period, for tone-pip and pulsed-tone stimuli.
- Built-in presets from Martorell et al. (2019), 1 ms 10 kHz tone trains at 8, 40 and 80 Hz, and Lahijanian et al. (2024), a 40 Hz train of 1 ms 5 kHz tones.
- Every built-in preset states what its source studied it for and summarizes the study and its findings in plain language, including study limitations and differences between the recipe and the source stimulus. These descriptions do not establish benefits from listening.
- `about PRESET` prints a preset's description, research source and adaptation as readable text, accepting a built-in name, file or standard input.
- `show NAME --output FILE` saves a built-in preset as UTF-8 without shell redirection or encoding differences. It requires a new destination and never replaces an existing file.
- A preset with a UTF-16 byte-order mark is reported as UTF-16 rather than as invalid UTF-8; preset input still requires UTF-8.

### Changed

- `list` and the Rust catalog order numeric parts of preset names as numbers, so 5 Hz precedes 10 Hz and 500 Hz precedes 1000 Hz.
- Release archives contain four files: the executable, `README.txt`, `LICENSE` and a single `THIRD-PARTY-NOTICES.txt`. `README.txt` replaces `INSTALL.txt` and `USAGE.txt`; the separate `BUILD_INFO.json` and notice directories are removed. The consolidated notices retain the full Rust standard-library component notices as plain text alongside crate and native-runtime notices.
- macOS executables are built for macOS 11 or later (Apple silicon) and 10.12 or later (Intel), where 0.1.0 required macOS 15. Release testing runs on macOS 15 and does not establish execution on those older minimum versions.
- Windows executables use open-source MinGW startup/support and static LLVM unwind code instead of the MSVC runtime, with UCRT supplied by Windows 10 or later on x86_64, or Windows 11 on ARM64. Release archives include the complete reviewed MinGW and LLVM runtime notices; Windows archive platform names are unchanged.
- Contributor release tools require Python 3.9 or later instead of 3.11, using only the standard library.
- Release publication checks uploaded asset digests before making a draft public and no longer resumes a partially uploaded draft automatically. Follow the [failure recovery procedure](https://github.com/resoltico/AuditoryLab/blob/v0.2.0/docs/releasing.md#when-publishing-fails) before retrying; published versions must never be replaced.

### Internal

- Native packaging checks Windows runtime link inputs using evidence bound to the executable's SHA-256. Quality also packages native builds on macOS, Linux and both Windows architectures. The previous native Windows packaged-interruption harness is removed, so these checks do not establish Windows Ctrl-C or Ctrl-Break behavior.

## [0.1.0] - 2026-10-14

- First release.
