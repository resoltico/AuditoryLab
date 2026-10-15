# Changelog

Notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-10-15

### Breaking

- Windows source builds use the native Rust GNU LLVM host (`x86_64-pc-windows-gnullvm` or `aarch64-pc-windows-gnullvm`) with bundled startup libraries and linker, replacing the MSVC build route. Install the host and `rust-mingw` component using the [source setup](README.md#install-from-source), and pass the native `--target` when building. Windows x86_64 executables require SSE3, CMPXCHG16B and LAHF/SAHF instructions.
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
- Release publication checks uploaded asset digests before making a draft public and no longer resumes a partially uploaded draft automatically. Follow the [failure recovery procedure](docs/releasing.md#when-publishing-fails) before retrying; published versions must never be replaced.

### Internal

- Native packaging checks Windows runtime link inputs using evidence bound to the executable's SHA-256. Quality also packages native builds on macOS, Linux and both Windows architectures. The previous native Windows packaged-interruption harness is removed, so these checks do not establish Windows Ctrl-C or Ctrl-Break behavior.

## [0.1.0] - 2026-10-14

- First release.
