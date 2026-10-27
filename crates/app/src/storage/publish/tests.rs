use super::*;
use crate::document::origin::LibraryKey;
use std::{
    fs,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
fn fixture() -> (tempfile::TempDir, Dir, PathBuf) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let path = temp.path().join("entry");
    let dir = Dir::open_ambient_dir(temp.path(), cap_std::ambient_authority()).unwrap();
    (temp, dir, path)
}
fn put(dir: &Dir, path: &Path, policy: Policy<'_>, bytes: &[u8]) -> Result<Published<()>, Error> {
    publish_in(
        dir,
        Path::new("entry"),
        path,
        policy,
        &Cancel::default(),
        |file| {
            file.write_all(bytes)
                .map_err(|e| io(path, IoOperation::Write, e))
        },
    )
}
fn inject(points: &[FaultPoint]) {
    INJECT.with(|slot| *slot.borrow_mut() = points.to_vec());
}
#[test]
fn policies_have_accepted_and_rejected_neighbors() {
    let (_temp, dir, path) = fixture();
    put(&dir, &path, Policy::CreateNew, b"first").unwrap();
    assert_eq!(
        put(&dir, &path, Policy::CreateNew, b"bad")
            .unwrap_err()
            .outcome(),
        crate::outcome::Outcome::DestinationExists
    );
    assert_eq!(fs::read(&path).unwrap(), b"first");
    put(&dir, &path, Policy::Replace, b"second").unwrap();
    let observed = same_file::Handle::from_path(&path).unwrap();
    let hash = Fingerprint::of(b"second");
    assert_eq!(
        put(
            &dir,
            &path,
            Policy::ReplaceIfUnchanged {
                fingerprint: Fingerprint::of(b"wrong"),
                identity: &observed
            },
            b"bad"
        )
        .unwrap_err()
        .outcome(),
        crate::outcome::Outcome::Conflict
    );
    put(
        &dir,
        &path,
        Policy::ReplaceIfUnchanged {
            fingerprint: hash,
            identity: &observed,
        },
        b"third",
    )
    .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"third");
    let current = same_file::Handle::from_path(&path).unwrap();
    let other = path.with_extension("replacement");
    fs::write(&other, b"third").unwrap();
    fs::rename(other, &path).unwrap();
    assert_eq!(
        put(
            &dir,
            &path,
            Policy::ReplaceIfUnchanged {
                fingerprint: Fingerprint::of(b"third"),
                identity: &current
            },
            b"bad"
        )
        .unwrap_err()
        .outcome(),
        crate::outcome::Outcome::Conflict
    );
    assert_eq!(fs::read(&path).unwrap(), b"third");
    fs::remove_file(&path).unwrap();
    put(&dir, &path, Policy::Replace, b"new").unwrap();
    assert_eq!(fs::read(path).unwrap(), b"new");
}
#[test]
fn precommit_failures_preserve_previous_bytes_and_cleanup_only_our_stage() {
    for point in [
        FaultPoint::StageCreate,
        FaultPoint::Write,
        FaultPoint::FileSync,
        FaultPoint::BeforePublish,
    ] {
        let (temp, dir, path) = fixture();
        fs::write(&path, b"original").unwrap();
        let foreign = temp.path().join("foreign.partial");
        fs::write(&foreign, b"keep").unwrap();
        inject(&[point]);
        assert!(put(&dir, &path, Policy::Replace, b"candidate").is_err());
        assert_eq!(fs::read(path).unwrap(), b"original");
        assert_eq!(fs::read(foreign).unwrap(), b"keep");
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    }
    let (temp, dir, path) = fixture();
    fs::write(&path, b"old").unwrap();
    let result: Result<Published<()>, Error> = publish_in(
        &dir,
        Path::new("entry"),
        &path,
        Policy::Replace,
        &Cancel::default(),
        |file| {
            file.write_all(b"partial").unwrap();
            Err(io(
                &path,
                IoOperation::Write,
                std::io::Error::other("writer failed after partial output"),
            ))
        },
    );
    assert!(result.is_err());
    assert_eq!(fs::read(path).unwrap(), b"old");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}
