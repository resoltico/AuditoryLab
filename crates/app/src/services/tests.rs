//! Service boundaries use real files and captured state; no test starts audio.
use super::*;
use crate::core::query::Scope;
use crate::document::origin::LibraryKey;
use library::{DiagnosticDetail, Library};
use {
    crate::audio::PlaybackError, crate::discovery::collection::CatalogQuery,
    crate::outcome::Outcome, crate::storage::library::LibraryError,
};

fn directory() -> tempfile::TempDir {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    tempfile::tempdir_in(base).unwrap()
}
#[test]
fn unavailable_root_keeps_builtin_rows_and_its_typed_failure() {
    let unavailable = std::path::Path::new("relative-unavailable-root");
    let context = LibraryContext::resolve(Some(unavailable));
    assert_eq!(context.source(), library::RootSource::CommandLine);
    assert_eq!(context.location().unwrap().path(), unavailable);
    assert!(context.problem().is_some());
    let listing = list(
        ListRequest {
            query: CatalogQuery::default(),
            files: Vec::new(),
        },
        &context,
        &Cancel::default(),
    )
    .unwrap();
    assert!(!listing.complete());
    assert_eq!(listing.outcome(), Some(Outcome::IncompleteInventory));
    assert_eq!(listing.entries().len(), crate::core::catalog::ENTRIES.len());
    assert!(
        listing
            .entries()
            .iter()
            .all(|entry| matches!(entry.key, LibraryKey::BuiltIn(_)))
    );
    assert!(
        matches!(&listing.diagnostics()[0].detail, DiagnosticDetail::Failure(error) if matches!(error.as_ref(), LibraryError::RelativeRoot { path } if path == unavailable))
    );
    let builtin = list(
        ListRequest {
            query: CatalogQuery {
                scope: Scope::BuiltIn,
                ..Default::default()
            },
            files: Vec::new(),
        },
        &context,
        &Cancel::default(),
    )
    .unwrap();
    assert!(builtin.complete() && builtin.diagnostics().is_empty());
    assert_eq!(builtin.entries().len(), listing.entries().len());
}
#[test]
fn add_as_copy_creates_independent_history_without_rewriting_input() {
    let directory = directory();
    let input = directory.path().join("original.toml");
    let original = crate::test_recipes::silent(1.0, 0.0);
    let bytes = original.to_toml().unwrap();
    std::fs::write(&input, &bytes).unwrap();
    let library = Library::at_path(directory.path().join("library")).unwrap();
    library.initialize().unwrap();
    let imported = library
        .add_file(&input, false, &Cancel::default())
        .unwrap()
        .record
        .unwrap();
    let copied = library
        .add_file(&input, true, &Cancel::default())
        .unwrap()
        .record
        .unwrap();
    assert_eq!(
        imported.snapshot.recipe().document().id,
        original.document().id
    );
    assert_ne!(
        copied.snapshot.recipe().document().id,
        original.document().id
    );
    assert_eq!(copied.snapshot.recipe().sound(), original.sound());
    assert!(copied.snapshot.recipe().document().created_at.is_some());
    assert_eq!(
        copied
            .snapshot
            .recipe()
            .lineage()
            .last()
            .unwrap()
            .document_id,
        Some(original.document().id)
    );
    assert_eq!(std::fs::read(&input).unwrap(), bytes.as_bytes());
    assert_eq!(library.scan(&Cancel::default()).entries.len(), 2);
}
#[test]
fn trash_membership_refuses_current_and_captured_playback_before_device_access() {
    let directory = directory();
    let library = Library::at_path(directory.path().join("library")).unwrap();
    library.initialize().unwrap();
    let snapshot = crate::document::recipe::RecipeSnapshot::new(
        crate::document::recipe::Origin::Stream("silence".into()),
        crate::test_recipes::silent(1.0, 0.0),
    );
    let record = library
        .add_snapshot(&snapshot, false, &Cancel::default())
        .unwrap()
        .record
        .unwrap();
    let context = LibraryContext::from_library(library.clone());
    let before = resolve(Source::Selector(record.key.clone()), &context).unwrap();
    library
        .trash(
            record.key.clone(),
            Some(record.version_token),
            &Cancel::default(),
        )
        .unwrap();
    let trashed = resolve(Source::Selector(record.key), &context).unwrap();
    assert!(trashed.summary().trashed);
    assert_eq!(
        before.summary().version_token,
        trashed.summary().version_token,
        "membership is independent of portable content"
    );
    for (source, captured_before_trash) in [(&before, true), (&trashed, false)] {
        let session = playback::Session::new();
        let control = session.control();
        let error = match playback::prepare_playback(session, source) {
            Err(error) => error,
            Ok(_) => panic!("Trash must be refused before native preparation"),
        };
        if captured_before_trash {
            assert!(matches!(
                error,
                PlaybackError::Library(LibraryError::StalePlayback)
            ));
        } else {
            assert!(matches!(error, PlaybackError::Trashed { .. }));
        }
        assert!(control.phase().is_terminal());
    }
    let session = playback::Session::new();
    session.control().request_stop();
    assert!(
        matches!(
            playback::prepare_playback(session, &trashed).unwrap(),
            playback::Preparation::Stopped(_)
        ),
        "accepted Stop wins before library or device preparation"
    );
}

