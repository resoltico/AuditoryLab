// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::signal::Signal;
use crate::synthesis::{self, Carrier as Generator, GeneratorVoice, Modulator, Pulse};
use crate::{ParameterId, SampleRate, VoiceId, numeric};
use core::fmt::{self, Display, Formatter};
use serde::{Deserialize, Serialize};

/// One carrier routed to one or both ears.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Voice {
    /// Ear routing.
    pub ears: Ears,
    /// Peak amplitude in dBFS.
    pub peak_dbfs: f64,
    /// Periodic sound source.
    pub carrier: Carrier,
    /// Optional amplitude modulation.
    pub modulation: Option<Modulation>,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ear {
    /// The left ear: the first channel of each frame.
    Left,
    /// The right ear: the second channel of each frame.
    Right,
}

/// A periodic change of a component's amplitude. Every modulation's gain
/// lies within `[0, 1]`, so it never raises the component's peak.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Modulation {
    /// Nominal gain follows `(1 + depth * sin(2π rate_hz t)) / (1 + depth)`,
    /// starting at phase zero. Validated gain is bounded by one; sampled
    /// values need not attain that bound.
    Am {
        /// Modulation frequency in hertz.
        rate_hz: f64,
        /// Modulation depth from 0 (none) to 1 (full).
        depth: f64,
    },
    /// Gain 1 during the first `width_seconds` of each `1 / rate_hz` period
    /// and 0 otherwise, the first period starting at time zero: the same
    /// pulse train as a click, gating the carrier on and off.
    Gate {
        /// Gates per second.
        rate_hz: f64,
        /// Length of each open gate in seconds.
        width_seconds: f64,
    },
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
        if !(peak_dbfs.is_finite() && peak_dbfs <= 0.0) {
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
                let step = frequency_step(ParameterId::AmRate, rate_hz, rate)?;
                if !(0.0..=1.0).contains(&depth) {
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
                rate_hz,
                width_seconds,
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
    synthesis::step(hz, rate.hz()).ok_or(VoiceError::Frequency {
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
    synthesis::pulse(width_seconds, rate_hz, rate.hz()).ok_or(VoiceError::PulseWidth {
        parameter: width_parameter,
        width_seconds,
        rate_hz,
        sample_rate_hz: rate.hz(),
    })
}

/// Why a sound cannot be rendered at a sample rate.
#[derive(Debug, Clone, PartialEq)]
pub enum SoundError {
    /// Authored identity, voice-budget or coupled pair arithmetic failure.
    Authored(crate::LoweringError),
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
    /// [`Signal::MAX_FRAMES`].
    Duration {
        /// The rejected duration.
        seconds: f64,
        /// The sample rate in frames per second.
        sample_rate_hz: u32,
    },
    /// `fade_seconds` is not finite and non-negative, or spans more than
    /// [`Signal::MAX_FRAMES`].
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
    /// A component has a value it cannot render.
    Component {
        /// The component's position, counting from 1.
        number: usize,
        /// What is wrong with it.
        error: VoiceError,
    },
    /// An ear's components add up to a peak above full scale.
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
            Self::PairCarrier { .. }
            | Self::Authored(crate::LoweringError::PairDifference { .. }) => {
                &[ParameterId::PairFirstFrequency, ParameterId::PairOffset]
            }
            Self::Authored(_) => &[],
            Self::Duration { .. } => &[ParameterId::Duration],
            Self::Fade { .. } => &[ParameterId::Fade],
            Self::FadesOverlap { .. } => &[ParameterId::Duration, ParameterId::Fade],
            Self::Component { error, .. } => error.parameters(),
            Self::Peak { .. } => &[],
        }
    }
}

/// Why a component cannot be rendered at a sample rate.
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

/// A model value in its displayed unit, with the unit's symbol.
struct Quantity(ParameterId, f64);

/// The longest length at a rate, in days.
struct Longest(u32);

impl Display for Longest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let days = numeric::frames_to_f64(Signal::MAX_FRAMES) / f64::from(self.0) / 86_400.0;
        write!(f, "{days:.1} days")
    }
}

/// A value as short text: scientific notation for magnitudes from 10¹² or
/// below 10⁻⁶, so a message never carries hundreds of digits.
struct Number(f64);

impl Display for Number {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let value = self.0;
        if value.is_finite() && value != 0.0 && (value.abs() >= 1e12 || value.abs() < 1e-6) {
            write!(f, "{value:e}")
        } else {
            write!(f, "{value}")
        }
    }
}

impl Display for Quantity {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let unit = self.0.descriptor().unit;
        let separator = if unit == crate::Unit::Percent {
            ""
        } else {
            " "
        };
        write!(
            f,
            "{} {}{separator}{}",
            self.0.descriptor().label,
            Number(unit.to_display(self.1)),
            unit.symbol()
        )
    }
}

