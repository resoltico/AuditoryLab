// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::{CarrierFacts, Ears, Modulation, ModulationFacts, ParameterId, Voice};
use alloc::vec;
use alloc::vec::Vec;

fn rate(hz: u32) -> SampleRate {
    SampleRate::new(hz).unwrap()
}

fn id(n: u32) -> crate::ComponentId {
    crate::ComponentId::new(n).unwrap()
}

fn tone(frequency_hz: f64, peak_dbfs: f64) -> Voice {
    Voice {
        ears: Ears::Both,
        peak_dbfs,
        carrier: crate::Carrier::Tone { frequency_hz },
        modulation: None,
    }
}

fn clicks(rate_hz: f64, width_seconds: f64) -> Voice {
    Voice {
        ears: Ears::Both,
        peak_dbfs: -30.0,
        carrier: crate::Carrier::Clicks {
            rate_hz,
            width_seconds,
        },
        modulation: None,
    }
}

fn gate(width_seconds: f64) -> Voice {
    Voice {
        modulation: Some(Modulation::Gate {
            rate_hz: 40.0,
            width_seconds,
        }),
        ..tone(1000.0, -30.0)
    }
}

#[test]
fn requested_and_realized_pulse_widths_keep_their_rate() {
    // Independent oracle: realized seconds are whole frames divided by the rate.
    for (component, hz, frames, realized) in [
        (clicks(40.0, 0.0001), 8_000, 1, 1.0 / 8_000.0),
        (gate(0.0001), 8_000, 1, 1.0 / 8_000.0),
        (clicks(40.0, 0.001), 44_100, 44, 44.0 / 44_100.0),
        (gate(0.001), 44_100, 44, 44.0 / 44_100.0),
    ] {
        let check = crate::test_support::check(rate(hz), &[(id(1), component)]);
        let facts = check.components()[0].1.as_ref().unwrap();
        let pulse = match (facts.carrier, facts.modulation) {
            (CarrierFacts::Clicks(p), _) | (_, Some(ModulationFacts::Gate(p))) => p,
            _ => panic!("pulse facts"),
        };
        assert_eq!(pulse.width_frames, frames);
        assert_eq!(pulse.sample_rate, rate(hz));
        assert_eq!(pulse.realized_width_seconds(), realized);
        assert!(pulse.requested_width_seconds == 0.0001 || pulse.requested_width_seconds == 0.001);
        assert_ne!(
            pulse.realized_width_seconds(),
            pulse.requested_width_seconds
        );
        assert_eq!(pulse.rate_hz, 40.0);
    }
    assert_eq!(1.0_f64 / 8_000.0, 0.000_125);
}

#[test]
fn one_pass_reports_every_row_and_keeps_the_first_whole_target_error() {
    let rows = [
        (id(1), tone(30_000.0, -30.0)),
        (id(2), tone(440.0, -30.0)),
        (id(3), tone(440.0, 1.0)),
        (id(4), clicks(160.0, 0.01)),
    ];
    let check = crate::test_support::check(rate(48_000), &rows);
    let errors: Vec<_> = check
        .components()
        .iter()
        .map(|(id, result)| (*id, result.as_ref().err().map(VoiceError::parameters)))
        .collect();
    assert_eq!(
        errors,
        [
            (id(1).into(), Some(&[ParameterId::ToneFrequency][..])),
            (id(2).into(), None),
            (id(3).into(), Some(&[ParameterId::PeakLevel][..])),
            (
                id(4).into(),
                Some(&[ParameterId::ClickRate, ParameterId::ClickWidth][..])
            ),
        ]
    );
    assert!(matches!(
        check.result(),
        Err(SoundError::Component {
            number: 1,
            error: VoiceError::Frequency {
                parameter: ParameterId::ToneFrequency,
                ..
            }
        })
    ));
    // Coupled and modulation constraints name every participating quantity.
    let sideband = Voice {
        modulation: Some(Modulation::Am {
            rate_hz: 10_000.0,
            depth: 0.5,
        }),
        ..tone(16_000.0, -30.0)
    };
    let depth = Voice {
        modulation: Some(Modulation::Am {
            rate_hz: 40.0,
            depth: 1.5,
        }),
        ..tone(440.0, -30.0)
    };
    let gate_width = gate(0.025);
    for (component, parameters) in [
        (
            sideband,
            &[ParameterId::ToneFrequency, ParameterId::AmRate][..],
        ),
        (depth, &[ParameterId::AmDepth][..]),
        (
            gate_width,
            &[ParameterId::GateRate, ParameterId::GateWidth][..],
        ),
    ] {
        let check = crate::test_support::check(rate(48_000), &[(id(9), component)]);
        let error = check.components()[0].1.as_ref().unwrap_err();
        assert_eq!(error.parameters(), parameters, "{component:?}");
        assert_eq!(check.result().unwrap_err().parameters(), parameters);
    }
}

