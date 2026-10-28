// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::discovery::query::{Direction, GroupBy, SortBy};
use crate::document::editing::{Draft, EditingField, EditorSession};
use crate::services::library::Library;
use crate::workspace::prompts::PromptState;
use crate::workspace::tasks::Conclusion;
use crate::workspace::{
    Capabilities, Command, EditorCommand, Effect, FileCommand, Job, JobKind, JobOutcome,
    LibraryCommand, PromptAnswer, QueryFacet, Response, ResponseDisposition, Runner, View,
    WorkerFailure, Workspace,
};

fn fixture() -> (tempfile::TempDir, Library, Workspace) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let store = crate::services::library::Library::at_path(temp.path().join("library")).unwrap();
    store.initialize().unwrap();
    let mut workspace = Workspace::with_library(
        crate::services::catalog().unwrap(),
        Capabilities::no_device_playback(),
        store.clone(),
    )
    .unwrap();
    idle(&mut workspace);
    (temp, store, workspace)
}

fn idle(w: &mut Workspace) {
    for _ in 0..100 {
        let effects = w.take_effects();
        let mut retained = Vec::new();
        for effect in effects {
            match effect {
                Effect::Run(job) if job.kind() == JobKind::File => {
                    let response = job.execute(|prepared| {
                        w.respond(prepared);
                    });
                    w.respond(response);
                }
                effect => retained.push(effect),
            }
        }
        w.effects.splice(0..0, retained);
        w.poll();
        if !w.file_busy() && !w.action_pending() {
            return;
        }
    }
    panic!("file work did not finish");
}
fn title(w: &mut Workspace, text: &str) {
    let handle = w
        .editor
        .session()
        .handle(EditingField::Metadata(
            crate::document::editing::MetadataField::Title,
        ))
        .unwrap();
    w.editor.stage(handle, text).unwrap();
    w.editor.commit().unwrap();
}
fn user(w: &mut Workspace) {
    w.apply(Command::Library(LibraryCommand::SetQuery(Box::new(
        CatalogQuery {
            scope: Scope::User,
            ..CatalogQuery::default()
        },
    ))))
    .unwrap();
    idle(w);
}
#[test]
fn applied_summary_lists_values_from_committed_query_and_refuses_invalid_replacement() {
    use crate::discovery::collection::DateRange;
    let (_temp, _store, mut w) = fixture();
    let from = "2026-03-28T22:00:00Z".parse().unwrap();
    let before = "2026-03-29T21:00:00Z".parse().unwrap();
    let query = CatalogQuery {
        search: "exact search".into(),
        studies: vec!["10.1234/exact".into()],
        populations: vec![crate::core::provenance::Population::Human],
        roles: vec![crate::core::provenance::ConditionRole::Comparison],
        analysis: Some(crate::core::rhythm::Completeness::Invalid),
        rhythm: crate::discovery::query::RhythmQuery {
            bands: vec![crate::core::rhythm::Band::Delta],
            methods: vec![crate::core::rhythm::Method::Binaural],
            state: Some(crate::discovery::query::RhythmState::Multiple),
        },
        created: DateRange {
            from: Some(from),
            before: Some(before),
        },
        modified: DateRange {
            from: Some(from),
            before: None,
        },
        added: DateRange {
            from: None,
            before: Some(before),
        },
        sort: SortBy::Title,
        group_by: GroupBy::Study,
        ..CatalogQuery::default()
    };
    w.apply(Command::Library(LibraryCommand::SetQuery(Box::new(
        query.clone(),
    ))))
    .unwrap();
    let summary = w.catalog_query().clone();
    assert_eq!(&summary, &query);
    let mut invalid = query;
    invalid.created.before = Some(from);
    assert!(
        w.apply(Command::Library(LibraryCommand::SetQuery(Box::new(
            invalid
        ))))
        .is_err()
    );
    assert_eq!(w.catalog_query(), &summary);
}

