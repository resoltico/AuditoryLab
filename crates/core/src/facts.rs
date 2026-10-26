//! Typed requested and realized generator quantities.
use crate::{Carrier, ComponentId, Ears, Modulation, SampleRate, Sound, Voice, VoiceId};
use serde::Serialize;
/// Requested rate and the rate represented by the renderer's phase increment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PhaseRate {
    /// Requested frequency in Hz.
    pub requested_hz: f64,
    /// Frequency in Hz of the actual integer phase increment.
    pub realized_hz: f64,
}
impl PhaseRate {
    #[expect(
        clippy::cast_precision_loss,
        reason = "Integer phase steps are rounded to binary64 for reported frequency; they are not used to reconstruct the renderer step"
    )]
    pub(crate) fn from_step(requested_hz: f64, step: u64, rate: SampleRate) -> Self {
        Self {
            requested_hz,
            realized_hz: step as f64 * f64::from(rate.hz()) / 18_446_744_073_709_551_616.0,
        }
    }
}
/// A pulse train's requested and realized quantities. Repetition uses
/// rational onset scheduling, not a rounded period, so the requested rate is
/// also the generated long-run rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PulseFacts {
    /// Configured rational repetition rate, in Hz.
    pub rate_hz: f64,
    /// Requested pulse length in seconds, as entered.
    pub requested_width_seconds: f64,
    /// Generated pulse length: the requested length rounded to whole frames.
    pub width_frames: u64,
    /// The rate the width was resolved at.
    pub sample_rate: SampleRate,
}
impl PulseFacts {
    /// Generated pulse length in seconds, derived from the frame count.
    #[must_use]
    pub fn realized_width_seconds(&self) -> f64 {
        self.sample_rate.seconds_in(self.width_frames)
    }
}
/// A component's carrier quantities.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CarrierFacts {
    /// Sine carrier.
    Tone(PhaseRate),
    /// A rectangular pulse train.
    Clicks(PulseFacts),
}
/// A component's envelope quantities.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModulationFacts {
    /// Sine amplitude modulation; depth zero is inactive.
    Am {
        /// Requested and realized modulation rate.
        rate: PhaseRate,
        /// Modulation depth in 0..1.
        depth: f64,
    },
    /// Periodic rectangular gating.
    Gate(PulseFacts),
}
/// Authored pair intent retained with its generated lanes, never reconstructed
/// by subtracting generated frequencies. Actual lane rates remain in CarrierFacts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PairFacts {
    /// Authored primary owner shared by both lanes.
    pub owner: ComponentId,
    /// Separate-ear or same-route construction.
    pub kind: crate::PairKind,
    /// Requested first/left carrier.
    pub first_hz: f64,
    /// Exact authored signed offset, independent of phase-step rounding.
    pub offset_hz: f64,
    /// Computed requested second/right carrier.
    pub second_hz: f64,
}
/// Validated component facts at a specific rendering rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ComponentFacts {
    /// Authored pair intent, absent for unpaired raw primitives.
    pub pair: Option<PairFacts>,
    /// Declared ear route.
    pub ears: Ears,
    /// Declared component peak bound in dBFS.
    pub peak_dbfs: f64,
    /// Carrier quantities.
    pub carrier: CarrierFacts,
    /// Optional envelope quantities.
    pub modulation: Option<ModulationFacts>,
}
impl ComponentFacts {
    pub(crate) fn from_voice(
        c: &Voice,
        v: &crate::synthesis::GeneratorVoice,
        rate: SampleRate,
    ) -> Self {
        let pulse = |rate_hz, width_seconds, frames| PulseFacts {
            rate_hz,
            requested_width_seconds: width_seconds,
            width_frames: frames,
            sample_rate: rate,
        };
        let carrier = match (c.carrier, v.carrier) {
            (Carrier::Tone { frequency_hz }, crate::synthesis::Carrier::Tone { step }) => {
                CarrierFacts::Tone(PhaseRate::from_step(frequency_hz, step, rate))
            }
            (
                Carrier::Clicks {
                    rate_hz,
                    width_seconds,
                },
                crate::synthesis::Carrier::Pulse(p),
            ) => CarrierFacts::Clicks(pulse(rate_hz, width_seconds, p.frames())),
            _ => unreachable!("validated carrier matches component"),
        };
        let modulation = match (c.modulation, v.modulation) {
            (
                Some(Modulation::Am { rate_hz, depth }),
                Some(crate::synthesis::Modulator::Am { step, .. }),
            ) => Some(ModulationFacts::Am {
                rate: PhaseRate::from_step(rate_hz, step, rate),
                depth,
            }),
            (
                Some(Modulation::Gate {
                    rate_hz,
                    width_seconds,
                }),
                Some(crate::synthesis::Modulator::Gate(p)),
            ) => Some(ModulationFacts::Gate(pulse(
                rate_hz,
                width_seconds,
                p.frames(),
            ))),
            (None, None) => None,
            _ => unreachable!("validated modulation matches component"),
        };
        Self {
            pair: None,
            ears: c.ears,
            peak_dbfs: c.peak_dbfs,
            carrier,
            modulation,
        }
    }
}
/// Attribution of a generator slot during a structural crossfade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotRole {
    /// Same generator continues, potentially gliding.
    Continuing,
    /// Generator fading away.
    Outgoing,
    /// Generator fading in.
    Incoming,
}
impl SlotRole {
    /// A short name for the role.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Continuing => "Continuing",
            Self::Outgoing => "Outgoing",
            Self::Incoming => "Incoming",
        }
    }
}
/// Actual generator quantities and routing gains for one generated frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceObservation {
    /// Caller component identity; crossfades can have two roles with the same identity.
    pub id: VoiceId,
    /// Slot attribution within the transition.
    pub role: SlotRole,
    /// Requested and generated component quantities.
    pub facts: ComponentFacts,
    /// Actual linear routing gains, left/right, at this frame.
    pub gains: [f64; 2],
}
/// Last-generated-frame state, independent of an output device or wall clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observation {
    /// Index of the last generated frame, not the next cursor position.
    pub frame: u64,
    /// Target currently generating or transitioning.
    pub active_revision: u64,
    /// Last target whose ramp finished; zero before the first settled target.
    pub settled_revision: u64,
    /// Whether the last generated frame precedes the end of a ramp.
    pub transitioning: bool,
    /// Fixed storage for every concurrent generator; unused entries are None.
    pub voices: [Option<VoiceObservation>; Observation::MAX_VOICES],
}
impl Observation {
    /// The most generators that run at once: every component of an outgoing
    /// target crossfading with every component of its replacement.
    pub const MAX_VOICES: usize = 2 * Sound::MAX_VOICES;
}
