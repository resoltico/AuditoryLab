//! Timing-model oracles use integer rationals, never the projection's pulse helper.
use auditorylab_core::{
    Carrier, Component, Ears, Modulation, SampleRate,
    live::{Engine, Target},
    timing::ModelKind,
};
fn clicks(hz: f64, width: f64) -> Component {
    Component {
        ears: Ears::Both,
        peak_dbfs: -30.0,
        carrier: Carrier::Click {
            rate_hz: hz,
            width_seconds: width,
        },
        modulation: None,
    }
}
#[test]
fn every_column_contains_all_rational_pulses_including_single_frame_impulses() {
    for fs in [44100, 48000, 768000] {
        for (num, den, width) in [
            (8_u64, 1_u64, 0.001),
            (625, 16, 0.001),
            (160, 1, 1.0 / f64::from(fs)),
        ] {
            let rate = SampleRate::new(fs).unwrap();
            let hz =
                f64::from(u32::try_from(num).unwrap()) / f64::from(u32::try_from(den).unwrap());
            let target = Target::new(rate, &[(7, clicks(hz, width))]).unwrap();
            let model = target.timing_model(257);
            let lane = &model.lanes[0];
            assert_eq!(lane.kind, ModelKind::Clicks);
            assert_eq!(lane.id, 7);
            let w = if width == 0.001 {
                if fs == 44100 {
                    44
                } else {
                    u64::from(fs) / 1000
                }
            } else {
                1
            };
            let period = |n: u64| n * num / (u64::from(fs) * den);
            let open = |n: u64| n < w || period(n) > period(n - w);
            let mut expected = vec![(false, true); lane.columns.len()];
            for n in 0..model.frames {
                let i =
                    usize::try_from(n * u64::try_from(lane.columns.len()).unwrap() / model.frames)
                        .unwrap();
                let active = open(n);
                expected[i].0 |= active;
                expected[i].1 &= active;
            }
            for (actual, (any, all)) in lane.columns.iter().zip(expected) {
                assert_eq!(actual.max, f64::from(u8::from(any)));
                assert_eq!(actual.min, f64::from(u8::from(all)));
            }
        }
    }
}
#[test]
fn gate_projection_matches_signal_and_dense_extrema_defeat_point_sampling() {
    let rate = SampleRate::new(48000).unwrap();
    let component = Component {
        carrier: Carrier::Tone {
            frequency_hz: 1000.0,
        },
        modulation: Some(Modulation::Rectangular {
            rate_hz: 40.0,
            width_seconds: 1.0 / 48000.0,
        }),
        ..clicks(40.0, 0.001)
    };
    let model = Target::new(rate, &[(1, component)])
        .unwrap()
        .timing_model(1);
    assert_eq!(model.lanes[0].kind, ModelKind::Gate);
    assert_eq!(model.lanes[0].columns[0].max, 1.0);
    assert_eq!(model.lanes[0].columns[0].min, 0.0);
    // Negative control: the single point at a selected point just after the midpoint misses every one-frame pulse.
    let point = model.frames / 2 + 1;
    assert_ne!(point % 1200, 0);
    let signal = auditorylab_core::Stimulus {
        duration_seconds: 0.25,
        fade_seconds: 0.0,
        components: vec![component],
    }
    .signal(rate)
    .unwrap();
    let mut out = [[0.0; 2]; 1201];
    signal.render(0, &mut out);
    assert!(
        out.iter()
            .enumerate()
            .all(|(n, v)| n % 1200 == 0 || *v == [0.0; 2])
    );
}
#[test]
fn bounded_spans_multiple_processes_and_observation_independence() {
    let rate = SampleRate::new(48000).unwrap();
    let c = clicks(0.1, 0.001);
    let muted = Target::new(
        rate,
        &[(
            1,
            Component {
                peak_dbfs: -10000.0,
                ..c
            },
        )],
    )
    .unwrap()
    .timing_model(512);
    assert!(muted.lanes.is_empty());
    assert_eq!(muted.frames, 12000);
    let t = Target::new(rate, &[(1, c)]).unwrap();
    let m = t.timing_model(usize::MAX);
    assert_eq!(m.frames, 192000);
    assert!(m.insufficient_cycle);
    assert_eq!(m.lanes[0].columns.len(), 512);
    let t = Target::new(rate, &[(1, clicks(8.0, 0.001))]).unwrap();
    assert_eq!(t.timing_model(256).frames, 24000);
    let c = Component {
        modulation: Some(Modulation::Sinusoidal {
            rate_hz: 39.0625,
            depth: 1.0,
        }),
        ..clicks(40.0, 0.001)
    };
    let t = Target::new(rate, &[(1, c)]).unwrap();
    let m = t.timing_model(256);
    assert_eq!(m.lanes.len(), 2);
    assert_eq!(m.lanes[1].kind, ModelKind::Envelope);
    assert!(
        m.lanes[1]
            .columns
            .iter()
            .all(|e| 0.0 <= e.min && e.min <= e.max && e.max <= 1.0)
    );
    let mut a = Engine::new(rate);
    a.set(&t);
    let mut b = a.clone();
    for _ in 0..5 {
        let _ = t.timing_model(256);
    }
    let mut x = [[0.0; 2]; 4000];
    let mut y = x;
    a.render(&mut x);
    b.render(&mut y);
    assert_eq!(x, y);
    assert_eq!(a.observation(), b.observation());
    let all: Vec<_> = (0..16).map(|id| (id, c)).collect();
    assert_eq!(
        Target::new(rate, &all)
            .unwrap()
            .timing_model(512)
            .lanes
            .len(),
        32
    );
}