#[test]
fn summary_rows_sort_by_real_dates_without_loading_documents() {
    let (_temp, _store, mut w) = fixture();
    let p = &mut w.library;
    let mut unknown = p.entries[0].clone();
    unknown.duration_seconds = 60.0;
    let mut known = unknown.clone();
    unknown.document_id = crate::core::DocumentUuid::from_random_bytes([1; 16]);
    unknown.key = LibraryKey::User(unknown.document_id);
    unknown.title = "A unknown edit".into();
    unknown.normalized_title = "a unknown edit".into();
    unknown.created_at = Some("2026-01-01T12:00:00Z".parse().unwrap());
    unknown.modified_at = None;
    unknown.added_at = Some("2026-10-24T12:00:00Z".parse().unwrap());
    known.document_id = crate::core::DocumentUuid::from_random_bytes([2; 16]);
    known.key = LibraryKey::User(known.document_id);
    known.title = "Z known edit".into();
    known.normalized_title = "z known edit".into();
    known.created_at = Some("2026-01-02T12:00:00Z".parse().unwrap());
    known.modified_at = Some("2026-06-01T12:00:00Z".parse().unwrap());
    known.added_at = Some("2026-10-23T12:00:00Z".parse().unwrap());
    p.entries = vec![unknown.clone(), known.clone()];
    p.query = CatalogQuery {
        scope: Scope::User,
        ..CatalogQuery::default()
    };
    p.refilter();
    let (rows, _) = p.rows();
    let rows = row_entries(&rows);
    assert_eq!(
        rows[0].key, known.key,
        "Unknown edit must not sort by newer Added here"
    );
    assert_eq!(rows[1].modified_at, None);
    assert!(rows[1].added_at.is_some());
    assert_eq!(rows[0].duration_seconds, 60.0);
    p.query.sort = SortBy::Created;
    p.query.direction = Some(Direction::Asc);
    p.refilter();
    let (rows, _) = p.rows();
    let rows = row_entries(&rows);
    assert_eq!(rows[0].key, unknown.key);
    assert_eq!(rows[0].created_at, unknown.created_at);
    p.query.sort = SortBy::Added;
    p.query.direction = Some(Direction::Desc);
    p.refilter();
    let (rows, _) = p.rows();
    let rows = row_entries(&rows);
    assert_eq!(rows[0].key, unknown.key);
    assert_eq!(rows[0].modified_at, None);
    assert!(rows[0].added_at.is_some());
    assert!(
        p.loaded.is_none(),
        "Rows consume summaries, not full selected User documents"
    );
}

