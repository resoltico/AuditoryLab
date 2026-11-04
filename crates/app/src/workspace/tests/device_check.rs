//! Device check and Recordings: their own Play, answers and file admission.
use super::*;
use crate::device_check::{ChannelResult, DeviceAnswer, DeviceTest, LowToneResult};

fn audio_jobs(workspace: &mut Workspace) -> usize {
    workspace
        .take_effects()
        .into_iter()
        .filter(|effect| matches!(effect, Effect::Run(job) if job.kind() == JobKind::Audio))
        .count()
}

#[test]
fn device_check_play_captures_only_the_selected_test_tone_in_its_own_view() {
    let mut workspace = workspace();
    let play = Command::DeviceCheck(DeviceCheckCommand::Play(DeviceTest::RightChannel));
    workspace.apply(play.clone()).unwrap();
    assert_eq!(
        audio_jobs(&mut workspace),
        0,
        "another view never plays a test tone"
    );
    assert!(!workspace.audio_busy());

    workspace
        .apply(Command::SetView(View::DeviceCheck))
        .unwrap();
    let actions = workspace.action_view();
    assert_eq!(
        actions.transport_action,
        crate::vocabulary::Operation::PlayTestTone
    );
    assert_eq!(actions.play_target, "Left channel");
    workspace.apply(play).unwrap();
    // Audio is deliberately never executed: this test opens no output device.
    assert_eq!(audio_jobs(&mut workspace), 1);
    assert!(workspace.audio_busy());
    let view = workspace.device_check_view();
    assert!(
        view.tests
            .iter()
            .all(|test| test.selected == (test.test == DeviceTest::RightChannel))
    );
    assert_eq!(workspace.action_view().play_target, "Right channel");
    assert!(matches!(
        workspace.apply(Command::File(FileCommand::ExportWav)),
        Err(Refusal::AudioPlaying | Refusal::NoSoundInView)
    ));
}

#[test]
fn device_check_answers_are_validated_interpreted_and_cleared() {
    let mut workspace = workspace();
    let answer = |test, answer| Command::DeviceCheck(DeviceCheckCommand::Answer(test, answer));
    let revision = workspace.revision();
    assert!(matches!(
        workspace.apply(answer(DeviceTest::Tone40Hz, DeviceAnswer::Left)),
        Err(Refusal::UnofferedAnswer(_))
    ));
    assert_eq!(workspace.revision(), revision);
    assert!(!workspace.device_check_view().answered);
    for (test, given) in [
        (DeviceTest::LeftChannel, DeviceAnswer::Right),
        (DeviceTest::RightChannel, DeviceAnswer::Left),
        (DeviceTest::Tone100Hz, DeviceAnswer::Heard),
        (DeviceTest::Tone60Hz, DeviceAnswer::NotHeard),
        (DeviceTest::Tone40Hz, DeviceAnswer::NotHeard),
    ] {
        workspace.apply(answer(test, given)).unwrap();
    }
    let view = workspace.device_check_view();
    assert!(view.answered);
    assert_eq!(view.assessment.channels, ChannelResult::Swapped);
    assert_eq!(
        view.assessment.low_tones,
        LowToneResult::LowestHeard {
            test: DeviceTest::Tone100Hz
        }
    );
    let clear = crate::vocabulary::Operation::ClearDeviceAnswers;
    assert!(workspace_operation(&mut workspace, clear));
    workspace
        .apply(Command::DeviceCheck(DeviceCheckCommand::Clear))
        .unwrap();
    assert!(!workspace.device_check_view().answered);
    assert!(!workspace_operation(&mut workspace, clear));
    assert_eq!(
        workspace.device_check_view().assessment.channels,
        ChannelResult::Incomplete
    );
}

fn workspace_operation(workspace: &mut Workspace, operation: crate::vocabulary::Operation) -> bool {
    workspace
        .action_view()
        .operations
        .iter()
        .any(|view| view.operation == operation && view.enabled)
}

