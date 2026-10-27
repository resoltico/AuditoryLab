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
    let listing = list(
        ListRequest {
            query: CatalogQuery::default(),
            files: Vec::new(),
        },
        Some(unavailable),
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
        Some(unavailable),
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
    let before = resolve(Source::Selector(record.key.clone()), Some(library.path())).unwrap();
    library
        .trash(
            record.key.clone(),
            Some(record.version_token),
            &Cancel::default(),
        )
        .unwrap();
    let trashed = resolve(Source::Selector(record.key), Some(library.path())).unwrap();
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
fn source_errors_keep_raw_path_data_and_escape_it_only_for_display() {
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
    let text = error.to_string();
    assert!(
        text.starts_with(directory.path().to_str().unwrap()),
        "{text}"
    );
    assert!(text.contains("source\\n\\u{202e}name.toml"), "{text}");
    assert!(!text.contains('\n') && !text.contains('\u{202e}'));
    assert!(text.contains("schema_version 3; this build reads 4"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes.as_bytes());
}