#[test]
fn save_publishes_captured_history_and_keeps_later_draft_and_filters() {
    let (_temp, store, mut w) = fixture();
    let _ = w.apply(Command::Editor(EditorCommand::New));
    title(&mut w, "captured");
    // Capture before a later edit; the real worker publishes only this Arc.
    let content = Arc::new(w.editor.session().draft().recipe().unwrap());
    w.start_managed_save(w.editor.session().document(), content)
        .unwrap();
    title(&mut w, "later");
    idle(&mut w);
    assert!(w.editor.session().dirty());
    assert_eq!(w.editor.session().draft().title(), "later");
    assert_eq!(w.catalog_query().scope, Scope::All);
    let inventory = store.scan(&Cancel::default());
    assert_eq!(inventory.entries.len(), 1);
    assert_eq!(inventory.entries[0].title, "captured");
    assert!(inventory.entries[0].created_at.is_some());
    user(&mut w);
    assert_eq!(w.selected().unwrap().title(), "captured");
    assert!(w.editor.session().dirty());
    w.save().unwrap();
    idle(&mut w);
    assert!(!w.editor.session().dirty());
    assert_eq!(store.scan(&Cancel::default()).entries[0].title, "later");
}
#[test]
fn two_editors_conflict_even_for_noop_and_refresh_preserves_draft() {
    let (_temp, store, mut w) = fixture();
    w.save().unwrap();
    idle(&mut w);
    user(&mut w);
    let _ = w.apply(Command::Library(LibraryCommand::EditSelected));
    let mut other = Workspace::with_library(
        crate::services::catalog().unwrap(),
        Capabilities::no_device_playback(),
        store.clone(),
    )
    .unwrap();
    idle(&mut other);
    user(&mut other);
    let _ = other.apply(Command::Library(LibraryCommand::EditSelected));
    w.apply(Command::Library(LibraryCommand::Mutate {
        entry: w.selected_entry_handle().unwrap(),
        mutation: EntryMutation::Rename {
            title: "renamed elsewhere".into(),
        },
    }))
    .unwrap();
    idle(&mut w);
    other.save().unwrap();
    idle(&mut other);
    assert!(
        matches!(&other.files.status(),FileStatus::Library(LibraryStatus::Failed {error,..}
            ) if error.outcome()==crate::outcome::Outcome::Conflict)
    );
    assert_eq!(other.editor.session().draft().title(), "Untitled sound");
    let _ = other.apply(Command::Library(LibraryCommand::Refresh));
    idle(&mut other);
    assert_eq!(other.selected().unwrap().title(), "renamed elsewhere");
    assert_eq!(other.editor.session().draft().title(), "Untitled sound");
    assert!(
        other
            .editor
            .session()
            .draft()
            .recipe()
            .unwrap()
            .document()
            .created_at
            .is_some()
    );
}
#[test]
fn trash_restore_and_purge_use_exact_gesture_and_explicit_prompt() {
    let (_temp, store, mut w) = fixture();
    w.save().unwrap();
    idle(&mut w);
    user(&mut w);
    let (key, _, _) = binding(w.library_binding());
    w.apply(Command::Library(LibraryCommand::Mutate {
        entry: w.selected_entry_handle().unwrap(),
        mutation: EntryMutation::Trash,
    }))
    .unwrap();
    idle(&mut w);
    assert!(w.library_view().rows.is_empty());
    w.apply(Command::Library(LibraryCommand::SetQuery(Box::new(
        CatalogQuery {
            scope: Scope::Trash,
            ..CatalogQuery::default()
        },
    ))))
    .unwrap();
    idle(&mut w);
    let (_, _, trashed) = binding(w.library_binding());
    assert!(trashed);
    assert!(w.prepare_audio().is_err());
    w.apply(Command::Library(LibraryCommand::Mutate {
        entry: w.selected_entry_handle().unwrap(),
        mutation: EntryMutation::Restore,
    }))
    .unwrap();
    idle(&mut w);
    user(&mut w);

    w.apply(Command::Library(LibraryCommand::Mutate {
        entry: w.selected_entry_handle().unwrap(),
        mutation: EntryMutation::Trash,
    }))
    .unwrap();
    idle(&mut w);
    w.apply(Command::Library(LibraryCommand::SetQuery(Box::new(
        CatalogQuery {
            scope: Scope::Trash,
            ..CatalogQuery::default()
        },
    ))))
    .unwrap();
    idle(&mut w);

    w.apply(Command::Library(LibraryCommand::Mutate {
        entry: w.selected_entry_handle().unwrap(),
        mutation: EntryMutation::RequestPurge,
    }))
    .unwrap();
    let prompt = w
        .take_effects()
        .into_iter()
        .find_map(|e| match e {
            Effect::AskPurge {
                prompt,
                key: prompted,
                ..
            } => {
                assert_eq!(prompted, key);
                Some(prompt)
            }
            _ => None,
        })
        .unwrap();
    assert!(
        store.load(&key.parse().unwrap()).is_ok(),
        "confirmation occurs before any deletion"
    );
    w.apply(Command::Answer {
        prompt,
        answer: PromptAnswer::Confirm,
    })
    .unwrap();
    idle(&mut w);
    assert!(store.scan(&Cancel::default()).entries.is_empty());
    assert!(w.library_data().detached.is_some());
    assert!(
        w.save().is_err(),
        "purging a saved entry cannot turn the retained draft into an implicit \
            Create with the old UUID"
    );
    assert!(store.scan(&Cancel::default()).entries.is_empty());
    w.save_independent_copy().unwrap();
    idle(&mut w);
    let inventory = store.scan(&Cancel::default());
    assert_eq!(inventory.entries.len(), 1);
    assert_ne!(inventory.entries[0].key.to_string(), key);
}
#[test]
fn incomplete_scan_retains_old_rows_and_never_overwrites_open_editor() {
    let (_temp, store, mut w) = fixture();
    w.save().unwrap();
    idle(&mut w);
    user(&mut w);
    let (key, _, _) = binding(w.library_binding());
    let record = store.load(&key.parse().unwrap()).unwrap();
    std::fs::write(
        store
            .path()
            .join("recipes")
            .join(format!("{}.toml", record.snapshot.recipe().document().id)),
        b"broken",
    )
    .unwrap();
    let _ = w.apply(Command::Library(LibraryCommand::Refresh));
    idle(&mut w);
    assert_eq!(
        w.library_view()
            .rows
            .iter()
            .filter(|row| matches!(row, super::super::LibraryRowView::Entry { .. }))
            .count(),
        1
    );
    assert!(w.library_view().rows.iter().any(|row| matches!(
        row,
        super::super::LibraryRowView::Entry {
            requires_read: true,
            ..
        }
    )));
    assert!(w.library.selected_unreadable());
    assert!(w.library_binding().unwrap().version_token.is_none());
    assert!(!w.library_data().inventory.complete);
    assert_eq!(w.editor.session().draft().title(), "Untitled sound");
}

