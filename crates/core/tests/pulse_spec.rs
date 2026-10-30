//! Independent discrete pulse references and width-intent boundaries.
use auditorylab_core::{Finite, PulseError, PulseShape, PulseSpec, PulseWidth, SampleRate, Signal};
fn f(value: f64) -> Finite {
    Finite::new(value).unwrap_or_else(|| panic!("finite pulse fixture"))
}
fn duration(hz: f64, seconds: f64) -> PulseSpec {
    PulseSpec {
        rate_hz: f(hz),
        width: PulseWidth::Duration {
            seconds: f(seconds),
        },
        shape: PulseShape::Rectangular {},
    }
}
fn duty(hz: f64, fraction: f64) -> PulseSpec {
    PulseSpec {
        width: PulseWidth::DutyCycle {
            fraction: f(fraction),
        },
        ..duration(hz, 0.001)
    }
}
#[test]
fn rate_edits_preserve_only_the_authored_width_authority() {
    let fs = SampleRate::DEFAULT;
    let fixed = duration(10.0, 0.001);
    let proportional = duty(10.0, 0.5);
    assert_eq!(fixed.resolve(fs).unwrap().width_frames(), 48);
    assert_eq!(
        PulseSpec {
            rate_hz: f(40.0),
            ..fixed
        }
        .resolve(fs)
        .unwrap()
        .width_frames(),
        48
    );
    assert_eq!(proportional.resolve(fs).unwrap().width_frames(), 2400);
    assert_eq!(
        PulseSpec {
            rate_hz: f(40.0),
            ..proportional
        }
        .resolve(fs)
        .unwrap()
        .width_frames(),
        600
    );
}
#[test]
fn shaped_endpoints_plateau_and_asymmetric_edges_follow_independent_cosines() {
    let fs = SampleRate::new(8000).unwrap();
    for (window, rise, fall) in [(3, 1, 1), (8, 2, 3), (17, 7, 9)] {
        let spec = PulseSpec {
            shape: PulseShape::RaisedCosine {
                rise_seconds: f(f64::from(rise) / 8000.0),
                fall_seconds: f(f64::from(fall) / 8000.0),
            },
            ..duration(100.0, f64::from(window) / 8000.0)
        };
        let pulse = spec.resolve(fs).unwrap();
        assert_eq!(
            pulse.edge_frames(),
            [u64::try_from(rise).unwrap(), u64::try_from(fall).unwrap()]
        );
        for frame in 0..240_u32 {
            let local = frame % 80;
            let expected = if local >= u32::try_from(window).unwrap() {
                0.0
            } else {
                let rising = (f64::from(local) / f64::from(rise)).min(1.0);
                let falling = (f64::from(window - 1) - f64::from(local)) / f64::from(fall);
                let position = rising.min(falling.min(1.0));
                (1.0 - (std::f64::consts::PI * position).cos()) / 2.0
            };
            let sample = pulse.sample(u64::from(frame));
            assert!(
                (sample - expected).abs() < 2e-15,
                "{window} {rise} {fall} {frame}"
            );
            assert!((0.0..=1.0).contains(&sample));
        }
        assert_eq!(pulse.sample(0), 0.0);
        assert_eq!(pulse.sample(u64::try_from(window - 1).unwrap()), 0.0);
    }
}
#[test]
fn invalid_edges_do_not_become_rectangular_or_resize_the_window() {
    for (window, rise, fall) in [
        (2.0, 1.0, 1.0),
        (3.0, 0.1, 1.0),
        (8.0, 4.0, 4.0),
        (8.0, -1.0, 1.0),
        (8.0, 1.0, 0.0),
    ] {
        let spec = PulseSpec {
            shape: PulseShape::RaisedCosine {
                rise_seconds: f(rise / 48000.0),
                fall_seconds: f(fall / 48000.0),
            },
            ..duration(10.0, window / 48000.0)
        };
        assert_eq!(spec.resolve(SampleRate::DEFAULT), Err(PulseError::Edges));
    }
    let p = duration(10.0, 1.0 / 48000.0)
        .resolve(SampleRate::DEFAULT)
        .unwrap();
    assert_eq!(p.sample(0), 1.0);
    assert_eq!(p.sample(1), 0.0);
}
#[test]
fn width_rounding_and_closed_frame_rejections_have_neighbour_controls() {
    let fs = SampleRate::DEFAULT;
    assert_eq!(
        duration(40.0, 0.5 / 48000.0)
            .resolve(fs)
            .unwrap()
            .width_frames(),
        1
    );
    assert_eq!(
        duration(40.0, (0.5_f64 / 48000.0).next_down()).resolve(fs),
        Err(PulseError::Width)
    );
    assert!(duration(40.0, 1199.0 / 48000.0).resolve(fs).is_ok());
    assert_eq!(
        duration(40.0_f64.next_up(), 1199.0 / 48000.0).resolve(fs),
        Err(PulseError::ClosedFrame)
    );
    assert_eq!(
        duration(40.0, 1200.0 / 48000.0).resolve(fs),
        Err(PulseError::ClosedFrame)
    );
    for fraction in [-1.0, 0.0, 1.0, 1.0_f64.next_up()] {
        assert_eq!(duty(40.0, fraction).resolve(fs), Err(PulseError::Duty));
    }
    assert_eq!(
        duty(40.0, 1.0_f64.next_down()).resolve(fs),
        Err(PulseError::ClosedFrame)
    );
}
#[test]
fn rational_onsets_and_shaped_samples_hold_at_the_horizon() {
    for fs in [8000, 44100, 48000, 96000, 768_000] {
        for (p, q) in [
            (1_u64, 10_u64),
            (8, 1),
            (10, 1),
            (625, 16),
            (40, 1),
            (80, 1),
        ] {
            // 0.1 is tested separately below with its exact binary rational.
            if q == 10 {
                continue;
            }
            let hz = f64::from(u32::try_from(p).unwrap()) / f64::from(u32::try_from(q).unwrap());
            let spec = PulseSpec {
                shape: PulseShape::RaisedCosine {
                    rise_seconds: f(1.0 / f64::from(fs)),
                    fall_seconds: f(1.0 / f64::from(fs)),
                },
                ..duration(hz, 3.0 / f64::from(fs))
            };
            let pulse = spec.resolve(SampleRate::new(fs).unwrap()).unwrap();
            for n in (0..10000).chain(Signal::MAX_FRAMES - 10000..Signal::MAX_FRAMES) {
                let k = u128::from(n) * u128::from(p) / (u128::from(q) * u128::from(fs));
                let onset = (k * u128::from(q) * u128::from(fs)).div_ceil(u128::from(p));
                assert_eq!(
                    pulse.sample(n),
                    if u128::from(n) - onset == 1 { 1.0 } else { 0.0 }
                );
            }
        }
    }
    let pulse = duration(0.1, 0.001).resolve(SampleRate::DEFAULT).unwrap();
    // 0.1's exact binary rational is slightly greater than 1/10.
    assert_eq!(pulse.sample(479_999), 0.0);
    assert_eq!(pulse.sample(480_000), 1.0);
    assert_eq!(pulse.sample(Signal::MAX_FRAMES), 0.0);
}
#[test]
fn canonical_pulsed_tone_parses_and_renders_the_independent_envelope() {
    let text = include_str!("fixtures/recipes/pulsed-tone.toml");
    let recipe = auditorylab_core::Recipe::parse(text).unwrap();
    let fs = SampleRate::DEFAULT;
    let signal = recipe.signal(fs).unwrap();
    let amplitude = 10.0_f64.powf(-30.0 / 20.0);
    // Beyond the finite startup fade: second pulse starts at frame 4800.
    for n in [4800_u32, 4801, 4848, 4896, 6000, 7103, 7104, 7199, 7200] {
        let i = n - 4800;
        let remaining = 2399_u32.saturating_sub(i);
        let gain = if i >= 2400 {
            0.0
        } else if i < 96 {
            (1.0 - (std::f64::consts::PI * f64::from(i) / 96.0).cos()) / 2.0
        } else if remaining < 96 {
            (1.0 - (std::f64::consts::PI * f64::from(remaining) / 96.0).cos()) / 2.0
        } else {
            1.0
        };
        let expected =
            amplitude * (std::f64::consts::TAU * 440.0 * f64::from(n) / 48000.0).sin() * gain;
        let actual = signal.frame(u64::from(n));
        assert!(
            (actual[0] - expected).abs() < 2e-13,
            "{n}: {actual:?} vs {expected}"
        );
        assert_eq!(actual[0], actual[1]);
    }
    let canonical = recipe.to_toml().unwrap();
    assert_eq!(auditorylab_core::Recipe::parse(&canonical).unwrap(), recipe);
    assert!(
        auditorylab_core::Recipe::parse(&text.replace("schema_version = 5", "schema_version = 4"))
            .is_err()
    );
    assert!(
        auditorylab_core::Recipe::parse(
            &text.replace("fraction = 0.5", "fraction = 0.5\nseconds = 0.05")
        )
        .is_err()
    );
    assert!(
        auditorylab_core::Recipe::parse(&text.replace("kind = \"pulsed\"", "kind = \"none\""))
            .is_err()
    );
}