#[test]
fn postcommit_failures_verify_candidate_previous_and_foreign_content() {
    for point in [FaultPoint::AfterPublish, FaultPoint::DirectorySync] {
        let (_temp, dir, path) = fixture();
        fs::write(&path, b"previous").unwrap();
        inject(&[point]);
        let result = put(&dir, &path, Policy::Replace, b"candidate").unwrap();
        assert!(!result.warnings.is_empty());
        assert_eq!(fs::read(path).unwrap(), b"candidate");
    }
    let (_temp, dir, path) = fixture();
    fs::write(&path, b"previous").unwrap();
    inject(&[FaultPoint::BeforePublish]);
    let error = put(&dir, &path, Policy::Replace, b"candidate").unwrap_err();
    assert!(matches!(
        &error,
        Error::AfterMutation {
            confirmation: Confirmation::Unchanged,
            ..
        }
    ));
    assert_eq!(fs::read(&path).unwrap(), b"previous");
    let changed = path.clone();
    AFTER_PUBLISH.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(move || fs::write(changed, b"foreign").unwrap()))
    });
    inject(&[FaultPoint::AfterPublish]);
    let error = put(&dir, &path, Policy::Replace, b"candidate").unwrap_err();
    assert!(
        matches!(&error,Error::AfterMutation {path:observed,confirmation:Confirmation::Different,..} if observed==&path)
    );
    assert_eq!(fs::read(path).unwrap(), b"foreign");
}
#[test]
fn no_clobber_race_and_cleanup_failure_preserve_publication_truth() {
    let (_temp, dir, path) = fixture();
    let error = publish_in(
        &dir,
        Path::new("entry"),
        &path,
        Policy::CreateNew,
        &Cancel::default(),
        |file| {
            file.write_all(b"ours").unwrap();
            fs::write(&path, b"foreign").unwrap();
            Ok(())
        },
    )
    .unwrap_err();
    assert_eq!(error.outcome(), crate::outcome::Outcome::DestinationExists);
    assert_eq!(fs::read(path).unwrap(), b"foreign");
    let (temp, dir, path) = fixture();
    inject(&[FaultPoint::Cleanup]);
    let result = put(&dir, &path, Policy::CreateNew, b"saved").unwrap();
    assert!(!result.warnings.is_empty());
    assert_eq!(fs::read(path).unwrap(), b"saved");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
}
#[cfg(unix)]
#[test]
fn symlinks_and_fifos_are_rejected_without_following_or_blocking() {
    use std::os::unix::fs::symlink;
    let (temp, dir, path) = fixture();
    let outside = temp.path().join("outside");
    fs::write(&outside, b"secret").unwrap();
    symlink(&outside, &path).unwrap();
    assert!(open_regular(&dir, Path::new("entry")).is_err());
    assert_eq!(fs::read(outside).unwrap(), b"secret");
    fs::remove_file(&path).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let start = Instant::now();
    assert!(open_regular(&dir, Path::new("entry")).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
}
#[test]
fn purge_failures_distinguish_existing_destination_and_confirmed_absence() {
    let (_temp, dir, path) = fixture();
    fs::write(&path, b"old").unwrap();
    inject(&[FaultPoint::BeforeRemove]);
    assert!(remove(&dir, Path::new("entry"), &path).is_err());
    assert!(path.exists());
    inject(&[FaultPoint::AfterRemove]);
    assert!(!remove(&dir, Path::new("entry"), &path).unwrap().is_empty());
    assert!(!path.exists());
}
#[test]
#[ignore = "subprocess entry point; exercised by ordinary killed-publication tests"]
fn process_child() {
    use crate::{
        document::editing::Draft,
        storage::library::{LibraryRoot, LibraryStore, Mutation},
    };
    let root = PathBuf::from(std::env::var_os("AUDITORYLAB_PUBLISH_ROOT").unwrap());
    let mode = std::env::var("AUDITORYLAB_PUBLISH_MODE").unwrap();
    let library = LibraryRoot::explicit(root).unwrap();
    if mode == "initialize" {
        library.initialize().unwrap();
        return;
    }
    let store = LibraryStore::new(library).unwrap();
    let id = std::env::var("AUDITORYLAB_PUBLISH_ID")
        .unwrap()
        .parse()
        .unwrap();
    if mode == "create" {
        let mut recipe = Draft::new().unwrap().recipe().unwrap().into_builder();
        recipe.document.id = id;
        store
            .apply(
                Mutation::Create(Arc::new(recipe.build().unwrap())),
                &Cancel::default(),
            )
            .unwrap();
        return;
    }
    let record = store.load(&LibraryKey::User(id)).unwrap();
    let mutation = match mode.as_str() {
        "trash" => Mutation::Trash {
            key: record.key,
            expect: Some(record.version_token),
        },
        "restore" => Mutation::Restore {
            key: record.key,
            expect: Some(record.version_token),
        },
        "purge" => Mutation::Purge {
            key: record.key,
            expect: Some(record.version_token),
            confirmed: true,
        },
        _ => {
            let mut recipe = record.snapshot.recipe().clone().into_builder();
            recipe.content.description.as_mut().unwrap().title = "Complete candidate".into();
            Mutation::Update {
                key: record.key,
                expect: Some(record.version_token),
                recipe: Arc::new(recipe.build().unwrap()),
            }
        }
    };
    store.apply(mutation, &Cancel::default()).unwrap();
}
fn child(root: &Path, id: &str, mode: &str, phase: &str, ready: &Path) -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "storage::publish::tests::process_child",
            "--ignored",
            "--nocapture",
        ])
        .env("AUDITORYLAB_PUBLISH_ROOT", root)
        .env("AUDITORYLAB_PUBLISH_ID", id)
        .env("AUDITORYLAB_PUBLISH_MODE", mode)
        .env("AUDITORYLAB_PUBLISH_PAUSE", phase)
        .env("AUDITORYLAB_PUBLISH_READY", ready)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}