#[test]
fn save_before_trash_uses_confirmed_new_token_and_preserves_published_edit() {
    let (_temp, store, mut w) = fixture();
    w.save().unwrap();
    idle(&mut w);
    user(&mut w);
    let _ = w.apply(Command::Library(LibraryCommand::EditSelected));
    title(&mut w, "save before trash");
    let (key, _, _) = binding(w.library_binding());
    w.apply(Command::Library(LibraryCommand::Mutate {
        entry: w.selected_entry_handle().unwrap(),
        mutation: EntryMutation::Trash,
    }))
    .unwrap();
    let prompt = w
        .take_effects()
        .into_iter()
        .find_map(|e| {
            if let Effect::AskUnsaved(prompt) = e {
                Some(prompt)
            } else {
                None
            }
        })
        .unwrap();
    w.apply(Command::Answer {
        prompt,
        answer: PromptAnswer::Save,
    })
    .unwrap();
    idle(&mut w);
    let record = store.load(&key.parse().unwrap()).unwrap();
    assert!(record.trashed_at.is_some());
    assert_eq!(record.snapshot.title(), "save before trash");
    assert_eq!(w.editor.session().draft().title(), "Untitled sound");
}
#[test]

fn discard_before_trash_preserves_edits_made_after_the_job_was_captured() {
    let (_temp, store, mut w) = fixture();
    w.save().unwrap();
    idle(&mut w);
    user(&mut w);
    let _ = w.apply(Command::Library(LibraryCommand::EditSelected));
    title(&mut w, "before request");
    let (key, _, _) = binding(w.library_binding());
    let document = w.editor().document();
    w.apply(Command::Library(LibraryCommand::Mutate {
        entry: w.selected_entry_handle().unwrap(),
        mutation: EntryMutation::Trash,
    }))
    .unwrap();
    let prompt = w
        .take_effects()
        .into_iter()
        .find_map(|effect| match effect {
            Effect::AskUnsaved(prompt) => Some(prompt),
            _ => None,
        })
        .unwrap();
    w.apply(Command::Answer {
        prompt,
        answer: PromptAnswer::Discard,
    })
    .unwrap();
    let job = w
        .take_effects()
        .into_iter()
        .find_map(|effect| match effect {
            Effect::Run(job) => Some(job),
            _ => None,
        })
        .unwrap();
    title(&mut w, "later edit retained");
    let response = job.execute(|prepared| {
        w.respond(prepared);
    });
    w.respond(response);
    idle(&mut w);
    assert_eq!(w.editor().document(), document);
    assert_eq!(w.editor().draft().title(), "later edit retained");
    assert!(w.editor().dirty());
    assert!(store.load(&key.parse().unwrap()).unwrap().trashed);
}
#[test]
fn managed_playback_worker_refuses_changed_or_trashed_observation() {
    let (_temp, store, mut w) = fixture();
    w.save().unwrap();
    idle(&mut w);
    user(&mut w);
    let captured = w.selected().unwrap().clone();
    assert!(crate::services::playback::revalidate_managed(&captured, &store).is_ok());
    let record = store.load(w.library_binding().unwrap().key).unwrap();
    store
        .apply(
            Mutation::Trash {
                key: record.key,
                expect: Some(record.version_token),
            },
            &Cancel::default(),
        )
        .unwrap();
    std::fs::remove_file(store.path().join("membership.toml")).unwrap();
    assert_eq!(
        crate::services::playback::revalidate_managed(&captured, &store)
            .unwrap_err()
            .outcome(),
        crate::outcome::Outcome::Conflict
    );
    assert!(!w.audio_busy());
    assert_eq!(captured.title(), "Untitled sound");
}

#[test]
fn retained_user_source_is_captured_temporary_content_not_a_fresh_user_selection() {
    let (_temp, store, mut w) = fixture();
    w.save().unwrap();
    idle(&mut w);
    user(&mut w);
    let original = w.selected().unwrap().clone();
    let _ = w.apply(Command::Editor(EditorCommand::CopySelected));
    let key: LibraryKey = format!("user:{}", original.recipe().document().id)
        .parse()
        .unwrap();
    let record = store.load(&key).unwrap();
    store
        .apply(
            Mutation::Rename {
                key,
                expect: Some(record.version_token),
                title: "Changed current User".into(),
            },
            &Cancel::default(),
        )
        .unwrap();
    let _ = w.apply(Command::Editor(EditorCommand::ShowBasedOnSource));
    assert_eq!(w.selected().unwrap().id(), original.id());
    assert_eq!(w.selected().unwrap().title(), "Untitled sound");
    assert_eq!(w.catalog_query().scope, Scope::OpenFiles);
    assert!(!w.library.selected_user());
    assert!(!w.file_busy());
    assert!(!w.audio_busy());
}

#[test]

