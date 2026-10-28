//! Native live generation through the production adapter without an audio device.
use crate as auditorylab_app;
use crate::audio::session::{LiveGenerator, LiveUpdateError};
use crate::test_recipes as common;
use crate::workspace::controls;
use auditorylab_app::core::{CarrierKind, ComponentForm, Ears, ParameterId, SampleRate, Target};
use auditorylab_app::document::editing::Draft;

fn component(frequency_hz: f64, ears: Ears) -> ComponentForm {
    ComponentForm::Tone {
        frequency_hz: finite(frequency_hz),
        ears,
        peak_dbfs: finite(-30.0),
        modulation: None,
    }
}

fn target(draft: &Draft) -> Target {
    draft
        .check(SampleRate::DEFAULT)
        .into_result()
        .unwrap_or_else(|e| panic!("{e}"))
}

fn difference(generator: &LiveGenerator) -> f64 {
    let rates: Vec<_> = generator
        .observation()
        .unwrap_or_else(|| panic!("rendered fixture must have an observation"))
        .voices
        .into_iter()
        .flatten()
        .filter_map(|voice| match voice.facts.carrier {
            auditorylab_app::core::CarrierFacts::Tone(rate) => Some(rate.realized_hz),
            auditorylab_app::core::CarrierFacts::Clicks(_) => None,
        })
        .collect();
    assert_eq!(rates.len(), 2);
    (rates[0] - rates[1]).abs()
}

#[test]
fn generated_quantities_follow_accepted_definitions_and_metadata_does_not_retarget() {
    let mut draft = Draft::new().unwrap();
    let left = draft
        .add_node(controls::default_component(CarrierKind::Tone))
        .unwrap();
    let right = draft
        .add_node(controls::default_component(CarrierKind::Tone))
        .unwrap();
    draft.replace(left, component(380.0, Ears::Left)).unwrap();
    draft.replace(right, component(420.0, Ears::Right)).unwrap();
    let mut generator = LiveGenerator::new(&(target(&draft)));
    let control = generator.control();
    assert_eq!(control.update(&target(&draft)).unwrap(), 1);
    let mut out = std::vec![[0.0; 2]; 2000];
    generator.render(&mut out);
    assert_eq!(difference(&generator), 40.0);
    draft.set_title("An edited research copy".into());
    draft.set_summary("My notes".into());
    assert_eq!(
        control.update(&target(&draft)).unwrap(),
        1,
        "metadata is not part of the target"
    );
    draft.replace(right, component(428.0, Ears::Right)).unwrap();
    assert_eq!(control.update(&target(&draft)).unwrap(), 2);
    generator.render(&mut [[0.0; 2]; 720]);
    let mid = difference(&generator);
    assert!(mid > 40.0 && mid < 48.0);
    draft.replace(right, component(439.0, Ears::Right)).unwrap();
    assert_eq!(control.update(&target(&draft)).unwrap(), 3);
    draft.replace(right, component(448.0, Ears::Right)).unwrap();
    assert_eq!(control.update(&target(&draft)).unwrap(), 4);
    generator.render(&mut std::vec![[0.0; 2]; 4000]);
    let settled = generator.observation().unwrap();
    assert_eq!((settled.active_revision, settled.settled_revision), (4, 4));
    assert_eq!(difference(&generator), 68.0);
    // An invalid edit never becomes a target, so nothing changes.
    draft
        .replace(right, component(30_000.0, Ears::Right))
        .unwrap();
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
            false,
        ),
    ] {
        let mut recipe = common::silent(1.0, 0.0).into_builder();
        recipe.content.sound = auditorylab_core::Sound::new(
            recipe.content.sound.duration_seconds(),
            recipe.content.sound.fade_seconds(),
            voices
                .into_iter()
                .enumerate()
                .map(|(index, voice)| auditorylab_app::core::Component {
                    id: auditorylab_app::core::ComponentId::numbered(index).unwrap(),
                    form: voice,
                })
                .collect(),
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
        let snapshot = auditorylab_app::services::read_source(
            recipe.build().unwrap().to_toml().unwrap().as_bytes(),
            "called binaural even if not a pair",
        )
        .unwrap();
        let sound = snapshot.at(SampleRate::DEFAULT).unwrap();
        let observation = sound.signal().observation(0).unwrap();
        assert_eq!(
            auditorylab_app::core::rhythm::realized(&observation)
                .interaural()
                .is_some(),
            expected
        );
    }
}

#[test]
fn first_class_pair_updates_both_lanes_atomically_and_invalid_pairs_never_retarget() {
    use auditorylab_app::core::{Lane, PairKind};
    use auditorylab_app::document::editing::{EditingField, EditorSession};
    let mut editor = EditorSession::new(Draft::new().unwrap()).unwrap();
    let owner = editor
        .add_component(
            editor.document_key(),
            controls::default_pair(PairKind::Binaural),
        )
        .unwrap();
    let mut generator = LiveGenerator::new(&(target(editor.draft())));
    let control = generator.control();
    control.update(&target(editor.draft())).unwrap();
    generator.render(&mut std::vec![[0.0; 2]; 2000]);
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
        generator.render(&mut std::vec![[0.0; 2]; 1440]);
        let observed = generator.observation().unwrap();
        let voices: Vec<_> = observed.voices.iter().flatten().collect();
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0].id, owner.into());
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
            auditorylab_app::document::editing::MetadataField::Title,
        ))
        .unwrap();
    editor
        .commit_field(title, "Metadata only")
        .unwrap()
        .unwrap();
    assert_eq!(control.update(&target(editor.draft())).unwrap(), revision);
    generator.stop();
    let mut output = std::vec![[1.0; 2]; 4000];
    generator.render(&mut output);
    assert!(generator.is_finished());
    assert!(output[2880..].iter().all(|frame| *frame == [0.0; 2]));
}

#[cfg(test)]
const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}
