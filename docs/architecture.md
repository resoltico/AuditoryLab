# Architecture

Four crates turn descriptive input into validated stereo frames and deliver them to a file or the configured default output.

| Crate | Responsibility |
|---|---|
| `auditory-core` | Preset format, catalog, stimulus validation and sample generation. |
| `auditory-wav` | Stereo float RIFF/RF64 export and destination ownership. |
| `auditory-device` | Configured-output discovery, conversion, placement and supervision. |
| `auditory-lab` | Arguments, input reading, presentation, streams, exit statuses and interrupts. |

Sinks depend on core; the CLI depends on all three. Domain rules, defaults, limits and messages belong to the libraries. The CLI supplies input and presents library results.

## Core model and rendering

`Preset` contains a descriptive `Stimulus` and optional `Provenance`. Components have explicit `Ears` routing and dBFS amplitude bounds. Tone/click carriers can use sinusoidal modulation; see the [format](presets.md).

Parsing checks bounded UTF-8, strict structure and schema version first. `Stimulus::session(rate)` checks timing, component count, components in order, then per-ear summed bounds. It returns an opaque `Session` with private validated voices/envelope. Descriptive values can be invalid before this boundary.

`Renderer` consumes session frames from the beginning, fills `[[f64; 2]]`, returns the active count and zeroes the tail. Its cursor advances once per active frame. Tone/modulation phases persist across buffers as wrapped binary64 cycle accumulators; clicks compute phase from the absolute frame index. Floating-point rounding can select an adjacent click frame near a boundary.

Each ear's amplitude bound and samples are summed in the same component order. Carriers stay within ±1, modulation/fades within 0–1, and a computed bound above full scale is rejected. Round-to-nearest arithmetic preserves that computed bound. Validated rendering produces finite full-scale-bounded samples without clipping. The conservative bound need not be attained.

Rendering allocates nothing, takes no locks and performs no I/O. Construction/cloning can allocate. Core uses `no_std` with `alloc` and explicit portable `libm` arithmetic with default features disabled. Review production imports/dependencies and renderer ownership; `no_std` removes implicit std linkage/prelude but does not prohibit explicitly linking std. Fixed-fixture digests check rendered identity on each platform where the tests execute.

The sorted catalog includes its TOML through `include_str!`. Source files are packaged with the core crate and embedded in the executable; no runtime catalog directory or source fetch is required.

## Sinks

WAV export renders 4,096-frame blocks, rounds to binary32, writes the stereo header/payload and syncs before completion. Sizes select RIFF/RF64. [Usage](usage.md#export-and-ownership) defines create-new and exclusive pathname ownership.

Device discovery queries the backend's default stream configuration and stores an `OutputDescription` with the CPAL handle. Queries may open backend resources; they do not start this app's playback stream or certify the physical hardware rate. The selected configuration requires at least two channels and a supported representation/rate; the session must match it. The callback uses preallocated stereo scratch, converts samples, places left/right on leading channels and zeroes extras.

Callbacks and supervision publish one terminal result through `OnceLock`: completion, cancellation or failure. Reading is nonblocking; concurrent publication attempts can wait for the result being initialized. Callbacks return silence when they observe a terminal result. Audio already queued to the backend cannot be recalled.

Completion requires a silent callback at or after the latest backend estimate of the session end. Nanosecond rounding does not move that estimate earlier. Backward clocks and malformed buffers fail. Progress, underruns, route changes and timeout policy belong to the sink; backend timing does not measure acoustic delivery.

## CLI

Names resolve through `catalog::find`; files/stdin share a bounded reader. The CLI invokes core parsing/validation, then presents descriptive values and library `Display` messages. It generates no samples or waveform/normalization rules.

`Failure` adds origins, applicable recovery hints and statuses. One terminal boundary protects output. Notices are best effort, reporting cannot undo completed operations, and closed pipes stop reporting quietly. A terminal-only stdin cue precedes reading; export/play install interrupt handlers after it finishes.

## Changing the model

For a component or modulation kind, update its core variant, validation, private generator and display, the [format](presets.md), and independent formula/boundary/allocation/partition tests. Add deterministic coverage for the affected waveform; preserve existing digests unless their samples intentionally change. CLI/sinks consume validated frames without kind-specific sample logic.

Catalog additions follow the [curation criteria](presets.md#adding-a-built-in-preset). Run the applicable [verification](verification.md) and update affected current documentation with the implementation.
