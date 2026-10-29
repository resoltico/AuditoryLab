# Formats

Auditory Lab reads recipe schema **5**, emits CLI JSON output **3**, uses nominal classifier revision **3**, and stores User Library layout **1**. These are separate contracts.

## Recipe schema

A portable recipe is UTF-8 TOML, at most 65,536 bytes including metadata. A UTF-8 byte-order mark is accepted. Unsupported versions, unknown/duplicate keys, mistyped fields and nonfinite numbers are rejected. Required `[sound]` and `[timing]` tables separate ordered sound components from finite duration and overall fade. Nonfinite numeric input produces a format error naming its key, line and column.

```toml
schema_version = 5

[document]
id = "00000000-0000-4000-8000-000000000001"

[description]
title = "Silent example"

[timing]
duration_seconds = 2.0
fade_seconds = 0.01

[sound]
component = []
```

Use New or Edit a copy in the desktop, or CLI `library add --as-copy`, to allocate an independent identity. The example UUID identifies this illustrative document.

| Section / field | Contract |
| --- | --- |
| `schema_version` | Required integer, exactly 5. |
| `[document] id` | Required canonical lowercase UUIDv4. |
| `document.created_at`, `document.modified_at` | Optional declared history; omission means Unknown. |
| `[description] title`, `notes` | Optional section; a supplied title must be nonempty. Notes are optional. |
| `[timing] duration_seconds`, `fade_seconds` | Required finite seconds. Acoustic validity is checked separately at a rate. |
| `[[sound.component]] id` | Unique positive u32 authored owner; source order determines summation. |
| `[[source]]` | Unique nonempty `key`, nonempty `label` and `citation`, and a DOI and/or explicit HTTP(S) `url`. |
| `[[evidence]]` | Bibliography key, scope, population, condition role, measures, methods, summary, limitations and adaptation. |
| `[[lineage]]` | Flat ancestor title/note, optional `document_id`/`catalog_id` and referenced `source_keys`. |

One authored budget permits 16 generator lanes: a tone or pulse train costs one; an explicit pair costs two. The maximum of 16 components follows from this budget. Crossfades run at most 32 lane-equivalents. Metadata allows 16 sources, 32 evidence records and 16 lineage descriptors. References must name existing bibliography keys. Copying refuses ancestry overflow.

### Components

Every `[[sound.component]]` requires a positive `id`, a `[sound.component.source]` table and a `[sound.component.modulation]` table. Components sum in authored order. Unknown or inapplicable fields are errors; there are no old-kind or old-field aliases.

| Source `kind` | Required source fields |
| --- | --- |
| `tone` | `route`, `frequency_hz`, `peak_dbfs` |
| `pulse_train` | `route`, `peak_dbfs`, nested `pulse` |
| `binaural_pair` | `first_hz`, signed `offset_hz`, `first_peak_dbfs`, `second_peak_dbfs` |
| `monaural_pair` | `route`, `first_hz`, signed `offset_hz`, `first_peak_dbfs`, `second_peak_dbfs` |

`route` means digital channel routing: `left`, `right` or `both`. Both sends identical component samples to both channels. A binaural pair fixes the first carrier to the left channel and the second to the right. The second carrier is `first_hz + offset_hz`; a monaural pair sends both tones through its common route. Zero offset is permitted. A nonzero offset that disappears during binary64 addition cannot compile. Channel routing does not establish acoustic delivery to separate ears.

Human references retain the authored `id`; a pair's second lane is its second tone. No relationship is inferred between independent tone components.

| Modulation `kind` | Required modulation fields |
| --- | --- |
| `none` | No other fields |
| `sinusoidal` | `rate_hz`, `depth` |
| `pulsed` | Nested `pulse` |

A component has one shared nonnegative modulation gain, including both lanes of either pair kind. A pulse source with pulsed modulation contains two separate authored repetition processes. There is no persisted isochronic kind, modulation chain or general DSP graph.

