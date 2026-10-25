// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use alloc::{string::ToString, vec, vec::Vec};
use core::f64::consts::TAU;

fn rate(hz: u32) -> SampleRate {
    SampleRate::new(hz).unwrap()
}

fn tone(ears: Ears, frequency_hz: f64, peak_dbfs: f64) -> Voice {
    Voice {
        ears,
        peak_dbfs,
        modulation: None,
        carrier: crate::Carrier::Tone { frequency_hz },
    }
}

fn stimulus(components: Vec<Voice>) -> Sound {
    Sound {
        duration_seconds: 1.0,
        fade_seconds: 0.0,
        components: components
            .into_iter()
            .enumerate()
            .map(|(index, voice)| crate::Component {
                id: crate::ComponentId::numbered(index).unwrap(),
                node: crate::Node::Voice(voice),
            })
            .collect(),
    }
}

fn render(stimulus: &Sound, hz: u32) -> Vec<[f64; 2]> {
    let session = stimulus.signal(rate(hz)).unwrap();
    let mut frames = vec![[9.0; 2]; usize::try_from(session.frames()).unwrap()];
    assert_eq!(session.render(0, &mut frames), frames.len());
    frames
}

#[test]
fn each_ear_hears_the_sum_of_its_components() {
    let frames = render(
        &stimulus(vec![
            tone(Ears::Left, 250.0, -20.0),
            tone(Ears::Right, 256.0, -20.0),
            tone(Ears::Both, 1_000.0, -26.0),
        ]),
        8_000,
    );
    let shared = 10_f64.powf(-26.0 / 20.0);
    for (n, [left, right]) in (0_u32..).zip(&frames) {
        let t = f64::from(n) / 8_000.0;
        let both = shared * (TAU * 1_000.0 * t).sin();
        assert!((left - (0.1 * (TAU * 250.0 * t).sin() + both)).abs() < 1e-12);
        assert!((right - (0.1 * (TAU * 256.0 * t).sin() + both)).abs() < 1e-12);
    }
}

#[test]
fn a_diotic_stimulus_has_bit_identical_ears_and_one_ear_stimuli_leave_the_other_silent() {
    let diotic = stimulus(vec![
        tone(Ears::Both, 396.5, -23.0103),
        tone(Ears::Both, 403.5, -23.0103),
    ]);
    assert!(!diotic.uses_separate_ears());
    for [left, right] in render(&diotic, 44_100) {
        assert_eq!(left.to_bits(), right.to_bits());
    }
    let one_ear = stimulus(vec![tone(Ears::Right, 1_000.0, -20.0)]);
    assert!(one_ear.uses_separate_ears());
    let frames = render(&one_ear, 8_000);
    assert!(frames.iter().all(|[left, _]| left.to_bits() == 0));
    assert!(frames.iter().any(|[_, right]| right.abs() > 0.09));
}

#[test]
fn rms_matched_two_tone_and_separate_ear_stimuli_have_equal_plateau_rms() {
    // 2 s coherent for both pairs at 8 kHz; each sine of amplitude a has RMS a/sqrt(2).
    let rms = |samples: &[f64]| {
        (samples.iter().map(|s| s * s).sum::<f64>()
            / f64::from(u32::try_from(samples.len()).unwrap()))
        .sqrt()
    };
    for (low, high) in [(396.5, 403.5), (380.0, 420.0)] {
        let mut separate = stimulus(vec![
            tone(Ears::Left, low, -20.0),
            tone(Ears::Right, high, -20.0),
        ]);
        let mut summed = stimulus(vec![
            tone(Ears::Both, low, -23.0103),
            tone(Ears::Both, high, -23.0103),
        ]);
        separate.duration_seconds = 2.0;
        summed.duration_seconds = 2.0;
        let separate: Vec<f64> = render(&separate, 8_000).iter().map(|f| f[0]).collect();
        let summed: Vec<f64> = render(&summed, 8_000).iter().map(|f| f[0]).collect();
        let ratio_db = 20.0 * (rms(&summed) / rms(&separate)).log10();
        assert!(ratio_db.abs() < 1e-4, "{ratio_db} dB");
        // Equal per-tone levels would leave the sum 3 dB louder: the check can tell.
        assert!(
            (20.0 * (rms(&summed) * 2_f64.sqrt() / rms(&separate)).log10() - 3.0103).abs() < 1e-3
        );
    }
}

