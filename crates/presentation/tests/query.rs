//! Shared search and correlated query behavior for built-in and external origins.
use auditorylab_presentation::AudioSummary;
use auditorylab_presentation::*;

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
    recipe.content.sound = auditorylab_app::core::Sound::new(
        finite(60.0),
        finite(0.1),
        (vec![auditorylab_app::core::ComponentForm::Tone {
            frequency_hz: finite(200.0),
            ears: auditorylab_app::core::Ears::Both,
            peak_dbfs: finite(-30.0),
            modulation: Some(auditorylab_app::core::Modulation::Am {
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
    recipe.content.description = Some(auditorylab_app::core::Description {
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
    use auditorylab_app::core::{Component, ComponentForm, ComponentId, Description, Ears};
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
    recipe.content.sound = auditorylab_core::Sound::new(
        recipe.content.sound.duration_seconds(),
        recipe.content.sound.fade_seconds(),
        vec![Component {
            id: ComponentId::new(1).unwrap(),
            form: ComponentForm::Tone {
                frequency_hz: finite(200.0),
                ears: Ears::Both,
                peak_dbfs: finite(-30.0),
                modulation: None,
            },
        }],
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
    let snapshot = RecipeSnapshot::new(Origin::Stream("fixture".into()), recipe.build().unwrap());
    // These captions are deliberately present in display output, not authored facts.
    assert!(snapshot.description().contains("Unknown"));
    assert!((snapshot.nominal()).summary().contains("No rhythmic rate"));
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
