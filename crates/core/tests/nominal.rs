//! Independent nominal boundary and structural counterexamples. No signal rendering.
use auditorylab_core::{Carrier, Ears, Modulation, Sound, Voice, nominal::*};
fn tone(hz: f64, ears: Ears) -> Voice {
    Voice {
        ears,
        peak_dbfs: -20.0,
        carrier: Carrier::Tone { frequency_hz: hz },
        modulation: None,
    }
}
#[expect(
    clippy::unwrap_used,
    reason = "Fixture identities must fit the finite test component list"
)]
fn stimulus(components: Vec<Voice>) -> Vec<auditorylab_core::Component> {
    Sound {
        duration_seconds: 60.0,
        fade_seconds: 0.1,
        components: components
            .into_iter()
            .enumerate()
            .map(|(index, voice)| auditorylab_core::Component {
                id: auditorylab_core::ComponentId::numbered(index).unwrap(),
                node: auditorylab_core::Node::Voice(voice),
            })
            .collect(),
    }
    .components
}
#[test]
fn exact_boundaries_and_adjacent_binary64_values_follow_the_disclosed_policy() {
    use Band::*;
    for (hz, below, exact, above) in [
        (0.1_f64, None, Some(Delta), Some(Delta)),
        (4.0, Some(Delta), Some(Theta), Some(Theta)),
        (8.0, Some(Theta), Some(Alpha), Some(Alpha)),
        (13.0, Some(Alpha), Some(Alpha), Some(Beta)),
        (30.0, Some(Beta), Some(Beta), Some(Gamma)),
        (80.0, Some(Gamma), Some(Gamma), None),
    ] {
        assert_eq!(Band::of(hz.next_down()), below, "below {hz}");
        assert_eq!(Band::of(hz), exact, "exact {hz}");
        assert_eq!(Band::of(hz.next_up()), above, "above {hz}");
    }
    for hz in [-40.0, 0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(Band::of(hz), None);
    }
}
#[test]
fn carrier_is_not_a_band_and_nominal_am_survives_target_invalidity() {
    for hz in [10.0, 40.0, 240.0] {
        let a = analyze(&stimulus(vec![tone(hz, Ears::Both)]));
        assert!(a.no_rhythm());
        assert!(a.bands.is_empty());
    }
    let mut c = tone(1_000.0, Ears::Both);
    c.modulation = Some(Modulation::Sinusoidal {
        rate_hz: 40.0,
        depth: 1.0,
    });
    let a = analyze(&stimulus(vec![c]));
    assert_eq!(a.bands, [Band::Gamma]);
    c.carrier = Carrier::Tone {
        frequency_hz: 100_000.0,
    };
    assert_eq!(analyze(&stimulus(vec![c])), a);
    c.modulation = Some(Modulation::Sinusoidal {
        rate_hz: 40.0,
        depth: 0.0,
    });
    assert!(analyze(&stimulus(vec![c])).no_rhythm());
    c.modulation = Some(Modulation::Sinusoidal {
        rate_hz: 40.0,
        depth: f64::from_bits(1),
    });
    assert_eq!(analyze(&stimulus(vec![c])).bands, [Band::Gamma]);
    c.peak_dbfs = -10_000.0;
    assert_eq!(analyze(&stimulus(vec![c])).bands, [Band::Gamma]);
}
#[test]
fn independent_processes_deduplicate_bands_but_never_rates() {
    let click = |hz| Voice {
        carrier: Carrier::Click {
            rate_hz: hz,
            width_seconds: 0.001,
        },
        ..tone(240.0, Ears::Both)
    };
    let a = analyze(&stimulus(vec![click(2.0), click(3.0)]));
    assert_eq!(a.rhythms.len(), 2);
    assert_eq!(a.bands, [Band::Delta]);
    assert!(a.multiple());
    let a = analyze(&stimulus(vec![click(6.0), click(40.0)]));
    assert_eq!(a.bands, [Band::Theta, Band::Gamma]);
    assert_eq!(a.matching_rate(&[Band::Gamma], &[]), Some(40.0));
    for hz in [0.05, 100.0] {
        let a = analyze(&stimulus(vec![click(hz)]));
        assert!(a.outside_bands());
        assert!(!a.no_rhythm());
    }
    for hz in [-40.0, 0.0, f64::INFINITY, f64::NAN] {
        let a = analyze(&stimulus(vec![click(hz)]));
        assert_eq!(a.completeness, Completeness::Invalid);
        assert!(a.rhythms.is_empty());
        assert!(!a.no_rhythm());
    }
}
#[test]
fn raw_inference_is_exact_and_restricted_to_the_whole_two_tone_document() {
    let left = tone(200.0, Ears::Left);
    let mut right = tone(206.0, Ears::Right);
    let a = analyze(&stimulus(vec![left, right]));
    assert_eq!(a.rhythms[0].hz, 6.0);
    assert_eq!(a.rhythms[0].method, Method::Binaural);
    right.carrier = Carrier::Tone {
        frequency_hz: 194.0,
    };
    assert_eq!(analyze(&stimulus(vec![left, right])).rhythms[0].hz, 6.0);
    right.carrier = Carrier::Tone {
        frequency_hz: 200.0,
    };
    assert!(analyze(&stimulus(vec![left, right])).no_rhythm());
    right.carrier = Carrier::Tone {
        frequency_hz: 206.0,
    };
    right.modulation = Some(Modulation::Sinusoidal {
        rate_hz: 40.0,
        depth: 0.0,
    });
    assert_eq!(analyze(&stimulus(vec![left, right])).bands, [Band::Theta]);
    let a = analyze(&stimulus(vec![left, right, tone(300.0, Ears::Both)]));
    assert_eq!(a.completeness, Completeness::Partial);
    assert!(!a.no_rhythm());
    assert!(a.rhythms.is_empty());
    let a = analyze(&stimulus(vec![tone(200.0, Ears::Both), right]));
    assert_eq!(a.completeness, Completeness::Partial);
    let a = analyze(&stimulus(vec![
        tone(200.0, Ears::Both),
        Voice {
            ears: Ears::Both,
            peak_dbfs: -40.0,
            ..right
        },
    ]));
    assert_eq!(a.rhythms[0].method, Method::Monaural);
}
#[test]
fn correlated_facets_cannot_borrow_rates_from_another_process() {
    // The descriptor oracle intentionally constructs a mixed analysis independently
    // of the primitive analyzer, which cannot attribute explicit pairs in mixtures.
    let a = Analysis {
        rhythms: vec![
            Descriptor {
                owner: auditorylab_core::ComponentId::new(1),
                related: None,
                method: Method::Binaural,
                hz: 6.0,
                band: Some(Band::Theta),
                derivation: Derivation::Explicit,
            },
            Descriptor {
                owner: auditorylab_core::ComponentId::new(2),
                related: None,
                method: Method::AmplitudeModulation,
                hz: 40.0,
                band: Some(Band::Gamma),
                derivation: Derivation::Explicit,
            },
        ],
        methods: vec![Method::Binaural, Method::AmplitudeModulation],
        bands: vec![Band::Theta, Band::Gamma],
        completeness: Completeness::Complete,
        diagnostics: vec![],
    };
    assert!(!a.matches(&[Band::Gamma], &[Method::Binaural]));
    assert!(a.matches(&[Band::Gamma], &[Method::AmplitudeModulation]));
    assert!(!a.matches(&[Band::Gamma], &[Method::SteadyTone]));
    assert!(a.matches(&[Band::Theta, Band::Gamma], &[Method::Binaural]));
    assert_eq!(a.matching_rate(&[Band::Gamma], &[]), Some(40.0));
}

