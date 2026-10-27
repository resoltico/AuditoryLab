# Auditory Lab

Auditory Lab lets you inspect, edit, play and export steady tones, click trains, amplitude modulation, gates and explicit binaural or monaural pairs. Built-in recipes include citations, study populations, methods and the adaptations used to make the sound. The desktop Workbench and UTF-8 TOML files describe the same sounds.

The `auditorylab` CLI and `auditorylab-gui` desktop share a persistent User Library. You can keep temporary Open files separate, save independent copies and recover User recipes from Trash.

## Listening safely

Set the device volume low before pressing Play, then raise it gradually. Digital dBFS values describe samples; device volume, headphones and speakers determine the listening level. Use headphones for binaural pairs so each ear receives its intended tone.

## Download

Choose a CLI or desktop archive for your OS and processor from [Releases](https://github.com/resoltico/AuditoryLab/releases/latest). macOS packages require macOS 15 or later and contain either Apple silicon or Intel code. Windows requires Windows 10 on x86_64 or Windows 11 on ARM64. See the [desktop platform requirements](docs/desktop.md#platforms) for graphics and Linux dependencies.

Compare the archive with its entry in `SHA256SUMS` before extracting: use `shasum -a 256 ARCHIVE` on macOS, `sha256sum ARCHIVE` on Linux, or `Get-FileHash ARCHIVE -Algorithm SHA256` in PowerShell. Follow the included `README.txt` to launch it. The macOS app contains its legal resources and can be moved to Applications.

## Find a sound

Use `./auditorylab` from an extracted macOS/Linux archive, `.\auditorylab.exe` on Windows, or `auditorylab` when installed on PATH:

```text
auditorylab list --collection builtin --band alpha --method binaural
auditorylab describe builtin:ioannou-2015-binaural-10-hz
auditorylab inspect builtin:ioannou-2015-binaural-10-hz
```

These commands open no audio device. `describe` gives the sound and study context; `inspect` checks the sound at the selected sample rate.

## Make a recipe and WAV

```text
auditorylab recipe builtin:ioannou-2015-binaural-10-hz --output my-pair.toml
auditorylab inspect --file my-pair.toml --sample-rate 44100
auditorylab export --file my-pair.toml --output my-pair.wav --sample-rate 48000
```

Edit the TOML to change the sound, then inspect it again. CLI exports keep existing output files untouched. [Authoring recipes](docs/recipes.md) covers fields and waveforms; [formats](docs/formats.md) defines the supported document, JSON and library contracts.

## Create a sound in the desktop

Open **Auditory Lab.app**, `auditorylab-gui.exe`, or `auditorylab-gui`. Choose **New Sound** for an empty, silent Workbench. Add a component, enter its rate and level, and use the persistent Play/Stop control to audition it after setting device volume low. **Save sound** stores the draft in User Library. Stop audio before **Export WAV…**.

Selecting a Library row shows its description and preview. **Play selected sound** starts that captured recipe; browsing keeps the playing session unchanged. **Open Recipe…** adds temporary input. Use **Add to User Library**, **Edit a copy**, or **Save independent copy** when you want persistent content. See the [desktop guide](docs/desktop.md).

## Build from source

Install [rustup](https://rustup.rs/); [rust-toolchain.toml](rust-toolchain.toml) selects Rust and its components.

- macOS: install Xcode Command Line Tools. Native app icon compilation needs full Xcode 26 or later on macOS 26 or later; [packaging](docs/releasing.md#build-distributions) also supports matching precompiled icon resources.
- Debian/Ubuntu: install `build-essential pkg-config libasound2-dev libfontconfig1-dev libxkbcommon-x11-0 libglib2.0-bin`.
- Windows: install Visual Studio Build Tools with Desktop development with C++, native MSVC tools and the Windows SDK. Use the native MSVC Rust host.

The workspace separates `auditorylab-core`, `auditorylab-app`, `auditorylab-presentation`, CLI and GUI. The [architecture](docs/architecture.md) describes their public boundaries; the GUI build script uses core vocabulary only. Runtime library choices and command admission belong to app, and presentation renders the app’s typed views. Shared behavioral fixtures use core’s dev-only `test-support` feature.

From a clean checkout:

```sh
cargo install --locked --path crates/cli
cargo run --release --locked -p auditorylab-gui
```

An unbundled macOS executable can be run through Cargo; build a distribution for its app bundle and Dock/Finder identity. Linux desktop use needs X11 or Wayland, fonts, fontconfig, xkbcommon, OpenGL and an xdg-desktop-portal backend for native file dialogs. GNOME Wayland text scaling uses GLib’s `gsettings`.

## Guides

- [CLI](docs/cli.md): sources, queries, lifecycle commands and cancellation.
- [Desktop](docs/desktop.md): editing, transport, monitoring and file decisions.
- [Formats](docs/formats.md): recipe schema, JSON, library layout and exit codes.
- [Recipes](docs/recipes.md): authoring and waveform formulas.
- [Catalog](docs/catalog.md): primary methods, conditions and adaptations.
- [Architecture](docs/architecture.md) and [contributing](CONTRIBUTING.md).
- [Packaging](docs/releasing.md), [changelog](CHANGELOG.md) and [security](SECURITY.md).

## License

Copyright © 2026 Ervins Strauhmanis. Software and documentation use MPL-2.0. Third-party components retain their own terms and appear in product-specific dependency notices. Owner artwork has separate terms in [LICENSE](LICENSE); the [artwork guide](artwork/README.md) identifies editable sources and attribution.
