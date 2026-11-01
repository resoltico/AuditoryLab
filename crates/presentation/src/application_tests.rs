//! Wording contracts exercised through the public application boundary.
use crate::*;
use auditorylab_app::discovery::calendar::local_range;
use auditorylab_app::discovery::collection::CatalogQuery;
use auditorylab_app::discovery::query::CalendarField;
use auditorylab_app::discovery::query::{GroupBy, SortBy};
use auditorylab_app::document::editing::Draft;
use auditorylab_app::document::editing::EditingField;
use auditorylab_app::job::Cancel;
use auditorylab_app::services::playback::BackendError;
use auditorylab_app::services::playback::DeviceError;
use auditorylab_app::workspace::status::FileStatus;
use auditorylab_app::workspace::{
    Capabilities, Command, EditorCommand, Effect, FileCommand, JobKind, LibraryCommand, View,
    Workspace,
};
use auditorylab_core::test_support::finite;
use auditorylab_core::{
    CarrierKind, ChannelRouting, ComponentForm, ComponentId, ParameterId, SampleRate,
};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
struct Harness {
    workspace: Workspace,
    root: tempfile::TempDir,
}
impl Deref for Harness {
    type Target = Workspace;
    fn deref(&self) -> &Workspace {
        &self.workspace
    }
}
impl DerefMut for Harness {
    fn deref_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }
}
fn workspace() -> Harness {
    let root = tempfile::tempdir().unwrap();
    let library =
        auditorylab_app::services::library::Library::at_path(root.path().join("library")).unwrap();
    library.initialize().unwrap();
    let workspace = Workspace::with_library(
        auditorylab_app::services::catalog().unwrap(),
        Capabilities::available(),
        library,
    )
    .unwrap();
    let mut harness = Harness { workspace, root };
    idle(&mut harness);
    harness
}
fn idle(workspace: &mut Workspace) {
    for _ in 0..100 {
        for effect in workspace.take_effects() {
            if let Effect::Run(job) = effect {
                assert_eq!(
                    job.kind(),
                    JobKind::File,
                    "wording checks never execute native audio"
                );
                let response = job.execute(|_| panic!("file work cannot prepare audio"));
                workspace.respond(response);
            }
        }
        workspace.poll();
        if !workspace.file_busy() {
            return;
        }
    }
    panic!("file work did not settle");
}
fn component(frequency_hz: f64, route: ChannelRouting) -> ComponentForm {
    ComponentForm {
        source: auditorylab_app::core::SoundSource::Tone {
            frequency_hz: finite(frequency_hz),
            route,
            peak_dbfs: finite(-30.0),
        },
        modulation: None,
    }
}
#[test]
fn receipt_age_reads_in_human_units() {
    use std::time::Duration;
    for (millis, held, text) in [
        (120, false, "Received samples just now"),
        (12_495, false, "Received samples 12 s ago"),
        (125_000, false, "Received samples 2 min ago"),
        (7_300_000, true, "Current session received samples 2 h ago"),
    ] {
        assert_eq!(
            crate::workspace::received_text(Duration::from_millis(millis), held),
            text
        );
    }
}

#[test]
fn committed_brief_never_borrows_pending_pair_facts() {
    let mut ws = workspace();
    ws.apply(Command::SetView(View::Workbench)).unwrap();
    let key = ws.editor().document_key();
    ws.apply(Command::Editor(EditorCommand::AddPair {
        row: key,
        kind: auditorylab_core::PairKind::Binaural,
    }))
    .unwrap();
    let id = ws.editor().draft().components()[0].id;
    let first = ws
        .editor()
        .handle(EditingField::Parameter(id, ParameterId::PairFirstFrequency))
        .unwrap();
    let offset = ws
        .editor()
        .handle(EditingField::Parameter(id, ParameterId::PairOffset))
        .unwrap();
    ws.apply(Command::Editor(EditorCommand::CommitField {
        field: first,
        text: ("317").to_string(),
    }))
    .unwrap();
    ws.apply(Command::Editor(EditorCommand::CommitField {
        field: offset,
        text: ("-7").to_string(),
    }))
    .unwrap();
    let original = crate::workspace::editor_view(&ws.editor_view()).brief;
    assert!(original.contains("7 Hz") && original.contains("317") && original.contains("310"));
    for text in ["40", "NaN"] {
        ws.apply(Command::Editor(EditorCommand::Stage {
            field: offset,
            text: (text).to_string(),
        }))
        .unwrap();
        let view = crate::workspace::editor_view(&ws.editor_view());
        assert_eq!(
            view.brief, original,
            "pending text never changes the applied brief"
        );
        assert!(!view.brief.contains("357") && !view.brief.contains("No valid sound"));
        assert_eq!(
            ws.editor().draft().components()[0]
                .form
                .pair()
                .unwrap()
                .offset_hz
                .get(),
            -7.0
        );
        ws.apply(Command::Editor(EditorCommand::Restore(offset)))
            .unwrap();
        assert_eq!(ws.editor().text(offset.field), "-7");
        assert_eq!(
            crate::workspace::editor_view(&ws.editor_view()).brief,
            original
        );
    }
    ws.apply(Command::Editor(EditorCommand::Stage {
        field: offset,
        text: ("40").to_string(),
    }))
    .unwrap();
    ws.apply(Command::Editor(EditorCommand::Commit)).unwrap();
    let committed = crate::workspace::editor_view(&ws.editor_view()).brief;
    assert!(committed.contains("40 Hz") && committed.contains("357"));
    assert!(!committed.contains("Last committed"));
}

