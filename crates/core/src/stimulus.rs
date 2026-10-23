// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::signal::{Envelope, Signal};
use crate::synthesis::{self, Carrier as Generator, Modulator, Pulse, Voice};
use crate::{ComponentId, ParameterId, SampleRate, Target, TargetCheck, numeric};
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};
use serde::{Deserialize, Serialize};

/// A sound for the two ears, described in physical units and independent
/// of any sample rate.
///
/// Each ear hears the sum of the components routed to it, multiplied by an
/// envelope that fades in over `fade_seconds`, holds, and fades out over
/// `fade_seconds`. A stimulus without components is silence.
#[derive(Debug, Clone, PartialEq)]
pub struct Stimulus {
    /// Length of the stimulus.
    pub duration_seconds: f64,
    /// Length of the raised-cosine fade at each edge; zero disables fading.
    pub fade_seconds: f64,
    /// The components, summed in this order.
    pub components: Vec<Component>,
}

/// One carrier routed to one or both ears.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Component {
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
    Click {
        /// Pulses per second.
        rate_hz: f64,
        /// Pulse length in seconds.
        width_seconds: f64,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DocumentComponent {
    /// A sine wave starting at phase zero.
    Tone {
        /// The ears that hear this component.
        ears: Ears,
        /// Frequency in hertz.
        frequency_hz: f64,
        /// Peak amplitude in dBFS.
        peak_dbfs: f64,
        /// Optional amplitude modulation.
        #[serde(skip_serializing_if = "Option::is_none")]
        modulation: Option<Modulation>,
    },
    /// Unipolar rectangular pulses, the first starting at time zero.
    Click {
        /// The ears that hear this component.
        ears: Ears,
        /// Pulses per second.
        rate_hz: f64,
        /// Length of each pulse in seconds.
        width_seconds: f64,
        /// Pulse height in dBFS.
        peak_dbfs: f64,
        /// Optional amplitude modulation.
        #[serde(skip_serializing_if = "Option::is_none")]
        modulation: Option<Modulation>,
    },
}

impl From<Component> for DocumentComponent {
    fn from(value: Component) -> Self {
        let Component {
            ears,
            peak_dbfs,
            carrier,
            modulation,
        } = value;
        match carrier {
            Carrier::Tone { frequency_hz } => Self::Tone {
                ears,
                frequency_hz,
                peak_dbfs,
                modulation,
            },
            Carrier::Click {
                rate_hz,
                width_seconds,
            } => Self::Click {
                ears,
                rate_hz,
                width_seconds,
                peak_dbfs,
                modulation,
            },
        }
    }
}

impl From<DocumentComponent> for Component {
    fn from(value: DocumentComponent) -> Self {
        match value {
            DocumentComponent::Tone {
                ears,
                peak_dbfs,
                modulation,
                frequency_hz,
            } => Self {
                ears,
                peak_dbfs,
                modulation,
                carrier: Carrier::Tone { frequency_hz },
            },
            DocumentComponent::Click {
                ears,
                peak_dbfs,
                modulation,
                rate_hz,
                width_seconds,
            } => Self {
                ears,
                peak_dbfs,
                modulation,
                carrier: Carrier::Click {
                    rate_hz,
                    width_seconds,
                },
            },
        }
    }
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
    Sinusoidal {
        /// Modulation frequency in hertz.
        rate_hz: f64,
        /// Modulation depth from 0 (none) to 1 (full).
        depth: f64,
    },
    /// Gain 1 during the first `width_seconds` of each `1 / rate_hz` period
    /// and 0 otherwise, the first period starting at time zero: the same
    /// pulse train as a click, gating the carrier on and off.
    Rectangular {
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

impl Component {
    /// Check every value at `rate` and build the generator.
    pub(crate) fn voice(&self, rate: SampleRate) -> Result<Voice, ComponentError> {
        let peak_dbfs = self.peak_dbfs;
        if !(peak_dbfs.is_finite() && peak_dbfs <= 0.0) {
            return Err(ComponentError::PeakDbfs { peak_dbfs });
        }
        let carrier = match self.carrier {
            Carrier::Tone { frequency_hz, .. } => Generator::Tone {
                step: frequency_step(ParameterId::ToneFrequency, frequency_hz, rate)?,
            },
            Carrier::Click {
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
            Some(Modulation::Sinusoidal { rate_hz, depth }) => {
                let step = frequency_step(ParameterId::AmplitudeModulationRate, rate_hz, rate)?;
                if !(0.0..=1.0).contains(&depth) {
                    return Err(ComponentError::Depth { depth });
                }
                if let Carrier::Tone { frequency_hz } = self.carrier
                    && frequency_hz + rate_hz >= rate.nyquist_hz()
                {
                    return Err(ComponentError::Sideband {
                        frequency_hz,
                        rate_hz,
                        sample_rate_hz: rate.hz(),
                    });
                }
                Some(Modulator::Sinusoidal { step, depth })
            }
            Some(Modulation::Rectangular {
                rate_hz,
                width_seconds,
            }) => Some(Modulator::Rectangular(pulse_train(
                (ParameterId::GateRate, ParameterId::GateWidth),
                rate_hz,
                width_seconds,
                rate,
            )?)),
        };
        let ears = self.ears;
        Ok(Voice {
            left: ears.hears(Ear::Left),
            right: ears.hears(Ear::Right),
            amplitude: numeric::amplitude(peak_dbfs),
            carrier,
            modulation,
        })
    }
}

fn frequency_step(
    parameter: ParameterId,
    hz: f64,
    rate: SampleRate,
) -> Result<u64, ComponentError> {
    synthesis::step(hz, rate.hz()).ok_or(ComponentError::Frequency {
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
) -> Result<Pulse, ComponentError> {
    frequency_step(rate_parameter, rate_hz, rate)?;
    synthesis::pulse(width_seconds, rate_hz, rate.hz()).ok_or(ComponentError::PulseWidth {
        parameter: width_parameter,
        width_seconds,
        rate_hz,
        sample_rate_hz: rate.hz(),
    })
}

impl Stimulus {
    /// The most components a stimulus may have. It bounds the work done
    /// for every frame, which playback does on the device's real-time thread.
    pub const MAX_COMPONENTS: usize = 16;

    /// Check the stimulus at a sample rate and build its seekable signal.
    ///
    /// Durations become whole frames at that rate. Each ear's components must
    /// add up to a peak of at most full scale, so no sample ever needs
    /// clipping. Components get the identities [`ComponentId::numbered`]
    /// gives their positions.
    ///
    /// # Errors
    /// Returns the first [`StimulusError`] found, checking the duration, the
    /// fade, the number of components, each component in order (level,
    /// carrier, modulation), and then each ear's peak.
    pub fn signal(&self, rate: SampleRate) -> Result<Signal, StimulusError> {
        let envelope = Envelope::resolve(rate, self.duration_seconds, self.fade_seconds)?;
        Ok(Signal::new(self.target(rate)?, envelope))
    }

    /// Check the components at a sample rate, without a duration or fade,
    /// keeping every component's own result. Components get the identities
    /// [`ComponentId::numbered`] gives their positions.
    #[must_use]
    pub fn target_check(&self, rate: SampleRate) -> TargetCheck {
        TargetCheck::numbered(rate, &self.components)
    }

    /// Validate the components as a target at a sample rate.
    ///
    /// # Errors
    /// Returns what [`Stimulus::target_check`] finds first.
    pub fn target(&self, rate: SampleRate) -> Result<Target, StimulusError> {
        self.target_check(rate).into_result()
    }

    /// Return whether any component uses a separate ear route.
    ///
    /// This describes routing, not waveform equality: matching components
    /// on separate left and right routes can still produce identical ears.
    /// A stimulus without components uses no separate ear route.
    #[must_use]
    pub fn uses_separate_ears(&self) -> bool {
        self.components
            .iter()
            .any(|component| component.ears != Ears::Both)
    }

    /// The stimulus's quantities that are not finite numbers: the duration
    /// and fade, then each component's by position.
    pub fn nonfinite(&self) -> impl Iterator<Item = (Option<usize>, ParameterId)> + '_ {
        [ParameterId::Duration, ParameterId::Fade]
            .into_iter()
            .filter(|id| id.read_envelope(self).is_some_and(|v| !v.is_finite()))
            .map(|id| (None, id))
            .chain(
                self.components
                    .iter()
                    .enumerate()
                    .flat_map(|(index, c)| c.nonfinite().map(move |id| (Some(index), id))),
            )
    }
}

/// Why a stimulus cannot be rendered at a sample rate.
#[derive(Debug, Clone, PartialEq)]
pub enum StimulusError {
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
        /// The stimulus length.
        frames: u64,
        /// The sample rate the lengths were resolved at.
        sample_rate_hz: u32,
    },
    /// Two components of a target share an identity.
    DuplicateComponentId {
        /// The duplicated identity.
        id: ComponentId,
    },
    /// More than [`Stimulus::MAX_COMPONENTS`] components.
    TooManyComponents {
        /// The number of components.
        count: usize,
    },
    /// A component has a value it cannot render.
    Component {
        /// The component's position, counting from 1.
        number: usize,
        /// What is wrong with it.
        error: ComponentError,
    },
    /// An ear's components add up to a peak above full scale.
    Peak {
        /// The ear.
        ear: Ear,
        /// The sum of the peak amplitudes reaching it.
        amplitude: f64,
    },
}

impl StimulusError {
    /// Every quantity the error concerns. Structural, identity and mix errors
    /// concern the whole stimulus and name none.
    #[must_use]
    pub const fn parameters(&self) -> &'static [ParameterId] {
        match self {
            Self::Duration { .. } => &[ParameterId::Duration],
            Self::Fade { .. } => &[ParameterId::Fade],
            Self::FadesOverlap { .. } => &[ParameterId::Duration, ParameterId::Fade],
            Self::Component { error, .. } => error.parameters(),
            Self::DuplicateComponentId { .. }
            | Self::TooManyComponents { .. }
            | Self::Peak { .. } => &[],
        }
    }
}

/// Why a component cannot be rendered at a sample rate.
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentError {
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

impl ComponentError {
    /// Every quantity the error concerns. A constraint coupling two
    /// quantities names both.
    #[must_use]
    pub const fn parameters(&self) -> &'static [ParameterId] {
        match self {
            Self::Frequency { parameter, .. } => match parameter {
                ParameterId::ClickRate => &[ParameterId::ClickRate],
                ParameterId::AmplitudeModulationRate => &[ParameterId::AmplitudeModulationRate],
                ParameterId::GateRate => &[ParameterId::GateRate],
                _ => &[ParameterId::ToneFrequency],
            },
            Self::Sideband { .. } => &[
                ParameterId::ToneFrequency,
                ParameterId::AmplitudeModulationRate,
            ],
            Self::PulseWidth { parameter, .. } => match parameter {
                ParameterId::GateWidth => &[ParameterId::GateRate, ParameterId::GateWidth],
                _ => &[ParameterId::ClickRate, ParameterId::ClickWidth],
            },
            Self::PeakDbfs { .. } => &[ParameterId::PeakLevel],
            Self::Depth { .. } => &[ParameterId::ModulationDepth],
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

impl Display for StimulusError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateComponentId { id } => {
                write!(f, "two components share the identity {id}")
            }
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
            Self::TooManyComponents { count } => write!(
                f,
                "{count} components exceed the limit of {}",
                Stimulus::MAX_COMPONENTS
            ),
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
                    "the components for the {ear} add up to a peak of {level} dBFS, above full scale; lower their peak levels"
                )
            }
        }
    }
}

impl Display for ComponentError {
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
                Quantity(ParameterId::AmplitudeModulationRate, *rate_hz),
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
                Quantity(ParameterId::ModulationDepth, *depth)
            ),
        }
    }
}

impl core::error::Error for StimulusError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Component { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl core::error::Error for ComponentError {}

impl Display for Ears {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Left => "left ear",
            Self::Right => "right ear",
            Self::Both => "both ears",
        })
    }
}

impl Display for Ear {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Left => "left ear",
            Self::Right => "right ear",
        })
    }
}

impl Display for Modulation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sinusoidal { rate_hz, depth } => {
                write!(f, "sinusoidal modulation {rate_hz} Hz, depth {depth}")
            }
            Self::Rectangular {
                rate_hz,
                width_seconds,
            } => write!(
                f,
                "rectangular modulation {rate_hz} Hz, {width_seconds} s open"
            ),
        }
    }
}

impl Display for Component {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.carrier {
            Carrier::Tone { frequency_hz } => write!(f, "tone {frequency_hz} Hz")?,
            Carrier::Click {
                rate_hz,
                width_seconds,
            } => write!(f, "clicks {rate_hz} Hz, {width_seconds} s wide")?,
        }
        write!(f, ", peak {} dBFS, {}", self.peak_dbfs, self.ears)?;
        if let Some(modulation) = self.modulation {
            write!(f, ", {modulation}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
