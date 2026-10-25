//! Actual independent processes: lock lifetime, lost updates and killed native writers.
use auditorylab::editing::Draft;
use auditorylab::error::ErrorKind;
use auditorylab::library::{LibraryKey, LibraryRoot, LibraryStore, Mutation, StoreFault};
use auditorylab::workflow::Cancel;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
#[expect(
    clippy::unwrap_used,
    reason = "Controlled native process fixture must construct or fail its caller"
)]
fn fixture() -> (
    tempfile::TempDir,
    PathBuf,
    LibraryStore,
    auditorylab::library::LoadedRecord,
) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let root = temp.path().join("library");
    let store = LibraryStore::new(LibraryRoot::explicit(root.clone()).unwrap()).unwrap();
    store.root().initialize().unwrap();
    let mut draft = Draft::new().unwrap();
    draft.set_title("Initial".into());
    let record = store
        .apply(
            Mutation::Create(Arc::new(draft.recipe())),
            &Cancel::default(),
        )
        .unwrap()
        .receipt
        .record
        .unwrap();
    (temp, root, store, record)
}
#[expect(
    clippy::unwrap_used,
    reason = "Spawning the current integration test binary is a required test precondition"
)]
fn child(
    root: &Path,
    mode: &str,
    ready: &Path,
    go: &Path,
    record: &auditorylab::library::LoadedRecord,
    result: &Path,
) -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "controlled_child", "--ignored", "--nocapture"])
        .env("AUDITORYLAB_STORE_TEST_ROOT", root)
        .env("AUDITORYLAB_STORE_TEST_MODE", mode)
        .env("AUDITORYLAB_STORE_TEST_READY_FILE", ready)
        .env("AUDITORYLAB_STORE_TEST_GO_FILE", go)
        .env("AUDITORYLAB_STORE_TEST_KEY", record.key.to_string())
        .env(
            "AUDITORYLAB_STORE_TEST_EXPECT",
            record.version_token.to_string(),
        )
        .env("AUDITORYLAB_STORE_TEST_RESULT_FILE", result)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}
