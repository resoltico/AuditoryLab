// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Voices validated at one sample rate, each compiled once.
//!
//! A [`TargetCheck`] compiles every voice of a candidate in one bounded
//! pass. It keeps each voice's own result for diagnostics and the first
//! whole-target error in the established order. A valid check yields a
//! [`Target`]: the compiled mix, which a finite [`Signal`] and the live
//! engine reuse without compiling the raw components again.
use crate::synthesis::GeneratorLane;
use crate::{
    Channel, SampleRate, Signal, SlotRole, Sound, SoundError, VoiceError, VoiceFacts, VoiceId,
    VoiceObservation, conversion, signal::Envelope,
};
use alloc::vec::Vec;

/// Source adaptation is bounded and retains the authored component relationship.
pub(crate) fn source_lanes(
    component: &crate::Component,
) -> Result<[Option<(VoiceId, crate::SourceLane)>; 2], crate::PairError> {
    use crate::{Carrier, ChannelRouting, SoundSource, SourceLane};
    let form = component.form;
    let id = component.id;
    let lane = |frequency_hz, peak_dbfs, route| SourceLane {
        carrier: Carrier::Tone { frequency_hz },
        peak_dbfs,
        route,
        modulation: None,
    };
    Ok(match form.source {
        SoundSource::Tone {
            frequency_hz,
            peak_dbfs,
            route,
        } => [
            Some((id.into(), lane(frequency_hz.get(), peak_dbfs.get(), route))),
            None,
        ],
        SoundSource::PulseTrain {
            pulse,
            peak_dbfs,
            route,
        } => [
            Some((
                id.into(),
                SourceLane {
                    carrier: Carrier::PulseTrain { pulse },
                    peak_dbfs: peak_dbfs.get(),
                    route,
                    modulation: None,
                },
            )),
            None,
        ],
        SoundSource::BinauralPair { .. } | SoundSource::MonauralPair { .. } => {
            let pair = form.pair().unwrap_or_else(|| unreachable!("pair source"));
            let [first, second] = pair.carriers(id)?;
            let [left, right] = form
                .route()
                .map_or([ChannelRouting::Left, ChannelRouting::Right], |route| {
                    [route; 2]
                });
            [
                Some((id.into(), lane(first, pair.first_peak_dbfs.get(), left))),
                Some((
                    id.with_lane(crate::Lane::Secondary),
                    lane(second, pair.second_peak_dbfs.get(), right),
                )),
            ]
        }
    })
}

/// One validated voice: its identity, its generator and the facts derived
/// from that generator and the requested definition. Nothing stores voices or
/// facts separately, so they cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CompiledVoice {
    pub(crate) id: VoiceId,
    pub(crate) voice: GeneratorLane,
    pub(crate) facts: VoiceFacts,
}

/// One atomic compiled component with a single shared gain authority.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CompiledComponent {
    pub(crate) source_kind: core::mem::Discriminant<crate::SoundSource>,
    pub(crate) id: crate::ComponentId,
    pub(crate) lanes: [Option<CompiledVoice>; 2],
    pub(crate) modulation: Option<crate::synthesis::Modulator>,
}
impl CompiledComponent {
    pub(crate) fn same_audio(&self, other: &Self) -> bool {
        self.id == other.id
            && self.source_kind == other.source_kind
            && self.modulation == other.modulation
            && self
                .lanes
                .iter()
                .zip(other.lanes.iter())
                .all(|(a, b)| match (a, b) {
                    (None, None) => true,
                    (Some(a), Some(b)) => a.voice == b.voice,
                    _ => false,
                })
    }
    pub(crate) fn frame(&self, n: u64) -> [f64; 2] {
        let samples = self
            .lanes
            .map(|lane| lane.map_or(0.0, |lane| lane.voice.carrier.sample(n)));
        let gains = self
            .lanes
            .map(|lane| lane.map_or([0.0; 2], |lane| lane.voice.gains()));
        crate::synthesis::component_frame(
            samples,
            gains,
            crate::synthesis::modulation_gain(self.modulation, n),
        )
    }
}

/// An ordered, bounded mix of compiled voices at one rate, with each
/// channel's summed peak bound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CompiledMix {
    rate: SampleRate,
    len: usize,
    components: [Option<CompiledComponent>; Sound::MAX_COMPONENTS],
    component_len: usize,
    peak: [f64; 2],
}

