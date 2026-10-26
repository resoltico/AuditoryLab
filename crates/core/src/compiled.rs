// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Components validated at one sample rate, each compiled once.
//!
//! A [`TargetCheck`] compiles every component of a candidate in one bounded
//! pass. It keeps each component's own result for diagnostics and the first
//! whole-target error in the established order. A valid check yields a
//! [`Target`]: the compiled mix, which a finite [`Signal`] and the live
//! engine reuse without compiling the raw components again.
use crate::synthesis::GeneratorVoice;
use crate::{
    ComponentFacts, Ear, SampleRate, Signal, SlotRole, Sound, SoundError, Voice, VoiceError,
    VoiceId, VoiceObservation, numeric, signal::Envelope,
};
use alloc::vec::Vec;

/// One validated component: its identity, its generator and the facts derived
/// from that generator and the requested definition. Nothing stores voices or
/// facts separately, so they cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CompiledComponent {
    pub(crate) id: VoiceId,
    pub(crate) voice: GeneratorVoice,
    pub(crate) facts: ComponentFacts,
}

/// An ordered, bounded mix of compiled components at one rate, with each
/// ear's summed peak bound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CompiledMix {
    rate: SampleRate,
    len: usize,
    entries: [Option<CompiledComponent>; Sound::MAX_VOICES],
    peak: [f64; 2],
}

impl CompiledMix {
    const fn empty(rate: SampleRate) -> Self {
        Self {
            rate,
            len: 0,
            entries: [None; Sound::MAX_VOICES],
            peak: [0.0; 2],
        }
    }

    pub(crate) const fn rate(&self) -> SampleRate {
        self.rate
    }

    pub(crate) fn voices(&self) -> impl Iterator<Item = VoiceObservation> + '_ {
        self.entries().map(|entry| VoiceObservation {
            id: entry.id,
            role: SlotRole::Continuing,
            facts: entry.facts,
            gains: entry.voice.gains(),
        })
    }

    /// The components in rendering order.
    pub(crate) fn entries(&self) -> impl Iterator<Item = &CompiledComponent> {
        self.entries[..self.len].iter().flatten()
    }
}

/// A validated set of components at one sample rate, each with a caller-chosen
/// identity. A component that keeps its identity across targets keeps its
/// phase in the live engine; a new identity starts a new generator.
///
/// Two targets are equal when they define the same audio and the same
/// requested quantities: identities and order, compiled generators, routing,
/// amplitudes, requested facts and sample rate.
///
/// Compile authored components through [`Sound::target`]; lowered generator
/// lanes are an internal compilation detail.
///
/// ```compile_fail
/// use auditorylab_core::{SampleRate, Target};
/// let _ = Target::new(SampleRate::DEFAULT, &[]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    mix: CompiledMix,
}

impl Target {
    /// No components: the live engine fades to silence.
    #[must_use]
    pub const fn silence(rate: SampleRate) -> Self {
        Self {
            mix: CompiledMix::empty(rate),
        }
    }

    pub(crate) const fn mix(&self) -> &CompiledMix {
        &self.mix
    }

    /// The sample rate the components were validated at.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.mix.rate
    }

    /// The number of components.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.mix.len
    }

    /// Whether the target has no components.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.mix.len == 0
    }

    /// Each component's identity and validated facts, in order.
    pub fn components(&self) -> impl Iterator<Item = (VoiceId, &ComponentFacts)> {
        self.mix.entries().map(|entry| (entry.id, &entry.facts))
    }

    /// Validated target quantities at model origin zero, not generated engine observations.
    pub fn voices(&self) -> impl Iterator<Item = VoiceObservation> + '_ {
        self.mix.voices()
    }

    /// Bounded, origin-zero timing/envelope projection; never advances an engine.
    #[must_use]
    pub fn timing_model(&self, bins: usize) -> crate::timing::TimingModel {
        crate::timing::project(&self.mix, bins)
    }

    /// Each ear's summed amplitude bound in dBFS; silence is negative infinity.
    #[must_use]
    pub fn peak_dbfs(&self) -> [f64; 2] {
        self.mix.peak.map(numeric::dbfs)
    }

    /// Whether `other` generates the same audio: the same identities in the
    /// same order, sample rate, compiled generators, routing and amplitudes.
    /// Requested-only differences that compile to the same audio are ignored.
    /// This compares compiled structure; it does not prove that two
    /// different structures sound alike.
    #[must_use]
    pub fn same_audio(&self, other: &Self) -> bool {
        self.mix.rate == other.mix.rate
            && self.mix.len == other.mix.len
            && self
                .mix
                .entries()
                .zip(other.mix.entries())
                .all(|(a, b)| a.id == b.id && a.voice == b.voice)
    }

    /// Give the target a finite envelope, reusing the compiled components.
    ///
    /// # Errors
    /// Returns [`SoundError::Duration`], [`SoundError::Fade`] or
    /// [`SoundError::FadesOverlap`].
    pub fn signal(&self, duration_seconds: f64, fade_seconds: f64) -> Result<Signal, SoundError> {
        let envelope = Envelope::resolve(self.mix.rate, duration_seconds, fade_seconds)?;
        Ok(Signal::new(*self, envelope))
    }
}

