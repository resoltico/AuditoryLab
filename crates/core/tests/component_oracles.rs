//! Independent spectra and compositional order/transition controls for the component renderer.
use auditorylab_core::{
    ChannelRouting, ComponentForm, Modulation, PulseShape, PulseSpec, PulseWidth, SampleRate,
    Signal, SoundSource,
    live::Engine,
    rhythm,
    test_support::{finite as f, recipe},
};
fn pulse(hz: f64) -> PulseSpec {
    PulseSpec {
        rate_hz: f(hz),
        width: PulseWidth::DutyCycle { fraction: f(0.5) },
        shape: PulseShape::RaisedCosine {
            rise_seconds: f(0.002),
            fall_seconds: f(0.002),
        },
    }
}
fn tone(hz: f64) -> ComponentForm {
    ComponentForm {
        source: SoundSource::Tone {
            frequency_hz: f(hz),
            peak_dbfs: f(-30.0),
            route: ChannelRouting::Both,
        },
        modulation: None,
    }
}
fn pair(separate: bool, modulation: Option<Modulation>) -> ComponentForm {
    let source = if separate {
        SoundSource::BinauralPair {
            first_hz: f(400.0),
            offset_hz: f(6.0),
            first_peak_dbfs: f(-20.0),
            second_peak_dbfs: f(-26.0),
        }
    } else {
        SoundSource::MonauralPair {
            first_hz: f(400.0),
            offset_hz: f(6.0),
            first_peak_dbfs: f(-20.0),
            second_peak_dbfs: f(-26.0),
            route: ChannelRouting::Both,
        }
    };
    ComponentForm { source, modulation }
}
fn signal(form: ComponentForm) -> Signal {
    recipe(2.0, 0.0, &[form])
        .signal(SampleRate::DEFAULT)
        .unwrap_or_else(|error| panic!("invalid component fixture: {error}"))
}
#[test]
fn component_sum_precedes_shared_modulation_and_detects_the_retired_association() {
    let pulse = pulse(10.0);
    let gain = signal(ComponentForm {
        source: SoundSource::PulseTrain {
            pulse,
            peak_dbfs: f(0.0),
            route: ChannelRouting::Both,
        },
        modulation: None,
    });
    let mut changed_bits = false;
    for separate in [false, true] {
        let base = signal(pair(separate, None));
        let modulated = signal(pair(separate, Some(Modulation::Pulsed { pulse })));
        let first = signal(ComponentForm {
            source: SoundSource::Tone {
                frequency_hz: f(400.0),
                peak_dbfs: f(-20.0),
                route: ChannelRouting::Both,
            },
            modulation: None,
        });
        let second = signal(ComponentForm {
            source: SoundSource::Tone {
                frequency_hz: f(406.0),
                peak_dbfs: f(-26.0),
                route: ChannelRouting::Both,
            },
            modulation: None,
        });
        for n in 0..20000 {
            let g = gain.frame(n)[0];
            let actual = modulated.frame(n);
            assert_eq!(actual, base.frame(n).map(|v| v * g));
            if !separate {
                let retired = first.frame(n)[0] * g + second.frame(n)[0] * g;
                changed_bits |= retired.to_bits() != actual[0].to_bits();
            }
        }
    }
    assert!(
        changed_bits,
        "the control must distinguish sum-then-modulate from per-lane modulation"
    );
}
#[test]
fn source_to_pair_transitions_restart_both_lanes_at_one_component_origin() {
    let rate = SampleRate::DEFAULT;
    for separate in [false, true] {
        let old = signal(tone(400.0));
        let next = signal(pair(
            separate,
            Some(Modulation::Pulsed { pulse: pulse(10.0) }),
        ));
        let mut engine = Engine::new(rate);
        engine.set(old.target());
        engine.render(&mut vec![[0.0; 2]; 2000]);
        assert!(engine.set(next.target()));
        let mut actual = vec![[0.0; 2]; 6000];
        engine.render(&mut actual);
        let ramp = usize::try_from(engine.ramp_frames()).unwrap();
        for (n, frame) in actual.iter().enumerate().skip(ramp) {
            assert_eq!(*frame, next.frame(u64::try_from(n).unwrap()));
        }
        let observation = engine.observation().unwrap();
        assert_eq!(observation.voices.iter().flatten().count(), 2);
        assert!(
            observation
                .voices
                .iter()
                .flatten()
                .all(|lane| lane.role == auditorylab_core::SlotRole::Continuing)
        );
    }
}
#[test]
fn width_mode_facts_only_changes_do_not_restart_shared_modulation_or_pair_phase() {
    let spec = pulse(10.0);
    let a = signal(pair(true, Some(Modulation::Pulsed { pulse: spec })));
    let b = signal(pair(
        true,
        Some(Modulation::Pulsed {
            pulse: PulseSpec {
                width: PulseWidth::Duration { seconds: f(0.05) },
                ..spec
            },
        }),
    ));
    assert!(a.target().same_audio(b.target()));
    assert_ne!(a.target(), b.target());
    let mut reference = Engine::new(SampleRate::DEFAULT);
    reference.set_revision(a.target(), 1);
    reference.render(&mut vec![[0.0; 2]; 7777]);
    let mut edited = reference.clone();
    assert!(edited.set_revision(b.target(), 2));
    let mut x = vec![[0.0; 2]; 10000];
    let mut y = x.clone();
    reference.render(&mut x);
    edited.render(&mut y);
    assert_eq!(x, y);
    assert!(!edited.observation().unwrap().transitioning);
}
#[test]
fn composed_processes_are_attributed_once_and_filters_cannot_borrow_rates() {
    let form = pair(true, Some(Modulation::Pulsed { pulse: pulse(10.0) }));
    let recipe = recipe(2.0, 0.0, &[form]);
    let nominal = rhythm::nominal(recipe.sound());
    assert_eq!(
        nominal
            .rhythms
            .iter()
            .map(|r| (r.method, r.hz))
            .collect::<Vec<_>>(),
        [
            (rhythm::Method::Binaural, 6.0),
            (rhythm::Method::Pulsed, 10.0)
        ]
    );
    assert!(!nominal.matches(&[rhythm::Band::Alpha], &[rhythm::Method::Binaural]));
    let signal = recipe.signal(SampleRate::DEFAULT).unwrap();
    let actual = rhythm::realized(signal.target());
    assert_eq!(actual.rhythms.len(), 2);
    assert_eq!(actual.voices.len(), 2);
    let model = signal.target().timing_model(127);
    assert_eq!(
        model
            .lanes
            .iter()
            .filter(|lane| lane.kind == auditorylab_core::timing::ModelKind::Pulsed)
            .count(),
        1
    );
    let nested = ComponentForm {
        source: SoundSource::PulseTrain {
            pulse: pulse(40.0),
            peak_dbfs: f(-30.0),
            route: ChannelRouting::Both,
        },
        modulation: Some(Modulation::Pulsed { pulse: pulse(10.0) }),
    };
    assert_eq!(
        rhythm::nominal(auditorylab_core::test_support::recipe(2.0, 0.0, &[nested]).sound())
            .rhythms
            .len(),
        2
    );
}
fn dft(signal: &[f64], bin: u32) -> [f64; 2] {
    let n =
        f64::from(u32::try_from(signal.len()).unwrap_or_else(|_| panic!("bounded DFT fixture")));
    signal
        .iter()
        .enumerate()
        .fold([0.0; 2], |mut out, (i, &sample)| {
            let angle = std::f64::consts::TAU
                * f64::from(bin)
                * f64::from(u32::try_from(i).unwrap_or_else(|_| panic!("bounded DFT index")))
                / n;
            out[0] += sample * angle.cos() / n;
            out[1] -= sample * angle.sin() / n;
            out
        })
}
fn dirichlet(x: f64, width: u32) -> [f64; 2] {
    if x.abs() < 1e-15 {
        return [f64::from(width), 0.0];
    }
    let amplitude =
        (std::f64::consts::PI * x * f64::from(width)).sin() / (std::f64::consts::PI * x).sin();
    let phase = -std::f64::consts::PI * x * f64::from(width - 1);
    [amplitude * phase.cos(), amplitude * phase.sin()]
}
#[test]
fn rectangular_and_raised_cosine_spectra_match_closed_discrete_fourier_sums() {
    let rate = SampleRate::new(8000).unwrap();
    let rectangle = PulseSpec {
        rate_hz: f(100.0),
        width: PulseWidth::Duration {
            seconds: f(20.0 / 8000.0),
        },
        shape: PulseShape::Rectangular {},
    }
    .resolve(rate)
    .unwrap();
    let shaped = PulseSpec {
        width: PulseWidth::Duration {
            seconds: f(21.0 / 8000.0),
        },
        shape: PulseShape::RaisedCosine {
            rise_seconds: f(10.0 / 8000.0),
            fall_seconds: f(10.0 / 8000.0),
        },
        ..rectangle.requested()
    }
    .resolve(rate)
    .unwrap();
    let rectangular: Vec<_> = (0..80).map(|n| rectangle.sample(n)).collect();
    let smooth: Vec<_> = (0..80).map(|n| shaped.sample(n)).collect();
    let mut tails = [0.0; 2];
    for bin in 0..=40 {
        let x = f64::from(bin) / 80.0;
        let raw = dirichlet(x, 20).map(|v| v / 80.0);
        let center = dirichlet(x, 21);
        let lower = dirichlet(x - 1.0 / 20.0, 21);
        let upper = dirichlet(x + 1.0 / 20.0, 21);
        let expected = [
            (0.5 * center[0] - 0.25 * lower[0] - 0.25 * upper[0]) / 80.0,
            (0.5 * center[1] - 0.25 * lower[1] - 0.25 * upper[1]) / 80.0,
        ];
        let actual_raw = dft(&rectangular, bin);
        let actual_smooth = dft(&smooth, bin);
        for channel in 0..2 {
            assert!((actual_raw[channel] - raw[channel]).abs() < 2e-14);
            assert!((actual_smooth[channel] - expected[channel]).abs() < 2e-14);
        }
        if bin >= 15 {
            tails[0] += actual_raw.iter().map(|v| v * v).sum::<f64>();
            tails[1] += actual_smooth.iter().map(|v| v * v).sum::<f64>();
        }
    }
    assert!(
        (dft(&rectangular, 0)[0] - 0.25).abs() < 1e-15,
        "DC is retained without automatic filtering"
    );
    assert!(
        (dft(&smooth, 0)[0] - 0.125).abs() < 1e-15,
        "no automatic level normalization"
    );
    assert!(
        tails[1] < tails[0],
        "this specified smooth window reduces aggregate high-frequency energy; individual bins \
            need not all decrease"
    );
}
#[test]
fn pair_am_sideband_guard_checks_both_tonal_lanes() {
    for separate in [false, true] {
        let mut form = pair(
            separate,
            Some(Modulation::Sinusoidal {
                rate_hz: f(100.0),
                depth: f(0.5),
            }),
        );
        match &mut form.source {
            SoundSource::BinauralPair {
                first_hz,
                offset_hz,
                ..
            }
            | SoundSource::MonauralPair {
                first_hz,
                offset_hz,
                ..
            } => {
                *first_hz = f(3800.0);
                *offset_hz = f(100.0);
            }
            _ => panic!(),
        }
        let sound = recipe(2.0, 0.0, &[form]);
        assert!(matches!(
            sound.sound().target(SampleRate::new(8000).unwrap()),
            Err(auditorylab_core::SoundError::PairCarrier {
                error: auditorylab_core::VoiceError::Sideband { .. },
                ..
            })
        ));
        form.modulation = Some(Modulation::Pulsed {
            pulse: pulse(100.0),
        });
        assert!(
            recipe(2.0, 0.0, &[form])
                .sound()
                .target(SampleRate::new(8000).unwrap())
                .is_ok(),
            "pulses do not acquire a false single-sideband alias-free promise"
        );
    }
}

