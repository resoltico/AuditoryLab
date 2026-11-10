//! Independent final managed-copy fixtures; never call the production copy planner.
use auditorylab_core::{
    DocumentIdentity, DocumentUuid, Finite, Recipe, Timestamp,
    provenance::{
        BibliographySource, ConditionRole, Description, Evidence, EvidenceScope, Lineage, Measure,
        Population,
    },
};
pub(super) const SOURCE_TITLE: &str = "Copy boundary";
pub(super) const EDITED_TITLE: &str = "Copy boundary (edited)";
const NOTES: &str = "Quoted \"value\", slash \\, λ";
const ATTRIBUTION: &str = "Attributed copy; source findings concern the ancestor, not this sound. Ancestor notes: Quoted \"value\", slash \\, λ";

fn identity(last: u8) -> DocumentIdentity {
    let mut bytes = [0; 16];
    bytes[15] = last;
    DocumentIdentity {
        id: DocumentUuid::from_random_bytes(bytes),
        created_at: None,
        modified_at: None,
    }
}
fn source(padding: usize) -> Recipe {
    let mut b = Recipe::builder(identity(1));
    b.content.description = Some(Description {
        title: SOURCE_TITLE.into(),
        notes: Some(NOTES.into()),
    });
    b.content.timing.duration_seconds =
        Finite::new(60.0).unwrap_or_else(|| panic!("independent finite fixture is representable"));
    b.content.timing.fade_seconds =
        Finite::new(0.0).unwrap_or_else(|| panic!("independent finite fixture is representable"));
    b.content.sources = vec![BibliographySource {
        key: "study".into(),
        label: "Independent study".into(),
        citation: "Citation".into(),
        doi: None,
        url: Some("https://example.com/study".into()),
    }];
    b.content.evidence = vec![Evidence {
        source: "study".into(),
        scope: EvidenceScope::ReferenceCondition,
        population: Population::Human,
        population_detail: None,
        condition_role: ConditionRole::Test,
        measures: vec![Measure::Eeg],
        methods_locator: "Methods".into(),
        summary: "Reference findings".into(),
        limitations: "No efficacy claim".into(),
        adaptation: "Authored fixture".into(),
    }];
    b.content.lineage = (0..16)
        .map(|i| Lineage {
            title: format!("Ancestor {i}"),
            note: if i == 0 {
                format!("Padding {}", "x".repeat(padding))
            } else {
                "Retained ancestry".into()
            },
            document_id: None,
            catalog_id: None,
            source_keys: vec!["study".into()],
        })
        .collect();
    b.build()
        .unwrap_or_else(|error| panic!("invalid independent copy fixture: {error:?}"))
}
fn expected(source: &Recipe, title: &str) -> Recipe {
    let mut b = Recipe::builder(identity(2));
    let now: Timestamp = "1970-01-01T00:00:00.000000000Z"
        .parse()
        .unwrap_or_else(|error| panic!("invalid independent copy fixture: {error:?}"));
    b.document.created_at = Some(now);
    b.document.modified_at = Some(now);
    b.content = source.content().clone();
    b.content
        .description
        .as_mut()
        .unwrap_or_else(|| panic!("independent fixture has a description"))
        .title = title.into();
    b.content.evidence[0].scope = EvidenceScope::Ancestor;
    b.content.lineage.push(Lineage {
        title: SOURCE_TITLE.into(),
        note: ATTRIBUTION.into(),
        document_id: Some(source.document().id),
        catalog_id: None,
        source_keys: vec!["study".into()],
    });
    b.build()
        .unwrap_or_else(|error| panic!("invalid independent copy fixture: {error:?}"))
}
/// Fill one existing ancestor note, which occurs once in the final canonical document.
pub(super) fn source_for_final_size(title: &str, bytes: usize) -> Recipe {
    assert!([SOURCE_TITLE, EDITED_TITLE].contains(&title));
    let base = source(0);
    let fixed = expected(&base, title)
        .to_toml()
        .unwrap_or_else(|error| panic!("invalid independent copy fixture: {error:?}"))
        .len();
    let padding = bytes
        .checked_sub(fixed)
        .unwrap_or_else(|| panic!("requested fixture includes its fixed fields"));
    let source = source(padding);
    assert!(
        source
            .to_toml()
            .unwrap_or_else(|error| panic!("invalid independent copy fixture: {error:?}"))
            .len()
            < 65_536
    );
    assert_eq!(fixed + padding, bytes);
    if bytes == 65_536 {
        assert_eq!(
            expected(&source, title)
                .to_toml()
                .unwrap_or_else(|error| panic!("invalid independent copy fixture: {error:?}"))
                .len(),
            65_536
        );
    }
    source
}
pub(super) fn assert_final_copy(source: &Recipe, actual: &Recipe, title: &str) {
    assert_ne!(actual.document().id, source.document().id);
    assert!(actual.document().created_at.is_some());
    assert_eq!(actual.document().modified_at, actual.document().created_at);
    assert_eq!(
        actual
            .to_toml()
            .unwrap_or_else(|error| panic!("invalid independent copy fixture: {error:?}"))
            .len(),
        65_536
    );
    assert_eq!(actual.content(), expected(source, title).content());
}