impl Display for SoundError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authored(error) => error.fmt(f),
            Self::PairCarrier {
                id,
                first_hz,
                offset_hz,
                error,
            } => write!(
                f,
                "component {id}: first carrier {first_hz} Hz and signed offset {offset_hz} Hz: {error}"
            ),
            Self::Duration {
                seconds,
                sample_rate_hz,
            } => write!(
                f,
                "{} must last from one sample (1/{sample_rate_hz} s) to {} at {sample_rate_hz} Hz",
                Quantity(ParameterId::Duration, *seconds),
                Longest(*sample_rate_hz)
            ),
            Self::Fade {
                seconds,
                sample_rate_hz,
            } => write!(
                f,
                "{} must be from 0 s to {} at {sample_rate_hz} Hz",
                Quantity(ParameterId::Fade, *seconds),
                Longest(*sample_rate_hz)
            ),
            Self::FadesOverlap {
                fade_frames,
                frames,
                sample_rate_hz,
            } => {
                let seconds = |n: u64| numeric::frames_to_f64(n) / f64::from(*sample_rate_hz);
                write!(
                    f,
                    "a {} s fade at each edge does not fit in {} s; the two fades together must be shorter than the duration",
                    Number(seconds(*fade_frames)),
                    Number(seconds(*frames))
                )
            }
            Self::Component { number, error } => write!(f, "component {number}: {error}"),
            Self::Peak { ear, amplitude } => {
                let db = numeric::dbfs(*amplitude);
                let rounded = alloc::format!("{db:.2}");
                let level = if db > 0.0 && rounded == "0.00" {
                    alloc::format!("{db:.2e}")
                } else {
                    rounded
                };
                write!(
                    f,
                    "the components for the {} add up to a peak of {level} dBFS, above full scale; lower their peak levels",
                    ear.phrase()
                )
            }
        }
    }
}

impl Display for VoiceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frequency {
                parameter,
                hz,
                sample_rate_hz,
            } => write!(
                f,
                "{} must be finite, positive and below {} Hz, half the {sample_rate_hz} Hz sample rate",
                Quantity(*parameter, *hz),
                f64::from(*sample_rate_hz) / 2.0
            ),
            Self::Sideband {
                frequency_hz,
                rate_hz,
                sample_rate_hz,
            } => write!(
                f,
                "{} plus {} must stay below {} Hz, half the {sample_rate_hz} Hz sample rate",
                Quantity(ParameterId::ToneFrequency, *frequency_hz),
                Quantity(ParameterId::AmRate, *rate_hz),
                f64::from(*sample_rate_hz) / 2.0
            ),
            Self::PulseWidth {
                parameter,
                width_seconds,
                rate_hz,
                sample_rate_hz,
            } => write!(
                f,
                "{} must give 1 to {} frames at {sample_rate_hz} Hz and leave at least one closed frame in each 1/{} s period",
                Quantity(*parameter, *width_seconds),
                crate::Signal::MAX_FRAMES,
                Number(*rate_hz)
            ),
            Self::PeakDbfs { peak_dbfs } => write!(
                f,
                "{} must be finite and at most 0 dBFS",
                Quantity(ParameterId::PeakLevel, *peak_dbfs)
            ),
            Self::Depth { depth } => write!(
                f,
                "{} must be from 0% to 100%",
                Quantity(ParameterId::AmDepth, *depth)
            ),
        }
    }
}

impl core::error::Error for SoundError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Authored(error) => Some(error),
            Self::PairCarrier { error, .. } => Some(error),
            Self::Component { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl core::error::Error for VoiceError {}

impl Ears {
    const fn terms(self) -> (&'static str, &'static str) {
        match self {
            Self::Left => ("Left ear", "left ear"),
            Self::Right => ("Right ear", "right ear"),
            Self::Both => ("Both ears", "both ears"),
        }
    }
    /// Short route name for a heading or choice.
    #[must_use]
    pub const fn label(self) -> &'static str {
        self.terms().0
    }
    /// Route name within a sentence.
    #[must_use]
    pub const fn phrase(self) -> &'static str {
        self.terms().1
    }
}
impl Ear {
    const fn terms(self) -> (&'static str, &'static str) {
        match self {
            Self::Left => ("Left ear", "left ear"),
            Self::Right => ("Right ear", "right ear"),
        }
    }
    /// Short ear name for a heading or choice.
    #[must_use]
    pub const fn label(self) -> &'static str {
        self.terms().0
    }
    /// Ear name within a sentence.
    #[must_use]
    pub const fn phrase(self) -> &'static str {
        self.terms().1
    }
}

impl Display for Modulation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Am { rate_hz, depth } => {
                write!(f, "AM {rate_hz} Hz, depth {depth}")
            }
            Self::Gate {
                rate_hz,
                width_seconds,
            } => write!(f, "gate {rate_hz} Hz, {width_seconds} s open"),
        }
    }
}

impl Display for Voice {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.carrier {
            Carrier::Tone { frequency_hz } => write!(f, "tone {frequency_hz} Hz")?,
            Carrier::Clicks {
                rate_hz,
                width_seconds,
            } => write!(f, "clicks {rate_hz} Hz, {width_seconds} s wide")?,
        }
        write!(f, ", peak {} dBFS, {}", self.peak_dbfs, self.ears.phrase())?;
        if let Some(modulation) = self.modulation {
            write!(f, ", {modulation}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