#[test]
fn am_bounds_contain_independent_sine_samples_and_depth_zero_is_steady() {
    for fs in [44100, 48000, 768000] {
        for depth in [0.0, 0.5, 1.0] {
            let rate = SampleRate::new(fs).unwrap();
            let c = Component {
                carrier: Carrier::Tone {
                    frequency_hz: 1000.0,
                },
                modulation: Some(Modulation::Sinusoidal {
                    rate_hz: 39.0625,
                    depth,
                }),
                ..clicks(40.0, 0.001)
            };
            let m = Target::new(rate, &[(1, c)]).unwrap().timing_model(257);
            if depth == 0.0 {
                assert_eq!(m.lanes[0].kind, ModelKind::Steady);
                continue;
            }
            let lane = &m.lanes[0];
            for n in 0..m.frames {
                let index = usize::try_from(n * 257 / m.frames).unwrap();
                let phase = std::f64::consts::TAU * 39.0625 * f64::from(u32::try_from(n).unwrap())
                    / f64::from(fs);
                let expected = (1.0 + depth * phase.sin()) / (1.0 + depth);
                assert!(
                    lane.columns[index].min <= expected + 1e-12
                        && expected <= lane.columns[index].max + 1e-12,
                    "{fs} {depth} {n}"
                );
            }
        }
    }
}
#[test]
fn maximum_rate_projection_has_fixed_storage_and_measured_cost() {
    let rate = SampleRate::new(768000).unwrap();
    let c = Component {
        modulation: Some(Modulation::Sinusoidal {
            rate_hz: 383999.0,
            depth: 1.0,
        }),
        ..clicks(383999.0, 1.0 / 768000.0)
    };
    let voices: Vec<_> = (0..16).map(|id| (id, c)).collect();
    let t = Target::new(rate, &voices).unwrap();
    let now = std::time::Instant::now();
    let model = t.timing_model(usize::MAX);
    assert_eq!(model.lanes.len(), 32);
    assert_eq!(
        model.lanes.iter().map(|l| l.columns.len()).sum::<usize>(),
        16384
    );
    println!(
        "max-rate timing model: {:?}, 16384 extrema (262144 payload bytes), {} frames",
        now.elapsed(),
        model.frames
    );
}

#[test]
fn span_uses_active_modulation_and_flags_only_incomplete_cycles() {
    let rate = SampleRate::new(48000).unwrap();
    let tone = Component {
        carrier: Carrier::Tone {
            frequency_hz: 1000.0,
        },
        ..clicks(40.0, 0.001)
    };
    for modulation in [
        Modulation::Rectangular {
            rate_hz: 2.0,
            width_seconds: 0.001,
        },
        Modulation::Sinusoidal {
            rate_hz: 2.0,
            depth: 0.5,
        },
    ] {
        let model = Target::new(
            rate,
            &[(
                1,
                Component {
                    modulation: Some(modulation),
                    ..tone
                },
            )],
        )
        .unwrap()
        .timing_model(17);
        assert_eq!(model.frames, 96000);
        assert!(!model.insufficient_cycle);
    }
    for (hz, incomplete) in [(0.125, true), (0.25, false), (8.0, false)] {
        let model = Target::new(rate, &[(1, clicks(hz, 0.001))])
            .unwrap()
            .timing_model(17);
        assert_eq!(model.insufficient_cycle, incomplete, "{hz}");
        assert_eq!(model.lanes.len(), 1);
        assert_eq!(model.lanes[0].kind, ModelKind::Clicks);
    }
    let model = Target::new(
        rate,
        &[(
            1,
            Component {
                modulation: Some(Modulation::Sinusoidal {
                    rate_hz: 0.1,
                    depth: 0.0,
                }),
                ..tone
            },
        )],
    )
    .unwrap()
    .timing_model(17);
    assert_eq!(model.frames, 12000);
    assert!(!model.insufficient_cycle);
    assert_eq!(model.lanes.len(), 1);
    assert_eq!(model.lanes[0].kind, ModelKind::Steady);
}

#[test]
fn envelope_bounds_match_endpoints_and_quarter_cycle_extrema() {
    let rate = SampleRate::new(48000).unwrap();
    // Eight cycles/second gives quarter-cycle extrema exactly every 1500 frames.
    // Independent rational phases also constrain partial columns, not just inclusion.
    for depth in [0.5, 1.0] {
        let component = Component {
            modulation: Some(Modulation::Sinusoidal {
                rate_hz: 8.0,
                depth,
            }),
            ..clicks(40.0, 0.001)
        };
        let model = Target::new(rate, &[(1, component)])
            .unwrap()
            .timing_model(37);
        assert_eq!(model.frames, 24000);
        assert_eq!(model.lanes.len(), 2);
        let lane = &model.lanes[1];
        assert_eq!(lane.kind, ModelKind::Envelope);
        assert_eq!(lane.columns.len(), 37);
        for (i, extent) in lane.columns.iter().enumerate() {
            let i = u64::try_from(i).unwrap();
            let start = (i * 24000).div_ceil(37);
            let last = ((i + 1) * 24000).div_ceil(37) - 1;
            let value = |n: u64| {
                let phase =
                    std::f64::consts::TAU * f64::from(u32::try_from(n % 6000).unwrap()) / 6000.0;
                (1.0 + depth * phase.sin()) / (1.0 + depth)
            };
            let mut low = value(start).min(value(last));
            let mut high = value(start).max(value(last));
            for quarter in 0..16_u64 {
                let frame = quarter * 1500;
                if (start..=last).contains(&frame) {
                    low = low.min(value(frame));
                    high = high.max(value(frame));
                }
            }
            assert!(
                (extent.min - low).abs() < 1e-12,
                "min {depth} {i}: {extent:?}/{low}"
            );
            assert!(
                (extent.max - high).abs() < 1e-12,
                "max {depth} {i}: {extent:?}/{high}"
            );
        }
    }
}
