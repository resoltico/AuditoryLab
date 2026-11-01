//! Human explanations of rate-dependent acoustic failures.
use auditorylab_core::Lane;
use auditorylab_core::ParameterId;
use auditorylab_core::Signal;
use auditorylab_core::SoundError;
use auditorylab_core::VoiceError;
use auditorylab_core::VoiceId;
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
        let separator = if unit == auditorylab_core::Unit::Percent {
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
            SoundError::Pair(error) => f.write_str(&crate::errors::core::pair_error(error)),
            SoundError::PairCarrier {
                id,
                first_hz,
                offset_hz,
                error,
            } => write!(
                f,
                "{}: first carrier {first_hz} Hz and beat offset {offset_hz} Hz: {}",
                voice_reference(*id),
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
                    "a {} s fade at each edge does not fit in {} s; the two fades together \
                        must be shorter than the duration",
                    Number(seconds(*fade_frames)),
                    Number(seconds(*frames))
                )
            }
            SoundError::Voice { id, error } => {
                write!(f, "{}: {}", voice_reference(*id), VoiceText(error))
            }
            SoundError::Peak { channel, amplitude } => {
                let db = 20.0 * amplitude.log10();
                let rounded = format!("{db:.2}");
                let level = if db > 0.0 && rounded == "0.00" {
                    format!("{db:.2e}")
                } else {
                    rounded
                };
                write!(
                    f,
                    "the components for the {} add up to a peak of {level} dBFS, above \
                        full scale; lower their peak levels",
                    channel.phrase()
                )
            }
        }
    }
}

impl Display for VoiceText<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.0 {
            VoiceError::Pulse {
                modulation,
                width,
                error,
                rate_hz,
                sample_rate_hz,
            } => pulse_text(f, *modulation, *width, *error, *rate_hz, *sample_rate_hz),
            VoiceError::Frequency {
                parameter,
                hz,
                sample_rate_hz,
            } => write!(
                f,
                "{} must be finite, positive and below {} Hz, half the \
                    {sample_rate_hz} Hz sample rate",
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
                Quantity(ParameterId::ModulationRate, *rate_hz),
                f64::from(*sample_rate_hz) / 2.0
            ),
            VoiceError::PeakDbfs { peak_dbfs } => write!(
                f,
                "{} must be finite and at most 0 dBFS",
                Quantity(ParameterId::PeakLevel, *peak_dbfs)
            ),
            VoiceError::Depth { depth } => write!(
                f,
                "{} must be from 0% to 100%",
                Quantity(ParameterId::ModulationDepth, *depth)
            ),
        }
    }
}

/// The authored component and, for a pair, its secondary tone.
#[must_use]
pub fn voice_reference(id: VoiceId) -> String {
    match id.lane {
        Lane::Primary => format!("Component {}", id.component),
        Lane::Secondary => format!("Component {} · second tone", id.component),
    }
}