#[test]
fn identity_count_and_mix_errors_stay_global_with_their_established_precedence() {
    // A duplicate identity precedes that entry's own compilation error.
    let check = crate::test_support::check(
        rate(48_000),
        &[(id(1), tone(440.0, -30.0)), (id(1), tone(30_000.0, -30.0))],
    );
    assert_eq!(
        check.result().unwrap_err(),
        &SoundError::Authored(crate::LoweringError::Identity { id: id(1) })
    );
    assert!(check.result().unwrap_err().parameters().is_empty());
    assert!(
        check.components().is_empty(),
        "invalid structure is refused before compiling any row"
    );
    // Left precedes right; neither is attributed to a field.
    let loud = [
        (
            id(1),
            Voice {
                ears: Ears::Right,
                ..tone(440.0, -3.0)
            },
        ),
        (
            id(2),
            Voice {
                ears: Ears::Right,
                ..tone(441.0, -3.0)
            },
        ),
        (
            id(3),
            Voice {
                ears: Ears::Left,
                ..tone(442.0, -3.0)
            },
        ),
        (
            id(4),
            Voice {
                ears: Ears::Left,
                ..tone(443.0, -3.0)
            },
        ),
    ];
    let check = crate::test_support::check(rate(48_000), &loud);
    assert!(matches!(
        check.result(),
        Err(SoundError::Peak { ear: Ear::Left, .. })
    ));
    assert!(check.result().unwrap_err().parameters().is_empty());
    assert!(check.components().iter().all(|(_, row)| row.is_ok()));
    // Capacity follows MAX_COMPONENTS and over-capacity input compiles nothing.
    for count in [0, Sound::MAX_COMPONENTS, Sound::MAX_COMPONENTS + 1] {
        let rows: Vec<_> = (0..count)
            .map(|n| (id(u32::try_from(n).unwrap() + 1), tone(30_000.0, -60.0)))
            .collect();
        let check = crate::test_support::check(rate(48_000), &rows);
        if count > Sound::MAX_COMPONENTS {
            assert_eq!(
                check.result().unwrap_err(),
                &SoundError::Authored(crate::LoweringError::ComponentBudget { count })
            );
            assert!(check.components().is_empty());
        } else {
            assert_eq!(check.components().len(), count);
        }
    }
}

#[test]
fn finite_entry_points_select_envelope_errors_before_component_errors() {
    let sound = Sound {
        duration_seconds: 0.0,
        fade_seconds: 0.0,
        components: (vec![tone(30_000.0, -30.0)])
            .into_iter()
            .enumerate()
            .map(|(index, voice)| crate::Component {
                id: crate::ComponentId::numbered(index).unwrap(),
                form: crate::ComponentForm::Voice(voice),
            })
            .collect(),
    };
    assert!(matches!(
        sound.signal(rate(48_000)),
        Err(SoundError::Duration { .. })
    ));
    assert!(matches!(
        sound.target(rate(48_000)),
        Err(SoundError::Component { number: 1, .. })
    ));
    // An already valid target checks only its envelope.
    let target = Sound {
        components: (vec![tone(440.0, -30.0)])
            .into_iter()
            .enumerate()
            .map(|(index, voice)| crate::Component {
                id: crate::ComponentId::numbered(index).unwrap(),
                form: crate::ComponentForm::Voice(voice),
            })
            .collect(),
        ..sound.clone()
    }
    .target(rate(48_000))
    .unwrap();
    assert!(matches!(
        target.signal(0.0, 0.0),
        Err(SoundError::Duration { .. })
    ));
    assert!(matches!(
        target.signal(1.0, f64::NAN),
        Err(SoundError::Fade { .. })
    ));
    assert!(matches!(
        target.signal(1.0, 0.5),
        Err(SoundError::FadesOverlap { .. })
    ));
    assert_eq!(
        SoundError::FadesOverlap {
            fade_frames: 1,
            frames: 2,
            sample_rate_hz: 8_000
        }
        .parameters(),
        [ParameterId::Duration, ParameterId::Fade]
    );
}