#[test]
fn shared_processes_remain_inspectable_when_only_the_second_pair_lane_is_audible() {
    let signal = signal(pair(true, Some(Modulation::Pulsed { pulse: pulse(10.0) })));
    let mut observation = signal.observation(100).unwrap();
    observation.voices[0].as_mut().unwrap().gains = [0.0; 2];
    let structure = rhythm::realized(&observation);
    assert_eq!(structure.voices.len(), 2);
    assert_eq!(structure.rhythms.len(), 2);
    observation.voices[1].as_mut().unwrap().gains = [0.0; 2];
    let structure = rhythm::realized(&observation);
    assert_eq!(
        structure.voices,
        [] as [auditorylab_core::VoiceObservation; 0]
    );
    assert_eq!(
        structure.rhythms,
        [] as [auditorylab_core::rhythm::Descriptor; 0]
    );
}
#[test]
fn sinusoidal_am_has_the_independent_carrier_and_sideband_coefficients() {
    let form = ComponentForm {
        source: SoundSource::Tone {
            frequency_hz: f(1000.0),
            peak_dbfs: f(-20.0),
            route: ChannelRouting::Both,
        },
        modulation: Some(Modulation::Sinusoidal {
            rate_hz: f(100.0),
            depth: f(0.5),
        }),
    };
    let signal = recipe(1.0, 0.0, &[form])
        .signal(SampleRate::new(8000).unwrap())
        .unwrap();
    let samples: Vec<_> = (0..8000).map(|n| signal.frame(n)[0]).collect();
    for (bin, expected) in [
        (900, [0.1 * 0.5 / 1.5 / 4.0, 0.0]),
        (1000, [0.0, -0.1 / 1.5 / 2.0]),
        (1100, [-0.1 * 0.5 / 1.5 / 4.0, 0.0]),
        (700, [0.0; 2]),
        (1300, [0.0; 2]),
    ] {
        let actual = dft(&samples, bin);
        for i in 0..2 {
            assert!(
                (actual[i] - expected[i]).abs() < 2e-13,
                "{bin} {actual:?} {expected:?}"
            );
        }
    }
}

