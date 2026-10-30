//! Exercise learning through production commands, including rejected hidden actions.
use super::*;
use crate::learning::{Example, LearningCommand as L, Topic};

#[test]
fn learning_preserves_staged_invalid_work_and_query_and_rejects_hidden_actions() {
    let mut ws = workspace();
    let (document, field) = document(&mut ws, 321.0);
    ws.apply(Command::Editor(EditorCommand::Stage {
        field,
        text: "invalid".into(),
    }))
    .unwrap();
    let input_revision = ws.editor().input_revision();
    let draft_revision = ws.editor().draft_revision();
    let query = ws.query().to_owned();
    ws.take_effects();
    ws.apply(Command::Learning(L::Open(Topic::Overview)))
        .unwrap();
    ws.apply(Command::Learning(L::Open(Topic::RhythmBands)))
        .unwrap();
    for command in [
        Command::Audio(AudioCommand::Play),
        Command::Audio(AudioCommand::PlaySelected),
        Command::Audio(AudioCommand::Toggle),
        Command::File(FileCommand::Save),
        Command::File(FileCommand::SaveCopy),
        Command::File(FileCommand::ExportRecipe),
        Command::File(FileCommand::ExportWav),
        Command::Library(LibraryCommand::Search("changed".into())),
        Command::Editor(EditorCommand::Commit),
    ] {
        assert!(matches!(ws.apply(command), Err(Refusal::LearningVisible)));
    }
    let actions = ws.action_view();
    assert!(!actions.can_play());
    assert!(!actions.can_save());
    assert!(!actions.can_export());
    assert!(!actions.can_export_recipe());
    assert!(!actions.can_edit());
    assert_eq!(actions.play_target, "");
    assert!(ws.take_effects().is_empty());
    ws.apply(Command::Learning(L::ReturnToWork)).unwrap();
    assert_eq!(ws.view(), View::Workbench);
    assert_eq!(ws.editor().document(), document);
    assert_eq!(ws.editor().input_revision(), input_revision);
    assert_eq!(ws.editor().draft_revision(), draft_revision);
    assert_eq!(ws.query(), query);
    assert!(ws.learning().is_none());
    assert!(matches!(
        ws.take_effects().as_slice(),
        [Effect::LearningDeparted]
    ));
}

#[test]
fn captured_example_survives_changed_topic_cancel_and_stale_repeated_requests() {
    let mut ws = workspace();
    let (original, _) = document(&mut ws, 321.0);
    ws.take_effects();
    ws.apply(Command::Learning(L::Open(Topic::Overview)))
        .unwrap();
    ws.apply(Command::Learning(L::CreateExample(Example::PulsedTone)))
        .unwrap();
    let prompt = take_unsaved_prompt(&mut ws);
    assert!(matches!(
        ws.apply(Command::Learning(L::CreateExample(Example::PulsedTone))),
        Err(Refusal::PromptOpen)
    ));
    ws.apply(Command::Learning(L::Open(Topic::SoundTypes)))
        .unwrap();
    ws.apply(Command::Answer {
        prompt,
        answer: PromptAnswer::Cancel,
    })
    .unwrap();
    assert_eq!(ws.editor().document(), original);
    assert!(ws.learning().is_some());
    ws.apply(Command::Learning(L::CreateExample(Example::PulsedTone)))
        .unwrap();
    let next = take_unsaved_prompt(&mut ws);
    ws.apply(Command::Answer {
        prompt,
        answer: PromptAnswer::Discard,
    })
    .unwrap();
    assert_eq!(ws.editor().document(), original);
    ws.apply(Command::Answer {
        prompt: next,
        answer: PromptAnswer::Discard,
    })
    .unwrap();
    let installed = ws.editor().document();
    assert_ne!(installed, original);
    assert!(ws.learning().is_none());
    assert_eq!(ws.view(), View::Workbench);
    assert!(!ws.audio_busy());
    assert!(!ws.file_busy());
    let draft = ws.editor().draft();
    assert!(draft.based_on().is_none());
    assert_eq!(draft.duration_seconds(), 60.0);
    assert_eq!(draft.fade_seconds(), 0.1);
    assert_eq!(draft.components().len(), 1);
    let form = draft.components()[0].form;
    let crate::core::SoundSource::Tone {
        frequency_hz,
        peak_dbfs,
        route,
    } = form.source
    else {
        panic!("teaching example must retain a tone carrier");
    };
    assert_eq!(frequency_hz.get(), 440.0);
    assert_eq!(peak_dbfs.get(), -30.0);
    assert_eq!(route, crate::core::ChannelRouting::Both);
    let Some(crate::core::Modulation::Pulsed { pulse }) = form.modulation else {
        panic!("teaching example must retain actual pulse modulation");
    };
    assert_eq!(pulse.rate_hz.get(), 10.0);
    let crate::core::PulseWidth::DutyCycle { fraction } = pulse.width else {
        panic!("teaching example preserves duty intent");
    };
    assert_eq!(fraction.get(), 0.5);
    let crate::core::PulseShape::RaisedCosine {
        rise_seconds,
        fall_seconds,
    } = pulse.shape
    else {
        panic!("teaching example preserves shaped edges");
    };
    assert_eq!(rise_seconds.get(), 0.002);
    assert_eq!(fall_seconds.get(), 0.002);
    ws.apply(Command::Answer {
        prompt: next,
        answer: PromptAnswer::Discard,
    })
    .unwrap();
    assert_eq!(ws.editor().document(), installed);
    let effects = ws.take_effects();
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::LearningDeparted))
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::Run(_)))
    );
}

