# Changelog

Notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking

- Windows source builds use the native Rust GNU LLVM host (`x86_64-pc-windows-gnullvm` or `aarch64-pc-windows-gnullvm`) with bundled startup libraries and linker. Install the host and `rust-mingw` component using the [source setup](README.md#install-from-source), and pass the native `--target` when building. The x86_64 build requires SSE3, CMPXCHG16B and LAHF/SAHF instructions.
- Presets use `schema_version = 2`. Component levels are named `peak_dbfs`, and the optional `[provenance]` table is replaced by `[about]`, in which only `title` is required and `description` is renamed `study`. Schema 1 and the old names are rejected. Built-in schema 2 recipes can be saved with `show NAME --output FILE` and edited.

### Added

- `rectangular` modulation switches a component on for `width_seconds` at the start of each period, for tone-pip and pulsed-tone stimuli.
- Built-in presets from Martorell et al. (2019), 1 ms 10 kHz tone trains at 8, 40 and 80 Hz, and Lahijanian et al. (2024), a 40 Hz train of 1 ms 5 kHz tones.
- Every built-in preset states what its source studied it for and summarizes the study and its findings in plain language.
- `about PRESET` prints a preset's description as readable text.
- `show NAME --output FILE` saves a built-in preset to a new file, so saving no longer depends on the shell's redirection or encoding.
- A preset saved as UTF-16 is reported as UTF-16 rather than as invalid text.

### Changed

- Release archives contain the executable, `README.txt`, `LICENSE` and a single `THIRD-PARTY-NOTICES.txt`.
- macOS executables are built for macOS 11 or later (Apple silicon) and 10.12 or later (Intel), where 0.1.0 required macOS 15.
- Windows instructions work in any shell. Windows executables use open-source MinGW startup/support and static LLVM unwind code, with UCRT supplied by Windows 10 or later on x86_64, or Windows 11 on ARM64. Release archives include the complete reviewed MinGW and LLVM runtime notices.

## [0.1.0] - 2026-10-14

- First release.
