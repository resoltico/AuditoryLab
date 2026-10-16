# Auditory Lab

Auditory Lab plays and exports sounds used in hearing and brain-rhythm research: binaural and monaural beats, steady and amplitude-modulated tones, click trains and pulsed tones. Each built-in sound comes from a published study. It says in plain words what the study tested and found, and gives the citation, known adaptations and details the source left unspecified. You can also write your own sounds as small text files.

The CLI (`auditorylab`) and native desktop application (`auditorylab-gui`) describe sounds, save WAV files and play through the default audio output. Both products target macOS, Linux and Windows, with macOS 15 or later required on Apple silicon and Intel. They share one application layer for sound generation, validation and file publication; native qualification is separate for each product.

## Listening safely

Turn the volume down before playing anything, then raise it slowly. Digital levels do not limit how loud a sound is, and long sessions at high volume can damage hearing. Binaural beats need headphones, because each ear must hear its own tone.

Most of the studies behind these sounds are small, and their results are mixed. A sound's description reports what its study found.

## Download

Choose the product, system and processor from the available archives in [Releases](https://github.com/resoltico/AuditoryLab/releases/latest), check the archive against the accompanying `SHA256SUMS`, and extract it. Follow the bundled README for launch instructions and platform requirements. The CLI executable's `--help` lists its commands and options; the desktop application opens without arguments.

## Use

These examples describe the CLI built from this source; [install it from source](#install-from-source) to use this interface. For a downloaded package, follow its bundled README. Run the commands in a terminal:

```text
auditorylab list
auditorylab describe ioannou-2015-binaural-10-hz
auditorylab inspect ioannou-2015-binaural-10-hz
auditorylab export ioannou-2015-binaural-10-hz --output beat.wav
```

`describe` gives the study context; `inspect` checks the recipe and shows its length, ear routing and digital levels without opening an audio device. This example exports a 60-second sound. To listen on the default output after checking its description and length:

```text
auditorylab play ioannou-2015-binaural-10-hz
```

To make your own version of a sound, save a built-in recipe, edit it in a text editor, then check and use it:

```text
auditorylab recipe chan-2022-clicks-40-hz --output my-clicks.toml
auditorylab inspect --file my-clicks.toml
auditorylab export --file my-clicks.toml --output my-clicks.wav
```

Use a built-in ID, `--file PATH` for a regular recipe file (including a symlink to one), or `--stdin` for an intentional stream. Recipe paths reject directories, pipes and devices. Running `auditorylab` alone shows help. From an unpacked download use `./auditorylab` on macOS/Linux or `.\auditorylab.exe` in PowerShell; installed commands use PATH. Ctrl-C stops playback or export. [Usage](docs/usage.md) covers every command and option, and [presets](docs/presets.md) the recipe format.

## Desktop application

The Compact session layout keeps Play/Stop and generated per-ear waveforms visible, with expandable research context, detail and file controls. Selecting a sound never starts playback. Freezing the display leaves sound running; the plot shows generated digital samples rather than an audible playhead.

The [desktop guide](docs/desktop.md) covers building and starting the application, native prerequisites, importing recipes, validation, playback, export, waveform controls and GPU/software recovery. GUI source builds must run from the repository root to load its system-font configuration. Linux GUI builds also need `libfontconfig1-dev`, `libfreetype6-dev` and installed fonts on an updated supported distribution.

## Install from source

Install [rustup](https://rustup.rs/); [rust-toolchain.toml](rust-toolchain.toml) selects the compiler. On macOS and Linux, install the platform's linker and audio prerequisites:

- macOS: Xcode Command Line Tools (`xcode-select --install`).
- Linux: a C toolchain, ALSA headers and pkg-config; on Debian or Ubuntu, `build-essential`, `libasound2-dev` and `pkg-config`.

Then, from a clone of this repository on macOS or Linux:

```sh
cargo install --locked --path crates/auditory-cli
```

On Windows, install the native GNU LLVM Rust host. For x86_64, run the downloaded rustup installer as:

```text
.\rustup-init.exe --default-host x86_64-pc-windows-gnullvm --default-toolchain none
```

Then open a terminal in a clone of this repository and run:

```text
rustup toolchain install --component rust-mingw
cargo install --locked --path crates/auditory-cli --target x86_64-pc-windows-gnullvm
```

On ARM64, use `aarch64-pc-windows-gnullvm` in both commands that name a host or target. The repository selects the pinned compiler; its bundled `rust-mingw` and `rust-lld` provide the startup libraries and linker. Visual Studio C++ Build Tools and a separate C toolchain are not required for this application. Use an explicit native `--target` for Windows builds and checks, as described in [verification](docs/verification.md#standard-checks).

Windows GUI builds and workspace tests additionally need Python 3.9 or later to [prepare the reviewed system-import material](docs/desktop.md#build-and-start). This prerequisite does not apply to CLI-only builds.

Windows executables require Windows 10 or later on x86_64, or Windows 11 on ARM64, with the system Universal C Runtime (UCRT). The x86_64 build requires SSE3, CMPXCHG16B and LAHF/SAHF instructions; see the [Windows build contract](docs/releasing.md#windows-build-contract).

Cargo installs `auditorylab` into its bin directory, which must be on PATH. Ordinary Cargo source builds are local/unqualified and use the toolchain's deployment defaults; they do not enforce the supported macOS 15 package baseline. Use the [native qualifier](docs/verification.md#product-identity-and-native-qualification) to build and check both packaged products against that baseline.

## Documentation

- [Usage](docs/usage.md): commands, input, output, errors and interruption.
- [Presets](docs/presets.md): the recipe format, waveforms, levels and the built-in catalog.
- [Desktop](docs/desktop.md): native UI and platform/runtime requirements.
- [Catalog review](docs/catalog-review.md): primary methods, supplements and evidence limits.
- [Signal verification](docs/signal-verification.md): independent numerical and offline computation checks.
- [Architecture](docs/architecture.md): crates, rendering and output contracts.
- [Contributing](CONTRIBUTING.md): changes, tests and dependencies.
- [Verification](docs/verification.md): checks and device tests.
- [Releasing](docs/releasing.md): building and publishing releases.
- [Changelog](CHANGELOG.md) and [security](SECURITY.md).

## License

Copyright © 2026 Ervins Strauhmanis. The source is available under the [Mozilla Public License 2.0](LICENSE). Third-party software in the executables is listed in [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).