#[test]
fn explicit_browse_preserves_query_and_new_sound_stays_empty() {
    let mut ws = workspace();
    ws.apply(Command::Library(LibraryCommand::Search("alpha".into())))
        .unwrap();
    let selected = ws.selected().cloned();
    ws.apply(Command::Learning(L::Open(Topic::Library)))
        .unwrap();
    ws.apply(Command::Learning(L::BrowseLibrary)).unwrap();
    assert_eq!(ws.query(), "alpha");
    assert_eq!(
        ws.selected().map(RecipeSnapshot::title),
        selected.as_ref().map(RecipeSnapshot::title)
    );
    ws.take_effects();
    ws.apply(Command::Learning(L::Open(Topic::Overview)))
        .unwrap();
    ws.apply(Command::Editor(EditorCommand::New)).unwrap();
    assert_eq!(ws.editor().draft().components(), []);
    assert!(!ws.audio_busy());
    assert!(ws.learning().is_none());
}

#[test]
fn learning_navigation_retains_actual_audio_and_stop_authority() {
    let mut ws = workspace();
    document(&mut ws, 440.0);
    ws.apply(Command::Audio(AudioCommand::Play)).unwrap();
    let effects = ws.take_effects();
    let audio = effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::Run(job) => Some(job),
            _ => None,
        })
        .unwrap();
    let task = ws.audio.task().unwrap().id;
    ws.apply(Command::Learning(L::Open(Topic::Overview)))
        .unwrap();
    ws.apply(Command::Learning(L::Open(Topic::PitchAndRhythm)))
        .unwrap();
    assert_eq!(ws.audio.task().unwrap().id, task);
    assert!(ws.take_effects().is_empty());
    assert!(ws.action_view().transport_enabled());
    ws.apply(Command::Audio(AudioCommand::Stop)).unwrap();
    assert_eq!(ws.audio.task().unwrap().id, task);
    assert!(ws.audio_busy(), "Stop must wait for the actual worker join");
    assert!(ws.learning().is_some());
    drop(audio); // Deliberately unexecuted: this is admission evidence, no device claim.
}

#[test]
fn learning_navigation_keeps_export_capture_and_cancel_targets_actual_job() {
    let mut ws =
        TestWorkspace::new(vec![short_silent_source()], Capabilities::available()).unwrap();
    let directory = temporary_directory();
    let path = directory.path().join("cancelled-learning.wav");
    ws.apply(Command::File(FileCommand::ExportWav)).unwrap();
    answer_path(&mut ws, PathPurpose::ExportWav, Some(path.clone()));
    let job = take_file_job(&mut ws);
    let task = job.id();
    ws.apply(Command::Learning(L::Open(Topic::Overview)))
        .unwrap();
    assert_eq!(ws.files.task().unwrap().id, task);
    assert!(ws.take_effects().is_empty());
    assert!(ws.action_view().exporting);
    ws.apply(Command::File(FileCommand::CancelExport)).unwrap();
    assert_eq!(ws.respond(execute_file(job)), ResponseDisposition::Applied);
    assert!(!path.exists());
    assert!(ws.learning().is_some());
    assert!(matches!(
        ws.files.status(),
        FileStatus::Notice(FileNotice::ExportCancelled)
    ));
}

