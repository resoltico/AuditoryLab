//! Typed requested and realized generator quantities.
#![allow(
    clippy::cast_precision_loss,
    reason = "phase increments and bounded widths are presented as binary64 quantities"
)]
use crate::{Carrier, Component, ComponentError, Ears, Modulation, SampleRate};
/// Requested rate and the rate represented by the renderer's phase increment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhaseRate {
    /// Requested frequency in Hz.
    pub requested_hz: f64,
    /// Frequency in Hz of the actual integer phase increment.
    pub realized_hz: f64,
}
impl PhaseRate {
    pub(crate) fn from_step(requested_hz: f64, step: u64, rate: SampleRate) -> Self {
        Self {
            requested_hz,
            realized_hz: step as f64 * f64::from(rate.hz()) / 18_446_744_073_709_551_616.0,
        }
    }
}
/// Pulse repetition uses rational onset scheduling, not a rounded period.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PulseFacts {
    /// Configured rational repetition rate, in Hz.
    pub rate_hz: f64,
    /// Requested width in seconds.
    pub width_seconds: f64,
    /// Realized width, rounded to whole frames.
    pub width_frames: u64,
}
/// A component's carrier quantities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CarrierFacts {
    /// Sinusoidal carrier.
    Tone(PhaseRate),
    /// Rectangular click train.
    Clicks(PulseFacts),
}
/// A component's envelope quantities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModulationFacts {
    /// Sinusoidal amplitude modulation; depth zero is inactive.
    Sinusoidal {
        /// Requested and realized modulation rate.
        rate: PhaseRate,
        /// Modulation depth in 0..1.
        depth: f64,
    },
    /// Rectangular gating.
    Gate(PulseFacts),
}
/// Validated component facts at a specific rendering rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComponentFacts {
    /// Declared ear route.
    pub ears: Ears,
    /// Declared component peak bound in dBFS.
    pub peak_dbfs: f64,
    /// Carrier quantities.
    pub carrier: CarrierFacts,
    /// Optional envelope quantities.
    pub modulation: Option<ModulationFacts>,
}
impl Component {
    /// Resolve requested/realized generator facts with the same validation as rendering.
    ///
    /// # Errors
    /// Returns the component validation error for this sample rate.
    pub fn facts(&self, rate: SampleRate) -> Result<ComponentFacts, ComponentError> {
        self.voice(rate)
            .map(|voice| ComponentFacts::from_voice(self, &voice, rate))
    }
}
impl ComponentFacts {
    pub(crate) fn from_voice(c: &Component, v: &crate::synthesis::Voice, rate: SampleRate) -> Self {
        let pulse = |rate_hz, width_seconds, frames| PulseFacts {
            rate_hz,
            width_seconds,
            width_frames: frames,
        };
        let carrier = match (c.carrier, v.carrier) {
            (Carrier::Tone { frequency_hz }, crate::synthesis::Carrier::Tone { step }) => {
                CarrierFacts::Tone(PhaseRate::from_step(frequency_hz, step, rate))
            }
            (
                Carrier::Click {
                    rate_hz,
                    width_seconds,
                },
                crate::synthesis::Carrier::Pulse(p),
            ) => CarrierFacts::Clicks(pulse(rate_hz, width_seconds, p.frames())),
            _ => unreachable!("validated carrier matches component"),
        };
        let modulation = match (c.modulation, v.modulation) {
            (
                Some(Modulation::Sinusoidal { rate_hz, depth }),
                Some(crate::synthesis::Modulator::Sinusoidal { step, .. }),
            ) => Some(ModulationFacts::Sinusoidal {
                rate: PhaseRate::from_step(rate_hz, step, rate),
                depth,
            }),
            (
                Some(Modulation::Rectangular {
                    rate_hz,
                    width_seconds,
                }),
                Some(crate::synthesis::Modulator::Rectangular(p)),
            ) => Some(ModulationFacts::Gate(pulse(
                rate_hz,
                width_seconds,
                p.frames(),
            ))),
            (None, None) => None,
            _ => unreachable!("validated modulation matches component"),
        };
        Self {
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
/// Actual generator quantities and routing gains for one generated frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceObservation {
    /// Caller component identity; crossfades can have two roles with the same identity.
    pub id: u32,
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
    /// Fixed storage for up to 32 concurrent generators; unused entries are None.
    pub voices: [Option<VoiceObservation>; 32],
}
