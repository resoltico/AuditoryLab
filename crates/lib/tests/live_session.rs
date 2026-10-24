//! Public-library live generation without an audio device.
use auditorylab::{
    Carrier, CarrierKind, Component, ComponentId, Draft, Ears, LiveGenerator, LiveUpdateError,
    ParameterId, SampleRate, Structure, Target,
};

fn component(frequency_hz: f64, ears: Ears) -> Component {
    Component {
        ears,
        peak_dbfs: -30.0,
        carrier: Carrier::Tone { frequency_hz },
        modulation: None,
    }
}

fn target(draft: &Draft) -> Target {
    draft
        .target_check(SampleRate::DEFAULT)
        .into_result()
        .unwrap_or_else(|e| panic!("{e}"))
}

#[expect(
    clippy::unwrap_used,
    reason = "Fixture helper panics when the generated frame is not an interaural pair, failing the calling test"
)]
fn difference(generator: &LiveGenerator) -> f64 {
    let (left, right) = Structure::of_observation(&generator.observation().unwrap())
        .interaural()
        .unwrap();
    (left - right).abs()
}

#[test]
fn generated_quantities_follow_accepted_definitions_and_metadata_does_not_retarget() {
    let mut draft = Draft::new();
    let left = draft.add(CarrierKind::Tone).unwrap();
    let right = draft.add(CarrierKind::Tone).unwrap();
    draft.replace(left, component(380.0, Ears::Left));
    draft.replace(right, component(420.0, Ears::Right));
    let mut generator = LiveGenerator::new(target(&draft));
    let control = generator.control();
    assert_eq!(control.update(&target(&draft)).unwrap(), 1);
    let mut out = [[0.0; 2]; 2000];
    generator.render(&mut out);
    assert_eq!(difference(&generator), 40.0);
    draft.set_title("An edited research copy".into());
    draft.set_summary("My notes".into());
    assert_eq!(
        control.update(&target(&draft)).unwrap(),
        1,
        "metadata is not part of the target"
    );
    draft.replace(right, component(428.0, Ears::Right));
    assert_eq!(control.update(&target(&draft)).unwrap(), 2);
    generator.render(&mut [[0.0; 2]; 720]);
    let mid = difference(&generator);
    assert!(mid > 40.0 && mid < 48.0);
    draft.replace(right, component(439.0, Ears::Right));
    assert_eq!(control.update(&target(&draft)).unwrap(), 3);
    draft.replace(right, component(448.0, Ears::Right));
    assert_eq!(control.update(&target(&draft)).unwrap(), 4);
    generator.render(&mut [[0.0; 2]; 4000]);
    let settled = generator.observation().unwrap();
    assert_eq!((settled.active_revision, settled.settled_revision), (4, 4));
    assert_eq!(difference(&generator), 68.0);
    // An invalid edit never becomes a target, so nothing changes.
    draft.replace(right, component(f64::NAN, Ears::Right));
    assert!(draft.target_check(SampleRate::DEFAULT).target().is_none());
    assert_eq!(generator.observation().unwrap(), settled);
    generator.stop();
    generator.render(&mut out);
    assert!(generator.is_finished());
    assert!(out[1440..].iter().all(|f| *f == [0.0; 2]));
    assert_eq!(
        control.update(&target(&Draft::new())),
        Err(LiveUpdateError::SessionClosed)
    );
}

#[test]
fn generated_interaural_structure_ignores_titles_and_rejects_other_mixtures() {
    for (text, expected) in [
        ("", false),
        (
            "[[component]]\nears='both'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=380\n[[component]]\nears='both'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=420\n",
            false,
        ),
        (
            "[[component]]\nears='left'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=380\n[[component]]\nears='right'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=420\n",
            true,
        ),
    ] {
        let snapshot = auditorylab::load_reader(
            format!("schema_version=3\nduration_seconds=1\nfade_seconds=0\n{text}").as_bytes(),
            "called binaural even if not a pair",
        )
        .unwrap();
        let sound = snapshot.at(SampleRate::DEFAULT).unwrap();
        let observation = sound.signal().observation(0).unwrap();
        assert_eq!(
            Structure::of_observation(&observation)
                .interaural()
                .is_some(),
            expected
        );
    }
}

#[test]
fn generated_routing_text_reports_intermediate_gains_not_the_requested_endpoint() {
    use auditorylab::{Observation, SlotRole, VoiceObservation, generated_description};
    let check = auditorylab::TargetCheck::new(
        SampleRate::DEFAULT,
        &[(ComponentId::new(1), component(440.0, Ears::Right))],
    );
    let facts = *check.target().unwrap().components().next().unwrap().1;
    let mut voices = [None; Observation::MAX_VOICES];
    voices[0] = Some(VoiceObservation {
        id: ComponentId::new(1),
        role: SlotRole::Continuing,
        facts,
        gains: [0.05, 0.05],
    });
    let observation = Observation {
        frame: 100,
        active_revision: 2,
        settled_revision: 1,
        transitioning: true,
        voices,
    };
    let text = generated_description(Some(&observation));
    assert!(text.contains("Both ears"), "{text}");
    assert!(auditorylab::generated_details(&observation).contains("gain L -26.0 / R -26.0 dBFS"));
    assert!(
        auditorylab::generated_details(&observation).contains("Continuing"),
        "role label, not Debug"
    );
    assert!(text.starts_with("Changing"));
    assert_eq!(ParameterId::ClickWidth.descriptor().label, "Click width");
}
