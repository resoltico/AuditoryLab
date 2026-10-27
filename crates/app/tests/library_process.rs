//! Actual independent processes: lock lifetime, lost updates and killed native writers.
use auditorylab_app::document::editing::Draft;
use auditorylab_app::document::origin::LibraryKey;
use auditorylab_app::job::Cancel;
use auditorylab_app::outcome::Outcome;
use auditorylab_app::services::library::{Library, Mutation};
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
    Library,
    auditorylab_app::services::library::LoadedRecord,
) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let temp = tempfile::tempdir_in(base).unwrap();
    let root = temp.path().join("library");
    let store = auditorylab_app::services::library::Library::at_path(root.clone()).unwrap();
    store.initialize().unwrap();
    let mut draft = Draft::new().unwrap();
    draft.set_title("Initial".into());
    let record = store
        .apply(
            Mutation::Create(Arc::new(draft.recipe().unwrap())),
            &Cancel::default(),
        )
        .unwrap()
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
    record: &auditorylab_app::services::library::LoadedRecord,
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
    let store = auditorylab_app::services::library::Library::at_path(root).unwrap();
    let key: LibraryKey = std::env::var("AUDITORYLAB_STORE_TEST_KEY")
        .unwrap()
        .parse()
        .unwrap();
    let expect: auditorylab_app::document::origin::VersionToken =
        std::env::var("AUDITORYLAB_STORE_TEST_EXPECT")
            .unwrap()
            .parse()
            .unwrap();
    if mode == "race-a" || mode == "race-b" {
        fs::write(&ready, b"ready").unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !go.exists() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    assert!(matches!(mode.as_str(), "race-a" | "race-b"));
    let mutation = Mutation::Rename {
        key,
        expect: Some(expect),
        title: mode,
    };
    let outcome = store.apply(mutation, &Cancel::default());
    let report = match outcome {
        Ok(result) => {
            serde_json::json!({"code":0,"status":format!("{:?}",result.status)})
        }
        Err(error) => {
            serde_json::json!({"code":match error.outcome(){Outcome::Conflict=>3,Outcome::Busy=>4,_=>1},"error":error.to_string()})
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
                    expect: Some(record.version_token),
                    title: "must be busy".into(),
                },
                &Cancel::default(),
            )
            .unwrap_err();
        assert_eq!(error.outcome(), Outcome::Busy);
        assert!(start.elapsed() < Duration::from_secs(2));
        if mode == "exclusive" {
            let scan = store.scan(&Cancel::default());
            assert!(!scan.complete);
            assert_eq!(scan.failure_kind, Some(Outcome::Busy));
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
                        expect: Some(record.version_token),
                        title: "after process closure".into()
                    },
                    &Cancel::default()
                )
                .unwrap()
                .record
                .is_some()
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
                    expect: Some(record.version_token),
                    recipe: Arc::new(record.snapshot.recipe().clone())
                },
                &Cancel::default()
            )
            .unwrap_err()
            .outcome(),
        Outcome::Conflict,
        "even a stale no-op must recheck"
    );
    let current = store.load(&record.key).unwrap();
    assert!(matches!(
        current.snapshot.title().as_str(),
        "race-a" | "race-b"
    ));
}
