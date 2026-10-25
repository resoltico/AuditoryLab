//! Controlled errors around real native FS publication, not a reference-store model.
use auditorylab::editing::Draft;
use auditorylab::error::ErrorKind;
use auditorylab::library::{
    LibraryRoot, LibraryStore, Mutation, MutationStatus, RecheckOutcome, StoreFault,
};
use auditorylab::workflow::Cancel;
use std::{fs, path::PathBuf, sync::Arc};
#[expect(
    clippy::unwrap_used,
    reason = "Native fixture setup failure must fail its calling fault test"
)]
fn fixture() -> (
    tempfile::TempDir,
    PathBuf,
    LibraryStore,
    auditorylab::library::LoadedRecord,
) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let path = temp.path().join("library");
    let store = LibraryStore::new(LibraryRoot::explicit(path.clone()).unwrap()).unwrap();
    store.root().initialize().unwrap();
    let mut draft = Draft::new().unwrap();
    draft.set_title("Original".into());
    let record = store
        .apply(
            Mutation::Create(Arc::new(draft.recipe())),
            &Cancel::default(),
        )
        .unwrap()
        .receipt
        .record
        .unwrap();
    (temp, path, store, record)
}
#[expect(
    clippy::unwrap_used,
    reason = "Observed token and full recipe belong to a validated native fixture"
)]
fn update(record: &auditorylab::library::LoadedRecord, title: &str) -> Mutation {
    let mut recipe = record.snapshot.recipe().clone();
    recipe.description.as_mut().unwrap().title = title.into();
    Mutation::Update {
        key: record.key.clone(),
        expect: record.version_token.clone(),
        recipe: Arc::new(recipe),
    }
}
#[test]
fn precommit_native_faults_and_cancel_preserve_old_bytes_and_remove_only_their_stage() {
    for fault in [
        StoreFault::Write,
        StoreFault::Flush,
        StoreFault::FileSync,
        StoreFault::CancelBeforePublication,
        StoreFault::RandomUnavailable,
    ] {
        let (_temp, path, store, record) = fixture();
        let file = path
            .join("records")
            .join(format!("{}.json", record.snapshot.recipe().document.id));
        let before = fs::read(&file).unwrap();
        let unrelated = path.join("records/.unrelated.partial");
        fs::write(&unrelated, b"keep this stage").unwrap();
        let error = store
            .clone()
            .with_fault(Some(fault))
            .apply(update(&record, "Not published"), &Cancel::default())
            .unwrap_err();
        assert!(matches!(
            error.kind(),
            ErrorKind::Write | ErrorKind::Cancelled | ErrorKind::Identity
        ));
        assert_eq!(fs::read(&file).unwrap(), before, "{fault:?}");
        assert_eq!(fs::read(unrelated).unwrap(), b"keep this stage");
        assert_eq!(fs::read_dir(path.join("records")).unwrap().count(), 2);
    }
}
#[test]
fn postcommit_directory_and_stage_cleanup_failures_remain_saved_with_exact_receipts() {
    for fault in [
        StoreFault::DirectorySync,
        StoreFault::StageCleanup,
        StoreFault::PublicationCommittedError,
    ] {
        let (_temp, path, store, record) = fixture();
        let outcome = if fault == StoreFault::StageCleanup {
            let mut draft = Draft::new().unwrap();
            draft.set_title("New with retained hard link".into());
            store
                .clone()
                .with_fault(Some(fault))
                .apply(
                    Mutation::Create(Arc::new(draft.recipe())),
                    &Cancel::default(),
                )
                .unwrap()
        } else {
            store
                .clone()
                .with_fault(Some(fault))
                .apply(update(&record, "Actually published"), &Cancel::default())
                .unwrap()
        };
        assert_eq!(outcome.status, MutationStatus::PublishedWithWarning);
        assert!(outcome.confirmed());
        assert!(!outcome.diagnostics.is_empty());
        let candidate = outcome.receipt.record.as_ref().unwrap();
        let actual = store.load(&outcome.receipt.key).unwrap();
        assert_eq!(actual.version_token, candidate.version_token);
        assert_eq!(actual.snapshot.recipe(), candidate.snapshot.recipe());
        if fault == StoreFault::StageCleanup {
            let stage = outcome.receipt.owned_stage.as_ref().unwrap();
            assert!(path.join("records").join(stage).exists());
            let scan = store.scan(&Cancel::default());
            assert!(scan.complete);
            assert_eq!(scan.entries.len(), 2);
            assert!(scan.stages.contains(stage));
        }
    }
}
#[test]
fn ambiguous_actual_committed_and_uncommitted_states_keep_receipts_and_require_recheck() {
    for (fault, committed) in [
        (StoreFault::PublicationCommittedUnreadable, true),
        (StoreFault::PublicationUncommittedUnreadable, false),
    ] {
        let (_temp, path, store, record) = fixture();
        let outcome = store
            .clone()
            .with_fault(Some(fault))
            .apply(update(&record, "Uncertain candidate"), &Cancel::default())
            .unwrap();
        assert_eq!(outcome.status, MutationStatus::PublicationUnconfirmed);
        assert!(!outcome.confirmed());
        let actual = store.load(&record.key).unwrap();
        assert_eq!(actual.snapshot.title() == "Uncertain candidate", committed);
        let retained = outcome.receipt.record.as_ref().unwrap();
        assert_eq!(retained.snapshot.title(), "Uncertain candidate");
        if !committed {
            assert!(
                path.join("records")
                    .join(outcome.receipt.owned_stage.as_ref().unwrap())
                    .exists()
            );
            assert_eq!(actual.version_token, record.version_token);
        }
        match store.recheck(&outcome.receipt).unwrap() {
            RecheckOutcome::Published(known) => {
                assert!(committed);
                assert!(known.confirmed());
                assert_eq!(
                    known.receipt.record.unwrap().version_token,
                    actual.version_token
                );
            }
            RecheckOutcome::Original(old) => {
                assert!(!committed);
                assert_eq!(old.version_token, record.version_token);
            }
            other => panic!("unexpected recheck: {other:?}"),
        }
    }
}
#[test]
fn ambiguous_error_with_immediately_readable_original_is_a_known_prepublication_failure() {
    let (_temp, _path, store, record) = fixture();
    assert!(
        store
            .clone()
            .with_fault(Some(StoreFault::PublicationUncommittedError))
            .apply(update(&record, "Not committed"), &Cancel::default())
            .is_err()
    );
    assert_eq!(
        store.load(&record.key).unwrap().version_token,
        record.version_token
    );
}
#[test]
fn no_clobber_race_retains_actual_foreign_destination_bytes() {
    let (_temp, path, store, _record) = fixture();
    let draft = Draft::new().unwrap();
    let id = draft.recipe().document.id;
    let error = store
        .with_fault(Some(StoreFault::CollisionAfterStage))
        .apply(
            Mutation::Create(Arc::new(draft.recipe())),
            &Cancel::default(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Conflict);
    assert_eq!(
        fs::read(path.join("records").join(format!("{id}.json"))).unwrap(),
        b"foreign bytes must remain intact"
    );
}
#[test]
fn purge_recheck_observes_absence_without_inventing_attributable_success_after_restart() {
    for (fault, removed) in [
        (StoreFault::PurgeCommittedUnreadable, true),
        (StoreFault::PurgeUncommittedUnreadable, false),
    ] {
        let (_temp, _path, store, record) = fixture();
        let trash = store
            .apply(
                Mutation::Trash {
                    key: record.key,
                    expect: record.version_token,
                },
                &Cancel::default(),
            )
            .unwrap()
            .receipt
            .record
            .unwrap();
        let outcome = store
            .clone()
            .with_fault(Some(fault))
            .apply(
                Mutation::Purge {
                    key: trash.key.clone(),
                    expect: trash.version_token.clone(),
                    confirmed: true,
                },
                &Cancel::default(),
            )
            .unwrap();
        assert_eq!(outcome.status, MutationStatus::PublicationUnconfirmed);
        match store.recheck(&outcome.receipt).unwrap() {
            RecheckOutcome::Absent => assert!(removed),
            RecheckOutcome::Original(still_there) => {
                assert!(!removed);
                assert_eq!(still_there.version_token, trash.version_token);
            }
            other => panic!("unexpected purge observation: {other:?}"),
        }
        if removed {
            let restarted = LibraryStore::new(store.root().clone()).unwrap();
            assert_eq!(
                restarted
                    .apply(
                        Mutation::Purge {
                            key: trash.key,
                            expect: trash.version_token,
                            confirmed: true
                        },
                        &Cancel::default()
                    )
                    .unwrap_err()
                    .kind(),
                ErrorKind::Conflict
            );
        }
    }
}
#[test]
fn regressed_clock_is_preserved_and_reported_but_cannot_authorize_stale_writes() {
    let (_temp, _path, store, _record) = fixture();
    let mut recipe = Draft::new().unwrap().recipe();
    recipe.document.created_at = Some("2026-10-24T10:00:00Z".parse().unwrap());
    recipe.document.modified_at = Some("2026-10-24T11:00:00Z".parse().unwrap());
    let first = store
        .clone()
        .with_fault(Some(StoreFault::Clock(
            "2026-10-24T12:00:00Z".parse().unwrap(),
        )))
        .apply(Mutation::Add(Arc::new(recipe)), &Cancel::default())
        .unwrap()
        .receipt
        .record
        .unwrap();
    let result = store
        .clone()
        .with_fault(Some(StoreFault::Clock(
            "2026-10-24T09:00:00Z".parse().unwrap(),
        )))
        .apply(update(&first, "Clock went back"), &Cancel::default())
        .unwrap();
    assert_eq!(result.status, MutationStatus::PublishedWithWarning);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.contains("ClockRegression"))
    );
    let saved = result.receipt.record.unwrap();
    assert_eq!(
        saved.snapshot.recipe().document.created_at,
        first.snapshot.recipe().document.created_at
    );
    assert_eq!(
        saved.snapshot.recipe().document.modified_at,
        Some("2026-10-24T09:00:00Z".parse().unwrap())
    );
    assert_eq!(saved.added_at, first.added_at);
    assert_ne!(saved.version_token, first.version_token);
    assert_eq!(
        store
            .apply(update(&first, "stale timestamp intent"), &Cancel::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Conflict
    );
    let noop = store
        .with_fault(Some(StoreFault::RandomUnavailable))
        .apply(
            Mutation::Update {
                key: saved.key,
                expect: saved.version_token,
                recipe: Arc::new(saved.snapshot.recipe().clone()),
            },
            &Cancel::default(),
        )
        .unwrap();
    assert_eq!(
        noop.status,
        MutationStatus::Unchanged,
        "a no-op needs no new nonce/time"
    );
}
#[test]
fn thousand_record_inventory_rebuilds_nominal_summaries_and_history_without_cached_recipes() {
    let (_temp, _path, store, _record) = fixture();
    let mut ids = std::collections::BTreeSet::new();
    for n in 0..999 {
        let mut draft = Draft::new().unwrap();
        draft.set_title(format!("Fixture {n}"));
        let record = store
            .apply(Mutation::Add(Arc::new(draft.recipe())), &Cancel::default())
            .unwrap()
            .receipt
            .record
            .unwrap();
        assert!(ids.insert(record.snapshot.recipe().document.id));
    }
    let scan = store.scan(&Cancel::default());
    assert!(scan.complete);
    assert_eq!(scan.entries.len(), 1000);
    assert!(scan.entries.iter().all(|e| e.nominal.no_rhythm()));
    assert!(scan.entries.iter().all(|e| e.added_at.is_some()));
    let rebuilt = LibraryStore::new(store.root().clone())
        .unwrap()
        .scan(&Cancel::default());
    assert!(rebuilt.complete);
    assert_eq!(rebuilt.entries.len(), 1000);
    for entry in &scan.entries {
        let again = rebuilt.entries.iter().find(|e| e.key == entry.key).unwrap();
        assert_eq!(entry.version_token, again.version_token);
        assert_eq!(entry.added_at, again.added_at);
        assert_eq!(entry.created_at, again.created_at);
    }
    let cancel = Cancel::default();
    cancel.request();
    let stopped = store.scan(&cancel);
    assert!(!stopped.complete);
    assert_eq!(stopped.failure_kind, Some(ErrorKind::Cancelled));
}
