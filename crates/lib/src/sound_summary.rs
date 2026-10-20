//! Shared physical meaning for validated targets and actual generated observations.
use crate::{CarrierFacts, ModulationFacts, Observation, VoiceObservation, compact_number};
use auditorylab_core::live::Target;
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
/// One rhythmic process with stable voice attribution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rhythm {
    /// Voice/component identity.
    pub id: u32,
    /// Role during structural transitions.
    pub role: crate::SlotRole,
    /// Physical mechanism.
    pub kind: RhythmKind,
    /// Actual/model-realized rate, never inferred from a title.
    pub hz: f64,
}
/// A deliberately narrow relationship between exactly two unmodulated tones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToneRelationship {
    /// One steady source exclusively in each ear.
    Interaural {
        /// Left carrier.
        left: f64,
        /// Right carrier.
        right: f64,
    },
    /// Equal-level tones with identical routes; no biological claim.
    Monaural {
        /// First carrier.
        first: f64,
        /// Second carrier.
        second: f64,
    },
}
/// Derived interpretation; never a second editable/saved sound model.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundSummary {
    /// All independently attributable rhythmic processes.
    pub rhythms: Vec<Rhythm>,
    /// Active constituents, including outgoing/incoming roles.
    pub voices: Vec<VoiceObservation>,
    /// Narrow two-tone relationship when justified.
    pub relationship: Option<ToneRelationship>,
    /// Generated state is changing.
    pub changing: bool,
}
impl SoundSummary {
    /// Interpret actual last-generated-frame quantities.
    pub fn generated(observation: &Observation) -> Self {
        Self::classify(
            observation.voices.iter().flatten().copied(),
            observation.transitioning,
        )
    }
    /// Interpret validated target facts at model origin, without advancing an engine.
    pub fn target(target: &Target) -> Self {
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
                })
            };
            if let CarrierFacts::Clicks(p) = v.facts.carrier {
                add(RhythmKind::Clicks, p.rate_hz);
            }
            match v.facts.modulation {
                Some(ModulationFacts::Gate(p)) => add(RhythmKind::Gate, p.rate_hz),
                Some(ModulationFacts::Sinusoidal { rate, depth }) if depth > 0.0 => {
                    add(RhythmKind::AmplitudeModulation, rate.realized_hz)
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
    /// Concise shared description; no global-frequency guess for independent processes.
    pub fn description(&self) -> String {
        let relation = match self.relationship {
            Some(ToneRelationship::Interaural { left, right }) => Some(if left == right {
                format!(
                    "Unison · Left/right {} Hz steady tones",
                    compact_number(left)
                )
            } else {
                format!(
                    "{} Hz interaural difference · Left {} / Right {} Hz steady tones",
                    compact_number((left - right).abs()),
                    compact_number(left),
                    compact_number(right)
                )
            }),
            Some(ToneRelationship::Monaural { first, second }) => Some(if first == second {
                format!("Unison · {} Hz steady tones", compact_number(first))
            } else {
                format!(
                    "{} Hz monaural beat · {} + {} Hz steady tones",
                    compact_number((first - second).abs()),
                    compact_number(first),
                    compact_number(second)
                )
            }),
            None => None,
        };
        let mut parts = Vec::new();
        for r in &self.rhythms {
            let detail = self
                .voices
                .iter()
                .find(|v| v.id == r.id && v.role == r.role)
                .map(|v| match r.kind {
                    RhythmKind::AmplitudeModulation => match v.facts.modulation {
                        Some(ModulationFacts::Sinusoidal { depth, .. }) => {
                            format!(" · depth {}%", crate::compact_number(depth * 100.0))
                        }
                        _ => String::new(),
                    },
                    RhythmKind::Gate => match v.facts.modulation {
                        Some(ModulationFacts::Gate(p)) => {
                            format!(" · {} ms", crate::compact_number(p.width_seconds * 1000.0))
                        }
                        _ => String::new(),
                    },
                    RhythmKind::Clicks => match v.facts.carrier {
                        CarrierFacts::Clicks(p) => {
                            format!(" · {} ms", crate::compact_number(p.width_seconds * 1000.0))
                        }
                        _ => String::new(),
                    },
                })
                .unwrap_or_default();
            parts.push(format!(
                "{}{} Hz {}{detail}",
                if self.changing {
                    match r.role {
                        crate::SlotRole::Outgoing => "Outgoing ",
                        crate::SlotRole::Incoming => "Incoming ",
                        _ => "",
                    }
                } else {
                    ""
                },
                compact_number(r.hz),
                match r.kind {
                    RhythmKind::Clicks => "clicks",
                    RhythmKind::Gate => "gate",
                    RhythmKind::AmplitudeModulation => "AM",
                }
            ));
        }
        for v in &self.voices {
            if let CarrierFacts::Tone(rate) = v.facts.carrier {
                parts.push(format!(
                    "{} Hz {}tone",
                    compact_number(rate.realized_hz),
                    if v.facts.modulation.is_none()
                        || matches!(
                            v.facts.modulation,
                            Some(ModulationFacts::Sinusoidal { depth: 0.0, .. })
                        )
                    {
                        "steady "
                    } else {
                        "carrier "
                    }
                ));
            }
        }
        let text = relation.unwrap_or_else(|| {
            if parts.is_empty() {
                "Silence · no active generators".into()
            } else if parts.len() > 4 {
                format!(
                    "{} components · {} + {} more processes (component details)",
                    self.voices.len(),
                    parts[..4].join(" + "),
                    parts.len() - 4
                )
            } else {
                parts.join(" + ")
            }
        });
        let routes: Vec<_> = self
            .voices
            .iter()
            .map(|v| match v.gains {
                [l, r] if l > 0.0 && r > 0.0 => "Both ears",
                [l, _] if l > 0.0 => "Left",
                _ => "Right",
            })
            .collect();
        let route = routes
            .first()
            .filter(|first| routes.iter().all(|r| r == *first))
            .map(|s| format!(" · {s}"))
            .unwrap_or_default();
        format!(
            "{}{text}{route}",
            if self.changing { "Changing… · " } else { "" }
        )
    }
}
/// Validate a recipe's timeless target at the chosen model rate.
pub fn recipe_target(
    preset: &crate::Preset,
    rate: crate::SampleRate,
) -> Result<Target, crate::Error> {
    let components: Vec<_> = preset
        .stimulus()
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (u32::try_from(i + 1).unwrap_or(u32::MAX), *c))
        .collect();
    Target::new(rate, &components)
        .map_err(|e| crate::Error::caused(crate::ErrorKind::InvalidSound, e.to_string(), e))
}
/// Concise component role without another editor model; active modulation is never hidden.
pub fn component_role(component: &crate::Component) -> &'static str {
    match component.modulation {
        Some(crate::Modulation::Rectangular { .. }) => "Gate rhythm",
        Some(crate::Modulation::Sinusoidal { depth, .. }) if depth > 0.0 => "AM envelope",
        _ => match component.carrier {
            crate::Carrier::Click { .. } => "Click rhythm",
            crate::Carrier::Tone { .. } => "Steady tone",
        },
    }
}