impl CompiledMix {
    const fn empty(rate: SampleRate) -> Self {
        Self {
            rate,
            len: 0,
            components: [None; Sound::MAX_COMPONENTS],
            component_len: 0,
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

    pub(crate) fn components(&self) -> impl Iterator<Item = &CompiledComponent> {
        self.components[..self.component_len].iter().flatten()
    }
    /// The voices in rendering order.
    pub(crate) fn entries(&self) -> impl Iterator<Item = &CompiledVoice> {
        self.components()
            .flat_map(|component| component.lanes.iter().flatten())
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

    /// Each channel's summed amplitude bound in dBFS; silence is negative infinity.
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
                .components()
                .zip(other.mix.components())
                .all(|(a, b)| a.same_audio(b))
    }

    /// Give the target a finite envelope, reusing the compiled voices.
    ///
    /// # Errors
    /// Returns [`SoundError::Duration`], [`SoundError::Fade`] or
    /// [`SoundError::FadesOverlap`].
    pub fn signal(&self, duration_seconds: f64, fade_seconds: f64) -> Result<Signal, SoundError> {
        let envelope = Envelope::resolve(self.mix.rate, duration_seconds, fade_seconds)?;
        Ok(Signal::new(self, envelope))
    }
}

fn compile_component(
    component: &crate::Component,
    rate: SampleRate,
    results: &mut Vec<(VoiceId, Result<VoiceFacts, VoiceError>)>,
) -> Result<CompiledComponent, SoundError> {
    let mut first = None;
    let pair = component
        .form
        .pair()
        .zip(component.form.pair_kind())
        .map(|(pair, kind)| crate::PairFacts {
            owner: component.id,
            kind,
            first_hz: pair.first_hz.get(),
            offset_hz: pair.offset_hz.get(),
            second_hz: pair.first_hz.get() + pair.offset_hz.get(),
            realized_offset_hz: None,
        });
    let lanes = match source_lanes(component) {
        Ok(lanes) => lanes,
        Err(error) => return Err(SoundError::Pair(error)),
    };
    let shared_modulation = crate::source::compile_modulation(component.form.modulation, rate);
    let mut compiled = CompiledComponent {
        source_kind: core::mem::discriminant(&component.form.source),
        id: component.id,
        lanes: [None; 2],
        modulation: shared_modulation.as_ref().ok().copied().flatten(),
    };
    let result_start = results.len();
    for (lane_index, (id, definition)) in lanes.into_iter().flatten().enumerate() {
        let voice = definition.compile(rate).and_then(|voice| {
            shared_modulation.as_ref().map_err(Clone::clone)?;
            crate::source::check_sideband(&definition, component.form.modulation, rate)?;
            Ok(voice)
        });
        match voice {
            Ok(voice) => {
                let mut facts = VoiceFacts::from_lane(
                    &definition,
                    &voice,
                    component.form.modulation,
                    compiled.modulation,
                    rate,
                );
                facts.pair = pair;
                results.push((id, Ok(facts)));
                compiled.lanes[lane_index] = Some(CompiledVoice { id, voice, facts });
            }
            Err(error) => {
                if first.is_none() {
                    first = Some(match (pair, &error) {
                        (
                            Some(pair),
                            VoiceError::Frequency {
                                parameter: crate::ParameterId::ToneFrequency,
                                ..
                            }
                            | VoiceError::Sideband { .. },
                        ) => SoundError::PairCarrier {
                            id,
                            first_hz: pair.first_hz,
                            offset_hz: pair.offset_hz,
                            error: error.clone(),
                        },
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
    resolve_pair_facts(&mut compiled, &mut results[result_start..], pair, rate);
    first.map_or(Ok(compiled), Err)
}

fn resolve_pair_facts(
    compiled: &mut CompiledComponent,
    results: &mut [(VoiceId, Result<VoiceFacts, VoiceError>)],
    pair: Option<crate::PairFacts>,
    rate: SampleRate,
) {
    if let Some(mut pair) = pair {
        pair.realized_offset_hz = match compiled.lanes {
            [Some(first), Some(second)] => match (first.voice.carrier, second.voice.carrier) {
                (
                    crate::synthesis::Carrier::Tone { step: first },
                    crate::synthesis::Carrier::Tone { step: second },
                ) => Some(crate::facts::pair_rate(first, second, rate)),
                _ => unreachable!("explicit pairs compile to two tone lanes"),
            },
            _ => None,
        };
        for lane in compiled.lanes.iter_mut().flatten() {
            lane.facts.pair = Some(pair);
        }
        for (_, result) in results.iter_mut() {
            if let Ok(facts) = result {
                facts.pair = Some(pair);
            }
        }
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
            match compile_component(component, rate, &mut results) {
                Ok(compiled) => {
                    mix.len += compiled.lanes.iter().flatten().count();
                    mix.components[mix.component_len] = Some(compiled);
                    mix.component_len += 1;
                }
                Err(error @ SoundError::Pair(_)) => return refused(error),
                Err(error) => {
                    first.get_or_insert(error);
                }
            }
        }
        let result = if let Some(error) = first {
            Err(error)
        } else {
            for component in mix.components[..mix.component_len].iter().flatten() {
                let gains = component
                    .lanes
                    .map(|lane| lane.map_or([0.0; 2], |lane| lane.voice.gains()));
                let bounds = crate::synthesis::component_frame([1.0; 2], gains, 1.0);
                for (peak, bound) in mix.peak.iter_mut().zip(bounds) {
                    *peak += bound;
                }
            }
            [(Channel::Left, mix.peak[0]), (Channel::Right, mix.peak[1])]
                .into_iter()
                .find(|(_, amplitude)| *amplitude > 1.0)
                .map_or(Ok(Target { mix }), |(channel, amplitude)| {
                    Err(SoundError::Peak { channel, amplitude })
                })
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
mod tests;