#[test]
fn actual_authored_pair_mixtures_keep_nominal_intent_and_correlated_facets() {
    use auditorylab_core::{Component, ComponentId, Node, Pair};
    let pair = |id, offset_hz| Component {
        id: ComponentId::new(id),
        node: Node::BinauralPair(Pair {
            first_hz: 200.0,
            offset_hz,
            first_peak_dbfs: -30.0,
            second_peak_dbfs: -30.0,
        }),
    };
    let am = Component {
        id: ComponentId::new(90),
        node: Node::Voice(Voice {
            modulation: Some(Modulation::Sinusoidal {
                rate_hz: 40.0,
                depth: 1.0,
            }),
            ..tone(1000.0, Ears::Both)
        }),
    };
    let a = analyze(&[pair(7, 6.0), am]);
    assert_eq!(a.bands, [Band::Theta, Band::Gamma]);
    assert_eq!(a.completeness, Completeness::Complete);
    assert_eq!(
        a.rhythms.iter().map(|r| r.owner.get()).collect::<Vec<_>>(),
        [7, 90]
    );
    assert!(!a.matches(&[Band::Gamma], &[Method::Binaural]));
    assert!(a.matches(&[Band::Gamma], &[Method::AmplitudeModulation]));
    assert_eq!(analyze(&[pair(1, 2.0), pair(2, 3.0)]).bands, [Band::Delta]);
    assert!(analyze(&[pair(1, 2.0), pair(2, 3.0)]).multiple());
    for offset in [-6.0, 6.0] {
        assert_eq!(analyze(&[pair(1, offset)]).bands, [Band::Theta]);
    }
    assert!(analyze(&[pair(1, 0.0)]).no_rhythm());
    let collapsed = analyze(&[pair(1, f64::from_bits(1))]);
    assert_eq!(collapsed.rhythms[0].hz, f64::from_bits(1));
    assert!(collapsed.outside_bands());
    assert_eq!(collapsed.completeness, Completeness::Invalid);
    let mut raw = vec![pair(1, 6.0), am];
    raw.extend([
        Component {
            id: ComponentId::new(5),
            node: Node::Voice(tone(200.0, Ears::Left)),
        },
        Component {
            id: ComponentId::new(6),
            node: Node::Voice(tone(206.0, Ears::Right)),
        },
    ]);
    let a = analyze(&raw);
    assert_eq!(a.completeness, Completeness::Partial);
    assert_eq!(
        a.rhythms.len(),
        2,
        "raw tones inside a larger mix cannot add inferred processes"
    );
}

