// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Voices validated at one sample rate, each compiled once.
//!
//! A [`TargetCheck`] compiles every voice of a candidate in one bounded
//! pass. It keeps each voice's own result for diagnostics and the first
//! whole-target error in the established order. A valid check yields a
//! [`Target`]: the compiled mix, which a finite [`Signal`] and the live
//! engine reuse without compiling the raw components again.
use crate::synthesis::GeneratorVoice;
use crate::{
    Ear, SampleRate, Signal, SlotRole, Sound, SoundError, VoiceError, VoiceFacts, VoiceId,
    VoiceObservation, conversion, signal::Envelope,
};
use alloc::vec::Vec;

/// One validated voice: its identity, its generator and the facts derived
/// from that generator and the requested definition. Nothing stores voices or
/// facts separately, so they cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CompiledVoice {
    pub(crate) id: VoiceId,
    pub(crate) voice: GeneratorVoice,
    pub(crate) facts: VoiceFacts,
}

/// An ordered, bounded mix of compiled voices at one rate, with each
/// ear's summed peak bound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CompiledMix {
    rate: SampleRate,
    len: usize,
    entries: [Option<CompiledVoice>; Sound::MAX_VOICES],
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

    pub(crate) fn observations(&self) -> impl Iterator<Item = VoiceObservation> + '_ {
        self.entries().map(|entry| VoiceObservation {
            id: entry.id,
            role: SlotRole::Continuing,
            facts: entry.facts,
            gains: entry.voice.gains(),
        })
    }

    /// The voices in rendering order.
    pub(crate) fn entries(&self) -> impl Iterator<Item = &CompiledVoice> {
        self.entries[..self.len].iter().flatten()
    }
}

/// A validated set of voices at one sample rate, each with a caller-chosen
/// identity. A voice that keeps its identity across targets keeps its
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
    /// No voices: the live engine fades to silence.
    #[must_use]
    pub const fn silence(rate: SampleRate) -> Self {
        Self {
            mix: CompiledMix::empty(rate),
        }
    }

    pub(crate) const fn mix(&self) -> &CompiledMix {
        &self.mix
    }

    /// The sample rate the voices were validated at.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.mix.rate
    }

    /// The number of voices.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.mix.len
    }

    /// Whether the target has no voices.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.mix.len == 0
    }

    /// Each voice's identity and validated facts, in order.
    pub fn voices(&self) -> impl Iterator<Item = (VoiceId, &VoiceFacts)> {
        self.mix.entries().map(|entry| (entry.id, &entry.facts))
    }

    /// Validated target quantities at model origin zero, not generated engine observations.
    pub fn observations(&self) -> impl Iterator<Item = VoiceObservation> + '_ {
        self.mix.observations()
    }

    /// Bounded, origin-zero timing/envelope projection; never advances an engine.
    #[must_use]
    pub fn timing_model(&self, bins: usize) -> crate::timing::TimingModel {
        crate::timing::project(&self.mix, bins)
    }

    /// Each ear's summed amplitude bound in dBFS; silence is negative infinity.
    #[must_use]
    pub fn peak_dbfs(&self) -> [f64; 2] {
        self.mix.peak.map(conversion::dbfs)
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

    /// Give the target a finite envelope, reusing the compiled voices.
    ///
    /// # Errors
    /// Returns [`SoundError::Duration`], [`SoundError::Fade`] or
    /// [`SoundError::FadesOverlap`].
    pub fn signal(&self, duration_seconds: f64, fade_seconds: f64) -> Result<Signal, SoundError> {
        let envelope = Envelope::resolve(self.mix.rate, duration_seconds, fade_seconds)?;
        Ok(Signal::new(*self, envelope))
    }
}

/// The result of compiling a candidate's voices at one rate: each
/// voice's own result and the whole target or its first error.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetCheck {
    rate: SampleRate,
    voices: Vec<(VoiceId, Result<VoiceFacts, VoiceError>)>,
    result: Result<Target, SoundError>,
}

impl TargetCheck {
    /// Compile each admitted authored component with its pair intent in scope.
    #[must_use]
    pub(crate) fn authored(rate: SampleRate, sound: &Sound) -> Self {
        let refused = |error| Self {
            rate,
            voices: Vec::new(),
            result: Err(error),
        };
        let mut mix = CompiledMix::empty(rate);
        let mut first = None;
        let mut results = Vec::new();
        for component in sound.components() {
            let pair = component
                .form
                .pair()
                .zip(component.form.pair_kind())
                .map(|(pair, kind)| crate::PairFacts {
                    owner: component.id,
                    kind,
                    first_hz: pair.first_hz,
                    offset_hz: pair.offset_hz,
                    second_hz: pair.first_hz + pair.offset_hz,
                });
            let lanes = match crate::authored::lower(component) {
                Ok(lanes) => lanes,
                Err(error) => return refused(SoundError::Pair(error)),
            };
            for (id, definition) in lanes {
                match definition.voice(rate) {
                    Ok(voice) => {
                        let mut facts = VoiceFacts::from_voice(&definition, &voice, rate);
                        facts.pair = pair;
                        results.push((id, Ok(facts)));
                        mix.entries[mix.len] = Some(CompiledVoice { id, voice, facts });
                        mix.len += 1;
                    }
                    Err(error) => {
                        if first.is_none() {
                            first = Some(match (pair, &error) {
                                (Some(pair), VoiceError::Frequency { .. }) => {
                                    SoundError::PairCarrier {
                                        id,
                                        first_hz: pair.first_hz,
                                        offset_hz: pair.offset_hz,
                                        error: error.clone(),
                                    }
                                }
                                _ => SoundError::Voice {
                                    id,
                                    error: error.clone(),
                                },
                            });
                        }
                        results.push((id, Err(error)));
                    }
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
            voices: results,
            result,
        }
    }

    /// The rate the voices were checked at.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    /// Each voice's own result in input order; empty when the input
    /// exceeds [`Sound::MAX_VOICES`].
    #[must_use]
    pub fn voices(&self) -> &[(VoiceId, Result<VoiceFacts, VoiceError>)] {
        &self.voices
    }

    /// The voice's own result, when the check compiled it.
    #[must_use]
    pub fn voice(&self, id: VoiceId) -> Option<&Result<VoiceFacts, VoiceError>> {
        self.voices
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
#[path = "compiled/tests.rs"]
mod tests;