/// Name the pulse quantity that failed and the bound it must meet, in the
/// units the editor shows.
fn pulse_text(
    f: &mut Formatter<'_>,
    modulation: bool,
    width: auditorylab_core::PulseWidth,
    error: auditorylab_core::PulseError,
    rate_hz: f64,
    sample_rate_hz: u32,
) -> fmt::Result {
    use auditorylab_core::{PulseError as E, PulseWidth as W};
    let what = if modulation {
        "Pulsed modulation"
    } else {
        "Pulse"
    };
    let rate = Number(rate_hz);
    let sample_ms = 1000.0 / f64::from(sample_rate_hz);
    match (error, width) {
        (E::Rate, _) => write!(
            f,
            "{what} rate {rate} Hz must be above 0 Hz and below {} Hz, half the \
                {sample_rate_hz} Hz sample rate",
            Number(f64::from(sample_rate_hz) / 2.0)
        ),
        (E::Duty, W::DutyCycle { fraction }) => write!(
            f,
            "{what} duty cycle {}% must be above 0% and below 100%",
            Number(fraction.get() * 100.0)
        ),
        (E::ClosedFrame, W::Duration { seconds }) => write!(
            f,
            "{what} window {} ms must be shorter than the {} ms period at {rate} Hz",
            Number(seconds.get() * 1000.0),
            Number(1000.0 / rate_hz)
        ),
        (E::ClosedFrame, W::DutyCycle { fraction }) => write!(
            f,
            "{what} duty cycle {}% leaves no silent sample between pulses at {rate} Hz and \
                {sample_rate_hz} Hz; lower the duty cycle",
            Number(fraction.get() * 100.0)
        ),
        (E::Edges, _) => write!(
            f,
            "{what} rise and fall must each last at least one sample ({} ms at \
                {sample_rate_hz} Hz) and fit inside the window",
            Number(sample_ms)
        ),
        (E::Width | E::Duty, _) => write!(
            f,
            "{what} window must last from one sample ({} ms) to {} frames at \
                {sample_rate_hz} Hz",
            Number(sample_ms),
            Signal::MAX_FRAMES
        ),
    }
}

#[cfg(test)]
const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excessive_width_names_the_signal_frame_limit() {
        let component = auditorylab_core::ComponentForm {
            source: auditorylab_core::SoundSource::PulseTrain {
                route: auditorylab_core::ChannelRouting::Both,
                peak_dbfs: finite(-30.0),
                pulse: auditorylab_app::core::PulseSpec {
                    rate_hz: finite(1e-12),
                    width: auditorylab_app::core::PulseWidth::Duration {
                        seconds: finite(68_719_476_737.0 / 48_000.0),
                    },
                    shape: auditorylab_app::core::PulseShape::Rectangular {},
                },
            },
            modulation: None,
        };
        let message = sound_error(
            &auditorylab_app::core::test_support::FiniteFixture::new(
                finite(1.0),
                finite(0.0),
                vec![auditorylab_core::Component {
                    id: auditorylab_core::ComponentId::new(1).unwrap(),
                    form: component,
                }],
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
            .signal(auditorylab_core::SampleRate::DEFAULT)
            .unwrap_err(),
        );
        assert!(
            message.contains(&auditorylab_core::Signal::MAX_FRAMES.to_string()),
            "{message}"
        );
    }

    #[test]
    fn pulse_failures_name_the_quantity_and_its_bound_without_frame_internals() {
        let window = |seconds: f64, error| {
            voice_error(&VoiceError::Pulse {
                modulation: false,
                width: auditorylab_core::PulseWidth::Duration {
                    seconds: finite(seconds),
                },
                rate_hz: 40.0,
                sample_rate_hz: 48_000,
                error,
            })
        };
        assert_eq!(
            window(0.05, auditorylab_core::PulseError::ClosedFrame),
            "Pulse window 50 ms must be shorter than the 25 ms period at 40 Hz"
        );
        let rate = window(0.001, auditorylab_core::PulseError::Rate);
        assert!(rate.contains("below 24000 Hz"), "{rate}");
        assert!(!rate.contains("frames"), "{rate}");
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
            "a 0.5 s fade at each edge does not fit in 1 s; the two fades together \
                must be shorter than the duration"
        );
    }
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    use auditorylab_core::ComponentId;

    #[test]
    fn acoustic_errors_name_the_authored_owner_and_secondary_tone() {
        let owner = ComponentId::new(9).unwrap();
        let error = VoiceError::Frequency {
            parameter: ParameterId::ToneFrequency,
            hz: 30_000.0,
            sample_rate_hz: 48_000,
        };
        let primary = SoundError::Voice {
            id: owner.with_lane(Lane::Primary),
            error: error.clone(),
        };
        assert!(sound_error(&primary).starts_with("Component 9: "));
        let secondary = SoundError::Voice {
            id: owner.with_lane(Lane::Secondary),
            error,
        };
        assert!(sound_error(&secondary).starts_with("Component 9 · second tone: "));
    }
}