#[test]
fn example_save_continuation_uses_real_capture_and_failure_keeps_original() {
    for fail in [false, true] {
        let mut ws = workspace();
        let (original, _) = document(&mut ws, 321.0);
        let original_uuid = ws.editor().draft().recipe().unwrap().document().id;
        ws.take_effects();
        ws.apply(Command::Learning(L::Open(Topic::Overview)))
            .unwrap();
        ws.apply(Command::Learning(L::CreateExample(Example::PulsedTone)))
            .unwrap();
        let prompt = take_unsaved_prompt(&mut ws);
        ws.apply(Command::Answer {
            prompt,
            answer: PromptAnswer::Save,
        })
        .unwrap();
        let save = take_file_job(&mut ws);
        ws.apply(Command::Learning(L::Open(Topic::RhythmBands)))
            .unwrap();
        let response = if fail {
            Response::Finished {
                job: save.id(),
                outcome: JobOutcome::Failed(WorkerFailure::InvalidOutcome),
            }
        } else {
            execute_file(save)
        };
        assert_eq!(ws.respond(response), ResponseDisposition::Applied);
        if fail {
            assert_eq!(ws.editor().document(), original);
            assert!(ws.learning().is_some());
            assert!(ws.editor().dirty());
        } else {
            assert_ne!(ws.editor().document(), original);
            assert!(ws.learning().is_none());
            assert!(ws.editor().dirty(), "the example must not be auto-saved");
            let saved = ws
                .store
                .load(&crate::document::origin::LibraryKey::User(original_uuid))
                .unwrap();
            assert_eq!(
                saved.snapshot.sound().components()[0]
                    .form
                    .get(ParameterId::ToneFrequency),
                Some(finite(321.0))
            );
        }
    }
}

#[test]
fn invalid_staging_prevents_save_before_example_without_discarding_input() {
    let mut ws = workspace();
    let (original, field) = document(&mut ws, 321.0);
    ws.apply(Command::Editor(EditorCommand::Stage {
        field,
        text: "not a frequency".into(),
    }))
    .unwrap();
    ws.take_effects();
    ws.apply(Command::Learning(L::Open(Topic::Overview)))
        .unwrap();
    ws.apply(Command::Learning(L::CreateExample(Example::PulsedTone)))
        .unwrap();
    let prompt = take_unsaved_prompt(&mut ws);
    ws.apply(Command::Answer {
        prompt,
        answer: PromptAnswer::Save,
    })
    .unwrap();
    assert_eq!(ws.editor().document(), original);
    assert_eq!(ws.editor().staged(field.field), Some("not a frequency"));
    assert!(ws.editor().dirty());
    assert!(!ws.file_busy());
    assert!(ws.learning().is_some());
    assert!(ws.take_effects().is_empty());
}

#[test]
fn selecting_teaching_construction_and_projecting_never_creates_work() {
    use crate::learning::Construction;
    let mut ws = workspace();
    let original = ws.editor().document();
    let recipe = ws.editor().draft().recipe().unwrap();
    ws.take_effects();
    ws.apply(Command::Learning(L::Open(Topic::SoundTypes)))
        .unwrap();
    for construction in Construction::ALL {
        ws.apply(Command::Learning(L::SelectConstruction(construction)))
            .unwrap();
        assert_eq!(ws.learning().unwrap().construction, construction);
        let _ = construction.visual();
        let _ = construction.visual();
        assert_eq!(ws.editor().document(), original);
        assert_eq!(ws.editor().draft().recipe().unwrap(), recipe);
        assert!(ws.take_effects().is_empty());
        assert!(!ws.audio_busy());
        assert!(!ws.file_busy());
    }
    ws.apply(Command::Learning(L::Open(Topic::PitchAndRhythm)))
        .unwrap();
    assert_eq!(
        ws.learning().unwrap().construction,
        Construction::PulseTrain
    );
}
