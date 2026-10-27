//! Coordinator semantics with real persistent stores and directly executed file jobs.
use super::*;
use crate::presentation::waveform_view::WaveformScale;
use auditorylab_core::{ComponentId, ParameterId};
use std::sync::Arc;

struct TestWorkspace {
    inner: Workspace,
    store: crate::services::library::Library,
    _root: tempfile::TempDir,
}
impl std::ops::Deref for TestWorkspace {
    type Target = Workspace;
    fn deref(&self) -> &Workspace {
        &self.inner
    }
}
impl std::ops::DerefMut for TestWorkspace {
    fn deref_mut(&mut self) -> &mut Workspace {
        &mut self.inner
    }
}
impl TestWorkspace {
    fn new(catalog: Vec<RecipeSnapshot>, capabilities: Capabilities) -> Result<Self, Error> {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
        std::fs::create_dir_all(&base).unwrap();
        let root = tempfile::tempdir_in(base).unwrap();
        let store = crate::services::library::Library::at_path(root.path().join("library"))?;
        store.initialize()?;
        let open_files = !catalog.is_empty()
            && catalog.iter().all(|snapshot| {
                !matches!(
                    snapshot.origin(),
                    crate::document::recipe::Origin::BuiltIn(_)
                )
            });
        let mut inner = Workspace::with_library(catalog, capabilities, store.clone())?;
        drain_files(&mut inner);
        if open_files {
            inner.set_catalog_query(crate::discovery::collection::CatalogQuery {
                scope: crate::discovery::collection::Scope::OpenFiles,
                ..Default::default()
            })?;
            drain_files(&mut inner);
        }
        Ok(Self {
            inner,
            store,
            _root: root,
        })
    }
    fn poll(&mut self) -> bool {
        drain_files(&mut self.inner);
        self.inner.poll()
    }
    fn select(&mut self, position: usize) {
        let key = self.library_view().rows[position].key.clone();
        self.select_library_key(&key);
    }
    fn visible_count(&self) -> usize {
        self.library_view()
            .rows
            .iter()
            .filter(|row| !row.key.is_empty())
            .count()
    }
}
fn drain_files(workspace: &mut Workspace) {
    loop {
        let effects = workspace.take_effects();
        let mut retained = Vec::new();
        let mut ran = false;
        for effect in effects {
            match effect {
                Effect::Run(job) if job.kind() == JobKind::File => {
                    let response = execute_file(job);
                    assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
                    ran = true;
                }
                effect => retained.push(effect),
            }
        }
        workspace.effects.splice(0..0, retained);
        if !ran {
            break;
        }
    }
}
fn workspace() -> TestWorkspace {
    TestWorkspace::new(
        crate::services::catalog().unwrap(),
        Capabilities::available(),
    )
    .unwrap()
}
fn document(ws: &mut Workspace, frequency: f64) -> (DocumentId, FieldHandle) {
    ws.set_view(View::Workbench);
    let document = ws.editor().document();
    let document_key = ws.editor().document_key();
    ws.add_component(document_key, CarrierKind::Tone).unwrap();
    let id = ws.editor().draft().components()[0].id;
    let handle = ws
        .editor()
        .handle(EditingField::Parameter(id, ParameterId::ToneFrequency))
        .unwrap();
    ws.commit_field(handle, &frequency.to_string()).unwrap();
    (document, handle)
}
#[test]
fn idle_ticks_do_no_evaluation_or_preview_work() {
    let mut ws = workspace();
    document(&mut ws, 440.0);
    ws.poll();
    let view = ws.editor_view();
    let actions = ws.action_view();
    let previews = ws.audio.output.previews;
    let models = ws.audio.output.models;
    let evaluations = ws.editor.session.evaluations();
    let serializations = ws.editor.session.serializations();
    let revision = ws.revision();
    let capture = ws.output_view().capture.unwrap();
    let model = ws.model_view().unwrap();
    for _ in 0..100 {
        assert!(!ws.poll());
    }
    assert_eq!(ws.action_view(), actions);
    assert_eq!(ws.audio.output.previews, previews);
    assert_eq!(ws.audio.output.models, models);
    assert_eq!(ws.editor.session.evaluations(), evaluations);
    assert_eq!(
        ws.editor.session.serializations(),
        serializations,
        "save readiness is checked once per candidate"
    );
    assert!(Arc::ptr_eq(&model, &ws.model_view().unwrap()));
    assert_eq!(ws.revision(), revision);
    assert!(Arc::ptr_eq(&capture, &ws.output_view().capture.unwrap()));
    assert_eq!(ws.editor_view(), view);
    assert_eq!(
        ws.editor.session.evaluations(),
        evaluations,
        "views reuse cached evaluations"
    );
    // A scale change reuses the preview's samples.
    ws.set_waveform(40, WaveformScale::Fit);
    ws.poll();
    assert_eq!(ws.audio.output.previews, previews);
    // A relevant change recomputes once.
    ws.set_waveform(10, WaveformScale::Fit);
    ws.poll();
    ws.poll();
    assert_eq!(ws.audio.output.previews, previews + 1);
}

#[test]
fn invalid_commits_clear_unheld_previews_and_held_captures_stay_frozen() {
    let mut ws = workspace();
    let (_, frequency) = document(&mut ws, 440.0);
    ws.poll();
    let valid = ws.output_view().capture.unwrap();
    ws.stage(frequency, "30000").unwrap();
    ws.poll();
    assert!(
        Arc::ptr_eq(&valid, &ws.output_view().capture.unwrap()),
        "staging keeps the committed preview"
    );
    ws.set_hold(true);
    ws.commit().unwrap();
    ws.poll();
    let held = ws.output_view();
    assert!(held.held);
    assert!(
        Arc::ptr_eq(&valid, &held.capture.unwrap()),
        "held capture keeps its content"
    );
    ws.set_hold(false);
    ws.poll();
    let current = ws.output_view();
    assert!(
        current.capture.is_none(),
        "invalid committed source clears the preview"
    );
    assert!(current.unavailable.unwrap().contains("48000 Hz"));
    let evaluations = ws.editor.session.evaluations();
    for _ in 0..10 {
        ws.poll();
    }
    assert_eq!(
        ws.editor.session.evaluations(),
        evaluations,
        "invalid results are cached"
    );
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
            crate::presentation::workspace_text::received_text(Duration::from_millis(millis), held),
            text
        );
    }
}

