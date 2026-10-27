//! Independent nominal boundary and structural counterexamples. No signal rendering.
use auditorylab_core::{ComponentForm, Ears, Modulation, Sound, rhythm::*};
fn tone(hz: f64, ears: Ears) -> ComponentForm {
    ComponentForm::Tone {
        frequency_hz: finite(hz),
        ears,
        peak_dbfs: finite(-20.0),
        modulation: None,
    }
}
#[expect(
    clippy::unwrap_used,
    reason = "Fixture identities must fit the finite test component list"
)]
fn sound(components: Vec<ComponentForm>) -> Sound {
    Sound::new(
        finite(60.0),
        finite(0.1),
        components
            .into_iter()
            .enumerate()
            .map(|(index, voice)| auditorylab_core::Component {
                id: auditorylab_core::ComponentId::numbered(index).unwrap(),
                form: voice,
            })
            .collect(),
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
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
        let a = nominal(&sound(vec![tone(hz, Ears::Both)]));
        assert!(a.no_rhythm());
        assert!(a.bands.is_empty());
    }
    let mut c = tone(1_000.0, Ears::Both);
    c.set_modulation(Some(Modulation::Am {
        rate_hz: finite(40.0),
        depth: finite(1.0),
    }));
    let a = nominal(&sound(vec![c]));
    assert_eq!(a.bands, [Band::Gamma]);
    c.set(
        auditorylab_core::ParameterId::ToneFrequency,
        finite(100_000.0),
    );
    assert_eq!(nominal(&sound(vec![c])), a);
    c.set_modulation(Some(Modulation::Am {
        rate_hz: finite(40.0),
        depth: finite(0.0),
    }));
    assert!(nominal(&sound(vec![c])).no_rhythm());
    c.set_modulation(Some(Modulation::Am {
        rate_hz: finite(40.0),
        depth: finite(f64::from_bits(1)),
    }));
    assert_eq!(nominal(&sound(vec![c])).bands, [Band::Gamma]);
    c.set(auditorylab_core::ParameterId::PeakLevel, finite(-10_000.0));
    assert_eq!(nominal(&sound(vec![c])).bands, [Band::Gamma]);
}
#[test]
fn independent_processes_deduplicate_bands_but_never_rates() {
    let click = |hz| {
        let base = tone(240.0, Ears::Both);
        ComponentForm::Clicks {
            rate_hz: hz,
            width_seconds: finite(0.001),
            ears: base.route().unwrap(),
            peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
            modulation: base.modulation(),
        }
    };
    let a = nominal(&sound(vec![click(finite(2.0)), click(finite(3.0))]));
    assert_eq!(a.rhythms.len(), 2);
    assert_eq!(a.bands, [Band::Delta]);
    assert!(a.multiple());
    let a = nominal(&sound(vec![click(finite(6.0)), click(finite(40.0))]));
    assert_eq!(a.bands, [Band::Theta, Band::Gamma]);
    assert_eq!(a.matching_rate(&[Band::Gamma], &[]), Some(40.0));
    for hz in [0.05, 100.0] {
        let a = nominal(&sound(vec![click(finite(hz))]));
        assert!(a.outside_bands());
        assert!(!a.no_rhythm());
    }
    for hz in [-40.0, 0.0, f64::INFINITY, f64::NAN] {
        if !hz.is_finite() {
            assert!(auditorylab_core::Finite::new(hz).is_none());
            continue;
        }
        let a = nominal(&sound(vec![click(finite(hz))]));
        assert_eq!(a.completeness, Completeness::Invalid);
        assert!(a.rhythms.is_empty());
        assert!(!a.no_rhythm());
    }
}
#[test]
fn plain_tones_never_infer_a_pair_or_a_rhythmic_band() {
    for voices in [
        vec![tone(200.0, Ears::Left), tone(206.0, Ears::Right)],
        vec![tone(200.0, Ears::Both), tone(206.0, Ears::Both)],
        vec![
            tone(200.0, Ears::Left),
            tone(206.0, Ears::Right),
            tone(300.0, Ears::Both),
        ],
    ] {
        let analysis = nominal(&sound(voices));
        assert_eq!(analysis.methods, [Method::SteadyTone]);
        assert_eq!(analysis.completeness, Completeness::Complete);
        assert!(analysis.no_rhythm());
        assert!(analysis.bands.is_empty());
    }
}

