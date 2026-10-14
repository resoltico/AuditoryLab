// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::generator::{Carrier, Phase, Sinusoidal, Voice};
use crate::session::{Envelope, Session};
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

/// One sound source routed to one or both ears.
///
/// `level_dbfs` sets the component's amplitude bound: 0 dBFS is full scale. An
/// optional [`Modulation`] shapes the amplitude without raising that peak.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Component {
    /// A sine wave starting at phase zero.
    Tone {
        /// The ears that hear this component.
        ears: Ears,
        /// Frequency in hertz.
        frequency_hz: f64,
        /// Amplitude bound in dBFS.
        level_dbfs: f64,
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
        /// Pulse amplitude bound in dBFS.
        level_dbfs: f64,
        /// Optional amplitude modulation.
        modulation: Option<Modulation>,
    },
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

/// A periodic change of a component's amplitude.
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
    /// Return the ears that hear this component.
    #[must_use]
    pub const fn ears(&self) -> Ears {
        match self {
            Self::Tone { ears, .. } | Self::Click { ears, .. } => *ears,
        }
    }

    /// Return the configured amplitude bound in dBFS.
    #[must_use]
    pub const fn level_dbfs(&self) -> f64 {
        match self {
            Self::Tone { level_dbfs, .. } | Self::Click { level_dbfs, .. } => *level_dbfs,
        }
    }

    /// Return the amplitude modulation, if any.
    #[must_use]
    pub const fn modulation(&self) -> Option<Modulation> {
        match self {
            Self::Tone { modulation, .. } | Self::Click { modulation, .. } => *modulation,
        }
    }

    /// Check every value at `rate` and build the generator.
    fn voice(&self, rate: SampleRate) -> Result<Voice, ComponentError> {
        let level_dbfs = self.level_dbfs();
        if !(level_dbfs.is_finite() && level_dbfs <= 0.0) {
            return Err(ComponentError::Level { level_dbfs });
        }
        let carrier = match *self {
            Self::Tone { frequency_hz, .. } => {
                Carrier::Tone(phase("frequency_hz", frequency_hz, rate)?)
            }
            Self::Click {
                rate_hz,
                width_seconds,
                ..
            } => {
                let step = frequency_step("rate_hz", rate_hz, rate)?;
                if !step.is_normal() {
                    return Err(ComponentError::ClickRateTooSmall {
                        rate_hz,
                        sample_rate_hz: rate.hz(),
                    });
                }
                let duty = width_seconds * rate_hz;
                // At least one frame per click, and a gap before the next.
                if !(width_seconds * rate.as_f64() >= 1.0 && duty < 1.0) {
                    return Err(ComponentError::ClickWidth {
                        width_seconds,
                        rate_hz,
                        sample_rate_hz: rate.hz(),
                    });
                }
                Carrier::Click { step, duty }
            }
        };
        let modulation = match self.modulation() {
            None => None,
            Some(Modulation::Sinusoidal { rate_hz, depth }) => {
                let phase = phase("modulation rate_hz", rate_hz, rate)?;
                if !(0.0..=1.0).contains(&depth) {
                    return Err(ComponentError::Depth { depth });
                }
                if let Self::Tone { frequency_hz, .. } = *self
                    && frequency_hz + rate_hz >= rate.nyquist_hz()
                {
                    return Err(ComponentError::Sideband {
                        frequency_hz,
                        rate_hz,
                        sample_rate_hz: rate.hz(),
                    });
                }
                Some(Sinusoidal { phase, depth })
            }
        };
        let ears = self.ears();
        Ok(Voice {
            left: ears.hears(Ear::Left),
            right: ears.hears(Ear::Right),
            amplitude: numeric::amplitude(level_dbfs),
            carrier,
            modulation,
        })
    }
}

/// A phase accumulator for a frequency that is finite, positive, below
/// half the sample rate and large enough to advance the phase.
fn phase(field: &'static str, hz: f64, rate: SampleRate) -> Result<Phase, ComponentError> {
    let step = frequency_step(field, hz, rate)?;
    if step > 0.0 {
        Ok(Phase::new(step))
    } else {
        Err(ComponentError::PhaseStepTooSmall {
            field,
            hz,
            sample_rate_hz: rate.hz(),
        })
    }
}

