//! Independent generated-state, attribution and revision oracles.
use auditorylab_core::{
    CarrierFacts, ChannelRouting, Component, ComponentForm, ComponentId, Modulation,
    ModulationFacts, SampleRate, SlotRole, StructureError, Target, VoiceFacts, live::Engine,
};
fn tone(hz: f64, route: ChannelRouting) -> ComponentForm {
    ComponentForm {
        source: auditorylab_core::SoundSource::Tone {
            frequency_hz: finite(hz),
            route,
            peak_dbfs: finite(-30.0),
        },
        modulation: None,
    }
}
#[expect(
    clippy::unwrap_used,
    reason = "Test fixture helper deliberately panics if its specified \
        component/rate becomes invalid, causing the calling observation test \
        to fail"
)]
fn target(rate: SampleRate, component: ComponentForm) -> Target {
    auditorylab_core::test_support::FiniteFixture::new(
        finite(1.0),
        finite(0.0),
        vec![Component {
            id: ComponentId::new(7).unwrap(),
            form: component,
        }],
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
    .target(rate)
    .unwrap()
}
#[expect(
    clippy::unwrap_used,
    reason = "Test fixture helper deliberately panics if its specified \
        component/rate becomes invalid, causing the calling observation test \
        to fail"
)]
fn facts(component: ComponentForm, rate: SampleRate) -> VoiceFacts {
    let check = auditorylab_core::test_support::FiniteFixture::new(
        finite(1.0),
        finite(0.0),
        ([(ComponentId::new(1).unwrap(), component)])
            .iter()
            .map(|(id, voice)| auditorylab_core::Component {
                id: *id,
                form: *voice,
            })
            .collect(),
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
    .check(rate);
    *check.target().unwrap().voices().next().unwrap().1
}
#[test]
fn observations_identify_last_frame_without_advancing_phase() {
    let rate = SampleRate::new(48000).unwrap();
    let mut e = Engine::new(rate);
    assert_eq!(e.observation(), None);
    e.render(&mut []);
    assert_eq!(e.observation(), None);
    e.set_revision(&target(rate, tone(380.0, ChannelRouting::Left)), 10);
    let mut out = std::vec![[0.0; 2]; 1440];
    e.render(&mut out);
    let obs = e.observation().unwrap();
    assert_eq!(obs.frame, 1439);
    assert_eq!(obs.active_revision, 10);
    assert_eq!(obs.settled_revision, 10);
    assert!(!obs.transitioning);
    let clone = e.clone();
    for _ in 0..10 {
        assert_eq!(e.observation(), clone.observation());
    }
    let mut b = e.clone();
    let mut x = [[0.0; 2]; 17];
    let mut y = x;
    e.render(&mut x);
    b.render(&mut y);
    assert_eq!(x, y);
    let before = e.observation();
    e.render(&mut []);
    assert_eq!(e.observation(), before);
}
#[test]
fn glide_reports_the_increment_used_for_the_last_sample() {
    for hz in [44100, 48000] {
        let rate = SampleRate::new(hz).unwrap();
        let mut e = Engine::new(rate);
        e.set_revision(&target(rate, tone(420.0, ChannelRouting::Right)), 1);
        e.render(&mut vec![
            [0.0; 2];
            usize::try_from(e.ramp_frames()).unwrap()
        ]);
        e.set_revision(&target(rate, tone(428.0, ChannelRouting::Right)), 2);
        let half = usize::try_from(e.ramp_frames() / 2).unwrap();
        e.render(&mut vec![[0.0; 2]; half]);
        let obs = e.observation().unwrap();
        let v = obs.voices.iter().flatten().next().unwrap();
        let CarrierFacts::Tone(f) = v.facts.carrier else {
            panic!()
        };
        assert!((f.realized_hz - 424.0).abs() < 0.01);
        assert_eq!(f.requested_hz, 428.0);
        assert!(obs.transitioning);
        assert_eq!(obs.settled_revision, 1);
        assert!(!e.set_revision(&target(rate, tone(999.0, ChannelRouting::Both)), 1));
        e.render(&mut vec![
            [0.0; 2];
            usize::try_from(e.ramp_frames()).unwrap()
        ]);
        assert_eq!(e.observation().unwrap().settled_revision, 2);
    }
}
#[test]
fn changed_pulses_preserve_both_real_rates_and_roles() {
    let rate = SampleRate::new(48000).unwrap();
    let clicks = |hz| {
        let base = tone(440.0, ChannelRouting::Both);
        ComponentForm {
            source: auditorylab_core::SoundSource::PulseTrain {
                route: base.route().unwrap(),
                peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
                pulse: auditorylab_core::PulseSpec {
                    rate_hz: hz,
                    width: auditorylab_core::PulseWidth::Duration {
                        seconds: finite(0.001),
                    },
                    shape: auditorylab_core::PulseShape::Rectangular {},
                },
            },
            modulation: base.modulation(),
        }
    };
    let mut e = Engine::new(rate);
    e.set(&target(rate, clicks(finite(40.0))));
    e.render(&mut std::vec![[0.0; 2]; 2000]);
    e.set(&target(rate, clicks(finite(80.0))));
    e.render(&mut [[0.0; 2]; 1]);
    let obs = e.observation().unwrap();
    let voices: Vec<_> = obs.voices.into_iter().flatten().collect();
    assert_eq!(voices.len(), 2);
    assert_eq!(voices[0].role, SlotRole::Outgoing);
    assert_eq!(voices[1].role, SlotRole::Incoming);
    let rates: Vec<_> = voices
        .iter()
        .map(|v| match v.facts.carrier {
            CarrierFacts::PulseTrain(p) => {
                assert_eq!(p.width_frames, 48);
                p.rate_hz
            }
            CarrierFacts::Tone(_) => panic!(),
        })
        .collect();
    assert_eq!(rates, [40.0, 80.0]);
    assert_eq!(voices[0].id, voices[1].id);
    assert!(matches!(
        auditorylab_core::test_support::FiniteFixture::new(
            finite(1.0),
            finite(0.0),
            vec![
                Component {
                    id: ComponentId::new(1).unwrap(),
                    form: clicks(finite(40.0))
                },
                Component {
                    id: ComponentId::new(1).unwrap(),
                    form: clicks(finite(80.0))
                },
            ]
        ),
        Err(StructureError::Identity { .. })
    ));
}
#[test]
fn latest_revision_wins_and_stop_does_not_claim_the_skipped_target() {
    let rate = SampleRate::new(48000).unwrap();
    let mut e = Engine::new(rate);
    for revision in [1_u32, 2, 3] {
        e.set_revision(
            &target(
                rate,
                tone(400.0 + f64::from(revision), ChannelRouting::Both),
            ),
            u64::from(revision),
        );
        e.render(&mut [[0.0; 2]; 1]);
    }
    e.render(&mut std::vec![[0.0; 2]; 3000]);
    assert_eq!(e.observation().unwrap().settled_revision, 3);
    e.set_revision(&target(rate, tone(800.0, ChannelRouting::Both)), 4);
    e.render(&mut [[0.0; 2]; 1]);
    e.set_revision(&target(rate, tone(900.0, ChannelRouting::Both)), 5);
    e.stop();
    e.render(&mut std::vec![[0.0; 2]; 4000]);
    assert!(e.is_finished());
    assert_eq!(e.observation().unwrap().active_revision, 4);
}
#[test]
#[expect(
    clippy::cast_precision_loss,
    reason = "independent integer-rational phase oracle is converted for presentation comparison"
)]
fn realized_rates_widths_depth_and_tiny_frequencies_are_explicit() {
    for hz in [44100, 48000, 768_000] {
        let rate = SampleRate::new(hz).unwrap();
        let c = {
            let mut form = tone(1000.0, ChannelRouting::Both);
            form.set_modulation(Some(Modulation::Sinusoidal {
                rate_hz: finite(39.0625),
                depth: finite(0.0),
            }));
            form
        };
        let f = facts(c, rate);
        let CarrierFacts::Tone(t) = f.carrier else {
            panic!()
        };
        let step = ((1000_u128 << 64).div_ceil(u128::from(hz))) as f64;
        assert_eq!(
            t.realized_hz,
            step * f64::from(hz) / 18_446_744_073_709_551_616.0
        );
        assert!(matches!(
            f.modulation,
            Some(ModulationFacts::Sinusoidal { depth: 0.0, .. })
        ));
        let CarrierFacts::Tone(tiny) =
            facts(tone(f64::from_bits(1), ChannelRouting::Both), rate).carrier
        else {
            panic!()
        };
        assert!(tiny.realized_hz > 0.0);
        assert_eq!(tiny.requested_hz, f64::from_bits(1));
    }
}
#[test]
fn observe_set_render_stop_remain_allocation_free() {
    let rate = SampleRate::new(48000).unwrap();
    let t = target(rate, tone(440.0, ChannelRouting::Both));
    let mut engine = Engine::new(rate);
    let mut out = [[0.0; 2]; 1024];
    let count = allocation_counter::measure(|| {
        engine.set(&t);
        for _ in 0..4 {
            engine.render(&mut out);
            std::hint::black_box(engine.observation());
        }
        engine.stop();
        engine.render(&mut out);
        std::hint::black_box(engine.observation());
    });
    assert_eq!(count.count_total, 0);
}

#[test]
fn slot_roles_have_their_own_names() {
    assert_eq!(SlotRole::Continuing.label(), "Continuing");
    assert_eq!(SlotRole::Outgoing.label(), "Outgoing");
    assert_eq!(SlotRole::Incoming.label(), "Incoming");
}

const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}
