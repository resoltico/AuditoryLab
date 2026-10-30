# CLI

Run `auditorylab` for help, `auditorylab COMMAND --help` for command options, or `auditorylab -V` for the product version and optional full source revision.

## Create a recipe

`auditorylab new KIND` prints a complete editable recipe with a fresh UUID. Kinds are `tone`, `pulse-train`, `binaural-pair`, `monaural-pair` and `pulsed-tone`, shared with Desktop Add component. Generic templates contain no research or benefit claims.

**Pulsed tone (isochronic)** means a single sine carrier under periodic gain pulses, with continuous carrier phase through the silent windows. It uses a `tone` source plus shared `pulsed` modulation, as defined in [waveforms](recipes.md#waveforms). `pulsed-tone` remains the CLI construction key; there is no persisted or CLI `isochronic` kind. A unipolar pulse-train source and a pulsed pair keep their own names.

```text
auditorylab new pulsed-tone --output pulse.toml
auditorylab inspect --file pulse.toml --sample-rate 48000
```

Omit `--output` for TOML on stdout. An existing output file is never replaced. Customize the emitted TOML rather than passing field overrides. See [pulse intent and waveforms](recipes.md#waveforms).

## Choose an input

`describe`, `inspect`, `play` and WAV `export` accept exactly one source:

- `builtin:CATALOG_ID` for a shipped recipe.
- `user:UUID` for a managed User recipe.
- `--file PATH` for a regular UTF-8 TOML file, including a symlink to a regular file.
- `--stdin` for a stream.

```sh
auditorylab describe builtin:ioannou-2015-binaural-10-hz
auditorylab inspect --file custom.toml --sample-rate 44100
cat custom.toml | auditorylab inspect --stdin
```

Use complete selectors, rather than titles or bare catalog IDs. Temporary file keys belong to one process and cannot be reused as selectors. Opening a file does not add it to User Library.

## Find recipes

```text
auditorylab list --collection builtin --band delta --method binaural
auditorylab list --collection user --sort modified --direction desc
auditorylab list --study ioannou-2015 --population human --role test
auditorylab list --file custom.toml --group-by method --format json
```

`list` shows each entry's title, selector, rhythm, duration and what its study was about (**Custom recipe** when it has none). Built-in rows show dates only while sorting by a date. `list` defaults to active Built-in and User entries. `--collection builtin` skips User discovery; `--collection user` selects active managed entries. Temporary `--file` inputs require All. Query Trash with `library list-trash`.

Search requires every Unicode-normalized term. A bare number matches a whole numeric value. Repeated facet options are alternatives. Band and method must match one rhythmic process; study, population and role must match one current evidence record. Vocabulary values use snake_case, including `steady_tone`, `outside_bands` and `needs_review`; CLI flag names retain kebab-case. See [query values and classification](formats.md#queries-and-classification).

Use `--created-from`, `--created-before`, `--modified-from`, `--modified-before`, `--added-from` and `--added-before` for inclusive/exclusive timestamp ranges. Supply an explicit offset, such as `2026-10-24T00:00:00+03:00`. Unknown dates fail bounded ranges and sort last. Automatic ordering is rate ascending for All/Built-in and modified descending for User/Trash; explicit sort choices survive collection changes.

`--format json` returns structured entries, groups and diagnostics. Each diagnostic/warning has a fixed `code`, human `message` and optional structured `params`; inspect exposes expanded lanes under `voices`. `--format ids` emits reusable selectors, requires complete discovery, and rejects grouping and temporary inputs. An incomplete IDs query emits no stdout IDs. [JSON and exit codes](formats.md#cli-json-output) define the scripting contract.

## Inspect, save portable text and export WAV

```text
auditorylab recipe builtin:ioannou-2015-binaural-10-hz --output pair.toml
auditorylab inspect --file pair.toml --sample-rate 48000
auditorylab export --file pair.toml --output pair.wav --sample-rate 48000
```

`describe` shows what a sound was studied for, its summary with limitations, what was measured, the study method, the adaptation, the citation and a link; records the sound inherits from an earlier recipe appear once under **Further evidence**. `inspect` opens no device. It reports the chosen rate, resolved frame count, per-channel digital peak bounds and generator-voice facts. Components are named by their authored ID; a pair's secondary lane is its second tone. WAV export writes the exact rendition inspected at its chosen rate as stereo 32-bit float RIFF/RF64. The [recipe guide](recipes.md) explains rate-dependent validity and waveforms.

`recipe SELECTOR` prints saved portable TOML; `--output PATH` writes it. Built-in output is standalone TOML assembled from embedded catalog conditions and study records; User output retains exact saved file bytes. Both preserve document identity and declared timestamps. CLI recipe and WAV exports never overwrite an existing destination; export checks the destination before announcing the file and its size. Exported files get the same permissions as other new files you create; library records stay readable only by you.

## Root and lifecycle

CLI and desktop share one native per-user library. Each command captures one configured root and precedence source; filesystem/layout checks still run against the state used for each action. An absolute `--library-dir PATH` takes precedence over `AUDITORYLAB_LIBRARY_DIR`, then the native default. Inspect the location before changing it:

```text
auditorylab library path --format json
auditorylab --library-dir /absolute/path/to/library library init
auditorylab library add --file pair.toml --as-copy --format json
auditorylab list --collection user --format json
```

Initialize explicit override roots before writing. A missing default root can be initialized by its first publication. Reads never initialize it, and an unavailable configured root never falls back to another location. List discovery retains healthy Built-in rows alongside typed incomplete-root diagnostics.

`library add` imports the file’s identity and declared history. `--as-copy` creates an independent identity and ancestry. Use a copy for reserved Built-in UUIDs or when retaining a separate version. Inputs remain unchanged.

Obtain `user:UUID` and `TOKEN` from current JSON output before a mutation:

```text
auditorylab library update user:UUID --file edited.toml --expect TOKEN
auditorylab library rename user:UUID --title "My sound" --expect TOKEN
auditorylab library trash user:UUID --expect TOKEN
auditorylab library list-trash --format json
auditorylab library restore user:UUID --expect TOKEN
auditorylab library purge user:UUID --expect TOKEN --yes
```

The placeholders must be replaced with observed values. `--expect` is optional in CLI commands and checks exact portable bytes even for a no-op. Re-read membership before another action: Trash/Restore can leave the token unchanged. Missing or stale mutation targets conflict rather than being recreated. Purge requires a trashed User entry and explicit confirmation. It affects only that entry.

`library check [--format json]` reports malformed recipes, unknown artifacts, orphan membership dates and retained stages. For unsupported files, follow the [unsupported-file handling](formats.md#unsupported-and-unreadable-recipes). The [library reference](formats.md#library-layout) describes locking, timestamps and interruption recovery.

## Playback and cancellation

Set device volume low, then run `auditorylab play SOURCE`. Playback validates at the default device’s actual rate. It sends left/right to channels 0/1 and silences extra channels. Restore a managed Trash entry before playing it. Facts, status and completion go to stderr; a terminal receives an updating clock.

The first Ctrl-C requests Stop or export cancellation; a second exits immediately with 130. Unix SIGTERM/SIGHUP also request cancellation. Stop fades generated audio and drains queued output within a bounded limit. Export checks cancellation before staging, between blocks and before publication. Cancellation after publication leaves the confirmed file saved.

A write can succeed with a durability or cleanup warning. If printing its confirmation fails, the CLI retains successful publication and reports the confirmed result on stderr. An unconfirmed I/O result names the path to inspect. See [exit codes](formats.md#exit-codes).