#[test]
fn a_signal_states_its_frames_fades_and_peaks() {
    let mut pair = stimulus(vec![
        tone(Ears::Left, 250.0, -20.0),
        tone(Ears::Both, 256.0, -26.0),
    ]);
    pair.duration_seconds = 480.0;
    pair.fade_seconds = 0.1;
    let session = pair.signal(rate(44_100)).unwrap();
    assert_eq!(session.sample_rate(), rate(44_100));
    assert_eq!(session.frames(), 21_168_000);
    assert_eq!(session.fade_frames(), 4_410);
    assert_eq!(session.duration_seconds(), 480.0);
    assert_eq!(session.fade_seconds(), 0.1);
    let [left, right] = session.peak_dbfs();
    assert!((left - 20.0 * (0.1 + 10_f64.powf(-1.3)).log10()).abs() < 1e-9);
    assert!((right + 26.0).abs() < 1e-9);
    assert_eq!(
        stimulus(vec![]).signal(rate(8_000)).unwrap().peak_dbfs(),
        [f64::NEG_INFINITY; 2]
    );
}

#[test]
fn silence_renders_zeros_for_its_duration() {
    let frames = render(&stimulus(vec![]), 8_000);
    assert_eq!(frames.len(), 8_000);
    assert!(
        frames
            .iter()
            .all(|f| f[0].to_bits() == 0 && f[1].to_bits() == 0)
    );
}

fn signal_error(stimulus: &Sound, hz: u32) -> StimulusError {
    stimulus.signal(rate(hz)).unwrap_err()
}

#[test]
fn durations_and_fades_must_give_whole_frames_that_fit() {
    let mut s = stimulus(vec![]);
    for seconds in [f64::NAN, f64::INFINITY, -1.0, 0.0, 0.000_01] {
        s.duration_seconds = seconds;
        assert!(
            matches!(signal_error(&s, 48_000), StimulusError::Duration { .. }),
            "{seconds}"
        );
    }
    s.duration_seconds = 1.0;
    for seconds in [f64::NAN, -0.1, f64::INFINITY] {
        s.fade_seconds = seconds;
        assert!(
            matches!(signal_error(&s, 48_000), StimulusError::Fade { .. }),
            "{seconds}"
        );
    }
    s.fade_seconds = 0.5;
    assert_eq!(
        signal_error(&s, 48_000),
        StimulusError::FadesOverlap {
            fade_frames: 24_000,
            frames: 48_000,
            sample_rate_hz: 48_000
        }
    );
    s.fade_seconds = 0.499_9;
    assert!(s.signal(rate(48_000)).is_ok());
}

#[test]
fn guards_accept_and_reject_neighbours() {
    let mut s = stimulus(vec![tone(Ears::Both, 1000.0, -20.0)]);
    for hz in [0.0, -1.0, f64::NAN, f64::INFINITY, 24_000.0] {
        first_voice(&mut s).carrier = Carrier::Tone { frequency_hz: hz };
        assert!(s.signal(rate(48_000)).is_err());
    }
    for hz in [f64::from_bits(1), 24_000.0_f64.next_down()] {
        first_voice(&mut s).carrier = Carrier::Tone { frequency_hz: hz };
        assert!(s.signal(rate(48_000)).is_ok());
    }
    first_voice(&mut s).carrier = Carrier::Tone {
        frequency_hz: 1000.0,
    };
    for depth in [0.0, 1.0] {
        first_voice(&mut s).modulation = Some(Modulation::Sinusoidal {
            rate_hz: 40.0,
            depth,
        });
        assert!(s.signal(rate(48_000)).is_ok());
    }
    for depth in [-f64::EPSILON, 1.0_f64.next_up(), f64::NAN] {
        first_voice(&mut s).modulation = Some(Modulation::Sinusoidal {
            rate_hz: 40.0,
            depth,
        });
        assert!(s.signal(rate(48_000)).is_err());
    }
    for depth in [0.0, f64::from_bits(1), 1.0] {
        for (frequency, active_valid) in [
            (23_960.0_f64.next_down(), true),
            (23_960.0, false),
            (23_999.0, false),
        ] {
            first_voice(&mut s).carrier = Carrier::Tone {
                frequency_hz: frequency,
            };
            first_voice(&mut s).modulation = Some(Modulation::Sinusoidal {
                rate_hz: 40.0,
                depth,
            });
            assert_eq!(s.signal(rate(48_000)).is_ok(), depth == 0.0 || active_valid);
        }
    }
    // Inactivity does not excuse an invalid stored rate.
    for rate_hz in [0.0, -1.0, 24_000.0, f64::INFINITY, f64::NAN] {
        first_voice(&mut s).modulation = Some(Modulation::Sinusoidal {
            rate_hz,
            depth: 0.0,
        });
        assert!(s.signal(rate(48_000)).is_err());
    }
    s.components = stimulus(vec![tone(Ears::Both, 1000.0, 0.0)]).components;
    assert!(s.signal(rate(48_000)).is_ok());
    s.components.push(crate::Component {
        id: ComponentId::new(2),
        node: crate::Node::Voice(tone(Ears::Both, 1000.0, -20.0)),
    });
    assert!(matches!(
        s.signal(rate(48_000)),
        Err(StimulusError::Peak { .. })
    ));
    s.components = stimulus(vec![tone(Ears::Both, 1000.0, -60.0); 16]).components;
    assert!(s.signal(rate(48_000)).is_ok());
    s.components.push(s.components[0]);
    assert!(matches!(
        s.signal(rate(48_000)),
        Err(StimulusError::Authored(crate::LoweringError::VoiceBudget))
    ));
    s.components.clear();
    s.duration_seconds = rate(48_000).seconds_in(Signal::MAX_FRAMES);
    assert!(s.signal(rate(48_000)).is_ok());
    s.duration_seconds = rate(48_000).seconds_in(Signal::MAX_FRAMES + 1);
    assert!(s.signal(rate(48_000)).is_err());
}
#[test]
fn peak_and_pulse_width_guards_reject_invalid_values() {
    let mut s = stimulus(vec![tone(Ears::Both, 1000.0, 0.0)]);
    assert!(s.signal(rate(48000)).is_ok());
    for peak in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MIN_POSITIVE,
    ] {
        first_voice(&mut s).peak_dbfs = peak;
        assert!(matches!(
            s.signal(rate(48000)),
            Err(StimulusError::Component {
                error: ComponentError::PeakDbfs { .. },
                ..
            })
        ));
    }
    first_voice(&mut s).peak_dbfs = -20.0;
    for width in [f64::NAN, f64::INFINITY, -1.0, 0.0, 0.4 / 48000.0] {
        first_voice(&mut s).carrier = Carrier::Click {
            rate_hz: 40.0,
            width_seconds: width,
        };
        assert!(matches!(
            s.signal(rate(48000)),
            Err(StimulusError::Component {
                error: ComponentError::PulseWidth { .. },
                ..
            })
        ));
    }
    first_voice(&mut s).carrier = Carrier::Click {
        rate_hz: 40.0,
        width_seconds: 0.5 / 48000.0,
    };
    assert!(s.signal(rate(48000)).is_ok());
}

