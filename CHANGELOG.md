# Changelog

Notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking

- The `auditorylab` library has no default features. Enable `device-playback` for default-device playback (`auditorylab::device`); without it, the library builds without CPAL and `Workspace` refuses audio with a typed reason. The CLI and desktop enable it.
- Library and core APIs use recipe vocabulary and semantic identifiers, with no aliases for the former names: core `Recipe` and library `RecipeSnapshot` (immutable content, origin and identity) replace `Preset`; `CarrierKind` replaces `SourceKind`; `ParameterId` with its label, unit and help descriptor replaces positional settings; `ComponentId` identifies components. `TargetCheck` compiles components once at a rate and replaces `Component::validate` and `Component::facts`; `Target` is a core type whose equality compares definitions and whose `same_audio` compares generated audio. Playback uses a single-owner `Session`, cloneable `PlaybackControl`, typed `PlaybackReport` and `EndReason`. Editing uses `EditorSession` with `FieldHandle` commands. Update imports and exhaustive matches. Recipe schema 2 and the catalog's signal parameters are unchanged.
- Validation messages name quantities by label in display units, such as “Modulation depth 150% must be from 0% to 100%”. Update consumers that match message text.

### Added

- `auditorylab::workspace::Workspace` holds the interactive workflow shared by desktop frontends: library search and selection, one open document, one audio and one file task, prompts and their typed responses, action availability and change-driven views.
- Pulse widths are reported as requested and realized, with the realized frame count at the stated rate, in CLI `inspect` and `describe`, desktop descriptions and generated details.

### Changed

- Committing typed input applies the whole sound at once. If any typed entry does not parse or any value is not a finite number, including an imported one, nothing changes and every typed entry stays for correction; otherwise all entries apply together, even values the sound cannot play, so the draft can be saved and repaired. Changing a carrier or modulation discards typed entries only for the fields it removes.
- Play and live playback are no longer blocked by the export rate. The device's actual rate decides validity before the stream starts: a 30 kHz tone can play on a 96 kHz device while it stays invalid for 48 kHz export, and a sound valid only at the export rate is refused before playback on a slower device. Live edits are checked at the device's rate.
- Live playback accepts settings that differ only in requested values that generate identical samples, such as a width that rounds to the same frames, as a new revision without a transition. **Pattern explained** follows the playing session's accepted settings.
- Reopening a recipe file replaces its Library entry with the new content; a copy made earlier keeps the recipe it was copied from in its source details.
- Recipe saving and WAV export without replacement reject an existing destination before writing anything.
- On macOS, Quit from the application menu or Command-Q and the window's close button use the same decision; repeated Quit asks once, and the app quits only after its audio and file tasks have finished. About in the application menu opens Auditory Lab's About window. The Stop button in a file panel stops only the audio that was playing when the panel opened.

### Removed

- `cargo xtask check-lints` and `lint-exceptions.toml`. Lint exceptions are narrow `#[expect]` attributes whose reasons explain them in the source; `allow` attributes are denied.

### Internal

- The six vendored toolkit crates record their upstream crates.io archives and SHA-256 values, with a command for reviewing the changes against them. The vendored Winit termination request state has unit tests that run on macOS CI.
- CI runs a downstream consumer without device playback and checks that CPAL is absent from its dependency tree, and mutates the core file that compiles targets.

## [0.4.0] - 2026-10-21

### Breaking

