//! Independent semantic witnesses for transport, mutation and absence wording.
use auditorylab_app::document::editing::EditingField;
use auditorylab_app::services::library::{Library, MutationKind as K, MutationStatus as S};
use auditorylab_app::workspace::status::{EditStatus, FileStatus, LibraryStatus};
use auditorylab_app::workspace::{
    Capabilities, Command, EditorCommand, Effect, JobKind, LibraryCommand, Refusal, View, Workspace,
};
use auditorylab_core::ParameterId;
use auditorylab_presentation::{library, workspace as text};

fn idle(workspace: &mut Workspace) {
    for _ in 0..100 {
        for effect in workspace.take_effects() {
            if let Effect::Run(job) = effect {
                assert_eq!(job.kind(), JobKind::File);
                workspace.respond(job.execute(|_| panic!("no audio in wording tests")));
            }
        }
        if !workspace.poll() {
            return;
        }
    }
    panic!("file work did not settle");
}
fn fixture() -> Result<(tempfile::TempDir, Workspace), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let store = Library::at_path(temp.path().join("library"))?;
    assert_eq!(store.initialize()?.warnings, []);
    let mut ws = Workspace::with_library(
        auditorylab_app::services::catalog()?,
        Capabilities::available(),
        store,
    )?;
    idle(&mut ws);
    Ok((temp, ws))
}

#[test]
fn transport_suppresses_only_the_complete_blocker_that_is_actually_shown() {
    let (_temp, mut ws) = fixture().unwrap();
    ws.apply(Command::SetView(View::Workbench)).unwrap();
    let row = ws.editor().document_key();
    ws.apply(Command::Editor(EditorCommand::AddPair {
        row,
        kind: auditorylab_core::PairKind::Binaural,
    }))
    .unwrap();
    let component = ws.editor_view().components[0].id;
    let field = ws
        .editor()
        .handle(EditingField::Parameter(
            component,
            ParameterId::PairFirstFrequency,
        ))
        .unwrap();
    ws.apply(Command::Editor(EditorCommand::Stage {
        field,
        text: "1e309".into(),
    }))
    .unwrap();
    let mut view = ws.action_view();
    let rendered = text::action_view(&view, &ws);
    assert!(rendered.play_reason.contains("Correct these fields"));
    assert!(rendered.play_reason.contains("not finite"));
    assert!(!rendered.operation_summary.contains("Correct these fields"));
    assert!(!rendered.operation_outcome.contains("Correct these fields"));
    assert!(
        rendered.save_reason.contains("not finite"),
        "disabled action Help remains"
    );
    assert!(rendered.operation_details.contains("not finite"));

    view.playing = true;
    view.transport_action = auditorylab_app::vocabulary::Operation::Stop;
    let busy = text::action_view(&view, &ws);
    assert_eq!(busy.transport_label, "Stop audio");
    assert!(
        busy.operation_summary.contains("Correct these fields"),
        "Stop shows generated facts, not the blocker"
    );
    view.playing = false;

    // Different full reasons can share the complete 180-character compact prefix.
    let common = "X".repeat(220);
    view.play_reason = Some(Refusal::Invalid {
        title: format!("{common} first cause"),
        rate: auditorylab_core::SampleRate::DEFAULT,
        error: None,
    });
    view.operation_refusal = Some(Refusal::Invalid {
        title: format!("{common} second cause"),
        rate: auditorylab_core::SampleRate::DEFAULT,
        error: None,
    });
    view.file_status = FileStatus::Idle;
    view.edit_status = EditStatus::Idle;
    view.audio_status = auditorylab_app::workspace::status::AudioStatus::Idle;
    let different = text::action_view(&view, &ws);
    assert_ne!(different.operation_summary, "");
    assert!(different.operation_details.contains("first cause"));
    assert!(different.operation_details.contains("second cause"));
    assert_eq!(
        different.operation_summary.trim_end_matches('…'),
        different.play_reason.chars().take(180).collect::<String>()
    );
    view.operation_refusal = Some(Refusal::FileBusy);
    view.file_status = FileStatus::DialogFailed {
        cause: "Independent native panel failure".into(),
    };
    let distinct = text::action_view(&view, &ws);
    assert!(
        distinct
            .operation_summary
            .contains("current file operation")
    );
    assert!(
        distinct
            .operation_outcome
            .contains("Independent native panel failure")
    );

    ws.apply(Command::Learning(
        auditorylab_app::learning::LearningCommand::Open(
            auditorylab_app::learning::Topic::Overview,
        ),
    ))
    .unwrap();
    let mut hidden_view = ws.action_view();
    hidden_view.file_status = FileStatus::Refused(Refusal::LearningVisible);
    let hidden = text::action_view(&hidden_view, &ws);
    assert!(
        hidden.operation_summary.contains("Leave Start here"),
        "a hidden primary line cannot suppress feedback"
    );
}

