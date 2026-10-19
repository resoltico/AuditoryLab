//! CLI contracts through the shipped executable.
#![cfg(test)]
#[cfg(unix)]
use std::time::{Duration, Instant};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .args(args)
        .output()
        .unwrap()
}
fn dir() -> tempfile::TempDir {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    tempfile::tempdir_in(base).unwrap()
}
const ID: &str = "chan-2022-clicks-40-hz";
const SILENCE: &[u8] = b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0\n";
#[test]
fn commands_sources_and_exit_codes() {
    for args in [
        vec![],
        vec!["--help"],
        vec!["-V"],
        vec!["list"],
        vec!["describe", ID],
        vec!["inspect", ID],
        vec!["recipe", ID],
    ] {
        let out = cli(&args);
        assert!(out.status.success(), "{args:?}: {:?}", out);
        assert!(!out.stdout.is_empty());
    }
    for args in [
        vec!["inspect"],
        vec!["inspect", ID, "--stdin"],
        vec!["inspect", "--file", "x", "--stdin"],
        vec!["export", ID],
        vec!["inspect", ID, "--sample-rate", "768001"],
        vec!["--build-info"],
        vec!["unknown"],
    ] {
        assert_eq!(cli(&args).status.code(), Some(2), "{args:?}");
    }
    let unknown = cli(&["inspect", "unknown"]);
    assert_eq!(unknown.status.code(), Some(2));
    let text = String::from_utf8(unknown.stderr).unwrap();
    assert!(text.contains("list") && text.contains("--file"));
    assert_eq!(
        cli(&["inspect", "--file", "/missing/auditorylab.toml"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        cli(&["recipe", ID]).stdout,
        auditorylab::builtin_source(ID).unwrap().as_bytes()
    );
}
#[test]
fn stdin_export_and_no_clobber() {
    let dir = dir();
    let path = dir.path().join("sound.wav");
    for success in [true, false] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
            .args(["export", "--stdin", "--output"])
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(SILENCE).unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.success(), success);
        if !success {
            assert_eq!(out.status.code(), Some(1));
        }
    }
    assert_eq!(fs::read(&path).unwrap().len(), 58 + 480 * 8);
    let recipe = dir.path().join("bad.toml");
    fs::write(&recipe, b"invalid").unwrap();
    assert_eq!(
        cli(&["inspect", "--file", recipe.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
}
#[test]
fn broken_pipe_is_quiet_success() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .args(["inspect", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Input cannot finish until after the output reader has closed.
    drop(child.stdout.take());
    child.stdin.take().unwrap().write_all(SILENCE).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(out.stderr.is_empty());
}
#[test]
#[ignore = "plays one second of component-free silence on the default output"]
fn play_reads_stdin_and_reports_native_silent_playback() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .args(["play", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"schema_version=2\nduration_seconds=1\nfade_seconds=0\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("Played "));
    let start = String::from_utf8(out.stderr).unwrap();
    assert!(start.contains("Press Ctrl-C to stop") && start.contains("Turn the volume down"));
}
#[cfg(unix)]
#[test]
fn sigint_cancels_a_long_silent_export_and_removes_the_stage() {
    let dir = dir();
    let input = dir.path().join("silence.toml");
    let output = dir.path().join("long.wav");
    fs::write(
        &input,
        b"schema_version=2\nduration_seconds=1000000\nfade_seconds=0\n",
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .args(["export", "--file"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !fs::read_dir(dir.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".partial")
    }) {
        assert!(child.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(130));
    assert!(!output.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
