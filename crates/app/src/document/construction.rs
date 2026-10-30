//! Shared authored construction defaults, independent of control ranges.
use auditorylab_core::{
    CarrierKind, ChannelRouting, Finite, Modulation, ModulationKind, ParameterId,
};
/// A quiet unmodulated component, routed to both channels.
#[must_use]
pub fn default_component(kind: CarrierKind) -> auditorylab_core::ComponentForm {
    let route = ChannelRouting::Both;
    let peak_dbfs = model_default(ParameterId::PeakLevel);
    let modulation = None;
    match kind {
        CarrierKind::Tone => auditorylab_core::ComponentForm {
            source: auditorylab_core::SoundSource::Tone {
                route,
                peak_dbfs,
                frequency_hz: model_default(ParameterId::ToneFrequency),
            },
            modulation,
        },
        CarrierKind::PulseTrain => auditorylab_core::ComponentForm {
            source: auditorylab_core::SoundSource::PulseTrain {
                route,
                peak_dbfs,
                pulse: auditorylab_core::PulseSpec {
                    rate_hz: model_default(ParameterId::SourcePulseRate),
                    width: auditorylab_core::PulseWidth::Duration {
                        seconds: model_default(ParameterId::SourcePulseDuration),
                    },
                    shape: auditorylab_core::PulseShape::Rectangular {},
                },
            },
            modulation,
        },
    }
}

/// Quiet explicit pair with application-owned defaults.
#[must_use]
pub fn default_pair(kind: auditorylab_core::PairKind) -> auditorylab_core::ComponentForm {
    match kind {
        auditorylab_core::PairKind::Binaural => auditorylab_core::ComponentForm {
            source: auditorylab_core::SoundSource::BinauralPair {
                first_hz: model_default(ParameterId::PairFirstFrequency),
                offset_hz: model_default(ParameterId::PairOffset),
                first_peak_dbfs: model_default(ParameterId::PairFirstLevel),
                second_peak_dbfs: model_default(ParameterId::PairSecondLevel),
            },
            modulation: None,
        },
        auditorylab_core::PairKind::Monaural => auditorylab_core::ComponentForm {
            source: auditorylab_core::SoundSource::MonauralPair {
                route: ChannelRouting::Both,
                first_hz: model_default(ParameterId::PairFirstFrequency),
                offset_hz: model_default(ParameterId::PairOffset),
                first_peak_dbfs: model_default(ParameterId::PairFirstLevel),
                second_peak_dbfs: model_default(ParameterId::PairSecondLevel),
            },
            modulation: None,
        },
    }
}

fn finite_default(value: f64) -> Finite {
    Finite::new(value).unwrap_or_else(|| unreachable!("finite authored defaults"))
}
fn model_default(id: ParameterId) -> Finite {
    use ParameterId as P;
    finite_default(match id {
        P::ToneFrequency => 440.0,
        P::PairFirstFrequency => 200.0,
        P::PairOffset => 6.0,
        P::SourcePulseRate | P::ModulationPulseRate | P::ModulationRate => 40.0,
        P::SourcePulseDuration | P::ModulationPulseDuration => 0.001,
        P::ModulationDepth => 1.0,
        P::PeakLevel | P::PairFirstLevel | P::PairSecondLevel => -30.0,
        P::SourceDuty | P::ModulationDuty => 0.5,
        P::SourceRise | P::SourceFall | P::ModulationRise | P::ModulationFall => 0.002,
        P::Duration => 60.0,
        P::Fade => 0.1,
    })
}

/// Default parameters for a modulation kind, in model units.
#[must_use]
pub fn default_modulation(kind: ModulationKind) -> Modulation {
    let model = model_default;
    match kind {
        ModulationKind::Sinusoidal => Modulation::Sinusoidal {
            rate_hz: model(ParameterId::ModulationRate),
            depth: model(ParameterId::ModulationDepth),
        },
        ModulationKind::Pulsed => Modulation::Pulsed {
            pulse: auditorylab_core::PulseSpec {
                rate_hz: model(ParameterId::ModulationPulseRate),
                width: auditorylab_core::PulseWidth::Duration {
                    seconds: model(ParameterId::ModulationPulseDuration),
                },
                shape: auditorylab_core::PulseShape::Rectangular {},
            },
        },
    }
}