#[test]
fn retained_user_provenance_does_not_invent_a_managed_selection() {
    let directory = directory();
    let library = Library::at_path(directory.path().join("library")).unwrap();
    library.initialize().unwrap();
    let input = crate::document::recipe::RecipeSnapshot::new(
        crate::document::recipe::Origin::Stream("original".into()),
        crate::test_recipes::silent(1.0, 0.0),
    );
    let record = library
        .add_snapshot(&input, false, &Cancel::default())
        .unwrap()
        .record
        .unwrap();
    let captured = record.snapshot;
    library
        .rename(
            record.key,
            Some(record.version_token),
            "Current saved version".into(),
            &Cancel::default(),
        )
        .unwrap();
    assert!(matches!(
        playback::resolve_captured(&captured, Some(&library)),
        Err(LibraryError::StalePlayback)
    ));
    let retained = playback::resolve_captured(&captured, None).unwrap();
    assert_eq!(retained.id(), captured.id());
    assert_eq!(retained.recipe(), captured.recipe());
    assert_eq!(retained.origin(), captured.origin());
}

#[test]
fn source_errors_keep_raw_path_data() {
    let directory = directory();
    let path = directory.path().join("source\n\u{202e}name.toml");
    let bytes = crate::test_recipes::silent(1.0, 0.0)
        .to_toml()
        .unwrap()
        .replacen("schema_version = 4", "schema_version = 3", 1);
    std::fs::write(&path, &bytes).unwrap();
    let error = open_file(&path).unwrap_err();
    assert!(
        matches!(&error, crate::storage::load::LoadError::Recipe { origin: crate::document::recipe::Origin::File(original), .. } if original == &path)
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes.as_bytes());
}

#[cfg(unix)]
#[test]
fn library_context_keeps_its_captured_root_and_revalidates_action_state() {
    use crate::document::editing::Draft;
    let directory = directory();
    let first = Library::at_path(directory.path().join("first")).unwrap();
    let second = Library::at_path(directory.path().join("second")).unwrap();
    for (library, title) in [(&first, "First root"), (&second, "Second root")] {
        library.initialize().unwrap();
        let mut draft = Draft::new().unwrap();
        draft.set_title(title.into());
        library
            .apply(
                library::Mutation::Create(std::sync::Arc::new(draft.recipe().unwrap())),
                &Cancel::default(),
            )
            .unwrap();
    }
    let alias = directory.path().join("selected");
    std::os::unix::fs::symlink(first.path(), &alias).unwrap();
    let context = LibraryContext::resolve(Some(&alias));
    assert_eq!(
        context.location().unwrap().path(),
        std::fs::canonicalize(first.path()).unwrap()
    );
    assert_eq!(context.source(), library::RootSource::CommandLine);
    std::fs::remove_file(&alias).unwrap();
    std::os::unix::fs::symlink(second.path(), &alias).unwrap();
    let request = || ListRequest {
        query: CatalogQuery {
            scope: Scope::User,
            ..Default::default()
        },
        files: Vec::new(),
    };
    let captured = list(request(), &context, &Cancel::default()).unwrap();
    assert_eq!(captured.entries()[0].title, "First root");
    let selected = LibraryContext::resolve(Some(&alias));
    assert_eq!(
        list(request(), &selected, &Cancel::default())
            .unwrap()
            .entries()[0]
            .title,
        "Second root"
    );
    std::fs::write(first.path().join("library.toml"), "layout = 999\n").unwrap();
    assert!(
        context.problem().is_none(),
        "configuration availability does not authorize later filesystem state"
    );
    let changed = list(request(), &context, &Cancel::default()).unwrap();
    assert!(!changed.complete());
    assert!(
        changed.entries().is_empty(),
        "a failed captured root never falls back to the replacement alias"
    );
    assert!(changed.diagnostics().iter().any(|diagnostic|matches!(&diagnostic.detail,DiagnosticDetail::Failure(error) if matches!(error.as_ref(),LibraryError::UnsupportedLayout {found:999}))));
}

#[test]
fn export_rendition_uses_the_capture_even_after_its_cache_entry_is_evicted() {
    let directory = directory();
    let snapshot = crate::document::recipe::RecipeSnapshot::new(
        crate::document::recipe::Origin::Stream("captured silence".into()),
        crate::core::test_support::silent(0.01, 0.0),
    );
    let captured = snapshot.at(SampleRate::DEFAULT).unwrap();
    snapshot.at(SampleRate::new(96000).unwrap()).unwrap();
    snapshot.at(SampleRate::new(192000).unwrap()).unwrap();
    assert_eq!(snapshot.cached_rates(), 2);
    let compilations = snapshot.realization_count();
    assert_eq!(compilations, 3);
    let path = directory.path().join("captured.wav");
    let exported = export_rendition(
        &captured,
        &path,
        Replace::Never,
        &Cancel::default(),
        &Progress::default(),
    )
    .unwrap();
    assert_eq!(exported.frames, 480);
    assert_eq!(exported.sample_rate, SampleRate::DEFAULT);
    assert_eq!(
        snapshot.realization_count(),
        compilations,
        "export must not realize the snapshot again"
    );
    assert!(path.is_file());
}
