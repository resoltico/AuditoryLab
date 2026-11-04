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
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../app/tests/fixtures/recording/tone.flac");
    let path_text = path.to_str().unwrap();
    let text = analyze(&[path_text, "--seconds", "2"]);
    assert!(text.status.success(), "{text:?}");
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.starts_with("Recording: tone.flac\n"), "{text}");
    for section in [
        "Summary",
        "Tones",
        "Left versus right",
        "Pulsing",
        "Low tones",
        "Stereo and level",
        "Limits",
    ] {
        assert!(text.contains(section), "{text}");
    }
    assert!(text.contains("1000.0"), "{text}");
    assert!(text.contains("40.0"), "{text}");
    assert!(text.contains("Digital samples only"), "{text}");
    let json = analyze(&["--format", "json", path_text, "--seconds", "2"]);
    assert!(json.status.success(), "{json:?}");
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(json["output_schema_version"], 3);
    assert_eq!(json["operation"], "analyze_recording");
    assert_eq!(json["complete"], true);
    assert_eq!(json["recording"]["path"], path_text);
    assert_eq!(
        json["recording"]["bytes"],
        fs::metadata(&path).unwrap().len()
    );
    assert_eq!(json["recording"]["format"], "flac");
    let a = &json["recording"]["analysis"];
    assert!(
        (a["tones"][0]["hz"].as_f64().unwrap() - 1000.0).abs()
            < a["resolution_hz"].as_f64().unwrap()
    );
    assert!(
        (a["window"]["end_seconds"].as_f64().unwrap()
            - a["window"]["start_seconds"].as_f64().unwrap()
            - 2.0)
            .abs()
            < 1e-8
    );
    assert!(
        a["limits"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("digital_samples_only"))
    );

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

#[test]
fn opus_and_malformed_media_are_typed_failures() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../app/tests/fixtures/recording/tone.opus");
    let failed = analyze(&["--format", "json", path.to_str().unwrap()]);
    assert_eq!(failed.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(
        json["diagnostics"][0]["code"],
        "recording.unsupported_codec"
    );
    assert!(
        json["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("convert to WAV or FLAC")
    );
    let invalid = analyze(&[path.to_str().unwrap(), "--seconds", "0"]);
    assert_eq!(invalid.status.code(), Some(2));
}
