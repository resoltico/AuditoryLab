// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::synthesis::{self, Carrier as Generator, GeneratorVoice, Modulator, Pulse};
use crate::{Modulation, ParameterId, SampleRate, VoiceId, numeric};
use core::fmt::{self, Display, Formatter};
use serde::{Deserialize, Serialize};

/// One carrier routed to one or both ears.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Voice {
    /// Ear routing.
    pub(crate) ears: Ears,
    /// Peak amplitude in dBFS.
    pub(crate) peak_dbfs: f64,
    /// Periodic sound source.
    pub(crate) carrier: Carrier,
    /// Optional amplitude modulation.
    pub(crate) modulation: Option<Modulation>,
}
/// A periodic sound source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Carrier {
    /// A sine wave.
    Tone {
        /// Frequency in hertz.
        frequency_hz: f64,
    },
    /// Unipolar rectangular pulses.
    Clicks {
        /// Pulses per second.
        rate_hz: f64,
        /// Pulse length in seconds.
        width_seconds: f64,
    },
}
/// Which ears hear a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ears {
    /// The left ear only.
    Left,
    /// The right ear only.
    Right,
    /// The identical signal in both ears.
    Both,
}

/// One ear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ear {
    /// The left ear: the first channel of each frame.
    Left,
    /// The right ear: the second channel of each frame.
    Right,
}

impl Ears {
    const fn hears(self, ear: Ear) -> bool {
        matches!(
            (self, ear),
            (Self::Both, _) | (Self::Left, Ear::Left) | (Self::Right, Ear::Right)
        )
    }
}

impl Voice {
    /// Check every value at `rate` and build the generator.
    pub(crate) fn voice(&self, rate: SampleRate) -> Result<GeneratorVoice, VoiceError> {
        let peak_dbfs = self.peak_dbfs;
        if crate::validity::check_value(ParameterId::PeakLevel, peak_dbfs).is_err() {
            return Err(VoiceError::PeakDbfs { peak_dbfs });
        }
        let carrier = match self.carrier {
            Carrier::Tone { frequency_hz, .. } => Generator::Tone {
                step: frequency_step(ParameterId::ToneFrequency, frequency_hz, rate)?,
            },
            Carrier::Clicks {
                rate_hz,
                width_seconds,
            } => Generator::Pulse(pulse_train(
                (ParameterId::ClickRate, ParameterId::ClickWidth),
                rate_hz,
                width_seconds,
                rate,
            )?),
        };
        let modulation = match self.modulation {
            None => None,
            Some(Modulation::Am { rate_hz, depth }) => {
                let (rate_hz, depth) = (rate_hz.get(), depth.get());
                let step = frequency_step(ParameterId::AmRate, rate_hz, rate)?;
                if crate::validity::check_value(ParameterId::AmDepth, depth).is_err() {
                    return Err(VoiceError::Depth { depth });
                }
                if let Carrier::Tone { frequency_hz } = self.carrier
                    && depth > 0.0
                    && frequency_hz + rate_hz >= rate.nyquist_hz()
                {
                    return Err(VoiceError::Sideband {
                        frequency_hz,
                        rate_hz,
                        sample_rate_hz: rate.hz(),
                    });
                }
                Some(Modulator::Am { step, depth })
            }
            Some(Modulation::Gate {
                rate_hz,
                width_seconds,
            }) => Some(Modulator::Gate(pulse_train(
                (ParameterId::GateRate, ParameterId::GateWidth),
                rate_hz.get(),
                width_seconds.get(),
                rate,
            )?)),
        };
        let ears = self.ears;
        Ok(GeneratorVoice {
            left: ears.hears(Ear::Left),
            right: ears.hears(Ear::Right),
            amplitude: numeric::amplitude(peak_dbfs),
            carrier,
            modulation,
        })
    }
}

fn frequency_step(parameter: ParameterId, hz: f64, rate: SampleRate) -> Result<u64, VoiceError> {
    crate::validity::check_value(parameter, hz)
        .ok()
        .and_then(|()| synthesis::step(hz, rate.hz()))
        .ok_or(VoiceError::Frequency {
            parameter,
            hz,
            sample_rate_hz: rate.hz(),
        })
}
fn pulse_train(
    (rate_parameter, width_parameter): (ParameterId, ParameterId),
    rate_hz: f64,
    width_seconds: f64,
    rate: SampleRate,
) -> Result<Pulse, VoiceError> {
    frequency_step(rate_parameter, rate_hz, rate)?;
    crate::validity::check_value(width_parameter, width_seconds)
        .ok()
        .and_then(|()| synthesis::pulse(width_seconds, rate_hz, rate.hz()))
        .ok_or(VoiceError::PulseWidth {
            parameter: width_parameter,
            width_seconds,
            rate_hz,
            sample_rate_hz: rate.hz(),
        })
}