#[test]
fn shaped_preview_extrema_match_every_sample_including_narrow_windows() {
    let fs = SampleRate::new(8000).unwrap();
    for (w, a, r) in [(3, 1, 1), (8, 2, 3), (17, 7, 9)] {
        let pulse = PulseSpec {
            shape: PulseShape::RaisedCosine {
                rise_seconds: f(f64::from(a) / 8000.0),
                fall_seconds: f(f64::from(r) / 8000.0),
            },
            ..duration(100.0, f64::from(w) / 8000.0)
        }
        .resolve(fs)
        .unwrap();
        for start in 0..160 {
            for length in [1, 2, 3, 7, 17, 39, 80, 93, 161] {
                let samples: Vec<_> = (start..start + length).map(|n| pulse.sample(n)).collect();
                let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
                let max = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let extent = pulse.extent(start, start + length).unwrap();
                assert_eq!(
                    (extent.min, extent.max),
                    (min, max),
                    "{w} {a} {r} {start} {length}"
                );
            }
        }
        assert!(pulse.extent(1, 1).is_none());
        assert!(pulse.extent(0, Signal::MAX_FRAMES + 1).is_none());
    }
}

#[test]
fn lowest_supported_rate_and_subnormal_clocks_keep_the_same_window_contract() {
    let fs = SampleRate::new(1).unwrap();
    let shaped = PulseSpec {
        shape: PulseShape::RaisedCosine {
            rise_seconds: f(1.0),
            fall_seconds: f(1.0),
        },
        ..duration(0.25, 3.0)
    }
    .resolve(fs)
    .unwrap();
    for n in (0..32).chain(Signal::MAX_FRAMES - 32..Signal::MAX_FRAMES) {
        assert_eq!(shaped.sample(n), if n % 4 == 1 { 1.0 } else { 0.0 });
    }
    let slow = duration(f64::from_bits(1), 3.0).resolve(fs).unwrap();
    assert_eq!(slow.sample(0), 1.0);
    assert_eq!(slow.sample(2), 1.0);
    assert_eq!(slow.sample(3), 0.0);
    assert_eq!(slow.sample(Signal::MAX_FRAMES - 1), 0.0);
    assert_eq!(
        duty(f64::from_bits(1), 0.5).resolve(fs),
        Err(PulseError::Width)
    );
}
#[test]
fn all_source_modulation_width_and_shape_combinations_roundtrip_the_current_reader() {
    use auditorylab_core::{ChannelRouting, ComponentForm, Modulation, SoundSource};
    let widths = [
        PulseWidth::Duration { seconds: f(0.005) },
        PulseWidth::DutyCycle { fraction: f(0.5) },
    ];
    let shapes = [
        PulseShape::Rectangular {},
        PulseShape::RaisedCosine {
            rise_seconds: f(0.001),
            fall_seconds: f(0.002),
        },
    ];
    let mut sources = vec![
        SoundSource::Tone {
            frequency_hz: f(440.0),
            peak_dbfs: f(-30.0),
            route: ChannelRouting::Both,
        },
        SoundSource::BinauralPair {
            first_hz: f(200.0),
            offset_hz: f(6.0),
            first_peak_dbfs: f(-30.0),
            second_peak_dbfs: f(-36.0),
        },
        SoundSource::MonauralPair {
            first_hz: f(200.0),
            offset_hz: f(-6.0),
            first_peak_dbfs: f(-30.0),
            second_peak_dbfs: f(-36.0),
            route: ChannelRouting::Left,
        },
    ];
    let mut modulators = vec![
        None,
        Some(Modulation::Sinusoidal {
            rate_hz: f(10.0),
            depth: f(0.5),
        }),
    ];
    for width in widths {
        for shape in shapes {
            let pulse = PulseSpec {
                rate_hz: f(40.0),
                width,
                shape,
            };
            sources.push(SoundSource::PulseTrain {
                pulse,
                peak_dbfs: f(-30.0),
                route: ChannelRouting::Right,
            });
            modulators.push(Some(Modulation::Pulsed { pulse }));
        }
    }
    let mut count = 0;
    for source in sources {
        for &modulation in &modulators {
            let recipe = auditorylab_core::test_support::recipe(
                2.0,
                0.0,
                &[ComponentForm { source, modulation }],
            );
            let text = recipe.to_toml().unwrap();
            assert_eq!(auditorylab_core::Recipe::parse(&text).unwrap(), recipe);
            assert!(recipe.signal(SampleRate::DEFAULT).is_ok());
            count += 1;
        }
    }
    assert_eq!(
        count, 42,
        "seven explicit source configurations times six modulator configurations"
    );
}
