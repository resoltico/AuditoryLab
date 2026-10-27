//! Human explanations of rate-dependent acoustic failures.
use crate::core::{ParameterId, Signal, SoundError, VoiceError};
use std::fmt::{self, Display, Formatter};
struct SoundText<'a>(&'a SoundError);
struct VoiceText<'a>(&'a VoiceError);
// Frame counts are bounded by Signal::MAX_FRAMES, below the exact integer range of f64.
#[expect(
    clippy::cast_precision_loss,
    reason = "frame counts are bounded below 2^53 by the signal contract"
)]
fn frames_to_f64(frames: u64) -> f64 {
    frames as f64
}
/// Explain an acoustic failure using the captured rate and quantities.
#[must_use]
pub fn sound_error(error: &SoundError) -> String {
    SoundText(error).to_string()
}
/// Explain a single generator's validation failure.
#[must_use]
pub fn voice_error(error: &VoiceError) -> String {
    VoiceText(error).to_string()
}
/// A model value in its displayed unit, with the unit's symbol.
struct Quantity(ParameterId, f64);

/// The longest length at a rate, in days.
struct Longest(u32);

impl Display for Longest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let days = frames_to_f64(Signal::MAX_FRAMES) / f64::from(self.0) / 86_400.0;
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
        let separator = if unit == crate::core::Unit::Percent {
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

impl Display for SoundText<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.0 {
            SoundError::Structure(error) => {
                f.write_str(&super::core_errors::structure_error(error))
            }
            SoundError::Pair(error) => f.write_str(&super::core_errors::pair_error(error)),
            SoundError::PairCarrier {
                id,
                first_hz,
                offset_hz,
                error,
            } => write!(
                f,
                "component {id}: first carrier {first_hz} Hz and signed offset {offset_hz} Hz: {}",
                VoiceText(error)
            ),
            SoundError::Duration {
                seconds,
                sample_rate_hz,
            } => write!(
                f,
                "{} must last from one sample (1/{sample_rate_hz} s) to {} at {sample_rate_hz} Hz",
                Quantity(ParameterId::Duration, *seconds),
                Longest(*sample_rate_hz)
            ),
            SoundError::Fade {
                seconds,
                sample_rate_hz,
            } => write!(
                f,
                "{} must be from 0 s to {} at {sample_rate_hz} Hz",
                Quantity(ParameterId::Fade, *seconds),
                Longest(*sample_rate_hz)
            ),
            SoundError::FadesOverlap {
                fade_frames,
                frames,
                sample_rate_hz,
            } => {
                let seconds = |n: u64| frames_to_f64(n) / f64::from(*sample_rate_hz);
                write!(
                    f,
                    "a {} s fade at each edge does not fit in {} s; the two fades together must be shorter than the duration",
                    Number(seconds(*fade_frames)),
                    Number(seconds(*frames))
                )
            }
            SoundError::Component { number, error } => {
                write!(f, "component {number}: {}", VoiceText(error))
            }
            SoundError::Peak { ear, amplitude } => {
                let db = 20.0 * amplitude.log10();
                let rounded = format!("{db:.2}");
                let level = if db > 0.0 && rounded == "0.00" {
                    format!("{db:.2e}")
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

impl Display for VoiceText<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.0 {
            VoiceError::Frequency {
                parameter,
                hz,
                sample_rate_hz,
            } => write!(
                f,
                "{} must be finite, positive and below {} Hz, half the {sample_rate_hz} Hz sample rate",
                Quantity(*parameter, *hz),
                f64::from(*sample_rate_hz) / 2.0
            ),
            VoiceError::Sideband {
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
            VoiceError::PulseWidth {
                parameter,
                width_seconds,
                rate_hz,
                sample_rate_hz,
            } => write!(
                f,
                "{} must give 1 to {} frames at {sample_rate_hz} Hz and leave at least one closed frame in each 1/{} s period",
                Quantity(*parameter, *width_seconds),
                Signal::MAX_FRAMES,
                Number(*rate_hz)
            ),
            VoiceError::PeakDbfs { peak_dbfs } => write!(
                f,
                "{} must be finite and at most 0 dBFS",
                Quantity(ParameterId::PeakLevel, *peak_dbfs)
            ),
            VoiceError::Depth { depth } => write!(
                f,
                "{} must be from 0% to 100%",
                Quantity(ParameterId::AmDepth, *depth)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excessive_width_names_the_signal_frame_limit() {
        let component = crate::core::Voice {
            ears: crate::core::Ears::Both,
            peak_dbfs: -30.0,
            carrier: crate::core::Carrier::Clicks {
                rate_hz: 1e-12,
                width_seconds: 68_719_476_737.0 / 48_000.0,
            },
            modulation: None,
        };
        let message = sound_error(
            &crate::core::Sound {
                components: vec![crate::core::Component {
                    id: crate::core::ComponentId::new(1).unwrap(),
                    form: crate::core::ComponentForm::Voice(component),
                }],
                duration_seconds: 1.0,
                fade_seconds: 0.0,
            }
            .signal(crate::core::SampleRate::DEFAULT)
            .unwrap_err(),
        );
        assert!(
            message.contains(&crate::core::Signal::MAX_FRAMES.to_string()),
            "{message}"
        );
    }

    #[test]
    fn messages_keep_extreme_values_short() {
        let error = voice_error(&VoiceError::Frequency {
            parameter: ParameterId::ToneFrequency,
            hz: 1e300,
            sample_rate_hz: 48_000,
        });
        assert!(error.contains("1e300 Hz"), "{error}");
        assert!(error.len() < 120, "{error}");
        let tiny = voice_error(&VoiceError::Frequency {
            parameter: ParameterId::ToneFrequency,
            hz: 1e-9,
            sample_rate_hz: 48_000,
        });
        assert!(tiny.contains("1e-9 Hz"), "{tiny}");
        let ordinary = voice_error(&VoiceError::Frequency {
            parameter: ParameterId::ToneFrequency,
            hz: 30_000.0,
            sample_rate_hz: 48_000,
        });
        assert!(ordinary.contains("30000 Hz"), "{ordinary}");
    }

    #[test]
    fn length_messages_speak_in_time_not_frames() {
        let duration = sound_error(&SoundError::Duration {
            seconds: 0.0,
            sample_rate_hz: 48_000,
        });
        assert_eq!(
            duration,
            "Duration 0 s must last from one sample (1/48000 s) to 16.6 days at 48000 Hz"
        );
        let overlap = sound_error(&SoundError::FadesOverlap {
            fade_frames: 24_000,
            frames: 48_000,
            sample_rate_hz: 48_000,
        });
        assert_eq!(
            overlap,
            "a 0.5 s fade at each edge does not fit in 1 s; the two fades together must be shorter than the duration"
        );
    }
}