Every pulse table requires `rate_hz`, a nested `width` and a nested `shape`. Width is either `{ kind = "duration", seconds = VALUE }` or `{ kind = "duty_cycle", fraction = VALUE }`; exactly one requested quantity is stored. Shape is `{ kind = "rectangular" }` or `{ kind = "raised_cosine", rise_seconds = VALUE, fall_seconds = VALUE }`. All numeric fields must be finite. Finite acoustic errors remain editable and serializable, with intrinsic/rate-specific diagnostics.

### Dates, evidence and ancestry

Timestamps use years 0001–9999, uppercase `T`/`Z`, explicit `Z` or a numeric offset, seconds 00–59 and at most nine fractional digits. Missing zones, `-00:00` and leap seconds are rejected. Instants serialize as UTC with nine fractional digits. Filesystem times and study years do not supply missing recipe dates.

Evidence requires `source`, `scope`, `population`, `condition_role`, `measures`, `methods_locator`, `summary`, `limitations` and `adaptation`; `population_detail` is optional.

- Scope: `reference_condition` or `ancestor`.
- Population: `human`, `mouse`, `other_animal`, `mixed`, `not_applicable`, `unspecified`.
- Condition role: `test`, `comparison`, `mixed`, `unspecified`.
- Measures: any list of `eeg`, `behavior`, `subjective`, `sleep`, `hearing`, `other`.

An independent copy starts with a new UUID and unknown declared dates, and appends an ancestor descriptor. Its first User Library save stamps Created and Modified; importing a file preserves its declared dates. Reference-condition evidence becomes ancestor evidence. Sound or finite-timing changes also demote applicable current evidence; title/notes changes alone do not. A reopened lineage record retains attribution rather than an ancestor waveform.

### Validation at a sample rate

Inspection/export accept integer sample rates from 1 through 768,000 Hz and default to 48,000 Hz. Playback uses the actual device rate.

- Duration must resolve to 1 through 2^36 frames. Fade is nonnegative, resolves within that bound, and satisfies `2F < N`.
- Carrier and modulation rates are positive and strictly below half the rate. Active sinusoidal AM on every tonal lane, including pairs, also requires `frequency_hz + rate_hz` below that boundary.
- Peak levels are at most 0 dBFS; AM depth is between 0 and 1.
- Pulse duty is strictly between zero and one. Pulse width resolves to `1 ≤ W ≤ 2^36` frames and satisfies `(W + 1) × rate_hz ≤ sample_rate`, leaving a closed frame per period.
- Each digital channel’s summed linear peak bound is at most 1.

