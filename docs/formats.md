# Formats

Auditory Lab reads recipe schema **4**, emits CLI JSON output **2**, uses nominal classifier revision **2**, and stores User Library layout **1**. These are separate contracts.

## Recipe schema

A portable recipe is UTF-8 TOML, at most 65,536 bytes including metadata. A UTF-8 byte-order mark is accepted. Unsupported versions, unknown/duplicate keys, mistyped fields and nonfinite numbers are rejected. A required `[sound]` table contains the finite envelope and ordered components.

```toml
schema_version = 4

[document]
id = "00000000-0000-4000-8000-000000000001"

[description]
title = "Silent example"

[sound]
duration_seconds = 2.0
fade_seconds = 0.01
component = []
```

Use New or Edit a copy in the desktop, or CLI `library add --as-copy`, to allocate an independent identity. The example UUID identifies this illustrative document.

| Section / field | Contract |
| --- | --- |
| `schema_version` | Required integer, exactly 4. |
| `[document] id` | Required canonical lowercase UUIDv4. |
| `document.created_at`, `document.modified_at` | Optional declared history; omission means Unknown. |
| `[description] title`, `notes` | Optional section; a supplied title must be nonempty. Notes are optional. |
| `[sound] duration_seconds`, `fade_seconds` | Required finite seconds. Acoustic validity is checked separately at a rate. |
| `[[sound.component]] id` | Unique positive u32 authored owner; source order determines summation. |
| `[[source]]` | Unique nonempty `key`, nonempty `label` and `citation`, and a DOI and/or explicit HTTP(S) `url`. |
| `[[evidence]]` | Bibliography key, scope, population, condition role, measures, methods, summary, limitations and adaptation. |
| `[[lineage]]` | Flat ancestor title/note, optional `document_id`/`catalog_id` and referenced `source_keys`. |

There are at most 16 authored components and 16 expanded voices. A raw tone/click uses one voice; a pair uses two. Metadata allows 16 sources, 32 evidence records and 16 lineage descriptors. References must name existing bibliography keys. Copying refuses ancestry overflow.

### Components

| `kind` | Required fields besides `id` |
| --- | --- |
| `tone` | `ears`, `frequency_hz`, `peak_dbfs` |
| `clicks` | `ears`, `rate_hz`, `width_seconds`, `peak_dbfs` |
| `binaural_pair` | `left_hz`, signed `offset_hz`, `left_peak_dbfs`, `right_peak_dbfs` |
| `monaural_pair` | `ears`, `first_hz`, signed `offset_hz`, `first_peak_dbfs`, `second_peak_dbfs` |

`ears` is `left`, `right` or `both`. A binaural pair fixes the first carrier to the left ear and the second to the right. Its second frequency is `left_hz + offset_hz`; a monaural pair uses `first_hz + offset_hz` on the same route. Zero offset is permitted. A nonzero offset that collapses during floating-point addition cannot render.

Raw tone/click components can have one `[sound.component.modulation]` table:

| `kind` | Required fields |
| --- | --- |
| `am` | `rate_hz`, `depth` |
| `gate` | `rate_hz`, `width_seconds` |

Pairs have no modulation table or duplicate second-frequency field. Modulation applies only to its own component.

### Dates, evidence and ancestry

Timestamps use years 0001–9999, uppercase `T`/`Z`, explicit `Z` or a numeric offset, seconds 00–59 and at most nine fractional digits. Missing zones, `-00:00` and leap seconds are rejected. Instants serialize as UTC with nine fractional digits. Filesystem times and study years do not supply missing recipe dates.

Evidence requires `source`, `scope`, `population`, `condition_role`, `measures`, `methods_locator`, `summary`, `limitations` and `adaptation`; `population_detail` is optional.

- Scope: `reference_condition` or `ancestor`.
- Population: `human`, `mouse`, `other_animal`, `mixed`, `not_applicable`, `unspecified`.
- Condition role: `test`, `comparison`, `mixed`, `unspecified`.
- Measures: any list of `eeg`, `behavior`, `subjective`, `sleep`, `hearing`, `other`.

