//! Production callback/projection/Slint witnesses for the field feedback fixes.
use super::*;

#[test]
fn overflowing_pair_has_one_transport_blocker_and_keeps_field_help_and_busy_stop_feedback() {
    let h = Harness::with_catalog(
        auditorylab_app::services::catalog().unwrap(),
        Capabilities::available(),
    );
    let ui = h.ui();
    h.workbench();
    h.adapter
        .command(|w| {
            let row = w.editor().document_key();
            w.apply(Command::Editor(EditorCommand::AddPair {
                row,
                kind: auditorylab_app::core::PairKind::Binaural,
            }))
        })
        .unwrap()
        .unwrap();
    let binding = h.component_key(0);
    ui.invoke_stage_parameter(
        parameter_field(&binding, ParameterId::PairFirstFrequency),
        "1e309".into(),
    );
    h.settle();
    let reason = ui.get_play_reason();
    assert!(reason.contains("not finite"));
    assert!(!ui.get_operation_summary().contains("Correct these fields"));
    assert!(!ui.get_operation_outcome().contains("Correct these fields"));
    assert!(ui.get_save_reason().contains("not finite"));
    assert!(
        ui.get_components()
            .row_data(0)
            .unwrap()
            .parameters
            .iter()
            .any(|p| p.diagnostic.contains("not finite"))
    );
    assert!(labelled(ui, reason.as_str()).geometry().size.height > 0.0);
    ui.invoke_restore_parameter(parameter_field(&binding, ParameterId::PairFirstFrequency));
    ui.invoke_play_stop();
    let audio = take_job(&h, JobKind::Audio);
    assert!(ui.get_playback_busy());
    ui.invoke_stage_parameter(
        parameter_field(&binding, ParameterId::PairFirstFrequency),
        "1e309".into(),
    );
    h.settle();
    assert_eq!(ui.get_transport_action(), "Stop audio");
    assert!(
        ui.get_operation_summary().contains("not finite"),
        "busy primary shows facts, so invalid editing must remain visible"
    );
    assert!(!ui.get_can_play());
    finish_stopped_audio(&h, audio);
    ui.invoke_restore_parameter(parameter_field(&binding, ParameterId::PairFirstFrequency));
    h.settle();
    assert!(!ui.get_play_reason().contains("not finite"));
}

#[test]
fn trash_restore_and_empty_trash_report_the_captured_action_and_actual_scope() {
    let h = Harness::with_catalog(
        auditorylab_app::services::catalog().unwrap(),
        Capabilities::available(),
    );
    let ui = h.ui();
    h.workbench();
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "Feedback record".into(),
    );
    ui.invoke_save_recipe();
    h.wait_for_files();
    assert!(
        ui.get_operation_summary()
            .starts_with("Saved “Feedback record”")
    );
    ui.invoke_tab_changed(0);
    ui.invoke_show_saved();
    h.wait_for_files();
    let saved = ui.get_library_key();
    ui.invoke_library_action(
        ui.get_library_entry(),
        crate::generated::LibraryMutationAction::Trash,
        "".into(),
    );
    h.wait_for_files();
    assert!(
        ui.get_operation_summary()
            .starts_with("Moved to Trash · Feedback record"),
        "{}",
        ui.get_operation_summary()
    );
    assert!(ui.get_status().contains("Moved to Trash"));
    assert_ne!(
        ui.get_library_key(),
        saved,
        "selection change cannot retarget completion wording"
    );
    ui.invoke_collection_filter(4, 0);
    h.wait_for_files();
    ui.invoke_library_action(
        ui.get_library_entry(),
        crate::generated::LibraryMutationAction::Restore,
        "".into(),
    );
    h.wait_for_files();
    assert!(
        ui.get_operation_summary()
            .starts_with("Restored from Trash · Feedback record")
    );
    assert!(ui.get_play_reason().starts_with("Trash is empty"));
    assert!(!ui.get_play_reason().contains("search"));
}

#[test]
fn recording_guidance_tracks_real_open_first_analysis_and_retained_cancel_failure_states() {
    let h = Harness::new();
    let ui = h.ui();
    ui.invoke_tab_changed(3);
    h.settle();
    assert!(!ui.get_has_recording());
    assert!(
        ui.get_recording_measurements()
            .contains("Choose Open recording")
    );
    assert!(!ui.get_recording_measurements().contains("Choose Analyze"));
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("owned.flac");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../app/tests/fixtures/recording/tone.flac"),
        &path,
    )
    .unwrap();
    ui.invoke_open_recording();
    let [Request::Path(prompt, PathPurpose::OpenRecording, None)] = h.take()[..] else {
        panic!("recording dialog")
    };
    h.adapter
        .respond(prompt, PromptAnswer::Path(Some(path.clone())));
    h.wait_for_files();
    assert!(
        ui.get_recording_measurements()
            .contains("Choose Analyze recording")
    );
    ui.invoke_analyze_recording("0".into(), "2".into());
    let first = take_job(&h, JobKind::File);
    h.settle();
    assert!(
        ui.get_recording_measurements()
            .contains("No completed analysis")
    );
    assert!(ui.get_recording_measurements().contains("Cancel analysis"));
    assert!(!ui.get_recording_measurements().contains("Choose Analyze"));
    assert!(labelled(ui, "Cancel analysis").geometry().size.height > 0.0);
    finish_file(&h, first);
    h.settle();
    let completed = ui.get_recording_findings();
    assert!(ui.get_has_recording_analysis());
    ui.invoke_analyze_recording("0".into(), "2".into());
    let repeated = take_job(&h, JobKind::File);
    h.settle();
    assert!(
        ui.get_recording_measurements()
            .contains("previous completed results")
    );
    assert_eq!(ui.get_recording_findings(), completed);
    ui.invoke_cancel_recording_analysis();
    finish_file(&h, repeated);
    h.settle();
    assert!(ui.get_recording_measurements().contains("cancelled"));
    assert!(ui.get_recording_measurements().contains("retained"));
    assert_eq!(ui.get_recording_findings(), completed);
    std::fs::write(&path, b"not a recording").unwrap();
    ui.invoke_analyze_recording("0".into(), "2".into());
    h.wait_for_files();
    h.settle();
    assert!(ui.get_recording_measurements().contains("failed"));
    assert!(ui.get_recording_measurements().contains("retained"));
    assert_eq!(ui.get_recording_findings(), completed);
    ui.window().set_size(slint::PhysicalSize::new(680, 600));
    h.settle();
    let hint = ui.get_recording_measurements();
    let item = reveal_scrolled_control(&h, hint.as_str(), "Recording content");
    assert_eq!(
        item.accessible_string_property(AccessibleStringProperty::Label)
            .as_deref(),
        Some(hint.as_str())
    );
    assert!(item.geometry().size.height > 0.0);
}
