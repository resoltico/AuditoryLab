//! Small controlled recordings through the actual decoder and shared human/wire consumers.
#![cfg(test)]
use auditorylab_app::{
    job::{Cancel, Progress},
    recording::AnalysisOptions,
    services,
};
use auditorylab_presentation::{recording, report::Report};
use std::{fs, io::Write, path::Path};

#[expect(
    clippy::cast_possible_truncation,
    reason = "bounded synthetic sums are quantized to the WAV f32 sample contract"
)]
fn write_tones(path: &Path) {
    let frames = 64_000_u32;
    let bytes = frames * 8;
    let mut file = fs::File::create(path).unwrap();
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + bytes).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt \x10\0\0\0\x03\0\x02\0").unwrap();
    file.write_all(&8000_u32.to_le_bytes()).unwrap();
    file.write_all(&64_000_u32.to_le_bytes()).unwrap();
    file.write_all(b"\x08\0\x20\0data").unwrap();
    file.write_all(&bytes.to_le_bytes()).unwrap();
    for frame in 0..frames {
        let t = f64::from(frame) / 8000.0;
        for frequencies in [[200.0, 220.0, 240.0], [205.0, 227.0, 249.0]] {
            let value: f64 = frequencies
                .iter()
                .map(|hz| 0.08 * (std::f64::consts::TAU * hz * t).sin())
                .sum();
            file.write_all(&(value as f32).to_le_bytes()).unwrap();
        }
    }
}

#[test]
fn completed_recording_coordinates_and_full_candidate_data_survive_shared_rendering() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let path = dir.path().join("controlled-stereo.wav");
    write_tones(&path);
    for (start, length, expected, envelope) in [
        (0, 2, "Analyzed: 0.0–2.0 s", "Envelope window: 0.1–1.9 s"),
        (3, 2, "Analyzed: 3.0–5.0 s", "Envelope window: 3.1–4.9 s"),
        (6, 4, "Analyzed: 6.0–8.0 s", "Envelope window: 6.1–7.9 s"),
    ] {
        let result = services::analyze_recording(
            &path,
            AnalysisOptions {
                start_seconds: start,
                seconds: Some(length),
            },
            &Cancel::default(),
            &Progress::default(),
        )
        .unwrap();
        let json_before = serde_json::to_value(Report::analyzed_recording(&result)).unwrap();
        let f = recording::recording_findings(&result.analysis, None);
        assert!(f.facts.contains(expected), "{}", f.facts);
        assert!(f.facts.contains(envelope));
        assert_eq!(f.facts.matches("Analyzed:").count(), 1);
        assert_eq!(f.facts.matches("Envelope window:").count(), 1);
        assert_eq!(
            result.analysis.ear_differences.len(),
            9,
            "three exclusive pitches per channel yield nine threshold-qualified comparisons"
        );
        assert!(
            f.summary
                .contains("9 left-versus-right tone comparison candidates")
        );
        assert!(
            f.summary
                .contains("See Left versus right for the complete list.")
        );
        assert!(!f.summary.contains("Left 200."));
        assert_eq!(f.ears.lines().count(), 9);
        let wire = &json_before["recording"]["analysis"]["ear_differences"];
        assert_eq!(wire.as_array().unwrap().len(), 9);
        for (row, pair) in wire
            .as_array()
            .unwrap()
            .iter()
            .zip(&result.analysis.ear_differences)
        {
            assert_eq!(row["left_hz"], pair.left_hz);
            assert_eq!(row["right_hz"], pair.right_hz);
            assert_eq!(row["difference_hz"], pair.difference_hz);
        }
        let report = recording::analysis_description(&result);
        assert_eq!(report.matches("Analyzed:").count(), 1);
        assert_eq!(report.matches("Envelope window:").count(), 1);
        assert!(report.contains(expected));
        assert!(report.contains("not creator intent"));
        assert_eq!(
            serde_json::to_value(Report::analyzed_recording(&result)).unwrap(),
            json_before
        );
    }
}
