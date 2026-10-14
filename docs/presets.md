# Presets

A preset is UTF-8 TOML describing stereo sound in hertz, seconds and dBFS. Its sample rate comes from inspection/export options or the playback stream configuration. Start with `auditory-lab show NAME > mine.toml` for a complete catalog entry, including its citation and adaptations.

## Editable examples

This two-second tone pair puts 250 Hz on the left and 240 Hz on the right:

```toml
schema_version = 1
duration_seconds = 2.0
fade_seconds = 0.01

[[component]]
ears = "left"
kind = "tone"
frequency_hz = 250.0
level_dbfs = -20.0

[[component]]
ears = "right"
kind = "tone"
frequency_hz = 240.0
level_dbfs = -20.0
```

A tone with full sinusoidal amplitude modulation:

```toml
schema_version = 1
duration_seconds = 1.0
fade_seconds = 0.01

[[component]]
ears = "both"
kind = "tone"
frequency_hz = 1000.0
level_dbfs = -20.0

[component.modulation]
kind = "sinusoidal"
rate_hz = 40.0
depth = 1.0
```

A 40 Hz train of 1 ms rectangular clicks:

```toml
schema_version = 1
duration_seconds = 1.0
fade_seconds = 0.01

[[component]]
ears = "both"
kind = "click"
rate_hz = 40.0
width_seconds = 0.001
level_dbfs = -20.0
```

These are engineering examples. A file with just `schema_version`, `duration_seconds` and `fade_seconds` produces silence. Validate a saved file with `inspect` before export or playback.

## Fields and schema

Input is at most 65,536 bytes. Unknown, missing, duplicated and mistyped keys are errors. Whole numbers are accepted for real-valued fields. This build reads only `schema_version = 1`; another numeric version is reported before unfamiliar fields are interpreted.

| Field | Meaning |
|---|---|
| `schema_version` | `1`. |
| `duration_seconds` | Stimulus length, resolved to frames. |
| `fade_seconds` | Fade length at each edge, resolved to frames; `0` disables fading. |
| `[[component]]` | Repeat to sum components. Omit for silence. |
| `[provenance]` | Optional descriptive metadata; all five strings below are required when present. |
| `provenance.title` | Short label in inspection/start notices; built-in titles appear in `list`. |
| `provenance.citation` | Source authors, year, title and publication. |
| `provenance.source_url` | Source link. |
| `provenance.description` | Stimulus role and findings reported by its source. |
| `provenance.adaptation` | Choices and differences between this preset and the source. |

Each component has `ears`, `kind`, `level_dbfs`, its kind's fields, and optionally `[component.modulation]` for the component written immediately above it.

| Field | Meaning |
|---|---|
| `ears` | `"left"`, `"right"`, or `"both"` for identical component samples in both channels. |
| `kind = "tone"` | Sine carrier with `frequency_hz`. |
| `kind = "click"` | Rectangular pulses with `rate_hz` and `width_seconds`. |
| `level_dbfs` | Component amplitude bound; 0 dBFS is full scale. Modulation does not raise the bound; samples need not attain it. |
| `modulation.kind = "sinusoidal"` | `rate_hz` and `depth` from 0 to 1. Available for either carrier kind. |

Provenance types are checked, but sources are not fetched or certified. Metadata does not affect samples. Keep `provenance.adaptation` accurate when editing parameters; validation does not check that its text matches them. Use `show` for the full metadata of a built-in condition.

## Waveforms and frame timing

For frame `n`, rate `fs` and nominal time `t = n / fs`, the waveform equations are:

- Tone: `sin(2π · frequency_hz · t)`, starting at phase zero.
- Click: `1` in the first `width_seconds` of each `1 / rate_hz` period, otherwise `0`, with a pulse at time zero.
- Sinusoidal modulation: multiply by `(1 + depth · sin(2π · rate_hz · t)) / (1 + depth)`, starting at phase zero and bounded by 1.
- Level: multiply each component by `10^(level_dbfs / 20)`.
- Routing: sum components per ear in file order, then apply the fade gain.

The implementation uses binary64 arithmetic. Tone/modulation phases advance as wrapped cycle accumulators across buffers. Click phase comes from the absolute frame index; rounding near a pulse boundary can select an adjacent sampled frame. The equations describe the waveforms rather than promising bit-exact equality to an independently evaluated time formula. [Architecture](architecture.md#core-model-and-rendering) describes rendering.

Duration and fade resolve by rounding the binary64 product `seconds · fs` to the nearest whole frame, with halfway values rounded away from zero. Let `N` be the total frames, `F` the fade frames and `d = min(n, N - 1 - n)`. For an active frame, fade gain is 1 when `F = 0` or `d >= F`; otherwise it is `0.5 - 0.5 · cos(π · d / F)`. Thus a positive resolved fade gives silent first/last frames, while a requested fade that rounds to zero applies no fade. Frames beyond the session are zero. Channels are left then right; export rounds samples to binary32.

## Rules checked at a sample rate

A preset can be valid at one rate and invalid at another. Inspect/export/play check:

- Finite, nonnegative duration/fade, resolving to 1 through 2^36 total frames, with `2 · F < N`.
- At most 16 components; finite `level_dbfs <= 0`.
- Finite, positive tone/click/modulation frequencies below half the rate. Modulated tones also require `frequency_hz + modulation.rate_hz < fs / 2`.
- Nonzero binary64 phase steps (`hz / fs`) for tones and modulation; positive representable subnormal steps are allowed.
- Normal phase steps for clicks, retaining relative timing precision. Click width is at least one frame and shorter than its period.
- Per-ear component amplitude bounds summing to at most full scale. Every rendered sample is finite and full-scale-bounded; no clipping is performed.

Inspection shows conservative summed bounds, not measured or necessarily attained maxima.

## Digital levels

For unfaded coherent intervals with orthogonal, non-DC spectral terms, a sine's RMS is 3.01 dB below its amplitude bound; a conventional 100% SAM tone is 7.27 dB below it. Coincident/overlapping spectral terms need separate calculation. Distinct tones add in power under the same conditions, so two tones each 3.01 dB lower match one tone's RMS.

For sampled clicks, use the actual active-frame fraction. Ideal duty gives `10 · log10(width_seconds · rate_hz)` dB below the bound: −13.98 dB for 1 ms/40 Hz. These are digital relationships. Ear-level sound pressure depends on the device, volume and transducer.

## Built-in catalog

Each entry implements selected source-described sound parameters and embeds its citation, study context and adaptations. Entries use 0.1 s fades and phase-zero tones/modulation; single-component bounds are −20 dBFS. Within-study level relationships are stated in the entry, including RMS-normalized summed tones.

The catalog reproduces digital stimuli with declared adaptations, not full study procedures. Baselines, condition ordering, tasks, EEG recordings, calibration and non-auditory stimulation are outside a preset. Descriptions attribute study findings, including null results, to their sources.

### Adding a built-in preset

Place lowercase `firstauthor-year-condition.toml` under `crates/auditory-core/catalog/` and add its name to the sorted list in `crates/auditory-core/src/catalog.rs`. Admit a condition only when its full-text methods specify waveform, ear routing, relevant carrier/modulation/click parameters and continuous duration. Sources with unstated maskers/routing/carriers, sweeps, live-measurement control or short recording-locked trials do not define a catalog condition.

Check corrections/retractions, write source-based description and explicit adaptations, and add the recipe to the independent table in `crates/auditory-core/tests/catalog.rs`. Follow [verification](verification.md) for changes affecting rendering or catalog delivery.
