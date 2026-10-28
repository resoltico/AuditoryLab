//! Checked public vocabulary and CLI query reference, without a generator command.
use auditorylab_app::discovery::query::{
    CalendarField, Direction, GroupBy, RhythmState, Scope, SortBy,
};
use auditorylab_app::vocabulary::{Operation, Status};
use auditorylab_core::rhythm::{Band, Completeness, Method};
use auditorylab_core::vocabulary::Vocabulary;
use auditorylab_core::{
    CarrierKind, ConditionRole, Ear, Ears, EvidenceScope, Measure, ModulationKind, PairKind,
    ParameterId, Population, SlotRole,
};
use std::collections::{BTreeMap, BTreeSet};
fn keys<T: Vocabulary>() -> Vec<String> {
    T::ALL.iter().map(|value| value.key().to_owned()).collect()
}
#[expect(
    clippy::expect_used,
    reason = "Malformed reference markup must fail the contract test instead of omitting a documented key"
)]
fn table(document: &str, marker: &str, header: &str) -> BTreeMap<String, Vec<String>> {
    let begin = format!("<!-- {marker}:begin -->");
    let end = format!("<!-- {marker}:end -->");
    let body = document
        .split_once(&begin)
        .expect("reference marker")
        .1
        .split_once(&end)
        .expect("reference end")
        .0;
    let mut rows = BTreeMap::new();
    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line == header || line == "| --- | --- |" {
            continue;
        }
        let cells = line
            .strip_prefix("| ")
            .and_then(|line| line.strip_suffix(" |"))
            .expect("two-column reference row")
            .split(" | ")
            .collect::<Vec<_>>();
        assert_eq!(cells.len(), 2);
        let name = cells[0]
            .strip_prefix('`')
            .and_then(|name| name.strip_suffix('`'))
            .expect("reference key");
        let values = cells[1]
            .split(", ")
            .map(|value| {
                value
                    .strip_prefix('`')
                    .and_then(|value| value.strip_suffix('`'))
                    .expect("quoted value")
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert!(
            rows.insert(name.to_owned(), values).is_none(),
            "duplicate reference row {name}"
        );
    }
    assert!(!rows.is_empty());
    rows
}
#[test]
fn vocabulary_and_query_tables_match_every_owned_key() {
    let mut expected = BTreeMap::new();
    macro_rules! owner {($($kind:ty),+)=>{$(assert!(expected.insert(auditorylab_core::Code::code(&"".parse::<$kind>().expect_err("empty input is outside every vocabulary")).strip_prefix("vocabulary.").unwrap().to_owned(),keys::<$kind>()).is_none());)+};}
    owner!(
        Scope,
        SortBy,
        Direction,
        GroupBy,
        RhythmState,
        CalendarField,
        Band,
        Method,
        Completeness,
        Population,
        ConditionRole,
        EvidenceScope,
        Measure,
        CarrierKind,
        ModulationKind,
        PairKind,
        ParameterId,
        Ears,
        Ear,
        SlotRole,
        Operation,
        Status
    );
    let declared = auditorylab_core::diagnostic::OWNERS
        .iter()
        .chain(auditorylab_app::diagnostic::OWNERS)
        .flat_map(|owner| owner.iter())
        .filter_map(|code| code.strip_prefix("vocabulary."))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        expected.keys().cloned().collect::<BTreeSet<_>>(),
        declared,
        "all vocabulary owners must participate"
    );
    let document = include_str!("../../../docs/formats.md");
    assert_eq!(
        table(document, "vocabulary-keys", "| Vocabulary | Keys |"),
        expected
    );
    let options = BTreeMap::from([
        (
            "--collection".into(),
            [Scope::All, Scope::BuiltIn, Scope::User]
                .into_iter()
                .map(|value| value.key().to_owned())
                .collect(),
        ),
        ("--band".into(), keys::<Band>()),
        ("--method".into(), keys::<Method>()),
        ("--rhythms".into(), keys::<RhythmState>()),
        ("--population".into(), keys::<Population>()),
        ("--role".into(), keys::<ConditionRole>()),
        ("--analysis".into(), keys::<Completeness>()),
        (
            "--sort".into(),
            SortBy::ALL
                .into_iter()
                .filter(|sort| *sort != SortBy::Automatic)
                .map(|sort| sort.key().to_owned())
                .collect(),
        ),
        ("--direction".into(), keys::<Direction>()),
        ("--group-by".into(), keys::<GroupBy>()),
    ]);
    assert_eq!(
        table(document, "query-options", "| Option | Values |"),
        options
    );
}
