//! Typed acoustic compilation failures and their semantic field attribution.
use crate::{Channel, ParameterId, VoiceId};
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
    /// An channel's voices add up to a peak above full scale.
    Peak {
        /// The channel.
        channel: Channel,
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
            Self::PairCarrier {
                error: VoiceError::Sideband { .. },
                ..
            } => &[
                ParameterId::PairFirstFrequency,
                ParameterId::PairOffset,
                ParameterId::ModulationRate,
            ],
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
    /// Pulse intent or edges cannot be realized at the selected rate.
    Pulse {
        /// Whether this is the shared modulation rather than the source.
        modulation: bool,
        /// Requested width intent retained for semantic attribution.
        width: crate::PulseWidth,
        /// Requested repetition rate, in hertz.
        rate_hz: f64,
        /// Rate at which resolution failed.
        sample_rate_hz: u32,
        /// Precise pulse resolution failure.
        error: crate::PulseError,
    },
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
            Self::Pulse {
                modulation,
                width,
                error,
                ..
            } => match (*modulation, *width, error) {
                (true, _, crate::PulseError::Rate) => &[ParameterId::ModulationPulseRate],
                (false, _, crate::PulseError::Rate) => &[ParameterId::SourcePulseRate],
                (true, crate::PulseWidth::Duration { .. }, crate::PulseError::Edges) => &[
                    ParameterId::ModulationPulseDuration,
                    ParameterId::ModulationRise,
                    ParameterId::ModulationFall,
                ],
                (false, crate::PulseWidth::Duration { .. }, crate::PulseError::Edges) => &[
                    ParameterId::SourcePulseDuration,
                    ParameterId::SourceRise,
                    ParameterId::SourceFall,
                ],
                (true, crate::PulseWidth::DutyCycle { .. }, crate::PulseError::Edges) => &[
                    ParameterId::ModulationPulseRate,
                    ParameterId::ModulationDuty,
                    ParameterId::ModulationRise,
                    ParameterId::ModulationFall,
                ],
                (false, crate::PulseWidth::DutyCycle { .. }, crate::PulseError::Edges) => &[
                    ParameterId::SourcePulseRate,
                    ParameterId::SourceDuty,
                    ParameterId::SourceRise,
                    ParameterId::SourceFall,
                ],
                (true, crate::PulseWidth::Duration { .. }, _) => &[
                    ParameterId::ModulationPulseRate,
                    ParameterId::ModulationPulseDuration,
                ],
                (false, crate::PulseWidth::Duration { .. }, _) => &[
                    ParameterId::SourcePulseRate,
                    ParameterId::SourcePulseDuration,
                ],
                (true, crate::PulseWidth::DutyCycle { .. }, _) => &[
                    ParameterId::ModulationPulseRate,
                    ParameterId::ModulationDuty,
                ],
                (false, crate::PulseWidth::DutyCycle { .. }, _) => {
                    &[ParameterId::SourcePulseRate, ParameterId::SourceDuty]
                }
            },
            Self::Frequency { parameter, .. } => match parameter {
                ParameterId::SourcePulseRate => &[ParameterId::SourcePulseRate],
                ParameterId::ModulationRate => &[ParameterId::ModulationRate],
                ParameterId::ModulationPulseRate => &[ParameterId::ModulationPulseRate],
                _ => &[ParameterId::ToneFrequency],
            },
            Self::Sideband { .. } => &[ParameterId::ToneFrequency, ParameterId::ModulationRate],
            Self::PeakDbfs { .. } => &[ParameterId::PeakLevel],
            Self::Depth { .. } => &[ParameterId::ModulationDepth],
        }
    }
}

impl core::fmt::Display for SoundError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}
impl core::fmt::Display for VoiceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}

impl core::error::Error for SoundError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Pair(error) => Some(error),
            Self::PairCarrier { error, .. } | Self::Voice { error, .. } => Some(error),
            _ => None,
        }
    }
}

