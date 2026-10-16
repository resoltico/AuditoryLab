// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Document selection rejects local streams before preparation or publication.
#![cfg(unix)]
use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
fn explicit_file_fifo_rejects_promptly_and_stdin_retains_stream_semantics() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("recipe.pipe");
    let destination = dir.path().join("not-created.wav");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .args(["export", "--file"])
        .arg(&fifo)
        .arg("--output")
        .arg(&destination)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("explicit file import blocked on FIFO");
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = child.wait_with_output().unwrap();
    assert!(!status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("regular file"), "{error}");
    assert!(error.contains("--stdin"), "{error}");
    assert!(!destination.exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    let mut streamed = Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .args(["inspect", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    streamed
        .stdin
        .take()
        .unwrap()
        .write_all(b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0\n")
        .unwrap();
    let output = streamed.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
