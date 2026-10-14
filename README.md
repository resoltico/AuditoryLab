# Auditory Lab

Auditory Lab plays and exports sounds used in hearing and brain-rhythm research: binaural and monaural beats, steady and amplitude-modulated tones, click trains and pulsed tones. Each built-in sound comes from a published study. It says in plain words what the study tested and found, and gives the citation and every way the sound differs from the study's. You can also write your own sounds as small text files.

It runs in a terminal on macOS, Linux and Windows. It describes a sound, saves it as a WAV file, or plays it through the default audio output.

## Listening safely

Turn the volume down before playing anything, then raise it slowly. Digital levels do not limit how loud a sound is, and long sessions at high volume can damage hearing. Binaural beats need headphones, because each ear must hear its own tone.

Most of the studies behind these sounds are small, and their results are mixed. A sound's description reports what its study found.

## Download

Download the archive for your system and processor from [Releases](https://github.com/resoltico/AuditoryLab/releases/latest), check it against that release's `SHA256SUMS`, and extract it. Follow the selected release's bundled instructions and platform requirements; for online instructions, open its tag and read that version's README. Use the downloaded executable's `--help` for its supported commands and options.

## Use

These examples and the linked usage and preset guides describe this checkout. [Install from source](#install-from-source) to build it, then run these commands in a terminal.

```sh
auditory-lab list
auditory-lab about beauchene-2016-binaural-15-hz
auditory-lab play beauchene-2016-binaural-15-hz
auditory-lab export beauchene-2016-binaural-15-hz --output beat.wav
```

To make your own version of a sound, save a built-in recipe, edit it in a text editor, then check and use it:

```sh
auditory-lab show chan-2022-clicks-40-hz --output my-clicks.toml
auditory-lab inspect my-clicks.toml
auditory-lab play my-clicks.toml
```

Ctrl-C stops playback or export. [Usage](docs/usage.md) covers every command and option, and [presets](docs/presets.md) the recipe format.

## Install from source

Install [rustup](https://rustup.rs/); [rust-toolchain.toml](rust-toolchain.toml) selects the compiler. On macOS and Linux, install the platform's linker and audio prerequisites:

- macOS: Xcode Command Line Tools (`xcode-select --install`).
- Linux: a C toolchain, ALSA headers and pkg-config; on Debian or Ubuntu, `build-essential`, `libasound2-dev` and `pkg-config`.

Then, from a clone of this repository on macOS or Linux:

```sh
cargo install --locked --path crates/auditory-lab
```

On Windows, install the native GNU LLVM Rust host. For x86_64, run the downloaded rustup installer as:

```text
.\rustup-init.exe --default-host x86_64-pc-windows-gnullvm --default-toolchain none
```

Then open a terminal in a clone of this repository and run:

```text
rustup toolchain install --component rust-mingw
cargo install --locked --path crates/auditory-lab --target x86_64-pc-windows-gnullvm
```

On ARM64, use `aarch64-pc-windows-gnullvm` in both commands that name a host or target. The repository selects the pinned compiler; its bundled `rust-mingw` and `rust-lld` provide the startup libraries and linker. Visual Studio C++ Build Tools and a separate C toolchain are not required for this application. Use an explicit native `--target` for Windows builds and checks, as described in [verification](docs/verification.md#standard-checks).

Windows executables built from this checkout need Windows 10 or later on x86_64, or Windows 11 on ARM64, with the system Universal C Runtime (UCRT). The x86_64 build requires SSE3, CMPXCHG16B and LAHF/SAHF instructions; see the [Windows build contract](docs/releasing.md#windows-build-contract).

Cargo installs `auditory-lab` into its bin directory, which must be on PATH.

## Documentation

- [Usage](docs/usage.md): commands, input, output, errors and interruption.
- [Presets](docs/presets.md): the recipe format, waveforms, levels and the built-in catalog.
- [Architecture](docs/architecture.md): crates, rendering and output contracts.
- [Contributing](CONTRIBUTING.md): changes, tests and dependencies.
- [Verification](docs/verification.md): checks and device tests.
- [Releasing](docs/releasing.md): building and publishing releases.
- [Changelog](CHANGELOG.md) and [security](SECURITY.md).

## License

Copyright © 2026 Ervins Strauhmanis. The source is available under the [Mozilla Public License 2.0](LICENSE). Third-party software in the executables is listed in [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).
