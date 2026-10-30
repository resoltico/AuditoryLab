//! Shared search and correlated query behavior for built-in and external origins.

use auditorylab_app::document::recipe::{Origin, RecipeSnapshot};
#[expect(
    clippy::unwrap_used,
    reason = "Controlled search fixture has no invalid date ranges"
)]
fn matches_search(snapshot: &RecipeSnapshot, search: &str) -> bool {
    let entry = auditorylab_app::discovery::collection::EntrySummary::from_snapshot(
        auditorylab_app::document::origin::LibraryKey::BuiltIn(
            auditorylab_core::catalog::CatalogId::new("fixture").unwrap(),
        ),
        snapshot,
        None,
    );
    !auditorylab_app::discovery::collection::CatalogQuery {
        search: search.into(),
        ..Default::default()
    }
    .execute(&[entry])
    .unwrap()
    .entries
    .is_empty()
}
#[test]
fn normalized_text_terms_are_anded_and_numeric_terms_are_not_substrings() {
    let mut recipe = auditorylab_app::document::editing::Draft::new()
        .unwrap()
        .recipe()
        .unwrap()
        .into_builder();
    recipe.content.sound = {
        let fixture = auditorylab_app::core::test_support::FiniteFixture::new(
            finite(60.0),
            finite(0.1),
            (vec![auditorylab_app::core::ComponentForm {
                source: auditorylab_app::core::SoundSource::Tone {
                    frequency_hz: finite(200.0),
                    route: auditorylab_app::core::ChannelRouting::Both,
                    peak_dbfs: finite(-30.0),
                },
                modulation: Some(auditorylab_app::core::Modulation::Sinusoidal {
                    rate_hz: finite(20.0),
                    depth: finite(1.0),
                }),
            }])
            .into_iter()
            .enumerate()
            .map(|(index, voice)| auditorylab_core::Component {
                id: auditorylab_core::ComponentId::numbered(index).unwrap(),
                form: voice,
            })
            .collect(),
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
        recipe.content.timing = fixture.timing;
        fixture.sound
    };
    recipe.content.description = Some(auditorylab_app::core::provenance::Description {
        title: "Rīga reference 2015".into(),
        notes: Some("two words".into()),
    });
    recipe.document.id = "20abcdef-0000-4000-8000-000000000001".parse().unwrap();
    let snapshot = RecipeSnapshot::new(Origin::Stream("external".into()), recipe.build().unwrap());
    for search in [
        "RĪGA",
        "Ri\u{304}ga",
        "two words",
        "20",
        "20.0",
        "rīga 20",
        "2015",
    ] {
        assert!(matches_search(&snapshot, search), "{search}");
    }
    assert!(matches_search(
        &snapshot,
        "20abcdef-0000-4000-8000-000000000001"
    ));
    for search in ["2", "201", "two absent", "rīga 19"] {
        assert!(!matches_search(&snapshot, search), "{search}");
    }
    let mut steady = snapshot.recipe().clone().into_builder();
    let component = steady.content.sound.components()[0];
    let mut form = component.form;
    form.set_modulation(None);
    steady.content.sound.replace(component.id, form).unwrap();
    let steady = RecipeSnapshot::new(Origin::Stream("steady".into()), steady.build().unwrap());
    assert!(
        !matches_search(&steady, "20"),
        "200 Hz and 2015 must not match 20"
    );
}

#[test]
fn search_excludes_generated_labels_and_unknown_date_captions() {
    use auditorylab_app::core::{
        ChannelRouting, Component, ComponentForm, ComponentId, provenance::Description,
    };
    let mut recipe = auditorylab_app::document::editing::Draft::new()
        .unwrap()
        .recipe()
        .unwrap()
        .into_builder();
    recipe.document.created_at = None;
    recipe.document.modified_at = None;
    recipe.content.description = Some(Description {
        title: "Reference".into(),
        notes: None,
    });
    recipe.content.sound = {
        let fixture = auditorylab_app::core::test_support::FiniteFixture::new(
            recipe.content.timing.duration_seconds,
            recipe.content.timing.fade_seconds,
            vec![Component {
                id: ComponentId::new(1).unwrap(),
                form: ComponentForm {
                    source: auditorylab_app::core::SoundSource::Tone {
                        frequency_hz: finite(200.0),
                        route: ChannelRouting::Both,
                        peak_dbfs: finite(-30.0),
                    },
                    modulation: None,
                },
            }],
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
        recipe.content.timing = fixture.timing;
        fixture.sound
    };
    let snapshot = RecipeSnapshot::new(Origin::Stream("fixture".into()), recipe.build().unwrap());
    // These captions are deliberately present in display output, not authored facts.
    assert!(auditorylab_presentation::snapshot_description(&(snapshot)).contains("Document:"));
    assert!(
        auditorylab_presentation::analysis_summary(snapshot.nominal()).contains("No rhythmic rate")
    );
    for caption in ["unknown", "document", "rhythmic"] {
        assert!(
            !matches_search(&snapshot, caption),
            "display-only caption {caption}"
        );
    }
    assert!(matches_search(&snapshot, "steady_tone 200"));
}

const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

#[test]
fn schema_three_group_labels_preserve_human_captions_and_machine_identity() {
    use auditorylab_app::discovery::collection::{Collection, GroupLabel, ResultGroup};
    use auditorylab_app::discovery::query::RhythmState;
    use auditorylab_core::rhythm::{Band, Method};
    for (label, expected) in [
        (GroupLabel::Band(Band::Gamma), "Gamma"),
        (GroupLabel::Method(Method::Sinusoidal), "Sinusoidal AM"),
        (GroupLabel::State(RhythmState::None), "No rhythmic rate"),
        (GroupLabel::NoMethod, "No declared method"),
        (GroupLabel::Study("10.1234/study".into()), "10.1234/study"),
        (GroupLabel::NoStudy, "No current study declaration"),
        (GroupLabel::Collection(Collection::BuiltIn), "Built-in"),
        (GroupLabel::Collection(Collection::User), "User"),
        (GroupLabel::Collection(Collection::External), "Open files"),
    ] {
        let group = ResultGroup {
            key: "independent-machine-key".into(),
            label,
            keys: Vec::new(),
        };
        let value =
            serde_json::to_value(auditorylab_presentation::report::GroupReport::from(&group))
                .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "key": "independent-machine-key",
                "label": expected,
                "keys": []
            })
        );
    }
}