fn kill_at_pause(child: &mut Child, ready: &Path) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ready.exists() {
        if let Some(status) = child.try_wait().unwrap() {
            let output = child
                .stderr
                .take()
                .map(|mut stream| {
                    let mut text = String::new();
                    stream.read_to_string(&mut text).unwrap();
                    text
                })
                .unwrap_or_default();
            panic!("child exited before pause {status}: {output}");
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("child did not reach bounded pause");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    child.kill().unwrap();
    child.wait().unwrap();
}
fn store_fixture() -> (
    tempfile::TempDir,
    crate::storage::library::LibraryStore,
    crate::storage::library::LoadedRecord,
) {
    let (temp, _dir, _path) = fixture();
    let store = crate::storage::library::LibraryStore::new(
        crate::storage::library::LibraryRoot::explicit(temp.path().join("library")).unwrap(),
    )
    .unwrap();
    store.root().initialize().unwrap();
    let record = store
        .apply(
            crate::storage::library::Mutation::Create(Arc::new(
                crate::document::editing::Draft::new()
                    .unwrap()
                    .recipe()
                    .unwrap(),
            )),
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    (temp, store, record)
}
#[test]
fn killed_recipe_writers_leave_old_or_complete_bytes_and_no_stage_is_promoted() {
    use crate::storage::library::Mutation;
    for phase in ["before", "after"] {
        let (temp, store, record) = store_fixture();
        let ready = temp.path().join("ready");
        let mut process = child(
            store.root().path(),
            &record.snapshot.recipe().document().id.to_string(),
            "update",
            phase,
            &ready,
        );
        kill_at_pause(&mut process, &ready);
        let observed = store.load(&record.key).unwrap();
        assert_eq!(
            observed.snapshot.title() == "Complete candidate",
            phase == "after"
        );
        let before = observed.recipe_toml.clone();
        assert!(!store.scan(&Cancel::default()).stages.is_empty() || phase == "after");
        store
            .apply(
                Mutation::Rename {
                    key: record.key.clone(),
                    expect: Some(observed.version_token),
                    title: observed.snapshot.title(),
                },
                &Cancel::default(),
            )
            .unwrap();
        assert!(store.scan(&Cancel::default()).stages.is_empty());
        assert_eq!(store.load(&record.key).unwrap().recipe_toml, before);
    }
}
#[test]
fn killed_creation_between_recipe_and_membership_keeps_readable_content_with_unknown_added_time() {
    use crate::storage::library::Mutation;
    let (temp, store, _record) = store_fixture();
    let id = "00000000-0000-4000-8000-000000000099";
    let ready = temp.path().join("ready");
    let mut process = child(store.root().path(), id, "create", "after", &ready);
    kill_at_pause(&mut process, &ready);
    let key = LibraryKey::User(id.parse().unwrap());
    let record = store.load(&key).unwrap();
    assert_eq!(record.added_at, None);
    assert!(!record.trashed);
    store
        .apply(
            Mutation::Rename {
                key: key.clone(),
                expect: None,
                title: "Retained unknown addition".into(),
            },
            &Cancel::default(),
        )
        .unwrap();
    assert_eq!(store.load(&key).unwrap().added_at, None);
}
#[test]
fn killed_moves_preserve_content_and_recognize_only_same_inode_duplicates() {
    use crate::storage::library::Mutation;
    for mode in ["trash", "restore"] {
        for phase in ["before-link", "after-link", "after-remove"] {
            let (temp, store, mut record) = store_fixture();
            if mode == "restore" {
                record = store
                    .apply(
                        Mutation::Trash {
                            key: record.key,
                            expect: Some(record.version_token),
                        },
                        &Cancel::default(),
                    )
                    .unwrap()
                    .record
                    .unwrap();
            }
            let bytes = record.recipe_toml.clone();
            let ready = temp.path().join("ready");
            let mut process = child(
                store.root().path(),
                &record.snapshot.recipe().document().id.to_string(),
                mode,
                phase,
                &ready,
            );
            kill_at_pause(&mut process, &ready);
            let current = store.load(&record.key).unwrap();
            assert_eq!(current.recipe_toml, bytes);
            let expected = match phase {
                "before-link" => mode == "restore",
                "after-link" => true,
                _ => mode == "trash",
            };
            assert_eq!(current.trashed, expected, "{mode} {phase}");
            store
                .apply(
                    Mutation::Trash {
                        key: record.key.clone(),
                        expect: Some(current.version_token),
                    },
                    &Cancel::default(),
                )
                .unwrap();
            let name = format!("{}.toml", record.snapshot.recipe().document().id);
            assert!(!store.root().path().join("recipes").join(&name).exists());
            assert!(store.root().path().join("trash").join(name).exists());
        }
    }
    let (_temp, store, record) = store_fixture();
    let name = format!("{}.toml", record.snapshot.recipe().document().id);
    fs::write(
        store.root().path().join("trash").join(&name),
        record.recipe_toml.as_bytes(),
    )
    .unwrap();
    assert!(store.load(&record.key).is_err());
    assert!(!store.scan(&Cancel::default()).complete);
    assert!(
        store
            .apply(
                Mutation::Trash {
                    key: record.key,
                    expect: None
                },
                &Cancel::default()
            )
            .is_err()
    );
    assert!(store.root().path().join("recipes").join(&name).exists());
    assert!(store.root().path().join("trash").join(name).exists());
}
#[test]
fn killed_initialization_marker_is_last_and_retry_cleans_owned_stages() {
    use crate::storage::library::LibraryRoot;
    let (temp, _dir, _path) = fixture();
    let root = temp.path().join("library");
    let ready = temp.path().join("ready");
    let mut process = child(&root, "unused", "initialize", "before", &ready);
    kill_at_pause(&mut process, &ready);
    assert!(!root.join("library.toml").exists());
    assert!(root.join("recipes").is_dir());
    LibraryRoot::explicit(root.clone())
        .unwrap()
        .initialize()
        .unwrap();
    assert!(root.join("library.toml").is_file());
    assert!(!fs::read_dir(root).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".partial")
    }));
}