#[test]
fn action_budget_messages_distinguish_components_from_expanded_voices() {
    for (primitives, pairs, components, voices) in [(0, 8, 8, 16), (16, 0, 16, 16), (15, 0, 15, 15)]
    {
        let mut ws = workspace();
        ws.apply(Command::SetView(View::Workbench)).unwrap();
        let key = ws.editor().document_key();
        for _ in 0..primitives {
            ws.apply(Command::Editor(EditorCommand::AddComponent {
                row: key,
                kind: CarrierKind::Tone,
            }))
            .unwrap();
        }
        for _ in 0..pairs {
            ws.apply(Command::Editor(EditorCommand::AddPair {
                row: key,
                kind: auditorylab_core::PairKind::Binaural,
            }))
            .unwrap();
        }
        let view = crate::workspace::action_view(&ws.action_view());
        assert!(!ws.action_view().can_add_pair());
        assert!(view.pair_add_reason.contains(&format!(
            "uses {voices} of 16 voices in {components} components"
        )));
        assert!(view.pair_add_reason.contains("2 more voices"));
        assert_eq!(
            ws.action_view().can_add_component(),
            voices < 16 && components < 16
        );
        if !ws.action_view().can_add_component() {
            assert!(view.add_component_reason.contains(&format!(
                "uses {voices} of 16 voices in {components} components"
            )));
            assert!(view.add_component_reason.contains("1 more voice"));
        }
    }
}

#[test]
fn structured_library_rows_keep_saved_identity_and_rate_independent_classification() {
    let mut ws = workspace();
    ws.apply(Command::Library(LibraryCommand::Search(
        ("ioannou 2").to_string(),
    )))
    .unwrap();
    let view = crate::workspace::library_view(&ws.library_view());
    let typed = ws.library_view();
    assert_eq!(typed.rows.len(), 1);
    let row = crate::library::row(&typed.rows[0], typed.sort);
    assert!(row.key.contains("ioannou-2015-binaural-2-hz"));
    assert_eq!(row.bands, "Delta");
    assert!(row.collection.contains("Read only"));
    assert!(row.text.contains(&row.title) && row.text.contains(&row.dates));
    let classification = view.classification.clone();
    ws.apply(Command::Editor(EditorCommand::SetExportRate(
        auditorylab_core::SampleRate::new(96_000).unwrap(),
    )))
    .unwrap();
    assert_eq!(
        crate::workspace::library_view(&ws.library_view()).classification,
        classification
    );
    assert_eq!(
        crate::library::row(&ws.library_view().rows[0], ws.library_view().sort).key,
        row.key
    );
}

#[test]
fn staged_envelope_failures_render_quantities_and_rate_in_metadata() {
    let mut workspace = workspace();
    let handle = workspace
        .editor()
        .handle(EditingField::Metadata(
            auditorylab_app::document::editing::MetadataField::Duration,
        ))
        .unwrap();
    workspace
        .apply(Command::Editor(EditorCommand::Stage {
            field: handle,
            text: ("0").to_string(),
        }))
        .unwrap();
    let view = workspace.editor_view();
    let duration = view
        .metadata
        .iter()
        .find(|field| field.handle == handle)
        .unwrap();
    assert_eq!(
        crate::workspace::metadata_text(duration, &view).diagnostic,
        "Duration 0 s must last from one sample (1/48000 s) to 16.6 days at 48000 Hz"
    );
}

