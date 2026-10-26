//! Public-library live generation without an audio device.
mod common;
use auditorylab::core::interpretation::Structure;
use auditorylab::core::{
    Carrier, CarrierKind, ComponentId, Ears, ParameterId, SampleRate, Target, Voice,
};
use auditorylab::editing::Draft;
use auditorylab::presentation::{AudioDetails, AudioSummary};
use auditorylab::session::{LiveGenerator, LiveUpdateError};

fn component(frequency_hz: f64, ears: Ears) -> Voice {
    Voice {
        ears,
        peak_dbfs: -30.0,
        carrier: Carrier::Tone { frequency_hz },
        modulation: None,
    }
}

fn target(draft: &Draft) -> Target {
    draft
        .check(SampleRate::DEFAULT)
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
    let mut draft = Draft::new().unwrap();
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
    assert!(draft.check(SampleRate::DEFAULT).target().is_none());
    assert_eq!(generator.observation().unwrap(), settled);
    generator.stop();
    generator.render(&mut out);
    assert!(generator.is_finished());
    assert!(out[1440..].iter().all(|f| *f == [0.0; 2]));
    assert_eq!(
        control.update(&target(&Draft::new().unwrap())),
        Err(LiveUpdateError::SessionClosed)
    );
}

#[test]
fn generated_interaural_structure_ignores_titles_and_rejects_other_mixtures() {
    for (voices, expected) in [
        (vec![], false),
        (
            vec![component(380.0, Ears::Both), component(420.0, Ears::Both)],
            false,
        ),
        (
            vec![component(380.0, Ears::Left), component(420.0, Ears::Right)],
            true,
        ),
    ] {
        let mut recipe = common::silent(1.0, 0.0);
        recipe.sound.components = voices
            .into_iter()
            .enumerate()
            .map(|(index, voice)| auditorylab::core::Component {
                id: auditorylab::core::ComponentId::numbered(index).unwrap(),
                form: auditorylab::core::ComponentForm::Voice(voice),
            })
            .collect();
        let snapshot = auditorylab::recipe::load_reader(
            recipe.to_toml().unwrap().as_bytes(),
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
    use auditorylab::core::{Observation, SlotRole, VoiceObservation};

    let check = auditorylab_core::Sound {
        duration_seconds: 1.0,
        fade_seconds: 0.0,
        components: ([(ComponentId::new(1), component(440.0, Ears::Right))])
            .iter()
            .map(|(id, voice)| auditorylab_core::Component {
                id: *id,
                form: auditorylab_core::ComponentForm::Voice(*voice),
            })
            .collect(),
    }
    .check(SampleRate::DEFAULT);
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
    let text = (Some(&observation)).summary();
    assert!(text.contains("Both ears"), "{text}");
    assert!(
        observation
            .details()
            .contains("gain L -26.0 / R -26.0 dBFS")
    );
    assert!(
        observation.details().contains("Continuing"),
        "role label, not Debug"
    );
    assert!(text.starts_with("Changing"));
    assert_eq!(ParameterId::ClickWidth.descriptor().label, "Clicks width");
}

#[test]
fn first_class_pair_updates_both_lanes_atomically_and_invalid_pairs_never_retarget() {
    use auditorylab::core::{Lane, PairKind};
    use auditorylab::editing::{EditingField, EditorSession};
    let mut editor = EditorSession::new(Draft::new().unwrap()).unwrap();
    let owner = editor
        .add_pair(editor.document_key(), PairKind::Binaural)
        .unwrap();
    let mut generator = LiveGenerator::new(target(editor.draft()));
    let control = generator.control();
    control.update(&target(editor.draft())).unwrap();
    generator.render(&mut [[0.0; 2]; 2000]);
    let commit = |editor: &mut EditorSession, parameter, value: &str| {
        let handle = editor
            .handle(EditingField::Parameter(owner, parameter))
            .unwrap();
        editor.commit_field(handle, value).unwrap().unwrap();
    };
    for (first, offset) in [(300.0, 6.0), (400.0, -2.0), (200.0, 40.0)] {
        let first_handle = editor
            .handle(EditingField::Parameter(
                owner,
                ParameterId::PairFirstFrequency,
            ))
            .unwrap();
        let offset_handle = editor
            .handle(EditingField::Parameter(owner, ParameterId::PairOffset))
            .unwrap();
        editor.stage(first_handle, &first.to_string()).unwrap();
        editor.stage(offset_handle, &offset.to_string()).unwrap();
        editor.commit().unwrap();
        control.update(&target(editor.draft())).unwrap();
        generator.render(&mut [[0.0; 2]; 1440]);
        let observed = generator.observation().unwrap();
        let voices: Vec<_> = observed.voices.iter().flatten().collect();
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].id, owner);
        assert_eq!(voices[1].id, owner.with_lane(Lane::Secondary));
        assert_eq!(voices[0].facts.pair, voices[1].facts.pair);
        assert_eq!(voices[0].facts.pair.unwrap().offset_hz, offset);
    }
    let before = generator.observation().unwrap();
    commit(&mut editor, ParameterId::PairOffset, "-200");
    assert!(editor.draft().check(SampleRate::DEFAULT).target().is_none());
    assert_eq!(generator.observation().unwrap(), before);
    commit(&mut editor, ParameterId::PairOffset, "2");
    let revision = control.update(&target(editor.draft())).unwrap();
    let title = editor
        .handle(EditingField::Metadata(
            auditorylab::editing::MetadataField::Title,
        ))
        .unwrap();
    editor
        .commit_field(title, "Metadata only")
        .unwrap()
        .unwrap();
    assert_eq!(control.update(&target(editor.draft())).unwrap(), revision);
    generator.stop();
    let mut output = [[1.0; 2]; 4000];
    generator.render(&mut output);
    assert!(generator.is_finished());
    assert!(output[2880..].iter().all(|frame| *frame == [0.0; 2]));
}
