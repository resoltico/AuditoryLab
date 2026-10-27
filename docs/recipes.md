# Authoring recipes

A recipe describes ordered stereo components, finite duration/fades, identity and optional research context. It contains no sample rate. Inspect/export choose one; playback validates against the device’s actual rate. The [format reference](formats.md#recipe-schema) defines all fields, limits and the supported schema.

Start from a built-in:

```text
auditorylab recipe builtin:ioannou-2015-binaural-10-hz --output custom.toml
auditorylab inspect --file custom.toml --sample-rate 48000
```

Use **Edit a copy** or `library add --as-copy` to allocate a separate identity. Portable export retains the source identity. Keep your own notes in `[description]`; retain source methods and limitations in scoped evidence.

## A signed-offset pair

Save the example as UTF-8. It sends 250 Hz left and 240 Hz right:

```toml
schema_version = 4

[document]
id = "00000000-0000-4000-8000-000000000001"

[description]
title = "Ten-hertz pair example"

[sound]
duration_seconds = 2.0
fade_seconds = 0.01

[[sound.component]]
id = 17
kind = "binaural_pair"
left_hz = 250.0
offset_hz = -10.0
left_peak_dbfs = -24.0
right_peak_dbfs = -24.0
```

The UUID is an illustrative fixture. The positive component ID names this authored owner; a pair expands to primary/secondary generator lanes. Use an explicit pair when its carrier relationship is part of the sound.

## AM and gates

This component gives a 1 kHz carrier full-depth 40 Hz AM:

```toml
schema_version = 4
[document]
id = "00000000-0000-4000-8000-000000000002"
[sound]
duration_seconds = 2.0
fade_seconds = 0.01
[[sound.component]]
id = 1
kind = "tone"
ears = "both"
frequency_hz = 1000.0
peak_dbfs = -24.0
[sound.component.modulation]
kind = "am"
rate_hz = 40.0
depth = 1.0
```

For a gated tone, use modulation `kind = "gate"`, `rate_hz` and `width_seconds` instead of depth. For a click carrier, use component `kind = "clicks"`, its rate/width, ears and peak level. Modulation belongs to its component. [Component fields](formats.md#components) show the complete shapes.

## Check and save

Run `inspect --file custom.toml` after editing. A finite portable document can remain savable while failing acoustic validation at a chosen rate. Correct the named quantities before WAV export or playback.

Peak amplitudes add per ear. Lower component levels until their summed linear bounds fit full scale, including both voices of a monaural pair. These bounds protect against possible peaks rather than relying on phase cancellation. Endpoint fades require room within the finite duration.

Workbench typing stages values; committing applies the complete finite candidate. Exact fields keep values outside comfortable slider ranges. An independent copy gets a fresh identity and bounded ancestry; its first User Library save sets its declared dates. Sound changes demote current source evidence to ancestor scope; title/notes changes alone retain it.

## Waveforms

For frame `n` at rate `fs`, let `c` be phase in cycles, `d` AM depth and `A = 10^(peak_dbfs/20)`:

| Process | Sample rule |
| --- | --- |
| Tone | `A × sin(2πc)` |
| AM | Multiply the carrier by `(1 + d × sin(2πc_m)) / (1 + d)`. |
| Clicks | Gain 1 during each resolved open pulse, 0 otherwise. |
| Gate | Multiply the carrier by that click/pulse gain; carrier phase continues while closed. |
| Mix | Sum each ear’s voices in authored order, then apply the finite fade. |

Tone/AM phases use a 64-bit cycle with step `ceil(hz × 2^64 / fs)`, derived exactly from the binary64 requested frequency. Frame `n` uses `n × step` modulo `2^64`. Sine evaluation uses the signed top 53 phase bits as cycles in `[-0.5,0.5)`.

Pulse width resolves as `W = round(width_seconds × fs)`, halves away from zero. Pulses start on the first frame at or after a period onset and stay open for exactly `W` frames. With `K(n) = floor(n × rate_hz / fs)`, frames after the initial open span are open when `K(n) > K(n-W)`. [Rate validation](formats.md#validation-at-a-sample-rate) leaves a closed frame in every period.

With `N` total frames, `F` fade frames and `q = min(n,N-1-n)`, the fade is 1 when `F=0` or `q≥F`, otherwise `0.5 - 0.5×cos(πq/F)`. A resolved positive fade silences both endpoints. Export rounds stereo samples to binary32.

Clicks and gates have abrupt edges and broadband spectra. Requested and realized pulse quantities can differ after frame resolution. Inspect shows both. **Fit signal** changes the monitor’s axes, while audio samples and meters retain their values.

## Research context and catalog entries

Bibliography, evidence and lineage have distinct roles. A source identifies a citation/DOI/URL. Evidence states the studied population, condition, measures, methods and adaptations. Lineage records an ancestor and its relationship to the current sound. The [reference](formats.md#dates-evidence-and-ancestry) defines their fields.

A catalog condition needs full primary methods identifying waveform, carrier/rhythm rates, widths, continuous duration and presentation route. Read supplements and publisher corrections. Disclose selected protocol portions, speaker-to-ear adaptation, unstated phase/ramps/rate/calibration and any unavailable evidence. Describe findings for the stated population and condition, including null results and study size.

Add a lowercase `firstauthor-year-condition.toml` under [catalog](../crates/core/catalog). Bibliography keys refer to records under [studies](../crates/core/studies); citations live there, while condition-specific methods and adaptations stay with the condition. The build script creates standalone portable recipes. Add independent numerical expectations in [catalog tests](../crates/core/src/catalog/tests.rs), then run the [contributor checks](../CONTRIBUTING.md#checks). The [catalog reference](catalog.md) documents shipped conditions and their source limits.
