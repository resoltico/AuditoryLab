//! Independent schema-4 wire witnesses; domain validity and publication are separate facts.
use auditorylab_app::{
    outcome::Outcome,
    services,
    services::library::{Inventory, LibraryContext, MutationKind, MutationStatus, WriteOutcome},
    vocabulary::Operation,
};
use auditorylab_core::{SampleRate, SampleRateError};
use auditorylab_presentation::report::{ListingOperation, Report, json_bytes};
use serde_json::{Value, json};

#[expect(
    clippy::unwrap_used,
    reason = "Independent fixture structure failures must fail the test"
)]
fn subset(actual: &Value, expected: &Value) {
    match expected {
        Value::Object(fields) => {
            for (key, value) in fields {
                assert!(actual.get(key).is_some(), "missing wire field {key}");
                subset(&actual[key], value);
            }
        }
        Value::Array(values) => {
            assert!(actual.as_array().unwrap().len() >= values.len());
            for (index, value) in values.iter().enumerate() {
                subset(&actual[index], value);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}
#[expect(
    clippy::unwrap_used,
    reason = "Handwritten JSON fixtures must parse or fail the test"
)]
fn fixture(name: &str) -> Value {
    serde_json::from_str(match name {
        "invalid" => include_str!("fixtures/report/invalid-inspection.json"),
        "partial" => include_str!("fixtures/report/partial-inventory.json"),
        "purge" => include_str!("fixtures/report/published-purge.json"),
        "cancelled" => include_str!("fixtures/report/cancelled.json"),
        _ => panic!("unknown fixture"),
    })
    .unwrap()
}
#[test]
fn completed_inspection_can_be_invalid_and_partial_inventory_keeps_its_own_completeness() {
    let directory = tempfile::tempdir().unwrap();
    let context = LibraryContext::resolve(Some(directory.path()));
    let source = services::resolve(
        services::Source::Selector("builtin:ioannou-2015-binaural-10-hz".parse().unwrap()),
        &context,
    )
    .unwrap();
    let inspection = services::inspect(&source, SampleRate::new(1).unwrap());
    assert!(inspection.sound().is_err());
    let report = serde_json::to_value(Report::inspect(&inspection)).unwrap();
    subset(&report, &fixture("invalid"));
    assert!(report.get("status").is_none());
    let inventory = Inventory {
        entries: vec![source.summary().clone()],
        diagnostics: vec![],
        stages: vec![],
        complete: false,
        not_initialized: false,
        ..Inventory::default()
    };
    let report = serde_json::to_value(Report::inventory(&inventory)).unwrap();
    subset(&report, &fixture("partial"));
    assert_eq!(report["entries"].as_array().unwrap().len(), 1);
}
#[test]
fn publication_states_and_explicit_absent_purge_token_are_independent_of_failures() {
    let mut outcome = WriteOutcome {
        status: MutationStatus::PublishedWithWarning,
        key: "user:00000000-0000-4000-8000-000000000001".parse().unwrap(),
        kind: MutationKind::Purge,
        record: None,
        diagnostics: vec![],
    };
    let report = serde_json::to_value(Report::mutation(&outcome)).unwrap();
    subset(&report, &fixture("purge"));
    assert!(report.get("complete").is_none());
    assert!(report.get("version_token").is_some_and(Value::is_null));
    for (status, expected) in [
        (MutationStatus::Published, "published"),
        (MutationStatus::Unchanged, "unchanged"),
    ] {
        outcome.status = status;
        assert_eq!(
            serde_json::to_value(Report::mutation(&outcome)).unwrap()["status"],
            expected
        );
    }
    for (outcome, status) in [
        (Outcome::Cancelled, "cancelled"),
        (Outcome::Conflict, "conflict"),
        (Outcome::InvalidArgument, "rejected"),
        (Outcome::Unreadable, "unreadable"),
        (Outcome::Write, "failed"),
    ] {
        let report = serde_json::to_value(Report::error(
            Operation::AnalyzeRecording,
            &SampleRateError { hz: 0 },
            "Cancelled",
            outcome,
            None,
        ))
        .unwrap();
        assert_eq!(report["status"], status);
        assert!(report.get("version_token").is_none());
        if outcome == Outcome::Cancelled {
            subset(&report, &fixture("cancelled"));
        }
    }
    assert!(ListingOperation::try_from(Operation::LibraryPurge).is_err());
}
#[test]
fn emitted_bytes_escape_boundary_characters_and_decoding_preserves_authored_data() {
    let authored =
        "\u{0001}\u{061c}\u{200e}\u{200f}\u{202a}\u{202e}\u{2066}\u{2069}\u{2028}\u{2029}";
    let bytes = json_bytes(&Report::error(
        Operation::Inspect,
        &SampleRateError { hz: 0 },
        authored,
        Outcome::InvalidArgument,
        None,
    ))
    .unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    for character in authored.chars() {
        assert!(!text.contains(character));
    }
    let decoded: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded["diagnostics"][0]["message"], authored);
    assert_eq!(
        decoded["diagnostics"][0]["params"],
        json!({"hz":0,"min_hz":1,"max_hz":768_000})
    );
    let human = auditorylab_presentation::display_text(authored);
    assert!(!human.contains('\u{202e}'));
}

