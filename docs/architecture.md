# Architecture

Auditory Lab has a pure synthesis core, one shared application library and two frontends. Cargo's direct dependencies express the direction: CLI and GUI depend on `auditorylab`, which depends on `auditorylab-core`. The library re-exports the model types needed by callers. Frontends select input, present results and own their threads; domain decisions belong in the library and core.

| Package | Responsibility |
|---|---|
| [`auditorylab-core`](../crates/core) | Recipe schema, model, validation, catalog, seekable and live synthesis, recipe writing and waveform extrema. |
| [`auditorylab`](../crates/lib) | Loading, contextual errors, notices, WAV export, editable drafts, control ranges, shared presentation, atomic publication and default-device playback. |
| [`auditorylab-cli`](../crates/cli) | clap grammar, terminal layout, exit codes and interrupts; binary `auditorylab`. |
| [`auditorylab-gui`](../crates/gui) | Slint controls, native dialogs, operation worker and scope paths. |
| [`xtask`](../xtask) | Archives, notices, smoke checks and release notes. |

## Recipe to signal

The core is `no_std` with `alloc`. `Preset` contains a `Stimulus` and optional `About` text. A component contains ear routing, peak level, a tone or click carrier and optional sinusoidal or rectangular modulation. A private serde representation keeps the strict schema independent of the public model.

Parsing checks byte length, UTF-8, schema version and TOML structure. Numeric values are validated by `Stimulus::signal(rate)`: duration, fade, fade overlap, component count, each component and finally each ear's peak sum. The resulting immutable `Signal` stores resolved frame counts and validated voices. Its clone can be shared across threads.

Tones and sinusoidal modulation have a 64-bit integer phase step, rounded upward from the exact binary64 frequency ratio. Frame n uses wrapping multiplication by n. Tones convert the phase's signed top 53 bits to cycles before evaluating portable `libm` sine. Pulse width is rounded to whole frames. Exact rational period counts place each onset on the first frame at or after it; comparing counts at n and n−W gives a constant open width. The exact closed-frame rule keeps successive pulses separate. Voices are summed in recipe order, then the raised-cosine fade applies. The [preset guide](presets.md#waveforms) defines the formulas.

`Signal::frame(n)` evaluates any frame directly. `render(start, out)` fills an arbitrary block, returns its active length and zeros the tail. It allocates nothing, locks nothing and performs no I/O. Each output owns its cursor. Integer phase, binary64 arithmetic and portable `libm` preserve sample bits across platforms and build profiles; pinned digests test that contract.

Catalog source files are embedded by the core build script in natural ID order. The public catalog table is generated from the directory. `waveform::trace` renders every frame of a clipped window into fixed stack scratch and reduces equal partitions to per-ear minima/maxima. Its caller reuses the output vector. Single-frame pulses reach their bins' extrema.

## Live synthesis

`live::Target` holds up to 16 validated components and caller-chosen stable identities at one rate. `Component::validate` supplies individual row errors; target validation also checks the total peak sum. `Engine` uses 32 fixed slots so all 16 old generators can crossfade with their replacements. It allocates nothing while setting targets, rendering or stopping.

Tone and sinusoidal-modulation oscillators advance integer phase accumulators. Constant settings match the seekable signal after the start ramp. A 30 ms transition interpolates per-ear levels, phase steps and depth. Matching component identities and generator kinds retain phase; pulse or kind changes crossfade generators. Each transition connects validated endpoints with shared progress, bounding the per-ear sum. Changes during a transition coalesce to the newest waiting target. Stop queues silence after the current transition; its fade takes 30 ms and the request can span almost two ramps. The engine ends at `Signal::MAX_FRAMES`.

## Library boundaries

A loaded library `Preset` holds an origin and an `Arc` to the core recipe. `Preset::at(rate)` returns a `Sound` with an `Arc<Signal>`. Files are opened nonblocking on Unix, and admission checks the opened handle before bounded reading. Explicit readers retain stream semantics. Errors carry a category, contextual message, original cause and an optional leftover path; frontends branch on the category.

WAV encoding renders blocks and rounds to stereo binary32 samples. Header size arithmetic chooses RIFF or RF64. Export updates shared progress after each written block. Publication stages output in the destination directory, syncs it, checks cancellation and persists it with or without replacement. Failure cleanup owns only that partial file and names it if removal fails. CLI publication always refuses replacement; GUI replacement follows native Save confirmation.

Playback preparation binds the default device snapshot to values validated at that device's rate. The callback owns a `Source`: a finite `Arc<Signal>` with a cursor and optional stop fade, or a boxed live engine with a target mailbox. The box is allocated before stream creation. Both sources render into preallocated scratch, report completion by returning fewer frames than requested, and preserve the backend-timestamp drain logic. Output conversion uses channels 0/1 for left/right and silences additional channels.

`prepare_live` stores the first whole target in an `Arc<Mutex<Option<Target>>>` before starting the stream. `LiveControl::update` validates a draft at the device rate before replacing the waiting target. The callback uses `try_lock` once per buffer, skips busy or poisoned locks, and copies the newest target into the engine. It never blocks or frees memory. Whole-target replacement preserves the previous valid sound when an edit is invalid.

Supervision turns cancellation into an atomic stop request. A finite source fades with gain `1 − (t+1)/R`, with R capped at its remaining frames, and reports Cancelled after draining; the CLI retains exit130. A stopped live source reports Played. If completion does not follow Stop within two seconds, supervision forces silence and returns Cancelled. Both sources retain the 30-second stall watchdog; live playback has no overall deadline apart from the core's frame limit. Terminal atomics use first publication wins, and backend diagnostics stay outside sample processing.

`Draft` owns editable metadata and components with identities that are never reused within that draft. Slider ranges and default components live in the library. Saving serializes finite values through the core's schema-2 writer, checks the reader's byte limit, then uses staged publication. Shared title, research, duration and dBFS helpers keep frontend presentation consistent.

Progress stores the callback's starting source frame minus the reported output latency, floored at zero. Export uses the same progress type for written frames. Listening notices and version wording live in the library.

## Frontend threads and packaging

The CLI uses clap for source exclusivity and command grammar, textwrap for prose, and a byte-preserving catalog recipe writer. Interrupt handling begins after input loading. One mapping from library error category selects the exit code.

The GUI's plain `App` state is separate from Slint wiring. One worker performs import, finite or live playback, recipe saving or export; an mpsc channel returns the prepared sound/live control and terminal result. The Workbench keeps parser errors and pending field text separately from core validation. Enter and blur commit values; actions commit pending text before taking their snapshot. Incremental row models preserve focused fields, and a whole-draft replacement resets editors. Live Ready sends the current valid draft, including edits made while the device was preparing. A timer reads progress and traces the playback signal at the device rate, or the stopped preview at the export rate. Scope paths connect bin extrema on a fixed scale. Timer intervals account for trace cost, and idle operation stops the timer. Ordinary close requests cancellation and joins the worker. Native dialogs own path selection and overwrite confirmation.

`cargo xtask dist` builds the distribution profile, assembles product notices, creates archives and smoke-tests versions, catalog listing and a silent export. macOS packaging joins the two slices and signs the app ad hoc. The reusable [CI workflow](../.github/workflows/ci.yml) supplies the release builds; [contributing](../CONTRIBUTING.md#packaging-and-releases) describes publication.
