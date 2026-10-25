//! Native filesystem lifecycle and strict managed-record behavior through shared APIs.
use auditorylab::{
    Cancel, Draft, ErrorKind, Recipe,
    library::{LibraryKey, LibraryRoot, LibraryStore, Mutation, MutationStatus, VersionToken},
};
use std::{fs, path::PathBuf, sync::Arc};
#[expect(
    clippy::unwrap_used,
    reason = "A fixture setup failure must fail its calling native store test"
)]
fn fixture() -> (tempfile::TempDir, PathBuf, LibraryStore) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let path = temp.path().join("library");
    let store = LibraryStore::new(LibraryRoot::explicit(path.clone()).unwrap()).unwrap();
    (temp, path, store)
}
#[expect(
    clippy::unwrap_used,
    reason = "A finite authored test recipe with a fresh provisional UUID must construct successfully"
)]
fn recipe(title: &str) -> Recipe {
    let mut draft = Draft::new().unwrap();
    draft.set_title(title.into());
    draft.recipe()
}
#[test]
fn readonly_discovery_and_builtin_refusals_never_initialize_a_root() {
    let (_temp, path, store) = fixture();
    let info = store.root().inspect().unwrap();
    assert!(!info.exists && !info.initialized);
    assert!(!path.exists());
    let scan = store.scan(&Cancel::default());
    assert!(!scan.complete && scan.not_initialized);
    assert!(!path.exists());
    let token: VersionToken = format!("00000000-0000-4000-8000-000000000001:{}", "0".repeat(64))
        .parse()
        .unwrap();
    let error = store
        .apply(
            Mutation::Rename {
                key: LibraryKey::BuiltIn("ioannou-2015-binaural-2-hz".into()),
                expect: token,
                title: "cannot change shipped".into(),
            },
            &Cancel::default(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ReadOnlyBuiltIn);
    assert!(!path.exists());
    let builtin = auditorylab::builtin("ioannou-2015-binaural-2-hz").unwrap();
    let error = store
        .apply(
            Mutation::Add(Arc::new(builtin.recipe().clone())),
            &Cancel::default(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Conflict);
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
    store.root().initialize().unwrap();
    assert!(!store.root().initialize().unwrap().created);
    let original = recipe("Imported unknown history");
    let id = original.document.id;
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
    assert_eq!(loaded.snapshot.recipe().document.created_at, None);
    assert_eq!(loaded.snapshot.recipe().document.modified_at, None);
    let added = loaded.added_at;
    let initial = loaded.version_token.clone();
    let record_path = path.join("records").join(format!("{id}.json"));
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
    let mut collision = original.clone();
    collision.description.as_mut().unwrap().title = "same UUID different title".into();
    assert_eq!(
        store
            .apply(Mutation::Add(Arc::new(collision)), &Cancel::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Conflict
    );
    let renamed = store
        .apply(
            Mutation::Rename {
                key: key.clone(),
                expect: initial.clone(),
                title: "Renamed".into(),
            },
            &Cancel::default(),
        )
        .unwrap();
    assert!(renamed.confirmed());
    let changed = store.load(&key).unwrap();
    assert_eq!(changed.snapshot.recipe().document.id, id);
    assert_eq!(changed.snapshot.recipe().document.created_at, None);
    assert!(changed.snapshot.recipe().document.modified_at.is_some());
    assert_eq!(changed.added_at, added);
    assert_ne!(changed.version_token, initial);
    let changed_bytes = fs::read(&record_path).unwrap();
    let error = store
        .apply(
            Mutation::Update {
                key: key.clone(),
                expect: initial,
                recipe: Arc::new(original),
            },
            &Cancel::default(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Conflict);
    assert_eq!(fs::read(&record_path).unwrap(), changed_bytes);
    let mut wrong_history = changed.snapshot.recipe().clone();
    wrong_history.document.created_at = Some("2020-01-01T00:00:00Z".parse().unwrap());
    assert_eq!(
        store
            .apply(
                Mutation::Update {
                    key: key.clone(),
                    expect: changed.version_token.clone(),
                    recipe: Arc::new(wrong_history)
                },
                &Cancel::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::HistoryConflict
    );
    let history = changed.snapshot.recipe().document.clone();
    let trashed = store
        .apply(
            Mutation::Trash {
                key: key.clone(),
                expect: changed.version_token,
            },
            &Cancel::default(),
        )
        .unwrap();
    let trash = trashed.receipt.record.unwrap();
    assert!(trash.trashed_at.is_some());
    assert_eq!(trash.snapshot.recipe().document, history);
    assert_eq!(trash.added_at, added);
    assert_eq!(
        store
            .apply(
                Mutation::Add(Arc::new(trash.snapshot.recipe().clone())),
                &Cancel::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Conflict
    );
    let restored = store
        .apply(
            Mutation::Restore {
                key: key.clone(),
                expect: trash.version_token,
            },
            &Cancel::default(),
        )
        .unwrap()
        .receipt
        .record
        .unwrap();
    assert_eq!(restored.trashed_at, None);
    assert_eq!(restored.snapshot.recipe().document, history);
    assert_eq!(restored.added_at, added);
    assert!(
        store
            .apply(
                Mutation::Purge {
                    key: key.clone(),
                    expect: restored.version_token.clone(),
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
                expect: restored.version_token,
            },
            &Cancel::default(),
        )
        .unwrap()
        .receipt
        .record
        .unwrap();
    assert!(
        store
            .apply(
                Mutation::Purge {
                    key: key.clone(),
                    expect: trash.version_token.clone(),
                    confirmed: false
                },
                &Cancel::default()
            )
            .is_err()
    );
    assert!(record_path.exists());
    let purged = store
        .apply(
            Mutation::Purge {
                key: key.clone(),
                expect: trash.version_token.clone(),
                confirmed: true,
            },
            &Cancel::default(),
        )
        .unwrap();
    assert!(purged.confirmed());
    assert!(!record_path.exists());
    assert_eq!(
        store
            .apply(
                Mutation::Restore {
                    key,
                    expect: trash.version_token
                },
                &Cancel::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Conflict
    );
    assert_eq!(
        fs::read_to_string(external).unwrap(),
        text,
        "managed lifecycle never modifies/deletes original external content"
    );
}
#[test]
fn new_creation_dates_are_assigned_only_to_the_confirmed_snapshot_and_noops_keep_bytes() {
    let (_temp, path, store) = fixture();
    store.root().initialize().unwrap();
    let provisional = recipe("New sound");
    let id = provisional.document.id;
    assert_eq!(provisional.document.created_at, None);
    let result = store
        .apply(
            Mutation::Create(Arc::new(provisional.clone())),
            &Cancel::default(),
        )
        .unwrap();
    let record = result.receipt.record.unwrap();
    let identity = &record.snapshot.recipe().document;
    assert_eq!(identity.id, id);
    assert_eq!(identity.created_at, Some(record.added_at));
    assert_eq!(identity.modified_at, identity.created_at);
    assert_eq!(
        provisional.document.created_at, None,
        "captured input is immutable"
    );
    let file = path.join("records").join(format!("{id}.json"));
    let bytes = fs::read(&file).unwrap();
    let same = store
        .apply(
            Mutation::Update {
                key: record.key.clone(),
                expect: record.version_token.clone(),
                recipe: Arc::new(record.snapshot.recipe().clone()),
            },
            &Cancel::default(),
        )
        .unwrap();
    assert_eq!(same.status, MutationStatus::Unchanged);
    assert_eq!(
        same.receipt.record.unwrap().version_token,
        record.version_token
    );
    assert_eq!(fs::read(file).unwrap(), bytes);
}
#[test]
fn digest_detects_changed_bytes_even_when_the_declared_revision_is_unchanged() {
    let (_temp, path, store) = fixture();
    store.root().initialize().unwrap();
    let result = store
        .apply(Mutation::Add(Arc::new(recipe("bytes"))), &Cancel::default())
        .unwrap();
    let record = result.receipt.record.unwrap();
    let path = path
        .join("records")
        .join(format!("{}.json", record.snapshot.recipe().document.id));
    let mut bytes = fs::read(&path).unwrap();
    bytes.push(b' ');
    fs::write(&path, &bytes).unwrap();
    let fresh = store.load(&record.key).unwrap();
    assert_eq!(
        fresh.version_token.revision(),
        record.version_token.revision()
    );
    assert_ne!(fresh.version_token, record.version_token);
    assert_eq!(
        store
            .apply(
                Mutation::Update {
                    key: record.key,
                    expect: record.version_token,
                    recipe: Arc::new(record.snapshot.recipe().clone())
                },
                &Cancel::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Conflict
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}
#[test]
fn strict_envelopes_corrupt_rows_and_owned_stages_are_reported_without_repair() {
    let (_temp, path, store) = fixture();
    store.root().initialize().unwrap();
    let result = store
        .apply(
            Mutation::Add(Arc::new(recipe("healthy"))),
            &Cancel::default(),
        )
        .unwrap();
    let record = result.receipt.record.unwrap();
    let id = record.snapshot.recipe().document.id;
    let file = path.join("records").join(format!("{id}.json"));
    let bytes = fs::read(&file).unwrap();
    let stage = path.join("records").join(format!(
        ".{id}.00000000-0000-4000-8000-000000000099.partial"
    ));
    fs::write(&stage, b"orphan candidate").unwrap();
    let scan = store.scan(&Cancel::default());
    assert!(scan.complete);
    assert_eq!(scan.entries.len(), 1);
    assert_eq!(scan.stages.len(), 1);
    assert!(stage.exists());
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for corrupted in [
        {
            let mut v = value.clone();
            v.as_object_mut().unwrap().remove("trashed_at");
            serde_json::to_vec(&v).unwrap()
        },
        {
            let mut v = value.clone();
            v["record_format"] = serde_json::json!(2);
            serde_json::to_vec(&v).unwrap()
        },
        {
            let mut v = value.clone();
            v["origin"] = serde_json::json!("builtin");
            serde_json::to_vec(&v).unwrap()
        },
        bytes
            .iter()
            .copied()
            .chain(b", malformed".iter().copied())
            .collect(),
    ] {
        fs::write(&file, &corrupted).unwrap();
        assert!(store.load(&record.key).is_err());
        let scan = store.scan(&Cancel::default());
        assert!(!scan.complete);
        assert!(scan.entries.is_empty());
        assert!(!scan.diagnostics.is_empty());
        assert_eq!(fs::read(&file).unwrap(), corrupted);
    }
}
#[cfg(unix)]
#[test]
fn managed_descendant_symlinks_and_nonregular_entries_never_read_external_content() {
    use std::os::unix::fs::symlink;
    let (temp, path, store) = fixture();
    store.root().initialize().unwrap();
    let external = temp.path().join("outside");
    fs::write(&external, b"secret outside bytes").unwrap();
    let bad = path.join("records/00000000-0000-4000-8000-000000000001.json");
    symlink(&external, &bad).unwrap();
    let scan = store.scan(&Cancel::default());
    assert!(!scan.complete);
    assert!(scan.entries.is_empty());
    assert!(!scan.diagnostics[0].message.contains("secret outside bytes"));
    assert_eq!(fs::read(&external).unwrap(), b"secret outside bytes");
    fs::remove_file(bad).unwrap();
    fs::remove_dir(path.join("records")).unwrap();
    symlink(temp.path(), path.join("records")).unwrap();
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
fn lock_contention_is_nonblocking_and_closure_releases_the_same_persistent_lock() {
    let (_temp, path, store) = fixture();
    store.root().initialize().unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path.join(".library.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let start = std::time::Instant::now();
    let inventory = store.scan(&Cancel::default());
    assert!(!inventory.complete);
    assert_eq!(inventory.failure_kind, Some(ErrorKind::Busy));
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
    drop(lock);
    assert!(store.scan(&Cancel::default()).complete);
    assert!(path.join(".library.lock").is_file());
}
#[test]
fn initialization_refuses_unknown_data_and_records_without_a_marker() {
    let (_temp, path, store) = fixture();
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("unrelated.txt"), b"keep").unwrap();
    assert!(store.root().initialize().is_err());
    assert!(!path.join("library.json").exists());
    assert_eq!(fs::read(path.join("unrelated.txt")).unwrap(), b"keep");
    fs::remove_file(path.join("unrelated.txt")).unwrap();
    fs::create_dir(path.join("records")).unwrap();
    fs::write(path.join("records/arbitrary.json"), b"keep record").unwrap();
    assert!(store.root().initialize().is_err());
    assert_eq!(
        fs::read(path.join("records/arbitrary.json")).unwrap(),
        b"keep record"
    );
    assert!(!path.join("library.json").exists());
}