#[test]
fn failed_and_unconfirmed_publication_have_distinct_wire_states() {
    use auditorylab_app::services::{Confirmation, FileError};
    let unavailable = Confirmation::Unavailable(Box::new(FileError::MissingFilename));
    for (confirmation, expected) in [
        (None, "publication_failed"),
        (Some(&Confirmation::Unchanged), "publication_failed"),
        (Some(&Confirmation::Different), "publication_unconfirmed"),
        (Some(&unavailable), "publication_unconfirmed"),
    ] {
        let report = Report::error(
            Operation::LibraryAdd,
            &SampleRateError { hz: 0 },
            "publication failed",
            Outcome::Write,
            confirmation,
        );
        assert!(!report.completion_confirmed());
        let value = serde_json::to_value(report).unwrap();
        assert_eq!(value["status"], expected);
        assert!(value.get("version_token").is_none());
    }
}

#[test]
fn failed_root_inspection_does_not_invent_unobserved_initialization_state() {
    let directory = tempfile::tempdir().unwrap();
    let context = LibraryContext::resolve(Some(directory.path()));
    let report = Report::unavailable_path(
        context.location().unwrap(),
        Some(true),
        &services::library::LibraryError::Busy,
    );
    let value = serde_json::to_value(report).unwrap();
    assert_eq!(value["root"]["exists"], true);
    assert!(value["root"]["initialized"].is_null());
    assert_eq!(value["status"], "busy");
}

#[test]
fn unknown_source_duration_is_not_replaced_with_measured_window_duration() {
    // FLAC STREAMINFO: 20 rate bits + 3 channel bits + 5 depth bits + 36 total-sample bits.
    // A zero 36-bit total means unknown; packet audio and its independent ten-second
    // synthetic source remain unchanged. Symphonia 0.6.1 reads this as no num_frames.
    let mut bytes = include_bytes!("../../app/tests/fixtures/recording/tone.flac").to_vec();
    assert_eq!(&bytes[..4], b"fLaC");
    assert_eq!(bytes[4] & 0x7f, 0);
    assert_eq!(&bytes[5..8], &[0, 0, 34]);
    bytes[21] &= 0xf0;
    bytes[22..26].fill(0);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unknown-total.flac");
    std::fs::write(&path, bytes).unwrap();
    let recording = services::analyze_recording(
        &path,
        auditorylab_app::recording::AnalysisOptions::default(),
        &auditorylab_app::job::Cancel::default(),
        &auditorylab_app::job::Progress::default(),
    )
    .unwrap();
    assert_eq!(recording.analysis.duration_seconds, None);
    let value = serde_json::to_value(Report::analyzed_recording(&recording)).unwrap();
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/report/unknown-recording.json")).unwrap();
    subset(&value, &expected);
    assert_eq!(
        value["recording"]["analysis"]["resolution_hz"],
        8000.0 / 65_536.0
    );
}
