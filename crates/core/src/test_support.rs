//! Authored recipe construction for internal behavioral tests.
use crate::{
    Component, ComponentForm, ComponentId, SampleRate, Sound, SoundError, Target, TargetCheck,
    Voice,
};
use alloc::vec::Vec;
pub(crate) fn sound(voices: &[(ComponentId, Voice)]) -> Result<Sound, crate::StructureError> {
    Sound::new(
        finite(1.0),
        finite(0.0),
        voices
            .iter()
            .map(|(id, voice)| Component {
                id: *id,
                form: form(*voice),
            })
            .collect(),
    )
}
pub(crate) fn check(rate: SampleRate, voices: &[(ComponentId, Voice)]) -> TargetCheck {
    sound(voices)
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        .check(rate)
}
pub(crate) fn target(
    rate: SampleRate,
    voices: &[(ComponentId, Voice)],
) -> Result<Target, SoundError> {
    check(rate, voices).into_result()
}

pub(crate) fn lower(
    components: &[Component],
) -> Result<Vec<(crate::VoiceId, Voice)>, crate::SoundError> {
    let mut voices = Vec::new();
    for component in components {
        voices.extend(crate::authored::lower(component).map_err(SoundError::Pair)?);
    }
    Ok(voices)
}

pub(crate) fn form(voice: Voice) -> ComponentForm {
    let Voice {
        ears,
        peak_dbfs,
        carrier,
        modulation,
    } = voice;
    match carrier {
        crate::Carrier::Tone { frequency_hz } => ComponentForm::Tone {
            ears,
            frequency_hz: crate::test_support::finite(frequency_hz),
            peak_dbfs: crate::test_support::finite(peak_dbfs),
            modulation,
        },
        crate::Carrier::Clicks {
            rate_hz,
            width_seconds,
        } => ComponentForm::Clicks {
            ears,
            rate_hz: crate::test_support::finite(rate_hz),
            width_seconds: crate::test_support::finite(width_seconds),
            peak_dbfs: crate::test_support::finite(peak_dbfs),
            modulation,
        },
    }
}

pub(crate) const fn finite(value: f64) -> crate::Finite {
    match crate::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

pub(crate) fn authored(components: &[Component]) -> Sound {
    Sound::new(finite(1.0), finite(0.0), components.to_vec())
        .unwrap_or_else(|error| panic!("invalid authored fixture: {error}"))
}
