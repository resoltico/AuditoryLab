//! Public diagnostic formatting, source chains and affected-quantity contracts.
use auditorylab_core::{
    ComponentId, PairError, PairIssue, ParameterId as P, SoundError, StructureError, ValueIssue,
    ValueRule, VoiceError,
};
use std::error::Error;

#[test]
fn display_preserves_stable_identifiers_and_nested_sources_preserve_types() {
    let id = ComponentId::new(7).unwrap();
    for (issue, code) in [
        (PairIssue::Nonfinite, "pair.nonfinite"),
        (PairIssue::Nonpositive, "pair.nonpositive"),
        (
            PairIssue::UnrepresentablePairDifference,
            "pair.unrepresentable_difference",
        ),
    ] {
        assert_eq!(issue.to_string(), code);
        let pair = PairError {
            issue,
            id,
            first_hz: 200.0,
            offset_hz: -200.0,
        };
        assert_eq!(pair.to_string(), code);
        let sound = SoundError::Pair(pair.clone());
        assert_eq!(sound.to_string(), code);
        assert_eq!(
            sound.source().unwrap().downcast_ref::<PairError>(),
            Some(&pair)
        );
        let rhythm = auditorylab_core::rhythm::Issue::Pair(pair.clone());
        assert_eq!(rhythm.to_string(), code);
        assert_eq!(
            rhythm.source().unwrap().downcast_ref::<PairError>(),
            Some(&pair)
        );
    }
    for (error, code) in [
        (StructureError::Identity { id }, "structure.identity"),
        (
            StructureError::ComponentBudget { count: 17 },
            "structure.component_budget",
        ),
        (
            StructureError::VoiceBudget { count: 18 },
            "structure.voice_budget",
        ),
    ] {
        assert_eq!(error.to_string(), code);
    }
    let issue = ValueIssue {
        parameter: P::ToneFrequency,
        value: 0.0,
        rule: ValueRule::Positive,
    };
    assert_eq!(issue.to_string(), "value.positive");
    let rhythm = auditorylab_core::rhythm::Issue::Value {
        owner: id.into(),
        issue,
    };
    assert_eq!(rhythm.to_string(), "value.positive");
    assert_eq!(
        rhythm.source().unwrap().downcast_ref::<ValueIssue>(),
        Some(&issue)
    );
    let voice = VoiceError::Frequency {
        parameter: P::GateRate,
        hz: 24_000.0,
        sample_rate_hz: 48_000,
    };
    assert_eq!(voice.to_string(), "voice.frequency");
    for sound in [
        SoundError::Voice {
            id: id.into(),
            error: voice.clone(),
        },
        SoundError::PairCarrier {
            id: id.into(),
            first_hz: 24_000.0,
            offset_hz: 6.0,
            error: voice.clone(),
        },
    ] {
        assert_eq!(sound.to_string(), "voice.frequency");
        assert_eq!(
            sound.source().unwrap().downcast_ref::<VoiceError>(),
            Some(&voice)
        );
    }
    assert!(
        SoundError::Duration {
            seconds: 0.0,
            sample_rate_hz: 48_000
        }
        .source()
        .is_none()
    );
}

#[test]
fn frequency_failures_identify_their_own_quantity() {
    for parameter in [P::ToneFrequency, P::ClickRate, P::AmRate, P::GateRate] {
        let error = VoiceError::Frequency {
            parameter,
            hz: 24_000.0,
            sample_rate_hz: 48_000,
        };
        assert_eq!(error.parameters(), &[parameter]);
        assert_eq!(
            SoundError::Voice {
                id: ComponentId::new(1).unwrap().into(),
                error
            }
            .parameters(),
            &[parameter]
        );
    }
}

#[test]
fn finite_display_keeps_the_value_and_requested_precision() {
    use auditorylab_core::Finite;
    assert_eq!(Finite::new(-0.0).unwrap().to_string(), "-0");
    assert_eq!(Finite::new(12.5).unwrap().to_string(), "12.5");
    assert_eq!(format!("{:.2}", Finite::new(12.5).unwrap()), "12.50");
}