fn a_failed_completion_keeps_draft_and_retry_observes_actual_storage() {
    for applied in [false, true] {
        let (_temp, store, mut workspace) = fixture();
        title(&mut workspace, "Unsaved draft must survive");
        workspace.save().unwrap();
        let job = workspace
            .take_effects()
            .into_iter()
            .find_map(|effect| match effect {
                Effect::Run(job) => Some(job),
                _ => None,
            })
            .unwrap();
        let id = job.id();
        if applied {
            let _ = job.execute(|_| {});
        } else {
            drop(job);
        }
        workspace.respond(Response::Finished {
            job: id,
            outcome: JobOutcome::Failed(WorkerFailure::Panicked),
        });
        assert!(workspace.editor().dirty());
        assert!(matches!(
            workspace.files.status(),
            FileStatus::WorkerFailed {
                error: WorkerFailure::Panicked,
                ..
            }
        ));
        workspace.save().unwrap();
        idle(&mut workspace);
        if applied {
            assert!(workspace.editor().dirty());
            assert!(
                matches!(&workspace.files.status(),FileStatus::Library(LibraryStatus::Failed {error,
                    ..}) if error.outcome()==crate::outcome::Outcome::Conflict)
            );
            workspace.save_independent_copy().unwrap();
            idle(&mut workspace);
            assert_eq!(store.scan(&Cancel::default()).entries.len(), 2);
        } else {
            assert_eq!(store.scan(&Cancel::default()).entries.len(), 1);
        }
        assert!(!workspace.editor().dirty());
    }
}

#[test]
fn refresh_reloads_membership_state_even_when_recipe_sha_is_unchanged() {
    let (_temp, store, mut workspace) = fixture();
    workspace.save().unwrap();
    idle(&mut workspace);
    user(&mut workspace);
    let (binding, token, _) = binding(workspace.library_binding());
    let key: LibraryKey = binding.parse().unwrap();
    store
        .apply(
            Mutation::Trash {
                key: key.clone(),
                expect: Some(token.parse().unwrap()),
            },
            &Cancel::default(),
        )
        .unwrap();
    std::fs::remove_file(store.path().join("membership.toml")).unwrap();
    workspace
        .set_catalog_query(Box::new(CatalogQuery {
            scope: Scope::Trash,
            ..CatalogQuery::default()
        }))
        .unwrap();
    let _ = workspace.apply(Command::Library(LibraryCommand::Refresh));
    idle(&mut workspace);
    assert!(workspace.library.selected_trashed());
    let loaded = &workspace.library.loaded.as_ref().unwrap();
    assert!(loaded.trashed);
    assert_eq!(loaded.trashed_at, None);
    assert_eq!(loaded.version_token.to_string(), token);
}

#[test]
fn confirmed_trash_retains_reset_failure_and_blocks_dependent_completion() {
    for invalid_editor in [false, true] {
        let (_temp, store, mut workspace) = fixture();
        workspace.save().unwrap();
        idle(&mut workspace);
        let key = store.scan(&Cancel::default()).entries[0].key.clone();
        let record = store.load(&key).unwrap();
        let outcome = store
            .apply(
                Mutation::Trash {
                    key: key.clone(),
                    expect: Some(record.version_token),
                },
                &Cancel::default(),
            )
            .unwrap();
        let publication = outcome.status;
        let result = workspace.accept_managed_outcome(None, outcome);
        let retained = workspace.editor().document();
        let replacement = if invalid_editor {
            Recipe::parse(include_str!(
                "../../../../core/tests/fixtures/invalid/duplicate-id.toml"
            ))
            .map_err(crate::document::error::EditorError::Recipe)
            .and_then(|recipe| EditorSession::new(Draft::from_recipe(recipe)))
        } else {
            Err(crate::document::error::EditorError::Identity(
                crate::identity::IdentityError::Entropy(getrandom::Error::UNSUPPORTED),
            ))
        };
        assert!(replacement.is_err());
        let (status, confirmed) = workspace.finish_discard_reset(result, replacement);
        assert!(store.load(&key).unwrap().trashed);
        assert_eq!(workspace.editor().document(), retained);
        assert!(matches!(workspace.editor.location(),
                DocumentLocation::Detached(detached) if detached==&key));
        assert!(
            matches!(&status, FileStatus::Library(LibraryStatus::Written { status, .. }
                ) if *status==publication)
        );
        assert!(status.editor_reset_error().is_some());
        assert_eq!(status.conclusion(Conclusion::Succeeded), Conclusion::Failed);
        assert_eq!(confirmed, Some(false));
        assert!(status.editor_reset_error().is_some());
        // A later background read retains the banner but does not make the old
        // reset failure a new shutdown failure.
        workspace.files.record_status(status);
        let _ = workspace.apply(Command::Library(LibraryCommand::Refresh));
        assert!(workspace.file_busy());
        Runner::new().shutdown(workspace).unwrap();
    }
}

