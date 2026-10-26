# Recipes

A recipe is UTF-8 TOML describing a stereo sound, in hertz, seconds and dBFS. It has no sample rate: `inspect` and `export` choose one, and playback uses the device's. `auditorylab recipe builtin:CATALOG_ID --output PATH` saves any built-in recipe as a starting point.

The desktop [Workbench](desktop.md#workbench) writes the same strict schema-3 document as the CLI reader. Identity, current notes, bibliography, evidence scope and flat ancestry are portable. Exporting a saved recipe preserves its UUID; **Edit a copy** creates a fresh identity and retains attribution without inherited validation. Inspect custom files with `auditorylab inspect --file PATH` before playback or export.

Recipe schema **3**, defined by [`Recipe::SCHEMA_VERSION`](../crates/core/src/recipe.rs), describes the portable UTF-8 TOML document. Unsupported schema versions and unknown fields are rejected.

## Examples

These UUIDs are illustrative fixtures. Use **New** or **Edit a copy** to allocate a fresh random identity for a new sound. Save UTF-8 text as `custom.toml`, then inspect it without opening an audio device.

An explicit signed-offset pair, 250 Hz left and 240 Hz right:

```toml
schema_version = 3
duration_seconds = 2.0
fade_seconds = 0.01
[document]
id = "00000000-0000-4000-8000-000000000001"
[description]
title = "Ten-hertz pair example"
[[component]]
id = 17
kind = "binaural_pair"
left_hz = 250.0
offset_hz = -10.0
left_peak_dbfs = -20.0
right_peak_dbfs = -20.0
```

A 1000 Hz tone with full-depth 40 Hz sinusoidal amplitude modulation:

```toml
schema_version = 3
duration_seconds = 1.0
fade_seconds = 0.01
[document]
id = "00000000-0000-4000-8000-000000000002"
[[component]]
id = 1
ears = "both"
kind = "tone"
frequency_hz = 1000.0
peak_dbfs = -20.0
[component.modulation]
kind = "sinusoidal"
rate_hz = 40.0
depth = 1.0
```

A gated 5 kHz tone and an independent click train, preserving authored order:

```toml
schema_version = 3
duration_seconds = 1.0
fade_seconds = 0.01
[document]
id = "00000000-0000-4000-8000-000000000003"
[[component]]
id = 7
ears = "left"
kind = "tone"
frequency_hz = 5000.0
peak_dbfs = -20.0
[component.modulation]
kind = "rectangular"
rate_hz = 40.0
width_seconds = 0.001
[[component]]
id = 19
ears = "right"
kind = "click"
rate_hz = 40.0
width_seconds = 0.001
peak_dbfs = -20.0
```

An empty component array means silence; the document identity and finite envelope fields are still required.

## Fields and identity

`schema_version = 3`, `duration_seconds`, `fade_seconds` and `[document] id` are required. The reader accepts only this document shape and requires an explicit identity. Unknown/duplicate keys, mistyped fields, nonfinite numbers and duplicate/nonpositive component IDs fail. Whole numeric literals are accepted as reals. A finite sound may still be invalid at a selected sample rate and can be saved for repair.

| Field | Meaning |
|---|---|
| `[document] id` | Canonical lowercase UUIDv4; declares identity, never ownership or scientific trust. |
| `document.created_at`, `document.modified_at` | Optional declared recipe history; omission is Unknown. |
| `[description] title`, `notes` | Current sound's nonempty title and optional own notes. |
| `[[component]] id` | Unique positive u32 owner, stable across edits/removals; source order determines summation. |
| `[[source]]` | Bibliography: unique `key`, `label`, `citation`, DOI and/or explicit http(s) `url`. |
| `[[evidence]]` | Source key, scope, population/role/measures, methods locator, summary, limitations and adaptations. |
| `[[lineage]]` | Flat ancestor title/note, optional declared UUID/catalog ID and bibliography keys; no recursive documents or private path field. |

Dates use years 0001–9999, uppercase `T`/`Z`, explicit `Z` or numeric offset, seconds 00–59 and at most nine fractional digits. Unknown offset `-00:00`, leap seconds and missing zones are rejected. Accepted instants normalize to UTC with nine fractional digits without inventing precision. Reading, playback and export preserve declared dates; they never come from filesystem timestamps or study years. Copy drafts receive a new UUID and unknown dates. Local membership history belongs outside portable TOML.

Raw `tone` and `click` nodes use `ears = "left"|"right"|"both"`, `peak_dbfs`, and their frequency or rate/width. Optional `[component.modulation]` is `sinusoidal` (rate/depth) or `rectangular` (rate/width). Each node's modulation applies to that node only.

`binaural_pair` uses `left_hz`, signed `offset_hz`, `left_peak_dbfs` and `right_peak_dbfs`; right carrier is left + offset. `monaural_pair` uses `ears`, `first_hz`, signed `offset_hz`, `first_peak_dbfs` and `second_peak_dbfs`. Pairs have no modulation table, duplicate second-carrier field or recursive children. Zero offset is a valid no-rhythm construction. Nonzero offsets that disappear during carrier addition cannot compile, with both coupled fields identified. Pair lowering preserves left/first then right/second order, levels, primitive phase/generator formulas and stable owner/lane attribution. Each raw tone or click expands to one voice; each pair expands to two. The authored-component count must fit [`Sound::MAX_COMPONENTS`](../crates/core/src/authored.rs), and the total expanded voice count must independently fit [`Sound::MAX_VOICES`](../crates/core/src/authored.rs). Both limits are 16, so eight pairs fill the voice budget while using only eight authored components.

## Evidence and copies

At most 16 bibliography sources, 32 evidence records and 16 flat lineage descriptors are retained. Dangling keys and ancestry overflow fail; no history is silently dropped. DOI identity normalizes independently of displayed citation text. The app does not fetch or authenticate declared sources automatically.

Each evidence record requires `scope` (`reference_condition` or `ancestor`), `population` (`human`, `mouse`, `other_animal`, `mixed`, `not_applicable`, `unspecified`), `condition_role` (`test`, `comparison`, `mixed`, `unspecified`), `measures` (zero or more `eeg`, `behavior`, `subjective`, `sleep`, `hearing`, `other`), `methods_locator`, `summary`, `limitations` and `adaptation`. `population_detail` is optional. A reference condition describes a published condition with disclosed adaptations; it does not validate generated output. Copying demotes reference evidence to ancestor scope. Sound-setting edits likewise demote applicable current evidence; title/notes edits alone do not.

A copied-from snapshot is available during the editor session. After reopen, flat lineage retains declared attribution, bibliography and notes, not a reconstructed ancestor waveform. A catalog pointer does not certify or recreate the original document. Full metadata roundtrips through the same writer/parser and immutable save candidates.

The whole UTF-8 document, including multibyte notes and all metadata, is limited to 65,536 bytes. Neither band/category tags nor ownership, local Added here/Trash/revision fields, safety flags or efficacy flags are accepted in portable content. Classification is read-only derived data. Terminal control/bidi characters are escaped for human display; recipe export preserves its captured portable bytes.

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
- Authored component count stays within [`Sound::MAX_COMPONENTS`](../crates/core/src/authored.rs), and expanded voice count stays within [`Sound::MAX_VOICES`](../crates/core/src/authored.rs). Each tone or click contributes one voice and each pair contributes two; every voice has a finite `peak_dbfs` of at most 0.
- Every frequency and rate is finite, positive and below half the sample rate. A tone with positive-depth sinusoidal modulation also needs `frequency_hz + rate_hz` below half the sample rate.
- Pulse widths must give a whole-frame count `1 ≤ W ≤ Signal::MAX_FRAMES` (2^36 frames) and satisfy `(W + 1) × rate_hz ≤ fs`, compared exactly. This leaves at least one closed frame in every period, including fractional-frame periods.
- Each ear's summed component peaks are at most full scale, so no sample ever needs clipping.

`inspect` reports these summed peaks per ear. They are bounds, not measured or necessarily reached maxima.

## Digital levels

`peak_dbfs` states a component's peak bound. Over complete steady cycles, an unmodulated tone's RMS is 3.01 dB below its peak bound. When the carrier frequency is above the modulation rate, a full-depth sinusoidally modulated tone is 7.27 dB below its peak bound over complete steady cycles. These relationships require distinct, non-DC spectral terms over the averaging interval; spectral coincidences need separate calculation. Distinct tones add in power over suitable steady intervals, so two tones each 3.01 dB lower match one tone's RMS under those conditions. Coincident frequencies can add coherently.

An ideal rectangular click train's RMS is its pulse peak plus `10 · log10(width_seconds · rate_hz)` dB. That duty-cycle change also applies to a gated tone relative to its ungated RMS when its open windows contain complete tone cycles, as in the catalog's tone pips; other gated-tone RMS values depend on carrier phase and frequency. The duty-cycle change is −13.98 dB for 1 ms at 40 Hz. For sampled pulses, use the actual active-frame fraction, and exclude endpoint fades from steady-state calculations. The validated linear/decibel bounds apply before sink conversion; nearest-f32 and signed integer quantization have separate spacing/rounding rules and can change a peak slightly. The [encoder](../crates/app/src/wav/export.rs) rounds samples to binary32. Loudness at the ear depends on the device, its volume and the headphones or speakers.

## Built-in catalog

Each built-in recipe is a parameterized audio adaptation of a selected condition from a published study. Its scoped evidence distinguishes stated source parameters, unspecified details and explicit synthesis or presentation choices. These recipes do not reproduce the original apparatus, calibrated acoustic levels or every waveform detail. The [catalog reference](catalog.md) documents the primary methods, supplements, adaptations and remaining evidence limits for the shipped recipes. Built-in sounds use 0.1 s fades, phase-zero starts and a −20 dBFS peak per component unless the study's own level relationships require otherwise; those cases are explained in `adaptation`. Study procedures beyond the sound itself (baselines, condition order, tasks, recordings, calibration, light stimulation) are not part of a recipe.

### Adding a built-in recipe

A condition qualifies only when its full primary methods give enough evidence to identify the waveform, frequencies or rates and widths, continuous duration, and original presentation. Headphone routing must be stated; a documented speaker presentation may be explicitly adapted to identical ear channels, without claiming that this recreates the acoustic field. Selecting one stated ear or EEG session from a wider protocol must also be disclosed. Unexplained routing guesses, unreported carrier or pulse parameters, sweeps, live-controlled sounds, unstated maskers/noise and short recording-locked trial reconstructions do not qualify. Record missing phase, ramps, audio rate and calibration instead of inventing them. Check supplements and publisher corrections/retractions, and state unavailable evidence honestly.

Name the file `firstauthor-year-condition.toml`, in lowercase, under `crates/core/catalog/`, and add its recipe to the independent table in `crates/core/src/catalog/tests.rs`. The build script discovers conditions in natural order and generates standalone schema-3 recipes. Conditions use a bibliography key under `[[source]]` to refer to a citation/DOI record under `crates/core/studies/`. Citation and URL exist only in that study record; condition-specific methods, findings and adaptations remain in the condition. Add an independent numerical expectation for each condition in `crates/core/src/catalog/tests.rs`. Text supports Unicode, correctly escaped quotes and multiline TOML strings.

- Population, condition role and measures name what was investigated; they do not state a benefit.
- `summary` says what the listener hears, what the study tested on whom, what it found, including null results and the study's size, and whether the result comes from animals. It reports findings and never promises an effect. Where a disease is named, state that the sound is not an established treatment.
- `methods_locator` and `adaptation` are for researchers and use the source's terms.

Then run the [contributor checks](../CONTRIBUTING.md#checks).
