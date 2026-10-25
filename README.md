# Auditory Lab

Auditory Lab plays and exports sounds used in hearing and brain-rhythm research: binaural and monaural beats, steady and amplitude-modulated tones, click trains and pulsed tones. Each built-in sound describes what its published study tested and found, with a citation and the recipe's adaptations. You can edit these sounds in the desktop Workbench or as TOML text files.

Use the `auditorylab` CLI or the `auditorylab-gui` desktop app. Both share immutable built-ins and a persistent User Library, validate recipes at the chosen sample rate, export stereo float WAV files and play through the default audio output.

## Listening safely

Turn the volume down before playing anything, then raise it slowly. Digital levels do not limit how loud a sound is, and long sessions at high volume can damage hearing. Binaural beats need headphones, because each ear must hear its own tone.

Most of the studies behind these sounds are small, and their results are mixed. A sound's description reports what its study found.

## Download

Choose a CLI or desktop archive from [Releases](https://github.com/resoltico/AuditoryLab/releases/latest). Choose **Mac · Apple silicon** (`macos-aarch64`) or **Mac · Intel** (`macos-x86_64`); both require macOS 15 or later. CLI and desktop archives are separate, and every macOS executable contains exactly one architecture. macOS desktop downloads use ZIP; Unix CLI and Linux desktop downloads use tar.gz; Windows downloads use ZIP. Follow the archive's README for platform requirements and launch instructions. The macOS app contains its own legal resources, so it can be moved alone to Applications. macOS binaries are ad-hoc signed, without Developer ID signing or notarization.

Download `SHA256SUMS` alongside the archive and compare its entry before extracting. On macOS use `shasum -a 256 ARCHIVE`; on Linux use `sha256sum ARCHIVE`; in PowerShell use `Get-FileHash ARCHIVE -Algorithm SHA256`.

## CLI quick start

From a download, use `./auditorylab` on macOS/Linux or `.\auditorylab.exe` in PowerShell. An installed command on PATH uses `auditorylab`:

```text
auditorylab list
auditorylab describe builtin:ioannou-2015-binaural-10-hz
auditorylab inspect builtin:ioannou-2015-binaural-10-hz
auditorylab export builtin:ioannou-2015-binaural-10-hz --output beat.wav
```

`describe` shows declared parameters and study context. `inspect` validates without opening an audio device. To listen after checking the sound and turning the volume down:

```text
auditorylab play builtin:ioannou-2015-binaural-10-hz
```

Save and edit a recipe, then check and export it:

```text
auditorylab recipe builtin:chan-2022-clicks-40-hz --output my-clicks.toml
auditorylab inspect --file my-clicks.toml
auditorylab export --file my-clicks.toml --output my-clicks.wav
```

Choose a `builtin:ID` or `user:UUID` selector, `--file PATH` for a regular recipe file or `--stdin` for a stream. Ctrl-C stops playback or export. The CLI keeps existing output files untouched. Run `auditorylab` for help or see the [CLI guide](docs/cli.md).

## Desktop

Open **Auditory Lab.app** on macOS, `auditorylab-gui.exe` on Windows, or `auditorylab-gui` on Linux. Search and select a sound to see its description and explicitly labeled preview. **Listen** starts finite audio; the persistent player names the actual session while you browse. **Open Recipe…** imports a file, and **Export WAV…** chooses a destination through the native Save dialog.

The player at the bottom keeps Stop and the elapsed time in reach on every screen. The optional **Output monitor** above it draws the samples being played, or a labeled preview, with their levels, and can hold a moment for a closer look. **Pattern explained** draws a sound's timing pattern beside its description.

Library scopes separate active **All**, read-only **Built-in**, **User**, temporary **Open files**, and recoverable **Trash**. Open does not add a file to persistent membership. Use **Add to User Library** or save a new draft; external originals remain unchanged by managed operations. GUI and CLI resolve the same native data root; an absolute `AUDITORYLAB_LIBRARY_DIR` overrides it. See [root and lifecycle commands](docs/cli.md#root-and-lifecycle).

The **Workbench** builds sounds from up to 16 expanded tone/click voices, including signed-offset pairs. Change their frequency, level, ears and modulation while they play, with smooth ramps, glides and crossfades. Start from an empty sound or **Edit a copy** from Library, then **Save sound** to the User Library while audio continues, or stop audio to export a finite WAV. Source research stays attributed separately from your custom notes; dirty drafts are protected on New and Quit. See the [desktop guide](docs/desktop.md).

## Build from source

Install [rustup](https://rustup.rs/). [rust-toolchain.toml](rust-toolchain.toml) selects the compiler and components.

- macOS 15 or later: install Xcode Command Line Tools with `xcode-select --install`. Compiling Icon Composer resources for app packaging needs full Xcode 26 or later on a macOS 26 or later host; app assembly can use those resources on macOS 15. See [packaging](docs/releasing.md#build-distributions).
- Debian/Ubuntu: `sudo apt-get install build-essential pkg-config libasound2-dev libfontconfig1-dev libxkbcommon-x11-0 libglib2.0-bin`.
- Windows: install Visual Studio Build Tools with Desktop development with C++, including the MSVC tools and Windows SDK for your processor. Use the native MSVC Rust host. Windows 10 or later is required on x86_64; ARM64 requires Windows 11. The x86_64 target requires SSE3, CMPXCHG16B and LAHF/SAHF.

From a clean checkout:

```sh
cargo install --locked --path crates/cli
cargo run --release --locked -p auditorylab-gui
```

Cargo installs the CLI into its bin directory, which must be on PATH. Linux desktop use needs X11 or Wayland, fontconfig, xkbcommon (including its X11 library on X11), fonts, OpenGL and an xdg-desktop-portal for file dialogs. Native UI fonts use platform defaults and toolkit DPI scaling. GNOME Wayland text scaling uses GLib’s `gsettings` utility. The desktop renderer uses OpenGL; sound synthesis runs on the CPU. Build commands also work with `--manifest-path` from another directory.


## Guides

- [CLI](docs/cli.md): commands, input, output, exit codes and Ctrl-C.
- [Desktop](docs/desktop.md): window controls, dialogs and platform requirements.
- [Recipes](docs/recipes.md): recipe fields, waveforms, validation and catalog additions.
- [Catalog sources](docs/catalog.md): study methods, parameters and adaptations.
- [Architecture](docs/architecture.md): crates, synthesis, playback, the shared workspace and the desktop adapter.
- [Contributing](CONTRIBUTING.md): checks and dependencies.
- [Packaging and releases](docs/releasing.md): distribution contents, build identity, package checks and publication.
- [Changelog](CHANGELOG.md) and [security](SECURITY.md).

## License

Copyright © 2026 Ervins Strauhmanis. Each download contains product-specific dependency notices.

Software and documentation use MPL-2.0; third-party components retain their own terms. Owner artwork and drawing sources have separate proprietary terms in [LICENSE](LICENSE), with unchanged artwork permitted in original or modified Auditory Lab distributions. Editable sources and attribution are in the [artwork guide](artwork/README.md).

Portable recipes carry stable UUID/history, authored signed-offset pairs, current notes and scoped bibliography/evidence/lineage. Copies receive a fresh identity. The supported [recipe format](docs/recipes.md), [library layout and JSON reports](docs/cli.md) have independent versions.