fn selected_users() -> (
    tempfile::TempDir,
    Library,
    Workspace,
    LibraryKey,
    LibraryKey,
) {
    let (root, store, _) = fixture();
    let mut keys = Vec::new();
    for name in ["Shared A", "Shared B"] {
        let mut draft = Draft::new().unwrap();
        draft.set_title(name.into());
        let result = store
            .apply(
                Mutation::Create(Arc::new(draft.recipe().unwrap())),
                &Cancel::default(),
            )
            .unwrap();
        keys.push(result.key);
    }
    let mut workspace = Workspace::with_library(
        crate::services::catalog().unwrap(),
        Capabilities::available(),
        store.clone(),
    )
    .unwrap();
    idle(&mut workspace);
    workspace
        .set_catalog_query(Box::new(CatalogQuery {
            scope: Scope::User,
            search: "Shared".into(),
            sort: crate::discovery::query::SortBy::Title,
            direction: Some(crate::discovery::query::Direction::Desc),
            group_by: crate::discovery::query::GroupBy::Collection,
            ..CatalogQuery::default()
        }))
        .unwrap();
    idle(&mut workspace);
    workspace
        .apply(Command::Library(LibraryCommand::Select(
            workspace.entry_handle(&keys[0]).unwrap(),
        )))
        .unwrap();
    idle(&mut workspace);
    (root, store, workspace, keys.remove(0), keys.remove(0))
}
fn captured_file(workspace: &mut Workspace) -> Job {
    let mut captured = None;
    let mut retained = Vec::new();
    for effect in workspace.take_effects() {
        match effect {
            Effect::Run(job) if job.kind() == JobKind::File => {
                assert!(captured.is_none());
                captured = Some(job);
            }
            effect => retained.push(effect),
        }
    }
    workspace.effects.extend(retained);
    captured.expect("one captured real file job")
}
fn complete_file(workspace: &mut Workspace, job: Job) {
    let response = job.execute(|_| panic!("file work cannot prepare audio"));
    assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
}
fn selected_ready(workspace: &mut Workspace, key: &LibraryKey, title: &str, query: &CatalogQuery) {
    assert_eq!(workspace.catalog_query(), query);
    assert_eq!(workspace.library.selection.as_ref(), Some(key));
    assert_eq!(
        workspace.library.loaded.as_ref().map(|record| &record.key),
        Some(key)
    );
    assert_eq!(workspace.selected().unwrap().title(), title);
    let view = workspace.library_view();
    assert_eq!(view.snapshot.as_ref().unwrap().title(), title);
    let actions = workspace.action_view();
    assert!(actions.can_play());
    assert!(actions.can_copy());
    assert!(actions.can_export());
}

#[test]
fn independent_copy_save_keeps_the_original_selected_user_cache() {
    let (_root, store, mut workspace, original, _) = selected_users();
    let query = workspace.catalog_query().clone();
    let selected = workspace.selected().unwrap().id();
    let _ = workspace.apply(Command::Editor(EditorCommand::CopySelected));
    workspace.save_independent_copy().unwrap();
    let job = captured_file(&mut workspace);
    complete_file(&mut workspace, job);
    assert!(
        !workspace.file_busy(),
        "saving another key needs no selected-record reload"
    );
    assert!(!workspace.editor.session().dirty());
    assert_eq!(store.scan(&Cancel::default()).entries.len(), 3);
    assert_ne!(workspace.editor.location().managed().unwrap().0, &original);
    let _ = workspace.apply(Command::SetView(View::Library));
    selected_ready(&mut workspace, &original, "Shared A", &query);
    assert_eq!(workspace.selected().unwrap().id(), selected);
    workspace
        .apply(Command::Library(LibraryCommand::Select(
            workspace.entry_handle(&original).unwrap(),
        )))
        .unwrap();
    assert!(
        !workspace.file_busy(),
        "clicking the same valid selected row needs no Refresh"
    );
    selected_ready(&mut workspace, &original, "Shared A", &query);
}

#[test]
fn saving_an_existing_nonselected_user_keeps_selected_details_and_filters() {
    let (_root, store, mut workspace, selected, edited) = selected_users();
    workspace
        .apply(Command::Library(LibraryCommand::Select(
            workspace.entry_handle(&edited).unwrap(),
        )))
        .unwrap();
    idle(&mut workspace);
    let _ = workspace.apply(Command::Library(LibraryCommand::EditSelected));
    title(&mut workspace, "Shared B edited");
    workspace
        .apply(Command::Library(LibraryCommand::Select(
            workspace.entry_handle(&selected).unwrap(),
        )))
        .unwrap();
    idle(&mut workspace);
    let query = workspace.catalog_query().clone();
    let snapshot = workspace.selected().unwrap().id();
    workspace.save().unwrap();
    let job = captured_file(&mut workspace);
    complete_file(&mut workspace, job);
    assert!(!workspace.file_busy());
    assert_eq!(
        store.load(&edited).unwrap().snapshot.title(),
        "Shared B edited"
    );
    assert!(!workspace.editor.session().dirty());
    selected_ready(&mut workspace, &selected, "Shared A", &query);
    assert_eq!(workspace.selected().unwrap().id(), snapshot);
}

