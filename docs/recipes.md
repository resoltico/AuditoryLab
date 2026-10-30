# Authoring recipes

A recipe describes ordered stereo components, finite duration/fades, identity and optional research context. It contains no sample rate. Inspect/export choose one; playback validates against the device’s actual rate. The [format reference](formats.md#recipe-schema) defines all fields, limits and the supported schema.

Create a generic document with `auditorylab new tone`, `pulse-train`, `binaural-pair`, `monaural-pair` or `pulsed-tone`. The command prints a complete schema-5 recipe with a fresh identity; optional `--output PATH` never overwrites. Desktop Add component uses the same constructors. New blank Workbench documents remain silent.

Desktop **Start here → Create a pulsed tone** captures the same [pulsed-tone constructor](#sinusoidal-and-pulsed-modulation) as a complete editable document. It has no research attribution and does not play or save automatically. The guide’s band pictures are normalized sine illustrations; Workbench’s sound-envelope previews describe the actual modulation.

Or start from a built-in:

```text
auditorylab recipe builtin:ioannou-2015-binaural-10-hz --output custom.toml
auditorylab inspect --file custom.toml --sample-rate 48000
```

Use **Edit a copy** or `library add --as-copy` to allocate a separate identity. Portable export retains the source identity. Keep your own notes in `[description]`; retain source methods and limitations in scoped evidence.

## A pair with a negative beat offset

Save the example as UTF-8. It sends 250 Hz left and 240 Hz right:

```toml
schema_version = 5

[document]
id = "00000000-0000-4000-8000-000000000001"

[description]
title = "Ten-hertz pair example"

[timing]
duration_seconds = 2.0
fade_seconds = 0.01

[[sound.component]]
id = 17

[sound.component.source]
kind = "binaural_pair"
first_hz = 250.0
offset_hz = -10.0
first_peak_dbfs = -24.0
second_peak_dbfs = -24.0

[sound.component.modulation]
kind = "none"
```

The UUID is an illustrative fixture. The positive component ID names this authored owner in Workbench, diagnostics and reports, independently of row position. A pair expands to primary/secondary generator lanes, with the secondary described as its second tone. Use an explicit pair when its carrier relationship is part of the sound.

## Sinusoidal and pulsed modulation

This component gives a 1 kHz carrier full-depth 40 Hz AM:

```toml
schema_version = 5
[document]
id = "00000000-0000-4000-8000-000000000002"
[timing]
duration_seconds = 2.0
fade_seconds = 0.01
[[sound.component]]
id = 1

[sound.component.source]
kind = "tone"
route = "both"
frequency_hz = 1000.0
peak_dbfs = -24.0

[sound.component.modulation]
kind = "sinusoidal"
rate_hz = 40.0
depth = 1.0
```

**Pulsed tone (isochronic)** means a single sine carrier under periodic gain pulses, with continuous carrier phase through the silent windows. It uses a `tone` source plus shared `pulsed` modulation; `pulsed-tone` is its CLI construction key. A unipolar pulse-train source and a pulsed pair keep their own names.

Create `auditorylab new pulsed-tone --output pulse.toml`. Its generic defaults are a 440 Hz carrier in both channels at −30 dBFS, 10 pulses/s, 50% requested duty and 2 ms raised-cosine rise/fall. It carries no study or benefit claims. Add the same shared modulation to either explicit pair kind; a generic pulse source is not a tonal construction.

The modulation table uses `kind = "pulsed"` and a nested pulse with `rate_hz`, `width` and `shape`; see [component fields](formats.md#components). Pulse sources use that identical pulse definition under their source table.

A fixed-duration width stays fixed when rate changes. Duty mode stores a fraction of the nominal period and changes the window with rate. Converting modes uses `seconds × rate_hz` or `fraction / rate_hz` atomically, without clamping. Binary64 round trips need not recover the original bits or boundary frame. Related staged rate/width text participates in conversion; unrelated staged text survives. Turning rectangular edges into raised cosine initializes each edge to 10% of the requested window, retaining a diagnosed candidate when positive edges cannot resolve at the chosen rate.

## Check and save

Run `inspect --file custom.toml` after editing. A finite portable document can remain savable while failing acoustic validation at a chosen rate. Correct the named quantities before WAV export or playback.

Peak bounds add per digital channel. Lower component levels until their summed linear bounds fit full scale, including both lanes of a monaural pair. These bounds protect against possible peaks rather than relying on phase cancellation. Endpoint fades require room within the finite duration.

Workbench typing stages values; committing applies the complete finite candidate. Numeric fields accept values outside comfortable slider ranges without gesture quantization; [display-unit conversion](desktop.md#build-and-edit-a-sound) uses binary64. An independent copy gets a fresh identity and bounded ancestry; its first User Library save sets its declared dates. Sound and finite-timing changes demote current source evidence to ancestor scope; title/notes changes alone retain it.

## Waveforms

Tone and sinusoidal-AM phases use a 64-bit cycle with step `ceil(hz × 2^64 / fs)`, derived from the exact binary64 requested frequency. Origin-zero frame `n` uses `n × step` modulo `2^64`. Portable libm evaluates sine from the signed top 53 phase bits as cycles in `[-0.5,0.5)`. Each tone starts at phase zero and continues through closed pulses. There is no per-pulse phase reset.

Evaluate lanes in source order (first/left, then second/right), applying levels `A = 10^(peak_dbfs/20)` and digital routes to form a stereo component sum. Multiply that sum by its one shared modulation gain. Sum components in authored order, then apply the finite or interactive outer gain. Levels are not normalized. Active sinusoidal AM uses `(1 + depth × sin(phase)) / (1 + depth)` and has an upper-sideband guard for each tonal lane.

Pulse onsets are `ceil(k × fs / rate_hz)` using the requested binary64 rate as an exact binary rational, without accumulating floating-point periods. The resolved window is constant:

- Duration: `W = round_away(seconds × fs)`.
- Duty: `W = round_away((fraction × fs) / rate_hz)`, in that evaluation order.

Halves round away from zero. Require `1 ≤ W ≤ 2^36` and the exact `(W+1) × rate_hz ≤ fs` rule, leaving a closed frame. Duty is strictly between zero and one. Fractional periods can alternate integer onset spacings; `W/fs × rate_hz` is a derived **nominal** duty, not identical duty in every integer interval.

Rectangular gain is one on local frames `0..W-1` and zero elsewhere. Raised cosine resolves positive rise/fall durations to `A`/`R` frames using the same rounding. Require `A≥1`, `R≥1`, `A+R≤W-1`; positive sub-frame edges are rejected rather than replaced by rectangular edges. At local frame `i`, use `(1-cos(πi/A))/2` for `i<A`, `(1-cos(π(W-1-i)/R))/2` when `W-1-i<R`, otherwise one. The regions do not overlap below unity. `W=3,A=R=1` gives `[0,1,0]`.

The total window includes zero-valued shaped endpoints; it is not the count of strictly nonzero samples. Calculated previews retain exact discrete extrema, including narrow pulses. Pulse trains are unipolar and retain DC. Rectangular and smooth sampled pulses can alias: raised-cosine edges are not band limited, and smoothing does not reduce every individual spectral bin. No automatic filtering, high-pass or normalization is applied. Independent discrete Fourier tests check expected DC, carrier/sidebands and specified pulse coefficients.

With `N` finite frames, `F` fade frames and `q=min(n,N-1-n)`, the overall fade is one when `F=0` or `q≥F`, otherwise `0.5-0.5×cos(πq/F)`. Positive finite fades silence both endpoints. Finite timing governs Library playback and export; live audition remains open-ended with named 30 ms startup/change/Stop transitions. Kind, pulse timing/width/shape changes restart and crossfade the entire component. Eligible carrier/AM frequencies glide with retained phase; levels/routes/depth ramp linearly. Facts-only changes that compile to the same audio keep phase and start no ramp. Settled finite/live output is bit-identical under identical origins, parameters and outer gain.

Export converts stereo samples to binary32. Peak/RMS and dBFS describe digital values, not perceived loudness or calibrated SPL. **Fit signal** changes only monitor axes. A static timing preview is origin-zero target data; recorded monitor samples and output-position estimates are separate quantities.

## Research context and catalog entries

Bibliography, evidence and lineage have distinct roles. A source identifies a citation/DOI/URL. Evidence states the studied population, condition, measures, methods and adaptations. Lineage records an ancestor and its relationship to the current sound. The [reference](formats.md#dates-evidence-and-ancestry) defines their fields.

A catalog condition needs full primary methods identifying waveform, carrier/rhythm rates, widths, continuous duration and presentation route. Read supplements and publisher corrections. Disclose selected protocol portions, speaker-to-ear adaptation, unstated phase/ramps/rate/calibration and any unavailable evidence. Describe findings for the stated population and condition, including null results and study size.

To add a catalog condition:

1. Add a lowercase `firstauthor-year-condition.toml` under [catalog](../crates/core/catalog). Keep condition-specific methods and adaptations with the condition. Its primary bibliography key identifies a record under [studies](../crates/core/studies), where the citation and DOI URL live.
2. Register the condition in `ENTRIES` and its study in `STUDIES` in the [catalog module](../crates/core/src/model/catalog.rs). Keep condition IDs in curated natural order. The directory-consistency checks require both lists to match their embedded source files.
3. Account for runtime assembly. The study supplies citation, DOI and URL; evidence records without `limitations` receive the shared limitation statement. Supply a condition-specific `limitations` value when it needs a distinct qualification. Condition files are assembly inputs; use `recipe builtin:ID` to obtain the complete portable document.
4. Add independent numerical expectations in [catalog tests](../crates/core/src/model/catalog/tests.rs), then run the [contributor checks](../CONTRIBUTING.md#checks). The [catalog reference](catalog.md) describes shipped conditions and their source limits.
