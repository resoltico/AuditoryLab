//! Pair-shared explanatory scope across monitor and teaching captions.
use auditorylab_core::{
    ChannelRouting, ComponentForm, Modulation, PairKind, PulseShape, PulseSpec, PulseWidth,
    SampleRate, SoundSource,
    test_support::{finite, recipe},
    timing::ModelKind,
};
use auditorylab_presentation::{learning, workspace::monitor};
fn form(kind: PairKind, modulation: Modulation) -> ComponentForm {
    let first_hz = finite(200.0);
    let offset_hz = finite(-7.25);
    let first_peak_dbfs = finite(-10_000.0);
    let second_peak_dbfs = finite(-30.0);
    ComponentForm {
        source: match kind {
            PairKind::Binaural => SoundSource::BinauralPair {
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
            },
            PairKind::Monaural => SoundSource::MonauralPair {
                route: ChannelRouting::Right,
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
            },
        },
        modulation: Some(modulation),
    }
}
#[test]
fn monitor_and_teaching_name_one_gain_as_applying_to_both_pair_tones() {
    for kind in [PairKind::Binaural, PairKind::Monaural] {
        for (modulation, expected_kind, quantity, teaching) in [
            (
                Modulation::Pulsed {
                    pulse: PulseSpec {
                        rate_hz: finite(40.0),
                        width: PulseWidth::Duration {
                            seconds: finite(0.001),
                        },
                        shape: PulseShape::Rectangular {},
                    },
                },
                ModelKind::Pulsed,
                "40 Hz pulsed modulation",
                "How shared pulses switch both tones on and off",
            ),
            (
                Modulation::Sinusoidal {
                    rate_hz: finite(8.0),
                    depth: finite(0.5),
                },
                ModelKind::Envelope,
                "8 Hz AM · 50%",
                "How shared gain raises and lowers both tones",
            ),
        ] {
            let form = form(kind, modulation);
            let target = recipe(0.1, 0.0, &[form])
                .sound()
                .target(SampleRate::new(48_000).unwrap())
                .unwrap();
            let timing = target.timing_model(32);
            assert_eq!(timing.lanes.len(), 1);
            let lane = &timing.lanes[0];
            assert_eq!(lane.kind, expected_kind);
            assert_eq!(
                monitor::model_view_lane_label(lane),
                format!("Component 1 · {quantity} · applies to both tones")
            );
            let visual = auditorylab_app::learning::TeachingVisual {
                form,
                carrier: vec![],
                waveform: vec![],
                carrier_seconds: 0.01,
                waveform_seconds: 0.1,
                facts: target.voices().map(|(_, facts)| *facts).collect(),
                timing,
            };
            let caption = learning::timing_caption(&visual, &visual.timing.lanes[0]);
            assert!(caption.starts_with(teaching), "{caption}");
            assert!(caption.contains("both tones’ full level"));
            assert!(!caption.contains("steady"));
        }
    }
}
#[test]
fn source_pulse_and_gain_labels_do_not_conflate_physical_processes() {
    let form = ComponentForm {
        source: SoundSource::PulseTrain {
            route: ChannelRouting::Both,
            peak_dbfs: finite(-30.0),
            pulse: PulseSpec {
                rate_hz: finite(40.0),
                width: PulseWidth::Duration {
                    seconds: finite(0.001),
                },
                shape: PulseShape::Rectangular {},
            },
        },
        modulation: Some(Modulation::Sinusoidal {
            rate_hz: finite(8.0),
            depth: finite(0.5),
        }),
    };
    let model = recipe(0.1, 0.0, &[form])
        .sound()
        .target(SampleRate::new(48_000).unwrap())
        .unwrap()
        .timing_model(32);
    assert_eq!(
        monitor::model_view_lane_label(&model.lanes[0]),
        "Component 1 · 40 Hz pulse train"
    );
    assert_eq!(
        monitor::model_view_lane_label(&model.lanes[1]),
        "Component 1 · 8 Hz AM · 50%"
    );
}

#[test]
fn absent_or_zero_depth_gain_retains_two_truthful_carrier_labels() {
    for kind in [PairKind::Binaural, PairKind::Monaural] {
        for modulation in [
            None,
            Some(Modulation::Sinusoidal {
                rate_hz: finite(8.0),
                depth: finite(0.0),
            }),
        ] {
            let mut source = form(
                kind,
                Modulation::Sinusoidal {
                    rate_hz: finite(8.0),
                    depth: finite(0.0),
                },
            );
            source.modulation = modulation;
            let model = recipe(0.1, 0.0, &[source])
                .sound()
                .target(SampleRate::new(48_000).unwrap())
                .unwrap()
                .timing_model(32);
            let labels: Vec<_> = model
                .lanes
                .iter()
                .map(monitor::model_view_lane_label)
                .collect();
            assert_eq!(
                labels,
                [
                    "Component 1 · 200 Hz steady tone",
                    "Component 1 · second tone · 192.75 Hz steady tone"
                ]
            );
        }
    }
}