Lengths resolve to nearest whole frames, with halves away from zero. A structurally valid finite document can fail these acoustic rules and remain savable for repair. [Waveform formulas](recipes.md#waveforms) define synthesis.

## Unsupported and unreadable recipes

Only recipe schema 5 is accepted; schema 4 and every other version are rejected without conversion. Unsupported versions and malformed documents produce a diagnostic naming the file and the failed rule. The file remains unchanged. If it is inside `recipes/` or `trash/`, discovery is incomplete: healthy rows remain available and `list` exits **5**.

Back up the affected file. Correct it against the [recipe reference](#recipe-schema), or move it outside the library. Check the corrected file with `auditorylab inspect --file PATH` before returning it, then run `library check` or Refresh. Changing a declared schema number does not establish that a document satisfies the format.

## Queries and classification

Vocabulary values use snake_case in CLI options and JSON. CLI flag names use kebab-case. Source and modulation processes are described independently. Composed headings retain both the source relationship and its modulation.

<!-- query-options:begin -->
| Option | Values |
| --- | --- |
| `--collection` | `all`, `builtin`, `user` |
| `--band` | `delta`, `theta`, `alpha`, `beta`, `gamma` |
| `--method` | `binaural`, `monaural`, `pulse_train`, `pulsed`, `sinusoidal`, `steady_tone` |
| `--rhythms` | `outside_bands`, `none`, `multiple`, `needs_review` |
| `--population` | `human`, `mouse`, `other_animal`, `mixed`, `not_applicable`, `unspecified` |
| `--role` | `test`, `comparison`, `mixed`, `unspecified` |
| `--analysis` | `complete`, `invalid` |
| `--sort` | `rate`, `duration`, `title`, `created`, `modified`, `added`, `study`, `collection` |
| `--direction` | `asc`, `desc` |
| `--group-by` | `none`, `band`, `method`, `study`, `collection` |
<!-- query-options:end -->

Nominal classification reads authored settings without a sample rate. Its completeness is independent of rate-specific inspection validity. Plain tones remain independent steady tones; use explicit pairs for pair relationships. Generated descriptions use realized rates and transition roles. Pair differences derive from integer phase-step differences before conversion to hertz, so rounded carrier displays cannot erase a small difference. Nominal analysis includes authored muted processes; generated analysis includes a component/role when any lane has nonzero routing gain, retaining all of that component’s lanes. Zero-depth AM is inactive. Structural rates are not EEG measurements or verified perceptions.

Reference intervals are Delta `[0.1,4)`, Theta `[4,8)`, Alpha `[8,13]`, Beta `(13,30]`, Gamma `(30,80]` Hz. Other positive rhythmic rates lie outside these bands. See the [catalog convention](catalog.md#nominal-rhythm-references).

Repeated facet values are alternatives. Band/method correlation uses one descriptor. Study/population/role correlation uses one current evidence record. Ancestor-only evidence does not match a current study. `--study` accepts a DOI or source key; `key:NAME` explicitly selects a local key.

Unknown dates fail bounded filters and sort last in either direction. Date sorts default descending; other explicit sorts default ascending. Ties use normalized title, then stable key ascending. Groups reference unique flat entries; repeated group memberships do not increase the unique count.

### Vocabulary keys

These are the vocabulary keys used by queries, portable choices and JSON values. Sound/research terms belong to core; query choices, operations and statuses belong to app. Both compiled registries are checked against this table. CLI options expose the subsets above; `automatic` sorting and `open_files`/`trash` scope also describe app query state. GUI absence options use the explicit key `any`; direction uses `automatic` for its query-owned default.

<!-- vocabulary-keys:begin -->
| Vocabulary | Keys |
| --- | --- |
| `band` | `delta`, `theta`, `alpha`, `beta`, `gamma` |
| `calendar_field` | `created`, `modified`, `added` |
| `carrier_kind` | `tone`, `pulse_train` |
| `channel` | `left`, `right` |
| `channel_routing` | `both`, `left`, `right` |
| `completeness` | `complete`, `invalid` |
| `condition_role` | `test`, `comparison`, `mixed`, `unspecified` |
| `direction` | `asc`, `desc` |
| `evidence_scope` | `reference_condition`, `ancestor` |
| `group_by` | `none`, `band`, `method`, `study`, `collection` |
| `measure` | `eeg`, `behavior`, `subjective`, `sleep`, `hearing`, `other` |
| `method` | `binaural`, `monaural`, `pulse_train`, `pulsed`, `sinusoidal`, `steady_tone` |
| `modulation_kind` | `sinusoidal`, `pulsed` |
| `operation` | `list`, `list_trash`, `describe`, `inspect`, `recipe`, `export`, `play`, `stop`, `library_path`, `library_init`, `library_add`, `library_update`, `library_rename`, `library_trash`, `library_restore`, `library_purge`, `library_check`, `library_refresh`, `library_read`, `new_sound`, `edit_copy`, `save`, `save_copy`, `open_recipe`, `export_recipe`, `stop_current_and_play_recipe`, `play_selected_sound`, `play_live`, `edit_saved_recipe`, `edit_external_file`, `close_open_file`, `show_saved`, `cancel_export`, `add_component`, `undo_remove`, `remove_component` |
| `pair_kind` | `binaural`, `monaural` |
| `parameter_id` | `tone_frequency`, `source_pulse_rate`, `source_pulse_duration`, `peak_level`, `modulation_rate`, `modulation_depth`, `modulation_pulse_rate`, `modulation_pulse_duration`, `pair_first_frequency`, `pair_offset`, `pair_first_level`, `pair_second_level`, `source_duty`, `source_rise`, `source_fall`, `modulation_duty`, `modulation_rise`, `modulation_fall`, `duration`, `fade` |
| `population` | `human`, `mouse`, `other_animal`, `mixed`, `not_applicable`, `unspecified` |
| `rhythm_state` | `outside_bands`, `none`, `multiple`, `needs_review` |
| `scope` | `all`, `builtin`, `user`, `open_files`, `trash` |
| `slot_role` | `continuing`, `outgoing`, `incoming` |
| `sort_by` | `automatic`, `rate`, `duration`, `title`, `created`, `modified`, `added`, `study`, `collection` |
| `status` | `saved`, `saved_with_warning`, `unchanged`, `unknown`, `none`, `operation_incomplete`, `operation_confirmed`, `waiting_for_generated_audio` |
<!-- vocabulary-keys:end -->

## CLI JSON output

Every envelope contains `output_schema_version`, `operation`, `complete`, `convention`, `query`, `unique_count`, `entries`, `groups` and `diagnostics`. `query` can be null. Optional operation fields are omitted when inapplicable.

| Operation | Data |
| --- | --- |
| List / Trash list | Exact query, `classifier_revision`, `effective_sort` as `[sort,direction]`, groups and retained `stages`. |
| Describe | Entry adds `recipe_toml` and `history_warnings`. |
| Inspect | `validation.status` is `valid` with rate/frames/peaks/`voices` facts, or `invalid` with rate/diagnostics. |
| Library path | `root` reports configured path/source, existence, layout and initialization; unavailable existence can be null. |
| Initialization | Root result and published/unchanged `status`. |
| Library check | Bounded inventory, diagnostics and retained stages. |
| Mutation | Confirmed `status`, `selector`, `version_token`, affected entry and diagnostics. |
| Failure | Outcome `status` and operation diagnostics. |

The [diagnostic contract](#diagnostic-codes) applies to nominal diagnostics, rate-validation diagnostics, declared-history warnings and publication warnings. Inspection exposes generator lanes under `voices`; authored components remain in recipe TOML. Pulse facts retain requested width mode/quantity and shape, resolved window/edge frames, and rate. Pair facts retain signed requested and resolved differences; unresolved pair checks have no resolved difference. Digital routes use `route`, and peak diagnostics name a `channel`.

Entries identify key, collection, title, portable document identity, duration, dates, content token, directory-owned Trash state, nominal facts, evidence and validation. Unknown dates/values use null. Discovery validation is `not_checked`.

Nominal descriptors contain `owner`, `method`, `hz` and optional `band` membership. Expanded owners use `{"component":N,"lane":"primary"}` or `secondary`. Each nominal descriptor identifies its generator lane and structural method. Transition roles belong to generated observations, not nominal JSON descriptors. Display labels are separate from serialized keys; method group captions use the same owning vocabulary labels as the other views. `groups[].label` is a human display caption rendered by the shared library presentation owner, independently of the group key and membership.

Read-only partial inventories return healthy entries with `complete:false`. IDs output emits no stdout selectors for incomplete discovery and rejects grouping or temporary inputs. Prose escapes controls/bidi overrides; raw portable output retains its bytes. JSON escaping preserves decoded string data.

### Diagnostic codes

Diagnostics use fixed `code` values, human `message` text and an optional `params` object. Parameters retain captured values separately from the code; inventory locations use `params.storage_id`. Nonfinite quantities use `"NaN"`, `"inf"` or `"-inf"` because JSON has no nonfinite numbers. Transport wrappers report the cause’s code; aggregate failures and confirmed-outcome warnings keep their own rules. Codes identify rules; the operation’s outcome and [exit category](#exit-codes) remain authoritative.

For example, an out-of-range rate reports `sample_rate.range` with `hz`, `min_hz` and `max_hz`; duplicate component identities report `structure.identity` with `component_id`; unsupported recipe versions report `recipe.schema_version` with `found` and `supported`.

<!-- diagnostic-codes:begin -->
| Code |
| --- |
| `capture.invalid_span` |
| `capture.non_finite_sample` |
| `cli.argument` |
| `cli.format` |
| `cli.interrupt` |
| `cli.json` |
| `cli.output` |
| `clock.range` |
| `contract.copy_lineage_full` |
| `contract.dangling_bibliography_reference` |
| `contract.duplicate_source_key` |
| `contract.empty_lineage_note` |
| `contract.empty_lineage_title` |
| `contract.empty_source_citation` |
| `contract.empty_source_key` |
| `contract.empty_source_label` |
| `contract.empty_title` |
| `contract.evidence_budget` |
| `contract.invalid_doi` |
| `contract.invalid_source_url` |
| `contract.lineage_budget` |
| `contract.missing_source_locator` |
| `contract.reused_copy_identity` |
| `contract.source_budget` |
| `contract.source_url_scheme` |
| `device.abandoned` |
| `device.backend` |
| `device.invalid_timing` |
| `device.misaligned_buffer` |
| `device.no_default_output` |
| `device.route_changed` |
| `device.timed_out` |
| `device.too_few_channels` |
| `device.unsupported_sample_format` |
| `edit.budget_exceeded` |
| `edit.commit` |
| `edit.identities_exhausted` |
| `edit.inapplicable` |
| `edit.nothing_removed` |
| `edit.pulse_conversion` |
| `edit.pulse_window` |
| `edit.stale_document` |
| `edit.stale_row` |
| `edit.unknown_component` |
| `file.after_mutation` |
| `file.cancelled` |
| `file.changed` |
| `file.destination_exists` |
| `file.missing_filename` |
| `file.missing_parent` |
| `file.not_regular` |
| `file.read_limit` |
| `file.relocation_exists` |
| `identity.entropy` |
| `identity.exhausted` |
| `input.nonfinite` |
| `input.unparseable` |
| `inventory.duplicate_identity` |
| `inventory.interrupted_move` |
| `inventory.orphan_membership` |
| `inventory.reserved_builtin_identity` |
| `inventory.unknown_entry` |
| `inventory.unknown_root_entry` |
| `inventory.uuid_filename_required` |
| `io.cause` |
| `io.failure` |
| `library.ambiguous_record` |
| `library.busy` |
| `library.cancelled` |
| `library.duplicate_builtin` |
| `library.entry_disappeared` |
| `library.expected_user` |
| `library.history_replacement` |
| `library.identity_exists` |
| `library.lock_not_regular` |
| `library.lock_unavailable` |
| `library.marker_format` |
| `library.marker_utf8` |
| `library.membership_encode` |
| `library.membership_format` |
| `library.membership_size` |
| `library.membership_utf8` |
| `library.membership_without_marker` |
| `library.missing_marker_on_create` |
| `library.missing_marker_on_mutation` |
| `library.must_restore` |
| `library.native_root_unavailable` |
| `library.not_initialized` |
| `library.proposed_identity` |
| `library.purge_requires_trash` |
| `library.purge_unconfirmed` |
| `library.read_only_builtin` |
| `library.recipes_without_marker` |
| `library.record_identity` |
| `library.relative_root` |
| `library.reserved_builtin` |
| `library.root_disappeared` |
| `library.root_missing` |
| `library.scan_budget` |
| `library.stale_content` |
| `library.stale_playback` |
| `library.temporary_selection` |
| `library.unknown_managed` |
| `library.unknown_root_contents` |
| `library.unsupported_layout` |
| `live_update.rate_mismatch` |
| `live_update.revision_limit` |
| `live_update.session_closed` |
| `live_update.unavailable` |
| `load.read` |
| `load.temporary_selector` |
| `load.unknown_builtin` |
| `monitor.fixed_range_exceeded` |
| `monitor.incomplete_coverage` |
| `monitor.stale` |
| `origin.fingerprint` |
| `origin.selector` |
| `pair.nonfinite` |
| `pair.nonpositive` |
| `pair.unrepresentable_difference` |
| `playback.missing_document` |
| `playback.missing_terminal_outcome` |
| `playback.monitor_in_use` |
| `playback.trashed` |
| `preview.unavailable` |
| `query.local_date` |
| `query.local_order` |
| `query.range_order` |
| `query.study` |
| `query.temporary_files_scope` |
| `recipe.format` |
| `recipe.schema_version` |
| `recipe.size` |
| `recipe.utf16` |
| `recipe.utf8` |
| `recipe.write` |
| `sample_rate.range` |
| `save.history_replacement` |
| `save_feedback.editor_refresh` |
| `save_feedback.editor_reset` |
| `save_feedback.invalid_at_rate` |
| `save_refusal.input` |
| `shutdown.failed` |
| `sound.duration` |
| `sound.fade` |
| `sound.fades_overlap` |
| `sound.peak` |
| `storage.clock_predates_edit` |
| `storage.clock_predates_history` |
| `storage.deleted_after_io` |
| `storage.deleted_durability` |
| `storage.future_history` |
| `storage.membership_read_failed` |
| `storage.membership_update_failed` |
| `storage.modification_precedes_creation` |
| `storage.moved_after_io` |
| `storage.moved_durability` |
| `storage.recovered_move` |
| `storage.removed_memberships` |
| `storage.removed_stage` |
| `storage.retained_stage` |
| `storage.saved_after_io` |
| `storage.saved_durability` |
| `storage.trash_link` |
| `storage.unavailable_history_clock` |
| `structure.identity` |
| `structure.voice_budget` |
| `value.depth` |
| `value.duty` |
| `value.peak` |
| `value.positive` |
| `vocabulary.band` |
| `vocabulary.calendar_field` |
| `vocabulary.carrier_kind` |
| `vocabulary.channel` |
| `vocabulary.channel_routing` |
| `vocabulary.completeness` |
| `vocabulary.condition_role` |
| `vocabulary.direction` |
| `vocabulary.evidence_scope` |
| `vocabulary.group_by` |
| `vocabulary.measure` |
| `vocabulary.method` |
| `vocabulary.modulation_kind` |
| `vocabulary.operation` |
| `vocabulary.pair_kind` |
| `vocabulary.parameter_id` |
| `vocabulary.population` |
| `vocabulary.rhythm_state` |
| `vocabulary.scope` |
| `vocabulary.slot_role` |
| `vocabulary.sort_by` |
| `vocabulary.status` |
| `voice.depth` |
| `voice.frequency` |
| `voice.peak_dbfs` |
| `voice.pulse` |
| `voice.sideband` |
| `wav.cancelled` |
| `wav.io` |
| `worker.invalid_outcome` |
| `worker.launch` |
| `worker.panicked` |
| `workspace.action_pending` |
| `workspace.audio_playing` |
| `workspace.detached_draft` |
| `workspace.device_playback_unavailable` |
| `workspace.existing_saved_identity` |
| `workspace.exporting` |
| `workspace.file_busy` |
| `workspace.input` |
| `workspace.invalid` |
| `workspace.no_audio` |
| `workspace.no_export` |
| `workspace.no_selection` |
| `workspace.not_finite` |
| `workspace.prompt_open` |
| `workspace.purge_selection_changed` |
| `workspace.restore_before_playback` |
| `workspace.saved_entry_unreadable` |
| `workspace.selection_changed` |
| `workspace.temporary_required` |
<!-- diagnostic-codes:end -->

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

Writers use an exclusive adjacent stage, synchronize it, check cancellation and publish by no-clobber link or confirmed replacement. Before-publication failure preserves previous bytes. After ambiguous I/O, verified candidate bytes mean saved with a warning; verified previous state means not applied; differing or unreadable state is an error naming the path. Confirmed durability/cleanup warnings remain success. The adapter explicitly synchronizes directory entries on Unix; non-Unix adapters perform no separate directory-sync operation.

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
