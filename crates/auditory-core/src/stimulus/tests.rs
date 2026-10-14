// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use alloc::string::ToString;
use alloc::vec;
use core::cell::Cell;
use core::f64::consts::TAU;
use proptest::{prelude::*, test_runner::Config, test_runner::TestRunner};

fn rate(hz: u32) -> SampleRate {
    SampleRate::new(hz).unwrap()
}

fn tone(ears: Ears, frequency_hz: f64, peak_dbfs: f64) -> Component {
    Component::Tone {
        ears,
        frequency_hz,
        peak_dbfs,
        modulation: None,
    }
}

fn stimulus(components: Vec<Component>) -> Stimulus {
    Stimulus {
        duration_seconds: 1.0,
        fade_seconds: 0.0,
        components,
    }
}

fn render(stimulus: &Stimulus, hz: u32) -> Vec<[f64; 2]> {
    let session = stimulus.session(rate(hz)).unwrap();
    let mut frames = vec![[9.0; 2]; usize::try_from(session.frames()).unwrap()];
    assert_eq!(session.into_renderer().render(&mut frames), frames.len());
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
    assert!(!diotic.uses_separate_ear_routing());
    for [left, right] in render(&diotic, 44_100) {
        assert_eq!(left.to_bits(), right.to_bits());
    }
    let one_ear = stimulus(vec![tone(Ears::Right, 1_000.0, -20.0)]);
    assert!(one_ear.uses_separate_ear_routing());
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
fn a_session_states_its_frames_fades_and_peaks() {
    let mut pair = stimulus(vec![
        tone(Ears::Left, 250.0, -20.0),
        tone(Ears::Both, 256.0, -26.0),
    ]);
    pair.duration_seconds = 480.0;
    pair.fade_seconds = 0.1;
    let session = pair.session(rate(44_100)).unwrap();
    assert_eq!(session.sample_rate(), rate(44_100));
    assert_eq!(session.frames(), 21_168_000);
    assert_eq!(session.fade_frames(), 4_410);
    assert_eq!(session.duration_seconds(), 480.0);
    assert_eq!(session.fade_seconds(), 0.1);
    let [left, right] = session.peak_dbfs();
    assert!((left - 20.0 * (0.1 + 10_f64.powf(-1.3)).log10()).abs() < 1e-9);
    assert!((right + 26.0).abs() < 1e-9);
    assert_eq!(
        stimulus(vec![]).session(rate(8_000)).unwrap().peak_dbfs(),
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

fn session_error(stimulus: &Stimulus, hz: u32) -> StimulusError {
    stimulus.session(rate(hz)).unwrap_err()
}

#[test]
fn durations_and_fades_must_give_whole_frames_that_fit() {
    let mut s = stimulus(vec![]);
    for seconds in [f64::NAN, f64::INFINITY, -1.0, 0.0, 0.000_01] {
        s.duration_seconds = seconds;
        assert!(
            matches!(session_error(&s, 48_000), StimulusError::Duration { .. }),
            "{seconds}"
        );
    }
    s.duration_seconds = 1.0;
    for seconds in [f64::NAN, -0.1, f64::INFINITY] {
        s.fade_seconds = seconds;
        assert!(
            matches!(session_error(&s, 48_000), StimulusError::Fade { .. }),
            "{seconds}"
        );
    }
    s.fade_seconds = 0.5;
    assert_eq!(
        session_error(&s, 48_000),
        StimulusError::FadesOverlap {
            fade_frames: 24_000,
            frames: 48_000
        }
    );
    s.fade_seconds = 0.499_9;
    assert!(s.session(rate(48_000)).is_ok());
}

#[test]
fn component_values_are_checked_and_blamed_on_their_component_and_field() {
    let modulated = |frequency_hz, rate_hz, depth| Component::Tone {
        ears: Ears::Left,
        frequency_hz,
        peak_dbfs: -20.0,
        modulation: Some(Modulation::Sinusoidal { rate_hz, depth }),
    };
    let gated = |rate_hz, width_seconds| Component::Tone {
        ears: Ears::Left,
        frequency_hz: 1_000.0,
        peak_dbfs: -20.0,
        modulation: Some(Modulation::Rectangular {
            rate_hz,
            width_seconds,
        }),
    };
    let click = |rate_hz, width_seconds| Component::Click {
        ears: Ears::Both,
        rate_hz,
        width_seconds,
        peak_dbfs: -20.0,
        modulation: None,
    };
    for (component, expected) in [
        (
            tone(Ears::Left, 24_000.0, -20.0),
            "frequency_hz 24000 Hz must be finite, positive and below 24000 Hz, half the 48000 Hz sample rate",
        ),
        (
            tone(Ears::Left, 0.0, -20.0),
            "frequency_hz 0 Hz must be finite, positive and below 24000 Hz, half the 48000 Hz sample rate",
        ),
        (
            tone(Ears::Left, f64::NAN, -20.0),
            "frequency_hz NaN Hz must be finite, positive and below 24000 Hz, half the 48000 Hz sample rate",
        ),
        (
            tone(Ears::Left, 440.0, 0.1),
            "peak_dbfs 0.1 must be finite and at most 0",
        ),
        (
            tone(Ears::Left, 440.0, f64::NEG_INFINITY),
            "peak_dbfs -inf must be finite and at most 0",
        ),
        (
            modulated(1_000.0, -1.0, 1.0),
            "modulation rate_hz -1 Hz must be finite, positive and below 24000 Hz, half the 48000 Hz sample rate",
        ),
        (
            modulated(1_000.0, 40.0, 1.5),
            "modulation depth 1.5 must be from 0 to 1",
        ),
        (
            modulated(23_970.0, 40.0, 1.0),
            "frequency_hz 23970 Hz plus modulation rate_hz 40 Hz must stay below 24000 Hz, half the 48000 Hz sample rate",
        ),
        (
            click(40.0, 0.000_01),
            "width_seconds 0.00001 must be at least one frame (0.000020833333333333333 s at 48000 Hz) and shorter than the 0.025 s period at 40 Hz",
        ),
        (
            click(40.0, 0.025),
            "width_seconds 0.025 must be at least one frame (0.000020833333333333333 s at 48000 Hz) and shorter than the 0.025 s period at 40 Hz",
        ),
        (
            gated(40.0, 0.000_01),
            "modulation width_seconds 0.00001 must be at least one frame (0.000020833333333333333 s at 48000 Hz) and shorter than the 0.025 s period at 40 Hz",
        ),
        (
            gated(40.0, 0.025),
            "modulation width_seconds 0.025 must be at least one frame (0.000020833333333333333 s at 48000 Hz) and shorter than the 0.025 s period at 40 Hz",
        ),
        (
            gated(30_000.0, 0.000_01),
            "modulation rate_hz 30000 Hz must be finite, positive and below 24000 Hz, half the 48000 Hz sample rate",
        ),
        (
            gated(f64::from_bits(1), 1.0),
            "modulation rate_hz 5e-324 Hz is too small for accurate pulse timing at 48000 Hz; raise modulation rate_hz",
        ),
        (
            click(30_000.0, 0.000_01),
            "rate_hz 30000 Hz must be finite, positive and below 24000 Hz, half the 48000 Hz sample rate",
        ),
    ] {
        let error = session_error(
            &stimulus(vec![tone(Ears::Right, 250.0, -20.0), component]),
            48_000,
        );
        assert!(
            matches!(error, StimulusError::Component { number: 2, .. }),
            "{error:?}"
        );
        assert_eq!(error.to_string(), alloc::format!("component 2: {expected}"));
    }
    // A positive frequency too small to advance the phase is rejected too.
    let underflow = session_error(&stimulus(vec![tone(Ears::Left, 5e-324, -20.0)]), 48_000);
    assert!(matches!(
        underflow,
        StimulusError::Component {
            number: 1,
            error: ComponentError::PhaseStepTooSmall {
                field: "frequency_hz",
                ..
            }
        }
    ));
    // A sum above Nyquist is linear addition, not a sideband: legal.
    assert!(
        stimulus(vec![
            tone(Ears::Both, 15_000.0, -10.0),
            tone(Ears::Both, 15_000.0, -10.0)
        ])
        .session(rate(48_000))
        .is_ok()
    );
    // A click's own spectrum is broadband; only its rate is checked against Nyquist.
    let modulated_click = Component::Click {
        ears: Ears::Both,
        rate_hz: 40.0,
        width_seconds: 0.001,
        peak_dbfs: -20.0,
        modulation: Some(Modulation::Sinusoidal {
            rate_hz: 2.0,
            depth: 1.0,
        }),
    };
    assert!(
        stimulus(vec![modulated_click])
            .session(rate(48_000))
            .is_ok()
    );
}

#[test]
fn a_frequency_valid_at_one_rate_can_be_invalid_at_another() {
    let s = stimulus(vec![tone(Ears::Both, 23_000.0, -20.0)]);
    assert!(s.session(rate(48_000)).is_ok());
    assert_eq!(
        session_error(&s, 44_100).to_string(),
        "component 1: frequency_hz 23000 Hz must be finite, positive and below 22050 Hz, half the 44100 Hz sample rate"
    );
}

#[test]
fn each_ear_may_reach_but_not_exceed_full_scale() {
    let half = -6.020_599_913_279_624;
    let full = stimulus(vec![
        tone(Ears::Left, 250.0, half),
        tone(Ears::Both, 256.0, half),
    ]);
    let session = full.session(rate(8_000));
    assert!(session.is_ok(), "{session:?}");
    let over = stimulus(vec![
        tone(Ears::Right, 250.0, -6.0),
        tone(Ears::Both, 256.0, half),
    ]);
    let error = session_error(&over, 8_000);
    assert!(
        matches!(
            error,
            StimulusError::Peak {
                ear: Ear::Right,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "the components for the right ear add up to a peak of 0.01 dBFS, above full scale; lower their peak_dbfs"
    );
    let many = stimulus(vec![
        tone(Ears::Both, 250.0, -40.0);
        Stimulus::MAX_COMPONENTS + 1
    ]);
    assert_eq!(
        session_error(&many, 8_000),
        StimulusError::TooManyComponents { count: 17 }
    );
    assert!(
        stimulus(vec![
            tone(Ears::Both, 250.0, -40.0);
            Stimulus::MAX_COMPONENTS
        ])
        .session(rate(8_000))
        .is_ok()
    );
}

#[test]
fn components_describe_themselves() {
    assert_eq!(
        tone(Ears::Left, 396.5, -20.0).to_string(),
        "tone 396.5 Hz, peak -20 dBFS, left ear"
    );
    assert_eq!(
        Component::Tone {
            ears: Ears::Right,
            frequency_hz: 1_000.0,
            peak_dbfs: -20.0,
            modulation: Some(Modulation::Sinusoidal {
                rate_hz: 39.0625,
                depth: 1.0
            })
        }
        .to_string(),
        "tone 1000 Hz, peak -20 dBFS, sinusoidal modulation 39.0625 Hz, depth 1, right ear"
    );
    assert_eq!(
        Component::Click {
            ears: Ears::Both,
            rate_hz: 40.0,
            width_seconds: 0.001,
            peak_dbfs: -20.0,
            modulation: None
        }
        .to_string(),
        "clicks 40 Hz, 0.001 s wide, peak -20 dBFS, both ears"
    );
    assert_eq!(
        Component::Tone {
            ears: Ears::Both,
            frequency_hz: 5_000.0,
            peak_dbfs: -20.0,
            modulation: Some(Modulation::Rectangular {
                rate_hz: 40.0,
                width_seconds: 0.001
            })
        }
        .to_string(),
        "tone 5000 Hz, peak -20 dBFS, rectangular modulation 40 Hz, 0.001 s open, both ears"
    );
}

/// Components whose peaks fill each ear to exactly full scale, in any mixture of kinds.
fn full_scale_components() -> impl Strategy<Value = Vec<Component>> {
    let kind = prop_oneof![
        (1.0..3_000.0_f64).prop_map(|hz| (hz, None, false)),
        (1.0..3_000.0_f64, 0.5..200.0_f64, 0.0..=1.0_f64).prop_map(|(hz, rate, depth)| (
            hz,
            Some(Modulation::Sinusoidal {
                rate_hz: rate,
                depth
            }),
            false
        )),
        (1.0..300.0_f64).prop_map(|hz| (hz, None, true)),
        (1.0..3_000.0_f64, 1.0..200.0_f64).prop_map(|(hz, rate)| (
            hz,
            Some(Modulation::Rectangular {
                rate_hz: rate,
                width_seconds: 0.4 / rate
            }),
            false
        )),
    ];
    proptest::collection::vec((kind, 1u32..=100), 1..=Stimulus::MAX_COMPONENTS).prop_map(|parts| {
        let total: u32 = parts.iter().map(|(_, weight)| weight).sum();
        parts
            .into_iter()
            .map(|((hz, modulation, click), weight)| {
                let peak_dbfs = 20.0 * (f64::from(weight) / f64::from(total)).log10();
                if click {
                    Component::Click {
                        ears: Ears::Both,
                        rate_hz: hz,
                        width_seconds: 0.4 / hz,
                        peak_dbfs,
                        modulation,
                    }
                } else {
                    Component::Tone {
                        ears: Ears::Both,
                        frequency_hz: hz,
                        peak_dbfs,
                        modulation,
                    }
                }
            })
            .collect()
    })
}

#[test]
fn rendered_samples_never_exceed_the_peak_the_session_states() {
    let config = Config {
        cases: 128,
        max_shrink_iters: 256,
        max_shrink_time: 1_000,
        failure_persistence: None,
        ..Config::default()
    };
    // A narrow skip budget for the weight -> dB -> amplitude round trip
    // and up to 16 positive additions: 32 ULP at one, about 7.1e-15.
    // It never relaxes the permitted sample range or lowers input peaks.
    const ROUNDING_PEAK_LIMIT: f64 = 1.0 + 32.0 * f64::EPSILON;
    let rendered_cases = Cell::new(0_usize);
    TestRunner::new(config)
        .run(&full_scale_components(), |components| {
            let s = stimulus(components);
            let session = match s.session(rate(8_000)) {
                Ok(session) => session,
                Err(error) => {
                    // Every generated component reaches Both ears; the first
                    // rejected peak must therefore be Left, just above one.
                    prop_assert!(
                        matches!(error, StimulusError::Peak { ear: Ear::Left, amplitude }
                            if amplitude > 1.0 && amplitude <= ROUNDING_PEAK_LIMIT),
                        "unexpected Session rejection: {error}"
                    );
                    return Ok(());
                }
            };
            let bound = 10_f64.powf(session.peak_dbfs()[0] / 20.0).min(1.0);
            let mut frames = vec![[0.0; 2]; 8_000];
            session.into_renderer().render(&mut frames);
            for [left, right] in frames {
                prop_assert!(left.is_finite() && left.abs() <= 1.0);
                prop_assert_eq!(left.to_bits(), right.to_bits());
                prop_assert!(left.abs() <= bound * (1.0 + 1e-12));
            }
            rendered_cases.set(rendered_cases.get() + 1);
            Ok(())
        })
        .unwrap();
    assert!(
        rendered_cases.get() > 0,
        "all full-scale cases were skipped"
    );
}

#[test]
fn rendering_is_independent_of_how_the_caller_divides_it_into_buffers() {
    let config = Config {
        cases: 64,
        max_shrink_iters: 256,
        max_shrink_time: 1_000,
        failure_persistence: None,
        ..Config::default()
    };
    let s = Stimulus {
        duration_seconds: 0.05,
        fade_seconds: 0.01,
        components: vec![
            tone(Ears::Left, 250.0, -20.0),
            Component::Click {
                ears: Ears::Right,
                rate_hz: 40.0,
                width_seconds: 0.001,
                peak_dbfs: -20.0,
                modulation: None,
            },
            Component::Tone {
                ears: Ears::Both,
                frequency_hz: 1_000.0,
                peak_dbfs: -20.0,
                modulation: Some(Modulation::Sinusoidal {
                    rate_hz: 39.0625,
                    depth: 1.0,
                }),
            },
        ],
    };
    let whole = render(&s, 44_100);
    TestRunner::new(config)
        .run(&proptest::collection::vec(0usize..=700, 0..=12), |blocks| {
            let mut renderer = s.session(rate(44_100)).unwrap().into_renderer();
            let mut parts = Vec::new();
            for count in blocks.into_iter().chain([whole.len() + 3]) {
                let mut buffer = vec![[7.0; 2]; count];
                let produced = renderer.render(&mut buffer);
                prop_assert!(
                    buffer[produced..]
                        .iter()
                        .all(|f| f[0].to_bits() == 0 && f[1].to_bits() == 0)
                );
                parts.extend_from_slice(&buffer[..produced]);
            }
            prop_assert!(renderer.is_complete());
            let bits = |frames: &[[f64; 2]]| {
                frames
                    .iter()
                    .flat_map(|f| f.map(f64::to_bits))
                    .collect::<Vec<_>>()
            };
            prop_assert_eq!(bits(&parts), bits(&whole));
            Ok(())
        })
        .unwrap();
}

#[test]
fn separate_ear_routing_does_not_imply_different_waveforms() {
    let separate = stimulus(vec![
        tone(Ears::Left, 200.0, -20.0),
        tone(Ears::Right, 200.0, -20.0),
    ]);
    assert!(separate.uses_separate_ear_routing());
    assert!(
        render(&separate, 8_000)
            .iter()
            .all(|[left, right]| left.to_bits() == right.to_bits())
    );
}

#[test]
fn click_rates_reject_zero_and_subnormal_phase_steps_without_narrowing_tone_rates() {
    for rate_hz in [f64::from_bits(1), f64::from_bits(52_800)] {
        let component = Component::Click {
            ears: Ears::Both,
            rate_hz,
            width_seconds: 1.0,
            peak_dbfs: -20.0,
            modulation: None,
        };
        let error = session_error(&stimulus(vec![component]), 48_000);
        assert_eq!(
            error,
            StimulusError::Component {
                number: 1,
                error: ComponentError::PulseRateTooSmall {
                    field: "rate_hz",
                    rate_hz,
                    sample_rate_hz: 48_000,
                },
            }
        );
        let text = error.to_string();
        assert!(
            text.contains("too small for accurate pulse timing at 48000 Hz"),
            "{text}"
        );
    }
    // The normal/subnormal boundary is deliberate and click-specific.
    let smallest_normal_step_rate = f64::MIN_POSITIVE * 48_000.0;
    assert!(
        stimulus(vec![Component::Click {
            ears: Ears::Both,
            rate_hz: smallest_normal_step_rate,
            width_seconds: 1.0,
            peak_dbfs: -20.0,
            modulation: None,
        }])
        .session(rate(48_000))
        .is_ok()
    );
    assert!(
        stimulus(vec![tone(Ears::Both, f64::from_bits(52_800), -20.0)])
            .session(rate(48_000))
            .is_ok()
    );
}

#[test]
fn phase_underflow_diagnostics_preserve_adjacent_subnormal_steps() {
    let modulated = |frequency_hz, rate_hz, depth| Component::Tone {
        ears: Ears::Left,
        frequency_hz,
        peak_dbfs: -20.0,
        modulation: Some(Modulation::Sinusoidal { rate_hz, depth }),
    };
    for bits in [24_000, 24_001] {
        let hz = f64::from_bits(bits);
        for component in [tone(Ears::Left, hz, -20.0), modulated(250.0, hz, 1.0)] {
            let result = stimulus(vec![component]).session(rate(48_000));
            assert_eq!(result.is_ok(), bits == 24_001);
            if let Err(error) = result {
                let text = error.to_string();
                assert!(text.contains("too small to advance the phase"));
                assert!(text.contains("raise "));
            }
        }
    }
    for component in [
        tone(Ears::Left, f64::from_bits(1), -20.0),
        modulated(0.1, f64::from_bits(1), 1.0),
    ] {
        assert!(stimulus(vec![component]).session(rate(1)).is_ok());
    }
}

#[test]
fn above_full_scale_diagnostics_never_round_positive_excess_to_zero() {
    for amplitude in [f64::from_bits(1.0_f64.to_bits() + 1), 1.0 + 1e-14] {
        let text = StimulusError::Peak {
            ear: Ear::Left,
            amplitude,
        }
        .to_string();
        let level = text
            .split("peak of ")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap();
        assert!(level.parse::<f64>().unwrap() > 0.0);
    }
    let text = StimulusError::Peak {
        ear: Ear::Right,
        amplitude: 1.5,
    }
    .to_string();
    assert!(text.contains("3.52 dBFS"));
    let error = session_error(
        &stimulus(vec![
            tone(Ears::Both, 250.0, 0.0),
            tone(Ears::Both, 256.0, -280.0),
        ]),
        48_000,
    );
    assert!(matches!(error, StimulusError::Peak { .. }));
    assert!(!error.to_string().contains("peak of 0.00"));
}
