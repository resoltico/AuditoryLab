//! Independently specified schema-4 values at compound wire boundaries.
use auditorylab_app::{
    discovery::{
        collection::{CatalogQuery, DateRange},
        query::{Direction, GroupBy, RhythmQuery, RhythmState, Scope, SortBy},
    },
    job::Cancel,
    services::library::{MutationKind, MutationStatus, WriteOutcome},
    services::{self, LibraryContext, StorageWarning},
};
use auditorylab_core::{
    SampleRate,
    provenance::{ConditionRole, Population},
    rhythm::{Band, Completeness, Method},
};
use auditorylab_presentation::report::{ListingOperation, Report};
use serde_json::{Value, json};

#[expect(
    clippy::unwrap_used,
    reason = "A fixture directory failure must fail its calling test"
)]
fn context() -> (tempfile::TempDir, LibraryContext) {
    let directory = tempfile::tempdir().unwrap();
    let context = LibraryContext::resolve(Some(directory.path()));
    (directory, context)
}
#[expect(
    clippy::unwrap_used,
    reason = "Independent catalog fixture admission must fail its calling test"
)]
fn inspect_builtin(id: &str) -> Value {
    let (_directory, context) = context();
    let source = services::resolve(
        services::Source::Selector(format!("builtin:{id}").parse().unwrap()),
        &context,
    )
    .unwrap();
    serde_json::to_value(Report::inspect(&services::inspect(
        &source,
        SampleRate::DEFAULT,
    )))
    .unwrap()
}

#[test]
fn published_warning_preserves_its_code_parameters_and_explicit_absent_record() {
    let outcome = WriteOutcome {
        status: MutationStatus::PublishedWithWarning,
        key: "user:00000000-0000-4000-8000-000000000001".parse().unwrap(),
        kind: MutationKind::Purge,
        record: None,
        diagnostics: vec![StorageWarning::RemovedMemberships { count: 3 }],
    };
    let value = serde_json::to_value(Report::mutation(&outcome)).unwrap();
    assert_eq!(value["output_schema_version"], 4);
    assert_eq!(value["operation"], "library_purge");
    assert_eq!(value["status"], "published_with_warning");
    assert_eq!(value["version_token"], Value::Null);
    assert!(value.get("version_token").is_some());
    assert!(value.get("complete").is_none());
    assert_eq!(value["diagnostics"].as_array().unwrap().len(), 1);
    assert_eq!(
        value["diagnostics"][0]["code"],
        "storage.removed_memberships"
    );
    assert_eq!(value["diagnostics"][0]["params"], json!({"count":3}));
    assert_ne!(value["diagnostics"][0]["message"].as_str().unwrap(), "");
}

#[test]
fn compound_query_fields_have_independent_wire_names_and_values() {
    let (_directory, context) = context();
    let query = CatalogQuery {
        scope: Scope::BuiltIn,
        include_external: false,
        search: "specific query".into(),
        rhythm: RhythmQuery {
            bands: vec![Band::Alpha],
            methods: vec![Method::Binaural],
            state: Some(RhythmState::Multiple),
        },
        studies: vec!["key:ioannou-2015".into()],
        roles: vec![ConditionRole::Test],
        populations: vec![Population::Human],
        analysis: Some(Completeness::Complete),
        created: DateRange {
            from: Some("2020-01-01T00:00:00Z".parse().unwrap()),
            before: Some("2021-01-01T00:00:00Z".parse().unwrap()),
        },
        modified: DateRange {
            from: Some("2022-01-01T00:00:00Z".parse().unwrap()),
            before: Some("2023-01-01T00:00:00Z".parse().unwrap()),
        },
        added: DateRange {
            from: Some("2024-01-01T00:00:00Z".parse().unwrap()),
            before: Some("2025-01-01T00:00:00Z".parse().unwrap()),
        },
        sort: SortBy::Title,
        direction: Some(Direction::Desc),
        group_by: GroupBy::Band,
    };
    let listing = services::list(
        services::ListRequest {
            query,
            files: vec![],
        },
        &context,
        &Cancel::default(),
    )
    .unwrap();
    let value = serde_json::to_value(Report::listing(ListingOperation::List, &listing)).unwrap();
    assert_eq!(
        value["query"],
        json!({
            "scope":"builtin", "include_external":false, "search":"specific query",
            "rhythm":{"bands":["alpha"],"methods":["binaural"],"state":"multiple"},
            "studies":["key:ioannou-2015"], "roles":["test"], "populations":["human"], "analysis":"complete",
            "created":{"from":"2020-01-01T00:00:00.000000000Z","before":"2021-01-01T00:00:00.000000000Z"},
            "modified":{"from":"2022-01-01T00:00:00.000000000Z","before":"2023-01-01T00:00:00.000000000Z"},
            "added":{"from":"2024-01-01T00:00:00.000000000Z","before":"2025-01-01T00:00:00.000000000Z"},
            "sort":"title", "direction":"desc", "group_by":"band"
        })
    );
    assert_eq!(value["effective_sort"], json!(["title", "desc"]));
}

