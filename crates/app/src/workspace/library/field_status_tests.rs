//! Actual storage outcomes keep their captured action through coordinator completion.
use super::*;

#[test]
fn library_completion_retains_all_mutation_kinds_and_purge_identity() {
    let (_temp, store, mut workspace) = fixture();
    let mut draft = Draft::new().unwrap();
    draft.set_title("Captured action".into());
    let created = store
        .apply(
            Mutation::Create(
                crate::services::library::Creation::prepare(Arc::new(draft.recipe().unwrap()))
                    .unwrap(),
            ),
            &Cancel::default(),
        )
        .unwrap();
    let key = created.key.clone();
    let check = |workspace: &mut Workspace, outcome: WriteOutcome, expected: MutationKind| {
        let expected_key = outcome.key.clone();
        let (status, _) = workspace.accept_managed_outcome(None, outcome);
        let FileStatus::Library(LibraryStatus::Written {
            kind, key, title, ..
        }) = status
        else {
            panic!("confirmed status")
        };
        assert_eq!(kind, expected);
        assert_eq!(key, expected_key);
        assert_eq!(title.is_none(), expected == MutationKind::Purge);
    };
    check(&mut workspace, created, MutationKind::Create);
    let mut imported = Draft::new().unwrap();
    imported.set_title("Imported action".into());
    let added = store
        .apply(
            Mutation::Add(Arc::new(imported.recipe().unwrap())),
            &Cancel::default(),
        )
        .unwrap();
    check(&mut workspace, added, MutationKind::Add);
    let record = store.load(&key).unwrap();
    let mut changed = record.snapshot.recipe().clone().into_builder();
    changed.content.description.as_mut().unwrap().notes = Some("Updated note".into());
    let updated = store
        .apply(
            Mutation::Update {
                key: key.clone(),
                expect: Some(record.version_token),
                recipe: Arc::new(changed.build().unwrap()),
            },
            &Cancel::default(),
        )
        .unwrap();
    check(&mut workspace, updated, MutationKind::Update);
    let renamed = store
        .apply(
            Mutation::Rename {
                key: key.clone(),
                expect: Some(store.load(&key).unwrap().version_token),
                title: "Renamed action".into(),
            },
            &Cancel::default(),
        )
        .unwrap();
    check(&mut workspace, renamed, MutationKind::Rename);
    for expected in [
        MutationKind::Trash,
        MutationKind::Restore,
        MutationKind::Trash,
        MutationKind::Purge,
    ] {
        let token = Some(store.load(&key).unwrap().version_token);
        let mutation = match expected {
            MutationKind::Trash => Mutation::Trash {
                key: key.clone(),
                expect: token,
            },
            MutationKind::Restore => Mutation::Restore {
                key: key.clone(),
                expect: token,
            },
            MutationKind::Purge => Mutation::Purge {
                key: key.clone(),
                expect: token,
                confirmed: true,
            },
            _ => unreachable!(),
        };
        let outcome = store.apply(mutation, &Cancel::default()).unwrap();
        check(&mut workspace, outcome, expected);
    }
    assert!(store.load(&key).is_err());
}
