// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The physical structure of a validated target or a generated frame.
//!
//! The interpretation names independent rhythmic processes and one narrowly
//! defined two-tone relationship. It never infers a rate from a title and
//! makes no claim about perception or biological response.
use crate::{
    CarrierFacts, ComponentId, ModulationFacts, Observation, SlotRole, Target, VoiceObservation,
};
use alloc::vec::Vec;

/// Kind of temporal process, retained even when two rates coincide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RhythmKind {
    /// Click onset schedule.
    Clicks,
    /// Rectangular gate.
    Gate,
    /// Active sinusoidal amplitude modulation.
    AmplitudeModulation,
}

impl RhythmKind {
    /// A short name for the process.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Clicks => "clicks",
            Self::Gate => "gate",
            Self::AmplitudeModulation => "AM",
        }
    }
}

/// One rhythmic process with stable voice attribution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rhythm {
    /// Voice/component identity.
    pub id: ComponentId,
    /// Role during structural transitions.
    pub role: SlotRole,
    /// Physical mechanism.
    pub kind: RhythmKind,
    /// Actual/model-realized rate, never inferred from a title.
    pub hz: f64,
}

/// A deliberately narrow relationship between exactly two unmodulated tones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToneRelationship {
    /// One steady tone exclusively in each ear.
    Interaural {
        /// Left carrier.
        left: f64,
        /// Right carrier.
        right: f64,
    },
    /// Equal-level tones with identical routes.
    Monaural {
        /// First carrier.
        first: f64,
        /// Second carrier.
        second: f64,
    },
}

/// Derived interpretation; never a second editable or saved sound model.
#[derive(Debug, Clone, PartialEq)]
pub struct Structure {
    /// All independently attributable rhythmic processes.
    pub rhythms: Vec<Rhythm>,
    /// Active constituents, including outgoing/incoming roles.
    pub voices: Vec<VoiceObservation>,
    /// Narrow two-tone relationship when justified.
    pub relationship: Option<ToneRelationship>,
    /// The generated state is changing.
    pub changing: bool,
}

impl Structure {
    /// Interpret a generated frame's actual quantities.
    #[must_use]
    pub fn of_observation(observation: &Observation) -> Self {
        Self::classify(
            observation.voices.iter().flatten().copied(),
            observation.transitioning,
        )
    }

    /// Interpret validated target facts at model origin, without advancing an engine.
    #[must_use]
    pub fn of_target(target: &Target) -> Self {
        Self::classify(target.voices(), false)
    }

    fn classify(voices: impl Iterator<Item = VoiceObservation>, changing: bool) -> Self {
        let voices: Vec<_> = voices
            .filter(|v| v.gains.iter().any(|g| *g > 0.0))
            .collect();
        let mut rhythms = Vec::new();
        for v in &voices {
            let mut add = |kind, hz| {
                rhythms.push(Rhythm {
                    id: v.id,
                    role: v.role,
                    kind,
                    hz,
                });
            };
            if let CarrierFacts::Clicks(p) = v.facts.carrier {
                add(RhythmKind::Clicks, p.rate_hz);
            }
            match v.facts.modulation {
                Some(ModulationFacts::Gate(p)) => add(RhythmKind::Gate, p.rate_hz),
                Some(ModulationFacts::Sinusoidal { rate, depth }) if depth > 0.0 => {
                    add(RhythmKind::AmplitudeModulation, rate.realized_hz);
                }
                _ => {}
            }
        }
        let relationship =
            if voices.len() == 2 && voices.iter().all(|v| v.facts.modulation.is_none()) {
                match (voices[0].facts.carrier, voices[1].facts.carrier) {
                    (CarrierFacts::Tone(a), CarrierFacts::Tone(b)) => {
                        let (x, y) = (voices[0].gains, voices[1].gains);
                        if x[0] > 0.0 && x[1] == 0.0 && y[0] == 0.0 && y[1] > 0.0 {
                            Some(ToneRelationship::Interaural {
                                left: a.realized_hz,
                                right: b.realized_hz,
                            })
                        } else if y[0] > 0.0 && y[1] == 0.0 && x[0] == 0.0 && x[1] > 0.0 {
                            Some(ToneRelationship::Interaural {
                                left: b.realized_hz,
                                right: a.realized_hz,
                            })
                        } else if x == y {
                            Some(ToneRelationship::Monaural {
                                first: a.realized_hz,
                                second: b.realized_hz,
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            } else {
                None
            };
        Self {
            rhythms,
            voices,
            relationship,
            changing,
        }
    }

    /// The narrow interaural relationship, when exactly one unmodulated tone
    /// reaches each ear exclusively and no other voice is active.
    #[must_use]
    pub const fn interaural(&self) -> Option<(f64, f64)> {
        match self.relationship {
            Some(ToneRelationship::Interaural { left, right }) => Some((left, right)),
            _ => None,
        }
    }

    /// The slowest active rhythmic rate, including an interaural difference.
    #[must_use]
    pub fn slowest_rate(&self) -> Option<f64> {
        let difference = self
            .interaural()
            .map(|(left, right)| (left - right).abs())
            .filter(|difference| *difference > 0.0);
        difference
            .into_iter()
            .chain(self.rhythms.iter().map(|rhythm| rhythm.hz))
            .reduce(f64::min)
    }
}