#[test]
fn pair_relationship_kind_changes_restart_the_whole_component_in_both_directions() {
    for separate in [false, true] {
        let old = signal(pair(
            separate,
            Some(Modulation::Pulsed { pulse: pulse(10.0) }),
        ));
        let next = signal(pair(
            !separate,
            Some(Modulation::Pulsed { pulse: pulse(10.0) }),
        ));
        let mut engine = Engine::new(SampleRate::DEFAULT);
        engine.set(old.target());
        engine.render(&mut vec![[0.0; 2]; 2003]);
        engine.set(next.target());
        let mut actual = vec![[0.0; 2]; 7000];
        engine.render(&mut actual);
        for (n, frame) in actual
            .iter()
            .enumerate()
            .skip(usize::try_from(engine.ramp_frames()).unwrap())
        {
            assert_eq!(
                *frame,
                next.frame(u64::try_from(n).unwrap()),
                "{separate} {n}"
            );
        }
    }
}

#[test]
fn a_pair_difference_survives_equal_rounded_carrier_displays() {
    let first = 200_000.0_f64;
    let second = first.next_up();
    let form = ComponentForm {
        source: SoundSource::BinauralPair {
            first_hz: f(first),
            offset_hz: f(second - first),
            first_peak_dbfs: f(-30.0),
            second_peak_dbfs: f(-30.0),
        },
        modulation: None,
    };
    let signal = recipe(1.0, 0.0, &[form])
        .signal(SampleRate::new(768_000).unwrap())
        .unwrap();
    let lanes: Vec<_> = signal.voices().map(|(_, facts)| *facts).collect();
    let rounded = lanes
        .iter()
        .map(|facts| match facts.carrier {
            auditorylab_core::CarrierFacts::Tone(rate) => rate.realized_hz,
            auditorylab_core::CarrierFacts::PulseTrain(_) => panic!(),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rounded[1] - rounded[0],
        0.0,
        "negative control: subtracting rounded displays loses this process"
    );
    // Independent rational ceil steps: ceil(first*2^64/768_000)=4803839602528529067,
    // ceil(next_up(first)*2^64/768_000)=4803839602528529766, difference=699.
    let expected = 699.0 * 768_000.0 / 18_446_744_073_709_551_616.0;
    assert_eq!(lanes[0].pair.unwrap().realized_offset_hz, Some(expected));
    assert_eq!(lanes[1].pair, lanes[0].pair);
    let actual = rhythm::realized(signal.target());
    assert_eq!(actual.rhythms.len(), 1);
    assert_eq!(actual.rhythms[0].hz, expected);
    assert_eq!(actual.pairs[0].difference_hz, expected);
}

#[test]
fn every_source_reports_invalid_shared_modulation_at_the_component_boundary() {
    use auditorylab_core::{ParameterId, PulseError, SoundError, VoiceError};
    let pulse_source = SoundSource::PulseTrain {
        route: ChannelRouting::Both,
        peak_dbfs: f(-30.0),
        pulse: auditorylab_core::test_support::rectangular_pulse(40.0, 0.001),
    };
    let sources = [
        (tone(440.0).source, 1),
        (pulse_source, 1),
        (pair(true, None).source, 2),
        (pair(false, None).source, 2),
    ];
    let cases = [
        (
            Modulation::Sinusoidal {
                rate_hz: f(0.0),
                depth: f(0.0),
            },
            VoiceError::Frequency {
                parameter: ParameterId::ModulationRate,
                hz: 0.0,
                sample_rate_hz: 48000,
            },
        ),
        (
            Modulation::Sinusoidal {
                rate_hz: f(10.0),
                depth: f(1.01),
            },
            VoiceError::Depth { depth: 1.01 },
        ),
        (
            Modulation::Pulsed {
                pulse: PulseSpec {
                    rate_hz: f(10.0),
                    width: PulseWidth::DutyCycle { fraction: f(1.0) },
                    shape: PulseShape::Rectangular {},
                },
            },
            VoiceError::Pulse {
                modulation: true,
                width: PulseWidth::DutyCycle { fraction: f(1.0) },
                rate_hz: 10.0,
                sample_rate_hz: 48000,
                error: PulseError::Duty,
            },
        ),
    ];
    for (source, lanes) in sources {
        for (modulation, expected) in cases.iter().cloned() {
            let recipe = recipe(
                1.0,
                0.0,
                &[ComponentForm {
                    source,
                    modulation: Some(modulation),
                }],
            );
            let check = recipe.sound().check(SampleRate::DEFAULT);
            assert_eq!(check.voices().len(), lanes);
            for (_, result) in check.voices() {
                assert_eq!(result.as_ref().unwrap_err(), &expected);
            }
            let Err(SoundError::Voice { id, error }) = recipe.signal(SampleRate::DEFAULT) else {
                panic!("invalid shared modulation must block the whole finite component");
            };
            assert_eq!(id.component.get(), 1);
            assert_eq!(error, expected);
        }
        assert!(
            recipe(
                1.0,
                0.0,
                &[ComponentForm {
                    source,
                    modulation: Some(Modulation::Sinusoidal {
                        rate_hz: f(10.0),
                        depth: f(0.5)
                    })
                }]
            )
            .signal(SampleRate::DEFAULT)
            .is_ok()
        );
    }
}