#[test]
fn all_mutation_kinds_keep_operation_warning_unchanged_and_document_save_meaning() {
    let key = auditorylab_app::document::origin::LibraryKey::User(
        "00000000-0000-4000-8000-000000000001".parse().unwrap(),
    );
    for (kind, expected) in [
        (K::Create, "Added to User Library"),
        (K::Add, "Added to User Library"),
        (K::Update, "Updated"),
        (K::Rename, "Renamed"),
        (K::Trash, "Moved to Trash"),
        (K::Restore, "Restored from Trash"),
        (K::Purge, "Permanently deleted"),
    ] {
        for status in [S::Published, S::PublishedWithWarning, S::Unchanged] {
            let value = LibraryStatus::Written {
                kind,
                key: key.clone(),
                title: (kind != K::Purge).then(|| "Captured title".into()),
                status,
                document: false,
                feedback: vec![],
                diagnostics: if status == S::PublishedWithWarning {
                    vec![auditorylab_app::services::StorageWarning::ClockPredatesEdit]
                } else {
                    vec![]
                },
            };
            let description = text::status::library_status_description(&value);
            if status == S::Unchanged {
                assert!(description.starts_with("Unchanged"));
            } else {
                assert!(description.starts_with(expected), "{description}");
            }
            if status == S::PublishedWithWarning {
                assert!(description.contains("with warnings"));
                assert!(
                    description.contains("Observed clock predates the prior declared edit time")
                );
            }
            assert!(!description.contains("Later edits"));
            if kind == K::Purge {
                assert!(description.contains(&key.to_string()));
            }
        }
    }
    for kind in [K::Create, K::Update, K::Trash, K::Restore, K::Purge] {
        let value = LibraryStatus::Written {
            kind,
            key: key.clone(),
            title: Some("Captured title".into()),
            status: S::Published,
            document: true,
            feedback: vec![],
            diagnostics: vec![],
        };
        let description = text::status::library_status_description(&value);
        assert_eq!(
            description.starts_with("Saved “Captured title”"),
            matches!(kind, K::Create | K::Update)
        );
        assert_eq!(
            description.contains("Later edits"),
            matches!(kind, K::Create | K::Update)
        );
    }
}

#[test]
fn library_absence_uses_scope_filters_and_actual_unique_results() {
    let (_temp, mut ws) = fixture().unwrap();
    for (scope, expected) in [
        (
            auditorylab_app::discovery::query::Scope::Trash,
            "Trash is empty",
        ),
        (
            auditorylab_app::discovery::query::Scope::User,
            "Your User Library is empty",
        ),
        (
            auditorylab_app::discovery::query::Scope::OpenFiles,
            "No open files",
        ),
    ] {
        ws.apply(Command::Library(LibraryCommand::SetQuery(Box::new(
            auditorylab_app::discovery::collection::CatalogQuery {
                scope,
                ..Default::default()
            },
        ))))
        .unwrap();
        idle(&mut ws);
        let rendered = text::action_view(&ws.action_view(), &ws);
        assert!(
            rendered.play_reason.starts_with(expected),
            "{}",
            rendered.play_reason
        );
        assert!(!rendered.play_reason.contains("clear"));
        assert_eq!(
            rendered.play_reason,
            library::workspace_library_empty_message(&ws)
        );
    }
    ws.apply(Command::Library(LibraryCommand::SetQuery(Box::default())))
        .unwrap();
    idle(&mut ws);
    ws.apply(Command::Library(LibraryCommand::Search(
        "no such study or recipe QZZQ".into(),
    )))
    .unwrap();
    let rendered = text::action_view(&ws.action_view(), &ws);
    assert!(rendered.play_reason.contains("Clear the restrictions"));
    ws.apply(Command::Library(LibraryCommand::Search(String::new())))
        .unwrap();
    let mut view = ws.action_view();
    view.play_reason = Some(Refusal::NoSelection);
    assert_eq!(
        text::action_view(&view, &ws).play_reason,
        "Choose one of the listed recipes."
    );
}

#[test]
fn no_recording_guidance_uses_the_actual_open_action_without_an_analyze_prompt() {
    let (_temp, mut ws) = fixture().unwrap();
    ws.apply(Command::SetView(View::Recordings)).unwrap();
    let mut actions = ws.action_view();
    let guidance =
        auditorylab_presentation::recording::measurement_guidance(None, None, None, &actions);
    assert!(guidance.contains("Choose Open recording"));
    assert!(!guidance.contains("Analyze"));
    for action in &mut actions.operations {
        if action.operation == auditorylab_app::vocabulary::Operation::OpenRecording {
            action.enabled = false;
        }
    }
    let blocked =
        auditorylab_presentation::recording::measurement_guidance(None, None, None, &actions);
    assert!(blocked.contains("Opening is unavailable"));
    assert!(!blocked.contains("Choose Open recording"));
    assert!(!blocked.contains("Analyze"));
}