#[expect(
    clippy::unwrap_used,
    reason = "Controlled test waits must observe process errors rather than silently time out"
)]
fn ready(child: &mut Child, path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("controlled child exited before readiness: {status}");
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("controlled child failed to become ready");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
#[ignore = "subprocess entry point; invoked explicitly by the native parent tests"]
fn controlled_child() {
    let root = PathBuf::from(std::env::var_os("AUDITORYLAB_STORE_TEST_ROOT").unwrap());
    let mode = std::env::var("AUDITORYLAB_STORE_TEST_MODE").unwrap();
    let ready = PathBuf::from(std::env::var_os("AUDITORYLAB_STORE_TEST_READY_FILE").unwrap());
    let go = PathBuf::from(std::env::var_os("AUDITORYLAB_STORE_TEST_GO_FILE").unwrap());
    if mode == "initialize" {
        let _ = LibraryRoot::explicit(root)
            .unwrap()
            .initialize_paused(&ready);
        return;
    }
    if mode == "exclusive" || mode == "shared" {
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join(".library.lock"))
            .unwrap();
        if mode == "exclusive" {
            lock.try_lock().unwrap();
        } else {
            lock.try_lock_shared().unwrap();
        }
        fs::write(ready, b"ready").unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !go.exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        drop(lock);
        return;
    }
    let mut store = LibraryStore::new(LibraryRoot::explicit(root).unwrap()).unwrap();
    let key: LibraryKey = std::env::var("AUDITORYLAB_STORE_TEST_KEY")
        .unwrap()
        .parse()
        .unwrap();
    let expect = std::env::var("AUDITORYLAB_STORE_TEST_EXPECT")
        .unwrap()
        .parse()
        .unwrap();
    let fault = match mode.as_str() {
        "before" => Some(StoreFault::PauseBeforePublication),
        "after" => Some(StoreFault::PauseAfterPublication),
        "purge" => Some(StoreFault::PauseAfterPurge),
        _ => None,
    };
    if let Some(fault) = fault {
        store = store.with_fault(Some(fault));
    }
    if mode == "race-a" || mode == "race-b" {
        fs::write(&ready, b"ready").unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !go.exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let mutation = if mode == "purge" {
        Mutation::Purge {
            key,
            expect,
            confirmed: true,
        }
    } else {
        Mutation::Rename {
            key,
            expect,
            title: if mode.starts_with("race") {
                mode
            } else {
                "Crash candidate".into()
            },
        }
    };
    let outcome = store.apply(mutation, &Cancel::default());
    let report = match outcome {
        Ok(result) => {
            serde_json::json!({"code":if result.confirmed(){0}else{1},"status":format!("{:?}",result.status)})
        }
        Err(error) => {
            serde_json::json!({"code":match error.kind(){ErrorKind::Conflict=>3,ErrorKind::Busy=>4,_=>1},"error":error.to_string()})
        }
    };
    fs::write(
        std::env::var_os("AUDITORYLAB_STORE_TEST_RESULT_FILE").unwrap(),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
}
#[test]
fn shared_and_exclusive_locks_exclude_real_other_processes_and_die_with_handles() {
    for mode in ["exclusive", "shared"] {
        let (temp, root, store, record) = fixture();
        let signal = temp.path().join("ready");
        let go = temp.path().join("go");
        let result = temp.path().join("result");
        let mut process = child(&root, mode, &signal, &go, &record, &result);
        ready(&mut process, &signal);
        let start = Instant::now();
        let error = store
            .apply(
                Mutation::Rename {
                    key: record.key.clone(),
                    expect: record.version_token.clone(),
                    title: "must be busy".into(),
                },
                &Cancel::default(),
            )
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Busy);
        assert!(start.elapsed() < Duration::from_secs(2));
        if mode == "exclusive" {
            let scan = store.scan(&Cancel::default());
            assert!(!scan.complete);
            assert_eq!(scan.failure_kind, Some(ErrorKind::Busy));
        } else {
            assert!(store.scan(&Cancel::default()).complete);
        }
        process.kill().unwrap();
        let output = process.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(root.join(".library.lock").is_file());
        assert!(
            store
                .apply(
                    Mutation::Rename {
                        key: record.key,
                        expect: record.version_token,
                        title: "after process closure".into()
                    },
                    &Cancel::default()
                )
                .unwrap()
                .confirmed()
        );
    }
}
#[test]
fn two_real_editors_cannot_overwrite_the_same_observed_version() {
    let (temp, root, store, record) = fixture();
    let go = temp.path().join("go");
    let a_ready = temp.path().join("a-ready");
    let b_ready = temp.path().join("b-ready");
    let a_result = temp.path().join("a-result");
    let b_result = temp.path().join("b-result");
    let mut a = child(&root, "race-a", &a_ready, &go, &record, &a_result);
    let mut b = child(&root, "race-b", &b_ready, &go, &record, &b_result);
    ready(&mut a, &a_ready);
    ready(&mut b, &b_ready);
    fs::write(go, b"go").unwrap();
    let ao = a.wait_with_output().unwrap();
    let bo = b.wait_with_output().unwrap();
    assert!(
        ao.status.success(),
        "{}",
        String::from_utf8_lossy(&ao.stderr)
    );
    assert!(
        bo.status.success(),
        "{}",
        String::from_utf8_lossy(&bo.stderr)
    );
    let a: serde_json::Value = serde_json::from_slice(&fs::read(a_result).unwrap()).unwrap();
    let b: serde_json::Value = serde_json::from_slice(&fs::read(b_result).unwrap()).unwrap();
    let codes = [a["code"].as_i64().unwrap(), b["code"].as_i64().unwrap()];
    assert_eq!(codes.iter().filter(|&&c| c == 0).count(), 1, "{a} {b}");
    assert!(codes.iter().any(|c| matches!(c, 3 | 4)));
    assert_eq!(
        store
            .apply(
                Mutation::Update {
                    key: record.key.clone(),
                    expect: record.version_token,
                    recipe: Arc::new(record.snapshot.recipe().clone())
                },
                &Cancel::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Conflict,
        "even a stale no-op must recheck"
    );
    let current = store.load(&record.key).unwrap();
    assert!(matches!(
        current.snapshot.title().as_str(),
        "race-a" | "race-b"
    ));
}
#[test]
fn killed_writers_leave_old_or_complete_new_records_and_never_promote_orphan_stages() {
    for mode in ["before", "after"] {
        let (temp, root, store, record) = fixture();
        let signal = temp.path().join("ready");
        let mut process = child(
            &root,
            mode,
            &signal,
            &temp.path().join("go"),
            &record,
            &temp.path().join("result"),
        );
        ready(&mut process, &signal);
        process.kill().unwrap();
        process.wait().unwrap();
        let restarted = LibraryStore::new(store.root().clone()).unwrap();
        let observed = restarted.load(&record.key).unwrap();
        assert_eq!(
            observed.snapshot.title() == "Crash candidate",
            mode == "after"
        );
        let scan = restarted.scan(&Cancel::default());
        assert!(scan.complete);
        assert_eq!(scan.entries.len(), 1);
        if mode == "before" {
            assert_eq!(scan.stages.len(), 1);
            assert_eq!(observed.version_token, record.version_token);
            assert_eq!(observed.snapshot.title(), "Initial");
        }
    }
}
#[test]
fn killed_purge_has_no_attributable_receipt_on_restart_and_never_recreates_target() {
    let (temp, root, store, record) = fixture();
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
    let signal = temp.path().join("ready");
    let mut process = child(
        &root,
        "purge",
        &signal,
        &temp.path().join("go"),
        &trash,
        &temp.path().join("result"),
    );
    ready(&mut process, &signal);
    process.kill().unwrap();
    process.wait().unwrap();
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
    assert!(restarted.scan(&Cancel::default()).entries.is_empty());
}

#[test]
fn killed_initialization_publishes_marker_last_and_retry_retains_owned_orphan() {
    let (temp, _root, _store, record) = fixture();
    let root = temp.path().join("interrupted-init");
    let signal = temp.path().join("init-ready");
    let mut process = child(
        &root,
        "initialize",
        &signal,
        &temp.path().join("unused-go"),
        &record,
        &temp.path().join("unused-result"),
    );
    ready(&mut process, &signal);
    assert!(root.join("records").is_dir());
    assert!(!root.join("library.json").exists());
    process.kill().unwrap();
    process.wait().unwrap();
    let root = LibraryRoot::explicit(root).unwrap();
    assert!(root.initialize().unwrap().created);
    let store = LibraryStore::new(root).unwrap();
    let inventory = store.scan(&Cancel::default());
    assert!(inventory.complete);
    assert_eq!(inventory.stages.len(), 1);
    assert!(inventory.stages[0].starts_with("root/.library."));
    assert!(inventory.entries.is_empty());
}
