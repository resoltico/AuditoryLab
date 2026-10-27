//! Independent signed-pair, owner/lane, strict shape and sample-preservation oracles.
use crate as auditorylab_core;
use auditorylab_core::{
    Carrier, Component, ComponentForm, ComponentId, Ears, Lane, Pair, PairError, PairIssue,
    SampleRate, Sound, StructureError, TargetCheck, Voice,
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
        first_hz: 200.0,
        offset_hz,
        first_peak_dbfs: -20.0,
        second_peak_dbfs: -30.0,
    }
}
fn form(id: u32, form: ComponentForm) -> Component {
    Component {
        id: ComponentId::new(id).unwrap(),
        form,
    }
}
fn tone(hz: f64, level: f64, ears: Ears) -> Voice {
    Voice {
        ears,
        peak_dbfs: level,
        carrier: Carrier::Tone { frequency_hz: hz },
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
                {
                    ComponentForm::MonauralPair {
                        ears: Ears::Right,
                        first_hz: crate::test_support::finite(pair.first_hz),
                        offset_hz: crate::test_support::finite(pair.offset_hz),
                        first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                        second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                    }
                }
            } else {
                {
                    ComponentForm::BinauralPair {
                        left_hz: crate::test_support::finite(pair.first_hz),
                        offset_hz: crate::test_support::finite(pair.offset_hz),
                        left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                        right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                    }
                }
            }
        };
        for (parameter, before, after) in [
            (
                P::PairFirstFrequency,
                200.0,
                Pair {
                    first_hz: 317.0,
                    ..original
                },
            ),
            (
                P::PairOffset,
                -6.0,
                Pair {
                    offset_hz: 317.0,
                    ..original
                },
            ),
            (
                P::PairFirstLevel,
                -20.0,
                Pair {
                    first_peak_dbfs: 317.0,
                    ..original
                },
            ),
            (
                P::PairSecondLevel,
                -30.0,
                Pair {
                    second_peak_dbfs: 317.0,
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
            P::ClickRate,
            P::ClickWidth,
            P::PeakLevel,
            P::AmRate,
            P::AmDepth,
            P::GateRate,
            P::GateWidth,
            P::Duration,
            P::Fade,
        ] {
            let mut actual = make(original);
            assert_eq!(actual.get(parameter), None);
            assert!(!actual.set(parameter, crate::test_support::finite(999.0)));
            assert_eq!(actual, make(original));
        }
    }
    let mut primitive = crate::test_support::form(tone(437.0, -24.0, Ears::Left));
    assert_eq!(
        primitive.get(P::ToneFrequency),
        Some(crate::test_support::finite(437.0))
    );
    assert_eq!(
        primitive.get(P::PeakLevel),
        Some(crate::test_support::finite(-24.0))
    );
    assert!(primitive.set(P::ToneFrequency, crate::test_support::finite(509.0)));
    assert_eq!(
        primitive,
        crate::test_support::form(tone(509.0, -24.0, Ears::Left))
    );
    assert!(!primitive.set(P::PairOffset, crate::test_support::finite(6.0)));
    assert_eq!(primitive.get(P::PairOffset), None);
    assert_eq!(
        primitive,
        crate::test_support::form(tone(509.0, -24.0, Ears::Left))
    );
}

