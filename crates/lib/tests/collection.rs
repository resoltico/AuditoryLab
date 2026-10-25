//! Shared query counterexamples using actual authored classification and typed evidence.
use auditorylab::{
    Carrier, Component, ComponentId, DocumentIdentity, Ears, Node, Origin, Pair, Recipe,
    RecipeSnapshot, Sound, Timestamp, Voice,
    collection::{CatalogQuery, DateRange, Direction, EntrySummary, GroupBy, Scope, SortBy},
    library::LibraryKey,
    nominal::{Band, Method},
};
#[expect(
    clippy::unwrap_used,
    reason = "Curated UUID/time fixture construction must fail rather than silently substitute data"
)]
fn entry(id: u32, title: &str, nodes: Vec<Component>, modified: Option<&str>) -> EntrySummary {
    let document = DocumentIdentity {
        id: format!("00000000-0000-4000-8000-{id:012}").parse().unwrap(),
        created_at: None,
        modified_at: modified.map(|s| s.parse().unwrap()),
    };
    let mut recipe = Recipe::silent(document);
    recipe.description = Some(auditorylab::Description {
        title: title.into(),
        notes: None,
    });
    recipe.stimulus = Sound {
        duration_seconds: 60.0,
        fade_seconds: 0.1,
        components: nodes,
    };
    let key = LibraryKey::User(recipe.document.id);
    let snapshot = RecipeSnapshot::new(Origin::Stream("fixture".into()), recipe);
    EntrySummary::from_snapshot(key, &snapshot, None)
}
fn pair(offset_hz: f64) -> Component {
    Component {
        id: ComponentId::new(7),
        node: Node::BinauralPair(Pair {
            first_hz: 200.0,
            offset_hz,
            first_peak_dbfs: -30.0,
            second_peak_dbfs: -30.0,
        }),
    }
}
fn am(rate_hz: f64) -> Component {
    Component {
        id: ComponentId::new(90),
        node: Node::Voice(Voice {
            ears: Ears::Both,
            peak_dbfs: -30.0,
            carrier: Carrier::Tone {
                frequency_hz: 1000.0,
            },
            modulation: Some(auditorylab::Modulation::Sinusoidal {
                rate_hz,
                depth: 1.0,
            }),
        }),
    }
}
#[test]
fn correlated_rate_sort_and_groups_use_unique_actual_authored_keys() {
    let mixed = entry(1, "Mixed", vec![pair(2.0), am(40.0)], None);
    let lower = entry(2, "Lower gamma", vec![am(35.0)], None);
    let input = vec![mixed.clone(), lower, mixed];
    let mut q = CatalogQuery::default();
    q.rhythm.bands = vec![Band::Gamma];
    q.rhythm.methods = vec![Method::Binaural];
    assert_eq!(q.execute(&input).unwrap().unique_count(), 0);
    q.rhythm.methods = vec![Method::AmplitudeModulation];
    q.group_by = GroupBy::Band;
    let result = q.execute(&input).unwrap();
    assert_eq!(result.unique_count(), 2);
    assert_eq!(result.entries[0].title, "Lower gamma");
    assert_eq!(
        result.entries[1].title, "Mixed",
        "Gamma-filtered sorting must use 40, not the unrelated 2 Hz process"
    );
    assert!(
        result
            .groups
            .iter()
            .any(|g| g.key == "delta" && g.keys == vec![input[0].key.clone()])
    );
    assert_eq!(
        result
            .groups
            .iter()
            .find(|g| g.key == "gamma")
            .unwrap()
            .keys
            .len(),
        2
    );
    let counts = q.facet_counts(&input);
    assert_eq!(
        counts
            .bands
            .iter()
            .find(|(b, _)| *b == Band::Gamma)
            .unwrap()
            .1,
        2
    );
    assert_eq!(
        counts
            .methods
            .iter()
            .find(|(m, _)| *m == Method::Binaural)
            .unwrap()
            .1,
        0,
        "method alternatives retain the Gamma restriction and same-descriptor correlation"
    );
}
#[test]
fn evidence_filters_cannot_borrow_population_or_role_and_ancestors_never_assert_current_study() {
    use auditorylab_core::{
        BibliographySource, ConditionRole as R, Evidence, EvidenceScope as S, Population as P,
    };
    let mut recipe = auditorylab::Draft::new().unwrap().recipe();
    recipe.sources = (0..3)
        .map(|i| BibliographySource {
            key: format!("study-{i}"),
            label: format!("Study {i}"),
            citation: "declared citation".into(),
            doi: Some(format!("10.1000/study-{i}")),
            url: None,
        })
        .collect();
    recipe.evidence = [
        (0, S::ReferenceCondition, P::Human, R::Test),
        (1, S::ReferenceCondition, P::Mouse, R::Comparison),
        (2, S::Ancestor, P::Human, R::Test),
    ]
    .into_iter()
    .map(|(n, scope, population, condition_role)| Evidence {
        source: format!("study-{n}"),
        scope,
        population,
        condition_role,
        population_detail: None,
        measures: Vec::new(),
        methods_locator: "Methods".into(),
        summary: if n == 2 {
            "unique ancestor phrase".into()
        } else {
            "source findings".into()
        },
        limitations: "not validation".into(),
        adaptation: "declared adaptation".into(),
    })
    .collect();
    recipe.validate().unwrap();
    let key = LibraryKey::User(recipe.document.id);
    let snapshot = RecipeSnapshot::new(Origin::Stream("fixture".into()), recipe);
    let input = [EntrySummary::from_snapshot(key, &snapshot, None)];
    let mut q = CatalogQuery {
        studies: vec!["10.1000/study-0".into()],
        roles: vec![R::Test],
        populations: vec![P::Mouse],
        ..Default::default()
    };
    assert_eq!(q.execute(&input).unwrap().unique_count(), 0);
    q.populations = vec![P::Human];
    assert_eq!(q.execute(&input).unwrap().unique_count(), 1);
    q.studies = vec!["study-2".into()];
    assert_eq!(q.execute(&input).unwrap().unique_count(), 0);
    q.studies.clear();
    q.roles.clear();
    q.populations.clear();
    q.search = "unique ancestor phrase".into();
    assert_eq!(q.execute(&input).unwrap().unique_count(), 1);
}
#[test]
fn date_nulls_stay_last_in_both_directions_and_ties_do_not_reverse() {
    let rows = vec![
        entry(1, "z", Vec::new(), Some("2026-10-24T06:00:00.123456789Z")),
        entry(
            2,
            "a",
            Vec::new(),
            Some("2026-10-24T09:00:00.123456789+03:00"),
        ),
        entry(3, "Unknown", Vec::new(), None),
        entry(
            4,
            "Earlier",
            Vec::new(),
            Some("2026-10-24T06:00:00.123456788Z"),
        ),
    ];
    let mut q = CatalogQuery {
        scope: Scope::User,
        sort: SortBy::Modified,
        ..Default::default()
    };
    for direction in [Direction::Asc, Direction::Desc] {
        q.direction = Some(direction);
        let result = q.execute(&rows).unwrap();
        assert_eq!(result.entries.last().unwrap().title, "Unknown");
        let same = result
            .entries
            .iter()
            .filter(|e| matches!(e.title.as_str(), "a" | "z"))
            .map(|e| e.title.as_str())
            .collect::<Vec<_>>();
        assert_eq!(same, ["a", "z"]);
    }
    let from: Timestamp = "2026-10-24T06:00:00.123456789Z".parse().unwrap();
    q.modified = DateRange {
        from: Some(from),
        before: Some("2026-10-24T06:00:00.123456790Z".parse().unwrap()),
    };
    assert_eq!(q.execute(&rows).unwrap().unique_count(), 2);
    q.modified.before = Some(from);
    assert!(q.execute(&rows).is_err());
}
#[test]
fn automatic_defaults_follow_scope_but_explicit_sort_is_retained() {
    let mut q = CatalogQuery::default();
    assert_eq!(q.effective_sort(), (SortBy::Rate, Direction::Asc));
    q.scope = Scope::User;
    assert_eq!(q.effective_sort(), (SortBy::Modified, Direction::Desc));
    q.sort = SortBy::Title;
    assert_eq!(q.effective_sort(), (SortBy::Title, Direction::Asc));
    q.scope = Scope::BuiltIn;
    assert_eq!(q.effective_sort(), (SortBy::Title, Direction::Asc));
    let mut external = entry(4, "temporary", Vec::new(), None);
    external.key = LibraryKey::External(auditorylab::library::ExternalId::allocate().unwrap());
    let mut trashed = entry(5, "trash", Vec::new(), None);
    trashed.trashed_at = Some("2026-10-24T06:00:00Z".parse().unwrap());
    let active = entry(6, "active", Vec::new(), None);
    let input = [external, trashed, active];
    let mut all = CatalogQuery::default();
    assert_eq!(all.execute(&input).unwrap().unique_count(), 1);
    all.include_external = true;
    assert_eq!(all.execute(&input).unwrap().unique_count(), 2);
    all.scope = Scope::Trash;
    assert_eq!(all.execute(&input).unwrap().unique_count(), 1);
}
