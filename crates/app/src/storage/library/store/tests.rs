use super::*;
use crate::{document::editing::Draft, storage::library::LibraryRoot};
use std::{fs, path::PathBuf};
fn fixture() -> (tempfile::TempDir, PathBuf, LibraryStore) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let path = temp.path().join("library");
    let store = LibraryStore::new(LibraryRoot::explicit(path.clone()).unwrap()).unwrap();
    store.root().initialize().unwrap();
    (temp, path, store)
}
#[test]
fn recipe_files_are_exact_portable_toml_and_tokens_are_exact_byte_hashes() {
    let (_temp, path, store) = fixture();
    let recipe = Draft::new().unwrap().recipe().unwrap();
    let id = recipe.document().id;
    let record = store
        .apply(Mutation::Add(Arc::new(recipe)), &Cancel::default())
        .unwrap()
        .record
        .unwrap();
    let file = path.join("recipes").join(format!("{id}.toml"));
    let bytes = fs::read(&file).unwrap();
    assert_eq!(bytes, record.recipe_toml.as_bytes());
    assert_eq!(
        record.version_token.to_string(),
        crate::storage::library::Fingerprint::of(&bytes).to_string()
    );
    let custom = format!("# manual formatting retained\\n{}", record.recipe_toml)
        .replace("retained\\n", "retained\n");
    fs::write(&file, custom.as_bytes()).unwrap();
    let current = store.load(&record.key).unwrap();
    assert_eq!(current.recipe_toml.as_ref(), custom);
    assert_ne!(current.version_token, record.version_token);
    let trashed = store
        .apply(
            Mutation::Trash {
                key: record.key.clone(),
                expect: None,
            },
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    assert!(trashed.trashed);
    assert_eq!(trashed.recipe_toml.as_ref(), custom);
    let restored = store
        .apply(
            Mutation::Restore {
                key: record.key,
                expect: None,
            },
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    assert!(!restored.trashed);
    assert_eq!(fs::read(file).unwrap(), custom.as_bytes());
}
#[test]
fn missing_membership_dates_never_change_directory_state_or_invent_history() {
    let (_temp, path, store) = fixture();
    let record = store
        .apply(
            Mutation::Add(Arc::new(Draft::new().unwrap().recipe().unwrap())),
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    let record = store
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
    fs::remove_file(path.join(MEMBERSHIP)).unwrap();
    let observed = store.load(&record.key).unwrap();
    assert!(observed.trashed);
    assert_eq!(observed.added_at, None);
    assert_eq!(observed.trashed_at, None);
    let scan = store.scan(&Cancel::default());
    assert!(scan.complete);
    assert_eq!(scan.entries.len(), 1);
    assert!(scan.entries[0].trashed);
    let mut query = crate::discovery::collection::CatalogQuery {
        scope: crate::discovery::collection::Scope::Trash,
        ..Default::default()
    };
    assert_eq!(query.execute(&scan.entries).unwrap().unique_count(), 1);
    query.scope = crate::discovery::collection::Scope::User;
    assert_eq!(query.execute(&scan.entries).unwrap().unique_count(), 0);
    let restored = store
        .apply(
            Mutation::Restore {
                key: record.key,
                expect: None,
            },
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    assert_eq!(restored.added_at, None);
    assert!(!restored.trashed);
}
#[test]
fn exact_a_to_b_to_a_bytes_accept_the_original_token_without_random_revisions() {
    let (_temp, path, store) = fixture();
    let record = store
        .apply(
            Mutation::Add(Arc::new(Draft::new().unwrap().recipe().unwrap())),
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    let id = record.snapshot.recipe().document().id;
    let file = path.join("recipes").join(format!("{id}.toml"));
    let original = fs::read(&file).unwrap();
    let mut other = original.clone();
    other.extend_from_slice(b"\n# changed\n");
    fs::write(&file, other).unwrap();
    assert_ne!(
        store.load(&record.key).unwrap().version_token,
        record.version_token
    );
    fs::write(file, original).unwrap();
    assert_eq!(
        store.load(&record.key).unwrap().version_token,
        record.version_token
    );
    store
        .apply(
            Mutation::Rename {
                key: record.key,
                expect: Some(record.version_token),
                title: "Authorized by identical bytes".into(),
            },
            &Cancel::default(),
        )
        .unwrap();
}
#[test]
fn orphan_timestamps_and_owned_stages_are_reported_then_cleaned_only_under_write_lock() {
    let (_temp, path, store) = fixture();
    let record = store
        .apply(
            Mutation::Add(Arc::new(Draft::new().unwrap().recipe().unwrap())),
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    let orphan = "00000000-0000-4000-8000-000000000099".parse().unwrap();
    let mut memberships =
        read_memberships(&Dir::open_ambient_dir(&path, cap_std::ambient_authority()).unwrap())
            .unwrap();
    memberships.entries.insert(
        orphan,
        Membership {
            added_at: Some("2026-01-01T00:00:00Z".parse().unwrap()),
            trashed_at: None,
        },
    );
    fs::write(
        path.join(MEMBERSHIP),
        toml::to_string(&memberships).unwrap(),
    )
    .unwrap();
    let owned = path.join("recipes/.auditorylab-00000000-0000-4000-8000-000000000099.partial");
    let foreign = path.join("recipes/unknown.partial");
    fs::write(&owned, b"partial").unwrap();
    fs::write(&foreign, b"foreign").unwrap();
    let scan = store.scan(&Cancel::default());
    assert!(scan.complete);
    assert!(
        scan.diagnostics
            .iter()
            .any(|d| d.code == "orphan_membership")
    );
    assert_eq!(scan.stages.len(), 1);
    assert!(owned.exists());
    store
        .apply(
            Mutation::Rename {
                key: record.key,
                expect: None,
                title: "Write cleans only owned state".into(),
            },
            &Cancel::default(),
        )
        .unwrap();
    assert!(!owned.exists());
    assert_eq!(fs::read(foreign).unwrap(), b"foreign");
    let memberships =
        read_memberships(&Dir::open_ambient_dir(path, cap_std::ambient_authority()).unwrap())
            .unwrap();
    assert!(!memberships.entries.contains_key(&orphan));
}
#[test]
fn thousand_plain_files_rebuild_without_retaining_full_recipe_snapshots() {
    let (_temp, path, store) = fixture();
    let recipe = Draft::new().unwrap().recipe().unwrap();
    for n in 0..1000 {
        let mut value = recipe.clone().into_builder();
        let id = format!("00000000-0000-4000-8000-{n:012}");
        value.document.id = id.parse().unwrap();
        value.content.description = Some(crate::core::Description {
            title: format!("Entry {n}"),
            notes: None,
        });
        fs::write(
            path.join("recipes").join(format!("{id}.toml")),
            value.build().unwrap().to_toml().unwrap(),
        )
        .unwrap();
    }
    let inventory = store.scan(&Cancel::default());
    assert!(inventory.complete);
    assert_eq!(inventory.entries.len(), 1000);
    assert!(
        inventory
            .entries
            .iter()
            .all(|entry| entry.added_at.is_none() && !entry.trashed)
    );
}
#[test]
fn bounded_scan_retains_healthy_rows_and_unknown_files_are_never_deleted() {
    let (_temp, path, store) = fixture();
    store
        .apply(
            Mutation::Add(Arc::new(Draft::new().unwrap().recipe().unwrap())),
            &Cancel::default(),
        )
        .unwrap();
    fs::write(path.join("recipes/user-notes.txt"), b"private notes").unwrap();
    let complete = store.scan(&Cancel::default());
    assert!(complete.complete);
    assert_eq!(complete.entries.len(), 1);
    assert!(
        complete
            .diagnostics
            .iter()
            .any(|d| d.code == "unrecognized_artifact")
    );
    let limited = store.scan_limited(
        &Cancel::default(),
        ScanLimits {
            entries: 0,
            raw: 1,
            decoded: 1,
        },
    );
    assert!(!limited.complete);
    assert_eq!(
        fs::read(path.join("recipes/user-notes.txt")).unwrap(),
        b"private notes"
    );
}

#[test]
fn private_stage_entries_are_bounded_by_the_same_inventory_budget() {
    let (_temp, path, store) = fixture();
    for name in ["first", "second"] {
        fs::write(path.join(".stage").join(name), b"unknown retained").unwrap();
    }
    let result = store.scan_limited(
        &Cancel::default(),
        ScanLimits {
            entries: 1,
            raw: 1024,
            decoded: 1024,
        },
    );
    assert!(!result.complete);
    assert_eq!(result.stages.len(), 1);
    assert!(path.join(".stage/first").exists());
    assert!(path.join(".stage/second").exists());
}