#[test]
fn captured_user_save_finishing_after_navigation_loads_only_the_current_selection() {
    let (_root, store, mut workspace, original, edited) = selected_users();
    workspace
        .apply(Command::Library(LibraryCommand::Select(
            workspace.entry_handle(&edited).unwrap(),
        )))
        .unwrap();
    idle(&mut workspace);
    let _ = workspace.apply(Command::Library(LibraryCommand::EditSelected));
    title(&mut workspace, "Shared B captured");
    let query = workspace.catalog_query().clone();
    workspace.save().unwrap();
    let save = captured_file(&mut workspace);
    workspace
        .apply(Command::Library(LibraryCommand::Select(
            workspace.entry_handle(&original).unwrap(),
        )))
        .unwrap();
    complete_file(&mut workspace, save);
    assert_eq!(workspace.library.selection.as_ref(), Some(&original));
    let load = captured_file(&mut workspace);
    complete_file(&mut workspace, load);
    assert!(
        !workspace.file_busy(),
        "one captured reload reaches the selected record"
    );
    assert_eq!(
        store.load(&edited).unwrap().snapshot.title(),
        "Shared B captured"
    );
    selected_ready(&mut workspace, &original, "Shared A", &query);
    workspace
        .apply(Command::Library(LibraryCommand::Select(
            workspace.entry_handle(&edited).unwrap(),
        )))
        .unwrap();
    idle(&mut workspace);
    selected_ready(&mut workspace, &edited, "Shared B captured", &query);
}

#[test]
fn a_load_completed_after_navigation_cannot_own_the_cache_or_suppress_later_reads() {
    let (_root, _store, mut workspace, original, other) = selected_users();
    let query = workspace.catalog_query().clone();
    workspace
        .apply(Command::Library(LibraryCommand::Select(
            workspace.entry_handle(&other).unwrap(),
        )))
        .unwrap();
    let obsolete = captured_file(&mut workspace);
    workspace
        .apply(Command::Library(LibraryCommand::Select(
            workspace.entry_handle(&original).unwrap(),
        )))
        .unwrap();
    complete_file(&mut workspace, obsolete);
    assert_eq!(workspace.library.selection.as_ref(), Some(&original));
    let current = captured_file(&mut workspace);
    complete_file(&mut workspace, current);
    assert!(!workspace.file_busy());
    selected_ready(&mut workspace, &original, "Shared A", &query);
    workspace
        .apply(Command::Library(LibraryCommand::Select(
            workspace.entry_handle(&other).unwrap(),
        )))
        .unwrap();
    let later = captured_file(&mut workspace);
    complete_file(&mut workspace, later);
    assert!(!workspace.file_busy());
    selected_ready(&mut workspace, &other, "Shared B", &query);
}

#[test]
fn retained_user_source_exports_its_captured_portable_text_without_reloading_user() {
    let (_temp, store, mut workspace) = fixture();
    workspace.save().unwrap();
    idle(&mut workspace);
    user(&mut workspace);
    let original = workspace.selected().unwrap().clone();
    let _ = workspace.apply(Command::Editor(EditorCommand::CopySelected));
    let key = LibraryKey::User(original.recipe().document().id);
    let record = store.load(&key).unwrap();
    store
        .rename(
            key,
            Some(record.version_token),
            "Current saved version".into(),
            &Cancel::default(),
        )
        .unwrap();
    let _ = workspace.apply(Command::Editor(EditorCommand::ShowBasedOnSource));
    assert_eq!(workspace.catalog_query().scope, Scope::OpenFiles);
    assert!(!workspace.library.selected_user());
    let next = workspace.next_id;
    for _ in 0..20 {
        assert!(workspace.action_view().can_export_recipe());
    }
    assert_eq!(workspace.next_id, next);
    let first = workspace.prepare_export_recipe().unwrap().0;
    let second = workspace.prepare_export_recipe().unwrap().0;
    assert!(
        Arc::ptr_eq(&first, &second),
        "immutable captured text is reused by readiness and execution"
    );
    workspace
        .apply(Command::File(FileCommand::ExportRecipe))
        .unwrap();
    let Some(prompt) = workspace.prompts.current.as_ref() else {
        panic!("portable export prompt");
    };
    let PromptState::ExportRecipe(text) = &prompt.state else {
        panic!("captured portable text");
    };
    assert_eq!(
        crate::core::Recipe::parse(text).unwrap(),
        *original.recipe()
    );
    assert!(!text.contains("Current saved version"));
}