#[test]
fn a_search_that_hides_every_recipe_leaves_nothing_to_play_or_export() {
    let mut ws = workspace();
    let chosen = ws.selected().unwrap().id();
    ws.set_query("no recipe has this text");
    assert!(ws.selected().is_none());
    assert!(matches!(ws.play(), Err(Refusal::NoSelection)));
    assert!(matches!(ws.export(), Err(Refusal::NoSelection)));
    assert!(
        ws.take_effects()
            .iter()
            .all(|effect| !matches!(effect, Effect::Run(_))),
        "refused commands never admit jobs"
    );
    assert!(!ws.action_view().can_play);
    assert!(ws.library_view().selected.is_none());
    ws.copy_selected();
    assert_eq!(ws.view(), View::Library, "nothing to copy");
    ws.set_query("");
    assert_eq!(
        ws.selected().unwrap().id(),
        chosen,
        "clearing restores the choice"
    );
}

#[test]
fn suggested_file_names_come_from_titles() {
    assert_eq!(
        super::file_name("5 Hz binaural beat (edited)", "toml"),
        "5 Hz binaural beat (edited).toml"
    );
    assert_eq!(super::file_name("a/b:c*?\"<>|\\d", "wav"), "a b c d.wav");
    assert_eq!(super::file_name("  \t ", "toml"), "sound.toml");
    assert_eq!(super::file_name("...", "toml"), "sound.toml");
    let long = super::file_name(&"x".repeat(500), "wav");
    assert_eq!(long.chars().count(), 84);
}

#[test]
fn readiness_is_pure_and_failed_preparation_keeps_all_staged_text() {
    let mut ws = workspace();
    let (_, frequency) = document(&mut ws, 440.0);
    ws.stage(frequency, "30000").unwrap();
    let revision = ws.revision();
    let committed = ws.editor.session.draft_revision();
    let input = ws.editor.session.input_revision();
    let next_id = ws.next_id;
    let effects = ws.take_effects();
    for _ in 0..10 {
        let actions = ws.action_view();
        assert!(actions.can_save);
        assert!(!actions.can_export);
    }
    assert_eq!(ws.next_id, next_id);
    assert_eq!(ws.revision(), revision);
    assert_eq!(ws.editor.session.draft_revision(), committed);
    assert_eq!(ws.editor.session.input_revision(), input);
    assert!(ws.take_effects().is_empty());
    assert!(!ws.audio_busy() && !ws.file_busy());
    assert!(ws.export().is_err());
    assert_eq!(ws.editor.session.staged(frequency.field), Some("30000"));
    assert_eq!(ws.editor.session.draft_revision(), committed);
    drop(effects);
    let notes = ws
        .editor
        .session
        .handle(EditingField::Metadata(
            crate::document::editing::MetadataField::Summary,
        ))
        .unwrap();
    ws.stage(notes, &"🎧".repeat(Recipe::MAX_BYTES)).unwrap();
    assert!(ws.save().is_err());
    assert!(ws.editor.session.staged(notes.field).is_some());
    assert_eq!(ws.editor.session.draft_revision(), committed);
}

#[test]
fn library_dependencies_exclude_editor_and_telemetry_but_include_selection_query_and_rate() {
    let mut ws = workspace();
    let initial = ws.library_view();
    let evaluations = ws.library.evaluations.get();
    let (_, field) = document(&mut ws, 440.0);
    ws.stage(field, "880").unwrap();
    ws.action_view();
    ws.editor_view();
    ws.poll();
    assert_eq!(ws.library_view(), initial);
    assert_eq!(ws.library.evaluations.get(), evaluations);
    ws.select(1);
    assert_ne!(ws.library_view().title, initial.title);
    assert_eq!(ws.library.evaluations.get(), evaluations + 1);
    ws.set_query("no matching result");
    let empty = ws.library_view();
    assert!(empty.rows.is_empty() && empty.title.is_empty() && empty.details.is_empty());
    ws.set_query("");
    ws.set_export_rate(SampleRate::new(96000).unwrap());
    assert!(!ws.library_view().rows.is_empty());
    assert_eq!(ws.library.evaluations.get(), evaluations + 3);
}

#[test]
fn reopened_copy_navigation_keeps_declared_ancestry_without_reconstructing_original_audio() {
    let dir = tempfile::tempdir_in(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp"),
    )
    .unwrap();
    let path = dir.path().join("original.toml");
    let original = crate::services::catalog().unwrap().remove(0);
    crate::services::save_content(original.recipe(), &path, crate::services::Replace::Never)
        .unwrap();
    let loaded = crate::storage::load::load_file(&path).unwrap();
    let mut draft = Draft::from_snapshot(&loaded).unwrap();
    draft.set_summary("Own notes".into());
    let derivative = dir.path().join("derivative.toml");
    crate::services::save_content(
        &draft.recipe().unwrap(),
        &derivative,
        crate::services::Replace::Never,
    )
    .unwrap();
    std::fs::remove_file(path).unwrap();
    let reopened = crate::storage::load::load_file(&derivative).unwrap();
    let mut ws = workspace();
    ws.replace_document(Draft::from_snapshot(&reopened).unwrap());
    ws.show_based_on_source();
    assert_eq!(
        ws.selected().unwrap().recipe(),
        reopened.recipe(),
        "navigation addresses the captured copied-from document, not a fabricated ancestor waveform"
    );
    assert_eq!(
        ws.selected().unwrap().recipe().lineage()[0].document_id,
        Some(original.recipe().document().id)
    );
    assert_eq!(
        ws.selected().unwrap().recipe().sources(),
        original.recipe().sources()
    );
    assert!(
        ws.selected()
            .unwrap()
            .recipe()
            .evidence()
            .iter()
            .all(|e| e.scope == crate::core::EvidenceScope::Ancestor)
    );
    assert_eq!(ws.editor.session.draft().summary(), "Own notes");
}

