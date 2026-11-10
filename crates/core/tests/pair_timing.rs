//! Shared-pair model meaning and independent finite audio controls.
use auditorylab_core::{
    CarrierFacts, ChannelRouting, ComponentForm, ComponentId, Lane, Modulation, PairKind,
    PulseShape, PulseSpec, PulseWidth, SampleRate, SoundSource,
    test_support::{finite, recipe},
    timing::{ModelKind, ModelOwner},
};
fn pair(kind: PairKind, first_level: f64, modulation: Option<Modulation>) -> ComponentForm {
    let first_hz = finite(200.0);
    let offset_hz = finite(-7.25);
    let first_peak_dbfs = finite(first_level);
    let second_peak_dbfs = finite(-30.0);
    let source = match kind {
        PairKind::Binaural => SoundSource::BinauralPair {
            first_hz,
            offset_hz,
            first_peak_dbfs,
            second_peak_dbfs,
        },
        PairKind::Monaural => SoundSource::MonauralPair {
            route: ChannelRouting::Both,
            first_hz,
            offset_hz,
            first_peak_dbfs,
            second_peak_dbfs,
        },
    };
    ComponentForm { source, modulation }
}
fn pulse() -> PulseSpec {
    PulseSpec {
        rate_hz: finite(40.0),
        width: PulseWidth::Duration {
            seconds: finite(0.001),
        },
        shape: PulseShape::Rectangular {},
    }
}
#[test]
fn active_pair_gain_is_one_component_process_even_with_a_muted_primary() {
    let rate = SampleRate::new(48_000).unwrap();
    for kind in [PairKind::Binaural, PairKind::Monaural] {
        for first_level in [-30.0, -10_000.0] {
            for (modulation, expected) in [
                (None, None),
                (
                    Some(Modulation::Sinusoidal {
                        rate_hz: finite(40.0),
                        depth: finite(0.0),
                    }),
                    None,
                ),
                (
                    Some(Modulation::Sinusoidal {
                        rate_hz: finite(40.0),
                        depth: finite(0.5),
                    }),
                    Some(ModelKind::Envelope),
                ),
                (
                    Some(Modulation::Pulsed { pulse: pulse() }),
                    Some(ModelKind::Pulsed),
                ),
                (
                    Some(Modulation::Pulsed {
                        pulse: PulseSpec {
                            shape: PulseShape::RaisedCosine {
                                rise_seconds: finite(0.0001),
                                fall_seconds: finite(0.0001),
                            },
                            ..pulse()
                        },
                    }),
                    Some(ModelKind::Pulsed),
                ),
            ] {
                let r = recipe(0.1, 0.0, &[pair(kind, first_level, modulation)]);
                let target = r.sound().target(rate).unwrap();
                let model = target.timing_model(usize::MAX);
                let component = ComponentId::new(1).unwrap();
                if let Some(expected) = expected {
                    assert_eq!(model.lanes.len(), 1);
                    assert_eq!(model.lanes[0].kind, expected);
                    assert_eq!(model.lanes[0].owner, ModelOwner::Component(component));
                    assert!(
                        model.lanes[0]
                            .columns
                            .iter()
                            .any(|extent| extent.min < extent.max)
                    );
                } else {
                    assert_eq!(model.lanes.len(), 2);
                    assert!(
                        model
                            .lanes
                            .iter()
                            .all(|lane| lane.kind == ModelKind::Steady)
                    );
                    assert_eq!(
                        model.lanes[0].owner,
                        ModelOwner::Voice(component.with_lane(Lane::Primary))
                    );
                    assert_eq!(
                        model.lanes[1].owner,
                        ModelOwner::Voice(component.with_lane(Lane::Secondary))
                    );
                }
                assert!(model.lanes.iter().all(|lane| lane.columns.len() <= 512));
                let facts: Vec<_> = target.voices().map(|(id, facts)| (id, *facts)).collect();
                assert_eq!(facts.len(), 2);
                assert_eq!(facts[0].1.peak_dbfs, first_level);
                assert_eq!(facts[1].1.peak_dbfs, -30.0);
                let expected_routes = if kind == PairKind::Binaural {
                    [ChannelRouting::Left, ChannelRouting::Right]
                } else {
                    [ChannelRouting::Both; 2]
                };
                for ((_, facts), (hz, route)) in facts
                    .iter()
                    .zip([200.0, 192.75].into_iter().zip(expected_routes))
                {
                    let CarrierFacts::Tone(tone) = facts.carrier else {
                        panic!("pair source remains a tone")
                    };
                    assert_eq!(tone.requested_hz, hz);
                    assert!((tone.realized_hz - hz).abs() < 1e-10);
                    assert_eq!(facts.route, route);
                }
            }
        }
    }
}
#[test]
fn source_pulses_and_shared_gain_remain_two_distinct_processes() {
    let r = recipe(
        0.1,
        0.0,
        &[ComponentForm {
            source: SoundSource::PulseTrain {
                route: ChannelRouting::Both,
                peak_dbfs: finite(-30.0),
                pulse: pulse(),
            },
            modulation: Some(Modulation::Sinusoidal {
                rate_hz: finite(8.0),
                depth: finite(0.5),
            }),
        }],
    );
    let model = r
        .sound()
        .target(SampleRate::new(48_000).unwrap())
        .unwrap()
        .timing_model(32);
    assert_eq!(model.lanes.len(), 2);
    assert_eq!(model.lanes[0].kind, ModelKind::PulseTrain);
    assert!(matches!(model.lanes[0].owner, ModelOwner::Voice(_)));
    assert_eq!(model.lanes[1].kind, ModelKind::Envelope);
    assert!(matches!(model.lanes[1].owner, ModelOwner::Component(_)));
}
#[test]
fn both_pair_tones_follow_independent_open_closed_finite_sample_windows() {
    let rate = SampleRate::new(48_000).unwrap();
    for kind in [PairKind::Binaural, PairKind::Monaural] {
        for first_level in [-30.0, -10_000.0] {
            let r = recipe(
                0.1,
                0.0,
                &[pair(
                    kind,
                    first_level,
                    Some(Modulation::Pulsed { pulse: pulse() }),
                )],
            );
            let signal = r.signal(rate).unwrap();
            let mut frames = vec![[0.0; 2]; 4800];
            assert_eq!(signal.render(0, &mut frames), 4800);
            let amplitude = 10_f64.powf(-30.0 / 20.0);
            let first_amplitude = 10_f64.powf(first_level / 20.0);
            let mut audible = [0; 2];
            for (n, actual) in frames.iter().enumerate() {
                if n % 1200 >= 48 {
                    assert_eq!(*actual, [0.0; 2], "closed window at {n}");
                } else {
                    let time = f64::from(u32::try_from(n).unwrap()) / 48_000.0;
                    let first = first_amplitude * (std::f64::consts::TAU * 200.0 * time).sin();
                    let second = amplitude * (std::f64::consts::TAU * 192.75 * time).sin();
                    let expected = if kind == PairKind::Binaural {
                        [first, second]
                    } else {
                        [first + second; 2]
                    };
                    for channel in 0..2 {
                        assert!(
                            (actual[channel] - expected[channel]).abs() < 1e-12,
                            "open {kind:?} {n} {channel}"
                        );
                        audible[channel] += usize::from(actual[channel] != 0.0);
                    }
                }
            }
            if kind == PairKind::Binaural && first_level == -10_000.0 {
                assert_eq!(audible[0], 0);
                assert!(audible[1] > 100);
            } else {
                assert!(audible.iter().all(|count| *count > 100));
            }
        }
    }
}