/// Return the step for a frequency in the permitted range; the caller checks
/// whether the result has the precision required by its generator.
fn frequency_step(field: &'static str, hz: f64, rate: SampleRate) -> Result<f64, ComponentError> {
    if hz > 0.0 && hz < rate.nyquist_hz() {
        Ok(hz / rate.as_f64())
    } else {
        Err(ComponentError::Frequency {
            field,
            hz,
            sample_rate_hz: rate.hz(),
        })
    }
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
    pub fn session(&self, rate: SampleRate) -> Result<Session, StimulusError> {
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
        Ok(Session::new(
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
    pub fn uses_separate_ear_routing(&self) -> bool {
        self.components
            .iter()
            .any(|component| component.ears() != Ears::Both)
    }
}

/// Why a stimulus cannot be rendered at a sample rate.
#[derive(Debug, Clone, PartialEq)]
pub enum StimulusError {
    /// `duration_seconds` is not finite, gives no frame, or spans more than
    /// [`Session::MAX_FRAMES`].
    Duration {
        /// The rejected duration.
        seconds: f64,
        /// The sample rate in frames per second.
        sample_rate_hz: u32,
    },
    /// `fade_seconds` is not finite and non-negative, or spans more than
    /// [`Session::MAX_FRAMES`].
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

/// Why a component cannot be rendered at a sample rate.
#[derive(Debug, Clone, PartialEq)]
pub enum ComponentError {
    /// A frequency is not finite and positive below half the sample rate.
    Frequency {
        /// The field holding it.
        field: &'static str,
        /// The rejected frequency in hertz.
        hz: f64,
        /// The sample rate in frames per second.
        sample_rate_hz: u32,
    },
    /// A positive, in-range frequency rounds to a zero phase step.
    PhaseStepTooSmall {
        /// The field holding it.
        field: &'static str,
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
    /// A click's rate gives a zero or subnormal phase step, whose relative
    /// rounding cannot preserve the requested pulse timing.
    ClickRateTooSmall {
        /// The rejected click rate in hertz.
        rate_hz: f64,
        /// The sample rate in frames per second.
        sample_rate_hz: u32,
    },
    /// A click is shorter than one frame or not shorter than its period.
    ClickWidth {
        /// The rejected width in seconds.
        width_seconds: f64,
        /// Clicks per second.
        rate_hz: f64,
        /// The sample rate in frames per second.
        sample_rate_hz: u32,
    },
    /// The level is not finite or is above full scale.
    Level {
        /// The rejected level.
        level_dbfs: f64,
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
                Session::MAX_FRAMES
            ),
            Self::Fade {
                seconds,
                sample_rate_hz,
            } => write!(
                f,
                "fade_seconds {seconds} must be finite, non-negative and at most {} frames at {sample_rate_hz} Hz",
                Session::MAX_FRAMES
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
                    "the components for the {ear} add up to a peak of {level} dBFS, above full scale; lower their level_dbfs"
                )
            }
        }
    }
}

impl Display for ComponentError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frequency {
                field,
                hz,
                sample_rate_hz,
            } => write!(
                f,
                "{field} {hz} Hz must be finite, positive and below {} Hz, half the {sample_rate_hz} Hz sample rate",
                f64::from(*sample_rate_hz) / 2.0
            ),
            Self::PhaseStepTooSmall {
                field,
                hz,
                sample_rate_hz,
            } => write!(
                f,
                "{field} {hz:e} Hz is too small to advance the phase at {sample_rate_hz} Hz; raise {field}"
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
            Self::ClickRateTooSmall {
                rate_hz,
                sample_rate_hz,
            } => write!(
                f,
                "rate_hz {rate_hz:e} Hz is too small for accurate click timing at {sample_rate_hz} Hz; raise rate_hz"
            ),
            Self::ClickWidth {
                width_seconds,
                rate_hz,
                sample_rate_hz,
            } => write!(
                f,
                "width_seconds {width_seconds} must be at least one frame ({} s at {sample_rate_hz} Hz) and shorter than the {} s between clicks at {rate_hz} Hz",
                1.0 / f64::from(*sample_rate_hz),
                1.0 / rate_hz
            ),
            Self::Level { level_dbfs } => {
                write!(f, "level_dbfs {level_dbfs} must be finite and at most 0")
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
        }
    }
}

impl Display for Component {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tone {
                frequency_hz,
                level_dbfs,
                ..
            } => write!(f, "tone {frequency_hz} Hz, {level_dbfs} dBFS")?,
            Self::Click {
                rate_hz,
                width_seconds,
                level_dbfs,
                ..
            } => write!(
                f,
                "clicks {rate_hz} Hz, {width_seconds} s wide, {level_dbfs} dBFS"
            )?,
        }
        if let Some(modulation) = self.modulation() {
            write!(f, ", {modulation}")?;
        }
        write!(f, ", {}", self.ears())
    }
}

#[cfg(test)]
mod tests;