#[test]
fn click_nyquist_rejection_is_not_masked_by_the_closed_frame_rule() {
    let mut s = stimulus(vec![Voice {
        ears: Ears::Both,
        peak_dbfs: -20.0,
        carrier: Carrier::Click {
            rate_hz: 24000.0,
            width_seconds: 1.0 / 48000.0,
        },
        modulation: None,
    }]);
    assert!(matches!(
        s.signal(rate(48000)),
        Err(StimulusError::Component {
            error: ComponentError::Frequency {
                parameter: ParameterId::ClickRate,
                ..
            },
            ..
        })
    ));
    first_voice(&mut s).carrier = Carrier::Click {
        rate_hz: 24000.0_f64.next_down(),
        width_seconds: 1.0 / 48000.0,
    };
    assert!(s.signal(rate(48000)).is_ok());
}

mod validate {
    use crate::{
        Carrier, ComponentError, ComponentId, Ears, Modulation, SampleRate, Sound, TargetCheck,
        Voice,
    };
    use alloc::vec;

    pub(super) fn own(component: Voice, rate: SampleRate) -> Result<(), ComponentError> {
        let check = TargetCheck::new(rate, &[(ComponentId::new(1), component)]);
        check.components()[0].1.clone().map(|_| ())
    }

    #[test]
    fn each_guard_accepts_and_rejects_like_signal() {
        let rate = SampleRate::new(48_000).unwrap();
        let base = Voice {
            ears: Ears::Both,
            peak_dbfs: -20.0,
            carrier: Carrier::Tone {
                frequency_hz: 440.0,
            },
            modulation: None,
        };
        let cases = [
            (base, true),
            (
                Voice {
                    peak_dbfs: 0.0,
                    ..base
                },
                true,
            ),
            (
                Voice {
                    peak_dbfs: 0.1,
                    ..base
                },
                false,
            ),
            (
                Voice {
                    carrier: Carrier::Tone {
                        frequency_hz: 23_999.0,
                    },
                    ..base
                },
                true,
            ),
            (
                Voice {
                    carrier: Carrier::Tone {
                        frequency_hz: 24_000.0,
                    },
                    ..base
                },
                false,
            ),
            (
                Voice {
                    modulation: Some(Modulation::Sinusoidal {
                        rate_hz: 40.0,
                        depth: 1.0,
                    }),
                    ..base
                },
                true,
            ),
            (
                Voice {
                    modulation: Some(Modulation::Sinusoidal {
                        rate_hz: 40.0,
                        depth: 1.01,
                    }),
                    ..base
                },
                false,
            ),
            (
                Voice {
                    carrier: Carrier::Tone {
                        frequency_hz: 23_959.0,
                    },
                    modulation: Some(Modulation::Sinusoidal {
                        rate_hz: 40.0,
                        depth: 1.0,
                    }),
                    ..base
                },
                true,
            ),
            (
                Voice {
                    carrier: Carrier::Tone {
                        frequency_hz: 23_960.0,
                    },
                    modulation: Some(Modulation::Sinusoidal {
                        rate_hz: 40.0,
                        depth: 1.0,
                    }),
                    ..base
                },
                false,
            ),
            (
                Voice {
                    carrier: Carrier::Click {
                        rate_hz: 40.0,
                        width_seconds: 0.001,
                    },
                    ..base
                },
                true,
            ),
            (
                Voice {
                    carrier: Carrier::Click {
                        rate_hz: 40.0,
                        width_seconds: 0.000_001,
                    },
                    ..base
                },
                false,
            ),
            (
                Voice {
                    modulation: Some(Modulation::Rectangular {
                        rate_hz: 40.0,
                        width_seconds: 1_199.0 / 48_000.0,
                    }),
                    ..base
                },
                true,
            ),
            (
                Voice {
                    modulation: Some(Modulation::Rectangular {
                        rate_hz: 40.0,
                        width_seconds: 1_200.0 / 48_000.0,
                    }),
                    ..base
                },
                false,
            ),
        ];
        for (component, valid) in cases {
            let own = own(component, rate);
            assert_eq!(own.is_ok(), valid, "{component:?}");
            let whole = Sound {
                duration_seconds: 1.0,
                fade_seconds: 0.0,
                components: (vec![component])
                    .into_iter()
                    .enumerate()
                    .map(|(index, voice)| crate::Component {
                        id: crate::ComponentId::numbered(index).unwrap(),
                        node: crate::Node::Voice(voice),
                    })
                    .collect(),
            }
            .signal(rate);
            match (own, whole) {
                (Ok(()), Ok(_)) => {}
                (
                    Err(error),
                    Err(crate::StimulusError::Component {
                        number: 1,
                        error: same,
                    }),
                ) => {
                    assert_eq!(error, same);
                }
                (own, whole) => panic!("{own:?} vs {whole:?}"),
            }
        }
        assert!(matches!(
            own(
                Voice {
                    peak_dbfs: f64::NAN,
                    ..base
                },
                rate
            ),
            Err(ComponentError::PeakDbfs { .. })
        ));
    }
}