#[test]
fn generated_routing_text_reports_intermediate_gains_not_the_requested_endpoint() {
    use auditorylab_core::{Observation, SlotRole, VoiceObservation};

    let check = auditorylab_app::core::test_support::FiniteFixture::new(
        finite(1.0),
        finite(0.0),
        ([(
            ComponentId::new(1).unwrap(),
            component(440.0, ChannelRouting::Right),
        )])
        .iter()
        .map(|(id, voice)| auditorylab_core::Component {
            id: *id,
            form: *voice,
        })
        .collect(),
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
    .check(SampleRate::DEFAULT);
    let facts = *check.target().unwrap().voices().next().unwrap().1;
    let mut voices = [None; Observation::MAX_VOICES];
    voices[0] = Some(VoiceObservation {
        id: ComponentId::new(1).unwrap().into(),
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
    let text = crate::optional_observation_summary(Some(&observation));
    assert!(text.contains("Both channels"), "{text}");
    assert!(crate::observation_details(&(observation)).contains("gain L -26.0 / R -26.0 dBFS"));
    assert!(
        crate::observation_details(&(observation)).contains("Continuing"),
        "role label, not Debug"
    );
    assert!(text.starts_with("Changing"));
    assert_eq!(
        ParameterId::SourcePulseDuration.descriptor().label,
        "Source pulse duration"
    );
}

#[test]
fn calendar_order_errors_name_visible_fields_and_same_day_is_inclusive() {
    for (field, label) in [
        (CalendarField::Created, "Created"),
        (CalendarField::Modified, "Modified"),
        (CalendarField::Added, "Added here"),
    ] {
        let error = local_range(field, "2026-10-26", "2026-10-25").unwrap_err();
        assert_eq!(
            auditorylab_core::diagnostic::Code::code(&error),
            "query.local_order"
        );
        let error = error.explain();
        assert_eq!(
            error,
            format!("{label} through (2026-10-25) must be on or after {label} from (2026-10-26).")
        );
        assert!(!error.contains("before"));
        let range = local_range(field, "2026-10-25", "2026-10-25").unwrap();
        assert!(range.from.unwrap() < range.before.unwrap());
        assert!(range.contains(range.from));
        assert!(!range.contains(range.before));
    }
}

#[test]
fn a_route_change_says_what_happened_and_what_to_do() {
    let changed =
        DeviceError::RouteChanged(BackendError::new(std::io::Error::other("route changed")));
    assert_eq!(
        auditorylab_core::diagnostic::Code::code(&changed),
        "device.route_changed"
    );
    assert_eq!(
        changed.explain(),
        "the audio output route changed and playback stopped; select the \
            intended output and play again"
    );
    let busy = DeviceError::Backend(BackendError::new(std::io::Error::other("in use")));
    assert_eq!(busy.explain(), "audio backend: in use");
    assert_eq!(
        DeviceError::TooFewChannels { channels: 1 }.explain(),
        "the default output device has 1 channel; stereo output needs two digital channels"
    );
}

#[test]
fn source_errors_keep_raw_path_data_and_escape_it_only_for_display() {
    let directory = directory();
    // Windows forbids newline in a filename; bidi controls are valid on both OSes.
    let filename = if cfg!(windows) {
        "source\u{202e}name.toml"
    } else {
        "source\n\u{202e}name.toml"
    };
    let path = directory.path().join(filename);
    let bytes = auditorylab_core::test_support::silent(1.0, 0.0)
        .to_toml()
        .unwrap()
        .replacen("schema_version = 5", "schema_version = 3", 1);
    std::fs::write(&path, &bytes).unwrap();
    let error = auditorylab_app::services::open_file(&path).unwrap_err();
    assert!(
        matches!(&error, auditorylab_app::services::LoadError::Recipe {
            origin: auditorylab_app::document::recipe::Origin::File(original), .. }
            if original == &path)
    );
    let text = error.explain();
    assert!(
        text.starts_with(directory.path().to_str().unwrap()),
        "{text}"
    );
    let displayed_filename = if cfg!(windows) {
        "source\\u{202e}name.toml"
    } else {
        "source\\n\\u{202e}name.toml"
    };
    assert!(text.contains(displayed_filename), "{text}");
    assert!(!text.contains('\n') && !text.contains('\u{202e}'));
    assert!(text.contains("schema_version 3; this build reads 5"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes.as_bytes());
}

fn directory() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}
#[test]
fn entropy_failure_mentions_random_uuid() {
    let error = auditorylab_app::identity::IdentityError::Entropy(getrandom::Error::UNSUPPORTED);
    assert!(error.explain().contains("random UUID"));
}
#[test]
fn backend_failure_preserves_native_message() {
    for text in ["unplugged", "original setup failure"] {
        let error = DeviceError::Backend(BackendError::new(std::io::Error::other(text)));
        assert_eq!(error.explain(), format!("audio backend: {text}"));
    }
}

fn fixture() -> (
    tempfile::TempDir,
    auditorylab_app::services::library::Library,
    Workspace,
) {
    let harness = workspace();
    let path = harness.workspace.library_data().path.unwrap().to_owned();
    (
        harness.root,
        auditorylab_app::services::library::Library::at_path(path).unwrap(),
        harness.workspace,
    )
}
fn idle_files(w: &mut Workspace) {
    idle(w);
}

#[test]
fn library_failure_notices_keep_conflicts_refusals_and_publication_io_distinct() {
    let (_directory, store, mut workspace) = fixture();
    workspace
        .apply(Command::SetView(
            auditorylab_app::workspace::View::Workbench,
        ))
        .unwrap();
    workspace.apply(Command::File(FileCommand::Save)).unwrap();
    idle_files(&mut workspace);
    let key = store.scan(&Cancel::default()).entries[0].key.clone();
    for (kind, publishing, inspect, replacement) in library_failure_cases() {
        let status =
            FileStatus::Library(auditorylab_app::workspace::status::LibraryStatus::Failed {
                error: captured_error(kind),
                publishing,
                context: auditorylab_app::workspace::status::LibraryContext {
                    path: store.path().into(),
                    selector: Some(key.clone()),
                },
            });
        let notice = crate::workspace::status::file_status_description(&(status));
        assert_eq!(
            notice.contains("inspect this location"),
            inspect,
            "{notice}"
        );
        assert_eq!(
            notice.contains("did not publish a replacement"),
            replacement,
            "{notice}"
        );
        assert!(
            notice.lines().next().unwrap().contains(&key.to_string()),
            "{notice}"
        );
        if !publishing
            && matches!(
                kind,
                auditorylab_app::outcome::Outcome::Conflict
                    | auditorylab_app::outcome::Outcome::HistoryConflict
            )
        {
            assert!(notice.starts_with("Library read conflict"), "{notice}");
        }
        assert!(
            notice.contains(&store.path().display().to_string()),
            "{notice}"
        );
    }
}

fn captured_error(
    outcome: auditorylab_app::outcome::Outcome,
) -> auditorylab_app::services::OperationError {
    use {
        auditorylab_app::identity::IdentityError, auditorylab_app::identity::IdentityScope,
        auditorylab_app::outcome::Outcome, auditorylab_app::services::FileError,
        auditorylab_app::services::LoadError, auditorylab_app::services::OperationError,
        auditorylab_app::services::library::LibraryError,
    };
    let id = auditorylab_app::services::catalog().unwrap()[0]
        .recipe()
        .document()
        .id;
    match outcome {
        Outcome::HistoryConflict => LibraryError::HistoryReplacement { id }.into(),
        Outcome::Conflict => LibraryError::StaleContent { id }.into(),
        Outcome::Write => FileError::MissingFilename.into(),
        Outcome::Unreadable => LoadError::Read {
            origin: auditorylab_app::document::recipe::Origin::Stream("captured".into()),
            cause: io_cause("read failed"),
        }
        .into(),
        Outcome::IncompleteInventory => LibraryError::RootMissing.into(),
        Outcome::Busy => LibraryError::Busy.into(),
        Outcome::LockUnavailable => LibraryError::LockUnavailable {
            error: io_cause("locking unavailable"),
        }
        .into(),
        Outcome::Cancelled => LibraryError::Cancelled.into(),
        Outcome::ReadOnlyBuiltIn => LibraryError::ReadOnlyBuiltin {
            id: "captured".into(),
        }
        .into(),
        Outcome::InvalidRecipe => {
            OperationError::Recipe(auditorylab_app::core::RecipeError::NotUtf8)
        }
        Outcome::InvalidSound => LoadError::Sound {
            origin: auditorylab_app::document::recipe::Origin::Stream("captured".into()),
            rate: SampleRate::DEFAULT,
            error: Box::new(auditorylab_app::core::SoundError::Duration {
                seconds: 0.0,
                sample_rate_hz: 48000,
            }),
        }
        .into(),
        Outcome::InvalidArgument => LibraryError::PurgeUnconfirmed.into(),
        Outcome::Identity => IdentityError::Exhausted(IdentityScope::ExternalEntry).into(),
        _ => panic!("category not used by this notice matrix"),
    }
}

fn io_cause(message: &'static str) -> auditorylab_app::services::IoCause {
    struct FailedReader(&'static str);
    impl std::io::Read for FailedReader {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other(self.0))
        }
    }
    let auditorylab_app::services::LoadError::Read { cause, .. } =
        auditorylab_app::services::read_source(FailedReader(message), "fixture").unwrap_err()
    else {
        panic!("reader failure")
    };
    cause
}

#[test]
fn global_summary_preserves_full_refusal_and_outcome_independent_of_editor_scroll() {
    let mut ws = workspace();
    ws.apply(Command::SetView(View::Workbench)).unwrap();
    let notes = ws
        .editor()
        .handle(EditingField::Metadata(
            auditorylab_app::document::editing::MetadataField::Summary,
        ))
        .unwrap();
    ws.apply(Command::Editor(EditorCommand::Stage {
        field: notes,
        text: "Z".repeat(66000),
    }))
    .unwrap();
    let mut typed = ws.action_view();
    let actions = crate::workspace::action_view(&typed);
    assert!(!typed.can_save());
    assert!(actions.operation_summary.contains("65536"));
    typed.file_status = FileStatus::Exported {
        path: PathBuf::from("capture.wav"),
        exported: auditorylab_app::services::Exported {
            frames: 100,
            sample_rate: SampleRate::DEFAULT,
            container: auditorylab_app::services::Container::Riff,
            warnings: Vec::new(),
        },
    };
    let actions = crate::workspace::action_view(&typed);
    assert!(actions.operation_summary.contains("65536"));
    assert!(actions.operation_outcome.starts_with("Exported 100"));
    assert!(actions.operation_summary.chars().count() <= 181);
    ws.apply(Command::Editor(EditorCommand::Restore(notes)))
        .unwrap();
    typed = ws.action_view();
    typed.file_status = FileStatus::Failed(
        auditorylab_app::services::ExportError::File {
            error: auditorylab_app::services::FileError::Io(auditorylab_app::services::IoFailure {
                action: auditorylab_app::services::IoOperation::Write,
                path: Some(std::path::PathBuf::from("deep/path/".repeat(50))),
                cause: io_cause("disk full"),
            }),
        }
        .into(),
    );
    let actions = crate::workspace::action_view(&typed);
    assert!(actions.operation_details.contains("cannot write file"));
    assert!(actions.operation_details.len() > actions.operation_summary.len());
}

#[test]
fn applied_summary_lists_values_from_committed_query_and_refuses_invalid_replacement() {
    use auditorylab_app::discovery::collection::DateRange;
    let (_temp, _store, mut w) = fixture();
    let from = "2026-03-28T22:00:00Z".parse().unwrap();
    let before = "2026-03-29T21:00:00Z".parse().unwrap();
    let query = CatalogQuery {
        search: "exact search".into(),
        studies: vec!["10.1234/exact".into()],
        populations: vec![auditorylab_core::provenance::Population::Human],
        roles: vec![auditorylab_core::provenance::ConditionRole::Comparison],
        analysis: Some(auditorylab_core::rhythm::Completeness::Invalid),
        rhythm: auditorylab_app::discovery::query::RhythmQuery {
            bands: vec![auditorylab_core::rhythm::Band::Delta],
            methods: vec![auditorylab_core::rhythm::Method::Binaural],
            state: Some(auditorylab_app::discovery::query::RhythmState::Multiple),
        },
        created: DateRange {
            from: Some(from),
            before: Some(before),
        },
        modified: DateRange {
            from: Some(from),
            before: None,
        },
        added: DateRange {
            from: None,
            before: Some(before),
        },
        sort: SortBy::Title,
        group_by: GroupBy::Study,
        ..CatalogQuery::default()
    };
    w.apply(Command::Library(LibraryCommand::SetQuery(Box::new(
        query.clone(),
    ))))
    .unwrap();
    let summary = crate::library::workspace_library_count(&(w));
    for expected in [
        "exact search",
        "10.1234/exact",
        "Population: Human",
        "Condition role: Comparison",
        "Analysis: Invalid",
        "Band: Delta",
        "Method: Binaural",
        "Rhythm state: Multiple",
        "Created from inclusive:",
        "Created before exclusive:",
        "Edited from inclusive:",
        "Added here before exclusive:",
        "2026-03-28T22:00:00.000000000Z",
        "2026-03-29T21:00:00.000000000Z",
    ] {
        assert!(summary.contains(expected), "{expected}: {summary}");
    }
    let mut invalid = query;
    invalid.created.before = Some(from);
    assert!(
        w.apply(Command::Library(LibraryCommand::SetQuery(Box::new(
            invalid
        ))))
        .is_err()
    );
    assert_eq!(crate::library::workspace_library_count(&(w)), summary);
}

#[test]
fn copies_keep_source_attribution_without_research_claims_and_match_recipe_ids() {
    let snapshot = auditorylab_app::services::catalog().unwrap().remove(0);
    let d = Draft::from_snapshot(&snapshot).unwrap();
    assert_eq!(d.title(), format!("{} (edited)", snapshot.title()));
    assert_eq!(d.based_on().unwrap().id(), snapshot.id());
    let ids: Vec<_> = d.components().iter().map(|c| c.id).collect();
    let target = snapshot.sound().target(SampleRate::DEFAULT).unwrap();
    let recipe_ids: Vec<_> = target.voices().map(|(id, _)| id).collect();
    assert_eq!(
        ids,
        snapshot
            .sound()
            .components()
            .iter()
            .map(|c| c.id)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        d.check(SampleRate::DEFAULT)
            .target()
            .unwrap()
            .voices()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        recipe_ids
    );
    let about = crate::recipe::about(&d.recipe().unwrap()).unwrap();
    assert_eq!(
        about.summary,
        crate::recipe::snapshot_about(&(snapshot)).unwrap().summary
    );
    assert!(about.research.is_none());
}

#[test]
fn publication_text_preserves_known_and_unconfirmed_postconditions() {
    use auditorylab_app::services::{Confirmation, FileError, MutationPhase};
    let path = PathBuf::from("complete/path/entry");
    for (phase, confirmation, positive, negative) in [
        (
            MutationPhase::Publish,
            Confirmation::Unchanged,
            vec!["previous content remains"],
            vec![],
        ),
        (
            MutationPhase::Publish,
            Confirmation::Different,
            vec!["differs from both"],
            vec![],
        ),
        (
            MutationPhase::Publish,
            Confirmation::Unavailable(Box::new(FileError::MissingFilename)),
            vec![
                "publication outcome is unconfirmed",
                "destination could not be verified",
            ],
            vec!["previous content remains", "differs from both"],
        ),
        (
            MutationPhase::Remove,
            Confirmation::Unavailable(Box::new(FileError::MissingFilename)),
            vec!["deletion outcome is unconfirmed"],
            vec!["was not applied"],
        ),
        (
            MutationPhase::Relocate {
                source: PathBuf::from("original"),
            },
            Confirmation::Unavailable(Box::new(FileError::MissingFilename)),
            vec!["relocation outcome is unconfirmed"],
            vec!["did not remove its source"],
        ),
    ] {
        let error = FileError::AfterMutation {
            phase,
            path: path.clone(),
            confirmation,
            cause: io_cause("injected"),
        };
        let message = error.explain();
        assert!(message.contains(&path.display().to_string()));
        for expected in positive {
            assert!(message.contains(expected), "{message}");
        }
        for rejected in negative {
            assert!(!message.contains(rejected), "{message}");
        }
    }
}
#[test]
fn waveform_captions_are_frozen_and_underruns_remain_readable() {
    use crate::workspace::monitor::output_underrun_warning;
    use auditorylab_app::workspace::monitor::{WaveformCapture, WaveformScale};
    let frozen = WaveformCapture::samples(
        "source".into(),
        SampleRate::DEFAULT,
        (0, 2),
        &[[0.0; 2]; 2],
        WaveformScale::Fit,
    )
    .unwrap();
    let caption = crate::workspace::monitor::waveform_capture_caption(&(frozen), true, false);
    let prior = frozen.clone();
    let _other = WaveformCapture::samples(
        "other".into(),
        SampleRate::new(8000).unwrap(),
        (3, 5),
        &[[0.1; 2]; 2],
        WaveformScale::Fixed,
    )
    .unwrap();
    assert_eq!(
        crate::workspace::monitor::waveform_capture_caption(&(frozen), true, false),
        caption
    );
    assert_eq!(
        crate::workspace::monitor::waveform_capture_caption(&(prior), true, false),
        caption
    );
    assert!(output_underrun_warning(1).contains("underruns: 1"));
    assert_eq!(
        output_underrun_warning(1),
        "Backend-reported output underruns: 1 · see details."
    );
}
#[test]
fn copied_attribution_and_reimported_notes_reach_library_details() {
    use auditorylab_app::document::recipe::{Origin, RecipeSnapshot};
    let source = auditorylab_app::services::catalog().unwrap().remove(0);
    let mut draft = Draft::from_snapshot(&source).unwrap();
    draft.set_summary("Own notes".into());
    let recipe = draft.recipe().unwrap();
    assert!(crate::recipe::about(&recipe).unwrap().research.is_none());
    let derived = RecipeSnapshot::new(Origin::Stream("derivative".into()), recipe);
    // The copy's ancestor study is not its own research; it appears once as further evidence.
    let details = crate::snapshot_details_at(&(derived), SampleRate::DEFAULT);
    assert!(details.contains("Further evidence (Ancestor") && details.contains("Further source ("));
    assert!(!details.contains("Studied for: Human study: Brain-rhythm response\nStudied for"));
    let mut changed = derived.recipe().clone().into_builder();
    changed.content.lineage[0].note = "reimport-source-marker".into();
    let changed = RecipeSnapshot::new(
        Origin::Stream("derivative".into()),
        changed.build().unwrap(),
    );
    assert!(
        crate::snapshot_details_at(&(changed), SampleRate::DEFAULT)
            .contains("reimport-source-marker")
    );
}

#[test]
fn rows_show_sort_dates_unknown_edit_and_duration() {
    use auditorylab_app::discovery::collection::EntrySummary;
    use auditorylab_app::discovery::query::SortBy;
    use auditorylab_app::document::origin::LibraryKey;
    use auditorylab_app::workspace::LibraryRowView;
    let handles = workspace();
    let handle = handles.selected_entry_handle().unwrap();
    let snapshot = auditorylab_app::services::catalog().unwrap().remove(0);
    let mut entry = EntrySummary::from_snapshot(
        LibraryKey::User(snapshot.recipe().document().id),
        &snapshot,
        None,
    );
    entry.duration_seconds = 60.0;
    entry.created_at = Some("2026-01-01T12:00:00Z".parse().unwrap());
    entry.modified_at = None;
    entry.added_at = Some("2026-10-24T12:00:00Z".parse().unwrap());
    let row = LibraryRowView::Entry {
        handle: handle.clone(),
        entry: Box::new(entry.clone()),
        requires_read: false,
    };
    for sort in [SortBy::Modified, SortBy::Added] {
        let text = crate::library::row(&row, sort);
        assert_eq!(text.key, entry.key.to_string());
        assert!(text.text.contains("Edited: Unknown") && text.text.contains("Added here:"));
        assert!(text.text.contains("Duration 1 min"));
    }
    let text = crate::library::row(&row, SortBy::Created);
    assert!(text.text.contains(&format!(
        "Created: {}",
        crate::library::calendar::local_timestamp(entry.created_at.unwrap())
    )));
    let stale = crate::library::row(
        &LibraryRowView::Entry {
            handle: handle.clone(),
            entry: Box::new(entry),
            requires_read: true,
        },
        SortBy::Modified,
    );
    assert!(stale.text.contains("Needs review"));
}

#[test]
fn operation_and_shutdown_failures_keep_recovery_context() {
    use auditorylab_app::document::origin::LibraryKey;
    use auditorylab_app::services::library::LibraryError;
    use auditorylab_app::workspace::status::{
        FileAction, FileActivity, LibraryContext, LibraryStatus, SaveFeedback,
    };
    use auditorylab_app::workspace::{
        JobContext, ShutdownError, ShutdownIssue, TaskFailure, WorkerFailure,
    };
    let id = auditorylab_app::services::catalog().unwrap()[0]
        .recipe()
        .document()
        .id;
    let key = LibraryKey::User(id);
    let path = PathBuf::from("complete/library/path");
    let context = LibraryContext {
        path: path.clone(),
        selector: Some(key.clone()),
    };
    assert!(
        LibraryError::StaleContent { id }
            .explain()
            .contains("saved recipe changed")
    );
    assert!(
        LibraryError::IdentityExists { id }
            .explain()
            .contains("identity already exists")
    );
    let status = FileStatus::WorkerFailed {
        activity: FileActivity {
            kind: FileAction::Library(auditorylab_app::vocabulary::Operation::LibraryRefresh),
            path: path.clone(),
            title: None,
            may_publish: true,
        },
        error: WorkerFailure::Panicked,
    };
    let outcome = crate::workspace::status::file_status_description(&(status));
    assert!(outcome.contains("thread failed"));
    assert!(outcome.contains("does not establish that storage was unchanged"));
    assert!(outcome.contains(&path.display().to_string()));
    let error = ShutdownError {
        issues: vec![ShutdownIssue::Task {
            error: TaskFailure::Worker(WorkerFailure::InvalidOutcome),
            context: JobContext::Library(context.clone()),
        }],
    }
    .explain();
    assert!(error.contains("another operation"));
    assert!(error.contains("library"));
    let notice = crate::workspace::status::library_status_description(
        &(LibraryStatus::Failed {
            error: LibraryError::StaleContent { id }.into(),
            publishing: true,
            context,
        }),
    );
    assert!(notice.contains(&format!("Selector: {key}")));
    assert!(notice.contains(&format!("Location: {}", path.display())));
    assert!(notice.contains("conflicts with the captured operation"));
    assert!(notice.contains("did not publish a replacement"));
    assert!(notice.contains("Draft retained"));
    assert!(!notice.contains("inspect"));
    let feedback = crate::workspace::status::save_feedback_description(
        &(SaveFeedback::EditorReset(
            auditorylab_app::identity::IdentityError::Exhausted(
                auditorylab_app::identity::IdentityScope::Document,
            )
            .into(),
        )),
    );
    for expected in [
        "Trash confirmed",
        "Open draft retained",
        "dependent action stopped",
    ] {
        assert!(feedback.contains(expected));
    }
    assert!(
        crate::library::document_location(None, Some(&key), None, None)
            .1
            .starts_with("Detached draft")
    );
}

#[test]
fn repeated_views_keep_all_rendered_fields_and_empty_selection_clears_text() {
    let mut ws = workspace();
    let initial = crate::workspace::library_view(&ws.library_view());
    assert_eq!(initial.title, ws.selected().unwrap().title());
    assert!(initial.details.contains(&initial.title));
    let editor = crate::workspace::editor_view(&ws.editor_view());
    let actions = crate::workspace::action_view(&ws.action_view());
    for _ in 0..10 {
        ws.poll();
        assert_eq!(crate::workspace::library_view(&ws.library_view()), initial);
        assert_eq!(crate::workspace::editor_view(&ws.editor_view()), editor);
        assert_eq!(crate::workspace::action_view(&ws.action_view()), actions);
    }
    ws.apply(Command::Library(LibraryCommand::Search(
        ("no matching result").to_string(),
    )))
    .unwrap();
    let empty = crate::workspace::library_view(&ws.library_view());
    assert!(
        ws.library_view().rows.is_empty() && empty.title.is_empty() && empty.details.is_empty()
    );
    assert!(!ws.action_view().can_play());
}

#[test]
fn preview_and_audio_failures_name_the_captured_rate() {
    use auditorylab_app::workspace::output::PreviewIssue;
    let preview = PreviewIssue::Acoustic {
        title: "preview".into(),
        rate: SampleRate::DEFAULT,
        error: None,
    };
    assert!(crate::workspace::status::preview_issue_description(&(preview)).contains("48000 Hz"));
    let error = auditorylab_app::services::LoadError::Sound {
        origin: auditorylab_app::document::recipe::Origin::Stream("tone".into()),
        rate: SampleRate::new(8000).unwrap(),
        error: Box::new(auditorylab_core::SoundError::Duration {
            seconds: 0.0,
            sample_rate_hz: 8000,
        }),
    };
    assert!(
        crate::workspace::status::audio_status_description(
            &(auditorylab_app::workspace::status::AudioStatus::Failed(error.into()))
        )
        .contains("8000")
    );
}

#[test]
fn file_activity_uses_the_captured_shared_operation_term() {
    use auditorylab_app::vocabulary::Operation;
    use auditorylab_app::workspace::status::{FileAction, FileActivity};
    for (kind, operation, label) in [
        (FileAction::Import, Operation::OpenRecipe, "Open recipe"),
        (
            FileAction::RecipeExport,
            Operation::ExportRecipe,
            "Export portable recipe",
        ),
        (FileAction::AudioExport, Operation::Export, "Export WAV"),
        (
            FileAction::Library(Operation::LibraryRead),
            Operation::LibraryRead,
            "Read saved recipe",
        ),
        (
            FileAction::Library(Operation::Save),
            Operation::Save,
            "Save sound",
        ),
        (
            FileAction::Library(Operation::LibraryTrash),
            Operation::LibraryTrash,
            "Move saved entry to Trash",
        ),
    ] {
        let activity = FileActivity {
            kind,
            path: PathBuf::from("captured"),
            title: Some("sound".into()),
            may_publish: false,
        };
        assert_eq!(kind.operation(), operation);
        assert!(
            crate::workspace::status::file_activity_description(&(activity)).starts_with(label)
        );
        assert!(crate::workspace::file_activity(Some(kind)).starts_with(label));
    }
}

fn library_failure_cases() -> [(auditorylab_app::outcome::Outcome, bool, bool, bool); 15] {
    [
        (
            auditorylab_app::outcome::Outcome::HistoryConflict,
            true,
            false,
            true,
        ),
        (
            auditorylab_app::outcome::Outcome::Conflict,
            false,
            false,
            false,
        ),
        (
            auditorylab_app::outcome::Outcome::HistoryConflict,
            false,
            false,
            false,
        ),
        (auditorylab_app::outcome::Outcome::Write, true, true, false),
        (
            auditorylab_app::outcome::Outcome::Unreadable,
            true,
            true,
            false,
        ),
        (
            auditorylab_app::outcome::Outcome::IncompleteInventory,
            true,
            true,
            false,
        ),
        (
            auditorylab_app::outcome::Outcome::Write,
            false,
            false,
            false,
        ),
        (auditorylab_app::outcome::Outcome::Busy, true, false, false),
        (
            auditorylab_app::outcome::Outcome::LockUnavailable,
            true,
            false,
            false,
        ),
        (
            auditorylab_app::outcome::Outcome::Cancelled,
            true,
            false,
            false,
        ),
        (
            auditorylab_app::outcome::Outcome::ReadOnlyBuiltIn,
            true,
            false,
            false,
        ),
        (
            auditorylab_app::outcome::Outcome::InvalidRecipe,
            true,
            false,
            false,
        ),
        (
            auditorylab_app::outcome::Outcome::InvalidSound,
            true,
            false,
            false,
        ),
        (
            auditorylab_app::outcome::Outcome::InvalidArgument,
            true,
            false,
            false,
        ),
        (
            auditorylab_app::outcome::Outcome::Identity,
            true,
            false,
            false,
        ),
    ]
}