/// Why a sound cannot be rendered at a sample rate.
#[derive(Debug, Clone, PartialEq)]
pub enum SoundError {
    /// A coupled pair arithmetic failure.
    Pair(crate::PairError),
    /// A pair carrier is invalid at this rate; both coupled inputs are named.
    PairCarrier {
        /// Authored owner, including which lane failed.
        id: VoiceId,
        /// Requested first carrier.
        first_hz: f64,
        /// Requested signed offset.
        offset_hz: f64,
        /// Underlying rate-dependent carrier failure.
        error: VoiceError,
    },
    /// `duration_seconds` is not finite, gives no frame, or spans more than
    /// [`crate::Signal::MAX_FRAMES`].
    Duration {
        /// The rejected duration.
        seconds: f64,
        /// The sample rate in frames per second.
        sample_rate_hz: u32,
    },
    /// `fade_seconds` is not finite and non-negative, or spans more than
    /// [`crate::Signal::MAX_FRAMES`].
    Fade {
        /// The rejected fade.
        seconds: f64,
        /// The sample rate in frames per second.
        sample_rate_hz: u32,
    },
    /// The two fades leave no frame between them.
    FadesOverlap {
        /// Fade length at each edge.
        fade_frames: u64,
        /// The sound length.
        frames: u64,
        /// The sample rate the lengths were resolved at.
        sample_rate_hz: u32,
    },
    /// A generator lane has a value it cannot render.
    Voice {
        /// The failing generator's authored owner and lane.
        id: crate::VoiceId,
        /// What is wrong with it.
        error: VoiceError,
    },
    /// An ear's voices add up to a peak above full scale.
    Peak {
        /// The ear.
        ear: Ear,
        /// The sum of the peak amplitudes reaching it.
        amplitude: f64,
    },
}

impl SoundError {
    /// Every quantity the error concerns. Structural, identity and mix errors
    /// concern the whole sound and name none.
    #[must_use]
    pub const fn parameters(&self) -> &'static [ParameterId] {
        match self {
            Self::PairCarrier { .. } | Self::Pair(_) => {
                &[ParameterId::PairFirstFrequency, ParameterId::PairOffset]
            }

            Self::Duration { .. } => &[ParameterId::Duration],
            Self::Fade { .. } => &[ParameterId::Fade],
            Self::FadesOverlap { .. } => &[ParameterId::Duration, ParameterId::Fade],
            Self::Voice { error, .. } => error.parameters(),
            Self::Peak { .. } => &[],
        }
    }
}

/// Why a generator lane cannot be rendered at a sample rate.
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceError {
    /// A frequency is not finite and positive below half the sample rate.
    Frequency {
        /// The quantity holding it.
        parameter: ParameterId,
        /// The rejected frequency in hertz.
        hz: f64,
        /// The sample rate in frames per second.
        sample_rate_hz: u32,
    },
    /// A tone's upper modulation sideband reaches half the sample rate.
    Sideband {
        /// The tone frequency in hertz.
        frequency_hz: f64,
        /// The modulation frequency in hertz.
        rate_hz: f64,
        /// The sample rate in frames per second.
        sample_rate_hz: u32,
    },
    /// A pulse width lies outside the signal frame limit or leaves no closed frame.
    PulseWidth {
        /// The width quantity holding it.
        parameter: ParameterId,
        /// The rejected width in seconds.
        width_seconds: f64,
        /// Pulses per second.
        rate_hz: f64,
        /// The sample rate in frames per second.
        sample_rate_hz: u32,
    },
    /// The peak is not finite or is above full scale.
    PeakDbfs {
        /// The rejected peak in dBFS.
        peak_dbfs: f64,
    },
    /// The modulation depth is outside 0 to 1.
    Depth {
        /// The rejected depth.
        depth: f64,
    },
}

impl VoiceError {
    /// Every quantity the error concerns. A constraint coupling two
    /// quantities names both.
    #[must_use]
    pub const fn parameters(&self) -> &'static [ParameterId] {
        match self {
            Self::Frequency { parameter, .. } => match parameter {
                ParameterId::ClickRate => &[ParameterId::ClickRate],
                ParameterId::AmRate => &[ParameterId::AmRate],
                ParameterId::GateRate => &[ParameterId::GateRate],
                _ => &[ParameterId::ToneFrequency],
            },
            Self::Sideband { .. } => &[ParameterId::ToneFrequency, ParameterId::AmRate],
            Self::PulseWidth { parameter, .. } => match parameter {
                ParameterId::GateWidth => &[ParameterId::GateRate, ParameterId::GateWidth],
                _ => &[ParameterId::ClickRate, ParameterId::ClickWidth],
            },
            Self::PeakDbfs { .. } => &[ParameterId::PeakLevel],
            Self::Depth { .. } => &[ParameterId::AmDepth],
        }
    }
}

impl Display for SoundError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pair(_) => "sound.pair",
            Self::PairCarrier { .. } => "sound.pair_carrier",
            Self::Duration { .. } => "sound.duration",
            Self::Fade { .. } => "sound.fade",
            Self::FadesOverlap { .. } => "sound.fades_overlap",
            Self::Voice { .. } => "sound.voice",
            Self::Peak { .. } => "sound.peak",
        })
    }
}
impl Display for VoiceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Frequency { .. } => "voice.frequency",
            Self::Sideband { .. } => "voice.sideband",
            Self::PulseWidth { .. } => "voice.pulse_width",
            Self::PeakDbfs { .. } => "voice.peak_dbfs",
            Self::Depth { .. } => "voice.depth",
        })
    }
}

impl core::error::Error for SoundError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Pair(error) => Some(error),
            Self::PairCarrier { error, .. } => Some(error),
            Self::Voice { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl core::error::Error for VoiceError {}

crate::vocabulary::vocabulary! { Ears,"ears" {
    Both => ("both","Both ears","both ears"),
    Left => ("left","Left ear","left ear"),
    Right => ("right","Right ear","right ear"),
}}

crate::vocabulary::vocabulary! { Ear,"ear" {
    Left => (crate::Ears::Left.key(),crate::Ears::Left.label(),crate::Ears::Left.phrase()),
    Right => (crate::Ears::Right.key(),crate::Ears::Right.label(),crate::Ears::Right.phrase()),
}}

#[cfg(test)]
mod tests;