#[test]
fn killed_purge_never_recreates_the_removed_identity() {
    use crate::storage::library::Mutation;
    let (temp, store, record) = store_fixture();
    let record = store
        .apply(
            Mutation::Trash {
                key: record.key,
                expect: Some(record.version_token),
            },
            &Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    let ready = temp.path().join("ready");
    let mut process = child(
        store.root().path(),
        &record.snapshot.recipe().document().id.to_string(),
        "purge",
        "after-remove",
        &ready,
    );
    kill_at_pause(&mut process, &ready);
    assert!(store.load(&record.key).is_err());
    assert!(store.scan(&Cancel::default()).entries.is_empty());
    assert_eq!(
        store
            .apply(
                Mutation::Restore {
                    key: record.key,
                    expect: None
                },
                &Cancel::default()
            )
            .unwrap_err()
            .outcome(),
        crate::outcome::Outcome::Conflict
    );
}

#[test]
fn failed_membership_write_is_saved_with_unknown_new_dates() {
    use crate::storage::library::{LibraryRoot, LibraryStore, Mutation, MutationStatus};
    let (temp, _dir, _path) = fixture();
    let store =
        LibraryStore::new(LibraryRoot::explicit(temp.path().join("library")).unwrap()).unwrap();
    store.root().initialize().unwrap();
    AFTER_PUBLISH.with(|slot| *slot.borrow_mut() = Some(Box::new(|| inject(&[FaultPoint::Write]))));
    let result = store
        .apply(
            Mutation::Create(Arc::new(
                crate::document::editing::Draft::new()
                    .unwrap()
                    .recipe()
                    .unwrap(),
            )),
            &Cancel::default(),
        )
        .unwrap();
    assert_eq!(result.status, MutationStatus::PublishedWithWarning);
    assert!(result.diagnostics.iter().any(|d| matches!(
        d,
        crate::services::StorageWarning::MembershipUpdateFailed { .. }
    )));
    let record = result.record.unwrap();
    assert_eq!(record.added_at, None);
    assert_eq!(store.load(&record.key).unwrap().added_at, None);
    assert!(record.snapshot.recipe().document().created_at.is_some());
}

#[test]
fn unreadable_publication_verification_never_guesses_the_destination_state() {
    for (point, expected) in [
        (FaultPoint::BeforePublish, b"previous".as_slice()),
        (FaultPoint::AfterPublish, b"candidate".as_slice()),
    ] {
        let (_temp, dir, path) = fixture();
        fs::write(&path, b"previous").unwrap();
        inject(&[point, FaultPoint::Verify]);
        let error = put(&dir, &path, Policy::Replace, b"candidate").unwrap_err();
        assert!(matches!(
            &error,
            Error::AfterMutation {
                confirmation: Confirmation::Unavailable(_),
                ..
            }
        ));
        let cause = std::error::Error::source(&error).unwrap();
        assert!(cause.to_string().contains("injected"));
        assert_eq!(error.outcome(), crate::outcome::Outcome::Write);
        // The test oracle can read actual storage after the injected failure;
        // the operation itself could not, and must report the same uncertainty.
        assert_eq!(fs::read(path).unwrap(), expected);
    }
}

#[test]
fn precommit_hashing_checks_cancellation_between_blocks() {
    use std::{cell::Cell, rc::Rc};
    for phase in ["previous", "stage", "conditional"] {
        let (temp, dir, path) = fixture();
        let original = vec![b'o'; 128 * 1024];
        let candidate = vec![b'n'; 128 * 1024];
        if phase != "stage" {
            fs::write(&path, &original).unwrap();
        }
        let identity = if phase == "conditional" {
            Some(same_file::Handle::from_path(&path).unwrap())
        } else {
            None
        };
        let cancel = Cancel::default();
        let request = cancel.clone();
        let blocks = Rc::new(Cell::new(0));
        let counted = blocks.clone();
        let stop_after = if phase == "conditional" { 5 } else { 1 };
        HASH_BLOCK.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                let count = counted.get() + 1;
                counted.set(count);
                if count == stop_after {
                    request.request();
                }
            }))
        });
        let written = Cell::new(false);
        let policy = match phase {
            "previous" => Policy::Replace,
            "stage" => Policy::CreateNew,
            _ => Policy::ReplaceIfUnchanged {
                fingerprint: Fingerprint::of(&original),
                identity: identity.as_ref().unwrap(),
            },
        };
        let result = publish_in(&dir, Path::new("entry"), &path, policy, &cancel, |file| {
            written.set(true);
            file.write_all(&candidate)
                .map_err(|error| io(&path, IoOperation::Write, error))
        });
        HASH_BLOCK.with(|slot| *slot.borrow_mut() = None);
        assert_eq!(
            result.unwrap_err().outcome(),
            crate::outcome::Outcome::Cancelled,
            "{phase}"
        );
        assert_eq!(
            blocks.get(),
            stop_after,
            "{phase} must stop before reading another block"
        );
        assert_eq!(written.get(), phase != "previous");
        if phase == "stage" {
            assert!(!path.exists());
            assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
        } else {
            assert_eq!(fs::read(&path).unwrap(), original);
            assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
        }
    }
}
#[test]
fn cancellation_after_publication_does_not_interrupt_truth_verification() {
    let (_temp, dir, path) = fixture();
    fs::write(&path, b"previous").unwrap();
    let cancel = Cancel::default();
    let request = cancel.clone();
    AFTER_PUBLISH.with(|slot| *slot.borrow_mut() = Some(Box::new(move || request.request())));
    inject(&[FaultPoint::AfterPublish]);
    let candidate = vec![b'n'; 128 * 1024];
    let result = publish_in(
        &dir,
        Path::new("entry"),
        &path,
        Policy::Replace,
        &cancel,
        |file| {
            file.write_all(&candidate)
                .map_err(|error| io(&path, IoOperation::Write, error))
        },
    )
    .unwrap();
    assert!(cancel.is_requested());
    assert!(!result.warnings.is_empty());
    assert_eq!(fs::read(path).unwrap(), candidate);
}

