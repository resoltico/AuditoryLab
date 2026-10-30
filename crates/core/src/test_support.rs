//! Checked recipe builders for behavioral tests, enabled by `test-support`.
#[cfg(test)]
use crate::{SampleRate, SoundError, Target, TargetCheck};
#[cfg(test)]
pub(crate) fn sound(
    components: &[(ComponentId, ComponentForm)],
) -> Result<FiniteFixture, crate::StructureError> {
    FiniteFixture::new(
        finite(1.0),
        finite(0.0),
        components
            .iter()
            .map(|(id, form)| Component {
                id: *id,
                form: *form,
            })
            .collect(),
    )
}
#[cfg(test)]
pub(crate) fn check(rate: SampleRate, components: &[(ComponentId, ComponentForm)]) -> TargetCheck {
    sound(components)
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        .check(rate)
}
#[cfg(test)]
pub(crate) fn target(
    rate: SampleRate,
    components: &[(ComponentId, ComponentForm)],
) -> Result<Target, SoundError> {
    check(rate, components).into_result()
}

/// A finite fixture value; panics on nonfinite input.
pub const fn finite(value: f64) -> crate::Finite {
    match crate::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

#[cfg(test)]
pub(crate) fn authored(components: &[Component]) -> FiniteFixture {
    FiniteFixture::new(finite(1.0), finite(0.0), components.to_vec())
        .unwrap_or_else(|error| panic!("invalid authored fixture: {error}"))
}

use crate::{Component, ComponentForm, ComponentId, DocumentIdentity, DocumentUuid, Recipe, Sound};
/// Build a checked recipe with stable identity for behavioral tests.
pub fn recipe(duration_seconds: f64, fade_seconds: f64, voices: &[ComponentForm]) -> Recipe {
    let sound = Sound::new(
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
    builder.content.timing = crate::FiniteTiming {
        duration_seconds: finite(duration_seconds),
        fade_seconds: finite(fade_seconds),
    };
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

/// Test input carrying finite timing explicitly beside the production sound.
/// Its live compilation delegates to Sound; finite generation delegates to `FiniteTiming`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiniteFixture {
    /// Ordered production sound.
    pub sound: Sound,
    /// Independently authored finite timing.
    pub timing: crate::FiniteTiming,
}
impl FiniteFixture {
    /// Derived component capacity, identical to the production sound budget.
    pub const MAX_COMPONENTS: usize = Sound::MAX_COMPONENTS;
    /// Production generator-lane budget.
    pub const MAX_VOICES: usize = Sound::MAX_VOICES;
    /// Capture explicit sound and timing from a portable recipe.
    pub fn from_recipe(recipe: &Recipe) -> Self {
        Self {
            sound: recipe.sound().clone(),
            timing: recipe.timing(),
        }
    }
    /// Construct a finite test input without requiring its acoustics to be valid.
    pub fn new(
        duration_seconds: crate::Finite,
        fade_seconds: crate::Finite,
        components: alloc::vec::Vec<Component>,
    ) -> Result<Self, crate::StructureError> {
        Ok(Self {
            sound: Sound::new(components)?,
            timing: crate::FiniteTiming {
                duration_seconds,
                fade_seconds,
            },
        })
    }
    /// Requested finite duration.
    pub const fn duration_seconds(&self) -> crate::Finite {
        self.timing.duration_seconds
    }
    /// Requested overall finite fade.
    pub const fn fade_seconds(&self) -> crate::Finite {
        self.timing.fade_seconds
    }
    /// Replace only finite duration.
    pub fn set_duration(&mut self, value: crate::Finite) {
        self.timing.duration_seconds = value;
    }
    /// Replace only finite fade.
    pub fn set_fade(&mut self, value: crate::Finite) {
        self.timing.fade_seconds = value;
    }
    /// Realize finite timing and sound using production compilation.
    pub fn signal(&self, rate: crate::SampleRate) -> Result<crate::Signal, crate::SoundError> {
        self.timing.check(rate)?;
        self.timing.signal(&self.sound.target(rate)?)
    }
}
impl core::ops::Deref for FiniteFixture {
    type Target = Sound;
    fn deref(&self) -> &Sound {
        &self.sound
    }
}
impl core::ops::DerefMut for FiniteFixture {
    fn deref_mut(&mut self) -> &mut Sound {
        &mut self.sound
    }
}

/// A finite rectangular pulse fixture, compiled by the production pulse primitive.
pub fn rectangular_pulse(rate_hz: f64, seconds: f64) -> crate::PulseSpec {
    crate::PulseSpec {
        rate_hz: finite(rate_hz),
        width: crate::PulseWidth::Duration {
            seconds: finite(seconds),
        },
        shape: crate::PulseShape::Rectangular {},
    }
}