#[test]
fn a_valid_mix_with_zero_duration_is_a_target_but_not_a_finite_signal() {
    let sound = Sound {
        duration_seconds: 0.0,
        fade_seconds: 0.0,
        components: (vec![tone(440.0, -30.0), clicks(40.0, 0.001)])
            .into_iter()
            .enumerate()
            .map(|(index, voice)| crate::Component {
                id: crate::ComponentId::numbered(index).unwrap(),
                form: crate::ComponentForm::Voice(voice),
            })
            .collect(),
    };
    let target = sound.target(rate(48_000)).unwrap();
    assert_eq!(target.len(), 2);
    assert!(target.signal(0.0, 0.0).is_err());
    // Conversions reuse the compiled mix with its identities, facts and rate.
    let signal = target.signal(1.0, 0.1).unwrap();
    assert_eq!(signal.target(), &target);
    assert_eq!(signal.sample_rate(), target.sample_rate());
    assert!(signal.components().eq(target.components()));
    let ids: Vec<_> = signal.components().map(|(id, _)| id).collect();
    assert_eq!(ids, [id(1).into(), id(2).into()]);
    let via_sound = Sound {
        duration_seconds: 1.0,
        fade_seconds: 0.1,
        ..sound.clone()
    }
    .signal(rate(48_000))
    .unwrap();
    assert_eq!(via_sound, signal);
}

#[test]
fn definition_equality_includes_requested_facts_and_same_audio_does_not() {
    let narrow =
        crate::test_support::target(rate(8_000), &[(id(1), clicks(40.0, 0.000_10))]).unwrap();
    let wider =
        crate::test_support::target(rate(8_000), &[(id(1), clicks(40.0, 0.000_11))]).unwrap();
    // Both widths round to one frame at 8 kHz.
    assert!(narrow.same_audio(&wider));
    assert_ne!(
        narrow, wider,
        "a cache keyed only by same_audio would miss this edit"
    );
    let moved =
        crate::test_support::target(rate(8_000), &[(id(2), clicks(40.0, 0.000_10))]).unwrap();
    assert!(
        !narrow.same_audio(&moved),
        "identity is part of the audio structure"
    );
    let other_rate =
        crate::test_support::target(rate(16_000), &[(id(1), clicks(40.0, 0.000_062_5))]).unwrap();
    assert!(!narrow.same_audio(&other_rate));
    let changed =
        crate::test_support::target(rate(8_000), &[(id(1), clicks(41.0, 0.000_10))]).unwrap();
    assert!(!narrow.same_audio(&changed));
}

#[test]
fn each_component_result_is_found_by_its_own_identity() {
    let check = crate::test_support::check(
        rate(48_000),
        &[(id(7), tone(440.0, -30.0)), (id(3), tone(30_000.0, -30.0))],
    );
    let valid = check.component(id(7).into()).unwrap();
    assert!(matches!(
        valid.as_ref().unwrap().carrier,
        CarrierFacts::Tone(rate) if rate.requested_hz == 440.0
    ));
    assert!(
        check.component(id(3).into()).unwrap().is_err(),
        "the invalid component's own error"
    );
    assert!(
        check.component(id(1).into()).is_none(),
        "an unknown identity has no result"
    );
}

#[test]
fn emptiness_counts_components() {
    assert!(Target::silence(rate(8_000)).is_empty());
    let target = crate::test_support::target(rate(8_000), &[(id(1), tone(440.0, -30.0))]).unwrap();
    assert!(!target.is_empty());
    assert_eq!(target.len(), 1);
}
