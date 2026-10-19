# Auditory Lab

Auditory Lab plays and exports sounds used in hearing and brain-rhythm research: binaural and monaural beats, steady and amplitude-modulated tones, click trains and pulsed tones. Each built-in sound describes what its published study tested and found, with a citation and the recipe's adaptations. You can edit these sounds as TOML text files.

Use the `auditorylab` CLI or the `auditorylab-gui` desktop app. Both load the same recipes, validate them at the chosen sample rate, export stereo float WAV files and play through the default audio output.

## Listening safely

Turn the volume down before playing anything, then raise it slowly. Digital levels do not limit how loud a sound is, and long sessions at high volume can damage hearing. Binaural beats need headphones, because each ear must hear its own tone.

Most of the studies behind these sounds are small, and their results are mixed. A sound's description reports what its study found.

## Download

Choose a CLI or desktop archive from [Releases](https://github.com/resoltico/AuditoryLab/releases/latest). macOS downloads are universal and require macOS 15 or later. Follow the archive's README for platform requirements and launch instructions.

Download `SHA256SUMS` alongside the archive and compare its entry before extracting. On macOS use `shasum -a 256 ARCHIVE`; on Linux use `sha256sum ARCHIVE`; in PowerShell use `Get-FileHash ARCHIVE -Algorithm SHA256`.

## CLI quick start

From a download, use `./auditorylab` on macOS/Linux or `.\auditorylab.exe` in PowerShell. An installed command on PATH uses `auditorylab`:

```text
auditorylab list
auditorylab describe ioannou-2015-binaural-10-hz
auditorylab inspect ioannou-2015-binaural-10-hz
auditorylab export ioannou-2015-binaural-10-hz --output beat.wav
```

`describe` shows declared parameters and study context. `inspect` validates without opening an audio device. To listen after checking the sound and turning the volume down:

```text
auditorylab play ioannou-2015-binaural-10-hz
```

Save and edit a recipe, then check and export it:

```text
auditorylab recipe chan-2022-clicks-40-hz --output my-clicks.toml
auditorylab inspect --file my-clicks.toml
auditorylab export --file my-clicks.toml --output my-clicks.wav
```

Choose a built-in ID, `--file PATH` for a regular recipe file or `--stdin` for a stream. Ctrl-C stops playback or export. The CLI keeps existing output files untouched. Run `auditorylab` for help or see the [CLI guide](docs/cli.md).

## Desktop

Open **Auditory Lab.app** on macOS, `auditorylab-gui.exe` on Windows, or `auditorylab-gui` on Linux. Select a sound to see its description and waveform. **Play/Stop** controls playback; elapsed time and progress follow its playback position. **Open Recipe…** imports a file, and **Export WAV…** chooses a destination through the native Save dialog. See the [desktop guide](docs/desktop.md).

## Build from source

Install [rustup](https://rustup.rs/). [rust-toolchain.toml](rust-toolchain.toml) selects the compiler and components.

- macOS 15 or later: install Xcode Command Line Tools with `xcode-select --install`.
- Debian/Ubuntu: `sudo apt-get install build-essential pkg-config libasound2-dev libfontconfig1-dev libxkbcommon-x11-0`.
- Windows: install Visual Studio Build Tools with Desktop development with C++, including the MSVC tools and Windows SDK for your processor. Use the native MSVC Rust host. Windows 10 or later is required on x86_64; ARM64 requires Windows 11. The x86_64 target requires SSE3, CMPXCHG16B and LAHF/SAHF.

From a clean checkout:

```sh
cargo install --locked --path crates/cli
cargo run --release --locked -p auditorylab-gui
```

Cargo installs the CLI into its bin directory, which must be on PATH. Linux desktop use needs X11 or Wayland, fontconfig, xkbcommon (including its X11 library on X11), fonts, OpenGL and an xdg-desktop-portal for file dialogs. The desktop renderer uses OpenGL; sound synthesis runs on the CPU. Build commands also work with `--manifest-path` from another directory.

## Guides

- [CLI](docs/cli.md): commands, input, output, exit codes and Ctrl-C.
- [Desktop](docs/desktop.md): window controls, dialogs and platform requirements.
- [Presets](docs/presets.md): recipe fields, waveforms, validation and catalog additions.
- [Catalog review](docs/catalog-review.md): study methods, parameters and adaptations.
- [Architecture](docs/architecture.md): crates, synthesis, playback and GUI threads.
- [Contributing](CONTRIBUTING.md): checks, dependencies, packaging and releases.
- [Changelog](CHANGELOG.md) and [security](SECURITY.md).

## License

Copyright © 2026 Ervins Strauhmanis. Source: [Mozilla Public License 2.0](LICENSE). Each download contains its product-specific `THIRD-PARTY-NOTICES.txt`.