#[test]
fn catalog_methods_match_independent_expectations_at_48_khz() {
    use auditorylab_core::{Recipe, SampleRate, catalog, interpretation::Structure};
    let expected = [
        Method::Binaural,
        Method::Binaural,
        Method::Binaural,
        Method::SteadyTone,
        Method::Clicks,
        Method::Binaural,
        Method::Binaural,
        Method::Binaural,
        Method::Binaural,
        Method::Binaural,
        Method::SteadyTone,
        Method::Binaural,
        Method::Gate,
        Method::Gate,
        Method::Gate,
        Method::Gate,
        Method::Binaural,
        Method::Binaural,
        Method::Monaural,
        Method::Monaural,
        Method::AmplitudeModulation,
        Method::AmplitudeModulation,
        Method::AmplitudeModulation,
    ];
    assert_eq!(catalog::ENTRIES.len(), expected.len());
    for (entry, method) in catalog::ENTRIES.iter().zip(expected) {
        let sound = Recipe::parse(entry.text).unwrap().stimulus;
        assert_eq!(
            analyze(&sound.components).methods,
            [method],
            "{} nominal",
            entry.id
        );
        assert_eq!(
            Structure::of_target(&sound.target(SampleRate::DEFAULT).unwrap()).methods(),
            [method],
            "{} realized",
            entry.id
        );
    }
}