#[test]
fn excessive_width_names_the_signal_frame_limit() {
    let component = crate::Voice {
        ears: crate::Ears::Both,
        peak_dbfs: -30.0,
        carrier: crate::Carrier::Click {
            rate_hz: 1e-12,
            width_seconds: 68_719_476_737.0 / 48_000.0,
        },
        modulation: None,
    };
    let message = validate::own(component, crate::SampleRate::DEFAULT)
        .unwrap_err()
        .to_string();
    assert!(
        message.contains(&crate::Signal::MAX_FRAMES.to_string()),
        "{message}"
    );
}

#[test]
fn messages_keep_extreme_values_short() {
    let error = ComponentError::Frequency {
        parameter: ParameterId::ToneFrequency,
        hz: 1e300,
        sample_rate_hz: 48_000,
    }
    .to_string();
    assert!(error.contains("1e300 Hz"), "{error}");
    assert!(error.len() < 120, "{error}");
    let tiny = ComponentError::Frequency {
        parameter: ParameterId::ToneFrequency,
        hz: 1e-9,
        sample_rate_hz: 48_000,
    }
    .to_string();
    assert!(tiny.contains("1e-9 Hz"), "{tiny}");
    let ordinary = ComponentError::Frequency {
        parameter: ParameterId::ToneFrequency,
        hz: 30_000.0,
        sample_rate_hz: 48_000,
    }
    .to_string();
    assert!(ordinary.contains("30000 Hz"), "{ordinary}");
}

#[test]
fn length_messages_speak_in_time_not_frames() {
    let duration = StimulusError::Duration {
        seconds: 0.0,
        sample_rate_hz: 48_000,
    }
    .to_string();
    assert_eq!(
        duration,
        "Duration 0 s must last from one sample (1/48000 s) to 16.6 days at 48000 Hz"
    );
    let overlap = StimulusError::FadesOverlap {
        fade_frames: 24_000,
        frames: 48_000,
        sample_rate_hz: 48_000,
    }
    .to_string();
    assert_eq!(
        overlap,
        "a 0.5 s fade at each edge does not fit in 1 s; the two fades together must be shorter than the duration"
    );
}

fn first_voice(sound: &mut Sound) -> &mut Voice {
    let crate::Node::Voice(voice) = &mut sound.components[0].node else {
        panic!("fixture must be a voice")
    };
    voice
}