Copying creates a new UUID with unknown dates and appends an ancestor descriptor. Reference-condition evidence becomes ancestor evidence. Sound-setting changes also demote applicable current evidence; title/notes changes alone do not. A reopened lineage record retains attribution rather than an ancestor waveform.

### Validation at a sample rate

Inspection/export accept integer sample rates from 1 through 768,000 Hz and default to 48,000 Hz. Playback uses the actual device rate.

- Duration must resolve to 1 through 2^36 frames. Fade is nonnegative, resolves within that bound, and satisfies `2F < N`.
- Carrier and modulation rates are positive and strictly below half the rate. Active AM on a tone also requires `frequency_hz + rate_hz` below that boundary.
- Peak levels are at most 0 dBFS; AM depth is between 0 and 1.
- Pulse width resolves to `1 ≤ W ≤ 2^36` frames and satisfies `(W + 1) × rate_hz ≤ sample_rate`, leaving a closed frame per period.
- Each ear’s summed linear peak bounds are at most 1.

Lengths resolve to nearest whole frames, with halves away from zero. A structurally valid finite document can fail these acoustic rules and remain savable for repair. [Waveform formulas](recipes.md#waveforms) define synthesis.

## Unsupported recipes and schema-3 libraries

A schema-3 file is rejected with a diagnostic such as `schema_version 3; this build reads 4`. If it remains under `recipes/` or `trash/`, User discovery is incomplete: healthy rows are retained and `list` exits **5**. The diagnostic names the file, which remains unchanged.

Back up the affected files. Rewrite each as schema 4 using the reference above, or move it outside the library. Schema 4 moves root duration/fade into `[sound]`, uses `[[sound.component]]` and `[sound.component.modulation]`, and spells the kinds `clicks`, `am` and `gate`. Changing only the version number is insufficient. Check a rewritten file with `auditorylab inspect --file PATH` before putting it back, then run `library check` or Refresh.

## Queries and classification

| Option | Values |
| --- | --- |
| `--band` | `delta`, `theta`, `alpha`, `beta`, `gamma` |
| `--method` | `binaural`, `monaural`, `clicks`, `gate`, `am`, `steady-tone` |
| `--rhythms` | `none`, `multiple`, `outside-bands`, `needs-review` |
| `--analysis` | `complete`, `invalid` |
| `--sort` | `rate`, `title`, `study`, `collection`, `created`, `modified`, `added` |
| `--direction` | `asc`, `desc` |
| `--group-by` | `none`, `band`, `method`, `study`, `collection` |

Nominal classification reads authored settings without a sample rate. Its completeness is independent of rate-specific inspection validity. Plain tones remain independent steady tones; use explicit pairs for pair relationships. Generated descriptions use realized rates and transition roles.

Reference intervals are Delta `[0.1,4)`, Theta `[4,8)`, Alpha `[8,13]`, Beta `(13,30]`, Gamma `(30,80]` Hz. Other positive rhythmic rates lie outside these bands. See the [catalog convention](catalog.md#nominal-rhythm-references).

Repeated facet values are alternatives. Band/method correlation uses one descriptor. Study/population/role correlation uses one current evidence record. Ancestor-only evidence does not match a current study. `--study` accepts a DOI or source key; `key:NAME` explicitly selects a local key.

Unknown dates fail bounded filters and sort last in either direction. Date sorts default descending; other explicit sorts default ascending. Ties use normalized title, then stable key ascending. Groups reference unique flat entries; repeated group memberships do not increase the unique count.

## CLI JSON output

Every envelope contains `output_schema_version`, `operation`, `complete`, `convention`, `query`, `unique_count`, `entries`, `groups` and `diagnostics`. `query` can be null. Optional operation fields are omitted when inapplicable.

| Operation | Data |
| --- | --- |
| List / Trash list | Exact query, `classifier_revision`, `effective_sort` as `[sort,direction]`, groups and retained `stages`. |
| Describe | Entry adds `recipe_toml` and `history_warnings`. |
| Inspect | `validation.status` is `valid` with rate/frames/peaks/component facts, or `invalid` with rate/diagnostics. |
| Library path | `root` reports configured path/source, existence, layout and initialization; unavailable existence can be null. |
| Initialization | Root result and published/unchanged `status`. |
| Library check | Bounded inventory, diagnostics and retained stages. |
| Mutation | Confirmed `status`, `selector`, `version_token`, affected entry and diagnostics. |
| Failure | Outcome `status` and operation diagnostics. |

Entries identify key, collection, title, portable document identity, duration, dates, content token, directory-owned Trash state, nominal facts, evidence and validation. Unknown dates/values use null. Discovery validation is `not_checked`.

Descriptors contain `owner`, `role`, `method`, `hz` and optional band membership. Expanded owners use `{"component":N,"lane":"primary"}` or `secondary`. Schema 2 has no inferred related-tone owner or derivation field. Display labels remain separate from serialized keys.

Read-only partial inventories return healthy entries with `complete:false`. IDs output emits no stdout selectors for incomplete discovery and rejects grouping or temporary inputs. Prose escapes controls/bidi overrides; raw portable output retains its bytes. JSON escaping preserves decoded string data.

## Library layout

Configuration precedence is explicit absolute CLI override, absolute `AUDITORYLAB_LIBRARY_DIR`, then native per-user application data. Override roots require initialization. A missing default root can be created by first publication; reads never initialize it.

Layout 1 contains:

```text
library.toml                 # layout = 1
recipes/<uuid>.toml           # active User content
trash/<uuid>.toml             # recoverable content
membership.toml              # local Added here / Trashed here timestamps
.library.lock                # persistent cooperative lock
.stage/                      # owned staging artifacts
```

Directory location owns active/Trash membership. Missing local dates are Unknown. A content token is lowercase SHA-256 of exact recipe bytes and excludes directory membership and local timestamps. CLI `--expect` is optional; desktop managed saves use the captured token. Expected tokens are checked even for a no-op.

## Publication and recovery

Managed writes hold the exclusive cooperative lock through read, expected-token/history checks, publication and verification. Recipe bytes are published before membership dates. An interruption can lose a local timestamp while retaining readable content.

Writers use an exclusive adjacent stage, synchronize it, check cancellation and publish by no-clobber link or confirmed replacement. Before-publication failure preserves previous bytes. After ambiguous I/O, verified candidate bytes mean saved with a warning; verified previous state means not applied; differing or unreadable state is an error naming the path. Confirmed durability/cleanup warnings remain success.

Trash/Restore use a no-clobber link then unlink. Same-file links in both directories retain Trash authority; a locked write removes the redundant active link. Different files sharing a UUID remain for explicit reconciliation. Purge confirms absence under the same lock and affects only the chosen Trash entry.

`library check` retains unknown artifacts. Locked writes prune orphan membership rows and recognized abandoned stages, without promoting their contents. Forced termination can leave `.auditorylab-UUID.partial` beside a destination. External Save checks the opened identity and bytes immediately before replacement; an uncooperating writer can still race that final check.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Completed, unchanged or complete empty result; confirmed publication warnings remain success. |
| 1 | Input/output, identity generation or device failure; destination already exists. |
| 2 | Usage, unknown selector, unsupported/invalid recipe or sound, Built-in mutation refusal. |
| 3 | Captured-content conflict, history mismatch or reserved identity. |
| 4 | Busy library or unavailable required locking assurance. |
| 5 | Incomplete discovery or unreadable requested managed content. |
| 130 | Cooperative cancellation before completion, or forced interrupt exit. |

Read-only broken pipes are quiet success. A confirmed mutation remains successful if printing its result fails; stderr reports confirmed publication and the reporting failure. [CLI workflows](cli.md) describe commands and interrupts.