#[cfg(unix)]
#[test]
fn directory_sync_uses_a_synchronizable_read_handle() {
    let (_temp, dir, _path) = fixture();
    sync_directory(&dir).unwrap();
    assert_eq!(dir.read_dir(".").unwrap().count(), 0);
}

#[test]
fn relocation_flushes_the_source_without_changing_bytes_or_file_identity() {
    let (temp, from, source) = fixture();
    from.create_dir("trash").unwrap();
    let to = from.open_dir("trash").unwrap();
    let name = Path::new("entry");
    let destination = temp.path().join("trash/entry");
    let bytes = b"exact portable bytes\nincluding trailing whitespace \n";
    fs::write(&source, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Unix relocation needs readable content and writable directories,
        // without requiring write permission on the source file itself.
        fs::set_permissions(&source, fs::Permissions::from_mode(0o444)).unwrap();
    }
    let identity = same_file::Handle::from_file(open_regular(&from, name).unwrap()).unwrap();
    assert!(
        relocate(&from, &to, name, &source, &destination, &Cancel::default())
            .unwrap()
            .is_empty()
    );
    assert!(!source.exists());
    assert_eq!(fs::read(&destination).unwrap(), bytes);
    let moved = same_file::Handle::from_file(open_regular(&to, name).unwrap()).unwrap();
    assert_eq!(identity, moved);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o444
        );
    }
    // Restoring uses the same flush contract and retains the same file.
    assert!(
        relocate(&to, &from, name, &destination, &source, &Cancel::default())
            .unwrap()
            .is_empty()
    );
    assert!(!destination.exists());
    assert_eq!(fs::read(&source).unwrap(), bytes);
    assert_eq!(
        identity,
        same_file::Handle::from_file(open_regular(&from, name).unwrap()).unwrap()
    );
    fs::write(&destination, b"foreign destination").unwrap();
    assert_eq!(
        relocate(&from, &to, name, &source, &destination, &Cancel::default())
            .unwrap_err()
            .outcome(),
        crate::outcome::Outcome::Conflict
    );
    assert_eq!(fs::read(&source).unwrap(), bytes);
    assert_eq!(fs::read(&destination).unwrap(), b"foreign destination");
}