#[test]
fn search_includes_own_notes_and_attributed_persisted_source_context() {
    let source = crate::services::catalog().unwrap().remove(0);
    let citation = source
        .about()
        .unwrap()
        .research
        .as_ref()
        .unwrap()
        .citation
        .clone()
        .unwrap();
    let mut draft = Draft::from_snapshot(&source).unwrap();
    draft.set_summary("unique-own-notes-marker 🎧".into());
    let bytes = draft.to_toml().unwrap();
    let reopened =
        crate::storage::load::load_reader(bytes.as_bytes(), "reopened derivative").unwrap();
    let mut ws = TestWorkspace::new(vec![reopened], Capabilities::no_device_playback()).unwrap();
    for query in ["unique-own-notes-marker", &citation] {
        ws.set_query(query);
        assert_eq!(ws.visible_count(), 1, "{query}");
        assert!(ws.library_view().details.contains("Source citation:"));
        assert!(ws.selected().unwrap().about().unwrap().research.is_none());
    }
    ws.set_query("not-present-anywhere-marker");
    assert!((ws.visible_count() == 0));
    let mut custom = Draft::new().unwrap();
    custom.set_summary("custom-notes-marker".into());
    ws.add_import(custom.snapshot().unwrap()).unwrap();
    ws.set_query("custom-notes-marker");
    assert_eq!(ws.visible_count(), 1);
    ws.set_query("unique-own-notes-marker");
    assert_eq!(ws.visible_count(), 1);
    ws.add_import(crate::document::recipe::RecipeSnapshot::new(
        crate::document::recipe::Origin::Stream("reopened derivative".into()),
        custom.recipe().unwrap(),
    ))
    .unwrap();
    ws.set_query("unique-own-notes-marker");
    // A reimport under the same origin must remove stale search text.
    assert_eq!(ws.visible_count(), 0);
    let mut direct = TestWorkspace::new(vec![source], Capabilities::no_device_playback()).unwrap();
    direct.set_query(&citation);
    assert_eq!(direct.visible_count(), 1);
}

#[test]
fn readiness_details_and_export_share_actual_snapshot_realizations_including_errors() {
    let mut ws = TestWorkspace::new(
        crate::services::catalog().unwrap(),
        Capabilities::no_device_playback(),
    )
    .unwrap();
    let selected = ws.selected().unwrap().clone();
    let _ = ws.library_view();
    let count = selected.realization_count();
    assert_eq!(
        count, 1,
        "rows, structure and details share the same actual compile"
    );
    for _ in 0..10 {
        ws.action_view();
        let _ = ws.library_view();
        ws.set_hold(true);
        ws.set_hold(false);
    }
    assert_eq!(selected.realization_count(), count);
    ws.export().unwrap();
    let prompt = ws.prompts.current.as_ref().unwrap().id;
    ws.respond(Response::Prompt {
        prompt,
        answer: PromptAnswer::Path(None),
    });
    assert_eq!(selected.realization_count(), count);
    ws.set_export_rate(SampleRate::new(96000).unwrap());
    ws.action_view();
    assert_eq!(selected.realization_count(), count + 1);
    ws.set_export_rate(SampleRate::DEFAULT);
    ws.action_view();
    assert_eq!(selected.realization_count(), count + 1);
    let mut invalid = Draft::new().unwrap();
    invalid.set_duration_seconds(-1.0);
    let snapshot = invalid.snapshot().unwrap();
    let mut bad =
        TestWorkspace::new(vec![snapshot.clone()], Capabilities::no_device_playback()).unwrap();
    for _ in 0..10 {
        assert!(!bad.action_view().can_export);
        let _ = bad.library_view();
    }
    assert_eq!(
        snapshot.realization_count(),
        1,
        "failures cache at the real compile path too"
    );
    for hz in [96000, 192000, 44100] {
        snapshot.at(SampleRate::new(hz).unwrap()).unwrap_err();
    }
    assert_eq!(snapshot.cached_rates(), 2);
    let replacement = crate::document::recipe::RecipeSnapshot::new(
        crate::document::recipe::Origin::File("same.toml".into()),
        selected.recipe().clone(),
    );
    let mut imported = TestWorkspace::new(
        vec![replacement.clone()],
        Capabilities::no_device_playback(),
    )
    .unwrap();
    imported.action_view();
    assert_eq!(replacement.realization_count(), 1);
    let fresh = crate::document::recipe::RecipeSnapshot::new(
        crate::document::recipe::Origin::File("same.toml".into()),
        selected.recipe().clone(),
    );
    imported.add_import(fresh.clone()).unwrap();
    imported.action_view();
    assert_eq!(
        fresh.realization_count(),
        1,
        "reimports cannot reuse another identity's result"
    );
    assert_eq!(replacement.realization_count(), 1);
}

