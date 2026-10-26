# CLI

Run `auditorylab` for help, `auditorylab COMMAND --help` for options, or `auditorylab -V` for product version, build counter and optional source revision. These commands open no device.

## Sources and collections

Use an explicit persistent selector, `builtin:<catalog-id>` or `user:<canonical-uuid>`. A title or bare catalog ID is not a selector. `describe`, `inspect`, `play` and WAV `export` alternatively accept exactly one `--file PATH` or `--stdin`. Files must resolve to regular files; use stdin for streams. Temporary inputs do not become User Library members.

```text
auditorylab list --collection builtin --band delta --method binaural
auditorylab describe builtin:ioannou-2015-binaural-2-hz
auditorylab inspect --file clicks.toml --sample-rate 44100
cat clicks.toml | auditorylab inspect --stdin
auditorylab recipe builtin:chan-2022-clicks-40-hz --output clicks.toml
```

`list` defaults to active Built-in + User entries. `--collection builtin` bypasses User discovery; `--collection user` includes only active managed entries. Repeat `--file PATH` with All to include explicitly supplied temporary inputs for that invocation. Trash is queried separately through `library list-trash`. Browsing never opens an audio device, initializes a library or rewrites dates.

`recipe SOURCE [--output PATH]` exports captured saved portable TOML. Built-in output preserves shipped source bytes and User output preserves the exact bytes of its plain recipe file. Export retains identity/history and does not fork a copy. Output files are never overwritten by CLI recipe/WAV export.

## Root and lifecycle

CLI and desktop share the native persistent data location. Resolve an absolute `--library-dir PATH` first, then absolute `AUDITORYLAB_LIBRARY_DIR`, then the native per-user application data directory. `library path --format json` reports the actual configured location and initialization state without creating it. There is no fallback to the checkout, bundle, cache or another root when an override is unavailable. Explicit override roots require `library init`; the first publication can initialize a missing default root.

```text
auditorylab library path
auditorylab --library-dir /absolute/path/to/library library init
auditorylab library add --file clicks.toml --as-copy --format json
auditorylab list --collection user --format json
```

`add` preserves portable identity and known/unknown history; `--as-copy` explicitly forks a new identity, scoped ancestry and publication history. A reserved built-in UUID requires a copy. Same-ID identical content/history is unchanged; differing content/history conflicts. The original input file remains untouched.

Use a selector for existing mutations. Optional `--expect TOKEN` checks the exact observed recipe bytes; scripts can obtain that token from list/describe JSON:

```text
auditorylab library update user:UUID --file edited.toml --expect TOKEN --format json
auditorylab library rename user:UUID --title "My sound" --expect TOKEN
auditorylab library trash user:UUID --expect TOKEN
auditorylab library list-trash --format json
auditorylab library restore user:UUID --expect TOKEN
auditorylab library purge user:UUID --expect TOKEN --yes
```

`UUID` and `TOKEN` above are placeholders for observed values. Re-read the current state before another mutation; Trash and Restore can retain the same content token. An expected token is the lowercase SHA-256 of the exact portable file bytes and is checked even for a no-op. Tokens do not encode timestamps or directory membership. Missing/stale targets conflict rather than being recreated. Built-ins cannot be updated, renamed, trashed or purged. Purge requires a currently trashed User entry and never cascades to ancestors, copies or external files. Trash/Restore preserve recipe history and Added here.

`library check [--format json]` reports malformed files, unknown artifacts, orphan membership timestamps and owned stages without changing them. Each write holds the exclusive library lock through reading, optional token checks, publication and verification. When an I/O result is ambiguous, the application re-reads the destination: candidate bytes mean saved with a warning; previous bytes mean not applied; other or unreadable content is an error naming the path. A confirmed purge is absence after unlink under that same lock. Completion-output failure cannot undo storage changes and is reported separately.

Library layout **1**, defined by [`library::LIBRARY_LAYOUT`](../crates/app/src/library/root.rs), stores `library.toml` with `layout = 1`, `recipes/<uuid>.toml`, `trash/<uuid>.toml`, timestamp-only `membership.toml`, `.library.lock` and a private `.stage/` directory. Directory location decides active/Trash state; missing membership dates are Unknown. The recipe is published before its membership timestamps, so a crash can lose a timestamp without making the recipe unreadable. A locked write drops orphan timestamp rows and cleans only recognized owned stages. Unknown files are retained. Interrupted no-clobber moves can leave two names for one file: Trash remains authoritative for those same-inode links, and a locked write removes the redundant active link. Different files with the same UUID require explicit reconciliation and are never deleted automatically.

## Shared queries

