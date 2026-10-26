//! Accepted-definition revisions in the live engine: requested-only edits
//! change facts and revisions, never generated samples, ramps or phases.
//! Expected samples come from engines that never received those edits.
use auditorylab_core::{
    Carrier, CarrierFacts, Component, ComponentForm, ComponentId, Ears, Observation, SampleRate,
    SlotRole, Sound, Target, Voice, live::Engine,
};

const fn rate() -> SampleRate {
    match SampleRate::new(8_000) {
        Ok(rate) => rate,
        Err(_) => panic!("8 kHz is a supported rate"),
    }
}

fn clicks(rate_hz: f64, width_seconds: f64) -> Voice {
    Voice {
        ears: Ears::Both,
        peak_dbfs: -20.0,
        carrier: Carrier::Clicks {
            rate_hz,
            width_seconds,
        },
        modulation: None,
    }
}

#[expect(
    clippy::unwrap_used,
    reason = "Fixture helper panics if its fixed component stops validating, failing the calling test"
)]
fn target(component: Voice) -> Target {
    Sound {
        duration_seconds: 1.0,
        fade_seconds: 0.0,
        components: vec![Component {
            id: ComponentId::new(1),
            form: ComponentForm::Voice(component),
        }],
    }
    .target(rate())
    .unwrap()
}

/// A at 40 Hz; B at 80 Hz; C a 120 Hz tone. The primed variants request
/// 0.11 ms instead of 0.10 ms; both realize one frame at 8 kHz.
fn a() -> Target {
    target(clicks(40.0, 0.000_10))
}
fn a_prime() -> Target {
    target(clicks(40.0, 0.000_11))
}
fn b() -> Target {
    target(clicks(80.0, 0.000_10))
}
fn b_prime() -> Target {
    target(clicks(80.0, 0.000_11))
}
fn c() -> Target {
    target(Voice {
        carrier: Carrier::Tone {
            frequency_hz: 120.0,
        },
        ..clicks(1.0, 0.1)
    })
}

fn render(engine: &mut Engine, frames: usize) -> Vec<[f64; 2]> {
    let mut out = vec![[9.0; 2]; frames];
    engine.render(&mut out);
    out
}

fn requested_width(voice: &auditorylab_core::VoiceObservation) -> f64 {
    match voice.facts.carrier {
        CarrierFacts::Clicks(p) => p.requested_width_seconds,
        CarrierFacts::Tone(_) => panic!("click facts expected"),
    }
}

fn voices(observation: &Observation) -> Vec<auditorylab_core::VoiceObservation> {
    observation.voices.iter().flatten().copied().collect()
}

#[test]
fn a_settled_facts_only_revision_changes_no_sample_and_is_active_and_settled() {
    let mut edited = Engine::new(rate());
    let mut reference = Engine::new(rate());
    assert!(edited.set_revision(&a(), 1));
    assert!(reference.set_revision(&a(), 1));
    assert_eq!(render(&mut edited, 1_000), render(&mut reference, 1_000));
    let before = edited.observation().unwrap();
    assert!(edited.set_revision(&a_prime(), 2));
    // Empty renders publish no generated frame and rewrite nothing.
    edited.render(&mut []);
    assert_eq!(edited.observation(), Some(before));
    assert_eq!(requested_width(&voices(&before)[0]), 0.000_10);
    assert_eq!(render(&mut edited, 777), render(&mut reference, 777));
    let after = edited.observation().unwrap();
    assert_eq!((after.active_revision, after.settled_revision), (2, 2));
    assert!(!after.transitioning, "a facts-only edit starts no ramp");
    assert_eq!(requested_width(&voices(&after)[0]), 0.000_11);
    assert_eq!(after.frame, before.frame + 777);
}

#[test]
fn a_facts_only_revision_clears_an_unrendered_audio_change() {
    let mut edited = Engine::new(rate());
    let mut reference = Engine::new(rate());
    edited.set_revision(&a(), 1);
    reference.set_revision(&a(), 1);
    assert_eq!(render(&mut edited, 600), render(&mut reference, 600));
    assert!(edited.set_revision(&b(), 2));
    assert!(edited.set_revision(&a_prime(), 3));
    // No excursion toward B: every later sample matches the A-only engine.
    assert_eq!(render(&mut edited, 3_000), render(&mut reference, 3_000));
    let observation = edited.observation().unwrap();
    assert_eq!(
        (observation.active_revision, observation.settled_revision),
        (3, 3)
    );
    assert_eq!(requested_width(&voices(&observation)[0]), 0.000_11);
    // Negative control: without the facts-only edit, B is generated.
    let mut control = Engine::new(rate());
    let mut a_only = Engine::new(rate());
    control.set_revision(&a(), 1);
    a_only.set_revision(&a(), 1);
    render(&mut control, 600);
    render(&mut a_only, 600);
    control.set_revision(&b(), 2);
    assert_ne!(render(&mut control, 3_000), render(&mut a_only, 3_000));
}

