//! Checked recipe builders for behavioral tests, enabled by `test-support`.
#[cfg(test)]
use crate::{SampleRate, SoundError, Target, TargetCheck, Voice};
#[cfg(test)]
use alloc::vec::Vec;
#[cfg(test)]
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
#[cfg(test)]
pub(crate) fn check(rate: SampleRate, voices: &[(ComponentId, Voice)]) -> TargetCheck {
    sound(voices)
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        .check(rate)
}
#[cfg(test)]
pub(crate) fn target(
    rate: SampleRate,
    voices: &[(ComponentId, Voice)],
) -> Result<Target, SoundError> {
    check(rate, voices).into_result()
}

#[cfg(test)]
pub(crate) fn lower(
    components: &[Component],
) -> Result<Vec<(crate::VoiceId, Voice)>, crate::SoundError> {
    let mut voices = Vec::new();
    for component in components {
        voices.extend(crate::authored::lower(component).map_err(SoundError::Pair)?);
    }
    Ok(voices)
}

#[cfg(test)]
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

/// A finite fixture value; panics on nonfinite input.
pub const fn finite(value: f64) -> crate::Finite {
    match crate::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

#[cfg(test)]
pub(crate) fn authored(components: &[Component]) -> Sound {
    Sound::new(finite(1.0), finite(0.0), components.to_vec())
        .unwrap_or_else(|error| panic!("invalid authored fixture: {error}"))
}

use crate::{Component, ComponentForm, ComponentId, DocumentIdentity, DocumentUuid, Recipe, Sound};
/// Build a checked recipe with stable identity for behavioral tests.
pub fn recipe(duration_seconds: f64, fade_seconds: f64, voices: &[ComponentForm]) -> Recipe {
    let sound = Sound::new(
        finite(duration_seconds),
        finite(fade_seconds),
        voices
            .iter()
            .enumerate()
            .map(|(index, voice)| Component {
                id: match ComponentId::numbered(index) {
                    Some(id) => id,
                    None => panic!("behavioral fixture exceeds the owner range"),
                },
                form: *voice,
            })
            .collect(),
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
    let mut builder = Recipe::builder(fixture_identity());
    builder.content.sound = sound;
    builder
        .build()
        .unwrap_or_else(|error| panic!("invalid behavioral fixture: {error}"))
}

fn fixture_identity() -> DocumentIdentity {
    DocumentIdentity {
        id: DocumentUuid::from_random_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
        created_at: None,
        modified_at: None,
    }
}
/// Build component-free silence without serialization or native audio.
pub fn silent(seconds: f64, fade: f64) -> Recipe {
    recipe(seconds, fade, &[])
}
