//! Native filesystem lifecycle and strict managed-record behavior through shared APIs.
use auditorylab_app::core::Recipe;
use auditorylab_app::document::editing::Draft;
use auditorylab_app::document::origin::{LibraryKey, VersionToken};
use auditorylab_app::job::Cancel;
use auditorylab_app::outcome::Outcome;
use auditorylab_app::services::library::{Library, Mutation, MutationStatus};
use std::{fs, path::PathBuf, sync::Arc};
#[expect(
    clippy::unwrap_used,
    reason = "A fixture setup failure must fail its calling native store test"
)]
fn fixture() -> (tempfile::TempDir, PathBuf, Library) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let path = temp.path().join("library");
    let store = auditorylab_app::services::library::Library::at_path(path.clone()).unwrap();
    (temp, path, store)
}
#[expect(
    clippy::unwrap_used,
    reason = "A finite authored test recipe with a fresh provisional UUID must \
        construct successfully"
)]
fn recipe(title: &str) -> Recipe {
    let mut draft = Draft::new().unwrap();
    draft.set_title(title.into());
    draft.recipe().unwrap()
}
#[test]
fn readonly_discovery_and_builtin_refusals_never_initialize_a_root() {
    let (_temp, path, store) = fixture();
    let info = store.inspect().unwrap();
    assert!(!info.exists && !info.initialized);
    assert!(!path.exists());
    let scan = store.scan(&Cancel::default());
    assert!(!scan.complete && scan.not_initialized);
    assert!(!path.exists());
    let token: VersionToken = "0".repeat(64).parse().unwrap();
    let error = store
        .apply(
            Mutation::Rename {
                key: LibraryKey::BuiltIn(
                    auditorylab_core::catalog::CatalogId::new("ioannou-2015-binaural-2-hz")
                        .unwrap(),
                ),
                expect: Some(token),
                title: "cannot change shipped".into(),
            },
            &Cancel::default(),
        )
        .unwrap_err();
    assert_eq!(error.outcome(), Outcome::ReadOnlyBuiltIn);
    assert!(!path.exists());
    let builtin = auditorylab_app::services::builtin("ioannou-2015-binaural-2-hz").unwrap();
    let error = store
        .apply(
            Mutation::Add(Arc::new(builtin.recipe().clone())),
            &Cancel::default(),
        )
        .unwrap_err();
    assert_eq!(error.outcome(), Outcome::Conflict);
    assert!(!path.exists());
    assert!(
        store
            .apply(
                Mutation::Create(Arc::new(recipe("new"))),
                &Cancel::default()
            )
            .is_err()
    );
    assert!(!path.exists());
}
#[test]
fn full_lifecycle_preserves_identity_history_external_originals_and_stale_intent() {
    let (temp, path, store) = fixture();
    store.initialize().unwrap();
    assert!(!store.initialize().unwrap().created);
    let original = recipe("Imported unknown history");
    let id = original.document().id;
    let key = LibraryKey::User(id);
    let external = temp.path().join("original.toml");
    let text = original.to_toml().unwrap();
    fs::write(&external, &text).unwrap();
    let first = store
        .apply(
            Mutation::Add(Arc::new(original.clone())),
            &Cancel::default(),
        )
        .unwrap();
    assert_eq!(first.status, MutationStatus::Published);
    let loaded = store.load(&key).unwrap();
    assert_eq!(loaded.snapshot.recipe().document().created_at, None);
    assert_eq!(loaded.snapshot.recipe().document().modified_at, None);
    let added = loaded.added_at;
    let initial = loaded.version_token;
    let record_path = path.join("recipes").join(format!("{id}.toml"));
    let original_bytes = fs::read(&record_path).unwrap();
    assert_eq!(
        store
            .apply(
                Mutation::Add(Arc::new(original.clone())),
                &Cancel::default()
            )
            .unwrap()
            .status,
        MutationStatus::Unchanged
    );
    assert_eq!(fs::read(&record_path).unwrap(), original_bytes);
    let mut collision = original.clone().into_builder();
    collision.content.description.as_mut().unwrap().title = "same UUID different title".into();
    assert_eq!(
        store
            .apply(
                Mutation::Add(Arc::new(collision.build().unwrap())),
                &Cancel::default()
            )
            .unwrap_err()
            .outcome(),
        Outcome::Conflict
    );
    let renamed = store
        .apply(
            Mutation::Rename {
                key: key.clone(),
                expect: Some(initial),
                title: "Renamed".into(),
            },
            &Cancel::default(),
        )
        .unwrap();
    assert!(matches!(
        renamed.status,
        MutationStatus::Published | MutationStatus::PublishedWithWarning
    ));
    let changed = store.load(&key).unwrap();
    assert_eq!(changed.snapshot.recipe().document().id, id);
    assert_eq!(changed.snapshot.recipe().document().created_at, None);
    assert!(changed.snapshot.recipe().document().modified_at.is_some());
    assert_eq!(changed.added_at, added);
    assert_ne!(changed.version_token, initial);
    let changed_bytes = fs::read(&record_path).unwrap();
    let error = store
        .apply(
            Mutation::Update {
                key: key.clone(),
                expect: Some(initial),
                recipe: Arc::new(original),
            },
            &Cancel::default(),
        )
        .unwrap_err();
    assert_eq!(error.outcome(), Outcome::Conflict);
    assert_eq!(fs::read(&record_path).unwrap(), changed_bytes);
    let trash = assert_trash_history(&store, &key, &changed);
    assert_restore_and_purge(&store, &key, &trash, &record_path);
    assert_eq!(
        fs::read_to_string(external).unwrap(),
        text,
        "managed lifecycle never modifies/deletes original external content"
    );
}
#[test]
fn new_creation_dates_are_assigned_only_to_the_confirmed_snapshot_and_noops_keep_bytes() {
    let (_temp, path, store) = fixture();
    store.initialize().unwrap();
    let provisional = recipe("New sound");
    let id = provisional.document().id;
    assert_eq!(provisional.document().created_at, None);
    let result = store
        .apply(
            Mutation::Create(Arc::new(provisional.clone())),
            &Cancel::default(),
        )
        .unwrap();
    let record = result.record.unwrap();
    let identity = &record.snapshot.recipe().document();
    assert_eq!(identity.id, id);
    assert_eq!(identity.created_at, record.added_at);
    assert_eq!(identity.modified_at, identity.created_at);
    assert_eq!(
        provisional.document().created_at,
        None,
        "captured input is immutable"
    );
    let file = path.join("recipes").join(format!("{id}.toml"));
    let bytes = fs::read(&file).unwrap();
    let same = store
        .apply(
            Mutation::Update {
                key: record.key.clone(),
                expect: Some(record.version_token),
                recipe: Arc::new(record.snapshot.recipe().clone()),
            },
            &Cancel::default(),
        )
        .unwrap();
    assert_eq!(same.status, MutationStatus::Unchanged);
    assert_eq!(same.record.unwrap().version_token, record.version_token);
    assert_eq!(fs::read(file).unwrap(), bytes);
}
#[test]
fn exact_byte_digest_detects_manual_whitespace_edits() {
    let (_temp, path, store) = fixture();
    store.initialize().unwrap();
    let result = store
        .apply(Mutation::Add(Arc::new(recipe("bytes"))), &Cancel::default())
        .unwrap();
    let record = result.record.unwrap();
    let path = path
        .join("recipes")
        .join(format!("{}.toml", record.snapshot.recipe().document().id));
    let mut bytes = fs::read(&path).unwrap();
    bytes.push(b' ');
    fs::write(&path, &bytes).unwrap();
    let fresh = store.load(&record.key).unwrap();
    assert_ne!(fresh.version_token, record.version_token);
    assert_eq!(
        store
            .apply(
                Mutation::Update {
                    key: record.key,
                    expect: Some(record.version_token),
                    recipe: Arc::new(record.snapshot.recipe().clone())
                },
                &Cancel::default()
            )
            .unwrap_err()
            .outcome(),
        Outcome::Conflict
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn managed_descendant_symlinks_and_nonregular_entries_never_read_external_content() {
    use std::os::unix::fs::symlink;
    let (temp, path, store) = fixture();
    store.initialize().unwrap();
    let external = temp.path().join("outside");
    fs::write(&external, b"secret outside bytes").unwrap();
    let bad = path.join("recipes/00000000-0000-4000-8000-000000000001.toml");
    symlink(&external, &bad).unwrap();
    let scan = store.scan(&Cancel::default());
    assert!(!scan.complete);
    assert!(scan.entries.is_empty());
    assert!(
        !auditorylab_presentation::errors::library_diagnostic_message(&(scan.diagnostics[0]))
            .contains("secret outside bytes")
    );
    assert_eq!(fs::read(&external).unwrap(), b"secret outside bytes");
    fs::remove_file(bad).unwrap();
    fs::remove_dir(path.join("recipes")).unwrap();
    symlink(temp.path(), path.join("recipes")).unwrap();
    assert!(
        store
            .load(&LibraryKey::User(
                "00000000-0000-4000-8000-000000000001".parse().unwrap()
            ))
            .is_err()
    );
    assert!(!store.scan(&Cancel::default()).complete);
}
#[test]
fn lock_contention_waits_a_bounded_time_and_closure_releases_the_same_persistent_lock() {
    let (_temp, path, store) = fixture();
    store.initialize().unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.join(".library.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let start = std::time::Instant::now();
    let inventory = store.scan(&Cancel::default());
    assert!(!inventory.complete);
    assert_eq!(inventory.failure_kind, Some(Outcome::Busy));
    let waited = start.elapsed();
    let bound = auditorylab_app::services::library::BUSY_WAIT;
    assert!(waited >= bound && waited < bound + std::time::Duration::from_secs(2));
    drop(lock);
    assert!(store.scan(&Cancel::default()).complete);
    assert!(path.join(".library.lock").is_file());
}
#[test]
fn initialization_refuses_unknown_data_and_records_without_a_marker() {
    let (_temp, path, store) = fixture();
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("unrelated.txt"), b"keep").unwrap();
    assert!(store.initialize().is_err());
    assert!(!path.join("library.toml").exists());
    assert_eq!(fs::read(path.join("unrelated.txt")).unwrap(), b"keep");
    fs::remove_file(path.join("unrelated.txt")).unwrap();
    fs::create_dir(path.join("recipes")).unwrap();
    fs::write(path.join("recipes/arbitrary.toml"), b"keep record").unwrap();
    assert!(store.initialize().is_err());
    assert_eq!(
        fs::read(path.join("recipes/arbitrary.toml")).unwrap(),
        b"keep record"
    );
    assert!(!path.join("library.toml").exists());
}

#[test]
fn corrupt_plain_files_and_owned_stages_are_reported_without_read_side_repairs() {
    let (_temp, path, store) = fixture();
    store.initialize().unwrap();
    let record = store
        .apply(
            Mutation::Add(Arc::new(recipe("healthy"))),
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    let id = record.snapshot.recipe().document().id;
    let file = path.join("recipes").join(format!("{id}.toml"));
    let bytes = fs::read(&file).unwrap();
    let stage = path.join("recipes/.auditorylab-00000000-0000-4000-8000-000000000099.partial");
    fs::write(&stage, b"abandoned candidate").unwrap();
    let scan = store.scan(&Cancel::default());
    assert!(scan.complete);
    assert_eq!(scan.entries.len(), 1);
    assert_eq!(scan.stages.len(), 1);
    assert!(stage.exists());
    for corrupted in [
        b"invalid TOML".to_vec(),
        bytes
            .iter()
            .copied()
            .chain(b"\nunsupported = true".iter().copied())
            .collect(),
        String::from_utf8(bytes.clone())
            .unwrap()
            .replace(&id.to_string(), "00000000-0000-4000-8000-000000000099")
            .into_bytes(),
    ] {
        fs::write(&file, &corrupted).unwrap();
        assert!(store.load(&record.key).is_err());
        let scan = store.scan(&Cancel::default());
        assert!(!scan.complete);
        assert!(scan.entries.is_empty());
        assert!(!scan.diagnostics.is_empty());
        assert_eq!(fs::read(&file).unwrap(), corrupted);
        assert!(stage.exists());
    }
}

#[test]
fn unknown_membership_dates_remain_visible_in_human_descriptions() {
    let (_temp, path, store) = fixture();
    store.initialize().unwrap();
    let record = store
        .apply(
            Mutation::Add(Arc::new(recipe("Missing local dates"))),
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    let trashed = store
        .apply(
            Mutation::Trash {
                key: record.key,
                expect: None,
            },
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    fs::remove_file(path.join("membership.toml")).unwrap();
    let summary = store.load(&trashed.key).unwrap().summary();
    assert!(summary.trashed);
    let text = auditorylab_presentation::library::entry_summary_membership_description(&(summary));
    assert!(text.contains("Added here: Unknown"));
    assert!(text.contains("Trash: Unknown"));
    assert!(text.contains(&summary.version_token.unwrap().to_string()));
}

#[test]
fn failed_report_completion_never_claims_a_failed_operation_was_confirmed() {
    let error = std::io::Error::other("rejected operation");
    let report = auditorylab_presentation::report::Report::error(
        auditorylab_app::vocabulary::Operation::LibraryInit,
        &auditorylab_core::SampleRateError { hz: 0 },
        error.to_string(),
        Outcome::InvalidArgument,
    );
    let text = auditorylab_presentation::library::report_completion_failure(
        report.complete,
        std::io::Error::other("closed output"),
    );
    assert!(text.contains("Operation not completed"));
    assert!(!text.contains("Operation confirmed"));
}

#[expect(
    clippy::unwrap_used,
    reason = "Lifecycle fixture failures must fail their calling test"
)]
fn assert_trash_history(
    store: &Library,
    key: &LibraryKey,
    changed: &auditorylab_app::services::library::LoadedRecord,
) -> auditorylab_app::services::library::LoadedRecord {
    let mut wrong_history = changed.snapshot.recipe().clone().into_builder();
    wrong_history.document.created_at = Some("2020-01-01T00:00:00Z".parse().unwrap());
    assert_eq!(
        store
            .apply(
                Mutation::Update {
                    key: key.clone(),
                    expect: Some(changed.version_token),
                    recipe: Arc::new(wrong_history.build().unwrap())
                },
                &Cancel::default()
            )
            .unwrap_err()
            .outcome(),
        Outcome::HistoryConflict
    );
    let history = changed.snapshot.recipe().document().clone();
    let added = changed.added_at;
    let trashed = store
        .apply(
            Mutation::Trash {
                key: key.clone(),
                expect: Some(changed.version_token),
            },
            &Cancel::default(),
        )
        .unwrap();
    let trash = trashed.record.unwrap();
    assert!(trash.trashed_at.is_some());
    assert_eq!(trash.snapshot.recipe().document(), &history);
    assert_eq!(trash.added_at, added);
    assert_eq!(
        store
            .apply(
                Mutation::Add(Arc::new(trash.snapshot.recipe().clone())),
                &Cancel::default()
            )
            .unwrap_err()
            .outcome(),
        Outcome::Conflict
    );

    trash
}

#[expect(
    clippy::unwrap_used,
    reason = "Lifecycle fixture failures must fail their calling test"
)]
fn assert_restore_and_purge(
    store: &Library,
    key: &LibraryKey,
    trash: &auditorylab_app::services::library::LoadedRecord,
    record_path: &std::path::Path,
) {
    let path = record_path.parent().unwrap().parent().unwrap();
    let id = trash.snapshot.recipe().document().id;
    let history = trash.snapshot.recipe().document().clone();
    let added = trash.added_at;
    let restored = store
        .apply(
            Mutation::Restore {
                key: key.clone(),
                expect: Some(trash.version_token),
            },
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    assert_eq!(restored.trashed_at, None);
    assert_eq!(restored.snapshot.recipe().document(), &history);
    assert_eq!(restored.added_at, added);
    assert!(
        store
            .apply(
                Mutation::Purge {
                    key: key.clone(),
                    expect: Some(restored.version_token),
                    confirmed: true
                },
                &Cancel::default()
            )
            .is_err()
    );
    let trash = store
        .apply(
            Mutation::Trash {
                key: key.clone(),
                expect: Some(restored.version_token),
            },
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    assert!(
        store
            .apply(
                Mutation::Purge {
                    key: key.clone(),
                    expect: Some(trash.version_token),
                    confirmed: false
                },
                &Cancel::default()
            )
            .is_err()
    );
    assert!(path.join("trash").join(format!("{id}.toml")).exists());
    let purged = store
        .apply(
            Mutation::Purge {
                key: key.clone(),
                expect: Some(trash.version_token),
                confirmed: true,
            },
            &Cancel::default(),
        )
        .unwrap();
    assert!(matches!(
        purged.status,
        MutationStatus::Published | MutationStatus::PublishedWithWarning
    ));
    assert!(!record_path.exists());
    assert_eq!(
        store
            .apply(
                Mutation::Restore {
                    key: key.clone(),
                    expect: Some(trash.version_token)
                },
                &Cancel::default()
            )
            .unwrap_err()
            .outcome(),
        Outcome::Conflict
    );
}

#[test]
fn completion_output_failures_preserve_the_exact_confirmation_wording() {
    for (complete, expected) in [
        (
            true,
            "Operation confirmed; completion output failed: closed output",
        ),
        (
            false,
            "Operation not completed; completion output failed: closed output",
        ),
    ] {
        assert_eq!(
            auditorylab_presentation::library::report_completion_failure(complete, "closed output"),
            expected
        );
    }
}
