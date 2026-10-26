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
                form: ComponentForm::Voice(*voice),
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
) -> Result<Vec<(crate::VoiceId, Voice)>, crate::LoweringError> {
    Sound::check_structure(components)?;
    let mut voices = Vec::new();
    for component in components {
        voices.extend(crate::authored::lower(component)?);
    }
    Ok(voices)
}
