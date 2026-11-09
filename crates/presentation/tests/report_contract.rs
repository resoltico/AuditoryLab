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

fn subset(actual: &Value, expected: &Value) {
    match expected {
        Value::Object(fields) => {
            for (key, value) in fields {
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
        (Outcome::Write, "publication_failed"),
    ] {
        let report = serde_json::to_value(Report::error(
            Operation::AnalyzeRecording,
            &SampleRateError { hz: 0 },
            "Cancelled",
            outcome,
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
        json!({"hz":0,"min_hz":1,"max_hz":768000})
    );
    let human = auditorylab_presentation::display_text(authored);
    assert!(!human.contains('\u{202e}'));
}
