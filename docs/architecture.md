# Architecture

Four crates turn a recipe into validated stereo frames and deliver them to a file or the default audio output.

| Crate | Responsibility |
|---|---|
| `auditory-core` | The preset format, the built-in catalog, validation and sample generation. |
| `auditory-wav` | Stereo float RIFF/RF64 export to a new file. |
| `auditory-device` | Default-output discovery, listening advice, sample conversion, channel placement and playback supervision. |
| `auditory-lab` | The command line: arguments, reading input, presenting results, exit statuses and interrupts. |

The sinks depend on the core, and the command line on all three. Libraries own sound definitions, validation, rendering, export and playback policy. Front ends supply input and decide how to present library values and diagnostics.

## Core

`Preset` holds a `Stimulus` and optional `About` text. A stimulus is a list of components, each a tone or click carrier with ear routing, a peak level and an optional sinusoidal or rectangular modulation; see the [format](presets.md).

`Preset::from_bytes` checks byte length and UTF-8, then delegates to `Preset::parse`. The latter accepts `&str` and checks length, the schema version and strict structure. `Stimulus::session(rate)` checks timing, the component count, each component in order, then each ear's summed peak, and returns an opaque `Session`. Values can be invalid until this point.

A `Renderer` takes a session's frames from the first, filling `[[f64; 2]]` buffers, returning how many came from the session and zeroing the rest. Tone and sinusoidal phases are binary64 cycle accumulators that persist across buffers. Clicks and rectangular modulation share one pulse-train generator whose phase comes from the absolute frame index.

Carriers lie within ±1 and modulation and fade gains within 0 to 1, and each ear's peaks are summed in the order its samples are, so round-to-nearest arithmetic keeps every rendered sample within the validated bound without clipping. Rendering allocates nothing, takes no locks and performs no I/O, so it can run on an audio thread.

The core is `no_std` with `alloc` and uses `libm` for portable math, so the same recipe renders the same bits across supported targets; pinned digests in the tests check this wherever they run. The catalog's TOML files are compiled in with `include_str!`.

## Sinks

WAV export renders 4,096-frame blocks, rounds samples to 32-bit float, writes the header and samples, and syncs the file before reporting success. The session length decides between RIFF and RF64. [Usage](usage.md#export) states the file-ownership contract.

Device discovery saves one backend-reported default stream configuration; the session must match its rate, which does not certify the physical hardware rate. The audio callback converts samples, places left and right on the first two channels and silences the rest, using preallocated scratch. Callbacks and the supervising thread record one result, the first of completion, cancellation, failure or timeout. Callbacks silence a buffer when they observe that result; sound already queued to the backend cannot be recalled. Completion requires a callback at or after the latest backend estimate of when the last frame plays. `ListeningAdvice` decides the advice given before playback.

## Command line

Built-in names resolve through `catalog::find`; files and standard input share one bounded reader. The command line calls the libraries and presents their results: technical reports quote untrusted text, `about` wraps it, and one output boundary escapes control characters. `Failure` adds where a failure happened and its exit status. Interrupt handling starts only after input has been read.

## Release artifacts

[release.py](../scripts/release.py) defines the native targets. On Windows, [windows_runtime.py](../scripts/windows_runtime.py) builds with the pinned GNU LLVM toolchain, checks bundled startup/support inputs and selected members against their reviewed source identity, and binds the link trace and map to the executable bytes. [package.py](../scripts/package.py) requires that receipt alongside binary, import and extracted-artifact checks. [notices.py](../scripts/notices.py) consolidates the complete official Rust component notices, reviewed MinGW/LLVM native grants and ordinary crate notices; [releasing](releasing.md) describes the build and publication boundary.

## Changing the model

A new component or modulation kind needs its core variant, validation, generator and display text, the [format](presets.md) documentation, and tests that check it against independent formulas, its rejection boundaries, allocation-free rendering and buffer-partition independence, plus a pinned digest. Existing digests change only when their samples deliberately change. Sinks consume validated sessions, and the command line presents library-owned values; neither needs waveform-specific logic.
