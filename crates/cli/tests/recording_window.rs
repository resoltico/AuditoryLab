//! Absolute recording windows through the shipped CLI and decoder.
use std::{io::Write, process::Command};
#[test]
fn analysis_uses_absolute_start_and_strict_shared_seconds_grammar() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("silence.wav");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(b"RIFF").unwrap();
    file.write_all(&8036_u32.to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt \x10\0\0\0\x01\0\x01\0").unwrap();
    file.write_all(&1000_u32.to_le_bytes()).unwrap();
    file.write_all(&2000_u32.to_le_bytes()).unwrap();
    file.write_all(b"\x02\0\x10\0data").unwrap();
    file.write_all(&8000_u32.to_le_bytes()).unwrap();
    file.set_len(8044).unwrap();
    drop(file);
    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_auditorylab"))
            .arg("analyze")
            .arg(&path)
            .args(["--format", "json"])
            .args(extra)
            .output()
            .unwrap()
    };
    let selected = run(&["--start", "1", "--seconds", "2"]);
    assert!(
        selected.status.success(),
        "{}",
        String::from_utf8_lossy(&selected.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&selected.stdout).unwrap();
    assert_eq!(
        value["recording"]["analysis"]["window"]["start_seconds"],
        1.0
    );
    assert_eq!(value["recording"]["analysis"]["window"]["end_seconds"], 3.0);
    let default = run(&[]);
    assert!(default.status.success());
    let value: serde_json::Value = serde_json::from_slice(&default.stdout).unwrap();
    assert_eq!(
        value["recording"]["analysis"]["window"]["start_seconds"],
        0.0
    );
    assert_eq!(value["recording"]["analysis"]["window"]["end_seconds"], 4.0);
    for arguments in [
        ["--start", "+1"],
        ["--start", " 1"],
        ["--start", "1.0"],
        ["--seconds", "3601"],
        ["--seconds", "1"],
    ] {
        assert_eq!(run(&arguments).status.code(), Some(2));
    }
}