impl core::error::Error for VoiceError {}

crate::diagnostic_codes! {
    pub(crate) const CODES;
    SoundError {
        codes {
            Self::Duration { .. } => "sound.duration",
            Self::Fade { .. } => "sound.fade",
            Self::FadesOverlap { .. } => "sound.fades_overlap",
            Self::Peak { .. } => "sound.peak",
        }
        delegates {
            Self::Pair(error) => error,
            Self::PairCarrier { error, .. } => error,
            Self::Voice { error, .. } => error,
        }
        params(this) {
            match this {
                Self::Duration {
                    seconds,
                    sample_rate_hz,
                }
                | Self::Fade {
                    seconds,
                    sample_rate_hz,
                } => crate::diagnostic_params! {
                    "seconds"=>*seconds,
                    "sample_rate_hz"=>*sample_rate_hz,
                },
                Self::FadesOverlap {
                    fade_frames,
                    frames,
                    sample_rate_hz,
                } => {
                    crate::diagnostic_params! {
                        "fade_frames"=>*fade_frames,
                        "frames"=>*frames,
                        "sample_rat\
                        e_hz"=>*sample_rate_hz,
                    }
                }
                Self::Peak { channel, amplitude } => {
                    crate::diagnostic_params! {
                        "channel"=>channel.key(),
                        "amplitude"=>*amplitude,
                    }
                }
                Self::Voice { id, .. } => {
                    crate::diagnostic_params! {"owner"=>alloc::string::ToString::to_string(id)}
                }
                Self::PairCarrier {
                    id,
                    first_hz,
                    offset_hz,
                    ..
                } => {
                    crate::diagnostic_params! {
                        "owner"=>alloc::string::ToString::to_string(id),
                        "first_hz"=>*first_hz,
                        "offset_hz"=>*offset_hz,
                    }
                }
                Self::Pair(_) => crate::Params::new(),
            }
        }
    }
    VoiceError {
        codes {
            Self::Pulse { .. } => "voice.pulse",
            Self::Frequency { .. } => "voice.frequency",
            Self::Sideband { .. } => "voice.sideband",
            Self::PeakDbfs { .. } => "voice.peak_dbfs",
            Self::Depth { .. } => "voice.depth",
        }
        delegates {}
        params(this) {
            match this {
                Self::Pulse {
                    modulation, width, rate_hz, sample_rate_hz, error
                } => crate::diagnostic_params! {
                    "modulation"=>*modulation,
                    "rate_hz"=>*rate_hz,
                    "sample_rate_hz"=>*sample_rate_hz,
                    "reason"=>error.key(),
                    "width_mode"=>match width {
                        crate::PulseWidth::Duration {..}=>"duration",
                        crate::PulseWidth::DutyCycle {..}=>"duty_cycle",
                    },
                    "requested_width"=>match width {
                        crate::PulseWidth::Duration {seconds}=>seconds.get(),
                        crate::PulseWidth::DutyCycle {fraction}=>fraction.get(),
                    }
                },
                Self::Frequency {
                    parameter,
                    hz,
                    sample_rate_hz,
                } => {
                    crate::diagnostic_params! {
                        "parameter"=>parameter.key(),
                        "hz"=>*hz,
                        "sample_rate\
                        _hz"=>*sample_rate_hz,
                    }
                }
                Self::Sideband {
                    frequency_hz,
                    rate_hz,
                    sample_rate_hz,
                } => {
                    crate::diagnostic_params! {
                        "frequency_hz"=>*frequency_hz,
                        "rate_hz"=>*rate_hz,
                        "sample_rat\
                        e_hz"=>*sample_rate_hz,
                    }
                }
                Self::PeakDbfs { peak_dbfs } => crate::diagnostic_params! {"peak_dbfs"=>*peak_dbfs},
                Self::Depth { depth } => crate::diagnostic_params! {"depth"=>*depth},
            }
        }
    }
}