#[test]
fn a_destination_facts_edit_during_a_ramp_keeps_the_ramp_and_outgoing_facts() {
    let mut edited = Engine::new(rate());
    let mut reference = Engine::new(rate());
    for engine in [&mut edited, &mut reference] {
        engine.set_revision(&a(), 1);
        render(engine, 400);
        engine.set_revision(&b(), 2);
        render(engine, 100);
    }
    let ramp = usize::try_from(edited.ramp_frames()).unwrap();
    assert!(edited.observation().unwrap().transitioning);
    edited.set_revision(&c(), 3);
    edited.set_revision(&b_prime(), 4);
    // The A-to-B ramp keeps its progress and end; C never generates.
    let rest = ramp - 100;
    assert_eq!(
        render(&mut edited, rest - 1),
        render(&mut reference, rest - 1)
    );
    let during = edited.observation().unwrap();
    assert!(during.transitioning);
    assert_eq!((during.active_revision, during.settled_revision), (4, 1));
    let roles: Vec<_> = voices(&during)
        .iter()
        .map(|v| (v.id, v.role, requested_width(v)))
        .collect();
    // The same identity on both sides keeps each side's own facts.
    assert_eq!(
        roles,
        [
            (ComponentId::new(1), SlotRole::Outgoing, 0.000_10),
            (ComponentId::new(1), SlotRole::Incoming, 0.000_11)
        ]
    );
    assert_eq!(render(&mut edited, 2_000), render(&mut reference, 2_000));
    let settled = edited.observation().unwrap();
    assert!(!settled.transitioning);
    assert_eq!((settled.active_revision, settled.settled_revision), (4, 4));
}

#[test]
fn the_exact_destination_after_a_different_waiting_target_keeps_the_ramp() {
    let mut edited = Engine::new(rate());
    let mut reference = Engine::new(rate());
    for engine in [&mut edited, &mut reference] {
        engine.set_revision(&a(), 1);
        render(engine, 400);
        engine.set_revision(&b(), 2);
        render(engine, 50);
    }
    edited.set_revision(&c(), 3);
    assert!(
        edited.set_revision(&b(), 4),
        "B after C is a new accepted revision"
    );
    assert_eq!(render(&mut edited, 3_000), render(&mut reference, 3_000));
    assert_eq!(edited.observation().unwrap().settled_revision, 4);
}

#[test]
fn a_target_matching_the_outgoing_side_is_a_real_return() {
    let mut edited = Engine::new(rate());
    let mut reference = Engine::new(rate());
    for engine in [&mut edited, &mut reference] {
        engine.set_revision(&a(), 1);
        render(engine, 400);
        engine.set_revision(&b(), 2);
        render(engine, 50);
    }
    edited.set_revision(&a_prime(), 3);
    let ramp = usize::try_from(edited.ramp_frames()).unwrap();
    // The current ramp finishes unchanged…
    assert_eq!(
        render(&mut edited, ramp - 50),
        render(&mut reference, ramp - 50)
    );
    assert_eq!(edited.observation().unwrap().settled_revision, 2);
    // …then a new ramp returns toward A′.
    let returning = render(&mut edited, 1);
    assert_ne!(returning, render(&mut reference, 1));
    let observation = edited.observation().unwrap();
    assert!(observation.transitioning);
    assert_eq!(
        (observation.active_revision, observation.settled_revision),
        (3, 2)
    );
    render(&mut edited, ramp);
    let back = edited.observation().unwrap();
    assert_eq!(back.settled_revision, 3);
    match voices(&back)[0].facts.carrier {
        CarrierFacts::Clicks(p) => {
            assert_eq!(p.rate_hz, 40.0);
            assert_eq!(p.requested_width_seconds, 0.000_11);
        }
        CarrierFacts::Tone(_) => panic!("clicks expected"),
    }
}

#[test]
fn stop_dominates_an_accepted_target_that_never_rendered() {
    let mut engine = Engine::new(rate());
    engine.set_revision(&a(), 1);
    render(&mut engine, 500);
    assert!(engine.set_revision(&b(), 2));
    engine.stop();
    assert!(
        !engine.set_revision(&a_prime(), 3),
        "Stop closes the engine to targets"
    );
    render(&mut engine, 2_000);
    assert!(engine.is_finished());
    let last = engine.observation().unwrap();
    assert_eq!(
        last.active_revision, 1,
        "the waiting target never generated"
    );
}