#[test]
fn catalog_evidence_and_pair_lane_facts_keep_distinct_source_and_generator_values() {
    let value = inspect_builtin("ioannou-2015-binaural-10-hz");
    assert_eq!(
        value["entries"][0]["evidence"],
        json!([{
            "source":"ioannou-2015", "doi":"10.1371/journal.pone.0129486",
            "scope":"reference_condition", "population":"human", "role":"test", "measures":["eeg"]
        }])
    );
    let validation = &value["entries"][0]["validation"];
    assert_eq!(validation["status"], "valid");
    assert_eq!(validation["frames"], 2_880_000);
    let voices = validation["voices"].as_array().unwrap();
    assert_eq!(voices.len(), 2);
    for (voice, lane, route, hz) in [
        (&voices[0], "primary", "left", 200.0),
        (&voices[1], "secondary", "right", 210.0),
    ] {
        assert_eq!(voice["id"], json!({"component":1,"lane":lane}));
        let facts = &voice["facts"];
        assert_eq!(facts["route"], route);
        assert_eq!(facts["peak_dbfs"], -20.0);
        assert_eq!(facts["carrier"]["kind"], "tone");
        assert_eq!(facts["carrier"]["requested_hz"], hz);
        assert!((facts["carrier"]["realized_hz"].as_f64().unwrap() - hz).abs() < 1e-9);
        assert_eq!(facts["modulation"], Value::Null);
        assert_eq!(facts["pair"]["owner"], 1);
        assert_eq!(facts["pair"]["kind"], "binaural");
        assert_eq!(facts["pair"]["first_hz"], 200.0);
        assert_eq!(facts["pair"]["offset_hz"], 10.0);
        assert_eq!(facts["pair"]["second_hz"], 210.0);
        assert!((facts["pair"]["realized_offset_hz"].as_f64().unwrap() - 10.0).abs() < 1e-9);
    }
}

#[test]
fn pulsed_modulation_preserves_duty_shape_and_resolved_window_fields() {
    let (_directory, context) = context();
    let authored = r#"schema_version = 5
[document]
id = "00000000-0000-4000-8000-000000000001"
[description]
title = "Shaped modulation"
[timing]
duration_seconds = 2.0
fade_seconds = 0.0
[[sound.component]]
id = 1
[sound.component.source]
kind = "tone"
route = "both"
frequency_hz = 100.0
peak_dbfs = -20.0
[sound.component.modulation]
kind = "pulsed"
[sound.component.modulation.pulse]
rate_hz = 10.0
[sound.component.modulation.pulse.width]
kind = "duty_cycle"
fraction = 0.5
[sound.component.modulation.pulse.shape]
kind = "raised_cosine"
rise_seconds = 0.001
fall_seconds = 0.002
"#;
    let mut reader = std::io::Cursor::new(authored);
    let source = services::resolve(
        services::Source::Stream {
            reader: &mut reader,
            label: "independent pulse",
        },
        &context,
    )
    .unwrap();
    let value = serde_json::to_value(Report::inspect(&services::inspect(
        &source,
        SampleRate::DEFAULT,
    )))
    .unwrap();
    assert_eq!(
        value["entries"][0]["validation"]["voices"][0]["facts"]["modulation"],
        json!({
            "kind":"pulsed", "rate_hz":10.0, "requested_width_seconds":0.05, "width_frames":2400,
            "sample_rate":48000, "edge_frames":[48,96],
            "width":{"kind":"duty_cycle","fraction":0.5},
            "shape":{"kind":"raised_cosine","rise_seconds":0.001,"fall_seconds":0.002}
        })
    );
}