#[test]
fn recordings_view_plays_and_exports_nothing() {
    let mut workspace = workspace();
    workspace.apply(Command::SetView(View::Recordings)).unwrap();
    let actions = workspace.action_view();
    assert!(matches!(actions.play_reason, Some(Refusal::NoSoundInView)));
    assert!(matches!(
        actions.export_reason,
        Some(Refusal::NoSoundInView)
    ));
    assert!(matches!(
        workspace.apply(Command::Audio(AudioCommand::Play)),
        Err(Refusal::NoSoundInView)
    ));
    assert_eq!(audio_jobs(&mut workspace), 0);
    assert!(workspace_operation(
        &mut workspace,
        crate::vocabulary::Operation::OpenRecording
    ));
}

#[test]
fn an_opened_recording_is_identified_by_a_file_job_and_failures_keep_the_previous_one() {
    let mut workspace = workspace();
    let directory = temporary_directory();
    let path = directory.path().join("take.flac");
    std::fs::write(&path, b"fLaC\0\0\0\x22rest").unwrap();
    workspace
        .apply(Command::File(FileCommand::OpenRecording))
        .unwrap();
    assert!(matches!(
        workspace.apply(Command::File(FileCommand::OpenRecording)),
        Err(Refusal::PromptOpen)
    ));
    answer_path(
        &mut workspace,
        PathPurpose::OpenRecording,
        Some(path.clone()),
    );
    let job = take_file_job(&mut workspace);
    assert!(
        workspace.recording().is_none(),
        "IO waits for its completion"
    );
    assert!(matches!(
        workspace.apply(Command::File(FileCommand::OpenRecording)),
        Err(Refusal::FileBusy)
    ));
    assert_eq!(
        workspace.respond(execute_file(job)),
        ResponseDisposition::Applied
    );
    let opened = workspace.recording().cloned().unwrap();
    assert_eq!(opened.path, path);
    assert_eq!(opened.bytes, 12);
    assert_eq!(opened.format, crate::recording::ContainerFormat::Flac);

    workspace
        .apply(Command::File(FileCommand::OpenRecording))
        .unwrap();
    answer_path(
        &mut workspace,
        PathPurpose::OpenRecording,
        Some(directory.path().join("missing.wav")),
    );
    let job = take_file_job(&mut workspace);
    assert_eq!(
        workspace.respond(execute_file(job)),
        ResponseDisposition::Applied
    );
    assert_eq!(workspace.recording(), Some(&opened));
    assert!(matches!(
        workspace.action_view().file_status,
        status::FileStatus::Failed(_)
    ));
}

#[test]
fn analysis_admission_capture_cancellation_and_failure_preserve_complete_findings() {
    use crate::recording::{AnalysisOptions, RecordingError};
    let mut workspace = workspace();
    let request = Command::File(FileCommand::AnalyzeRecording(AnalysisOptions::default()));
    assert!(matches!(
        workspace.apply(request.clone()),
        Err(Refusal::NoSelection)
    ));
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recording/tone.flac");
    workspace
        .apply(Command::File(FileCommand::OpenRecording))
        .unwrap();
    answer_path(&mut workspace, PathPurpose::OpenRecording, Some(path));
    let job = take_file_job(&mut workspace);
    workspace.respond(execute_file(job));
    workspace.apply(request.clone()).unwrap();
    assert!(workspace.analyzing_recording());
    assert!(matches!(
        workspace.apply(request.clone()),
        Err(Refusal::FileBusy)
    ));
    let job = take_file_job(&mut workspace);
    assert!(workspace.recording_analysis().is_none());
    workspace.respond(execute_file(job));
    let previous = workspace.recording_analysis().cloned().unwrap();
    workspace.apply(request).unwrap();
    let job = take_file_job(&mut workspace);
    workspace
        .apply(Command::File(FileCommand::CancelAnalysis))
        .unwrap();
    assert!(
        workspace.analyzing_recording(),
        "cancellation does not release an unjoined worker"
    );
    workspace.respond(execute_file(job));
    assert!(!workspace.analyzing_recording());
    assert_eq!(workspace.recording_analysis(), Some(&previous));
    assert!(matches!(
        workspace.action_view().file_status,
        status::FileStatus::Failed(crate::services::OperationError::Recording(
            RecordingError::Cancelled
        ))
    ));
    workspace
        .apply(Command::File(FileCommand::AnalyzeRecording(
            AnalysisOptions { seconds: Some(0) },
        )))
        .unwrap();
    let job = take_file_job(&mut workspace);
    workspace.respond(execute_file(job));
    assert_eq!(workspace.recording_analysis(), Some(&previous));
}
