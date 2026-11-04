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

#[test]
#[expect(
    clippy::cast_possible_truncation,
    reason = "independent float-WAV oracle quantizes to f32"
)]
fn lower_boundary_right_only_modulation_agrees_in_summary_and_json() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let path = dir.path().join("right-am.wav");
    let rate = 8000_u32;
    let mut data = Vec::new();
    for i in 0..rate * 20 {
        let t = f64::from(i) / f64::from(rate);
        let carrier = 0.1 * (std::f64::consts::TAU * 1000.0 * t).sin();
        for sample in [
            carrier,
            carrier * (1.0 + 0.8 * (std::f64::consts::TAU * 0.5 * t).cos()),
        ] {
            data.extend_from_slice(&(sample as f32).to_le_bytes());
        }
    }
    let length = u32::try_from(data.len()).unwrap();
    let mut wav = b"RIFF".to_vec();
    wav.extend_from_slice(&(36 + length).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&3_u16.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 8).to_le_bytes());
    wav.extend_from_slice(&8_u16.to_le_bytes());
    wav.extend_from_slice(&32_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&length.to_le_bytes());
    wav.extend(data);
    fs::write(&path, wav).unwrap();
    let output = analyze(&[path.to_str().unwrap()]);
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let summary = text
        .split("\nSummary\n")
        .nth(1)
        .unwrap()
        .split("\n\nTones\n")
        .next()
        .unwrap();
    assert!(
        summary.contains(
            "In Right, the whole signal has loudness variation at 0.50 Hz; \
        this is a modulation pattern."
        ),
        "{summary}"
    );
    assert!(!summary.contains("In Left,"), "{summary}");
    let output = analyze(&[path.to_str().unwrap(), "--format", "json"]);
    assert!(output.status.success(), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["output_schema_version"], 3);
    let pulses = json["recording"]["analysis"]["pulsing"].as_array().unwrap();
    assert!(pulses.iter().any(|p| p["channel"] == 1
        && p["envelope_band"] == "whole"
        && (p["hz"].as_f64().unwrap() - 0.5).abs() < 0.003
        && p["cause"]["kind"] == "modulation"));
    assert!(
        !pulses
            .iter()
            .any(|p| p["channel"] == 0 && p["envelope_band"] == "whole")
    );
    assert_eq!(
        json["recording"]["analysis"]["minimum_resolved_envelope_hz"],
        0.5
    );
    let short = analyze(&[path.to_str().unwrap(), "--seconds", "2", "--format", "json"]);
    assert!(short.status.success(), "{short:?}");
    let short: serde_json::Value = serde_json::from_slice(&short.stdout).unwrap();
    assert_eq!(
        short["recording"]["analysis"]["pulsing"],
        serde_json::json!([])
    );
    assert!(
        short["recording"]["analysis"]["minimum_resolved_envelope_hz"]
            .as_f64()
            .unwrap()
            > 1.9
    );
    let text = analyze(&[path.to_str().unwrap(), "--seconds", "2"]);
    assert!(
        String::from_utf8(text.stdout)
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains("Envelope rates below 3.91 Hz are unresolved")
    );
}
