//! `auditorylab analyze` through the shipped executable.
#![cfg(test)]
use std::{fs, process::Command};

fn analyze(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_auditorylab"))
        .arg("analyze")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn analyze_reports_file_identity_as_text_and_json_and_fails_on_missing_input() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let path = dir.path().join("excerpt.m4a");
    let mut bytes = b"\0\0\0\x20ftypM4A ".to_vec();
    bytes.resize(2_500, 0);
    fs::write(&path, &bytes).unwrap();
    let path_text = path.to_str().unwrap();

    let text = analyze(&[path_text]);
    assert!(text.status.success(), "{text:?}");
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.starts_with("Recording: excerpt.m4a\n"), "{text}");
    assert!(text.contains("\nSize: 2.5 kB (2500 bytes)\n"), "{text}");
    assert!(text.contains("\nFormat: MPEG-4 (M4A/MP4)\n"), "{text}");
    assert!(
        text.contains("It does not decode or measure the audio."),
        "{text}"
    );

    let json = analyze(&["--format", "json", path_text]);
    assert!(json.status.success(), "{json:?}");
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(json["operation"], "analyze_recording");
    assert_eq!(json["complete"], true);
    assert_eq!(json["recording"]["path"], path_text);
    assert_eq!(json["recording"]["bytes"], 2_500);
    assert_eq!(json["recording"]["format"], "mp4");

    let missing = dir.path().join("missing.wav");
    let failed = analyze(&["--format", "json", missing.to_str().unwrap()]);
    assert_eq!(failed.status.code(), Some(1));
    let failed: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(failed["complete"], false);
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["diagnostics"][0]["code"], "io.failure");
    assert!(failed.get("recording").is_none());

    let directory = analyze(&[dir.path().to_str().unwrap()]);
    assert_eq!(directory.status.code(), Some(1));
    // Windows refuses to open a directory before admission can name it.
    #[cfg(unix)]
    assert!(
        String::from_utf8(directory.stderr)
            .unwrap()
            .contains("regular file")
    );
}