fn binding(binding: Option<SelectionBinding<'_>>) -> (String, String, bool) {
    let binding = binding.unwrap();
    (
        binding.key.to_string(),
        binding
            .version_token
            .map(|token| token.to_string())
            .unwrap_or_default(),
        binding.trashed,
    )
}
fn row_entries(rows: &[super::super::LibraryRowView]) -> Vec<&EntrySummary> {
    rows.iter()
        .filter_map(|row| match row {
            super::super::LibraryRowView::Entry { entry, .. } => Some(entry.as_ref()),
            _ => None,
        })
        .collect()
}

#[test]
fn superseded_listing_and_foreign_handles_are_refused() {
    let (_root, _store, mut workspace) = fixture();
    let stale = workspace.selected_entry_handle().unwrap();
    workspace
        .apply(Command::Library(LibraryCommand::Search("theta".into())))
        .unwrap();
    assert!(matches!(
        workspace.apply(Command::Library(LibraryCommand::Select(stale.clone()))),
        Err(Refusal::SelectionChanged)
    ));
    assert!(matches!(
        workspace.apply(Command::Library(LibraryCommand::Mutate {
            entry: stale,
            mutation: EntryMutation::RequestPurge
        })),
        Err(Refusal::PurgeSelectionChanged)
    ));
    let (_other_root, _other_store, other) = fixture();
    assert!(matches!(
        workspace.apply(Command::Library(LibraryCommand::Select(
            other.selected_entry_handle().unwrap()
        ))),
        Err(Refusal::SelectionChanged)
    ));
}

#[test]
fn keyed_query_edits_reject_unknown_options_atomically() {
    let (_root, _store, mut workspace) = fixture();
    let before = workspace.catalog_query().clone();
    assert!(matches!(
        workspace.apply(Command::Library(LibraryCommand::SetFacets(vec![
            (QueryFacet::Scope, "user".into()),
            (QueryFacet::Method, "not_offered".into())
        ]))),
        Err(Refusal::SelectionChanged)
    ));
    assert_eq!(workspace.catalog_query(), &before);
    let bands = workspace.library_choices(QueryFacet::Band);
    let counts = workspace.library_facets();
    assert_eq!(
        bands
            .options
            .iter()
            .map(|option| option.key)
            .collect::<std::collections::BTreeSet<_>>(),
        counts
            .bands
            .iter()
            .map(|count| count.kind.key())
            .collect::<std::collections::BTreeSet<_>>()
    );
}

#[test]
fn clear_rhythm_command_retains_every_non_rhythm_query_field() {
    use crate::discovery::collection::DateRange;
    for scope in [Scope::All, Scope::BuiltIn] {
        let (_root, _store, mut workspace) = fixture();
        let query = CatalogQuery {
            scope,
            include_external: scope == Scope::All,
            search: "40".into(),
            studies: vec!["key:source".into()],
            populations: vec![crate::core::provenance::Population::Human],
            roles: vec![crate::core::provenance::ConditionRole::Test],
            analysis: Some(crate::core::rhythm::Completeness::Complete),
            created: DateRange {
                from: Some("2026-01-01T00:00:00Z".parse().unwrap()),
                before: Some("2026-02-01T00:00:00Z".parse().unwrap()),
            },
            modified: DateRange {
                from: Some("2026-03-01T00:00:00Z".parse().unwrap()),
                before: Some("2026-04-01T00:00:00Z".parse().unwrap()),
            },
            added: DateRange {
                from: Some("2026-05-01T00:00:00Z".parse().unwrap()),
                before: Some("2026-06-01T00:00:00Z".parse().unwrap()),
            },
            direction: Some(Direction::Desc),
            sort: SortBy::Added,
            group_by: GroupBy::Study,
            rhythm: crate::discovery::query::RhythmQuery {
                bands: vec![crate::core::rhythm::Band::Gamma],
                methods: vec![crate::core::rhythm::Method::Am],
                state: Some(crate::discovery::query::RhythmState::OutsideBands),
            },
        };
        workspace
            .apply(Command::Library(LibraryCommand::SetQuery(Box::new(
                query.clone(),
            ))))
            .unwrap();
        assert!(workspace.catalog_query().rhythm_filtered());
        workspace
            .apply(Command::Library(LibraryCommand::ClearRhythmFilters))
            .unwrap();
        let mut expected = query;
        expected.rhythm = crate::discovery::query::RhythmQuery::default();
        assert_eq!(workspace.catalog_query(), &expected);
        assert!(!workspace.catalog_query().rhythm_filtered());
    }
}
