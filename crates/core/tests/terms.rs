//! Public domain vocabulary follows the glossary in both grammatical forms.
use auditorylab_core::{
    CarrierKind, ConditionRole, Ear, Ears, EvidenceScope, Measure, ModulationKind, PairKind,
    ParameterId, Population,
    rhythm::{Band, Completeness, Method},
};

#[test]
fn selectable_terms_expose_ordered_labels_and_running_text() {
    assert_eq!(
        CarrierKind::ALL.map(|v| (v.label(), v.phrase())),
        [("Tone", "tone"), ("Clicks", "clicks")]
    );
    assert_eq!(
        ModulationKind::ALL.map(|v| (v.label(), v.phrase())),
        [("AM", "AM"), ("Gate", "gate")]
    );
    assert_eq!(
        PairKind::ALL.map(|v| (v.label(), v.phrase())),
        [
            ("Binaural pair", "binaural pair"),
            ("Monaural pair", "monaural pair")
        ]
    );
    assert_eq!(
        Method::ALL.map(|v| (v.label(), v.phrase())),
        [
            ("Binaural", "binaural"),
            ("Monaural", "monaural"),
            ("Clicks", "clicks"),
            ("Gate", "gate"),
            ("AM", "AM"),
            ("Steady tone", "steady tone")
        ]
    );
    assert_eq!(
        Population::ALL.map(|v| v.phrase()),
        [
            "human",
            "mouse",
            "other animal",
            "mixed population",
            "not applicable",
            "unspecified population"
        ]
    );
    assert_eq!(
        ConditionRole::ALL.map(|v| v.phrase()),
        [
            "test condition",
            "comparison condition",
            "mixed roles",
            "unspecified role"
        ]
    );
    assert_eq!(
        Band::ALL.map(|v| (v.label(), v.phrase())),
        [
            ("Delta", "Delta"),
            ("Theta", "Theta"),
            ("Alpha", "Alpha"),
            ("Beta", "Beta"),
            ("Gamma", "Gamma")
        ]
    );
    assert_eq!(
        Ears::ALL.map(|v| v.phrase()),
        ["both ears", "left ear", "right ear"]
    );
    assert_eq!(Ear::ALL.map(|v| v.phrase()), ["left ear", "right ear"]);
    assert_eq!(
        EvidenceScope::ALL.map(|v| v.phrase()),
        ["reference condition", "ancestor"]
    );
    assert_eq!(
        Measure::ALL.map(|v| v.phrase()),
        [
            "EEG",
            "behavior",
            "subjective report",
            "sleep",
            "hearing",
            "other"
        ]
    );
    assert_eq!(
        Completeness::ALL.map(|v| v.phrase()),
        ["complete", "invalid"]
    );
    for parameter in ParameterId::ALL {
        assert_eq!(parameter.label(), parameter.descriptor().label);
        assert!(!parameter.phrase().is_empty());
    }
}
