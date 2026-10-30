//! Independent signed-pair, owner/lane, strict shape and sample-preservation oracles.
use crate as auditorylab_core;
use auditorylab_core::{
    ChannelRouting, Component, ComponentForm, ComponentId, FiniteFixture, Lane, Pair, PairError,
    PairIssue, SampleRate, StructureError, TargetCheck,
};
use serde::{Deserialize, Serialize};
use std::{format, vec, vec::Vec};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Document {
    component: Vec<Component>,
}
fn pair(offset_hz: f64) -> Pair {
    Pair {
        first_hz: crate::test_support::finite(200.0),
        offset_hz: crate::test_support::finite(offset_hz),
        first_peak_dbfs: crate::test_support::finite(-20.0),
        second_peak_dbfs: crate::test_support::finite(-30.0),
    }
}
fn form(id: u32, form: ComponentForm) -> Component {
    Component {
        id: ComponentId::new(id).unwrap(),
        form,
    }
}
fn tone(hz: f64, level: f64, route: ChannelRouting) -> ComponentForm {
    crate::ComponentForm {
        source: crate::SoundSource::Tone {
            route,
            peak_dbfs: crate::test_support::finite(level),
            frequency_hz: crate::test_support::finite(hz),
        },
        modulation: None,
    }
}

#[test]
fn pair_parameters_read_exact_values_and_write_only_the_requested_field() {
    use crate::ParameterId as P;
    let original = pair(-6.0);
    for monaural in [false, true] {
        let make = |pair: Pair| {
            if monaural {
                monaural_form(pair, ChannelRouting::Right)
            } else {
                binaural_form(pair)
            }
        };
        for (parameter, before, after) in [
            (
                P::PairFirstFrequency,
                200.0,
                Pair {
                    first_hz: crate::test_support::finite(317.0),
                    ..original
                },
            ),
            (
                P::PairOffset,
                -6.0,
                Pair {
                    offset_hz: crate::test_support::finite(317.0),
                    ..original
                },
            ),
            (
                P::PairFirstLevel,
                -20.0,
                Pair {
                    first_peak_dbfs: crate::test_support::finite(317.0),
                    ..original
                },
            ),
            (
                P::PairSecondLevel,
                -30.0,
                Pair {
                    second_peak_dbfs: crate::test_support::finite(317.0),
                    ..original
                },
            ),
        ] {
            let mut actual = make(original);
            assert_eq!(
                actual.get(parameter),
                Some(crate::test_support::finite(before))
            );
            assert!(actual.set(parameter, crate::test_support::finite(317.0)));
            assert_eq!(actual, make(after));
            assert_eq!(
                actual.get(parameter),
                Some(crate::test_support::finite(317.0))
            );
        }
        for parameter in [
            P::ToneFrequency,
            P::SourcePulseRate,
            P::SourcePulseDuration,
            P::PeakLevel,
            P::ModulationRate,
            P::ModulationDepth,
            P::ModulationPulseRate,
            P::ModulationPulseDuration,
            P::Duration,
            P::Fade,
        ] {
            let mut actual = make(original);
            assert_eq!(actual.get(parameter), None);
            assert!(!actual.set(parameter, crate::test_support::finite(999.0)));
            assert_eq!(actual, make(original));
        }
    }
    let mut primitive = tone(437.0, -24.0, ChannelRouting::Left);
    assert_eq!(
        primitive.get(P::ToneFrequency),
        Some(crate::test_support::finite(437.0))
    );
    assert_eq!(
        primitive.get(P::PeakLevel),
        Some(crate::test_support::finite(-24.0))
    );
    assert!(primitive.set(P::ToneFrequency, crate::test_support::finite(509.0)));
    assert_eq!(primitive, tone(509.0, -24.0, ChannelRouting::Left));
    assert!(!primitive.set(P::PairOffset, crate::test_support::finite(6.0)));
    assert_eq!(primitive.get(P::PairOffset), None);
    assert_eq!(primitive, tone(509.0, -24.0, ChannelRouting::Left));
}

#[test]
fn node_content_comparison_preserves_shape_routes_and_each_requested_value() {
    let cases = component_comparison_cases();
    for (left, right) in cases.different {
        assert!(!(left == right), "{left:?} versus {right:?}");
        assert!(!(right == left));
    }
    for value in cases.reflexive {
        assert!((value == value), "{value:?}");
    }
}

#[test]
fn content_equality_preserves_signed_zero_and_nan_payloads_are_rejected() {
    let nan = f64::from_bits(0x7ff8_0000_0000_0042);
    let other_nan = f64::from_bits(0x7ff8_0000_0000_0043);
    for value in [0.0, nan] {
        let changed = if value.is_nan() { other_nan } else { -0.0 };
        if !value.is_finite() {
            assert!(crate::Finite::new(value).is_none());
            assert!(crate::Finite::new(changed).is_none());
            continue;
        }
        for (left, right) in [
            (
                tone(value, -20.0, ChannelRouting::Both),
                tone(changed, -20.0, ChannelRouting::Both),
            ),
            (binaural_form(pair(value)), binaural_form(pair(changed))),
            (
                monaural_form(pair(value), ChannelRouting::Both),
                monaural_form(pair(changed), ChannelRouting::Both),
            ),
        ] {
            assert_eq!(left, left);
            assert!(!(left == right));
            assert!(!(right == left));
        }
    }
}

