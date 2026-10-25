//! Independent signed-pair, owner/lane, strict shape and sample-preservation oracles.
use auditorylab_core::{
    AuthoredComponent, Carrier, Component, ComponentId, Ears, Lane, LoweringError, Node, Pair,
    PairIssue, SampleRate, Stimulus, Target, authored,
};
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Document {
    component: Vec<AuthoredComponent>,
}
fn pair(offset_hz: f64) -> Pair {
    Pair {
        first_hz: 200.0,
        offset_hz,
        first_peak_dbfs: -20.0,
        second_peak_dbfs: -30.0,
    }
}
fn node(id: u32, node: Node) -> AuthoredComponent {
    AuthoredComponent {
        id: ComponentId::new(id),
        node,
    }
}
fn tone(hz: f64, level: f64, ears: Ears) -> Component {
    Component {
        ears,
        peak_dbfs: level,
        carrier: Carrier::Tone { frequency_hz: hz },
        modulation: None,
    }
}
#[test]
fn signed_pairs_lower_into_exact_ordered_raw_oracles_and_stable_lanes() {
    for (offset, second) in [(2.0, 202.0), (-2.0, 198.0), (0.0, 200.0), (40.0, 240.0)] {
        for (node_kind, routes) in [
            (Node::BinauralPair(pair(offset)), [Ears::Left, Ears::Right]),
            (
                Node::MonauralPair {
                    ears: Ears::Both,
                    pair: pair(offset),
                },
                [Ears::Both, Ears::Both],
            ),
        ] {
            let lowered = authored::lower(&[node(u32::MAX, node_kind)]).unwrap();
            assert_eq!(
                lowered,
                [
                    (ComponentId::new(u32::MAX), tone(200.0, -20.0, routes[0])),
                    (
                        ComponentId::new(u32::MAX).with_lane(Lane::Secondary),
                        tone(second, -30.0, routes[1])
                    )
                ]
            );
            assert_ne!(lowered[0].0, lowered[1].0);
            for hz in [44_100, 48_000, 96_000] {
                let rate = SampleRate::new(hz).unwrap();
                let target = Target::new(rate, &lowered).unwrap();
                let actual = target.signal(1.0, 0.1).unwrap();
                let expected = Stimulus {
                    duration_seconds: 1.0,
                    fade_seconds: 0.1,
                    components: vec![
                        tone(200.0, -20.0, routes[0]),
                        tone(second, -30.0, routes[1]),
                    ],
                }
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
fn raw_order_is_never_regrouped_and_both_budgets_are_enforced() {
    let raw = tone(401.0, -40.0, Ears::Both);
    let values = [
        node(7, Node::Primitive(raw)),
        node(2, Node::BinauralPair(pair(6.0))),
        node(9, Node::Primitive(raw)),
    ];
    let lowered = authored::lower(&values).unwrap();
    assert_eq!(
        lowered.iter().map(|c| c.0.get()).collect::<Vec<_>>(),
        [7, 2, 2, 9]
    );
    assert_eq!(lowered[0].1, raw);
    assert_eq!(lowered[3].1, raw);
    let eight = (1..=8)
        .map(|id| node(id, Node::BinauralPair(pair(6.0))))
        .collect::<Vec<_>>();
    assert_eq!(authored::lower(&eight).unwrap().len(), 16);
    let mut nine = eight;
    nine.push(node(9, Node::Primitive(raw)));
    assert_eq!(authored::lower(&nine), Err(LoweringError::VoiceBudget));
    let raw17 = (1..=17)
        .map(|id| node(id, Node::Primitive(raw)))
        .collect::<Vec<_>>();
    assert_eq!(authored::lower(&raw17), Err(LoweringError::VoiceBudget));
    assert_eq!(
        authored::lower(&[values[0], values[0]]),
        Err(LoweringError::Identity)
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
            first_hz: first,
            offset_hz: offset,
            ..pair(0.0)
        };
        assert!(
            matches!(authored::lower(&[node(1,Node::BinauralPair(p))]),Err(LoweringError::PairDifference {issue:found,..}) if found==issue)
        );
    }
}
#[test]
fn authored_wire_shape_has_one_carrier_authority_and_preserves_ids() {
    let valid = "[[component]]\nid=4294967295\nkind='binaural_pair'\nleft_hz=200\noffset_hz=-2\nleft_peak_dbfs=-20\nright_peak_dbfs=-30\n";
    let parsed: Document = toml::from_str(valid).unwrap();
    assert_eq!(
        parsed.component,
        [node(u32::MAX, Node::BinauralPair(pair(-2.0)))]
    );
    let written = toml::to_string(&parsed).unwrap();
    assert_eq!(
        toml::from_str::<Document>(&written).unwrap().component,
        parsed.component
    );
    for extra in [
        "right_hz=198\n",
        "ears='both'\n",
        "band='delta'\n",
        "[component.modulation]\nkind='sinusoidal'\nrate_hz=2\ndepth=1\n",
    ] {
        assert!(toml::from_str::<Document>(&format!("{valid}{extra}")).is_err());
    }
    assert!(toml::from_str::<Document>(&valid.replace("id=4294967295", "id=0")).is_err());
    assert!(toml::from_str::<Document>(&valid.replace("id=4294967295\n", "")).is_err());
}

#[test]
fn requested_pair_intent_survives_identical_realized_audio_without_a_new_live_ramp() {
    use auditorylab_core::{AuthoredStimulus, ParameterId, StimulusError};
    let rate = SampleRate::new(768_000).unwrap();
    let mut p = pair(13.0);
    p.first_hz = 200_000.0;
    let a = AuthoredStimulus {
        duration_seconds: 1.0,
        fade_seconds: 0.0,
        components: vec![node(1, Node::BinauralPair(p))],
    };
    p.offset_hz = 13.0_f64.next_up();
    let b = AuthoredStimulus {
        components: vec![node(1, Node::BinauralPair(p))],
        ..a.clone()
    };
    let first = a.target(rate).unwrap();
    let second = b.target(rate).unwrap();
    assert!(first.same_audio(&second));
    assert_ne!(first, second);
    assert_eq!(
        auditorylab_core::nominal::analyze_authored(&a.components).bands,
        [auditorylab_core::nominal::Band::Alpha]
    );
    assert_eq!(
        auditorylab_core::nominal::analyze_authored(&b.components).bands,
        [auditorylab_core::nominal::Band::Beta]
    );
    assert_eq!(
        second
            .components()
            .next()
            .unwrap()
            .1
            .pair
            .unwrap()
            .offset_hz,
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
    let bad = AuthoredStimulus {
        components: vec![node(
            1,
            Node::BinauralPair(Pair {
                first_hz: 23_999.0,
                offset_hz: 6.0,
                ..pair(0.0)
            }),
        )],
        ..a.clone()
    };
    let error = bad.target(SampleRate::DEFAULT).unwrap_err();
    assert!(matches!(error, StimulusError::PairCarrier { .. }));
    assert_eq!(
        error.parameters(),
        [ParameterId::PairFirstFrequency, ParameterId::PairOffset]
    );
    let mut both_bad = bad;
    both_bad.duration_seconds = -1.0;
    assert!(
        matches!(
            both_bad.signal(SampleRate::DEFAULT),
            Err(StimulusError::Duration { .. })
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
                node(
                    start + i,
                    Node::BinauralPair(Pair {
                        first_hz: 200.0 + f64::from(i) * 20.0,
                        first_peak_dbfs: -40.0,
                        second_peak_dbfs: -40.0,
                        ..pair(offset)
                    }),
                )
            })
            .collect::<Vec<_>>()
    };
    let a = auditorylab_core::TargetCheck::authored(rate, &make(1, 6.0))
        .into_result()
        .unwrap();
    let b = auditorylab_core::TargetCheck::authored(rate, &make(101, 40.0))
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
    let nodes = [node(1, Node::BinauralPair(pair(2.0)))];
    let target = auditorylab_core::TargetCheck::authored(SampleRate::DEFAULT, &nodes)
        .into_result()
        .unwrap();
    let model = target.timing_model(512);
    assert!(
        (96_000..=96_001).contains(&model.frames),
        "four ideal cycles span 2 seconds; generated phase rounding and upward frame rounding can add one frame"
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
        &[node(1, Node::BinauralPair(pair(0.1)))],
    )
    .into_result()
    .unwrap()
    .timing_model(512);
    assert!(slow.insufficient_cycle);
    assert_eq!(slow.frames, 192_000);
    assert!(slow.lanes.iter().all(|lane| lane.columns.len() <= 512));
}

#[test]
fn every_raw_budget_and_owner_lane_boundary_is_checked_at_the_authored_boundary() {
    let raw = tone(440.0, -40.0, Ears::Both);
    let sixteen = (1..=16)
        .map(|id| node(id, Node::Primitive(raw)))
        .collect::<Vec<_>>();
    assert_eq!(authored::lower(&sixteen).unwrap().len(), 16);
    let mut seventeen = sixteen;
    seventeen.push(node(17, Node::Primitive(raw)));
    assert_eq!(authored::lower(&seventeen), Err(LoweringError::VoiceBudget));
    for invalid in [
        AuthoredComponent {
            id: ComponentId::new(0),
            node: Node::Primitive(raw),
        },
        AuthoredComponent {
            id: ComponentId::new(1).with_lane(Lane::Secondary),
            node: Node::Primitive(raw),
        },
    ] {
        assert_eq!(authored::lower(&[invalid]), Err(LoweringError::Identity));
    }
    for (first_hz, offset_hz) in [
        (f64::INFINITY, 1.0),
        (200.0, f64::INFINITY),
        (f64::NAN, 1.0),
        (200.0, f64::NAN),
    ] {
        let value = node(
            1,
            Node::BinauralPair(Pair {
                first_hz,
                offset_hz,
                ..pair(0.0)
            }),
        );
        assert!(matches!(
            authored::lower(&[value]),
            Err(LoweringError::PairDifference {
                issue: PairIssue::Nonfinite,
                ..
            })
        ));
    }
    assert!(matches!(
        authored::lower(&[node(
            1,
            Node::BinauralPair(Pair {
                first_hz: -200.0,
                offset_hz: 400.0,
                ..pair(0.0)
            })
        )]),
        Err(LoweringError::PairDifference {
            issue: PairIssue::Nonpositive,
            ..
        })
    ));
}

#[test]
fn mixed_explicit_pairs_retain_independent_requested_facts_on_every_lane() {
    use auditorylab_core::{PairFacts, PairKind, TargetCheck};
    let authored = [
        node(7, Node::Primitive(tone(400.0, -60.0, Ears::Both))),
        node(
            11,
            Node::BinauralPair(Pair {
                first_hz: 200.0,
                offset_hz: -6.0,
                first_peak_dbfs: -30.0,
                second_peak_dbfs: -30.0,
            }),
        ),
        node(
            29,
            Node::MonauralPair {
                ears: Ears::Both,
                pair: Pair {
                    first_hz: 210.0,
                    offset_hz: 6.0,
                    first_peak_dbfs: -30.0,
                    second_peak_dbfs: -30.0,
                },
            },
        ),
    ];
    let expected = [
        PairFacts {
            owner: ComponentId::new(11),
            kind: PairKind::Binaural,
            first_hz: 200.0,
            offset_hz: -6.0,
            second_hz: 194.0,
        },
        PairFacts {
            owner: ComponentId::new(29),
            kind: PairKind::Monaural,
            first_hz: 210.0,
            offset_hz: 6.0,
            second_hz: 216.0,
        },
    ];
    let check = TargetCheck::authored(SampleRate::DEFAULT, &authored);
    let target = check.target().unwrap();
    assert_eq!(check.components().len(), 5);
    assert_eq!(target.len(), 5);
    assert!(
        check
            .component(ComponentId::new(7))
            .unwrap()
            .as_ref()
            .unwrap()
            .pair
            .is_none()
    );
    for facts in expected {
        for lane in [Lane::Primary, Lane::Secondary] {
            let id = facts.owner.with_lane(lane);
            assert_eq!(
                check.component(id).unwrap().as_ref().unwrap().pair,
                Some(facts)
            );
            assert_eq!(
                target
                    .components()
                    .find(|(owner, _)| *owner == id)
                    .unwrap()
                    .1
                    .pair,
                Some(facts)
            );
            assert_eq!(
                target
                    .voices()
                    .find(|voice| voice.id == id)
                    .unwrap()
                    .facts
                    .pair,
                Some(facts)
            );
        }
    }
}
