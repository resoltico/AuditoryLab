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

use crate::nominal::Method;

/// One rhythmic process with stable voice attribution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rhythm {
    /// Voice/component identity.
    pub id: ComponentId,
    /// Role during structural transitions.
    pub role: SlotRole,
    /// Physical mechanism.
    pub kind: Method,
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
    /// Two tones with identical routes; their levels may differ.
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

    pub(crate) fn classify(voices: impl Iterator<Item = VoiceObservation>, changing: bool) -> Self {
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
            if let Some(pair) = v.facts.pair
                && v.id.lane() == crate::Lane::Primary
                && let Some(other) = voices.iter().find(|other| {
                    other.id == v.id.with_lane(crate::Lane::Secondary)
                        && other.facts.pair == Some(pair)
                })
                && let (CarrierFacts::Tone(first), CarrierFacts::Tone(second)) =
                    (v.facts.carrier, other.facts.carrier)
            {
                let difference = (second.realized_hz - first.realized_hz).abs();
                if difference > 0.0 {
                    add(Method::of_pair(pair.kind), difference);
                }
            }
            if let CarrierFacts::Clicks(p) = v.facts.carrier {
                add(Method::Clicks, p.rate_hz);
            }
            match v.facts.modulation {
                Some(ModulationFacts::Gate(p)) => add(Method::Gate, p.rate_hz),
                Some(ModulationFacts::Am { rate, depth }) if depth > 0.0 => {
                    add(Method::Am, rate.realized_hz);
                }
                _ => {}
            }
        }
        let relationship = if voices.len() == 2
            && voices
                .iter()
                .all(|v| effectively_unmodulated(v.facts.modulation))
        {
            match (voices[0].facts.carrier, voices[1].facts.carrier) {
                (CarrierFacts::Tone(a), CarrierFacts::Tone(b)) => {
                    let (x, y) = (voices[0].gains, voices[1].gains);
                    match Method::pair_routes(x.map(|g| g > 0.0), y.map(|g| g > 0.0)) {
                        Some(Method::Binaural) if x[0] > 0.0 => {
                            Some(ToneRelationship::Interaural {
                                left: a.realized_hz,
                                right: b.realized_hz,
                            })
                        }
                        Some(Method::Binaural) => Some(ToneRelationship::Interaural {
                            left: b.realized_hz,
                            right: a.realized_hz,
                        }),
                        Some(Method::Monaural) => Some(ToneRelationship::Monaural {
                            first: a.realized_hz,
                            second: b.realized_hz,
                        }),
                        _ => None,
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

    /// Deduplicated structural methods, including nonrhythmic steady tones.
    #[must_use]
    pub fn methods(&self) -> Vec<Method> {
        let mut methods: Vec<_> = self.rhythms.iter().map(|r| r.kind).collect();
        if let Some(relationship) = self.relationship {
            methods.push(match relationship {
                ToneRelationship::Interaural { .. } => Method::Binaural,
                ToneRelationship::Monaural { .. } => Method::Monaural,
            });
        }
        for voice in &self.voices {
            if let Some(pair) = voice.facts.pair {
                methods.push(Method::of_pair(pair.kind));
            } else if self.relationship.is_none()
                && matches!(voice.facts.carrier, CarrierFacts::Tone(_))
                && effectively_unmodulated(voice.facts.modulation)
            {
                methods.push(Method::SteadyTone);
            }
        }
        methods.sort();
        methods.dedup();
        methods
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

    /// The slowest active rhythmic rate, including either two-tone difference.
    #[must_use]
    pub fn slowest_rate(&self) -> Option<f64> {
        let difference = self
            .relationship
            .map(|relationship| match relationship {
                ToneRelationship::Interaural { left, right } => (left - right).abs(),
                ToneRelationship::Monaural { first, second } => (first - second).abs(),
            })
            .filter(|difference| *difference > 0.0);
        difference
            .into_iter()
            .chain(self.rhythms.iter().map(|rhythm| rhythm.hz))
            .reduce(f64::min)
    }
}

fn effectively_unmodulated(modulation: Option<ModulationFacts>) -> bool {
    matches!(
        modulation,
        None | Some(ModulationFacts::Am { depth: 0.0, .. })
    )
}