- macOS CLI and desktop archives use `.tar.gz` instead of `.zip`; update automated download and extraction commands. Apple silicon and Intel packages remain separate.
- CLI and desktop archives add `COPYRIGHT-library.html` for Rust standard-library component notices. Keep it with `THIRD-PARTY-NOTICES.txt` when redistributing, and update exact archive-member allowlists.
- Desktop drawing requires an OpenGL-capable graphics driver (OpenGL 2.0 or newer on Windows). Software-renderer recovery and `SLINT_BACKEND=winit-software` are unavailable; install a compatible graphics driver.
- Integer-phase oscillators and exact frame-quantized pulse timing change generated sample bits. Click and rectangular-gate pulses last exactly `round(width_seconds × sample rate)` frames and start on the first frame at or after each onset. Recipes that leave no closed frame in a period, or whose pulse widths exceed 2^36 frames at the selected rate, are rejected; reduce those widths. Regenerate sample-exact reference audio and expectations. Recipe schema 2 and the built-in catalog's signal parameters are unchanged.
- Rust packages are `auditorylab-core`, `auditorylab`, `auditorylab-cli` and `auditorylab-gui`, under `crates/core`, `crates/lib`, `crates/cli` and `crates/gui`. WAV, device and application APIs are consolidated in `auditorylab`; use `Stimulus::signal`, immutable seekable `Signal` and the library's loading/export/playback and monitoring APIs. Update imports, observation consumers and exhaustive error matches. Install the CLI with `cargo install --locked --path crates/cli`.
- Windows source builds require the native MSVC Rust host and Visual Studio Build Tools; downloads statically link the Microsoft C runtime. Linux source builds use distribution fontconfig and work with `--manifest-path` outside the repository root. Linux desktop font preferences use Pango/GTK 3/GIO; Debian/Ubuntu users need `libpango-1.0-0 libgtk-3-0`. See [source prerequisites](https://github.com/resoltico/AuditoryLab/blob/v0.4.0/README.md#build-from-source).
- Source builds require Rust 1.99.1 or newer; the repository toolchain pins 1.99.1.
- `--build-info` and build-kind markers are removed. Use `--version` or desktop About for product version, numeric native build counter and optional source revision; update identity parsers.
- CLI playback completion moves from stdout to stderr alongside start/status text and warnings; update redirected-output consumers. Redirected stderr remains static, while terminal stderr displays generated status at 4 Hz. Other report formats also change; CLI commands, explicit input selection and exit codes are unchanged.

### Added

- A desktop Workbench builds and changes up to 16 tone/click components live, with sliders, exact numeric fields, ear routing, level and modulation controls. Smooth ramps, glides and crossfades connect valid settings. Save drafts as schema-2 recipes with custom title/summary text, or stop audio and export a finite WAV. The Rust library exposes draft editing, shared control ranges, live generation/control and generated observations.
- Document-local **Pattern explained** shows static timing/envelope models for validated sounds and accepted draft settings, separately from actual-output captures. Click, gate and AM rhythms, steady companions and interaural/monaural pair conditions retain their distinct meanings.
- CLI `-V` as an alias for `--version`.
- Positive frequencies too small to advance the previous phase model are accepted.

### Changed

- Desktop navigation and Save stay fixed around a scrolling sound document, bounded optional Output monitor drawer and compact persistent transport. Large text/small space can use a focused monitor window with scrolling and keyboard access to its controls and warnings; full selectable diagnostics open separately. About is available through the application/Help menu.
- Native desktop typography follows San Francisco on macOS, Windows message-font/text-size settings and the configured Linux desktop family/size. Native font changes reflow application windows while preserving pending edits.
- Library search covers titles, IDs, research and sound parameters, with explicit human/mouse study context. Wrapped results, compact layers and collapsible settings keep sound actions accessible. Edited source research stays attributed separately from custom notes.
- Exact fields retain precision and admit valid pending corrections without an extra blur. Setting-specific diagnostics identify coupled constraints and whole-mix peak errors. Slider gestures use three significant digits for frequency/rate/width, 0.1 dB levels and whole-percent depths.
- One desktop player retains its session while browsing Library and editing Workbench. Generated rates, crossfade roles, stereo traces and window sample peak/RMS distinguish accepted settings from generated output. Waveforms show samples before device conversion, not microphone input, sound pressure or an acoustic playhead. Fixed/Fit use one stereo scale; Hold retains source/session/rate/span/scale/meter context across playback changes. Raw waveform history is bounded to 100 ms.
- Live/Held/Stale/Waiting/Unavailable/Last capture states and receipt age distinguish current captures from retained data. Current coverage warnings recover independently of cumulative gaps. Coverage/display-range advice stays readable without repeated automatic speech; stale/unavailable monitoring, capture errors and independent backend underruns remain announced. Native motion preferences distinguish enabled, disabled and unavailable, with Hold/Return overrides, Follow system and a diagnosed live fallback.
- Sound snapshots save while audio continues; later edits stay dirty, and obsolete results cannot mark replacement drafts saved. Audio Stop and file-task cancellation are independent. WAV export requires idle audio. Native Open/Save/Export dialogs select paths, and Save confirms replacement; CLI output remains no-clobber.
- After open native dialogs are resolved, New, draft replacement, window close and File/app-menu Quit protect unsaved content with Save/Discard/Cancel and wait for the required audio/file workers to finish, including native macOS Cmd-Q. Save-and-close preserves newer edits. Forced termination can leave a hidden `.NAME.RANDOM.partial` file beside an interrupted destination.
- Finite Stop fades over up to 30 ms; live Stop finishes its current transition before a 30 ms fade, allowing almost 60 ms of generation plus device buffering. CLI Ctrl-C retains exit 130.
- CLI help and usage use clap. CLI/desktop durations use hours, minutes and seconds; desktop peak bounds use one decimal and silence reads −∞. Library waveform previews use the same seekable signal as finite playback/export; Workbench previews use the current valid settings.

### Fixed

- Fatal Windows application errors appear in a native dialog, including graphics-context creation failures.
- macOS keyboard shortcuts honor native modifier flags when accessibility input supplies no separate modifier-key events.
- Linux ALSA fault-poll waits are bounded to one second so missed fault notifications can reach recovery without spending the full 30-second stall deadline in one wait.

### Removed

- The sample observation transport and display-memory-budget API; library monitoring supplies bounded generated quantities and captures.
- Python build/release tools and committed generated third-party notices; use `cargo xtask` and the product-specific notices inside each archive.

### Internal

- Authored Rust lint exceptions require an exact source scope and written rationale, checked across inactive platform code, macro attributes and opaque Rust syntax. Package lint inheritance and production lint-weakening flags are checked; numerical and native unsafe exceptions stay confined to their operations.
- Packaging and reusable CI generate separate native CLI/GUI archives and verify their identities, architectures, notices and license-text coverage. Asset collection rejects incomplete, duplicate or unexpected product/target sets. Release calls run every native target and all five scoped mutation suites. Dispatch defaults to non-publishing verification with downloadable checksums, notes and provenance; notes and assets are attested, and publication requires `publish: true` on the verified current main commit.
- Core seekable/live rendering and generated-state observation allocate nothing. Device monitoring uses preallocated storage and one nonblocking lock attempt per buffer; observation does not determine playback completion.

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