#[test]
fn node_content_comparison_preserves_shape_routes_and_each_requested_value() {
    use crate::Modulation as M;
    let plain = tone(437.0, -24.0, Ears::Left);
    let click = Voice {
        carrier: Carrier::Clicks {
            rate_hz: 40.0,
            width_seconds: 0.002,
        },
        ..plain
    };
    let am = Voice {
        modulation: Some(M::Am {
            rate_hz: crate::test_support::finite(7.0),
            depth: crate::test_support::finite(0.5),
        }),
        ..plain
    };
    let gate = Voice {
        modulation: Some(M::Gate {
            rate_hz: crate::test_support::finite(7.0),
            width_seconds: crate::test_support::finite(0.01),
        }),
        ..plain
    };
    let p = pair(-6.0);
    // Each unequal pair changes one authored fact; audio equivalence is irrelevant.
    let mut unequal = vec![
        (
            crate::test_support::form(plain),
            crate::test_support::form(Voice {
                ears: Ears::Right,
                ..plain
            }),
        ),
        (
            crate::test_support::form(plain),
            crate::test_support::form(click),
        ),
        (
            crate::test_support::form(plain),
            crate::test_support::form(am),
        ),
        (
            crate::test_support::form(am),
            crate::test_support::form(gate),
        ),
        (
            crate::test_support::form(plain),
            crate::test_support::form(tone(438.0, -24.0, Ears::Left)),
        ),
        (
            crate::test_support::form(plain),
            crate::test_support::form(Voice {
                peak_dbfs: -25.0,
                ..plain
            }),
        ),
        (
            crate::test_support::form(click),
            crate::test_support::form(Voice {
                carrier: Carrier::Clicks {
                    rate_hz: 41.0,
                    width_seconds: 0.002,
                },
                ..click
            }),
        ),
        (
            crate::test_support::form(click),
            crate::test_support::form(Voice {
                carrier: Carrier::Clicks {
                    rate_hz: 40.0,
                    width_seconds: 0.003,
                },
                ..click
            }),
        ),
        (
            crate::test_support::form(am),
            crate::test_support::form(Voice {
                modulation: Some(M::Am {
                    rate_hz: crate::test_support::finite(8.0),
                    depth: crate::test_support::finite(0.5),
                }),
                ..am
            }),
        ),
        (
            crate::test_support::form(am),
            crate::test_support::form(Voice {
                modulation: Some(M::Am {
                    rate_hz: crate::test_support::finite(7.0),
                    depth: crate::test_support::finite(0.6),
                }),
                ..am
            }),
        ),
        (
            crate::test_support::form(gate),
            crate::test_support::form(Voice {
                modulation: Some(M::Gate {
                    rate_hz: crate::test_support::finite(8.0),
                    width_seconds: crate::test_support::finite(0.01),
                }),
                ..gate
            }),
        ),
        (
            crate::test_support::form(gate),
            crate::test_support::form(Voice {
                modulation: Some(M::Gate {
                    rate_hz: crate::test_support::finite(7.0),
                    width_seconds: crate::test_support::finite(0.02),
                }),
                ..gate
            }),
        ),
        (
            {
                let pair = p;
                ComponentForm::BinauralPair {
                    left_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            },
            {
                let pair = p;
                ComponentForm::MonauralPair {
                    ears: Ears::Left,
                    first_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            },
        ),
        (crate::test_support::form(plain), {
            let pair = p;
            ComponentForm::BinauralPair {
                left_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        }),
        (
            {
                let pair = p;
                ComponentForm::MonauralPair {
                    ears: Ears::Left,
                    first_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            },
            {
                let pair = p;
                ComponentForm::MonauralPair {
                    ears: Ears::Right,
                    first_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            },
        ),
    ];
    for changed in [
        Pair {
            first_hz: 201.0,
            ..p
        },
        Pair {
            offset_hz: -7.0,
            ..p
        },
        Pair {
            first_peak_dbfs: -21.0,
            ..p
        },
        Pair {
            second_peak_dbfs: -31.0,
            ..p
        },
    ] {
        unequal.push((
            {
                let pair = p;
                ComponentForm::BinauralPair {
                    left_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            },
            {
                let pair = changed;
                ComponentForm::BinauralPair {
                    left_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            },
        ));
        unequal.push((
            {
                let pair = p;
                ComponentForm::MonauralPair {
                    ears: Ears::Both,
                    first_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            },
            {
                let pair = changed;
                ComponentForm::MonauralPair {
                    ears: Ears::Both,
                    first_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            },
        ));
    }
    for (left, right) in unequal {
        assert!(!(left == right), "{left:?} versus {right:?}");
        assert!(!(right == left));
    }
    for value in [
        crate::test_support::form(plain),
        crate::test_support::form(click),
        crate::test_support::form(am),
        crate::test_support::form(gate),
        {
            let pair = p;
            ComponentForm::BinauralPair {
                left_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        },
        {
            let pair = p;
            ComponentForm::MonauralPair {
                ears: Ears::Both,
                first_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        },
    ] {
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
                crate::test_support::form(tone(value, -20.0, Ears::Both)),
                crate::test_support::form(tone(changed, -20.0, Ears::Both)),
            ),
            (
                {
                    let pair = pair(value);
                    ComponentForm::BinauralPair {
                        left_hz: crate::test_support::finite(pair.first_hz),
                        offset_hz: crate::test_support::finite(pair.offset_hz),
                        left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                        right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                    }
                },
                {
                    let pair = pair(changed);
                    ComponentForm::BinauralPair {
                        left_hz: crate::test_support::finite(pair.first_hz),
                        offset_hz: crate::test_support::finite(pair.offset_hz),
                        left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                        right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                    }
                },
            ),
            (
                {
                    let pair = pair(value);
                    ComponentForm::MonauralPair {
                        ears: Ears::Both,
                        first_hz: crate::test_support::finite(pair.first_hz),
                        offset_hz: crate::test_support::finite(pair.offset_hz),
                        first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                        second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                    }
                },
                {
                    let pair = pair(changed);
                    ComponentForm::MonauralPair {
                        ears: Ears::Both,
                        first_hz: crate::test_support::finite(pair.first_hz),
                        offset_hz: crate::test_support::finite(pair.offset_hz),
                        first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                        second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                    }
                },
            ),
        ] {
            assert!((left == left));
            assert!(!(left == right));
            assert!(!(right == left));
        }
    }
}

#[test]
fn sound_content_comparison_keeps_envelope_owner_order_and_component_identity() {
    let original = Sound::new(
        crate::test_support::finite(2.0),
        crate::test_support::finite(0.125),
        vec![
            form(7, crate::test_support::form(tone(437.0, -24.0, Ears::Left))),
            form(19, {
                let pair = pair(-6.0);
                ComponentForm::BinauralPair {
                    left_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            }),
        ],
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
    assert!((original == original.clone()));
    let mut changed = vec![
        {
            let base = original.clone();
            Sound::new(
                crate::test_support::finite(3.0),
                base.fade_seconds(),
                base.components().to_vec(),
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        },
        {
            let base = original.clone();
            Sound::new(
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
    parameter.components[1].form = {
        let pair = pair(-7.0);
        ComponentForm::BinauralPair {
            left_hz: crate::test_support::finite(pair.first_hz),
            offset_hz: crate::test_support::finite(pair.offset_hz),
            left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
            right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
        }
    };
    changed.push(parameter);
    let mut shape = original.clone();
    shape.components[0].form = {
        let pair = pair(-6.0);
        ComponentForm::BinauralPair {
            left_hz: crate::test_support::finite(pair.first_hz),
            offset_hz: crate::test_support::finite(pair.offset_hz),
            left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
            right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
        }
    };
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
            Sound::new(
                crate::test_support::finite(first),
                base.fade_seconds(),
                base.components().to_vec(),
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        };
        assert!((duration == duration.clone()));
        assert!(
            !(duration == {
                let base = duration.clone();
                Sound::new(
                    crate::test_support::finite(second),
                    base.fade_seconds(),
                    base.components().to_vec(),
                )
                .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
            })
        );
        let fade = {
            let base = original.clone();
            Sound::new(
                base.duration_seconds(),
                crate::test_support::finite(first),
                base.components().to_vec(),
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        };
        assert!((fade == fade.clone()));
        assert!(
            !(fade == {
                let base = fade.clone();
                Sound::new(
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
    let plain = tone(437.0, -24.0, Ears::Left);
    for (value, carrier, modulation, heading, editable) in [
        (
            crate::test_support::form(plain),
            Some(C::Tone),
            None,
            "Steady tone",
            true,
        ),
        (
            crate::test_support::form(Voice {
                carrier: Carrier::Clicks {
                    rate_hz: 40.0,
                    width_seconds: 0.002,
                },
                ..plain
            }),
            Some(C::Clicks),
            None,
            "Clicks",
            true,
        ),
        (
            crate::test_support::form(Voice {
                modulation: Some(M::Am {
                    rate_hz: crate::test_support::finite(7.0),
                    depth: crate::test_support::finite(0.5),
                }),
                ..plain
            }),
            Some(C::Tone),
            Some(K::Am),
            "AM",
            true,
        ),
        (
            crate::test_support::form(Voice {
                modulation: Some(M::Gate {
                    rate_hz: crate::test_support::finite(7.0),
                    width_seconds: crate::test_support::finite(0.01),
                }),
                ..plain
            }),
            Some(C::Tone),
            Some(K::Gate),
            "Gate",
            true,
        ),
        (
            {
                let pair = pair(-6.0);
                ComponentForm::BinauralPair {
                    left_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            },
            None,
            None,
            "Binaural",
            false,
        ),
        (
            {
                let pair = pair(-6.0);
                ComponentForm::MonauralPair {
                    ears: Ears::Right,
                    first_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            },
            None,
            None,
            "Monaural",
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
        crate::test_support::form(tone(437.0, -24.0, Ears::Both)),
        {
            let pair = pair(-6.0);
            ComponentForm::BinauralPair {
                left_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        },
        {
            let pair = pair(6.0);
            ComponentForm::MonauralPair {
                ears: Ears::Right,
                first_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        },
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
fn signed_pairs_lower_into_exact_ordered_raw_oracles_and_stable_lanes() {
    for (offset, second) in [(2.0, 202.0), (-2.0, 198.0), (0.0, 200.0), (40.0, 240.0)] {
        for (node_kind, routes) in [
            (
                {
                    let pair = pair(offset);
                    ComponentForm::BinauralPair {
                        left_hz: crate::test_support::finite(pair.first_hz),
                        offset_hz: crate::test_support::finite(pair.offset_hz),
                        left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                        right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                    }
                },
                [Ears::Left, Ears::Right],
            ),
            (
                {
                    let pair = pair(offset);
                    ComponentForm::MonauralPair {
                        ears: Ears::Both,
                        first_hz: crate::test_support::finite(pair.first_hz),
                        offset_hz: crate::test_support::finite(pair.offset_hz),
                        first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                        second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                    }
                },
                [Ears::Both, Ears::Both],
            ),
        ] {
            let lowered = crate::test_support::lower(&[form(u32::MAX, node_kind)]).unwrap();
            assert_eq!(
                lowered,
                [
                    (
                        ComponentId::new(u32::MAX).unwrap().into(),
                        tone(200.0, -20.0, routes[0])
                    ),
                    (
                        ComponentId::new(u32::MAX)
                            .unwrap()
                            .with_lane(Lane::Secondary),
                        tone(second, -30.0, routes[1])
                    )
                ]
            );
            assert_ne!(lowered[0].0, lowered[1].0);
            for hz in [44_100, 48_000, 96_000] {
                let rate = SampleRate::new(hz).unwrap();
                let target = TargetCheck::authored(
                    rate,
                    &crate::test_support::authored(&[form(u32::MAX, node_kind)]),
                )
                .into_result()
                .unwrap();
                let actual = target.signal(1.0, 0.1).unwrap();
                let expected = Sound::new(
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
                        form: crate::test_support::form(voice),
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
fn raw_order_is_never_regrouped_and_both_budgets_are_enforced() {
    let raw = tone(401.0, -40.0, Ears::Both);
    let values = [
        form(7, crate::test_support::form(raw)),
        form(2, {
            let pair = pair(6.0);
            ComponentForm::BinauralPair {
                left_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        }),
        form(9, crate::test_support::form(raw)),
    ];
    let lowered = crate::test_support::lower(&values).unwrap();
    assert_eq!(
        lowered.iter().map(|c| c.0.get()).collect::<Vec<_>>(),
        [7, 2, 2, 9]
    );
    assert_eq!(lowered[0].1, raw);
    assert_eq!(lowered[3].1, raw);
    let eight = (1..=8)
        .map(|id| {
            form(id, {
                let pair = pair(6.0);
                ComponentForm::BinauralPair {
                    left_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(crate::test_support::lower(&eight).unwrap().len(), 16);
    let mut nine = eight;
    nine.push(form(9, crate::test_support::form(raw)));
    assert_eq!(
        Sound::new(
            crate::test_support::finite(1.0),
            crate::test_support::finite(0.0),
            nine.to_vec()
        ),
        Err(StructureError::VoiceBudget { count: 17 })
    );
    let raw17 = (1..=17)
        .map(|id| form(id, crate::test_support::form(raw)))
        .collect::<Vec<_>>();
    assert_eq!(
        Sound::new(
            crate::test_support::finite(1.0),
            crate::test_support::finite(0.0),
            raw17.to_vec()
        ),
        Err(StructureError::ComponentBudget { count: 17 })
    );
    assert_eq!(
        Sound::new(
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
            first_hz: first,
            offset_hz: offset,
            ..pair(0.0)
        };
        assert!(
            matches!(crate::authored::lower(&form(1,{ let pair = p; ComponentForm::BinauralPair { left_hz: crate::test_support::finite(pair.first_hz), offset_hz: crate::test_support::finite(pair.offset_hz), left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs), right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs) } })),Err(PairError {issue:found,..}) if found==issue)
        );
    }
}
#[test]
fn authored_wire_shape_has_one_carrier_authority_and_preserves_ids() {
    let valid = "[[component]]\nid=4294967295\nkind='binaural_pair'\nleft_hz=200\noffset_hz=-2\nleft_peak_dbfs=-20\nright_peak_dbfs=-30\n";
    let parsed: Document = toml::from_str(valid).unwrap();
    assert_eq!(
        parsed.component,
        [form(u32::MAX, {
            let pair = pair(-2.0);
            ComponentForm::BinauralPair {
                left_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        })]
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
        "[component.modulation]\nkind='am'\nrate_hz=2\ndepth=1\n",
    ] {
        assert!(toml::from_str::<Document>(&format!("{valid}{extra}")).is_err());
    }
    assert!(toml::from_str::<Document>(&valid.replace("id=4294967295", "id=0")).is_err());
    assert!(toml::from_str::<Document>(&valid.replace("id=4294967295\n", "")).is_err());
}

#[test]
fn requested_pair_intent_survives_identical_realized_audio_without_a_new_live_ramp() {
    use auditorylab_core::{ParameterId, Sound, SoundError};
    let rate = SampleRate::new(768_000).unwrap();
    let mut p = pair(13.0);
    p.first_hz = 200_000.0;
    let a = Sound::new(
        crate::test_support::finite(1.0),
        crate::test_support::finite(0.0),
        vec![form(1, {
            let pair = p;
            ComponentForm::BinauralPair {
                left_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        })],
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
    p.offset_hz = 13.0_f64.next_up();
    let b = {
        let base = a.clone();
        Sound::new(
            base.duration_seconds(),
            base.fade_seconds(),
            vec![form(1, {
                let pair = p;
                ComponentForm::BinauralPair {
                    left_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            })],
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
        Sound::new(
            base.duration_seconds(),
            base.fade_seconds(),
            vec![form(1, {
                let pair = Pair {
                    first_hz: 23_999.0,
                    offset_hz: 6.0,
                    ..pair(0.0)
                };
                ComponentForm::BinauralPair {
                    left_hz: crate::test_support::finite(pair.first_hz),
                    offset_hz: crate::test_support::finite(pair.offset_hz),
                    left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                    right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                }
            })],
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
                form(start + i, {
                    let pair = Pair {
                        first_hz: 200.0 + f64::from(i) * 20.0,
                        first_peak_dbfs: -40.0,
                        second_peak_dbfs: -40.0,
                        ..pair(offset)
                    };
                    ComponentForm::BinauralPair {
                        left_hz: crate::test_support::finite(pair.first_hz),
                        offset_hz: crate::test_support::finite(pair.offset_hz),
                        left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                        right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                    }
                })
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
    let nodes = [form(1, {
        let pair = pair(2.0);
        ComponentForm::BinauralPair {
            left_hz: crate::test_support::finite(pair.first_hz),
            offset_hz: crate::test_support::finite(pair.offset_hz),
            left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
            right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
        }
    })];
    let target = auditorylab_core::TargetCheck::authored(
        SampleRate::DEFAULT,
        &crate::test_support::authored(&nodes),
    )
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
        &crate::test_support::authored(&[form(1, {
            let pair = pair(0.1);
            ComponentForm::BinauralPair {
                left_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        })]),
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
    let raw = tone(440.0, -40.0, Ears::Both);
    let sixteen = (1..=16)
        .map(|id| form(id, crate::test_support::form(raw)))
        .collect::<Vec<_>>();
    assert_eq!(crate::test_support::lower(&sixteen).unwrap().len(), 16);
    let mut seventeen = sixteen;
    seventeen.push(form(17, crate::test_support::form(raw)));
    assert_eq!(
        Sound::new(
            crate::test_support::finite(1.0),
            crate::test_support::finite(0.0),
            seventeen.to_vec()
        ),
        Err(StructureError::ComponentBudget { count: 17 })
    );
    assert!(ComponentId::new(0).is_none());
    let duplicate = [form(1, crate::test_support::form(raw)); 2];
    assert_eq!(
        Sound::new(
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
        form(7, crate::test_support::form(tone(400.0, -60.0, Ears::Both))),
        form(
            11,
            ComponentForm::BinauralPair {
                left_hz: crate::test_support::finite(200.0),
                offset_hz: crate::test_support::finite(-6.0),
                left_peak_dbfs: crate::test_support::finite(-30.0),
                right_peak_dbfs: crate::test_support::finite(-30.0),
            },
        ),
        form(29, {
            let pair = Pair {
                first_hz: 210.0,
                offset_hz: 6.0,
                first_peak_dbfs: -30.0,
                second_peak_dbfs: -30.0,
            };
            ComponentForm::MonauralPair {
                ears: Ears::Both,
                first_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                first_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                second_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        }),
    ];
    let expected = [
        PairFacts {
            owner: ComponentId::new(11).unwrap(),
            kind: PairKind::Binaural,
            first_hz: 200.0,
            offset_hz: -6.0,
            second_hz: 194.0,
        },
        PairFacts {
            owner: ComponentId::new(29).unwrap(),
            kind: PairKind::Monaural,
            first_hz: 210.0,
            offset_hz: 6.0,
            second_hz: 216.0,
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
                    let carrier = if clicks {
                        Carrier::Clicks {
                            rate_hz: 40.0 + f64::from(n),
                            width_seconds: 0.001,
                        }
                    } else {
                        Carrier::Tone {
                            frequency_hz: 200.0 + f64::from(n) * 20.0,
                        }
                    };
                    (
                        ComponentId::new(n + 1).unwrap(),
                        Voice {
                            ears: Ears::Both,
                            peak_dbfs: -30.0,
                            carrier,
                            modulation: Some(crate::Modulation::Am {
                                rate_hz: crate::test_support::finite(23.0 + f64::from(n) * 0.75),
                                depth: crate::test_support::finite(1.0),
                            }),
                        },
                    )
                })
                .collect();
            let authored = Sound::new(
                crate::test_support::finite(1.0),
                crate::test_support::finite(0.0),
                voices
                    .iter()
                    .map(|(id, voice)| Component {
                        id: *id,
                        form: crate::test_support::form(*voice),
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
    let raw = tone(440.0, -40.0, Ears::Both);
    for (components, expected) in [
        (
            vec![form(7, crate::test_support::form(raw)); 2],
            StructureError::Identity {
                id: ComponentId::new(7).unwrap(),
            },
        ),
        (
            (1..=17)
                .map(|id| form(id, crate::test_support::form(raw)))
                .collect(),
            StructureError::ComponentBudget { count: 17 },
        ),
        (
            (1..=9)
                .map(|id| {
                    form(id, {
                        let pair = pair(6.0);
                        ComponentForm::BinauralPair {
                            left_hz: crate::test_support::finite(pair.first_hz),
                            offset_hz: crate::test_support::finite(pair.offset_hz),
                            left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                            right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
                        }
                    })
                })
                .collect(),
            StructureError::VoiceBudget { count: 18 },
        ),
    ] {
        assert_eq!(
            Sound::new(
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
        let components = [form(42, {
            let pair = Pair {
                first_hz,
                offset_hz,
                ..pair(0.0)
            };
            ComponentForm::BinauralPair {
                left_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        })];
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
        let components = [form(7, {
            let pair = Pair {
                first_hz: first,
                offset_hz: offset,
                ..pair(0.0)
            };
            ComponentForm::BinauralPair {
                left_hz: crate::test_support::finite(pair.first_hz),
                offset_hz: crate::test_support::finite(pair.offset_hz),
                left_peak_dbfs: crate::test_support::finite(pair.first_peak_dbfs),
                right_peak_dbfs: crate::test_support::finite(pair.second_peak_dbfs),
            }
        })];
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
        assert!(check.voices().is_empty());
    }
}