/// The result of compiling a candidate's components at one rate: each
/// component's own result and the whole target or its first error.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetCheck {
    rate: SampleRate,
    components: Vec<(VoiceId, Result<ComponentFacts, VoiceError>)>,
    result: Result<Target, SoundError>,
}

impl TargetCheck {
    /// Lower an atomic authored candidate before compiling any generator lane.
    #[must_use]
    pub(crate) fn authored(rate: SampleRate, components: &[crate::Component]) -> Self {
        let lowered = match crate::authored::lower(components) {
            Ok(values) => values,
            Err(error) => {
                return Self {
                    rate,
                    components: Vec::new(),
                    result: Err(SoundError::Authored(error)),
                };
            }
        };
        let mut check = Self::compile_lanes(rate, &lowered);
        let intent = |id: VoiceId| {
            components
                .iter()
                .find(|c| c.id.get() == id.get())
                .and_then(|c| match c.form {
                    crate::ComponentForm::BinauralPair(pair) => {
                        Some((pair, crate::PairKind::Binaural))
                    }
                    crate::ComponentForm::MonauralPair { pair, .. } => {
                        Some((pair, crate::PairKind::Monaural))
                    }
                    _ => None,
                })
        };
        if let Err(SoundError::Component {
            number,
            error: VoiceError::Frequency { .. },
        }) = &check.result
        {
            let id = lowered[*number - 1].0;
            if let Some((pair, _)) = intent(id) {
                let error = match &check.result {
                    Err(SoundError::Component { error, .. }) => error.clone(),
                    _ => unreachable!("matched carrier error above"),
                };
                check.result = Err(SoundError::PairCarrier {
                    id,
                    first_hz: pair.first_hz,
                    offset_hz: pair.offset_hz,
                    error,
                });
            }
        }
        let annotate = |id: VoiceId, facts: &mut ComponentFacts| {
            if let Some((pair, kind)) = intent(id) {
                facts.pair = Some(crate::PairFacts {
                    owner: id.component,
                    kind,
                    first_hz: pair.first_hz,
                    offset_hz: pair.offset_hz,
                    second_hz: pair.first_hz + pair.offset_hz,
                });
            }
        };
        for (id, result) in &mut check.components {
            if let Ok(facts) = result {
                annotate(*id, facts);
            }
        }
        if let Ok(target) = &mut check.result {
            for entry in target.mix.entries[..target.mix.len].iter_mut().flatten() {
                annotate(entry.id, &mut entry.facts);
            }
        }
        check
    }

    fn compile_lanes(rate: SampleRate, components: &[(VoiceId, Voice)]) -> Self {
        let mut mix = CompiledMix::empty(rate);
        let mut first = None;
        let mut results = Vec::with_capacity(components.len());
        for (index, (id, component)) in components.iter().enumerate() {
            match component.voice(rate) {
                Ok(voice) => {
                    let facts = ComponentFacts::from_voice(component, &voice, rate);
                    results.push((*id, Ok(facts)));
                    mix.entries[mix.len] = Some(CompiledComponent {
                        id: *id,
                        voice,
                        facts,
                    });
                    mix.len += 1;
                }
                Err(error) => {
                    if first.is_none() {
                        first = Some(SoundError::Component {
                            number: index + 1,
                            error: error.clone(),
                        });
                    }
                    results.push((*id, Err(error)));
                }
            }
        }
        let result = match first {
            Some(error) => Err(error),
            None => {
                // Summed in rendering order, so that no rendered sample can exceed it.
                for entry in mix.entries[..mix.len].iter().flatten() {
                    if entry.voice.left {
                        mix.peak[0] += entry.voice.amplitude;
                    }
                    if entry.voice.right {
                        mix.peak[1] += entry.voice.amplitude;
                    }
                }
                [(Ear::Left, mix.peak[0]), (Ear::Right, mix.peak[1])]
                    .into_iter()
                    .find(|(_, amplitude)| *amplitude > 1.0)
                    .map_or(Ok(Target { mix }), |(ear, amplitude)| {
                        Err(SoundError::Peak { ear, amplitude })
                    })
            }
        };
        Self {
            rate,
            components: results,
            result,
        }
    }

    /// The rate the components were checked at.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    /// Each component's own result in input order; empty when the input
    /// exceeds [`Sound::MAX_VOICES`].
    #[must_use]
    pub fn components(&self) -> &[(VoiceId, Result<ComponentFacts, VoiceError>)] {
        &self.components
    }

    /// The component's own result, when the check compiled it.
    #[must_use]
    pub fn component(&self, id: VoiceId) -> Option<&Result<ComponentFacts, VoiceError>> {
        self.components
            .iter()
            .find(|(other, _)| *other == id)
            .map(|(_, result)| result)
    }

    /// The validated target, or the first whole-target error.
    ///
    /// # Errors
    /// Returns the first whole-target error.
    pub const fn result(&self) -> Result<&Target, &SoundError> {
        self.result.as_ref()
    }

    /// The validated target, when there is one.
    #[must_use]
    pub fn target(&self) -> Option<&Target> {
        self.result.as_ref().ok()
    }

    /// The validated target, or the first whole-target error.
    ///
    /// # Errors
    /// Returns the first whole-target error.
    pub fn into_result(self) -> Result<Target, SoundError> {
        self.result
    }
}

#[cfg(test)]
mod tests;