/// Application-owned construction choices shared by desktop and CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationKind {
    Tone,
    PulseTrain,
    BinauralPair,
    MonauralPair,
    PulsedTone,
}
impl CreationKind {
    /// All offered creation kinds, in menu order.
    pub const ALL: [Self; 5] = [
        Self::Tone,
        Self::PulseTrain,
        Self::BinauralPair,
        Self::MonauralPair,
        Self::PulsedTone,
    ];
    /// Domain-owned construction wording; imperative menu text belongs to presentation.
    pub const fn term(self) -> auditorylab_core::vocabulary::Term {
        use auditorylab_core::PairKind;
        match self {
            Self::Tone => auditorylab_core::vocabulary::STEADY_TONE,
            Self::PulseTrain => CarrierKind::PulseTrain.term(),
            Self::BinauralPair => PairKind::Binaural.term(),
            Self::MonauralPair => PairKind::Monaural.term(),
            Self::PulsedTone => auditorylab_core::vocabulary::PULSED_TONE,
        }
    }
    /// Stable command-line key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Tone => "tone",
            Self::PulseTrain => "pulse-train",
            Self::BinauralPair => "binaural-pair",
            Self::MonauralPair => "monaural-pair",
            Self::PulsedTone => "pulsed-tone",
        }
    }
    /// Construct one component without identity, metadata or playback effects.
    pub fn component(self) -> auditorylab_core::ComponentForm {
        use auditorylab_core::{PairKind, PulseShape, PulseSpec, PulseWidth};
        match self {
            Self::Tone => default_component(CarrierKind::Tone),
            Self::PulseTrain => default_component(CarrierKind::PulseTrain),
            Self::BinauralPair => default_pair(PairKind::Binaural),
            Self::MonauralPair => default_pair(PairKind::Monaural),
            Self::PulsedTone => {
                let mut form = default_component(CarrierKind::Tone);
                form.modulation = Some(Modulation::Pulsed {
                    pulse: PulseSpec {
                        rate_hz: finite_default(10.0),
                        width: PulseWidth::DutyCycle {
                            fraction: finite_default(0.5),
                        },
                        shape: PulseShape::RaisedCosine {
                            rise_seconds: finite_default(0.002),
                            fall_seconds: finite_default(0.002),
                        },
                    },
                });
                form
            }
        }
    }
}
/// A complete portable document with a fresh fallibly generated identity.
pub fn new_recipe(
    kind: CreationKind,
) -> Result<auditorylab_core::Recipe, crate::services::OperationError> {
    let mut draft = super::editing::Draft::new()?;
    draft
        .add_node(kind.component())
        .unwrap_or_else(|_| unreachable!("one constructed component fits an empty document"));
    Ok(draft.recipe()?)
}
/// Convert width intent atomically; invalid candidates are rejected without clamping.
pub fn convert_width(
    pulse: auditorylab_core::PulseSpec,
    duty: bool,
) -> Option<auditorylab_core::PulseSpec> {
    use auditorylab_core::PulseWidth;
    let hz = pulse.rate_hz.get();
    if hz <= 0.0 {
        return None;
    }
    let width = match (pulse.width, duty) {
        (PulseWidth::Duration { seconds }, true) => {
            let fraction = Finite::new(seconds.get() * hz)?;
            if !PulseWidth::valid_fraction(fraction.get()) {
                return None;
            }
            PulseWidth::DutyCycle { fraction }
        }
        (PulseWidth::DutyCycle { fraction }, false) => {
            if !PulseWidth::valid_fraction(fraction.get()) {
                return None;
            }
            let seconds = Finite::new(fraction.get() / hz)?;
            if seconds.get() <= 0.0 {
                return None;
            }
            PulseWidth::Duration { seconds }
        }
        _ => return Some(pulse),
    };
    Some(auditorylab_core::PulseSpec { width, ..pulse })
}
/// Initialize shaped edges from requested duration; realization remains a compilation concern.
pub fn set_pulse_shape(
    pulse: auditorylab_core::PulseSpec,
    shaped: bool,
) -> Option<auditorylab_core::PulseSpec> {
    use auditorylab_core::PulseShape;
    let shape = match (pulse.shape, shaped) {
        (PulseShape::Rectangular {}, true) => {
            let edge = Finite::new(pulse.requested_seconds() * 0.1)?;
            PulseShape::RaisedCosine {
                rise_seconds: edge,
                fall_seconds: edge,
            }
        }
        (PulseShape::RaisedCosine { .. }, false) => PulseShape::Rectangular {},
        _ => return Some(pulse),
    };
    Some(auditorylab_core::PulseSpec { shape, ..pulse })
}

#[cfg(test)]
mod tests {
    use super::CreationKind;
    #[test]
    fn construction_terms_match_the_domain_and_keep_the_canonical_keys() {
        let labels = CreationKind::ALL.map(|kind| kind.term().label);
        assert_eq!(
            labels,
            [
                "Steady tone",
                "Pulse train",
                "Binaural pair",
                "Monaural pair",
                "Pulsed tone (isochronic)"
            ]
        );
        for kind in CreationKind::ALL {
            assert_eq!(kind.term().label, kind.component().heading());
        }
        assert_eq!(CreationKind::PulsedTone.key(), "pulsed-tone");
        assert!(
            !CreationKind::PulseTrain
                .term()
                .phrase
                .contains("isochronic")
        );
    }
}