#[test]
fn search_retains_flat_ancestor_notes_and_reimport_invalidates_old_metadata() {
    let mut original = Draft::new().unwrap();
    original.set_summary("original-custom-note-marker".into());
    let mut edited = Draft::from_snapshot(&original.snapshot().unwrap()).unwrap();
    edited.set_summary("current-own-notes-marker".into());
    let reopened =
        crate::storage::load::load_reader(edited.to_toml().unwrap().as_bytes(), "derivative")
            .unwrap();
    let recipe = reopened.recipe().clone();
    let first = RecipeSnapshot::new(
        crate::document::recipe::Origin::File("same.toml".into()),
        recipe.clone(),
    );
    let mut ws =
        TestWorkspace::new(vec![first.clone()], Capabilities::no_device_playback()).unwrap();
    ws.set_query("original-custom-note-marker");
    assert_eq!(ws.visible_count(), 1);
    let initial = first.realization_count();
    let mut changed = recipe.into_builder();
    changed.lineage[0].note = "reimport-source-marker".into();
    let replacement = RecipeSnapshot::new(
        crate::document::recipe::Origin::File("same.toml".into()),
        changed.build().unwrap(),
    );
    ws.add_import(replacement.clone()).unwrap();
    ws.set_query("original-custom-note-marker");
    assert!(ws.library_view().rows.is_empty());
    ws.set_query("reimport-source-marker");
    assert_eq!(ws.visible_count(), 1);
    assert!(ws.library_view().details.contains("reimport-source-marker"));
    ws.action_view();
    assert_eq!(replacement.realization_count(), 1);
    assert_eq!(first.realization_count(), initial);
}

#[test]
fn over_budget_copy_content_is_refused_before_becoming_a_snapshot() {
    let mut candidate = Draft::new().unwrap().recipe().unwrap().into_builder();
    candidate.sound.components = (0..17)
        .map(|i| crate::core::Component {
            id: ComponentId::numbered(i).unwrap(),
            form: crate::core::ComponentForm::Voice(crate::document::controls::default_component(
                CarrierKind::Tone,
            )),
        })
        .collect();
    assert!(candidate.build().is_err());
}

#[test]
fn copy_inputs_reject_nonfinite_construction_and_full_lineage() {
    let mut source = Draft::new().unwrap().recipe().unwrap().into_builder();
    source.sound.components.push(crate::core::Component {
        id: ComponentId::new(1).unwrap(),
        form: crate::core::ComponentForm::Voice(crate::core::Voice {
            carrier: crate::core::Carrier::Tone {
                frequency_hz: f64::NAN,
            },
            ..crate::document::controls::default_component(CarrierKind::Tone)
        }),
    });
    assert!(source.clone().build().is_err());
    source.sound.components[0]
        .form
        .write(auditorylab_core::ParameterId::ToneFrequency, 440.0);
    source.sound.duration_seconds = -1.0;
    let finite = RecipeSnapshot::new(
        crate::document::recipe::Origin::Stream("finite invalid".into()),
        source.build().unwrap(),
    );
    assert!(Workspace::copy_content_admission(&finite).is_ok());
    let mut full = finite.recipe().clone().into_builder();
    full.lineage = vec![
        crate::core::Lineage {
            title: "ancestor".into(),
            note: "attributed".into(),
            document_id: None,
            catalog_id: None,
            source_keys: Vec::new()
        };
        16
    ];
    let full = RecipeSnapshot::new(
        crate::document::recipe::Origin::Stream("full ancestry".into()),
        full.build().unwrap(),
    );
    assert!(Workspace::copy_content_admission(&full).is_err());
}

#[test]
fn global_summary_preserves_full_refusal_and_outcome_independent_of_editor_scroll() {
    let mut ws = workspace();
    ws.set_view(View::Workbench);
    let notes = ws
        .editor
        .session
        .handle(EditingField::Metadata(
            crate::document::editing::MetadataField::Summary,
        ))
        .unwrap();
    ws.stage(notes, &"Z".repeat(66000)).unwrap();
    let actions = ws.action_view();
    assert!(!actions.can_save);
    assert!(actions.operation_summary.contains("65536"));
    ws.files.status = FileStatus::Exported {
        path: PathBuf::from("capture.wav"),
        exported: crate::storage::wav::Exported {
            frames: 100,
            sample_rate: SampleRate::DEFAULT,
            container: crate::storage::wav::Container::Riff,
            warnings: Vec::new(),
        },
    };
    let actions = ws.action_view();
    assert!(actions.operation_summary.contains("65536"));
    assert!(actions.operation_outcome.starts_with("Exported 100"));
    assert!(actions.operation_summary.chars().count() <= 181);
    ws.editor.session.restore(notes).unwrap();
    ws.files.status = FileStatus::Failed(
        crate::error::ExportError::File {
            error: crate::error::FileError::Io(crate::error::IoFailure::new(
                crate::error::IoOperation::Write,
                Some(std::path::PathBuf::from("deep/path/".repeat(50))),
                std::io::Error::other("disk full"),
            )),
        }
        .into(),
    );
    let actions = ws.action_view();
    assert!(actions.operation_details.contains("cannot write file"));
    assert!(actions.operation_details.len() > actions.operation_summary.len());
}