#[test]
fn correlated_facets_cannot_borrow_rates_from_another_process() {
    // The descriptor oracle intentionally constructs a mixed analysis independently
    // of the primitive analyzer, which cannot attribute explicit pairs in mixtures.
    let a = Analysis {
        rhythms: vec![
            Descriptor {
                owner: auditorylab_core::ComponentId::new(1).unwrap().into(),
                role: auditorylab_core::SlotRole::Continuing,
                method: Method::Binaural,
                hz: 6.0,
                band: Some(Band::Theta),
            },
            Descriptor {
                owner: auditorylab_core::ComponentId::new(2).unwrap().into(),
                role: auditorylab_core::SlotRole::Continuing,
                method: Method::Am,
                hz: 40.0,
                band: Some(Band::Gamma),
            },
        ],
        methods: vec![Method::Binaural, Method::Am],
        bands: vec![Band::Theta, Band::Gamma],
        completeness: Completeness::Complete,
        diagnostics: vec![],
    };
    assert!(!a.matches(&[Band::Gamma], &[Method::Binaural]));
    assert!(a.matches(&[Band::Gamma], &[Method::Am]));
    assert!(!a.matches(&[Band::Gamma], &[Method::SteadyTone]));
    assert!(a.matches(&[Band::Theta, Band::Gamma], &[Method::Binaural]));
    assert_eq!(a.matching_rate(&[Band::Gamma], &[]), Some(40.0));
}

#[test]
fn actual_authored_pair_mixtures_keep_nominal_intent_and_correlated_facets() {
    use auditorylab_core::{Component, ComponentForm, ComponentId};
    let pair = |id, offset_hz| Component {
        id: ComponentId::new(id).unwrap(),
        form: ComponentForm::BinauralPair {
            left_hz: finite(200.0),
            offset_hz,
            left_peak_dbfs: finite(-30.0),
            right_peak_dbfs: finite(-30.0),
        },
    };
    let am = Component {
        id: ComponentId::new(90).unwrap(),
        form: {
            let mut form = tone(1000.0, Ears::Both);
            form.set_modulation(Some(Modulation::Am {
                rate_hz: finite(40.0),
                depth: finite(1.0),
            }));
            form
        },
    };
    let a = nominal(&authored(&[pair(7, finite(6.0)), am]));
    assert_eq!(a.bands, [Band::Theta, Band::Gamma]);
    assert_eq!(a.completeness, Completeness::Complete);
    assert_eq!(
        a.rhythms.iter().map(|r| r.owner.get()).collect::<Vec<_>>(),
        [7, 90]
    );
    assert!(!a.matches(&[Band::Gamma], &[Method::Binaural]));
    assert!(a.matches(&[Band::Gamma], &[Method::Am]));
    assert_eq!(
        nominal(&authored(&[pair(1, finite(2.0)), pair(2, finite(3.0))])).bands,
        [Band::Delta]
    );
    assert!(nominal(&authored(&[pair(1, finite(2.0)), pair(2, finite(3.0))])).multiple());
    for offset in [-6.0, 6.0] {
        assert_eq!(
            nominal(&authored(&[pair(1, finite(offset))])).bands,
            [Band::Theta]
        );
    }
    assert!(nominal(&authored(&[pair(1, finite(0.0))])).no_rhythm());
    let collapsed = nominal(&authored(&[pair(1, finite(f64::from_bits(1)))]));
    assert_eq!(collapsed.rhythms[0].hz, f64::from_bits(1));
    assert!(collapsed.outside_bands());
    assert_eq!(collapsed.completeness, Completeness::Invalid);
    let mut raw = vec![pair(1, finite(6.0)), am];
    raw.extend([
        Component {
            id: ComponentId::new(5).unwrap(),
            form: tone(200.0, Ears::Left),
        },
        Component {
            id: ComponentId::new(6).unwrap(),
            form: tone(206.0, Ears::Right),
        },
    ]);
    let a = nominal(&authored(&raw));
    assert_eq!(a.completeness, Completeness::Complete);
    assert_eq!(
        a.rhythms.len(),
        2,
        "raw tones inside a larger mix cannot add inferred processes"
    );
}

