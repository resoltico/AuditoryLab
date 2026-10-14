# Auditory Lab

Auditory Lab composes tones, rectangular clicks and sinusoidal amplitude modulation into stereo stimuli. It can inspect a preset, export a float WAV file, or play through the configured default output. An embedded catalog contains selected published sound parameters with their sources and adaptations.

## Quick start

Download the archive for your operating system and CPU from [Releases](https://github.com/resoltico/AuditoryLab/releases/latest). macOS 15 or later is required; Intel and Apple Silicon have separate ZIPs. Verify the download against `SHA256SUMS`, extract it and follow its `INSTALL.txt`; `USAGE.txt` gives commands. Mac executables are ad hoc signed and are not Apple notarized. The archive explains the app-specific security approval if macOS blocks a trusted download.

Run `auditory-lab list` to discover the embedded presets. No compiler or checkout is needed for the downloaded executable.

### Install from source

Install [rustup](https://rustup.rs/); [rust-toolchain.toml](rust-toolchain.toml) selects the compiler. Rust also needs the platform's native linker:

- macOS: Xcode Command Line Tools (`xcode-select --install`).
- Linux: a C toolchain, ALSA headers and pkg-config; on Debian or Ubuntu, install `build-essential`, `libasound2-dev` and `pkg-config`.
- Windows with MSVC: Visual Studio C++ Build Tools and a Windows SDK; follow rustup's setup prompts.

See Rust's [installation guidance](https://doc.rust-lang.org/book/ch01-01-installation.html) for platform setup. Ensure Cargo's bin directory is on PATH so `auditory-lab` is found after installation.

Obtain the source and install from its root:

```sh
git clone https://github.com/resoltico/AuditoryLab.git
cd AuditoryLab
cargo install --locked --path crates/auditory-lab
auditory-lab list
auditory-lab inspect orozco-perez-2020-binaural-7-hz
auditory-lab show chan-2022-clicks-40-hz > short-clicks.toml
```

Edit `short-clicks.toml` to set `duration_seconds = 1.0` and update `provenance.adaptation` to describe the one-second copy. Then inspect and export it:

```sh
auditory-lab inspect short-clicks.toml
auditory-lab export short-clicks.toml --output short-clicks.wav
```

The installed executable works without the checkout. `list` shows durations; built-in conditions can last many minutes. Export requires a fresh destination and never replaces existing files.

**Set the output volume low before playback.** Digital levels do not limit sound pressure. Headphones preserve separate left/right ear routing; the program does not detect or calibrate them. To play the short copy:

```sh
auditory-lab play short-clicks.toml
```

Ctrl-C requests cancellation during export or playback. Forced termination may leave incomplete output. See [usage](docs/usage.md) for input, device and interruption details.

## Documentation

- [Usage](docs/usage.md): commands, input sources, output, errors and interruption.
- [Presets](docs/presets.md): editable examples, fields, waveforms, levels and catalog sources.
- [Architecture](docs/architecture.md): responsibilities, rendering and sink contracts.
- [Contributing](CONTRIBUTING.md): change workflow, conventions and dependencies.
- [Verification](docs/verification.md): checks, device tests and bounded mutation testing.
- [Releasing](docs/releasing.md): native packages, release gates and publication.
- [Changelog](CHANGELOG.md): public release outcomes and release notes.
- [Security](SECURITY.md): private vulnerability reports.

## License

Copyright © 2026 Ervins Strauhmanis. Source is available under the [Mozilla Public License 2.0](LICENSE). Dependencies retain their own licenses.