#[test]
fn unreadable_deletion_verification_never_claims_the_file_remains() {
    for (point, exists) in [
        (FaultPoint::BeforeRemove, true),
        (FaultPoint::AfterRemove, false),
    ] {
        let (_temp, dir, path) = fixture();
        fs::write(&path, b"original").unwrap();
        inject(&[point, FaultPoint::Verify]);
        let error = remove(&dir, Path::new("entry"), &path).unwrap_err();
        assert!(matches!(
            &error,
            Error::AfterMutation {
                confirmation: Confirmation::Unavailable(_),
                ..
            }
        ));
        let cause = std::error::Error::source(&error).unwrap();
        assert!(cause.to_string().contains("injected"));
        assert_eq!(path.exists(), exists);
    }
}

#[test]
fn unreadable_relocation_verification_does_not_guess_source_removal() {
    for (point, source_exists) in [
        (FaultPoint::BeforeRemove, true),
        (FaultPoint::AfterRemove, false),
    ] {
        let (temp, dir, path) = fixture();
        fs::write(&path, b"original").unwrap();
        dir.create_dir("trash").unwrap();
        let trash = dir.open_dir("trash").unwrap();
        let destination = temp.path().join("trash/entry");
        inject(&[point, FaultPoint::Verify]);
        let error = relocate(
            &dir,
            &trash,
            Path::new("entry"),
            &path,
            &destination,
            &Cancel::default(),
        )
        .unwrap_err();
        assert!(matches!(
            &error,
            Error::AfterMutation {
                confirmation: Confirmation::Unavailable(_),
                ..
            }
        ));
        let cause = std::error::Error::source(&error).unwrap();
        assert!(cause.to_string().contains("injected"));
        assert_eq!(path.exists(), source_exists);
        assert_eq!(fs::read(destination).unwrap(), b"original");
    }
}
