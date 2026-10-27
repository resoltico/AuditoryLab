//! Authored recipe construction for internal behavioral tests.
use crate::{
    Component, ComponentForm, ComponentId, SampleRate, Sound, SoundError, Target, TargetCheck,
    Voice,
};
use alloc::vec::Vec;
pub(crate) fn check(rate: SampleRate, voices: &[(ComponentId, Voice)]) -> TargetCheck {
    let sound = Sound {
        duration_seconds: 1.0,
        fade_seconds: 0.0,
        components: voices
            .iter()
            .map(|(id, voice)| Component {
                id: *id,
                form: crate::test_support::form(*voice),
            })
            .collect::<Vec<_>>(),
    };
    sound.check(rate)
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
    Sound::check_structure(components).map_err(SoundError::Structure)?;
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
            frequency_hz,
            peak_dbfs,
            modulation,
        },
        crate::Carrier::Clicks {
            rate_hz,
            width_seconds,
        } => ComponentForm::Clicks {
            ears,
            rate_hz,
            width_seconds,
            peak_dbfs,
            modulation,
        },
    }
}