#[test]
fn sound_content_comparison_keeps_envelope_owner_order_and_component_identity() {
    let original = content_comparison_sound();
    assert_eq!(original, original.clone());
    let mut changed = vec![
        {
            let base = original.clone();
            crate::test_support::FiniteFixture::new(
                crate::test_support::finite(3.0),
                base.fade_seconds(),
                base.components().to_vec(),
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        },
        {
            let base = original.clone();
            crate::test_support::FiniteFixture::new(
                base.duration_seconds(),
                crate::test_support::finite(0.25),
                base.components().to_vec(),
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        },
    ];
    let mut shorter = original.clone();
    shorter.components.pop();
    changed.push(shorter);
    let mut owner = original.clone();
    owner.components[0].id = ComponentId::new(8).unwrap();
    changed.push(owner);
    let mut reordered = original.clone();
    reordered.components.swap(0, 1);
    changed.push(reordered);
    let mut parameter = original.clone();
    parameter.components[1].form = binaural_form(pair(-7.0));
    changed.push(parameter);
    let mut shape = original.clone();
    shape.components[0].form = binaural_form(pair(-6.0));
    changed.push(shape);
    for candidate in changed {
        assert!(!(original == candidate), "{candidate:?}");
        assert!(!(candidate == original));
    }
    // Envelope comparisons retain bits too, including invalid-but-editable values.
    for (first, second) in [
        (0.0, -0.0),
        (
            f64::from_bits(0x7ff8_0000_0000_0042),
            f64::from_bits(0x7ff8_0000_0000_0043),
        ),
    ] {
        if !first.is_finite() {
            assert!(crate::Finite::new(first).is_none());
            assert!(crate::Finite::new(second).is_none());
            continue;
        }
        let duration = {
            let base = original.clone();
            crate::test_support::FiniteFixture::new(
                crate::test_support::finite(first),
                base.fade_seconds(),
                base.components().to_vec(),
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        };
        assert_eq!(duration, duration.clone());
        assert!(
            !(duration == {
                let base = duration.clone();
                crate::test_support::FiniteFixture::new(
                    crate::test_support::finite(second),
                    base.fade_seconds(),
                    base.components().to_vec(),
                )
                .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
            })
        );
        let fade = {
            let base = original.clone();
            crate::test_support::FiniteFixture::new(
                base.duration_seconds(),
                crate::test_support::finite(first),
                base.components().to_vec(),
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        };
        assert_eq!(fade, fade.clone());
        assert!(
            !(fade == {
                let base = fade.clone();
                crate::test_support::FiniteFixture::new(
                    base.duration_seconds(),
                    crate::test_support::finite(second),
                    base.components().to_vec(),
                )
                .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
            })
        );
    }
}

#[test]
fn node_choices_headings_routes_and_component_text_describe_authored_intent() {
    use crate::{CarrierKind as C, Modulation as M, ModulationKind as K};
    let plain = tone(437.0, -24.0, ChannelRouting::Left);
    for (value, carrier, modulation, heading, editable) in [
        (plain, Some(C::Tone), None, "Steady tone", true),
        (
            {
                let base = plain;
                crate::ComponentForm {
                    source: crate::SoundSource::PulseTrain {
                        route: base.route().unwrap(),
                        peak_dbfs: base.get(crate::ParameterId::PeakLevel).unwrap(),
                        pulse: crate::test_support::rectangular_pulse(40.0, 0.002),
                    },
                    ..base
                }
            },
            Some(C::PulseTrain),
            None,
            "Pulse train",
            true,
        ),
        (
            crate::ComponentForm {
                modulation: Some(M::Sinusoidal {
                    rate_hz: crate::test_support::finite(7.0),
                    depth: crate::test_support::finite(0.5),
                }),
                ..plain
            },
            Some(C::Tone),
            Some(K::Sinusoidal),
            "Tone with sinusoidal AM",
            true,
        ),
        (
            crate::ComponentForm {
                modulation: Some(M::Pulsed {
                    pulse: crate::PulseSpec {
                        rate_hz: crate::test_support::finite(7.0),
                        width: crate::PulseWidth::Duration {
                            seconds: crate::test_support::finite(0.01),
                        },
                        shape: crate::PulseShape::Rectangular {},
                    },
                }),
                ..plain
            },
            Some(C::Tone),
            Some(K::Pulsed),
            "Pulsed tone (isochronic)",
            true,
        ),
        (
            binaural_form(pair(-6.0)),
            None,
            None,
            "Binaural pair",
            false,
        ),
        (
            monaural_form(pair(-6.0), ChannelRouting::Right),
            None,
            None,
            "Monaural pair",
            true,
        ),
    ] {
        assert_eq!(value.primitive_kind(), carrier);
        assert_eq!(value.modulation_kind(), modulation);
        assert_eq!(value.heading(), heading);
        assert_eq!(value.route().is_some(), editable);
    }
}

#[test]
fn component_deserialization_rejects_zero_and_preserves_positive_owners() {
    for value in [
        tone(437.0, -24.0, ChannelRouting::Both),
        binaural_form(pair(-6.0)),
        monaural_form(pair(6.0), ChannelRouting::Right),
    ] {
        let zero = toml::to_string(&form(7, value))
            .unwrap()
            .replace("id = 7", "id = 0");
        assert!(toml::from_str::<Component>(&zero).is_err());
        let valid = form(7, value);
        let text = toml::to_string(&valid).unwrap();
        assert_eq!(toml::from_str::<Component>(&text).unwrap(), valid);
    }
}
#[test]
fn signed_pairs_compile_ordered_lanes_and_match_independent_tones() {
    for (offset, second) in [(2.0, 202.0), (-2.0, 198.0), (0.0, 200.0), (40.0, 240.0)] {
        for (node_kind, routes) in [
            (
                binaural_form(pair(offset)),
                [ChannelRouting::Left, ChannelRouting::Right],
            ),
            (
                monaural_form(pair(offset), ChannelRouting::Both),
                [ChannelRouting::Both, ChannelRouting::Both],
            ),
        ] {
            for hz in [44_100, 48_000, 96_000] {
                let rate = SampleRate::new(hz).unwrap();
                let target = TargetCheck::authored(
                    rate,
                    &crate::test_support::authored(&[form(u32::MAX, node_kind)]),
                )
                .into_result()
                .unwrap();
                let lanes = target.voices().collect::<Vec<_>>();
                assert_eq!(
                    lanes.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                    [
                        ComponentId::new(u32::MAX).unwrap().into(),
                        ComponentId::new(u32::MAX)
                            .unwrap()
                            .with_lane(Lane::Secondary),
                    ]
                );
                for (index, (_, facts)) in lanes.iter().enumerate() {
                    assert_eq!(facts.route, routes[index]);
                    assert_eq!(facts.peak_dbfs, [-20.0, -30.0][index]);
                    let crate::CarrierFacts::Tone(rate) = facts.carrier else {
                        panic!("pair tone")
                    };
                    assert_eq!(rate.requested_hz, [200.0, second][index]);
                }
                let actual = target.signal(1.0, 0.1).unwrap();
                let expected = crate::test_support::FiniteFixture::new(
                    crate::test_support::finite(1.0),
                    crate::test_support::finite(0.1),
                    (vec![
                        tone(200.0, -20.0, routes[0]),
                        tone(second, -30.0, routes[1]),
                    ])
                    .into_iter()
                    .enumerate()
                    .map(|(index, voice)| auditorylab_core::Component {
                        id: auditorylab_core::ComponentId::numbered(index).unwrap(),
                        form: voice,
                    })
                    .collect(),
                )
                .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
                .signal(rate)
                .unwrap();
                for frame in [0, 1, 199, 4799, 9999, 44098] {
                    assert_eq!(
                        actual.frame(frame).map(f64::to_bits),
                        expected.frame(frame).map(f64::to_bits)
                    );
                }
            }
        }
    }
}
#[test]
fn authored_order_and_weighted_lane_capacity_are_enforced() {
    let raw = tone(401.0, -40.0, ChannelRouting::Both);
    let values = [
        form(7, raw),
        form(2, binaural_form(pair(6.0))),
        form(9, raw),
    ];
    let target = crate::test_support::authored(&values)
        .target(SampleRate::DEFAULT)
        .unwrap();
    let lanes = target.voices().collect::<Vec<_>>();
    assert_eq!(
        lanes.iter().map(|(id, _)| id.get()).collect::<Vec<_>>(),
        [7, 2, 2, 9]
    );
    assert_eq!(lanes[0].1.carrier, lanes[3].1.carrier);
    let eight = (1..=8)
        .map(|id| form(id, binaural_form(pair(6.0))))
        .collect::<Vec<_>>();
    assert_eq!(
        crate::test_support::authored(&eight)
            .target(SampleRate::DEFAULT)
            .unwrap()
            .voices()
            .count(),
        16
    );
    let mut nine = eight;
    nine.push(form(9, raw));
    assert_eq!(
        crate::test_support::FiniteFixture::new(
            crate::test_support::finite(1.0),
            crate::test_support::finite(0.0),
            nine.clone()
        ),
        Err(StructureError::VoiceBudget { count: 17 })
    );
    let raw17 = (1..=17).map(|id| form(id, raw)).collect::<Vec<_>>();
    assert_eq!(
        crate::test_support::FiniteFixture::new(
            crate::test_support::finite(1.0),
            crate::test_support::finite(0.0),
            raw17.clone()
        ),
        Err(StructureError::VoiceBudget { count: 17 })
    );
    assert_eq!(
        crate::test_support::FiniteFixture::new(
            crate::test_support::finite(1.0),
            crate::test_support::finite(0.0),
            [values[0], values[0]].to_vec()
        ),
        Err(StructureError::Identity {
            id: ComponentId::new(7).unwrap()
        })
    );
}
#[test]
fn coupled_pair_failures_are_not_clamped_or_rounded_into_valid_audio() {
    for (first, offset, issue) in [
        (200.0, -200.0, PairIssue::Nonpositive),
        (f64::MAX, f64::MAX, PairIssue::Nonfinite),
        (
            200.0,
            f64::from_bits(1),
            PairIssue::UnrepresentablePairDifference,
        ),
    ] {
        let p = Pair {
            first_hz: crate::test_support::finite(first),
            offset_hz: crate::test_support::finite(offset),
            ..pair(0.0)
        };
        assert!(matches!(p.carriers(ComponentId::new(1).unwrap()),
                Err(PairError {issue:found,..}) if found==issue));
    }
}
#[test]
fn authored_wire_shape_has_one_carrier_authority_and_preserves_ids() {
    let valid = "[[component]]\nid=4294967295\n[component.source]\nkind='binaural_pair'\n\
        first_hz=200\noffset_hz=-2\nfirst_peak_dbfs=-20\nsecond_peak_dbfs=-30\n\
        [component.modulation]\nkind='none'\n";
    let parsed: Document = toml::from_str(valid).unwrap();
    assert_eq!(
        parsed.component,
        [form(u32::MAX, binaural_form(pair(-2.0)))]
    );
    let written = toml::to_string(&parsed).unwrap();
    assert_eq!(
        toml::from_str::<Document>(&written).unwrap().component,
        parsed.component
    );
    for extra in [
        "right_hz=198\n",
        "route='both'\n",
        "band='delta'\n",
        "[component.modulation]\nkind='am'\nrate_hz=2\ndepth=1\n",
    ] {
        assert!(toml::from_str::<Document>(&format!("{valid}{extra}")).is_err());
    }
    assert!(toml::from_str::<Document>(&valid.replace("id=4294967295", "id=0")).is_err());
    assert!(toml::from_str::<Document>(&valid.replace("id=4294967295\n", "")).is_err());
}

#[test]
fn requested_pair_intent_survives_identical_realized_audio_without_a_new_live_ramp() {
    use auditorylab_core::{ParameterId, SoundError};
    let rate = SampleRate::new(768_000).unwrap();
    let mut p = pair(13.0);
    p.first_hz = crate::test_support::finite(200_000.0);
    let a = crate::test_support::FiniteFixture::new(
        crate::test_support::finite(1.0),
        crate::test_support::finite(0.0),
        vec![form(1, binaural_form(p))],
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
    p.offset_hz = crate::test_support::finite(13.0_f64.next_up());
    let b = {
        let base = a.clone();
        crate::test_support::FiniteFixture::new(
            base.duration_seconds(),
            base.fade_seconds(),
            vec![form(1, binaural_form(p))],
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
    };
    let first = a.target(rate).unwrap();
    let second = b.target(rate).unwrap();
    assert!(first.same_audio(&second));
    assert_ne!(first, second);
    assert_eq!(
        auditorylab_core::rhythm::nominal(&crate::test_support::authored(&a.components)).bands,
        [auditorylab_core::rhythm::Band::Alpha]
    );
    assert_eq!(
        auditorylab_core::rhythm::nominal(&crate::test_support::authored(&b.components)).bands,
        [auditorylab_core::rhythm::Band::Beta]
    );
    assert_eq!(
        second.voices().next().unwrap().1.pair.unwrap().offset_hz,
        13.0_f64.next_up()
    );
    let mut engine = auditorylab_core::live::Engine::new(rate);
    assert!(engine.set_revision(&first, 1));
    let mut frames = vec![[0.0; 2]; usize::try_from(engine.ramp_frames() + 1).unwrap()];
    engine.render(&mut frames);
    assert!(engine.set_revision(&second, 2));
    engine.render(&mut [[0.0; 2]; 1]);
    let observation = engine.observation().unwrap();
    assert!(!observation.transitioning);
    assert!(
        observation
            .voices
            .iter()
            .flatten()
            .all(|v| v.facts.pair.unwrap().offset_hz == 13.0_f64.next_up())
    );
    // Both coupled fields are attributed when the second requested carrier fails.
    let bad = {
        let base = a.clone();
        crate::test_support::FiniteFixture::new(
            base.duration_seconds(),
            base.fade_seconds(),
            vec![form(
                1,
                binaural_form(Pair {
                    first_hz: crate::test_support::finite(23_999.0),
                    offset_hz: crate::test_support::finite(6.0),
                    ..pair(0.0)
                }),
            )],
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
    };
    let error = bad.target(SampleRate::DEFAULT).unwrap_err();
    assert!(matches!(error, SoundError::PairCarrier { .. }));
    assert_eq!(
        error.parameters(),
        [ParameterId::PairFirstFrequency, ParameterId::PairOffset]
    );
    let mut both_bad = bad;
    both_bad.set_duration(crate::test_support::finite(-1.0));
    assert!(
        matches!(
            both_bad.signal(SampleRate::DEFAULT),
            Err(SoundError::Duration { .. })
        ),
        "finite envelope error precedence is retained"
    );
}
#[test]
fn live_pair_replacements_and_stop_keep_the_expanded_slot_bound() {
    let rate = SampleRate::DEFAULT;
    let make = |start: u32, offset| {
        (0_u32..8)
            .map(|i| {
                form(
                    start + i,
                    binaural_form(Pair {
                        first_hz: crate::test_support::finite(200.0 + f64::from(i) * 20.0),
                        first_peak_dbfs: crate::test_support::finite(-40.0),
                        second_peak_dbfs: crate::test_support::finite(-40.0),
                        ..pair(offset)
                    }),
                )
            })
            .collect::<Vec<_>>()
    };
    let a = auditorylab_core::TargetCheck::authored(
        rate,
        &crate::test_support::authored(&make(1, 6.0)),
    )
    .into_result()
    .unwrap();
    let b = auditorylab_core::TargetCheck::authored(
        rate,
        &crate::test_support::authored(&make(101, 40.0)),
    )
    .into_result()
    .unwrap();
    let mut engine = auditorylab_core::live::Engine::new(rate);
    engine.set(&a);
    let mut frames = [[0.0; 2]; 256];
    for n in 0..24 {
        engine.set(if n % 2 == 0 { &b } else { &a });
        engine.render(&mut frames);
        assert!(
            frames
                .iter()
                .flatten()
                .all(|v| v.is_finite() && v.abs() <= 1.0)
        );
        assert!(
            engine
                .observation()
                .unwrap()
                .voices
                .iter()
                .flatten()
                .count()
                <= 32
        );
    }
    engine.stop();
    for _ in 0..30 {
        engine.render(&mut frames);
        if engine.is_finished() {
            break;
        }
    }
    assert!(engine.is_finished());
    assert_eq!(engine.render(&mut frames), 0);
    assert!(frames.iter().all(|f| *f == [0.0; 2]));
}
#[test]
fn pair_timing_extends_only_the_bounded_model_not_the_audio_history() {
    let nodes = [form(1, binaural_form(pair(2.0)))];
    let target = auditorylab_core::TargetCheck::authored(
        SampleRate::DEFAULT,
        &crate::test_support::authored(&nodes),
    )
    .into_result()
    .unwrap();
    let model = target.timing_model(512);
    assert!(
        (96_000..=96_001).contains(&model.frames),
        "four ideal cycles span 2 seconds; generated phase rounding and upward \
            frame rounding can add one frame"
    );
    assert!(!model.insufficient_cycle);
    assert!(
        model
            .lanes
            .iter()
            .all(|lane| lane.kind == auditorylab_core::timing::ModelKind::Steady)
    );
    let slow = auditorylab_core::TargetCheck::authored(
        SampleRate::DEFAULT,
        &crate::test_support::authored(&[form(1, binaural_form(pair(0.1)))]),
    )
    .into_result()
    .unwrap()
    .timing_model(512);
    assert!(slow.insufficient_cycle);
    assert_eq!(slow.frames, 192_000);
    assert!(slow.lanes.iter().all(|lane| lane.columns.len() <= 512));
}

#[test]
fn raw_component_budget_and_duplicate_owners_are_checked_at_the_authored_boundary() {
    let raw = tone(440.0, -40.0, ChannelRouting::Both);
    let sixteen = (1..=16).map(|id| form(id, raw)).collect::<Vec<_>>();
    assert_eq!(
        crate::test_support::authored(&sixteen)
            .target(SampleRate::DEFAULT)
            .unwrap()
            .voices()
            .count(),
        16
    );
    let mut seventeen = sixteen;
    seventeen.push(form(17, raw));
    assert_eq!(
        crate::test_support::FiniteFixture::new(
            crate::test_support::finite(1.0),
            crate::test_support::finite(0.0),
            seventeen.clone()
        ),
        Err(StructureError::VoiceBudget { count: 17 })
    );
    assert!(ComponentId::new(0).is_none());
    let duplicate = [form(1, raw); 2];
    assert_eq!(
        crate::test_support::FiniteFixture::new(
            crate::test_support::finite(1.0),
            crate::test_support::finite(0.0),
            duplicate.to_vec()
        ),
        Err(StructureError::Identity {
            id: ComponentId::new(1).unwrap()
        })
    );
}

#[test]
fn mixed_explicit_pairs_retain_independent_requested_facts_on_every_lane() {
    use auditorylab_core::{PairFacts, PairKind, TargetCheck};
    let authored = [
        form(7, tone(400.0, -60.0, ChannelRouting::Both)),
        form(
            11,
            ComponentForm {
                source: crate::SoundSource::BinauralPair {
                    first_hz: crate::test_support::finite(200.0),
                    offset_hz: crate::test_support::finite(-6.0),
                    first_peak_dbfs: crate::test_support::finite(-30.0),
                    second_peak_dbfs: crate::test_support::finite(-30.0),
                },
                modulation: None,
            },
        ),
        form(
            29,
            monaural_form(
                Pair {
                    first_hz: crate::test_support::finite(210.0),
                    offset_hz: crate::test_support::finite(6.0),
                    first_peak_dbfs: crate::test_support::finite(-30.0),
                    second_peak_dbfs: crate::test_support::finite(-30.0),
                },
                ChannelRouting::Both,
            ),
        ),
    ];
    let expected = [
        PairFacts {
            owner: ComponentId::new(11).unwrap(),
            kind: PairKind::Binaural,
            first_hz: 200.0,
            offset_hz: -6.0,
            second_hz: 194.0,
            realized_offset_hz: Some(-6.0),
        },
        PairFacts {
            owner: ComponentId::new(29).unwrap(),
            kind: PairKind::Monaural,
            first_hz: 210.0,
            offset_hz: 6.0,
            second_hz: 216.0,
            realized_offset_hz: Some(6.0),
        },
    ];
    let check = TargetCheck::authored(
        SampleRate::DEFAULT,
        &crate::test_support::authored(&authored),
    );
    let target = check.target().unwrap();
    assert_eq!(check.voices().len(), 5);
    assert_eq!(target.len(), 5);
    assert!(
        check
            .voice(ComponentId::new(7).unwrap().into())
            .unwrap()
            .as_ref()
            .unwrap()
            .pair
            .is_none()
    );
    for facts in expected {
        for lane in [Lane::Primary, Lane::Secondary] {
            let id = facts.owner.with_lane(lane);
            assert_eq!(check.voice(id).unwrap().as_ref().unwrap().pair, Some(facts));
            assert_eq!(
                target
                    .voices()
                    .find(|(owner, _)| *owner == id)
                    .unwrap()
                    .1
                    .pair,
                Some(facts)
            );
            assert_eq!(
                target
                    .observations()
                    .find(|voice| voice.id == id)
                    .unwrap()
                    .facts
                    .pair,
                Some(facts)
            );
        }
    }
}

#[test]
fn authored_callback_workloads_preserve_distinct_owner_lanes() {
    for hz in [48_000, 768_000] {
        let rate = SampleRate::new(hz).unwrap();
        for clicks in [false, true] {
            let voices: Vec<_> = (0_u32..16)
                .map(|n| {
                    let source = if clicks {
                        crate::SoundSource::PulseTrain {
                            route: ChannelRouting::Both,
                            peak_dbfs: crate::test_support::finite(-30.0),
                            pulse: crate::test_support::rectangular_pulse(
                                40.0 + f64::from(n),
                                0.001,
                            ),
                        }
                    } else {
                        crate::SoundSource::Tone {
                            route: ChannelRouting::Both,
                            peak_dbfs: crate::test_support::finite(-30.0),
                            frequency_hz: crate::test_support::finite(200.0 + f64::from(n) * 20.0),
                        }
                    };
                    (
                        ComponentId::new(n + 1).unwrap(),
                        crate::ComponentForm {
                            source,
                            modulation: Some(crate::Modulation::Sinusoidal {
                                rate_hz: crate::test_support::finite(23.0 + f64::from(n) * 0.75),
                                depth: crate::test_support::finite(1.0),
                            }),
                        },
                    )
                })
                .collect();
            let authored = crate::test_support::FiniteFixture::new(
                crate::test_support::finite(1.0),
                crate::test_support::finite(0.0),
                voices
                    .iter()
                    .map(|(id, voice)| Component {
                        id: *id,
                        form: *voice,
                    })
                    .collect(),
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
            let target = authored.target(rate).unwrap();
            assert_eq!(target.len(), 16);
            assert_eq!(target.sample_rate(), rate);
            assert_eq!(
                target
                    .voices()
                    .map(|(id, _)| (id.component.get(), id.lane))
                    .collect::<Vec<_>>(),
                (1..=16).map(|n| (n, Lane::Primary)).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn structural_failures_are_single_and_distinguish_both_budgets() {
    let raw = tone(440.0, -40.0, ChannelRouting::Both);
    for (components, expected) in [
        (
            vec![form(7, raw); 2],
            StructureError::Identity {
                id: ComponentId::new(7).unwrap(),
            },
        ),
        (
            (1..=17).map(|id| form(id, raw)).collect(),
            StructureError::VoiceBudget { count: 17 },
        ),
        (
            (1..=9)
                .map(|id| form(id, binaural_form(pair(6.0))))
                .collect(),
            StructureError::VoiceBudget { count: 18 },
        ),
    ] {
        assert_eq!(
            crate::test_support::FiniteFixture::new(
                crate::test_support::finite(1.0),
                crate::test_support::finite(0.0),
                components
            ),
            Err(expected)
        );
    }
}

#[test]
fn direct_pair_compilation_reports_the_failing_lane_and_requested_carrier() {
    use crate::{ParameterId, SoundError, VoiceError};
    let owner = ComponentId::new(42).unwrap();
    for (first_hz, offset_hz, lane, failed_hz) in [
        (24_000.0, -6.0, Lane::Primary, 24_000.0),
        (23_999.0, 6.0, Lane::Secondary, 24_005.0),
    ] {
        let components = [form(
            42,
            binaural_form(Pair {
                first_hz: crate::test_support::finite(first_hz),
                offset_hz: crate::test_support::finite(offset_hz),
                ..pair(0.0)
            }),
        )];
        let check = TargetCheck::authored(
            SampleRate::DEFAULT,
            &crate::test_support::authored(&components),
        );
        assert_eq!(
            check.result(),
            Err(&SoundError::PairCarrier {
                id: owner.with_lane(lane),
                first_hz,
                offset_hz,
                error: VoiceError::Frequency {
                    parameter: ParameterId::ToneFrequency,
                    hz: failed_hz,
                    sample_rate_hz: 48_000
                },
            })
        );
        assert!(check.voice(owner.with_lane(lane)).unwrap().is_err());
    }
}

#[test]
fn direct_pair_arithmetic_failures_retain_their_independent_reason() {
    for (first, offset, expected) in [
        (f64::NAN, 6.0, PairIssue::Nonfinite),
        (200.0, f64::INFINITY, PairIssue::Nonfinite),
        (f64::MAX, f64::MAX, PairIssue::Nonfinite),
        (0.0, 6.0, PairIssue::Nonpositive),
        (200.0, -200.0, PairIssue::Nonpositive),
        (
            200.0,
            f64::from_bits(1),
            PairIssue::UnrepresentablePairDifference,
        ),
    ] {
        if !first.is_finite() || !offset.is_finite() {
            assert!(crate::Finite::new(first).is_none() || crate::Finite::new(offset).is_none());
            continue;
        }
        let components = [form(
            7,
            binaural_form(Pair {
                first_hz: crate::test_support::finite(first),
                offset_hz: crate::test_support::finite(offset),
                ..pair(0.0)
            }),
        )];
        let check = TargetCheck::authored(
            SampleRate::DEFAULT,
            &crate::test_support::authored(&components),
        );
        let Err(crate::SoundError::Pair(PairError {
            issue,
            id,
            first_hz,
            offset_hz,
        })) = check.result()
        else {
            panic!("missing pair arithmetic failure")
        };
        assert_eq!(*issue, expected);
        assert_eq!(id.get(), 7);
        assert_eq!(first_hz.to_bits(), first.to_bits());
        assert_eq!(offset_hz.to_bits(), offset.to_bits());
        assert_eq!(check.voices(), []);
    }
}

#[test]
fn standalone_forms_use_the_located_strict_component_codec() {
    let text = "[source]\nkind = 'tone'\nroute = 'both'\nfrequency_hz = nan\npeak_dbfs = \
        -20.0\n[modulation]\nkind='none'\n";
    let error = toml::from_str::<ComponentForm>(text).unwrap_err();
    assert!(error.message().contains("expected a finite number"));
    let span = error.span().unwrap();
    assert_eq!(&text[span], "nan");
    let valid = text.replace("nan", "440.0");
    let value = toml::from_str::<ComponentForm>(&valid).unwrap();
    assert_eq!(
        toml::from_str::<ComponentForm>(&toml::to_string(&value).unwrap()).unwrap(),
        value
    );
    assert!(toml::from_str::<ComponentForm>(&format!("id = 1\n{valid}")).is_err());
}

#[test]
fn disallowed_fields_keep_the_parameter_schema_rejection_order() {
    let text = "id=1\n[source]\nkind='tone'\nroute='both'\nfrequency_hz=440.0\npeak_dbfs=-20.0\n\
        rate_hz=40.0\n[modulation]\nkind='none'\n";
    let error = toml::from_str::<Component>(text).unwrap_err();
    assert!(error.message().contains("unknown field `rate_hz`"));
}

fn binaural_form(pair: Pair) -> ComponentForm {
    ComponentForm {
        source: crate::SoundSource::BinauralPair {
            first_hz: pair.first_hz,
            offset_hz: pair.offset_hz,
            first_peak_dbfs: pair.first_peak_dbfs,
            second_peak_dbfs: pair.second_peak_dbfs,
        },
        modulation: None,
    }
}
fn monaural_form(pair: Pair, route: ChannelRouting) -> ComponentForm {
    ComponentForm {
        source: crate::SoundSource::MonauralPair {
            route,
            first_hz: pair.first_hz,
            offset_hz: pair.offset_hz,
            first_peak_dbfs: pair.first_peak_dbfs,
            second_peak_dbfs: pair.second_peak_dbfs,
        },
        modulation: None,
    }
}

struct ComponentComparisonCases {
    different: Vec<(ComponentForm, ComponentForm)>,
    reflexive: [ComponentForm; 6],
}
#[expect(
    clippy::too_many_lines,
    reason = "Independent data corpus enumerates authored fields and both explicit pair forms"
)]
fn component_comparison_cases() -> ComponentComparisonCases {
    use crate::Modulation as M;
    let plain = tone(437.0, -24.0, ChannelRouting::Left);
    let click = {
        let base = plain;
        crate::ComponentForm {
            source: crate::SoundSource::PulseTrain {
                route: base.route().unwrap(),
                peak_dbfs: base.get(crate::ParameterId::PeakLevel).unwrap(),
                pulse: crate::test_support::rectangular_pulse(40.0, 0.002),
            },
            ..base
        }
    };
    let am = crate::ComponentForm {
        modulation: Some(M::Sinusoidal {
            rate_hz: crate::test_support::finite(7.0),
            depth: crate::test_support::finite(0.5),
        }),
        ..plain
    };
    let gate = crate::ComponentForm {
        modulation: Some(M::Pulsed {
            pulse: crate::PulseSpec {
                rate_hz: crate::test_support::finite(7.0),
                width: crate::PulseWidth::Duration {
                    seconds: crate::test_support::finite(0.01),
                },
                shape: crate::PulseShape::Rectangular {},
            },
        }),
        ..plain
    };
    let p = pair(-6.0);
    // Each unequal pair changes one authored fact; audio equivalence is irrelevant.
    let mut unequal = vec![
        (plain, {
            let mut value = plain;
            let crate::SoundSource::Tone { route, .. } = &mut value.source else {
                panic!("tone fixture")
            };
            *route = ChannelRouting::Right;
            value
        }),
        (plain, click),
        (plain, am),
        (am, gate),
        (plain, tone(438.0, -24.0, ChannelRouting::Left)),
        (plain, {
            let mut value = plain;
            assert!(value.set(
                crate::ParameterId::PeakLevel,
                crate::test_support::finite(-25.0)
            ));
            value
        }),
        (click, {
            let base = click;
            crate::ComponentForm {
                source: crate::SoundSource::PulseTrain {
                    route: base.route().unwrap(),
                    peak_dbfs: base.get(crate::ParameterId::PeakLevel).unwrap(),
                    pulse: crate::test_support::rectangular_pulse(41.0, 0.002),
                },
                ..base
            }
        }),
        (click, {
            let base = click;
            crate::ComponentForm {
                source: crate::SoundSource::PulseTrain {
                    route: base.route().unwrap(),
                    peak_dbfs: base.get(crate::ParameterId::PeakLevel).unwrap(),
                    pulse: crate::test_support::rectangular_pulse(40.0, 0.003),
                },
                ..base
            }
        }),
        (
            am,
            crate::ComponentForm {
                modulation: Some(M::Sinusoidal {
                    rate_hz: crate::test_support::finite(8.0),
                    depth: crate::test_support::finite(0.5),
                }),
                ..am
            },
        ),
        (
            am,
            crate::ComponentForm {
                modulation: Some(M::Sinusoidal {
                    rate_hz: crate::test_support::finite(7.0),
                    depth: crate::test_support::finite(0.6),
                }),
                ..am
            },
        ),
        (
            gate,
            crate::ComponentForm {
                modulation: Some(M::Pulsed {
                    pulse: crate::PulseSpec {
                        rate_hz: crate::test_support::finite(8.0),
                        width: crate::PulseWidth::Duration {
                            seconds: crate::test_support::finite(0.01),
                        },
                        shape: crate::PulseShape::Rectangular {},
                    },
                }),
                ..gate
            },
        ),
        (
            gate,
            crate::ComponentForm {
                modulation: Some(M::Pulsed {
                    pulse: crate::PulseSpec {
                        rate_hz: crate::test_support::finite(7.0),
                        width: crate::PulseWidth::Duration {
                            seconds: crate::test_support::finite(0.02),
                        },
                        shape: crate::PulseShape::Rectangular {},
                    },
                }),
                ..gate
            },
        ),
        (binaural_form(p), monaural_form(p, ChannelRouting::Left)),
        (plain, binaural_form(p)),
        (
            monaural_form(p, ChannelRouting::Left),
            monaural_form(p, ChannelRouting::Right),
        ),
    ];
    for changed in [
        Pair {
            first_hz: crate::test_support::finite(201.0),
            ..p
        },
        Pair {
            offset_hz: crate::test_support::finite(-7.0),
            ..p
        },
        Pair {
            first_peak_dbfs: crate::test_support::finite(-21.0),
            ..p
        },
        Pair {
            second_peak_dbfs: crate::test_support::finite(-31.0),
            ..p
        },
    ] {
        unequal.push((binaural_form(p), binaural_form(changed)));
        unequal.push((
            monaural_form(p, ChannelRouting::Both),
            monaural_form(changed, ChannelRouting::Both),
        ));
    }

    ComponentComparisonCases {
        different: unequal,
        reflexive: [
            plain,
            click,
            am,
            gate,
            binaural_form(p),
            monaural_form(p, ChannelRouting::Both),
        ],
    }
}

fn content_comparison_sound() -> FiniteFixture {
    crate::test_support::FiniteFixture::new(
        crate::test_support::finite(2.0),
        crate::test_support::finite(0.125),
        vec![
            form(7, tone(437.0, -24.0, ChannelRouting::Left)),
            form(19, binaural_form(pair(-6.0))),
        ],
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
}
