# Desktop application

`auditorylab-gui` is the native desktop application. It uses the same `auditory-app` capabilities as the CLI for sound generation, validation and file publication.

## Build and start

Install the [pinned Rust compiler and native prerequisites](../README.md#install-from-source). Slint's runtime and UI compiler are resolved together by Cargo; no separate UI compiler installation is needed. Run builds from the repository root with its checked Cargo configuration. On macOS and Linux:

```sh
cargo build --locked -p auditory-gui
target/debug/auditorylab-gui
```

On Windows, use the native GNU LLVM host from the CLI setup, then run:

```text
python scripts/windows_runtime.py prepare --target x86_64-pc-windows-gnullvm
cargo build --locked -p auditory-gui --target x86_64-pc-windows-gnullvm
target\x86_64-pc-windows-gnullvm\debug\auditorylab-gui.exe
```

Substitute `aarch64-pc-windows-gnullvm` on Windows ARM64.

The preparation command verifies the locked system-import producer and creates its finite, unchanged library-name views under `target/windows-imports/TARGET`. It needs Python 3.9 or later and no Windows SDK. Run it again after dependency updates or removal of that directory, including before ordinary GUI tests. Existing unexpected or altered files reject; inspect the named owned output directory before removing it and preparing again. The GUI build script supplies this directory only to GUI binaries/tests; CLI builds do not use it. Qualified builds repeat the byte checks and inspect the actual link receipt. An ordinary Cargo build remains local/unqualified.

CLI and GUI packages require macOS 15 or later on Apple silicon and Intel. The desktop application's other platform requirements are Windows 10 on x86_64, Windows 11 on ARM64, and Ubuntu 22.04-class Linux. Ordinary Cargo source builds are local/unqualified and use toolchain deployment defaults; the [native qualifier](verification.md#product-identity-and-native-qualification) sets and checks the macOS package baseline. A declared minimum OS does not establish execution on that OS.

Linux needs a configured X11 or Wayland session, installed fonts, fontconfig and working accessibility/D-Bus services. On Ubuntu install the normal desktop runtime, `libasound2`, `libfontconfig1`, `libfreetype6`, `libxkbcommon0`, and a font package such as `fonts-dejavu-core`; X11 also needs `libxkbcommon-x11-0`. Native file pickers use a desktop portal with a suitable backend or Zenity, depending on the available picker route. A picker returning no selection does not diagnose whether its backend failed; manual path entry remains available.

Linux source builds additionally require `libfontconfig1-dev` and `libfreetype6-dev`. Keep distribution font libraries security-updated. The root Cargo configuration links system FreeType and prevents the bundled native fallback from compiling; running from another directory with `--manifest-path` or using alternate Cargo configuration does not establish that policy. Native qualification rejects unsupported build overrides.

Packaged macOS builds are `Auditory Lab.app` bundles signed ad hoc rather than notarized. Windows opens without a console. Linux includes a desktop file with optional installation instructions in its bundled README. No installer, elevated setup or updater is provided.

## Choose, inspect and export

Opening the window or selecting a sound never starts audio. The Compact session layout keeps the selected source, Signal through position, Play/Stop and recent per-ear peak ranges visible. Expand **Research context and recipe** for the citation, source, adaptations, declared routing and listening guidance. These declarations do not establish that a custom recipe's numbers are valid. Compact titles collapse whitespace and elide to the available width; research context retains the complete declared title. A whitespace-only title displays “No title.”

Expand **Open recipe, validate or export WAV** for file controls. Use **Browse…** or **Edit path**, then **Open Recipe**. File filters are a convenience: the application admits an opened regular file, including a symlink to one, then checks size, encoding and schema. Numeric playability is checked separately by Validate, Play or Export WAV. Pipes, devices and directories reject before reading. Successfully captured bytes become an immutable recipe snapshot; later file edits are ignored until you reopen it. A failed import identifies its own request and leaves the previous valid selection available. Regular-file admission does not promise an I/O deadline for stalled remote or failing filesystems, and capture is not an atomic filesystem snapshot.

Chosen native paths are kept separately from their quoted display text. **Edit path** replaces that authority with a manual draft; a non-Unicode native choice starts with an empty draft rather than lossy text. An empty or invalid draft cannot execute the old choice. **Cancel edit** restores the prior choice. The export-folder form follows the same rule. Browsing with no selection preserves the prior choice.

**Validate** checks at the displayed rate, initially 48,000 Hz, without opening an audio device. It reports exact frames, declared and resolved durations and fades, components and per-ear bounds before output conversion. Numeric rules remain in the shared application/core. Conversion can round or quantize a sample slightly above its generation bound; digital levels do not certify sound pressure.

**Export WAV** uses an existing folder and one new native filename. It captures the destination, rate and duration before execution. Existing files, directories and symlinks are never replaced; choose another filename on conflict. The shared [publication limits](usage.md#new-file-publication) cover caller-controlled directories, cancellation at commit, cleanup failures and crash leftovers.

## Play, stop and close

Lower output volume before pressing **Play**. Playback discovers and validates the actual output on its owning worker, then delivers that stream's preparation facts and listening notices to UI state before execution. This does not guarantee that pixels have been painted or a screen reader has finished announcing them.

One operation runs at a time. Source selection is locked throughout playback, and each Play starts at source frame zero. Other operations keep their captured source, preparation and completed result even if you select a different catalog description. Changing the stopped selection clears the old plot. A selection change cannot relabel an export or conceal its success; failures retain the requested destination without claiming that no file remains.

**Stop** requests cancellation. Scheduling and native calls can delay it, and already queued sound cannot be recalled. No timer invents an audible playhead or partial played-frame count.

Close keeps the window and event loop alive until the actual terminal result, worker cleanup and any pending native chooser settle. Ordinary macOS menu/Dock Quit follows the same path through the [Slint backend integration](../vendor/slint-winit/AUDITORYLAB.md). A successful file commit stays successful if cancellation arrives later. Force termination and OS logout/shutdown negotiation are not qualified; a cancelled OS termination request may abort logout.

## Generated signal

The plot observes actual mixed and faded left/right core `f64` samples before device conversion and channel placement. Samples are generated once. Generation can run ahead of playback, and cancellation may silence a buffer after its samples were observed. The plot therefore does not establish submission, acoustic output, sound pressure or what a listener currently hears.

**Signal through** identifies the last observed source coverage at the prepared stream rate. The separately reported latest generated cursor can be ahead when observations are lost. Neither is an audible playhead or seek control. Empty and preparing views have no synthetic trace. Waiting, stale and suspended observations are distinct from the real playback result. Completed views retain actual samples rather than extending them with a silent session tail.

Both ears use a fixed ±1 digital-amplitude scale. Amplitude ticks, zero references, traces and frame/time endpoints share one plot rectangle. Start and exclusive end come from the supplied series; approximate times are measured from source frame zero, with exact frame numbers retained. Each recent-view bar covers the supplied minimum and maximum in its actual integer-frame interval. This **Peak range** is not an analytical modulation envelope. Detail comes from retained raw samples. Missing and partly observed intervals stay blank and have an explicit gap summary; blank does not mean silence. Fewer than two samples are identified as insufficient for a waveform.

The recent history covers at most two seconds. **Show waveform detail** offers 2, 10, 40 and 100 ms windows. Window changes apply to subsequent eligible live captures; stopped views retain their captured extent. For held displays, a change takes effect after **Refresh display** obtains new coverage.

**Freeze display** leaves playback running and holds the current trace; **Resume display** resumes eligible updates. **Reduced motion** retains a static real snapshot. **Refresh display** requests new coverage while preserving the held mode; it needs active playback and an eligible window, so a request is not an immediate new snapshot.

Live updates run only while the main window is focused, unminimized and not reported occluded by the OS. This also suspends an unfocused window that remains visible; OS occlusion reporting varies. Held or inactive displays suspend observation retention. Resuming waits for fresh coverage instead of presenting queued data as live, and lost intervals remain explicit. Updates are capped at 25 per second; idle windows do not poll.

Observation is bounded and lossy, and audio never waits for drawing. If display capacity is exceeded, the plot clears and suspends while the real operation and result remain available. The [architecture](architecture.md#generated-signal-observation) defines transport, history, snapshot and presentation capacity accounting; its monitoring budget is not a process-memory or GPU-memory limit.

## Rendering, keyboard and accessibility

The Slint Winit profile enables native accessibility, FemtoVG/WGPU drawing and software recovery. Compatible systems prefer one compatible GPU through Metal, Direct3D or Vulkan. Linux GPU drawing needs an installed Vulkan loader and driver. UI rendering does not offload sound generation or use every GPU/CPU core. Lower CPU use alone does not prove lower total energy use.

For graphics startup recovery, launch the built executable with a per-process `SLINT_BACKEND=winit-software`. For example, on macOS/Linux:

```sh
SLINT_BACKEND=winit-software target/debug/auditorylab-gui
```

On Windows set that environment value for the launched process. For a macOS bundle, launch its `Contents/MacOS/auditorylab-gui` executable. Software drawing still needs the platform's window/display and keyboard libraries. Do not pass runtime renderer variables to `cargo build`: its controlled compiler environment rejects unsupported Slint overrides, including changes that could embed external source or resources.

Early renderer construction or no-compatible-adapter preflight can fall back automatically. Later surface initialization and device loss do not promise a software switch. Errors preserve active-worker cleanup and completed file context; the program does not auto-restart. Compiling a GPU renderer or requesting it does not prove that a frame used it.

Standard controls support keyboard focus and activation. Research and diagnostic text can be selected and copied. **Help → About Auditory Lab** includes build identity, source/license information and the standard AboutSlint attribution. Local builds are unqualified and claim no immutable source revision; candidate and release identities include their checked source and target.

Accessibility roles, names, values, focus and reading order need native platform checks. Enabling a feature or passing mock-backend checks does not establish screen-reader support. [Verification](verification.md) distinguishes native interaction, virtual-display startup, headless checks and assistive-technology evidence.
