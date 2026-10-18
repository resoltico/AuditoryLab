# Architecture

Auditory Lab has a pure synthesis core, one shared application library and two frontends. Cargo's direct dependencies express the direction: CLI and GUI depend on `auditorylab`, which depends on `auditorylab-core`. The library re-exports the model types needed by callers. Frontends select input, present results and own their threads; domain decisions belong in the library and core.

| Package | Responsibility |
|---|---|
| [`auditorylab-core`](../crates/core) | Recipe schema, model, validation, catalog, seekable synthesis and waveform extrema. |
| [`auditorylab`](../crates/lib) | Loading, contextual errors, notices, WAV export, atomic publication and default-device playback. |
| [`auditorylab-cli`](../crates/cli) | clap grammar, terminal layout, exit codes and interrupts; binary `auditorylab`. |
| [`auditorylab-gui`](../crates/gui) | Slint controls, native dialogs, operation worker and scope paths. |
| [`xtask`](../xtask) | Archives, notices, smoke checks and release notes. |

## Recipe to signal

The core is `no_std` with `alloc`. `Preset` contains a `Stimulus` and optional `About` text. A component contains ear routing, peak level, a tone or click carrier and optional sinusoidal or rectangular modulation. A private serde representation keeps the strict schema independent of the public model.

Parsing checks byte length, UTF-8, schema version and TOML structure. Numeric values are validated by `Stimulus::signal(rate)`: duration, fade, fade overlap, component count, each component and finally each ear's peak sum. The resulting immutable `Signal` stores resolved frame counts and validated voices. Its clone can be shared across threads.

Tones and sinusoidal modulation have a 64-bit integer phase step, rounded upward from the exact binary64 frequency ratio. Frame n uses wrapping multiplication by n. Tones convert the phase's signed top 53 bits to cycles before evaluating portable `libm` sine. Pulse width is rounded to whole frames. Exact rational period counts place each onset on the first frame at or after it; comparing counts at n and n−W gives a constant open width. The exact closed-frame rule keeps successive pulses separate. Voices are summed in recipe order, then the raised-cosine fade applies. The [preset guide](presets.md#waveforms) defines the formulas.

`Signal::frame(n)` evaluates any frame directly. `render(start, out)` fills an arbitrary block, returns its active length and zeros the tail. It allocates nothing, locks nothing and performs no I/O. Each output owns its cursor. Integer phase, binary64 arithmetic and portable `libm` preserve sample bits across platforms and build profiles; pinned digests test that contract.

Catalog source files are embedded by the core build script in natural ID order. The public catalog table is generated from the directory. `waveform::trace` renders every frame of a clipped window into fixed stack scratch and reduces equal partitions to per-ear minima/maxima. Its caller reuses the output vector. Single-frame pulses reach their bins' extrema.

## Library boundaries

A loaded library `Preset` holds an origin and an `Arc` to the core recipe. `Preset::at(rate)` returns a `Sound` with an `Arc<Signal>`. Files are opened nonblocking on Unix, and admission checks the opened handle before bounded reading. Explicit readers retain stream semantics. Errors carry a category, contextual message, original cause and an optional leftover path; frontends branch on the category.

WAV encoding renders blocks and rounds to stereo binary32 samples. Header size arithmetic chooses RIFF or RF64. Export updates shared progress after each written block. Publication stages output in the destination directory, syncs it, checks cancellation and persists it with or without replacement. Failure cleanup owns only that partial file and names it if removal fails. CLI publication always refuses replacement; GUI replacement follows native Save confirmation.

Playback preparation binds the default device snapshot to the recipe validated at that device's rate. The prepared value and stream remain on the executing worker thread. The callback owns a signal, cursor and preallocated scratch, converts samples to the backend format, places left/right on channels 0/1 and silences extra channels. Completion uses backend timestamps. Shared terminal atomics use first publication wins; supervision polls cancellation and watches for stalls. Backend diagnostic storage stays separate from sample processing.

Progress stores the callback's starting source frame minus the reported output latency, floored at zero. Export uses the same progress type for written frames. Listening notices and version wording live in the library.

## Frontend threads and packaging

The CLI uses clap for source exclusivity and command grammar, textwrap for prose, and a byte-preserving recipe writer. Interrupt handling begins after input loading. One mapping from library error category selects the exit code.

The GUI's plain `App` state is separate from Slint wiring. One worker performs import, play or export; an mpsc channel returns the prepared playback sound and terminal result. A timer reads progress and traces the playback signal at the device rate, or the stopped preview at the export rate. Scope paths connect bin extrema on a fixed scale. Timer intervals account for trace cost, and idle operation stops the timer. Ordinary close cancels and joins the worker. Native dialogs own path selection and overwrite confirmation.

`cargo xtask dist` builds the distribution profile, assembles product notices, creates archives and smoke-tests versions, catalog listing and a silent export. macOS packaging joins the two slices and signs the app ad hoc. The reusable [CI workflow](../.github/workflows/ci.yml) supplies the release builds; [contributing](../CONTRIBUTING.md#packaging-and-releases) describes publication.
