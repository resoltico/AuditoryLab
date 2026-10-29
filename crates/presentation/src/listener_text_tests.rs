//! Listener-facing text: each fact once, plain labels, and purpose in every list.
use auditorylab_app::discovery::collection::EntrySummary;
use auditorylab_app::discovery::query::SortBy;
use auditorylab_app::document::origin::LibraryKey;
use auditorylab_app::document::recipe::RecipeSnapshot;
use auditorylab_core::SampleRate;

fn clicks() -> RecipeSnapshot {
    auditorylab_app::services::catalog()
        .unwrap()
        .into_iter()
        .find(|s| s.title() == "40 Hz clicks")
        .unwrap()
}

#[test]
fn description_states_each_research_fact_once_with_plain_labels() {
    let text = crate::snapshot_description(&clicks());
    for label in [
        "Studied for:",
        "Summary:",
        "Measured:",
        "Study:",
        "Adaptation:",
        "Citation:",
        "Link:",
    ] {
        assert_eq!(
            text.matches(label).count(),
            1,
            "{label} appears once:\n{text}"
        );
    }
    assert!(text.contains("Recipe: builtin:chan-2022-clicks-40-hz"));
    assert!(text.contains("Measured: EEG, Behavior, Other"));
    for removed in [
        "Source citation",
        "Source DOI",
        "Population detail",
        "Evidence Reference",
        "Limitations:",
        "Document:",
        "Unknown",
        "Source chan-2022",
    ] {
        assert!(!text.contains(removed), "{removed} is gone:\n{text}");
    }
}

#[test]
fn inspection_and_technical_details_leave_research_to_its_own_view() {
    let snapshot = clicks();
    let inspection = crate::rendition_inspection(&snapshot.at(SampleRate::DEFAULT).unwrap());
    let details = crate::snapshot_details_at(&snapshot, SampleRate::DEFAULT);
    for text in [&inspection, &details] {
        assert!(
            !text.contains("Summary:") && !text.contains("Citation:"),
            "{text}"
        );
        assert!(
            text.contains("Recipe: builtin:chan-2022-clicks-40-hz"),
            "{text}"
        );
    }
    assert!(inspection.contains("Peak level: left -20.0 dBFS · right -20.0 dBFS"));
}

#[test]
fn rows_say_what_a_sound_was_studied_for_and_hide_unknown_built_in_dates() {
    let snapshot = clicks();
    let entry = EntrySummary::from_snapshot(
        LibraryKey::BuiltIn(
            auditorylab_core::catalog::CatalogId::new("chan-2022-clicks-40-hz").unwrap(),
        ),
        &snapshot,
        None,
    );
    let row = crate::library::entry_summary_list_description(&entry, SortBy::Rate);
    assert!(
        row.contains("\n  Human study: Alzheimer's disease research\n"),
        "{row}"
    );
    assert!(
        !row.contains("Created"),
        "built-in rows omit unknown dates: {row}"
    );
    let dated = crate::library::entry_summary_list_description(&entry, SortBy::Created);
    assert!(
        dated.contains("Created Unknown"),
        "a date sort explains the order: {dated}"
    );

    let mut own = entry.clone();
    own.key = LibraryKey::User(snapshot.recipe().document().id);
    own.evidence.clear();
    let own_row = crate::library::entry_summary_list_description(&own, SortBy::Rate);
    assert!(own_row.contains("\n  Custom recipe\n") && own_row.contains("Created"));
}

#[test]
fn levels_sizes_and_zones_read_naturally() {
    assert_eq!(
        crate::numbers::peak_level([f64::NEG_INFINITY; 2]),
        "Peak level: silent"
    );
    assert_eq!(
        crate::numbers::peak_level([-20.0, f64::NEG_INFINITY]),
        "Peak level: left -20.0 dBFS · right −∞ dBFS"
    );
    assert_eq!(crate::numbers::file_size(999), "999 bytes");
    assert_eq!(crate::numbers::file_size(15_360_058), "15.4 MB");
    assert_eq!(crate::numbers::file_size(1_382_400_058), "1.38 GB");
    assert_eq!(
        crate::library::calendar::zone_label("+02:00", "+02:00"),
        "+02:00"
    );
    assert_eq!(
        crate::library::calendar::zone_label("EET", "+02:00"),
        "EET (+02:00)"
    );
}

#[test]
fn guide_links_follow_the_build() {
    let mut identity = auditorylab_app::identity::BUILD_IDENTITY;
    identity.version = "1.2.3";
    identity.revision = None;
    assert!(
        identity
            .document_url("docs/desktop.md")
            .ends_with("/blob/v1.2.3/docs/desktop.md")
    );
    identity.revision = Some("0123456789abcdef0123456789abcdef01234567");
    assert!(
        identity
            .document_url("docs/desktop.md")
            .ends_with("/blob/0123456789abcdef0123456789abcdef01234567/docs/desktop.md")
    );
}

#[test]
fn list_purpose_text_stays_out_of_the_machine_report() {
    let snapshot = clicks();
    let entry = EntrySummary::from_snapshot(
        LibraryKey::BuiltIn(auditorylab_core::catalog::CatalogId::new("x").unwrap()),
        &snapshot,
        None,
    );
    assert!(entry.evidence[0].population_detail.is_some());
    let json = serde_json::to_string(&entry.evidence).unwrap();
    assert!(!json.contains("population_detail"), "JSON schema 2 is unchanged: {json}");
}
