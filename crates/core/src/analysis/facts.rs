//! Typed requested and realized generator quantities.
use crate::{
    Carrier, ChannelRouting, ComponentId, Modulation, SampleRate, Sound, SourceLane, VoiceId,
};
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
        reason = "Integer phase steps are rounded to binary64 for reported frequency; \
            they are not used to reconstruct the renderer step"
    )]
    pub(crate) fn from_step(requested_hz: f64, step: u64, rate: SampleRate) -> Self {
        Self {
            requested_hz,
            realized_hz: step as f64 * f64::from(rate.hz()) / 18_446_744_073_709_551_616.0,
        }
    }
}
/// Signed pair rate derived before rounding either carrier to a displayed frequency.
#[expect(
    clippy::cast_precision_loss,
    reason = "The exact integer phase-step difference is converted once for reported hertz; small \
        differences retain their full precision"
)]
pub(crate) fn pair_rate(first: u64, second: u64, rate: SampleRate) -> f64 {
    let magnitude =
        first.abs_diff(second) as f64 * f64::from(rate.hz()) / 18_446_744_073_709_551_616.0;
    if second < first {
        -magnitude
    } else {
        magnitude
    }
}
/// A pulse train's requested and realized quantities. Repetition uses
/// rational onset scheduling, not a rounded period, so the requested rate is
/// also the generated long-run rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PulseFacts {
    /// Configured rational repetition rate, in Hz.
    pub rate_hz: f64,
    /// Requested window in seconds; derived from fraction/rate in duty mode.
    pub requested_width_seconds: f64,
    /// Generated pulse length: the requested length rounded to whole frames.
    pub width_frames: u64,
    /// The rate the width was resolved at.
    pub sample_rate: SampleRate,
    /// Authored width intent, retaining its authoritative requested quantity.
    pub width: crate::PulseWidth,
    /// Requested edge shape and durations.
    pub shape: crate::PulseShape,
    /// Resolved rise and fall lengths, in frames.
    pub edge_frames: [u64; 2],
}
impl PulseFacts {
    /// Duty derived from window frames and the nominal period, not every integer onset interval.
    pub fn nominal_duty(&self) -> f64 {
        self.realized_width_seconds() * self.rate_hz
    }
    /// Generated pulse length in seconds, derived from the frame count.
    #[must_use]
    pub fn realized_width_seconds(&self) -> f64 {
        self.sample_rate.seconds_in(self.width_frames)
    }
}
/// A voice's carrier quantities.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CarrierFacts {
    /// Sine carrier.
    Tone(PhaseRate),
    /// A rectangular pulse train.
    PulseTrain(PulseFacts),
}
/// A voice's envelope quantities.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModulationFacts {
    /// Sine amplitude modulation; depth zero is inactive.
    Sinusoidal {
        /// Requested and realized modulation rate.
        rate: PhaseRate,
        /// Modulation depth in 0..1.
        depth: f64,
    },
    /// Periodic rectangular gating.
    Pulsed(PulseFacts),
}
/// Authored pair intent retained with its generated lanes, never reconstructed
/// by subtracting generated frequencies. Actual lane rates remain in `CarrierFacts`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PairFacts {
    /// Authored primary owner shared by both lanes.
    pub owner: ComponentId,
    /// Intended separate-ear or same-route construction.
    pub kind: crate::PairKind,
    /// Requested first/left carrier.
    pub first_hz: f64,
    /// Exact authored signed offset, independent of phase-step rounding.
    pub offset_hz: f64,
    /// Computed requested second/right carrier.
    pub second_hz: f64,
    /// Signed difference of compiled/current phase increments; unavailable for an incomplete pair
    /// check.
    pub realized_offset_hz: Option<f64>,
}
/// Validated voice facts at a specific rendering rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct VoiceFacts {
    /// Authored pair intent, absent for unpaired raw primitives.
    pub pair: Option<PairFacts>,
    /// Declared digital channel route.
    pub route: ChannelRouting,
    /// Declared voice peak bound in dBFS.
    pub peak_dbfs: f64,
    /// Carrier quantities.
    pub carrier: CarrierFacts,
    /// Optional envelope quantities.
    pub modulation: Option<ModulationFacts>,
}
impl VoiceFacts {
    pub(crate) fn from_lane(
        c: &SourceLane,
        v: &crate::synthesis::GeneratorLane,
        requested: Option<Modulation>,
        compiled: Option<crate::synthesis::Modulator>,
        rate: SampleRate,
    ) -> Self {
        let carrier = match (c.carrier, v.carrier) {
            (Carrier::Tone { frequency_hz }, crate::synthesis::Carrier::Tone { step }) => {
                CarrierFacts::Tone(PhaseRate::from_step(frequency_hz, step, rate))
            }
            (Carrier::PulseTrain { pulse: requested }, crate::synthesis::Carrier::Pulse(p)) => {
                CarrierFacts::PulseTrain(PulseFacts {
                    rate_hz: requested.rate_hz.get(),
                    requested_width_seconds: requested.requested_seconds(),
                    width_frames: p.frames(),
                    sample_rate: rate,
                    width: requested.width,
                    shape: requested.shape,
                    edge_frames: p.edges(),
                })
            }
            _ => unreachable!("validated carrier matches component"),
        };
        let modulation = match (requested, compiled) {
            (
                Some(Modulation::Sinusoidal { rate_hz, depth }),
                Some(crate::synthesis::Modulator::Sinusoidal { step, .. }),
            ) => Some(ModulationFacts::Sinusoidal {
                rate: PhaseRate::from_step(rate_hz.get(), step, rate),
                depth: depth.get(),
            }),
            (
                Some(Modulation::Pulsed { pulse: requested }),
                Some(crate::synthesis::Modulator::Pulsed(p)),
            ) => Some(ModulationFacts::Pulsed(PulseFacts {
                rate_hz: requested.rate_hz.get(),
                requested_width_seconds: requested.requested_seconds(),
                width_frames: p.frames(),
                sample_rate: rate,
                width: requested.width,
                shape: requested.shape,
                edge_frames: p.edges(),
            })),
            (None, None) => None,
            _ => unreachable!("validated modulation matches component"),
        };
        Self {
            pair: None,
            route: c.route,
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
    /// Caller voice identity; crossfades can have two roles with the same identity.
    pub id: VoiceId,
    /// Slot attribution within the transition.
    pub role: SlotRole,
    /// Requested and generated voice quantities.
    pub facts: VoiceFacts,
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
    /// The most generators that run at once: every voice of an outgoing
    /// target crossfading with every voice of its replacement.
    pub const MAX_VOICES: usize = 2 * Sound::MAX_VOICES;
}

crate::vocabulary::vocabulary! { SlotRole,"slot_role" {
    Continuing => ("continuing","Continuing","continuing"),
    Outgoing => ("outgoing","Outgoing","outgoing"),
    Incoming => ("incoming","Incoming","incoming"),
}}
