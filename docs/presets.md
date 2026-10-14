# Presets

A preset is a UTF-8 TOML recipe for a stereo sound, in hertz, seconds and dBFS. It has no sample rate: `inspect` and `export` choose one, and playback uses the device's. `show NAME --output FILE` saves any built-in preset as a starting point.

## Examples

Two tones, 250 Hz on the left and 240 Hz on the right, for two seconds:

```toml
schema_version = 2
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
schema_version = 2
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
schema_version = 2
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

Input is at most 65,536 bytes. Unknown, missing, duplicated and mistyped keys are errors, and whole numbers are accepted where a real number is expected. This build reads `schema_version = 2`; a file stating another version is reported as such before its other fields are read.

| Field | Meaning |
|---|---|
| `schema_version` | `2`. |
| `duration_seconds` | Length of the sound. |
| `fade_seconds` | Length of the fade-in and fade-out at each end; `0` for none. |
| `[[component]]` | One sound source; repeat to add sources. |
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

| `[about]` field | Meaning |
|---|---|
| `title` | A short name, shown by `list`, `about`, `inspect` and the start line of export and playback. |
| `studied_for` | What the source studied the sound for, in a few plain words. |
| `summary` | For listeners: what the sound is, what was studied and what was found, in plain language. |
| `citation` | The source's authors, year, title and publication. |
| `source_url` | A link to the source. |
| `study` | For researchers: the sound's role in the source and the source's reported findings. |
| `adaptation` | Every choice and difference between this preset and the source. |

`[about]` text never affects the samples, and sources are neither fetched nor checked.

## Waveforms

For frame `n` at sample rate `fs`, with time `t = n / fs`:

- Tone: `sin(2π · frequency_hz · t)`, starting at phase zero.
- Click: `1` during the first `width_seconds` of each `1 / rate_hz` period and `0` otherwise, with a pulse starting at time zero.
- Sinusoidal modulation: the carrier times `(1 + depth · sin(2π · rate_hz · t)) / (1 + depth)`, starting at phase zero; it never exceeds 1.
- Rectangular modulation: the carrier times the click waveform for its `rate_hz` and `width_seconds`, so a modulated tone keeps its phase across the gaps.
- Peak: each component is multiplied by `10^(peak_dbfs / 20)`.
- Mixing: each ear sums its components in file order, then the fade gain applies.

Clicks and rectangular gates switch instantly, so their spectra are broadband; only their rates are checked against half the sample rate, and part of their energy aliases. A higher sample rate reduces the aliased part.

Arithmetic is binary64 with portable math functions, so a recipe renders the same bits on every platform. Tone and sinusoidal phases advance per frame and wrap at one cycle. Click and rectangular phases come from the absolute frame index; near a pulse edge, rounding can move the edge by one frame. The formulas describe the waveforms; they do not promise bit equality with another implementation.

Lengths become whole frames: `seconds · fs` rounded to the nearest frame, halves away from zero. With `N` frames, `F` fade frames and `d = min(n, N − 1 − n)`, the fade gain is 1 when `F = 0` or `d ≥ F`, otherwise `0.5 − 0.5 · cos(π · d / F)`. A positive resolved fade therefore makes the first and last frames silent; a requested fade that rounds to zero applies no fade. Channels are left then right; export rounds samples to 32-bit float.

## Rules checked at a sample rate

A recipe can be valid at one sample rate and invalid at another. `inspect`, `export` and `play` check, in order:

- Duration and fade are finite and not negative, the duration gives 1 to 2^36 frames, and the two fades leave at least one frame between them (`2F < N`).
- At most 16 components, each with a finite `peak_dbfs` of at most 0.
- Every frequency and rate is finite, positive and below half the sample rate. A tone with sinusoidal modulation also needs `frequency_hz + rate_hz` below half the sample rate.
- Tone and sinusoidal rates are large enough to advance the phase. Click and rectangular rates keep full relative timing precision, and each pulse lasts at least one frame and ends before the next period.
- Each ear's summed component peaks are at most full scale, so no sample ever needs clipping.

`inspect` reports these summed peaks per ear. They are bounds, not measured or necessarily reached maxima.

## Digital levels

`peak_dbfs` states a component's peak bound. Over complete steady cycles, an unmodulated tone's RMS is 3.01 dB below its peak bound. When the carrier frequency is above the modulation rate, a full-depth sinusoidally modulated tone is 7.27 dB below its peak bound over complete steady cycles. These relationships require distinct, non-DC spectral terms over the averaging interval; spectral coincidences need separate calculation. Distinct tones add in power over suitable steady intervals, so two tones each 3.01 dB lower match one tone's RMS under those conditions. Coincident frequencies can add coherently.

An ideal rectangular click train's RMS is its pulse peak plus `10 · log10(width_seconds · rate_hz)` dB. That duty-cycle change also applies to a gated tone relative to its ungated RMS when its open windows contain complete tone cycles, as in the catalog's tone pips; other gated-tone RMS values depend on carrier phase and frequency. The duty-cycle change is −13.98 dB for 1 ms at 40 Hz. For sampled pulses, use the actual active-frame fraction, and exclude endpoint fades from steady-state calculations. Digital levels do not establish loudness at the ear; that depends on the device, its volume and the headphones or speakers.

## Built-in catalog

Each built-in preset reproduces the sound of one condition from a published study, and its `[about]` table gives the plain-language summary, the citation, the study context and every adaptation. Built-in sounds use 0.1 s fades, phase-zero starts and a −20 dBFS peak per component unless the study's own level relationships require otherwise; those cases are explained in `adaptation`. Study procedures beyond the sound itself (baselines, condition order, tasks, recordings, calibration, light stimulation) are not part of a preset.

### Adding a built-in preset

A study's condition qualifies only when its full-text methods state the waveform, which ear hears what, the frequencies or rates and widths, and a continuous duration. Sweeps, sounds controlled by live measurements, unstated maskers or noise, unstated ear assignment and short recording-locked trials do not qualify. Check the source for corrections and retractions.

Name the file `firstauthor-year-condition.toml`, in lowercase, under `crates/auditory-core/catalog/`, and add the name to the list in `crates/auditory-core/src/catalog.rs`, which is in name order with numbers compared numerically, and its recipe to the independent table in `crates/auditory-core/tests/catalog.rs`. Write `[about]` values using printable ASCII characters from space through tilde, excluding double quotes, on one line per field:

- `studied_for` names what the source investigated, not a benefit.
- `summary` says what the listener hears, what the study tested on whom, what it found, including null results and the study's size, and whether the result comes from animals. It reports findings and never promises an effect. Where a disease is named, state that the sound is not an established treatment.
- `study` and `adaptation` are for researchers and use the source's terms.

Then run the [verification](verification.md) checks.