Search normalizes Unicode/case and requires every term. Bare numeric `20` matches a whole value without matching carrier 200 or year 2015. The [reference convention](catalog.md#nominal-rhythm-references) describes audio structure, not EEG or promised effects.

| Option | Values / semantics |
|---|---|
| `--band` | delta, theta, alpha, beta, gamma |
| `--method` | binaural, monaural, clicks, gate, am, steady-tone |
| `--rhythms` | none, multiple, outside-bands, needs-review |
| `--study` | DOI or local source key (`key:NAME` is explicit) |
| `--role` | test, comparison, mixed, unspecified |
| `--population` | human, mouse, other_animal, mixed, not_applicable, unspecified |
| `--analysis` | complete, partial, invalid nominal coverage |
| `--created-from/--created-before` | Inclusive/exclusive explicit-offset instants |
| `--modified-from/--modified-before` | Inclusive/exclusive declared edit instants |
| `--added-from/--added-before` | Inclusive/exclusive local membership instants |
| `--sort` | rate, title, study, collection, created, modified, added |
| `--direction` | asc, desc |
| `--group-by` | none, band, method, study, collection |
| `--format` | text, json, ids |

Repeat facet values for alternatives. Band and method must match one descriptor; study, role and population must match one current reference-condition evidence record. Ancestor-only evidence confers no current-study match. Unknown dates fail bounded ranges and sort last in either direction. Date bounds require explicit zones, for example `2026-10-24T00:00:00+03:00`; upper bounds are exclusive. Inverse ranges are errors.

Automatic ordering is rate ascending for All/Built-in, modified descending for User/Trash. Explicit choices survive collection changes; dates default descending, other sorts ascending. Ties use normalized title then stable key ascending. Groups reference unique flat entries; repeated memberships never inflate `unique_count`.

## Output and failures

CLI JSON output **2**, defined by [`reports::OUTPUT_SCHEMA_VERSION`](../crates/app/src/reports.rs), uses numeric quantities, explicit null unknowns, normalized UTC instants, the effective query, unique entries, group-key references and diagnostics. Read-only partial inventories retain healthy rows with `complete:false`. IDs output accepts only reusable persistent selectors, rejects grouping/temporary inputs, and emits **no stdout IDs** when discovery is incomplete.

Human prose follows terminal width (78 columns when redirected); controls and bidi overrides are escaped. Raw recipe bytes remain unchanged. JSON escapes unsafe display controls without changing decoded data. Read-only broken pipes are quiet success; mutation reporting failure cannot undo publication and reports confirmed publication and an output warning on stderr.

| Code | Meaning |
|---|---|
| 0 | Completed, unchanged or empty complete result; confirmed publication warnings remain success. |
| 1 | I/O or device failure; file diagnostics name the affected path. |
| 2 | Usage, unknown selector, unsupported recipe/validation or Built-in mutation refusal. |
| 3 | Conflict, history mismatch or reserved identity. |
| 4 | Busy or unavailable required locking assurance. |
| 5 | Incomplete inventory or unreadable requested managed record. |
| 130 | Cooperative cancellation before completion. |

Before-publication failures preserve old bytes. Confirmed durability or cleanup warnings remain saved. An error after an ambiguous result identifies the path for inspection and does not claim unchanged storage. There is no force-overwrite bypass or automatic stage promotion.

## Playback and WAV export

`inspect SOURCE [--sample-rate HZ]` and `export SOURCE --output PATH [--sample-rate HZ]` default to 48,000 Hz; [SampleRate](../crates/core/src/rate.rs) defines the supported range. Inspect reports requested and realized rates/widths, frame count, peaks and pair facts without opening a device. Finite invalid documents remain inspectable; export validates at its chosen rate. WAV output is stereo 32-bit float, RIFF or RF64 as required by size.

Turn device volume down before explicit `play SOURCE`. It validates at the actual default-device rate, sends left/right to output channels 0/1 and silences additional channels. Start/final facts, hearing/headphone notices, underruns and status go to stderr. A terminal gets a 4 Hz clock/status line; redirected output has no carriage-return animation. Trash cannot be played until restored.

During export/play the first Ctrl-C requests cancellation and the second exits immediately with 130; Unix SIGTERM/SIGHUP also request cancellation. Export checks before staging, between blocks and before publication. Playback checks every 10 ms and fades up to 30 ms before queued device audio drains. Cancellation after publication cannot unpublish a file. Forced termination can leave a hidden partial file beside the destination. The CLI has no interactive live-edit grammar.

### JSON report fields

The application defines the serializable reports in [`reports`](../crates/app/src/reports.rs). Every envelope contains `output_schema_version`, `operation`, `complete`, `convention`, `query`, `unique_count`, `entries`, `groups`, and `diagnostics`.

| Operation | Additional fields |
| --- | --- |
| List and Trash list | `classifier_revision`, `effective_sort` (sort and direction), `stages` |
| Describe | Each entry adds `recipe_toml` and `history_warnings` |
| Inspect | Each entry's `validation.status` is `valid` with rate, frames, peaks and voice facts, or `invalid` with rate and diagnostics; discovery entries use `not_checked` |
| Library path | `root` contains the configured path, source, existence, layout and initialization state; unknown existence is null |
| Library initialization | `root` contains the initialization result; `status` states whether it published or was unchanged |
| Library check | `stages` names retained stages; entry and diagnostic arrays describe the bounded inventory |
| Mutations | `status`, `selector`, `version_token`, entries and diagnostics report confirmed publication truth |
| Errors | `status` classifies the exit outcome and `diagnostics` explains it |

`effective_sort` is a two-element JSON array `[sort, direction]`. An entry includes its key, collection, title, document identity, duration, declared and membership dates, version token, directory-owned `trashed` state, nominal classification, evidence and validation state. Nominal descriptors carry owner and optional related owner, method, rate, band and derivation. Groups reference keys in the unique entry list. Diagnostics are storage-context objects or operation messages; unknown dates and absent comparison values are JSON null.