#[test]
fn catalog_bands_and_methods_match_independently_listed_expectations() {
    use Band::{Alpha, Beta, Delta, Gamma, Theta};
    use Method::{Am, Binaural, Clicks, Gate, Monaural, SteadyTone};
    use auditorylab_core::{Recipe, SampleRate, catalog};
    let expected = [
        ("beauchene-2016-binaural-5-hz", Binaural, Some(Theta)),
        ("beauchene-2016-binaural-10-hz", Binaural, Some(Alpha)),
        ("beauchene-2016-binaural-15-hz", Binaural, Some(Beta)),
        ("beauchene-2016-pure-tone-240-hz", SteadyTone, None),
        ("chan-2022-clicks-40-hz", Clicks, Some(Gamma)),
        ("ioannou-2015-binaural-2-hz", Binaural, Some(Delta)),
        ("ioannou-2015-binaural-6-hz", Binaural, Some(Theta)),
        ("ioannou-2015-binaural-10-hz", Binaural, Some(Alpha)),
        ("ioannou-2015-binaural-20-hz", Binaural, Some(Beta)),
        ("ioannou-2015-binaural-40-hz", Binaural, Some(Gamma)),
        ("ioannou-2015-pure-tone-200-hz", SteadyTone, None),
        ("jirakittayakorn-2017-binaural-6-hz", Binaural, Some(Theta)),
        ("lahijanian-2024-tone-pips-40-hz", Gate, Some(Gamma)),
        ("martorell-2019-tone-pips-8-hz", Gate, Some(Alpha)),
        ("martorell-2019-tone-pips-40-hz", Gate, Some(Gamma)),
        ("martorell-2019-tone-pips-80-hz", Gate, Some(Gamma)),
        ("orozco-perez-2020-binaural-7-hz", Binaural, Some(Theta)),
        ("orozco-perez-2020-binaural-40-hz", Binaural, Some(Gamma)),
        ("orozco-perez-2020-monaural-7-hz", Monaural, Some(Theta)),
        ("orozco-perez-2020-monaural-40-hz", Monaural, Some(Gamma)),
        ("van-eeckhoutte-2018-am-500-hz", Am, Some(Gamma)),
        ("van-eeckhoutte-2018-am-1000-hz", Am, Some(Gamma)),
        ("van-eeckhoutte-2018-am-2000-hz", Am, Some(Gamma)),
    ];
    assert_eq!(catalog::ENTRIES.len(), expected.len());
    for (id, method, band) in expected {
        let entry = catalog::find(id).unwrap();
        let recipe = Recipe::parse(entry.text).unwrap();
        let analysis = nominal(&authored(recipe.sound().components()));
        assert_eq!(analysis.methods, [method], "{id}");
        assert_eq!(analysis.bands, band.into_iter().collect::<Vec<_>>(), "{id}");
        for hz in [44_100, 48_000] {
            let target = recipe.sound().target(SampleRate::new(hz).unwrap()).unwrap();
            let actual = auditorylab_core::rhythm::realized(&target);
            assert_eq!(actual.methods(), [method], "{id} at {hz}");
            assert_eq!(
                actual
                    .rhythms
                    .iter()
                    .filter_map(|r| r.band)
                    .collect::<Vec<_>>(),
                analysis.bands,
                "{id} at {hz}"
            );
        }
    }
}

const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

fn authored(components: &[auditorylab_core::Component]) -> Sound {
    Sound::new(finite(60.0), finite(0.1), components.to_vec())
        .unwrap_or_else(|error| panic!("invalid authored fixture: {error}"))
}
