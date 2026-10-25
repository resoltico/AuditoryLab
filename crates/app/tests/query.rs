//! Shared search and correlated query behavior for built-in and external origins.
use auditorylab::query;
use auditorylab::recipe::{Origin, RecipeSnapshot};
#[test]
fn normalized_text_terms_are_anded_and_numeric_terms_are_not_substrings() {
    let mut recipe = auditorylab::editing::Draft::new().unwrap().recipe();
    recipe.stimulus = auditorylab::core::Sound {
        duration_seconds: 60.0,
        fade_seconds: 0.1,
        components: (vec![auditorylab::core::Voice {
            ears: auditorylab::core::Ears::Both,
            peak_dbfs: -30.0,
            carrier: auditorylab::core::Carrier::Tone {
                frequency_hz: 200.0,
            },
            modulation: Some(auditorylab::core::Modulation::Sinusoidal {
                rate_hz: 20.0,
                depth: 1.0,
            }),
        }])
        .into_iter()
        .enumerate()
        .map(|(index, voice)| auditorylab_core::Component {
            id: auditorylab_core::ComponentId::numbered(index).unwrap(),
            node: auditorylab_core::Node::Voice(voice),
        })
        .collect(),
    };
    recipe.description = Some(auditorylab::core::Description {
        title: "Rīga reference 2015".into(),
        notes: Some("two words".into()),
    });
    recipe.document.id = "20abcdef-0000-4000-8000-000000000001".parse().unwrap();
    let snapshot = RecipeSnapshot::new(Origin::Stream("external".into()), recipe);
    for search in [
        "RĪGA",
        "Ri\u{304}ga",
        "two words",
        "20",
        "20.0",
        "rīga 20",
        "2015",
    ] {
        assert!(query::search(&snapshot, search), "{search}");
    }
    assert!(query::search(
        &snapshot,
        "20abcdef-0000-4000-8000-000000000001"
    ));
    for search in ["2", "201", "two absent", "rīga 19"] {
        assert!(!query::search(&snapshot, search), "{search}");
    }
    let mut steady = snapshot.recipe().clone();
    if let auditorylab::core::Node::Voice(c) = &mut steady.stimulus.components[0].node {
        c.modulation = None;
    }
    let steady = RecipeSnapshot::new(Origin::Stream("steady".into()), steady);
    assert!(
        !query::search(&steady, "20"),
        "200 Hz and 2015 must not match 20"
    );
}
