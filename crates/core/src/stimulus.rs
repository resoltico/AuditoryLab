// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::signal::{Envelope, Signal};
use crate::synthesis::{self, Carrier as Generator, Modulator, Pulse, Voice};
use crate::{SampleRate, numeric};
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};
use serde::Deserialize;

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
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
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
        modulation: Option<Modulation>,
    },
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
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
    fn voice(&self, rate: SampleRate) -> Result<Voice, ComponentError> {
        let peak_dbfs = self.peak_dbfs;
        if !(peak_dbfs.is_finite() && peak_dbfs <= 0.0) {
            return Err(ComponentError::PeakDbfs { peak_dbfs });
        }
        let carrier = match self.carrier {
            Carrier::Tone { frequency_hz, .. } => Generator::Tone {
                step: frequency_step(Parameter::Frequency, frequency_hz, rate)?,
            },
            Carrier::Click {
                rate_hz,
                width_seconds,
            } => Generator::Pulse(pulse_train(
                (Parameter::Rate, Parameter::Width),
                rate_hz,
                width_seconds,
                rate,
            )?),
        };
        let modulation = match self.modulation {
            None => None,
            Some(Modulation::Sinusoidal { rate_hz, depth }) => {
                let step = frequency_step(Parameter::ModulationRate, rate_hz, rate)?;
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
                (Parameter::ModulationRate, Parameter::ModulationWidth),
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

fn frequency_step(parameter: Parameter, hz: f64, rate: SampleRate) -> Result<u64, ComponentError> {
    synthesis::step(hz, rate.hz()).ok_or(ComponentError::Frequency {
        parameter,
        hz,
        sample_rate_hz: rate.hz(),
    })
}
fn pulse_train(
    (rate_parameter, width_parameter): (Parameter, Parameter),
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

    /// Check the stimulus at a sample rate and build its session.
    ///
    /// Durations become whole frames at that rate. Each ear's components must
    /// add up to a peak of at most full scale, so no sample ever needs
    /// clipping.
    ///
    /// # Errors
    /// Returns the first [`StimulusError`] found, checking the duration, the
    /// fade, the number of components, each component in order (level,
    /// carrier, modulation), and then each ear's peak.
    pub fn signal(&self, rate: SampleRate) -> Result<Signal, StimulusError> {
        let frames = rate
            .frames_in(self.duration_seconds)
            .filter(|frames| *frames > 0)
            .ok_or(StimulusError::Duration {
                seconds: self.duration_seconds,
                sample_rate_hz: rate.hz(),
            })?;
        let fade_frames = rate
            .frames_in(self.fade_seconds)
            .ok_or(StimulusError::Fade {
                seconds: self.fade_seconds,
                sample_rate_hz: rate.hz(),
            })?;
        if fade_frames > (frames - 1) / 2 {
            return Err(StimulusError::FadesOverlap {
                fade_frames,
                frames,
            });
        }
        if self.components.len() > Self::MAX_COMPONENTS {
            return Err(StimulusError::TooManyComponents {
                count: self.components.len(),
            });
        }
        let mut voices = Vec::with_capacity(self.components.len());
        for (index, component) in self.components.iter().enumerate() {
            let voice = component
                .voice(rate)
                .map_err(|error| StimulusError::Component {
                    number: index + 1,
                    error,
                })?;
            voices.push(voice);
        }
        // Summed in rendering order, so that no rendered sample can exceed it.
        let mut peak = [0.0; 2];
        for voice in &voices {
            if voice.left {
                peak[0] += voice.amplitude;
            }
            if voice.right {
                peak[1] += voice.amplitude;
            }
        }
        for (ear, amplitude) in [(Ear::Left, peak[0]), (Ear::Right, peak[1])] {
            if amplitude > 1.0 {
                return Err(StimulusError::Peak { ear, amplitude });
            }
        }
        Ok(Signal::new(
            rate,
            Envelope::new(frames, fade_frames),
            voices,
            peak,
        ))
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

/// A carrier or modulation parameter identified independently of diagnostic wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parameter {
    /// Tone carrier frequency_hz.
    Frequency,
    /// Click carrier rate_hz.
    Rate,
    /// Click carrier width_seconds.
    Width,
    /// Sinusoidal or rectangular modulation rate_hz.
    ModulationRate,
    /// Rectangular modulation width_seconds.
    ModulationWidth,
}
impl Display for Parameter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Frequency => "frequency_hz",
            Self::Rate => "rate_hz",
            Self::Width => "width_seconds",
            Self::ModulationRate => "modulation rate_hz",
            Self::ModulationWidth => "modulation width_seconds",
        })
    }
}

/// Why a component cannot be rendered at a sample rate.
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentError {
    /// A frequency is not finite and positive below half the sample rate.
    Frequency {
        /// The parameter holding it.
        parameter: Parameter,
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
    /// A pulse rounds to no open frame or leaves no closed frame.
    PulseWidth {
        /// The parameter holding it.
        parameter: Parameter,
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

impl Display for StimulusError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duration {
                seconds,
                sample_rate_hz,
            } => write!(
                f,
                "duration_seconds {seconds} must be finite and give from 1 to {} frames at {sample_rate_hz} Hz",
                Signal::MAX_FRAMES
            ),
            Self::Fade {
                seconds,
                sample_rate_hz,
            } => write!(
                f,
                "fade_seconds {seconds} must be finite, non-negative and at most {} frames at {sample_rate_hz} Hz",
                Signal::MAX_FRAMES
            ),
            Self::FadesOverlap {
                fade_frames,
                frames,
            } => write!(
                f,
                "a fade of {fade_frames} frames at each edge does not fit within {frames} frames; the fades must leave at least one frame between them"
            ),
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
                    "the components for the {ear} add up to a peak of {level} dBFS, above full scale; lower their peak_dbfs"
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
                "{parameter} {hz} Hz must be finite, positive and below {} Hz, half the {sample_rate_hz} Hz sample rate",
                f64::from(*sample_rate_hz) / 2.0
            ),
            Self::Sideband {
                frequency_hz,
                rate_hz,
                sample_rate_hz,
            } => write!(
                f,
                "frequency_hz {frequency_hz} Hz plus modulation rate_hz {rate_hz} Hz must stay below {} Hz, half the {sample_rate_hz} Hz sample rate",
                f64::from(*sample_rate_hz) / 2.0
            ),
            Self::PulseWidth {
                parameter,
                width_seconds,
                rate_hz,
                sample_rate_hz,
            } => write!(
                f,
                "{parameter} {width_seconds} must give at least one frame at {sample_rate_hz} Hz and leave at least one closed frame in each 1/{rate_hz} s period"
            ),
            Self::PeakDbfs { peak_dbfs } => {
                write!(f, "peak_dbfs {peak_dbfs} must be finite and at most 0")
            }
            Self::Depth { depth } => write!(f, "modulation depth {depth} must be from 0 to 1"),
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
