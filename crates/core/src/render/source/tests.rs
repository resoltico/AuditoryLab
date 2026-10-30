// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::{ComponentForm, ComponentId, FiniteFixture};
use crate::{Signal, SoundError};
use alloc::{vec, vec::Vec};
use core::f64::consts::TAU;

fn rate(hz: u32) -> SampleRate {
    SampleRate::new(hz).unwrap()
}

fn tone(route: ChannelRouting, frequency_hz: f64, peak_dbfs: f64) -> ComponentForm {
    crate::ComponentForm {
        source: crate::SoundSource::Tone {
            route,
            peak_dbfs: crate::test_support::finite(peak_dbfs),
            frequency_hz: crate::test_support::finite(frequency_hz),
        },
        modulation: None,
    }
}

fn sound(components: Vec<ComponentForm>) -> FiniteFixture {
    crate::test_support::FiniteFixture::new(
        crate::test_support::finite(1.0),
        crate::test_support::finite(0.0),
        components
            .into_iter()
            .enumerate()
            .map(|(index, voice)| crate::Component {
                id: crate::ComponentId::numbered(index).unwrap(),
                form: voice,
            })
            .collect(),
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
}

fn render(sound: &FiniteFixture, hz: u32) -> Vec<[f64; 2]> {
    let session = sound.signal(rate(hz)).unwrap();
    let mut frames = vec![[9.0; 2]; usize::try_from(session.frames()).unwrap()];
    assert_eq!(session.render(0, &mut frames), frames.len());
    frames
}

#[test]
fn each_ear_hears_the_sum_of_its_components() {
    let frames = render(
        &sound(vec![
            tone(ChannelRouting::Left, 250.0, -20.0),
            tone(ChannelRouting::Right, 256.0, -20.0),
            tone(ChannelRouting::Both, 1_000.0, -26.0),
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
fn a_diotic_sound_has_bit_identical_ears_and_one_ear_stimuli_leave_the_other_silent() {
    let diotic = sound(vec![
        tone(ChannelRouting::Both, 396.5, -23.0103),
        tone(ChannelRouting::Both, 403.5, -23.0103),
    ]);
    assert!(!diotic.uses_separate_channels());
    for [left, right] in render(&diotic, 44_100) {
        assert_eq!(left.to_bits(), right.to_bits());
    }
    let one_ear = sound(vec![tone(ChannelRouting::Right, 1_000.0, -20.0)]);
    assert!(one_ear.uses_separate_channels());
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
        let mut separate = sound(vec![
            tone(ChannelRouting::Left, low, -20.0),
            tone(ChannelRouting::Right, high, -20.0),
        ]);
        let mut summed = sound(vec![
            tone(ChannelRouting::Both, low, -23.0103),
            tone(ChannelRouting::Both, high, -23.0103),
        ]);
        separate.set_duration(crate::test_support::finite(2.0));
        summed.set_duration(crate::test_support::finite(2.0));
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
    let mut pair = sound(vec![
        tone(ChannelRouting::Left, 250.0, -20.0),
        tone(ChannelRouting::Both, 256.0, -26.0),
    ]);
    pair.set_duration(crate::test_support::finite(480.0));
    pair.set_fade(crate::test_support::finite(0.1));
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
        sound(vec![]).signal(rate(8_000)).unwrap().peak_dbfs(),
        [f64::NEG_INFINITY; 2]
    );
}

#[test]
fn silence_renders_zeros_for_its_duration() {
    let frames = render(&sound(vec![]), 8_000);
    assert_eq!(frames.len(), 8_000);
    assert!(
        frames
            .iter()
            .all(|f| f[0].to_bits() == 0 && f[1].to_bits() == 0)
    );
}

fn signal_error(sound: &FiniteFixture, hz: u32) -> SoundError {
    sound.signal(rate(hz)).unwrap_err()
}

#[test]
fn durations_and_fades_must_give_whole_frames_that_fit() {
    let mut s = sound(vec![]);
    for seconds in [f64::NAN, f64::INFINITY, -1.0, 0.0, 0.000_01] {
        if !seconds.is_finite() {
            assert!(crate::Finite::new(seconds).is_none());
            assert!(matches!(
                s.target(rate(48_000)).unwrap().signal(seconds, 0.0),
                Err(SoundError::Duration { .. })
            ));
            continue;
        }
        s.set_duration(crate::test_support::finite(seconds));
        assert!(
            matches!(signal_error(&s, 48_000), SoundError::Duration { .. }),
            "{seconds}"
        );
    }
    s.set_duration(crate::test_support::finite(1.0));
    for seconds in [f64::NAN, -0.1, f64::INFINITY] {
        if !seconds.is_finite() {
            assert!(crate::Finite::new(seconds).is_none());
            assert!(matches!(
                s.target(rate(48_000)).unwrap().signal(1.0, seconds),
                Err(SoundError::Fade { .. })
            ));
            continue;
        }
        s.set_fade(crate::test_support::finite(seconds));
        assert!(
            matches!(signal_error(&s, 48_000), SoundError::Fade { .. }),
            "{seconds}"
        );
    }
    s.set_fade(crate::test_support::finite(0.5));
    assert_eq!(
        signal_error(&s, 48_000),
        SoundError::FadesOverlap {
            fade_frames: 24_000,
            frames: 48_000,
            sample_rate_hz: 48_000
        }
    );
    s.set_fade(crate::test_support::finite(0.499_9));
    assert!(s.signal(rate(48_000)).is_ok());
}

// All shared-modulation guards are exercised at the actual component compiler,
// not through an artificial lane carrying its own modulation.
#[test]
fn scalar_component_guards_preserve_the_finite_boundary_cause() {
    let base = tone(ChannelRouting::Both, 1000.0, -30.0);
    let am = |hz, depth| ComponentForm {
        modulation: Some(Modulation::Sinusoidal {
            rate_hz: crate::test_support::finite(hz),
            depth: crate::test_support::finite(depth),
        }),
        ..base
    };
    let mut cases = vec![(base, None), (am(40.0, 0.0), None), (am(40.0, 1.0), None)];
    for depth in [-f64::EPSILON, 1.0_f64.next_up()] {
        cases.push((am(40.0, depth), Some(VoiceError::Depth { depth })));
    }
    for hz in [0.0, -1.0, 24000.0] {
        cases.push((
            am(hz, 0.0),
            Some(VoiceError::Frequency {
                parameter: ParameterId::ModulationRate,
                hz,
                sample_rate_hz: 48000,
            }),
        ));
    }
    for hz in [0.0, -1.0, 24000.0] {
        cases.push((
            tone(ChannelRouting::Both, hz, -30.0),
            Some(VoiceError::Frequency {
                parameter: ParameterId::ToneFrequency,
                hz,
                sample_rate_hz: 48000,
            }),
        ));
    }
    cases.push((tone(ChannelRouting::Both, f64::from_bits(1), -30.0), None));
    cases.push((
        tone(ChannelRouting::Both, 24000.0_f64.next_down(), -30.0),
        None,
    ));
    cases.push((
        tone(ChannelRouting::Both, 1000.0, f64::MIN_POSITIVE),
        Some(VoiceError::PeakDbfs {
            peak_dbfs: f64::MIN_POSITIVE,
        }),
    ));
    for (form, expected) in cases {
        assert_component_cause(form, expected);
    }
}
#[test]
fn pulse_guards_preserve_source_and_shared_modulation_attribution() {
    use crate::PulseError;
    let base = tone(ChannelRouting::Both, 1000.0, -30.0);
    let pulse = |seconds| ComponentForm {
        modulation: Some(Modulation::Pulsed {
            pulse: crate::test_support::rectangular_pulse(40.0, seconds),
        }),
        ..base
    };
    let source_pulse = |hz, seconds| ComponentForm {
        source: crate::SoundSource::PulseTrain {
            route: ChannelRouting::Both,
            peak_dbfs: crate::test_support::finite(-30.0),
            pulse: crate::test_support::rectangular_pulse(hz, seconds),
        },
        modulation: None,
    };
    let mut cases = vec![
        (pulse(1199.0 / 48000.0), None),
        (source_pulse(40.0, 0.5 / 48000.0), None),
        (source_pulse(24000.0_f64.next_down(), 1.0 / 48000.0), None),
    ];
    cases.push((
        source_pulse(40.0, 0.4 / 48000.0),
        Some(VoiceError::Pulse {
            modulation: false,
            width: crate::PulseWidth::Duration {
                seconds: crate::test_support::finite(0.4 / 48000.0),
            },
            rate_hz: 40.0,
            sample_rate_hz: 48000,
            error: PulseError::Width,
        }),
    ));
    cases.push((
        source_pulse(24000.0, 1.0 / 48000.0),
        Some(VoiceError::Pulse {
            modulation: false,
            width: crate::PulseWidth::Duration {
                seconds: crate::test_support::finite(1.0 / 48000.0),
            },
            rate_hz: 24000.0,
            sample_rate_hz: 48000,
            error: PulseError::Rate,
        }),
    ));
    cases.push((
        pulse(1200.0 / 48000.0),
        Some(VoiceError::Pulse {
            modulation: true,
            width: crate::PulseWidth::Duration {
                seconds: crate::test_support::finite(1200.0 / 48000.0),
            },
            rate_hz: 40.0,
            sample_rate_hz: 48000,
            error: PulseError::ClosedFrame,
        }),
    ));
    for (form, expected) in cases {
        assert_component_cause(form, expected);
    }
}
fn assert_component_cause(form: ComponentForm, expected: Option<VoiceError>) {
    let input = sound(vec![form]);
    let check = input.check(rate(48000));
    assert_eq!(check.voices()[0].0, ComponentId::new(1).unwrap().into());
    assert_eq!(
        check.voices()[0].1.as_ref().err(),
        expected.as_ref(),
        "{form:?}"
    );
    match (expected, input.signal(rate(48000))) {
        (None, Ok(_)) => {}
        (Some(expected), Err(SoundError::Voice { id, error })) => {
            assert_eq!(id, ComponentId::new(1).unwrap().into());
            assert_eq!(error, expected);
        }
        (expected, actual) => panic!("{expected:?}: {actual:?}"),
    }
}
#[test]
fn shared_am_sideband_guard_is_inactive_only_at_zero_depth() {
    for depth in [0.0, f64::from_bits(1), 1.0] {
        for (hz, active_valid) in [
            (23960.0_f64.next_down(), true),
            (23960.0, false),
            (23999.0, false),
        ] {
            let mut form = tone(ChannelRouting::Both, hz, -30.0);
            form.modulation = Some(Modulation::Sinusoidal {
                rate_hz: crate::test_support::finite(40.0),
                depth: crate::test_support::finite(depth),
            });
            assert_eq!(
                sound(vec![form]).signal(rate(48000)).is_ok(),
                depth == 0.0 || active_valid
            );
        }
    }
}
#[test]
fn mix_guards_cover_peak_capacity_and_terminal_frame_neighbours() {
    let mut input = sound(vec![tone(ChannelRouting::Both, 1000.0, 0.0)]);
    assert!(input.signal(rate(48000)).is_ok());
    input
        .push(crate::Component {
            id: ComponentId::new(2).unwrap(),
            form: tone(ChannelRouting::Both, 1000.0, -30.0),
        })
        .unwrap();
    assert!(matches!(
        input.signal(rate(48000)),
        Err(SoundError::Peak { .. })
    ));
    let mut full = sound(vec![tone(ChannelRouting::Both, 1000.0, -60.0); 16]);
    assert!(full.signal(rate(48000)).is_ok());
    let duplicate = full.components()[0];
    assert_eq!(
        full.push(duplicate),
        Err(crate::StructureError::VoiceBudget { count: 17 })
    );
    let mut silent = sound(vec![]);
    silent.set_duration(crate::test_support::finite(
        rate(48000).seconds_in(Signal::MAX_FRAMES),
    ));
    assert!(silent.signal(rate(48000)).is_ok());
    silent.set_duration(crate::test_support::finite(
        rate(48000).seconds_in(Signal::MAX_FRAMES + 1),
    ));
    assert!(silent.signal(rate(48000)).is_err());
}
#[test]
fn raw_source_boundary_keeps_its_nonfinite_safeguards() {
    // These values cannot inhabit an authored Finite; this tests the remaining
    // private f64 compilation boundary directly, without any lane modulator.
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(crate::Finite::new(bad).is_none());
        let bad_level = SourceLane {
            route: ChannelRouting::Both,
            peak_dbfs: bad,
            carrier: Carrier::Tone {
                frequency_hz: 440.0,
            },
        };
        assert!(matches!(
            bad_level.compile(rate(48000)),
            Err(VoiceError::PeakDbfs { .. })
        ));
        assert!(frequency_step(ParameterId::ToneFrequency, bad, rate(48000)).is_err());
        assert!(frequency_step(ParameterId::ModulationRate, bad, rate(48000)).is_err());
    }
}
