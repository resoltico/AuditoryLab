# Recipes

A recipe is UTF-8 TOML describing a stereo sound, in hertz, seconds and dBFS. It has no sample rate: `inspect` and `export` choose one, and playback uses the device's. `auditorylab recipe RECIPE_ID --output PATH` saves any built-in recipe as a starting point.

The desktop [Workbench](desktop.md#workbench) also writes schema-3 recipes. Its saved files retain the edited title and own notes, with one original source stimulus and its research context stored separately. Inspect them with `auditorylab inspect --file PATH` before playback or export.

## Examples

Save an example as `custom.toml` in UTF-8, then run `auditorylab inspect --file ./custom.toml` to validate it without opening an audio device.

Two tones, 250 Hz on the left and 240 Hz on the right, for two seconds:

```toml
schema_version = 3
duration_seconds = 2.0
fade_seconds = 0.01

[[component]]
ears = "left"
kind = "tone"
frequency_hz = 250.0
peak_dbfs = -20.0

[[component]]
ears = "right"
kind = "tone"
frequency_hz = 240.0
peak_dbfs = -20.0
```

A 1000 Hz tone whose amplitude swells and fades 40 times a second:

```toml
schema_version = 3
duration_seconds = 1.0
fade_seconds = 0.01

[[component]]
ears = "both"
kind = "tone"
frequency_hz = 1000.0
peak_dbfs = -20.0

[component.modulation]
kind = "sinusoidal"
rate_hz = 40.0
depth = 1.0
```

A 5 kHz tone switched on for 1 ms at the start of every 25 ms, and a 40 Hz train of 1 ms clicks:

```toml
schema_version = 3
duration_seconds = 1.0
fade_seconds = 0.01

[[component]]
ears = "left"
kind = "tone"
frequency_hz = 5000.0
peak_dbfs = -20.0

[component.modulation]
kind = "rectangular"
rate_hz = 40.0
width_seconds = 0.001

[[component]]
ears = "right"
kind = "click"
rate_hz = 40.0
width_seconds = 0.001
peak_dbfs = -20.0
```

A file with only `schema_version`, `duration_seconds` and `fade_seconds` is silence.

## Fields

Input is bounded by [`Recipe::MAX_BYTES`](../crates/core/src/recipe.rs). `schema_version`, `duration_seconds` and `fade_seconds` are required; components and `[about]` are optional. Unknown or duplicated keys, missing required fields and values of the wrong type are errors. Whole numbers are accepted where a real number is expected. The required `schema_version` is `3`; other version values are rejected.

| Field | Meaning |
|---|---|
| `schema_version` | `3`. |
| `duration_seconds` | Length of the sound. |
| `fade_seconds` | Length of the fade-in and fade-out at each end; `0` for none. |
| `[[component]]` | Optional sound source; repeat to add sources. |
| `[about]` | Optional text describing the sound; `title` is required when present. |

Each component has `ears`, `kind`, `peak_dbfs` and the fields of its kind, and optionally a `[component.modulation]` table, which applies to the component written just above it.

| Component field | Meaning |
|---|---|
| `ears` | `"left"`, `"right"`, or `"both"` for the same samples in both channels. |
| `kind = "tone"` | A sine wave of `frequency_hz`. |
| `kind = "click"` | Rectangular pulses: `rate_hz` per second, each `width_seconds` long. |
| `peak_dbfs` | The component's peak amplitude; 0 dBFS is full scale. Modulation never raises it. |
| `modulation.kind = "sinusoidal"` | Amplitude swelling at `rate_hz` with `depth` from 0 to 1. |
| `modulation.kind = "rectangular"` | The component switched on for `width_seconds` at the start of each `1 / rate_hz` period, and off otherwise. |

Within `[about]`, `title` is required and `summary` is optional plain notes about the current sound. `[about.research]` optionally describes research about that exact stimulus: `studied_for`, `summary`, `citation`, `source_url`, `study` and `adaptation` are optional text fields. Notes and research have different subjects when a sound is edited.

A derivative stores one `[source]` content anchor containing `duration_seconds`, `fade_seconds`, `[[source.component]]`, optional `[source.about]` and `[source.about.research]`. It has no schema version, nested source, path or runtime identity. **Show source** uses this captured content after reopen, even if the original file changes or disappears. Copies of a derivative retain its original anchor and preserve their own notes. Research remains visibly attributed to that source.

The whole UTF-8 document, including multibyte notes and source content, is limited to 65,536 bytes. Saving rejects nonfinite numbers in either current or source content before creating a destination; repair nonfinite source documents before deriving them. Finite acoustically invalid values remain serializable for repair. Source content need not be renderable at the selected rate. Parsing is structural, and rejects prior/future schemas and unknown or recursive source fields. No migration reader is provided.

Normal Unicode, escaped quotes and multiline TOML strings are supported. Source URLs are displayed without fetching or certifying their content. Human-readable CLI output escapes terminal controls and bidirectional formatting controls; raw recipe output preserves the generated recipe bytes.

## Waveforms

For frame `n` at sample rate `fs`, tones and sinusoidal modulation use a 64-bit integer phase: one cycle is `2^64` units. Its step is `ceil(hz × 2^64 / fs)`, computed exactly from the binary64 frequency. Frame `n` uses `n × step` modulo `2^64`. The frequency resolution is `fs / 2^64`; any finite positive frequency below Nyquist has a positive step.

- Tone: `sin(2π × c)`, where `c` is the signed top 53 phase bits converted exactly to cycles in `[-0.5, 0.5)`.
- Click: let `W = round(width_seconds × fs)`, halves away from zero. A pulse starts on the first frame at or after each period's onset and stays open for exactly `W` frames. Pulses use the exact binary64 rate: with `K(n) = floor(n × rate_hz / fs)`, a frame after the initial open span is open exactly when `K(n) > K(n − W)`.
- Sinusoidal modulation: multiply by `(1 + depth × sin(2π × c_m)) / (1 + depth)` at the modulation phase.
- Rectangular modulation: multiply by the click waveform at its rate and width. A tone keeps its phase across closed gates.
- Peak: multiply each component by `10^(peak_dbfs / 20)`.
- Mixing: sum each ear's components in file order, then apply the fade gain.

Clicks and rectangular gates switch instantly and have broadband spectra. Rate validation checks Nyquist; a higher sample rate reduces aliasing. Integer phases, binary64 arithmetic and portable `libm` give identical sample bits across platforms and build profiles. Any frame can be evaluated directly.

Lengths become whole frames: `seconds · fs` rounded to the nearest frame, halves away from zero. With `N` frames, `F` fade frames and `d = min(n, N − 1 − n)`, the fade gain is 1 when `F = 0` or `d ≥ F`, otherwise `0.5 − 0.5 · cos(π · d / F)`. A positive resolved fade therefore makes the first and last frames silent; a requested fade that rounds to zero applies no fade. Channels are left then right; export rounds samples to 32-bit float.

## Rules checked at a sample rate

A recipe can be valid at one sample rate and invalid at another. `inspect`, `export` and `play` check these rules:

- Duration and fade are finite and not negative, duration gives at least one frame within [`Signal::MAX_FRAMES`](../crates/core/src/signal.rs), and the fades leave at least one frame between them (`2F < N`).
- Component count stays within [`Stimulus::MAX_COMPONENTS`](../crates/core/src/stimulus.rs), and each component has a finite `peak_dbfs` of at most 0.
- Every frequency and rate is finite, positive and below half the sample rate. A tone with sinusoidal modulation also needs `frequency_hz + rate_hz` below half the sample rate.
- Pulse widths must give a whole-frame count `1 ≤ W ≤ Signal::MAX_FRAMES` (2^36 frames) and satisfy `(W + 1) × rate_hz ≤ fs`, compared exactly. This leaves at least one closed frame in every period, including fractional-frame periods.
- Each ear's summed component peaks are at most full scale, so no sample ever needs clipping.

`inspect` reports these summed peaks per ear. They are bounds, not measured or necessarily reached maxima.

## Digital levels

`peak_dbfs` states a component's peak bound. Over complete steady cycles, an unmodulated tone's RMS is 3.01 dB below its peak bound. When the carrier frequency is above the modulation rate, a full-depth sinusoidally modulated tone is 7.27 dB below its peak bound over complete steady cycles. These relationships require distinct, non-DC spectral terms over the averaging interval; spectral coincidences need separate calculation. Distinct tones add in power over suitable steady intervals, so two tones each 3.01 dB lower match one tone's RMS under those conditions. Coincident frequencies can add coherently.

An ideal rectangular click train's RMS is its pulse peak plus `10 · log10(width_seconds · rate_hz)` dB. That duty-cycle change also applies to a gated tone relative to its ungated RMS when its open windows contain complete tone cycles, as in the catalog's tone pips; other gated-tone RMS values depend on carrier phase and frequency. The duty-cycle change is −13.98 dB for 1 ms at 40 Hz. For sampled pulses, use the actual active-frame fraction, and exclude endpoint fades from steady-state calculations. The validated linear/decibel bounds apply before sink conversion; nearest-f32 and signed integer quantization have separate spacing/rounding rules and can change a peak slightly. The [encoder](../crates/lib/src/wav/export.rs) rounds samples to binary32. Loudness at the ear depends on the device, its volume and the headphones or speakers.

## Built-in catalog

Each built-in recipe is a parameterized audio adaptation of a selected condition from a published study. Its `[about.research]` table distinguishes stated source parameters, unspecified details and explicit synthesis or presentation choices. These recipes do not reproduce the original apparatus, calibrated acoustic levels or every waveform detail. The [catalog reference](catalog.md) documents the primary methods, supplements, adaptations and remaining evidence limits for the shipped recipes. Built-in sounds use 0.1 s fades, phase-zero starts and a −20 dBFS peak per component unless the study's own level relationships require otherwise; those cases are explained in `adaptation`. Study procedures beyond the sound itself (baselines, condition order, tasks, recordings, calibration, light stimulation) are not part of a recipe.

### Adding a built-in recipe

A condition qualifies only when its full primary methods give enough evidence to identify the waveform, frequencies or rates and widths, continuous duration, and original presentation. Headphone routing must be stated; a documented speaker presentation may be explicitly adapted to identical ear channels, without claiming that this recreates the acoustic field. Selecting one stated ear or EEG session from a wider protocol must also be disclosed. Unexplained routing guesses, unreported carrier or pulse parameters, sweeps, live-controlled sounds, unstated maskers/noise and short recording-locked trial reconstructions do not qualify. Record missing phase, ramps, audio rate and calibration instead of inventing them. Check supplements and publisher corrections/retractions, and state unavailable evidence honestly.

Name the file `firstauthor-year-condition.toml`, in lowercase, under `crates/core/catalog/`, and add its recipe to the independent table in `crates/core/tests/catalog.rs`. The build script discovers conditions in natural order and generates standalone schema-3 recipes. Conditions use `[about.research] source = "firstauthor-year"` to refer to a citation/DOI record under `crates/core/studies/`. Citation and URL exist only in that study record; condition-specific methods, findings and adaptations remain in the condition. Add an independent numerical expectation for each condition in `crates/core/tests/catalog.rs`. Text supports Unicode, correctly escaped quotes and multiline TOML strings.

- `studied_for` names what the source investigated, not a benefit.
- `summary` says what the listener hears, what the study tested on whom, what it found, including null results and the study's size, and whether the result comes from animals. It reports findings and never promises an effect. Where a disease is named, state that the sound is not an established treatment.
- `study` and `adaptation` are for researchers and use the source's terms.

Then run the [contributor checks](../CONTRIBUTING.md#checks).
