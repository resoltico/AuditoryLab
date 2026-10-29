//! Public domain vocabulary follows the glossary in both grammatical forms.
use auditorylab_core::{
    CarrierKind, Channel, ChannelRouting, ModulationKind, PairKind, ParameterId, SlotRole,
    provenance::ConditionRole,
    provenance::EvidenceScope,
    provenance::Measure,
    provenance::Population,
    rhythm::{Band, Completeness, Method},
};

#[test]
fn selectable_terms_expose_ordered_labels_and_running_text() {
    assert_eq!(
        CarrierKind::ALL.map(|v| (v.label(), v.phrase())),
        [("Tone", "tone"), ("Pulse train", "pulse train")]
    );
    assert_eq!(
        ModulationKind::ALL.map(|v| (v.label(), v.phrase())),
        [
            ("Sinusoidal AM", "sinusoidal AM"),
            ("Pulsed modulation", "pulsed modulation")
        ]
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
            ("Pulse train", "pulse train"),
            ("Pulsed modulation", "pulsed modulation"),
            ("Sinusoidal AM", "sinusoidal AM"),
            ("Steady tone", "steady tone")
        ]
    );
    assert_eq!(
        Population::ALL.map(auditorylab_core::provenance::Population::phrase),
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
        ConditionRole::ALL.map(auditorylab_core::provenance::ConditionRole::phrase),
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
        ChannelRouting::ALL.map(auditorylab_core::ChannelRouting::phrase),
        ["both channels", "left channel", "right channel"]
    );
    assert_eq!(
        Channel::ALL.map(auditorylab_core::Channel::phrase),
        ["left channel", "right channel"]
    );
    assert_eq!(
        EvidenceScope::ALL.map(auditorylab_core::provenance::EvidenceScope::phrase),
        ["reference condition", "ancestor"]
    );
    assert_eq!(
        Measure::ALL.map(auditorylab_core::provenance::Measure::phrase),
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
        Completeness::ALL.map(auditorylab_core::rhythm::Completeness::phrase),
        ["complete", "invalid"]
    );
    assert_eq!(
        [SlotRole::Continuing, SlotRole::Outgoing, SlotRole::Incoming]
            .map(|v| (v.label(), v.phrase())),
        [
            ("Continuing", "continuing"),
            ("Outgoing", "outgoing"),
            ("Incoming", "incoming")
        ]
    );
    for parameter in ParameterId::ALL {
        assert_eq!(parameter.label(), parameter.descriptor().label);
        assert_ne!(parameter.phrase(), "");
    }
}