#[test]
fn nominal_listing_a_thousand_rows_does_not_compile_every_sound() {
    let base = crate::services::builtin("ioannou-2015-binaural-2-hz").unwrap();
    let snapshots = (0..1000)
        .map(|i| {
            RecipeSnapshot::new(
                crate::document::recipe::Origin::Stream(format!("fixture-{i}")),
                base.recipe().clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut ws = TestWorkspace::new(snapshots.clone(), Capabilities::no_device_playback()).unwrap();
    ws.set_rhythm_query(crate::discovery::query::RhythmQuery {
        bands: vec![crate::core::rhythm::Band::Delta],
        ..Default::default()
    });
    assert_eq!(ws.library_view().rows.len(), 1000);
    assert_eq!(
        snapshots
            .iter()
            .map(RecipeSnapshot::realization_count)
            .sum::<u64>(),
        1,
        "only the selected detail is compiled"
    );
    ws.set_export_rate(SampleRate::new(96_000).unwrap());
    assert_eq!(ws.library_view().rows.len(), 1000);
    assert_eq!(
        snapshots
            .iter()
            .map(RecipeSnapshot::realization_count)
            .sum::<u64>(),
        2
    );
}

#[test]
fn reimports_get_new_identities_and_based_on_details_use_the_retained_snapshot() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    let dir = tempfile::tempdir_in(base).unwrap();
    let path = dir.path().join("same.toml");
    let text = |duration, title: &str| {
        let mut recipe = crate::test_recipes::silent(duration, 0.0).into_builder();
        recipe.description = Some(crate::core::Description {
            title: title.into(),
            notes: None,
        });
        recipe.build().unwrap().to_toml().unwrap()
    };
    let first_text = text(1.0, "First");
    let second_text = text(2.0, "Second");
    std::fs::write(&path, &first_text).unwrap();
    let mut ws = workspace();
    for text in [&first_text, &second_text] {
        std::fs::write(&path, text).unwrap();
        ws.import().unwrap();
        let Effect::ChoosePath { prompt, .. } = ws.take_effects()[0] else {
            panic!()
        };
        ws.respond(Response::Prompt {
            prompt,
            answer: PromptAnswer::Path(Some(path.clone())),
        });
        drain_files(&mut ws);
        if text == &first_text {
            ws.copy_selected();
            assert!(matches!(
                ws.take_effects().as_slice(),
                [Effect::ShowView(View::Workbench)]
            ));
        }
    }
    let first_copy = ws.editor().draft().based_on().unwrap().clone();
    assert_eq!(first_copy.title(), "First");
    let current = ws.selected().unwrap().clone();
    assert_eq!(current.title(), "Second");
    assert_ne!(current.id(), first_copy.id(), "same path, new identity");
    assert_eq!(
        ws.catalog().len(),
        crate::services::catalog().unwrap().len() + 1,
        "row replaced in place"
    );
    ws.show_based_on_source();
    assert_eq!(
        ws.selected().unwrap().id(),
        first_copy.id(),
        "not resolved by path"
    );
    assert_eq!(ws.view(), View::Library);
}

fn take_file_job(workspace: &mut Workspace) -> Job {
    let mut captured = None;
    let mut retained = Vec::new();
    for effect in workspace.take_effects() {
        match effect {
            Effect::Run(job) if job.kind() == JobKind::File => {
                assert!(captured.is_none(), "one file slot admits one immutable job");
                captured = Some(job);
            }
            effect => retained.push(effect),
        }
    }
    workspace.effects.extend(retained);
    captured.expect("the real command admitted a file job")
}

fn execute_file(job: Job) -> Response {
    assert_eq!(job.kind(), JobKind::File);
    job.execute(|_| panic!("a file job cannot prepare audio"))
}

fn take_path_prompt(workspace: &mut Workspace) -> (PromptId, PathPurpose, Option<String>) {
    let mut captured = None;
    let mut retained = Vec::new();
    for effect in workspace.take_effects() {
        match effect {
            Effect::ChoosePath {
                prompt,
                purpose,
                suggested_name,
            } => {
                assert!(captured.is_none(), "one native panel at a time");
                captured = Some((prompt, purpose, suggested_name));
            }
            effect => retained.push(effect),
        }
    }
    workspace.effects.extend(retained);
    captured.expect("the command requested a native file panel")
}

fn answer_path(workspace: &mut Workspace, purpose: PathPurpose, path: Option<PathBuf>) -> PromptId {
    let (prompt, actual, _) = take_path_prompt(workspace);
    assert_eq!(actual, purpose);
    assert_eq!(
        workspace.respond(Response::Prompt {
            prompt,
            answer: PromptAnswer::Path(path)
        }),
        ResponseDisposition::Applied
    );
    prompt
}

fn take_unsaved_prompt(workspace: &mut Workspace) -> PromptId {
    let effects = workspace.take_effects();
    let [Effect::AskUnsaved(prompt)] = effects.as_slice() else {
        panic!("one unsaved-changes prompt: {effects:?}");
    };
    *prompt
}

fn assert_close_resolution(workspace: &mut Workspace, approved: bool) {
    let effects = workspace.take_effects();
    assert!(
        matches!(effects.as_slice(), [Effect::CloseResolved { approved: actual }] if *actual == approved),
        "{effects:?}"
    );
}

fn temporary_directory() -> tempfile::TempDir {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    tempfile::tempdir_in(base).unwrap()
}

fn short_silent_source() -> RecipeSnapshot {
    let mut draft = Draft::new().unwrap();
    draft.set_duration_seconds(0.01);
    draft.set_fade_seconds(0.0);
    draft.snapshot().unwrap()
}

#[test]
fn captured_managed_save_is_sans_io_and_later_edits_stay_dirty() {
    let mut workspace = workspace();
    let (_, field) = document(&mut workspace, 440.0);
    let captured = workspace.editor().draft().recipe().unwrap();
    workspace.save().unwrap();
    let job = take_file_job(&mut workspace);
    let identity = job.id();
    assert!(workspace.file_busy());
    assert!(
        workspace.store.scan(&Cancel::default()).entries.is_empty(),
        "admission itself performs no disk IO"
    );
    assert!(matches!(workspace.import(), Err(Refusal::FileBusy)));
    assert!(matches!(workspace.save(), Err(Refusal::FileBusy)));
    assert!(matches!(workspace.export(), Err(Refusal::FileBusy)));
    workspace.commit_field(field, "880").unwrap();
    for _ in 0..10 {
        workspace.inner.poll();
    }
    assert!(workspace.file_busy());
    assert!(workspace.store.scan(&Cancel::default()).entries.is_empty());
    let response = execute_file(job);
    let key = crate::storage::library::LibraryKey::User(captured.document().id);
    let saved = workspace.store.load(&key).unwrap();
    assert_eq!(
        saved.snapshot.sound().components[0]
            .form
            .read(ParameterId::ToneFrequency),
        Some(440.0)
    );
    assert!(
        workspace.file_busy(),
        "published bytes alone do not release the admitted slot"
    );
    assert!(workspace.editor().dirty());
    assert!(matches!(&response, Response::Finished { job, .. } if *job == identity));
    assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
    assert!(!workspace.file_busy());
    assert!(
        workspace.editor().dirty(),
        "the saved baseline is the captured 440 Hz version"
    );
    assert_eq!(
        workspace.editor().draft().components()[0]
            .form
            .read(ParameterId::ToneFrequency),
        Some(880.0)
    );
    assert_eq!(
        workspace
            .editor()
            .draft()
            .recipe()
            .unwrap()
            .document()
            .created_at,
        saved.snapshot.recipe().document().created_at
    );
}

#[test]
fn captured_save_completion_never_replaces_a_different_editor_document() {
    let mut workspace = workspace();
    document(&mut workspace, 440.0);
    let captured = workspace.editor().draft().recipe().unwrap();
    workspace.save().unwrap();
    let job = take_file_job(&mut workspace);
    // The identity guard must also protect an independently replaced embedding/editor session.
    let mut replacement = Draft::new().unwrap();
    replacement.set_summary("Replacement content must stay independent".into());
    assert!(workspace.replace_document(replacement));
    let replacement_id = workspace.editor().document();
    let before = workspace.editor().draft().recipe().unwrap();
    let dirty = workspace.editor().dirty();
    assert_eq!(
        workspace.respond(execute_file(job)),
        ResponseDisposition::Applied
    );
    assert_eq!(workspace.editor().document(), replacement_id);
    assert_eq!(workspace.editor().draft().recipe().unwrap(), before);
    assert_eq!(workspace.editor().dirty(), dirty);
    assert!(matches!(
        workspace.editor.location,
        DocumentLocation::Unsaved
    ));
    let saved = workspace
        .store
        .load(&crate::storage::library::LibraryKey::User(
            captured.document().id,
        ))
        .unwrap();
    assert_eq!(
        saved.snapshot.sound().components[0]
            .form
            .read(ParameterId::ToneFrequency),
        Some(440.0)
    );
}

#[test]
fn save_before_close_waits_for_the_captured_job_completion() {
    let mut workspace = workspace();
    document(&mut workspace, 440.0);
    let captured = workspace.editor().draft().recipe().unwrap();
    workspace.close();
    workspace.close();
    let prompt = take_unsaved_prompt(&mut workspace);
    assert_eq!(
        workspace.respond(Response::Prompt {
            prompt,
            answer: PromptAnswer::Save
        }),
        ResponseDisposition::Applied
    );
    let job = take_file_job(&mut workspace);
    assert!(workspace.action_pending());
    assert!(matches!(workspace.import(), Err(Refusal::ActionPending)));
    let response = execute_file(job);
    assert!(
        workspace
            .store
            .load(&crate::storage::library::LibraryKey::User(
                captured.document().id
            ))
            .is_ok()
    );
    for _ in 0..10 {
        workspace.inner.poll();
    }
    assert!(workspace.file_busy());
    assert!(workspace.action_pending());
    assert!(
        workspace.take_effects().is_empty(),
        "publication is not an executor completion"
    );
    assert!(workspace.editor().dirty());
    assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
    assert!(!workspace.file_busy());
    assert!(!workspace.action_pending());
    assert!(!workspace.editor().dirty());
    assert_close_resolution(&mut workspace, true);
}

#[test]
fn failed_captured_save_denies_close_and_preserves_foreign_storage() {
    let mut workspace = workspace();
    document(&mut workspace, 440.0);
    let captured = workspace.editor().draft().recipe().unwrap();
    workspace.close();
    let prompt = take_unsaved_prompt(&mut workspace);
    workspace.respond(Response::Prompt {
        prompt,
        answer: PromptAnswer::Save,
    });
    let job = take_file_job(&mut workspace);
    let path = workspace
        .store
        .path()
        .join("recipes")
        .join(format!("{}.toml", captured.document().id));
    std::fs::write(&path, b"foreign malformed content must remain").unwrap();
    assert_eq!(
        workspace.respond(execute_file(job)),
        ResponseDisposition::Applied
    );
    assert!(!workspace.file_busy());
    assert!(!workspace.action_pending());
    assert!(workspace.editor().dirty());
    assert_eq!(
        std::fs::read(path).unwrap(),
        b"foreign malformed content must remain"
    );
    assert!(matches!(
        &workspace.files.status,
        FileStatus::Library(LibraryStatus::Failed {
            publishing: true,
            ..
        })
    ));
    assert_close_resolution(&mut workspace, false);
}

#[test]
fn one_prompt_coalesces_quit_and_ignores_an_older_real_panel_answer() {
    let mut workspace = workspace();
    workspace.import().unwrap();
    let stale = answer_path(&mut workspace, PathPurpose::OpenRecipe, None);
    document(&mut workspace, 440.0);
    workspace.close();
    workspace.close();
    let prompt = take_unsaved_prompt(&mut workspace);
    workspace.new_document();
    assert!(workspace.take_effects().is_empty());
    assert!(matches!(
        &workspace.files.status,
        FileStatus::Refused(Refusal::PromptOpen)
    ));
    assert!(
        matches!(workspace.stop(), Err(Refusal::NoAudio)),
        "Stop admission is independent of a native panel"
    );
    assert_eq!(
        workspace.respond(Response::Prompt {
            prompt: stale,
            answer: PromptAnswer::Discard
        }),
        ResponseDisposition::Ignored
    );
    assert!(workspace.prompt_open());
    assert_eq!(
        workspace.respond(Response::Prompt {
            prompt,
            answer: PromptAnswer::Failed("panel unavailable".into())
        }),
        ResponseDisposition::Applied
    );
    assert!(!workspace.prompt_open());
    assert!(workspace.editor().dirty());
    assert_close_resolution(&mut workspace, false);
    assert_eq!(
        workspace.respond(Response::Prompt {
            prompt,
            answer: PromptAnswer::Discard
        }),
        ResponseDisposition::Ignored
    );
    assert!(workspace.take_effects().is_empty());
}

#[test]
fn invalid_staged_input_denies_save_before_close_without_starting_a_job() {
    let mut workspace = workspace();
    let (_, field) = document(&mut workspace, 440.0);
    workspace.stage(field, "not a frequency").unwrap();
    workspace.close();
    let prompt = take_unsaved_prompt(&mut workspace);
    assert_eq!(
        workspace.respond(Response::Prompt {
            prompt,
            answer: PromptAnswer::Save
        }),
        ResponseDisposition::Applied
    );
    assert_eq!(
        workspace.editor().staged(field.field),
        Some("not a frequency")
    );
    assert!(workspace.editor().dirty());
    assert!(!workspace.file_busy());
    assert!(workspace.store.scan(&Cancel::default()).entries.is_empty());
    assert_close_resolution(&mut workspace, false);
    workspace.close();
    let prompt = take_unsaved_prompt(&mut workspace);
    workspace.respond(Response::Prompt {
        prompt,
        answer: PromptAnswer::Discard,
    });
    assert_close_resolution(&mut workspace, true);
}

#[test]
fn actual_import_completion_controls_dependent_close_and_never_runs_on_poll() {
    for exists in [false, true] {
        let mut workspace = workspace();
        let directory = temporary_directory();
        let path = directory.path().join("chosen.toml");
        if exists {
            crate::services::save_content(
                short_silent_source().recipe(),
                &path,
                crate::services::Replace::Never,
            )
            .unwrap();
        }
        let count = workspace.catalog().len();
        workspace.import().unwrap();
        assert!(matches!(workspace.save(), Err(Refusal::PromptOpen)));
        answer_path(&mut workspace, PathPurpose::OpenRecipe, Some(path.clone()));
        let job = take_file_job(&mut workspace);
        assert!(
            !workspace.can_edit(),
            "admitted import excludes editor changes"
        );
        assert!(matches!(workspace.import(), Err(Refusal::FileBusy)));
        workspace.close();
        assert!(workspace.action_pending());
        for _ in 0..10 {
            workspace.inner.poll();
        }
        assert_eq!(workspace.catalog().len(), count);
        assert!(workspace.take_effects().is_empty());
        let response = execute_file(job);
        assert_eq!(
            workspace.catalog().len(),
            count,
            "IO does not mutate the coordinator before its completion response"
        );
        assert!(workspace.file_busy());
        assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
        assert!(!workspace.file_busy());
        assert_eq!(workspace.catalog().len(), count + usize::from(exists));
        assert_close_resolution(&mut workspace, exists);
    }
}

#[test]
fn portable_export_keeps_the_prompt_capture_and_does_not_advance_the_save_baseline() {
    let mut workspace = workspace();
    let (_, field) = document(&mut workspace, 440.0);
    let identity = workspace.editor().draft().recipe().unwrap().document().id;
    let directory = temporary_directory();
    let path = directory.path().join("portable.toml");
    workspace.export_recipe().unwrap();
    assert!(matches!(
        workspace.commit_field(field, "880"),
        Err(Refusal::PromptOpen)
    ));
    answer_path(
        &mut workspace,
        PathPurpose::ExportRecipe,
        Some(path.clone()),
    );
    let job = take_file_job(&mut workspace);
    workspace.commit_field(field, "880").unwrap();
    let response = execute_file(job);
    let saved = crate::storage::load::load_file(&path).unwrap();
    assert_eq!(saved.recipe().document().id, identity);
    assert_eq!(
        saved.sound().components[0]
            .form
            .read(ParameterId::ToneFrequency),
        Some(440.0)
    );
    assert!(workspace.file_busy());
    assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
    assert!(workspace.editor().dirty());
    assert!(matches!(
        workspace.editor.location,
        DocumentLocation::Unsaved
    ));
    assert!(
        workspace.store.scan(&Cancel::default()).entries.is_empty(),
        "portable export does not create managed membership"
    );
}

#[test]
fn canceled_real_exports_cleanup_and_close_treats_requested_cancellation_as_expected() {
    for close in [false, true] {
        let mut workspace =
            TestWorkspace::new(vec![short_silent_source()], Capabilities::available()).unwrap();
        let directory = temporary_directory();
        let path = directory.path().join("cancelled.wav");
        workspace.export().unwrap();
        assert!(matches!(workspace.play(), Err(Refusal::PromptOpen)));
        answer_path(&mut workspace, PathPurpose::ExportWav, Some(path.clone()));
        let job = take_file_job(&mut workspace);
        assert!(matches!(workspace.play(), Err(Refusal::Exporting)));
        assert!(matches!(workspace.import(), Err(Refusal::FileBusy)));
        if close {
            workspace.close();
        } else {
            workspace.cancel_export().unwrap();
        }
        let response = execute_file(job);
        assert!(!path.exists());
        assert!(workspace.file_busy());
        assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
        assert!(!workspace.file_busy());
        assert!(!workspace.audio_busy());
        assert!(matches!(
            workspace.files.status,
            FileStatus::Notice(FileNotice::ExportCancelled)
        ));
        if close {
            assert_close_resolution(&mut workspace, true);
        } else {
            assert!(workspace.take_effects().is_empty());
        }
        assert!(matches!(workspace.cancel_export(), Err(Refusal::NoExport)));
    }
}

#[test]
fn a_close_request_after_export_publication_does_not_unpublish_the_real_file() {
    let mut workspace =
        TestWorkspace::new(vec![short_silent_source()], Capabilities::available()).unwrap();
    let directory = temporary_directory();
    let path = directory.path().join("complete.wav");
    workspace.export().unwrap();
    answer_path(&mut workspace, PathPurpose::ExportWav, Some(path.clone()));
    let job = take_file_job(&mut workspace);
    let response = execute_file(job);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(bytes.len(), 58 + 480 * 8);
    assert!(workspace.file_busy());
    workspace.close();
    assert!(workspace.action_pending());
    assert!(workspace.take_effects().is_empty());
    assert_eq!(workspace.respond(response), ResponseDisposition::Applied);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert!(matches!(
        workspace.files.status,
        FileStatus::Exported { .. }
    ));
    assert_close_resolution(&mut workspace, true);
}

#[test]
fn managed_save_can_run_beside_admitted_audio_and_stop_does_not_cancel_it() {
    let mut workspace = workspace();
    document(&mut workspace, 440.0);
    let identity = workspace.editor().draft().recipe().unwrap().document().id;
    workspace.play().unwrap();
    let effects = workspace.take_effects();
    let mut jobs = effects.into_iter().filter_map(|effect| match effect {
        Effect::Run(job) => Some(job),
        _ => None,
    });
    let audio = jobs
        .next()
        .expect("the command captured audio without starting a device");
    assert!(
        jobs.next().is_none(),
        "one audio admission produces one job"
    );
    assert_eq!(audio.kind(), JobKind::Audio);
    // Audio is deliberately never executed: this test opens no output device.
    assert!(workspace.audio_busy());
    assert!(matches!(workspace.export(), Err(Refusal::AudioPlaying)));
    workspace.save().unwrap();
    let save = take_file_job(&mut workspace);
    assert!(workspace.file_busy());
    assert!(matches!(workspace.import(), Err(Refusal::FileBusy)));
    assert!(matches!(workspace.stop(), Ok(StopRequest::Accepted)));
    assert_eq!(
        workspace.respond(execute_file(save)),
        ResponseDisposition::Applied
    );
    assert!(!workspace.file_busy());
    assert!(
        workspace.audio_busy(),
        "an admitted audio job remains owned until an executor completion"
    );
    assert!(
        workspace
            .store
            .load(&crate::storage::library::LibraryKey::User(identity))
            .is_ok()
    );
    assert!(!workspace.editor().dirty());
    drop(audio);
}

#[test]
fn committed_brief_never_borrows_pending_pair_facts() {
    let mut ws = workspace();
    ws.set_view(View::Workbench);
    let key = ws.editor().document_key();
    ws.add_pair(key, crate::core::PairKind::Binaural).unwrap();
    let id = ws.editor().draft().components()[0].id;
    let first = ws
        .editor()
        .handle(EditingField::Parameter(id, ParameterId::PairFirstFrequency))
        .unwrap();
    let offset = ws
        .editor()
        .handle(EditingField::Parameter(id, ParameterId::PairOffset))
        .unwrap();
    ws.commit_field(first, "317").unwrap();
    ws.commit_field(offset, "-7").unwrap();
    let original = ws.editor_view().brief;
    assert!(original.contains("7 Hz") && original.contains("317") && original.contains("310"));
    for text in ["40", "NaN"] {
        ws.stage(offset, text).unwrap();
        let view = ws.editor_view();
        assert_eq!(
            view.brief,
            original.replace("Draft classification", "Last committed classification")
        );
        assert!(!view.brief.contains("357") && !view.brief.contains("No valid sound"));
        assert_eq!(
            ws.editor().draft().components()[0]
                .form
                .pair()
                .unwrap()
                .offset_hz,
            -7.0
        );
        assert_eq!(ws.restore(offset).unwrap(), "-7");
        assert_eq!(ws.editor_view().brief, original);
    }
    ws.stage(offset, "40").unwrap();
    ws.commit().unwrap();
    let committed = ws.editor_view().brief;
    assert!(committed.contains("40 Hz") && committed.contains("357"));
    assert!(!committed.contains("Last committed"));
}

#[test]
fn action_budget_messages_distinguish_components_from_expanded_voices() {
    for (primitives, pairs, components, voices) in [(0, 8, 8, 16), (16, 0, 16, 16), (15, 0, 15, 15)]
    {
        let mut ws = workspace();
        ws.set_view(View::Workbench);
        let key = ws.editor().document_key();
        for _ in 0..primitives {
            ws.add_component(key, CarrierKind::Tone).unwrap();
        }
        for _ in 0..pairs {
            ws.add_pair(key, crate::core::PairKind::Binaural).unwrap();
        }
        let view = ws.action_view();
        assert!(!view.can_add_pair);
        assert!(view.pair_add_reason.contains(&format!(
            "{components} authored components and {voices} expanded voices"
        )));
        assert!(view.pair_add_reason.contains("2 more voices"));
        assert_eq!(view.can_add_component, voices < 16 && components < 16);
        if !view.can_add_component {
            assert!(view.add_component_reason.contains(&format!(
                "{components} authored components and {voices} expanded voices"
            )));
            assert!(view.add_component_reason.contains("1 more voice"));
        }
    }
}

#[test]
fn structured_library_rows_keep_saved_identity_and_rate_independent_classification() {
    let mut ws = workspace();
    ws.set_query("ioannou 2");
    let view = ws.library_view();
    assert_eq!(view.rows.len(), 1);
    let row = &view.rows[0];
    assert!(row.key.contains("ioannou-2015-binaural-2-hz"));
    assert_eq!(row.bands, "Delta");
    assert!(row.collection.contains("Read only"));
    assert!(row.text.contains(&row.title) && row.text.contains(&row.dates));
    let classification = view.classification.clone();
    ws.set_export_rate(auditorylab_core::SampleRate::new(96_000).unwrap());
    assert_eq!(ws.library_view().classification, classification);
    assert_eq!(ws.library_view().rows[0].key, row.key);
}

#[test]
fn staged_envelope_failures_render_quantities_and_rate_in_metadata() {
    let mut workspace = workspace();
    let handle = workspace
        .editor
        .session
        .handle(EditingField::Metadata(
            crate::document::editing::MetadataField::Duration,
        ))
        .unwrap();
    workspace.stage(handle, "0").unwrap();
    let view = workspace.editor_view();
    let duration = view
        .metadata
        .iter()
        .find(|field| field.handle == handle)
        .unwrap();
    assert_eq!(
        duration.diagnostic,
        "Duration 0 s must last from one sample (1/48000 s) to 16.6 days at 48000 Hz"
    );
}

#[path = "tests/admissions.rs"]
mod admissions;
