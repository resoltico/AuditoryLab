//! Production-binding integration tests: generated callbacks, wired by
//! [`SlintAdapter::install`] exactly as at startup, drive a real workspace.
//! Dialogs are answered through a recording shell; audio is the portable
//! workspace's typed refusal, so no device is touched.
use crate::adapter::{Shell, SlintAdapter, Windows, parameter_id, parameter_key};
use crate::generated::{MainWindow, MetadataKey, ParameterKey};
use crate::test_platform::{install, settle};
use auditorylab::core::ParameterId;
use auditorylab::editing::{EditingField, MetadataField};
use auditorylab::presentation::ParameterText;
use auditorylab::workspace::{
    Capabilities, Job, JobKind, PathPurpose, PromptAnswer, PromptId, View, Workspace,
};
use slint::platform::{Key, WindowEvent};
use slint::private_unstable_api::re_exports::{
    AccessibleStringProperty, FocusReason, ItemRc, TextInput, WindowInner,
};
use slint::{ComponentHandle, Model};
use std::{cell::RefCell, ops::ControlFlow, rc::Rc, time::Duration};

#[derive(Debug, Clone, PartialEq)]
enum Request {
    Unsaved(PromptId),
    Purge(PromptId, String, String),
    Path(PromptId, PathPurpose, Option<String>),
    Close(bool),
}

struct Recorder(Rc<RefCell<Vec<Request>>>);

impl Shell for Recorder {
    fn ask_purge(&self, _: &Rc<SlintAdapter>, prompt: PromptId, key: String, title: String) {
        self.0.borrow_mut().push(Request::Purge(prompt, key, title));
    }
    fn ask_unsaved(&self, _: &Rc<SlintAdapter>, prompt: PromptId) {
        self.0.borrow_mut().push(Request::Unsaved(prompt));
    }
    fn choose_path(
        &self,
        _: &Rc<SlintAdapter>,
        prompt: PromptId,
        purpose: PathPurpose,
        suggested_name: Option<String>,
    ) {
        self.0
            .borrow_mut()
            .push(Request::Path(prompt, purpose, suggested_name));
    }
    fn close_resolved(&self, _: &SlintAdapter, approved: bool) {
        self.0.borrow_mut().push(Request::Close(approved));
    }
}

fn bound_workspace(
    catalog: Vec<auditorylab::recipe::RecipeSnapshot>,
    capabilities: Capabilities,
) -> (
    Workspace,
    tempfile::TempDir,
    auditorylab::library::LibraryStore,
) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    let directory = tempfile::tempdir_in(base).unwrap();
    let store = auditorylab::library::LibraryStore::new(
        auditorylab::library::LibraryRoot::explicit(directory.path().join("library")).unwrap(),
    )
    .unwrap();
    store.root().initialize().unwrap();
    let workspace = Workspace::with_library(catalog, capabilities, store.clone()).unwrap();
    (workspace, directory, store)
}

pub(crate) struct Harness {
    pub(crate) windows: Windows,
    pub(crate) adapter: Rc<SlintAdapter>,
    requests: Rc<RefCell<Vec<Request>>>,
    library: Option<(tempfile::TempDir, auditorylab::library::LibraryStore)>,
}

impl Harness {
    pub(crate) fn new() -> Self {
        Self::with_catalog(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
        )
    }
    fn with_catalog(
        catalog: Vec<auditorylab::recipe::RecipeSnapshot>,
        capabilities: Capabilities,
    ) -> Self {
        let (workspace, directory, store) = bound_workspace(catalog, capabilities);
        let mut harness = Self::with_workspace(workspace);
        harness.library = Some((directory, store));
        harness
    }
    fn store(&self) -> &auditorylab::library::LibraryStore {
        &self
            .library
            .as_ref()
            .expect("default harness owns its real library")
            .1
    }

    fn with_workspace(workspace: Workspace) -> Self {
        let harness = Self::with_startup_workspace(workspace);
        // Most integration cases also exercise the open monitor and its focus
        // handoffs. Request that view explicitly, not through a startup default.
        harness.ui().set_monitor_open(true);
        harness.settle();
        harness
    }

    fn with_startup_workspace(workspace: Workspace) -> Self {
        install();
        let windows = Windows::new().unwrap();
        let requests = Rc::new(RefCell::new(Vec::new()));
        let adapter = SlintAdapter::install(
            &windows,
            workspace,
            Box::new(Recorder(requests.clone())),
            None,
        );
        adapter.show_main().unwrap();
        windows
            .main
            .window()
            .set_size(slint::LogicalSize::new(1100.0, 900.0));
        windows
            .main
            .window()
            .dispatch_event(WindowEvent::WindowActiveChanged(true));
        settle(windows.main.window());
        adapter.main_frame_ready();
        settle(windows.main.window());
        let harness = Self {
            windows,
            adapter,
            requests,
            library: None,
        };
        harness.wait_for_files();
        harness
    }

    pub(crate) fn ui(&self) -> &MainWindow {
        &self.windows.main
    }

    pub(crate) fn settle(&self) {
        settle(self.ui().window());
    }

    pub(crate) fn workbench(&self) {
        self.ui().set_tab_index(1);
        self.settle();
        assert_eq!(self.adapter.inspect(|w| w.view()), View::Workbench);
    }

    fn take(&self) -> Vec<Request> {
        std::mem::take(&mut *self.requests.borrow_mut())
    }

    fn component_key(&self, index: usize) -> slint::SharedString {
        self.ui().get_components().row_data(index).unwrap().key
    }

    fn keys(&self, index: usize) -> Vec<ParameterKey> {
        let row = self.ui().get_components().row_data(index).unwrap();
        row.parameters.iter().map(|p| p.key).collect()
    }

    fn wait_for_files(&self) {
        for _ in 0..1000 {
            self.adapter.tick();
            if !self.adapter.inspect(|w| w.file_busy()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("file task did not finish");
    }

    pub(crate) fn field(&self, label: &str) -> ItemRc {
        field(self.ui(), label)
    }

    pub(crate) fn key(&self, text: impl Into<slint::SharedString>) {
        let text = text.into();
        self.ui()
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        self.ui()
            .window()
            .dispatch_event(WindowEvent::KeyReleased { text });
        self.settle();
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            self.adapter.shutdown().unwrap();
        }
    }
}

/// The text input inside the control whose accessible label is `label`.
pub(crate) fn field(ui: &MainWindow, label: &str) -> ItemRc {
    let root = WindowInner::from_pub(ui.window()).window_item_rc().unwrap();
    let owner = root
        .visit_descendants(|item| {
            if item
                .accessible_string_property(AccessibleStringProperty::Label)
                .as_deref()
                == Some(label)
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap_or_else(|| panic!("no control labelled {label}"));
    owner
        .visit_descendants(|item| {
            if item.downcast::<TextInput>().is_some() {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap()
}

fn text_of(item: &ItemRc) -> String {
    item.downcast::<TextInput>()
        .unwrap()
        .as_pin_ref()
        .text()
        .to_string()
}

fn parameter(component: usize, id: ParameterId) -> impl Fn(&mut Workspace) -> EditingField {
    move |w| {
        let cid = w.editor().draft().components()[component].id;
        EditingField::Parameter(cid, id)
    }
}

#[test]
fn generated_editing_callbacks_reach_the_workspace_and_project_its_results() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    assert_eq!(ui.get_components().row_count(), 1);
    let row = ui.get_components().row_data(0).unwrap();
    assert_eq!(row.heading, "Steady tone");
    assert_eq!(
        h.keys(0),
        [ParameterKey::ToneFrequency, ParameterKey::PeakLevel]
    );
    let binding = h.component_key(0);
    let frequency = h.adapter.inspect(parameter(0, ParameterId::ToneFrequency));

    ui.invoke_stage_parameter(binding.clone(), ParameterKey::ToneFrequency, "bad".into());
    assert_eq!(
        h.adapter
            .inspect(|w| w.editor().staged(frequency).map(str::to_owned)),
        Some("bad".to_owned())
    );
    let shown = ui
        .get_components()
        .row_data(0)
        .unwrap()
        .parameters
        .row_data(0)
        .unwrap();
    assert!(shown.staged && shown.text == "bad" && !shown.diagnostic.is_empty());
    assert!(!ui.get_can_save(), "unparseable staged input blocks Save");
    assert!(!ui.get_save_reason().is_empty());

    ui.invoke_commit_parameter(binding.clone(), ParameterKey::ToneFrequency, "3750".into());
    assert_eq!(
        h.adapter.inspect(|w| w.editor().draft().value(frequency)),
        Some(3750.0)
    );
    assert!(ui.get_can_save());

    ui.invoke_stage_parameter(binding.clone(), ParameterKey::ToneFrequency, "12".into());
    ui.invoke_restore_parameter(binding.clone(), ParameterKey::ToneFrequency);
    assert!(
        h.adapter
            .inspect(|w| w.editor().staged(frequency).is_none())
    );
    let shown = ui
        .get_components()
        .row_data(0)
        .unwrap()
        .parameters
        .row_data(0)
        .unwrap();
    assert_eq!(shown.text, "3750");
    assert!(!shown.staged);

    let title = EditingField::Metadata(MetadataField::Title);
    ui.invoke_commit_metadata(ui.get_document(), MetadataKey::Title, "Probe".into());
    assert_eq!(
        h.adapter.inspect(|w| w.editor().draft().title().to_owned()),
        "Probe"
    );
    assert_eq!(ui.get_title_field().text, "Probe");
    ui.invoke_stage_metadata(ui.get_document(), MetadataKey::Title, "Draft".into());
    assert_eq!(
        h.adapter
            .inspect(|w| w.editor().staged(title).map(str::to_owned)),
        Some("Draft".to_owned())
    );
    ui.invoke_restore_metadata(ui.get_document(), MetadataKey::Title);
    assert_eq!(ui.get_title_field().text, "Probe");

    ui.invoke_set_modulation(binding.clone(), 1);
    assert_eq!(ui.get_components().row_data(0).unwrap().modulation, 1);
    assert_eq!(
        h.keys(0),
        [
            ParameterKey::ToneFrequency,
            ParameterKey::PeakLevel,
            ParameterKey::AmplitudeModulationRate,
            ParameterKey::ModulationDepth
        ]
    );
    ui.invoke_set_ears(binding.clone(), 1);
    assert_eq!(ui.get_components().row_data(0).unwrap().ears, 1);
    ui.invoke_remove_component(binding.clone());
    assert_eq!(ui.get_components().row_count(), 0);
}

#[test]
fn play_save_export_and_prompts_run_through_the_shared_workflow() {
    let h = Harness::new();
    let ui = h.ui();
    let directory = tempfile::tempdir().unwrap();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);

    // This workspace has no device playback: Play is refused with its reason
    // before any task starts.
    ui.invoke_play_stop();
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    assert!(!ui.get_status().is_empty());
    assert!(!ui.get_playback_busy());

    ui.invoke_save_recipe();
    assert!(
        h.take().is_empty(),
        "Save publishes to the bound User Library"
    );
    h.wait_for_files();
    let inventory = h.store().scan(&auditorylab::workflow::Cancel::default());
    assert_eq!(inventory.entries.len(), 1);
    assert!(h.store().load(&inventory.entries[0].key).is_ok());
    assert!(
        !ui.get_dirty(),
        "the published snapshot is the saved baseline"
    );

    ui.invoke_export_wav();
    let [Request::Path(prompt, PathPurpose::ExportWav, _)] = h.take()[..] else {
        panic!("Export asks for a destination");
    };
    let exported = directory.path().join("probe.wav");
    h.adapter
        .respond(prompt, PromptAnswer::Path(Some(exported.clone())));
    assert!(ui.get_exporting() || exported.is_file());
    h.wait_for_files();
    assert_eq!(&std::fs::read(&exported).unwrap()[..4], b"RIFF");

    // A dirty document asks once; a second request while it is open is refused.
    ui.invoke_add_component(ui.get_document(), 1);
    ui.invoke_new_draft();
    let [Request::Unsaved(prompt)] = h.take()[..] else {
        panic!("unsaved changes are confirmed");
    };
    ui.invoke_new_draft();
    ui.invoke_quit();
    assert_eq!(
        h.take(),
        [Request::Close(false)],
        "an open prompt denies Quit and asks nothing more"
    );
    let before = ui.get_document();
    h.adapter.respond(prompt, PromptAnswer::Cancel);
    assert_eq!(ui.get_document(), before, "Cancel keeps the document");
    ui.invoke_new_draft();
    let [Request::Unsaved(prompt)] = h.take()[..] else {
        panic!("asked again");
    };
    h.adapter.respond(prompt, PromptAnswer::Discard);
    h.adapter.respond(prompt, PromptAnswer::Save);
    h.adapter.tick();
    assert_ne!(ui.get_document(), before, "Discard replaces the document");
    assert_eq!(ui.get_components().row_count(), 0);
    assert!(h.take().is_empty(), "a stale answer changes nothing");

    ui.invoke_quit();
    h.adapter.tick();
    assert_eq!(h.take(), [Request::Close(true)]);
}

#[test]
fn each_carrier_and_modulation_renders_exactly_its_semantic_fields() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    for id in ParameterId::ALL {
        assert_eq!(parameter_id(parameter_key(id)), id);
    }
    ui.invoke_add_component(ui.get_document(), 0);
    let binding = h.component_key(0);
    // Independent oracle: the fields each kind has.
    let carriers = [
        (0, vec![ParameterKey::ToneFrequency]),
        (1, vec![ParameterKey::ClickRate, ParameterKey::ClickWidth]),
    ];
    let modulations = [
        (0, vec![]),
        (
            1,
            vec![
                ParameterKey::AmplitudeModulationRate,
                ParameterKey::ModulationDepth,
            ],
        ),
        (2, vec![ParameterKey::GateRate, ParameterKey::GateWidth]),
    ];
    for (carrier, carrier_keys) in &carriers {
        for (modulation, modulation_keys) in &modulations {
            ui.invoke_set_carrier(binding.clone(), *carrier);
            ui.invoke_set_modulation(binding.clone(), *modulation);
            let mut expected = carrier_keys.clone();
            expected.push(ParameterKey::PeakLevel);
            expected.extend(modulation_keys);
            assert_eq!(
                h.keys(0),
                expected,
                "carrier {carrier} modulation {modulation}"
            );
            let row = ui.get_components().row_data(0).unwrap();
            for shown in row.parameters.iter() {
                let id = parameter_id(shown.key);
                assert_eq!(shown.label, (id).input_label());
                assert_eq!(shown.help, id.descriptor().help);
                assert_eq!(
                    shown.has_slider,
                    auditorylab::controls::range(id, Workspace::export_rates()[1]).is_some()
                );
                ui.invoke_stage_parameter(binding.clone(), shown.key, "1x".into());
                let field = h.adapter.inspect(parameter(0, id));
                assert_eq!(
                    h.adapter
                        .inspect(|w| w.editor().staged(field).map(str::to_owned)),
                    Some("1x".to_owned()),
                    "{:?} stages its own field",
                    shown.key
                );
                ui.invoke_restore_parameter(binding.clone(), shown.key);
            }
        }
    }
    // A key the component does not have is refused, not redirected.
    ui.invoke_set_carrier(binding.clone(), 0);
    ui.invoke_set_modulation(binding.clone(), 0);
    ui.invoke_stage_parameter(binding.clone(), ParameterKey::GateRate, "5".into());
    assert!(!h.adapter.inspect(|w| w.editor().has_staged()));
}

#[test]
fn a_focused_field_survives_updates_to_other_rows() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    for carrier in [0, 1, 0] {
        ui.invoke_add_component(ui.get_document(), carrier);
    }
    h.settle();
    let nested_before = ui.get_components().row_data(1).unwrap().parameters;
    let input = h.field("Click rhythm · component 2: Click rate (Hz)");
    let inner = WindowInner::from_pub(ui.window());
    inner.set_focus_item(&input, true, FocusReason::Programmatic);
    for _ in 0..4 {
        h.key(Key::RightArrow);
    }
    h.key("5");
    assert_eq!(text_of(&input), "405");
    let cursor = input
        .downcast::<TextInput>()
        .unwrap()
        .as_pin_ref()
        .cursor_position_byte_offset();

    let first = h.component_key(0);
    let third = h.component_key(2);
    // Structural commands change other rows without committing staged text.
    ui.invoke_set_ears(first.clone(), 2);
    ui.invoke_set_modulation(third.clone(), 2);
    ui.invoke_remove_component(first.clone());
    h.settle();

    assert!(
        inner.focus_item.borrow().upgrade().as_ref() == Some(&input),
        "the same widget keeps focus"
    );
    assert_eq!(text_of(&input), "405", "pending input survives");
    assert_eq!(
        input
            .downcast::<TextInput>()
            .unwrap()
            .as_pin_ref()
            .cursor_position_byte_offset(),
        cursor,
        "the caret stays where it was"
    );
    assert_eq!(
        ui.get_components().row_data(0).unwrap().parameters,
        nested_before,
        "the nested model object is kept"
    );
    assert_eq!(
        field(ui, "Click rhythm · component 1: Click rate (Hz)"),
        input,
        "the moved row relabels the same widget"
    );
    let rate = h.adapter.inspect(parameter(0, ParameterId::ClickRate));
    assert_eq!(
        h.adapter
            .inspect(|w| w.editor().staged(rate).map(str::to_owned)),
        Some("405".to_owned())
    );
}

#[test]
fn callbacks_from_a_replaced_document_are_refused() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    let old_component = h.component_key(0);
    let old_document = ui.get_document();

    // Focus and edit the title, then replace the document while it is focused.
    let title = h.field("Sound title");
    let inner = WindowInner::from_pub(ui.window());
    inner.set_focus_item(&title, true, FocusReason::Programmatic);
    h.key("x");
    ui.invoke_new_draft();
    let [Request::Unsaved(prompt)] = h.take()[..] else {
        panic!("the edited document asks first");
    };
    h.adapter.respond(prompt, PromptAnswer::Discard);
    h.adapter.tick();
    let new_document = ui.get_document();
    assert_ne!(new_document, old_document);
    let fresh_title = h.adapter.inspect(|w| w.editor().draft().title().to_owned());

    // Blur the old focused field: its rebinding suppressed the commit.
    inner.set_focus_item(&h.field("Sound title"), false, FocusReason::Programmatic);
    let other = ui.window();
    other.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Tab.into(),
    });
    h.settle();
    // Late callbacks carrying the old bindings.
    ui.invoke_commit_parameter(
        old_component.clone(),
        ParameterKey::ToneFrequency,
        "999".into(),
    );
    ui.invoke_commit_metadata(old_document.clone(), MetadataKey::Title, "Old".into());
    ui.invoke_stage_metadata(old_document.clone(), MetadataKey::Summary, "Old".into());
    ui.invoke_add_component(old_document.clone(), 1);
    h.adapter.inspect(|w| {
        assert_eq!(w.editor().draft().title(), fresh_title);
        assert!(w.editor().draft().components().is_empty());
        assert!(!w.editor().has_staged());
        assert!(!w.editor().dirty());
    });
}

#[test]
fn unchanged_state_projects_nothing_and_paths_follow_only_new_captures() {
    let h = Harness::new();
    for _ in 0..3 {
        h.adapter.tick();
    }
    let documents = h.adapter.counts.document_projections.get();
    let paths = h.adapter.counts.path_builds.get();
    assert!(paths > 0, "the selected recipe has a preview");
    for _ in 0..50 {
        h.adapter.tick();
    }
    assert_eq!(h.adapter.counts.document_projections.get(), documents);
    assert_eq!(h.adapter.counts.path_builds.get(), paths);
    // Selecting another recipe projects once and draws its new preview once.
    h.ui()
        .invoke_select(h.ui().get_catalog().row_data(1).unwrap().binding);
    h.adapter.tick();
    h.adapter.tick();
    assert_eq!(h.adapter.counts.path_builds.get(), paths + 1);
    assert!(h.adapter.counts.document_projections.get() > documents);
}

#[test]
fn a_tab_choice_survives_a_projection_before_its_change_handler_runs() {
    let h = Harness::new();
    let ui = h.ui();
    ui.set_tab_index(1);
    // A tick projects before Slint has run the tab's change handler.
    h.adapter.tick();
    h.settle();
    assert_eq!(ui.get_tab_index(), 1);
    assert_eq!(h.adapter.inspect(|w| w.view()), View::Workbench);
}

#[test]
fn a_dragged_slider_keeps_following_typed_values() {
    use slint::platform::PointerEventButton;
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    h.settle();
    let label = "Steady tone · component 1: Tone frequency (Hz) slider";
    let slider = || {
        let root = WindowInner::from_pub(ui.window()).window_item_rc().unwrap();
        root.visit_descendants(|item| {
            if item
                .accessible_string_property(AccessibleStringProperty::Label)
                .as_deref()
                == Some(label)
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .expect("the frequency slider")
    };
    let value = || {
        slider()
            .accessible_string_property(AccessibleStringProperty::Value)
            .unwrap()
            .parse::<f32>()
            .unwrap()
    };
    let item = slider();
    let geometry = item.geometry();
    let origin = item.map_to_window(geometry.origin);
    let y = origin.y + geometry.size.height / 2.0;
    let at =
        |fraction: f32| slint::LogicalPosition::new(origin.x + geometry.size.width * fraction, y);
    let window = ui.window();
    window.dispatch_event(WindowEvent::PointerPressed {
        position: at(0.5),
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerMoved { position: at(0.2) });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: at(0.2),
        button: PointerEventButton::Left,
    });
    h.settle();
    let dragged = value();
    let binding = h.component_key(0);
    ui.invoke_commit_parameter(binding.clone(), ParameterKey::ToneFrequency, "5000".into());
    h.settle();
    let expected = ui
        .get_components()
        .row_data(0)
        .unwrap()
        .parameters
        .row_data(0)
        .unwrap()
        .position;
    assert!(
        (value() - expected).abs() < 1e-3,
        "knob {} after typing 5000 should be {expected}; it was {dragged} after the drag",
        value()
    );
}

#[test]
fn an_imported_recipe_is_scrolled_into_view() {
    let h = Harness::new();
    let ui = h.ui();
    let directory = tempfile::tempdir().unwrap();
    for index in 0..24 {
        let mut draft = auditorylab::editing::Draft::new().unwrap();
        draft.set_title(format!("A earlier opened recipe {index:02}"));
        let path = directory.path().join(format!("earlier-{index}.toml"));
        std::fs::write(&path, draft.recipe().to_toml().unwrap()).unwrap();
        ui.invoke_open_recipe();
        answer_path(&h, &path);
        h.wait_for_files();
    }
    ui.set_library_scroll_y(0.0);
    h.settle();
    let path = directory.path().join("imported.toml");
    std::fs::write(
        &path,
        "schema_version = 3\nduration_seconds = 1.0\nfade_seconds = 0.01\n\n[description]\ntitle = \"Imported probe\"\n\n[document]\nid=\"00000000-0000-4000-8000-000000000001\"\n",
    )
    .unwrap();
    ui.invoke_open_recipe();
    let [Request::Path(prompt, PathPurpose::OpenRecipe, _)] = h.take()[..] else {
        panic!("Open asks for a file");
    };
    h.adapter.respond(prompt, PromptAnswer::Path(Some(path)));
    h.wait_for_files();
    for _ in 0..5 {
        h.settle();
    }
    let root = WindowInner::from_pub(ui.window()).window_item_rc().unwrap();
    let row = root
        .visit_descendants(|item| {
            if item
                .accessible_string_property(AccessibleStringProperty::Label)
                .is_some_and(|label| label.contains("Imported probe"))
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .expect("the imported row");
    let search = root
        .visit_descendants(|item| {
            if item
                .accessible_string_property(AccessibleStringProperty::Label)
                .is_some_and(|label| label == "Find a recipe")
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .expect("the search field");
    let list_top = search.map_to_window(search.geometry().origin).y + search.geometry().size.height;
    let geometry = row.geometry();
    let origin = row.map_to_window(geometry.origin);
    assert!(
        origin.y >= list_top - 0.1 && origin.y + geometry.size.height <= 900.0,
        "the imported row is visible: {origin:?} {geometry:?}"
    );
}

#[test]
fn clicking_a_recipe_row_selects_it() {
    let h = Harness::new();
    let ui = h.ui();
    let second = ui.get_catalog().row_data(1).unwrap();
    let expected_key = second.binding.clone();
    let row = labelled(ui, &second.text);
    let list = labelled(ui, "Library results");
    // Reveal the row through the real scroll control before clicking. Neither
    // accessibility visibility nor a card's centre implies an unclipped hit.
    let mut pointer = None;
    for _ in 0..8 {
        let row_bounds = row.geometry();
        let list_bounds = list.geometry();
        let row_origin = row.map_to_window(row_bounds.origin);
        let list_origin = list.map_to_window(list_bounds.origin);
        let top = row_origin.y.max(list_origin.y);
        let bottom = (row_origin.y + row_bounds.size.height)
            .min(list_origin.y + list_bounds.size.height);
        if bottom - top > 8.0 {
            pointer = Some(slint::LogicalPosition::new(
                row_origin.x + row_bounds.size.width / 2.0,
                (top + bottom) / 2.0,
            ));
            break;
        }
        ui.window().dispatch_event(WindowEvent::PointerScrolled {
            position: centre(&list),
            delta_x: 0.0,
            delta_y: centre(&list).y - centre(&row).y,
        });
        h.settle();
    }
    let pointer = pointer.expect("second card must be reachable by scrolling");
    drag(ui, pointer, pointer);
    h.settle();
    assert_eq!(ui.get_selected(), 1);
    assert_eq!(ui.get_library_key(), expected_key);
}

#[test]
fn dragging_the_pane_handles_resizes_the_panes() {
    let h = Harness::new();
    let ui = h.ui();
    let width = ui.get_sidebar_width();
    let divider = centre(&labelled(ui, "Library pane width"));
    drag(
        ui,
        divider,
        slint::LogicalPosition::new(divider.x + 40.0, divider.y),
    );
    h.settle();
    assert_eq!(ui.get_sidebar_width(), width + 40.0);

    ui.invoke_resize_monitor(240.0);
    h.settle();
    let height = ui.get_effective_drawer_height();
    let grip = centre(&labelled(ui, "Output monitor height"));
    drag(ui, grip, slint::LogicalPosition::new(grip.x, grip.y - 30.0));
    h.settle();
    assert!(
        ui.get_effective_drawer_height() > height,
        "dragging the grip up grows the monitor from {height}"
    );
}

#[test]
fn a_removed_component_returns_through_undo_remove() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    ui.invoke_add_component(ui.get_document(), 1);
    h.settle();
    let first = h.adapter.inspect(|w| w.editor().draft().components()[0].id);
    assert_eq!(ui.get_removed_component(), "");
    ui.invoke_remove_component(h.component_key(0));
    h.settle();
    assert_eq!(ui.get_components().row_count(), 1);
    assert_eq!(
        ui.get_removed_component(),
        "Removed Steady tone · component 1."
    );
    assert!(ui.get_can_restore_component());
    let button = labelled(ui, "Undo remove");
    let width = ui
        .window()
        .size()
        .to_logical(ui.window().scale_factor())
        .width;
    assert!(
        button.geometry().size.width < width / 4.0,
        "Undo remove is sized to its label"
    );
    let undo = centre(&button);
    drag(ui, undo, undo);
    h.settle();
    assert_eq!(ui.get_components().row_count(), 2);
    assert_eq!(
        h.adapter.inspect(|w| w.editor().draft().components()[0].id),
        first
    );
    assert_eq!(ui.get_removed_component(), "");
}

#[test]
fn component_options_are_sized_to_their_content() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    h.settle();
    let options = labelled(ui, "Routing, modulation and component options");
    // The disclosure follows the editor fields in a scrolling document; font
    // metrics determine whether it starts inside the viewport.
    WindowInner::from_pub(ui.window()).set_focus_item(&options, true, FocusReason::Programmatic);
    h.settle();
    let position = centre(&options);
    let size = ui.window().size().to_logical(ui.window().scale_factor());
    assert!(position.x >= 0.0 && position.x < size.width);
    assert!(position.y >= 0.0 && position.y < size.height);
    drag(ui, position, position);
    h.settle();
    assert!(
        options
            .accessible_string_property(AccessibleStringProperty::Label)
            .unwrap()
            .ends_with(" · expanded")
    );
    let name = ui.get_components().row_data(0).unwrap().name;
    let width = ui
        .window()
        .size()
        .to_logical(ui.window().scale_factor())
        .width;
    let bounds = |label: &str| {
        let item = labelled(ui, label);
        let geometry = item.geometry();
        (item.map_to_window(geometry.origin).x, geometry.size.width)
    };
    let (remove_x, remove_width) = bounds(&format!("Remove {name}"));
    assert!(
        remove_width < width / 4.0,
        "Remove is {remove_width} of {width}"
    );
    let choices: Vec<_> = ["ears", "carrier", "modulation"]
        .iter()
        .map(|choice| bounds(&format!("{name}: {choice}")))
        .collect();
    assert!(
        choices.iter().all(|&c| c == choices[0]),
        "the choices share one column: {choices:?}"
    );
    assert!(
        choices[0].1 < width / 2.0,
        "choices are {:?} of {width}",
        choices[0]
    );
    assert_eq!(remove_x, choices[0].0, "Remove aligns with the choices");
}

/// The first item whose accessible label starts with `prefix`.
fn labelled(ui: &MainWindow, prefix: &str) -> ItemRc {
    WindowInner::from_pub(ui.window())
        .window_item_rc()
        .unwrap()
        .visit_descendants(|item| {
            if item
                .accessible_string_property(AccessibleStringProperty::Label)
                .is_some_and(|label| label.starts_with(prefix))
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap_or_else(|| panic!("an item labelled {prefix:?}"))
}

fn centre(item: &ItemRc) -> slint::LogicalPosition {
    let geometry = item.geometry();
    let origin = item.map_to_window(geometry.origin);
    slint::LogicalPosition::new(
        origin.x + geometry.size.width / 2.0,
        origin.y + geometry.size.height / 2.0,
    )
}

/// Press the left button at `from`, move to `to` and release there.
fn drag(ui: &MainWindow, from: slint::LogicalPosition, to: slint::LogicalPosition) {
    use slint::platform::PointerEventButton;
    let button = PointerEventButton::Left;
    let window = ui.window();
    window.dispatch_event(WindowEvent::PointerPressed {
        position: from,
        button,
    });
    window.dispatch_event(WindowEvent::PointerMoved { position: to });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: to,
        button,
    });
}

#[test]
fn about_and_licenses_use_production_actions_and_reopen_without_losing_terms() {
    let h = Harness::new();
    h.ui().invoke_about();
    settle(h.windows.about.window());
    assert!(h.windows.about.window().is_visible());
    assert_eq!(
        h.windows.about.get_identity().as_str(),
        format!(
            "Version {} (Build {})",
            auditorylab::identity::BUILD_IDENTITY.version,
            auditorylab::identity::BUILD_IDENTITY.build
        )
    );
    h.windows.about.invoke_open_licenses();
    settle(h.windows.licenses.window());
    assert!(h.windows.licenses.window().is_visible());
    assert_eq!(
        h.windows.licenses.get_licenses().as_str(),
        include_str!("../../../LICENSE")
    );
    h.windows.licenses.invoke_third_party_licenses_requested();
    assert!(h.windows.licenses.get_notice().contains("missing or empty"));
    h.windows
        .licenses
        .window()
        .dispatch_event(WindowEvent::KeyPressed {
            text: Key::Escape.into(),
        });
    assert!(
        !h.windows.licenses.window().is_visible(),
        "Escape closes the focused license window"
    );
    h.windows.about.invoke_open_licenses();
    assert!(h.windows.licenses.window().is_visible());
    for _ in 0..2 {
        h.windows.about.invoke_dismiss();
        assert!(!h.windows.about.window().is_visible());
        h.ui().invoke_about();
        assert!(h.windows.about.window().is_visible());
        h.windows.licenses.invoke_dismiss();
        assert!(!h.windows.licenses.window().is_visible());
        h.windows.about.invoke_open_licenses();
        assert!(h.windows.licenses.window().is_visible());
    }
}

#[test]
fn retired_component_callbacks_stay_retired_after_undo() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    h.settle();
    let identity = h.adapter.inspect(|w| w.editor().draft().components()[0].id);
    let mut previous = h.component_key(0);
    for _ in 0..100 {
        ui.invoke_remove_component(previous.clone());
        h.settle();
        assert_eq!(ui.get_components().row_count(), 0);
        ui.invoke_restore_component(ui.get_document());
        h.settle();
        let current = h.component_key(0);
        assert_ne!(current, previous);
        assert_eq!(ui.get_components().row_count(), 1);
        ui.invoke_commit_parameter(previous.clone(), ParameterKey::ToneFrequency, "999".into());
        assert_eq!(
            h.adapter.inspect(|w| w
                .editor()
                .draft()
                .component(identity)
                .unwrap()
                .read(ParameterId::ToneFrequency)),
            Some(440.0)
        );
        previous = current;
    }
}

#[test]
fn editing_and_telemetry_do_not_project_the_library_again() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    h.settle();
    let libraries = h.adapter.counts.library_projections.get();
    let binding = h.component_key(0);
    ui.invoke_commit_parameter(binding.clone(), ParameterKey::ToneFrequency, "880".into());
    ui.invoke_stage_parameter(binding.clone(), ParameterKey::ToneFrequency, "900".into());
    for _ in 0..20 {
        h.adapter.tick();
    }
    assert_eq!(h.adapter.counts.library_projections.get(), libraries);
    ui.invoke_select(ui.get_catalog().row_data(1).unwrap().binding);
    assert_eq!(h.adapter.counts.library_projections.get(), libraries + 1);
}

fn tick_until(h: &Harness, condition: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        h.adapter.tick();
        if condition() {
            return;
        }
        std::thread::yield_now();
    }
    panic!("controlled GUI task did not settle");
}

fn controlled_harness() -> (Harness, tempfile::TempDir) {
    let mut draft = auditorylab::editing::Draft::new().unwrap();
    draft.set_duration_seconds(0.01);
    draft.set_fade_seconds(0.0);
    let h = Harness::with_catalog(vec![draft.snapshot()], Capabilities::available());
    // This is a real temporary input in the one persistent workspace query path.
    h.ui().invoke_collection_filter(3, 0);
    h.wait_for_files();
    let directory = tempfile::tempdir_in(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp"),
    )
    .unwrap();
    (h, directory)
}
fn take_job(h: &Harness, kind: JobKind) -> Job {
    let mut jobs = h.adapter.take_jobs();
    assert_eq!(jobs.len(), 1, "one admitted operation was captured");
    let job = jobs.remove(0);
    assert_eq!(job.kind(), kind);
    job
}
fn finish_file(h: &Harness, job: Job) {
    assert_eq!(job.kind(), JobKind::File);
    let finished = job.execute(|_| panic!("a file job emits no audio preparation"));
    h.adapter.deliver(finished);
}
fn finish_stopped_audio(h: &Harness, job: Job) {
    assert_eq!(job.kind(), JobKind::Audio);
    // The queued job has never run. Stop makes its real execution return an
    // early terminal report before any device discovery or preparation.
    assert!(h.adapter.command(Workspace::stop).unwrap().is_ok());
    let finished = job.execute(|_| panic!("early-stopped audio must not prepare a device"));
    h.adapter.deliver(finished);
}

fn answer_path(h: &Harness, path: &std::path::Path) {
    let prompt = h
        .take()
        .into_iter()
        .find_map(|request| match request {
            Request::Path(prompt, _, _) => Some(prompt),
            _ => None,
        })
        .expect("production callback opened a path panel");
    h.adapter
        .respond(prompt, PromptAnswer::Path(Some(path.to_owned())));
}

#[test]
fn library_button_admits_play_during_save() {
    assert_library_play_during_file(true);
}
#[test]
fn library_button_admits_play_during_import() {
    assert_library_play_during_file(false);
}

fn action_button(ui: &MainWindow, label: &str) -> ItemRc {
    WindowInner::from_pub(ui.window())
        .window_item_rc()
        .unwrap()
        .visit_descendants(|item| {
            if item.is_visible()
                && item.is_accessible()
                && item
                    .accessible_string_property(AccessibleStringProperty::Label)
                    .is_some_and(|text| text == label)
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap_or_else(|| panic!("visible accessible action {label:?}"))
}

fn assert_library_play_during_file(save: bool) {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let (h, dir) = controlled_harness();
    let ui = h.ui();
    let path = dir.path().join("sound.toml");
    let file = if save {
        h.workbench();
        ui.invoke_save_recipe();
        assert!(h.take().is_empty(), "managed Save opens no external panel");
        let file = take_job(&h, JobKind::File);
        ui.set_tab_index(0);
        h.settle();
        file
    } else {
        std::fs::write(&path, "schema_version=3\nduration_seconds=0.01\nfade_seconds=0\n\n[document]\nid=\"00000000-0000-4000-8000-000000000001\"\n").unwrap();
        ui.invoke_open_recipe();
        assert!(
            !ui.get_can_play(),
            "a native path prompt blocks new playback"
        );
        action_button(ui, "Listen").accessible_action(&AccessibilityAction::Default);
        assert!(!h.adapter.inspect(|w| w.audio_busy()));
        answer_path(&h, &path);
        take_job(&h, JobKind::File)
    };
    assert!(h.adapter.inspect(|w| w.file_busy()));
    assert!(ui.get_can_play(), "Save/Import can coexist with audio");
    assert_eq!(
        ui.get_library_play_action(),
        auditorylab::vocabulary::Action::library_play_labels()[0]
    );
    action_button(ui, "Listen").accessible_action(&AccessibilityAction::Default);
    assert!(
        h.adapter.inspect(|w| w.audio_busy()),
        "the actual accessible button dispatches Play"
    );
    let audio = take_job(&h, JobKind::Audio);
    assert_eq!(
        ui.get_library_play_action(),
        auditorylab::vocabulary::Action::library_play_labels()[1]
    );
    finish_stopped_audio(&h, audio);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    finish_file(&h, file);
    h.wait_for_files();
}

#[test]
fn library_button_obeys_export_prompt_pending_and_capability_refusals() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let (h, dir) = controlled_harness();
    let ui = h.ui();
    ui.invoke_export_wav();
    assert!(!ui.get_can_play());
    action_button(ui, "Listen").accessible_action(&AccessibilityAction::Default);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    answer_path(&h, &dir.path().join("sound.wav"));
    let export = take_job(&h, JobKind::File);
    assert!(!ui.get_can_play());
    action_button(ui, "Listen").accessible_action(&AccessibilityAction::Default);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    finish_file(&h, export);
    tick_until(&h, || !h.adapter.inspect(|w| w.file_busy()));
    action_button(ui, "Listen").accessible_action(&AccessibilityAction::Default);
    let audio = take_job(&h, JobKind::Audio);
    action_button(ui, "Stop current & play recipe")
        .accessible_action(&AccessibilityAction::Default);
    assert!(h.adapter.inspect(|w| w.action_pending()));
    assert!(!ui.get_can_play());
    action_button(ui, "Stop current & play recipe")
        .accessible_action(&AccessibilityAction::Default);
    finish_stopped_audio(&h, audio);
    tick_until(&h, || !h.adapter.inspect(|w| w.action_pending()));
    let replacement = take_job(&h, JobKind::Audio);
    finish_stopped_audio(&h, replacement);
    tick_until(&h, || !h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn library_button_obeys_unavailable_playback_capability() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let h = Harness::with_catalog(
        vec![auditorylab::editing::Draft::new().unwrap().snapshot()],
        Capabilities::no_device_playback(),
    );
    h.ui().invoke_collection_filter(3, 0);
    h.wait_for_files();
    h.settle();
    assert!(h.ui().get_has_results());
    assert!(!h.ui().get_can_play());
    assert_eq!(
        h.ui().get_play_reason(),
        "Device playback is disabled for this workspace."
    );
    action_button(h.ui(), "Listen").accessible_action(&AccessibilityAction::Default);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn notes_return_stages_newlines_and_save_preserves_exact_multiline_text() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    labelled(ui, "Sound settings").accessible_action(&AccessibilityAction::Default);
    ui.set_notes(true);
    h.settle();
    let input = h.field("Custom recipe notes");
    let inner = WindowInner::from_pub(ui.window());
    inner.set_focus_item(&input, true, FocusReason::Programmatic);
    h.key("A");
    h.key("\n");
    h.key("B");
    h.key("\n");
    h.key("\n");
    h.key("測定 🎧");
    let text = text_of(&input);
    assert_eq!(text, "A\nB\n\n測定 🎧");
    assert_eq!(
        h.adapter.inspect(|w| w
            .editor()
            .staged(EditingField::Metadata(MetadataField::Summary))
            .map(str::to_owned)),
        Some(text.clone())
    );
    ui.set_ui_size(20.0);
    h.adapter.tick();
    h.settle();
    assert_eq!(text_of(&input), text);
    h.key(Key::Escape);
    assert_eq!(text_of(&input), "");
    h.key("A\nB\n\n測定 🎧");
    ui.invoke_save_recipe();
    assert!(h.take().is_empty());
    h.wait_for_files();
    let entry = h
        .store()
        .scan(&auditorylab::workflow::Cancel::default())
        .entries
        .remove(0);
    let saved = h.store().load(&entry.key).unwrap().snapshot;
    assert_eq!(
        saved.about().unwrap().summary.as_deref(),
        Some(text.as_str())
    );
}

#[test]
fn native_event_filter_keeps_focus_suspension_after_frame_readiness() {
    use slint::winit_030::winit::event::WindowEvent;
    let h = Harness::new();
    h.adapter.main_window_event(&WindowEvent::Focused(false));
    assert!(h.ui().get_edits_suspended());
    h.adapter.main_window_event(&WindowEvent::RedrawRequested);
    h.settle();
    assert!(h.ui().get_edits_suspended());
    h.adapter.main_window_event(&WindowEvent::Focused(true));
    assert!(!h.ui().get_edits_suspended());
}

#[test]
fn monitor_presentation_waits_for_main_and_failures_are_visible_and_retryable() {
    let fail = Rc::new(std::cell::Cell::new(true));
    crate::test_platform::install_with_auxiliary_failure(fail.clone());
    let windows = Windows::new().unwrap();
    // Exercise the requested monitor; startup now leaves it closed for discovery.
    windows.main.set_monitor_open(true);
    let (workspace, _library_directory, _store) = bound_workspace(
        auditorylab::recipe::catalog().unwrap(),
        Capabilities::no_device_playback(),
    );
    let adapter = SlintAdapter::install(
        &windows,
        workspace,
        Box::new(Recorder(Rc::new(RefCell::new(Vec::new())))),
        None,
    );
    assert!(
        !adapter.monitor_presented(),
        "install cannot present an auxiliary before main"
    );
    windows.main.set_ui_size(40.0);
    windows
        .main
        .window()
        .set_size(slint::PhysicalSize::new(680, 700));
    adapter.show_main().unwrap();
    settle(windows.main.window());
    adapter.main_frame_ready();
    settle(windows.main.window());
    adapter.tick();
    assert!(!windows.main.get_drawer_fits());
    assert!(!adapter.monitor_presented());
    assert!(
        windows
            .main
            .get_auxiliary_error()
            .contains("injected Output monitor presentation failure")
    );
    windows.main.set_monitor_open(false);
    adapter.tick();
    assert_eq!(
        windows.main.get_auxiliary_error(),
        "",
        "disabled monitoring clears its resolved presentation error"
    );
    windows.main.set_monitor_open(true);
    adapter.tick();
    assert!(
        windows
            .main
            .get_auxiliary_error()
            .contains("presentation failure")
    );
    fail.set(false);
    adapter.tick();
    assert!(adapter.monitor_presented());
    windows
        .monitor
        .window()
        .set_size(slint::PhysicalSize::new(950, 500));
    settle(windows.monitor.window());
    adapter.tick();
    settle(windows.monitor.window());
    let monitor_root = WindowInner::from_pub(windows.monitor.window())
        .window_item_rc()
        .unwrap();
    for (label, expected) in [
        ("Waveform time window", "40 ms"),
        ("Waveform amplitude scale", "Fit signal"),
    ] {
        let control = monitor_root
            .visit_descendants(|item| {
                if item.is_visible()
                    && item
                        .accessible_string_property(AccessibleStringProperty::Label)
                        .is_some_and(|name| name == label)
                {
                    ControlFlow::Break(item.clone())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .unwrap();
        assert_eq!(
            control
                .accessible_string_property(AccessibleStringProperty::Value)
                .as_deref(),
            Some(expected)
        );
    }
    assert_eq!(windows.main.get_auxiliary_error(), "");
    adapter.shutdown().unwrap();
}

#[test]
fn nominal_filter_callbacks_and_committed_draft_labels_use_production_bindings() {
    let h = Harness::new();
    let ui = h.ui();
    ui.invoke_rhythm_filter(1, 1); // Delta and binaural.
    h.adapter.inspect(|w| {
        assert_eq!(
            w.library_view()
                .rows
                .iter()
                .filter(|row| !row.key.is_empty())
                .count(),
            1
        );
        assert_eq!(
            w.selected().unwrap().origin(),
            &auditorylab::recipe::Origin::BuiltIn("ioannou-2015-binaural-2-hz")
        );
    });
    ui.invoke_rhythm_filter(5, 6); // A steady carrier cannot borrow Gamma.
    assert!(!ui.get_has_results());
    ui.invoke_rhythm_filter(0, 0);
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 1); // Clicks.
    let binding = h.component_key(0);
    ui.invoke_commit_parameter(binding.clone(), ParameterKey::ClickRate, "2".into());
    assert!(
        ui.get_draft_brief()
            .contains("Draft classification · Delta")
    );
    ui.invoke_stage_parameter(binding.clone(), ParameterKey::ClickRate, "bad".into());
    assert!(
        ui.get_draft_brief()
            .contains("Last committed classification · Delta")
    );
    ui.invoke_commit_parameter(binding.clone(), ParameterKey::ClickRate, "6".into());
    assert!(
        ui.get_draft_brief()
            .contains("Draft classification · Theta")
    );
    assert!(ui.get_draft_brief().contains("Saved version · Not saved"));
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn catalog_callback_keeps_snapshot_identity_after_filter_reorders_rows() {
    let h = Harness::new();
    let ui = h.ui();
    let stale = ui.get_catalog().row_data(0).unwrap().binding;
    let original = h.adapter.inspect(|w| w.selected().unwrap().id());
    ui.invoke_rhythm_filter(1, 1);
    let selected = h.adapter.inspect(|w| w.selected().unwrap().id());
    ui.invoke_select(stale.clone());
    assert_eq!(h.adapter.inspect(|w| w.selected().unwrap().id()), selected);
    ui.invoke_rhythm_filter(0, 0);
    ui.invoke_select(stale.clone());
    assert_eq!(h.adapter.inspect(|w| w.selected().unwrap().id()), original);
    assert_eq!(ui.get_library_key(), stale);
}

#[test]
fn authored_pair_controls_validate_both_coupled_fields_and_both_voice_budgets() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 2);
    assert_eq!(ui.get_components().row_count(), 1);
    assert_eq!(
        h.keys(0),
        [
            ParameterKey::PairFirstFrequency,
            ParameterKey::PairOffset,
            ParameterKey::PairFirstLevel,
            ParameterKey::PairSecondLevel
        ]
    );
    let binding = h.component_key(0);
    ui.invoke_commit_parameter(binding.clone(), ParameterKey::PairOffset, "-6".into());
    assert!(
        ui.get_components()
            .row_data(0)
            .unwrap()
            .context
            .contains("194 Hz")
    );
    assert!(ui.get_draft_brief().contains("Theta"));
    ui.invoke_commit_parameter(
        binding.clone(),
        ParameterKey::PairFirstFrequency,
        "24000".into(),
    );
    let rows = ui.get_components().row_data(0).unwrap().parameters;
    for key in [ParameterKey::PairFirstFrequency, ParameterKey::PairOffset] {
        assert!(
            rows.iter()
                .find(|p| p.key == key)
                .is_some_and(|p| !p.diagnostic.is_empty())
        );
    }
    assert!(!ui.get_can_export());
    assert!(
        ui.get_can_save(),
        "finite rate-invalid pairs remain serializable for repair"
    );
    ui.invoke_commit_parameter(
        binding.clone(),
        ParameterKey::PairFirstFrequency,
        "200.00000000000003".into(),
    );
    let value = h.adapter.inspect(|w| {
        w.editor().draft().components()[0]
            .node
            .read(ParameterId::PairFirstFrequency)
            .unwrap()
    });
    assert_eq!(
        value.to_bits(),
        200.00000000000003_f64.to_bits(),
        "text never passes through a Slint f32 property"
    );
    for _ in 0..6 {
        ui.invoke_add_component(ui.get_document(), 2);
    }
    ui.invoke_add_component(ui.get_document(), 0); // 7 pairs + one primitive = 15 voices.
    assert!(ui.get_can_add_component());
    assert!(!ui.get_can_add_pair());
    assert!(!ui.get_pair_add_reason().is_empty());
    let before = ui.get_components().row_count();
    ui.invoke_add_component(ui.get_document(), 2);
    assert_eq!(ui.get_components().row_count(), before);
    ui.invoke_add_component(ui.get_document(), 0);
    assert!(!ui.get_can_add_component());
    assert!(!ui.get_can_add_pair());
}

#[test]
fn full_u32_owner_ids_keep_distinct_model_groups_and_edit_bindings() {
    let mut recipe = auditorylab::editing::Draft::new().unwrap().recipe();
    recipe.stimulus.components = vec![
        auditorylab::core::Component {
            id: auditorylab::core::ComponentId::new(u32::MAX),
            node: auditorylab::core::Node::BinauralPair(auditorylab::core::Pair {
                first_hz: 200.0,
                offset_hz: 2.0,
                first_peak_dbfs: -30.0,
                second_peak_dbfs: -30.0,
            }),
        },
        auditorylab::core::Component {
            id: auditorylab::core::ComponentId::new(u32::MAX - 1),
            node: auditorylab::core::Node::Voice(auditorylab::controls::default_component(
                auditorylab::core::CarrierKind::Clicks,
            )),
        },
    ];
    let snapshot = auditorylab::recipe::RecipeSnapshot::new(
        auditorylab::recipe::Origin::Stream("large owners".into()),
        recipe,
    );
    let h = Harness::with_catalog(vec![snapshot], Capabilities::no_device_playback());
    h.ui().invoke_collection_filter(3, 0);
    h.wait_for_files();
    h.adapter.command(Workspace::copy_selected);
    h.workbench();
    let rows = h.ui().get_components();
    assert_eq!(rows.row_data(0).unwrap().id, u32::MAX.to_string());
    assert_eq!(rows.row_data(1).unwrap().id, (u32::MAX - 1).to_string());
    let first = h.component_key(0);
    let second = h.component_key(1);
    assert_ne!(first, second);
    h.ui()
        .invoke_commit_parameter(first.clone(), ParameterKey::PairOffset, "6".into());
    assert_eq!(
        h.adapter.inspect(|w| w.editor().draft().components()[1]
            .node
            .read(ParameterId::ClickRate)),
        Some(40.0)
    );
}

#[test]
fn persistent_library_slint_bindings_publish_exact_draft_and_keep_filter_identity() {
    use auditorylab::library::{LibraryRoot, LibraryStore};
    let temp = tempfile::tempdir().unwrap();
    let store =
        LibraryStore::new(LibraryRoot::explicit(temp.path().join("library")).unwrap()).unwrap();
    store.root().initialize().unwrap();
    let workspace = Workspace::with_library(
        auditorylab::recipe::catalog().unwrap(),
        Capabilities::no_device_playback(),
        store.clone(),
    )
    .unwrap();
    let h = Harness::with_workspace(workspace);
    h.wait_for_files();
    let ui = h.ui();
    ui.invoke_search("ioannou 2".into());
    ui.invoke_open_workbench();
    h.settle();
    assert_eq!(h.adapter.inspect(|w| w.view()), View::Workbench);
    let binding = h.component_key(0);
    ui.invoke_commit_metadata(
        ui.get_document(),
        MetadataKey::Title,
        "Native managed copy".into(),
    );
    ui.invoke_save_recipe();
    h.wait_for_files();
    assert!(
        h.take().is_empty(),
        "managed Save must not ask for an external destination"
    );
    assert!(!ui.get_dirty());
    assert!(ui.get_draft_location().starts_with("User Library"));
    let first = store
        .scan(&auditorylab::workflow::Cancel::default())
        .entries
        .remove(0);
    assert_eq!(first.title, "Native managed copy");
    assert!(first.created_at.is_some());
    ui.invoke_commit_parameter(binding.clone(), ParameterKey::PairOffset, "6".into());
    assert!(
        ui.get_draft_brief()
            .contains("Draft classification · Theta")
    );
    assert!(ui.get_draft_brief().contains("Saved version · Delta"));
    ui.invoke_rhythm_filter(1, 0);
    ui.invoke_collection_filter(2, 0);
    h.wait_for_files();
    assert!(
        !h.adapter.inspect(|w| w.file_busy()),
        "selected User content is loaded before Save"
    );
    assert_eq!(
        ui.get_catalog().row_count(),
        1,
        "saved Delta remains visible while the Theta draft is unsaved"
    );
    ui.invoke_save_recipe();
    h.wait_for_files();
    assert!(
        h.adapter
            .inspect(|w| w.catalog_query().search.contains("ioannou"))
    );
    assert_eq!(
        ui.get_catalog().row_count(),
        0,
        "Save must retain restrictions"
    );
    ui.invoke_show_saved();
    h.wait_for_files();
    assert_eq!(ui.get_collection_choice(), 2);
    assert_eq!(ui.get_catalog().row_count(), 1);
    assert_eq!(ui.get_sound_title(), "Native managed copy");
    assert!(ui.get_catalog().row_data(0).unwrap().text.contains("Theta"));
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    let key = ui.get_library_key();
    let token = ui.get_library_token();
    ui.invoke_library_action(key.clone(), token, "trash".into(), "".into());
    h.wait_for_files();
    ui.invoke_collection_filter(4, 0);
    h.wait_for_files();
    assert!(ui.get_library_trashed());
    assert!(!ui.get_can_play());
    ui.invoke_library_action(
        key.clone(),
        ui.get_library_token(),
        "restore".into(),
        "".into(),
    );
    h.wait_for_files();
    ui.invoke_collection_filter(2, 0);
    h.wait_for_files();
    ui.invoke_library_action(
        key.clone(),
        ui.get_library_token(),
        "trash".into(),
        "".into(),
    );
    h.wait_for_files();
    ui.invoke_collection_filter(4, 0);
    h.wait_for_files();
    ui.invoke_library_action(
        key.clone(),
        ui.get_library_token(),
        "purge".into(),
        "".into(),
    );
    let [Request::Purge(prompt, ref prompted, ref title)] = h.take()[..] else {
        panic!("native purge confirmation missing")
    };
    assert_eq!(prompted, key.as_str());
    assert_eq!(title, "Native managed copy");
    assert!(store.load(&key.parse().unwrap()).is_ok());
    h.adapter.respond(prompt, PromptAnswer::Confirm);
    h.wait_for_files();
    assert!(
        store
            .scan(&auditorylab::workflow::Cancel::default())
            .entries
            .is_empty()
    );
}

#[test]
fn persistent_native_filters_use_shared_query_and_invalid_dates_preserve_it() {
    use auditorylab::library::{LibraryRoot, LibraryStore};
    let temp = tempfile::tempdir().unwrap();
    let store =
        LibraryStore::new(LibraryRoot::explicit(temp.path().join("library")).unwrap()).unwrap();
    store.root().initialize().unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store,
        )
        .unwrap(),
    );
    h.wait_for_files();
    let ui = h.ui();
    ui.invoke_collection_filter(1, 2);
    ui.invoke_more_filters(
        "".into(),
        0,
        0,
        0,
        0,
        1,
        "".into(),
        "".into(),
        "".into(),
        "".into(),
        "".into(),
        "".into(),
    );
    let expected = h.adapter.inspect(|w| {
        w.catalog_query()
            .clone()
            .execute(
                &w.catalog()
                    .iter()
                    .map(|s| {
                        auditorylab::collection::EntrySummary::from_snapshot(
                            auditorylab::library::LibraryKey::BuiltIn(match s.origin() {
                                auditorylab::recipe::Origin::BuiltIn(id) => id.to_string(),
                                _ => unreachable!(),
                            }),
                            s,
                            None,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap()
            .unique_count()
    });
    assert!(
        ui.get_library_count()
            .starts_with(&format!("{expected} unique sounds"))
    );
    assert!(
        ui.get_catalog().row_count() > expected,
        "virtual band groups repeat references, not counted sounds"
    );
    ui.invoke_more_filters(
        "".into(),
        0,
        0,
        0,
        0,
        1,
        "2026-10-24".into(),
        "2026-10-23".into(),
        "".into(),
        "".into(),
        "".into(),
        "".into(),
    );
    assert!(
        h.adapter
            .inspect(|w| w.catalog_query().created.from.is_none())
    );
    assert!(
        !ui.get_library_alert().is_empty(),
        "the rejected range is shown in the library's live diagnostic banner"
    );
    let date_error = ui.get_library_alert();
    assert!(
        date_error.contains("Created through")
            && date_error.contains("Created from")
            && date_error.contains("on or after")
    );
    assert!(!date_error.contains("before"));
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

slint::slint! {
    import { BoundButton } from "ui/library-controls.slint";
    export component LibraryGestureProbe inherits Window {
        width:240px;height:100px;
        in-out property <string> entry-key;
        in-out property <string> version;
        in-out property <string> operation;
        callback delivered(string,string,string);
        BoundButton {x:10px;y:10px;width:220px;height:50px;text:"Bound library action";key:root.entry-key;token:root.version;action:root.operation;dispatch(k,t,a)=>{root.delivered(k,t,a);}}
    }
}
#[test]
fn native_library_pointer_gesture_keeps_key_token_and_action_through_rebind() {
    install();
    let probe = LibraryGestureProbe::new().unwrap();
    probe.set_entry_key("user:first".into());
    probe.set_version("observed-first".into());
    probe.set_operation("trash".into());
    let received = Rc::new(RefCell::new(None));
    let result = received.clone();
    probe.on_delivered(move |k, t, a| {
        *result.borrow_mut() = Some((k.to_string(), t.to_string(), a.to_string()))
    });
    probe.show().unwrap();
    settle(probe.window());
    let position = slint::LogicalPosition::new(50.0, 30.0);
    probe.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    probe.set_entry_key("user:replacement".into());
    probe.set_version("replacement-token".into());
    probe.set_operation("restore".into());
    settle(probe.window());
    probe.window().dispatch_event(WindowEvent::PointerReleased {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    assert_eq!(
        *received.borrow(),
        Some(("user:first".into(), "observed-first".into(), "trash".into()))
    );
}
#[test]
fn persistent_native_transport_stays_inside_800_by_600_with_large_text() {
    let temp = tempfile::tempdir().unwrap();
    let root = auditorylab::library::LibraryRoot::explicit(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = auditorylab::library::LibraryStore::new(root).unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store,
        )
        .unwrap(),
    );
    h.wait_for_files();
    for (width, height, size) in [
        (1440.0, 960.0, 13.0),
        (800.0, 600.0, 13.0),
        (800.0, 600.0, 26.0),
    ] {
        h.ui().set_ui_size(size);
        h.ui()
            .window()
            .set_size(slint::LogicalSize::new(width, height));
        h.settle();
        for tab in [0, 1] {
            h.ui().set_tab_index(tab);
            h.settle();
            let button = labelled(h.ui(), "Audio details");
            let rectangle = button.geometry();
            let origin = button.map_to_window(rectangle.origin);
            assert!(
                origin.x >= 0.0
                    && origin.y >= 0.0
                    && origin.x + rectangle.size.width <= width + 0.5
                    && origin.y + rectangle.size.height <= height + 0.5,
                "transport outside {width}x{height} at {size}px: {origin:?} {rectangle:?}"
            );
        }
    }
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn native_open_add_external_save_conflict_and_independent_copy_keep_originals_distinct() {
    let temp = tempfile::tempdir().unwrap();
    let root = auditorylab::library::LibraryRoot::explicit(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = auditorylab::library::LibraryStore::new(root).unwrap();
    let file = temp.path().join("external.toml");
    let mut draft = auditorylab::editing::Draft::new().unwrap();
    draft.set_title("External original".into());
    let original = draft.recipe();
    std::fs::write(&file, original.to_toml().unwrap()).unwrap();
    let initial_bytes = std::fs::read(&file).unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store.clone(),
        )
        .unwrap(),
    );
    h.wait_for_files();
    let ui = h.ui();
    ui.invoke_open_recipe();
    let [Request::Path(prompt, PathPurpose::OpenRecipe, _)] = h.take()[..] else {
        panic!("Open picker missing")
    };
    h.adapter
        .respond(prompt, PromptAnswer::Path(Some(file.clone())));
    h.wait_for_files();
    assert_eq!(ui.get_collection_choice(), 3);
    assert!(
        store
            .scan(&auditorylab::workflow::Cancel::default())
            .entries
            .is_empty()
    );
    let external_key = ui.get_library_key();
    ui.invoke_add_to_library(external_key.clone());
    h.wait_for_files();
    assert_eq!(
        store
            .scan(&auditorylab::workflow::Cancel::default())
            .entries
            .len(),
        1
    );
    assert_eq!(std::fs::read(&file).unwrap(), initial_bytes);
    ui.invoke_edit_saved(external_key, "".into());
    h.settle();
    assert!(ui.get_draft_location().starts_with("External file"));
    assert_eq!(
        h.adapter
            .inspect(|w| w.editor().draft().recipe().document.id),
        original.document.id
    );
    ui.invoke_commit_metadata(
        ui.get_document(),
        MetadataKey::Title,
        "External edited".into(),
    );
    ui.invoke_save_recipe();
    h.wait_for_files();
    assert_eq!(
        auditorylab::recipe::load_file(&file).unwrap().title(),
        "External edited"
    );
    assert_eq!(
        store
            .scan(&auditorylab::workflow::Cancel::default())
            .entries[0]
            .title,
        "External original"
    );
    assert!(!ui.get_dirty());
    let mut other = auditorylab::recipe::load_file(&file)
        .unwrap()
        .recipe()
        .clone();
    other.description.as_mut().unwrap().title = "Other application".into();
    std::fs::write(&file, other.to_toml().unwrap()).unwrap();
    ui.invoke_commit_metadata(
        ui.get_document(),
        MetadataKey::Title,
        "Retained after conflict".into(),
    );
    ui.invoke_save_recipe();
    h.wait_for_files();
    assert!(ui.get_dirty());
    assert!(ui.get_status().contains("changed identity or bytes"));
    assert_eq!(
        auditorylab::recipe::load_file(&file).unwrap().title(),
        "Other application"
    );
    ui.invoke_save_copy();
    h.wait_for_files();
    assert!(!ui.get_dirty());
    assert_ne!(
        h.adapter
            .inspect(|w| w.editor().draft().recipe().document.id),
        original.document.id
    );
    assert_eq!(
        store
            .scan(&auditorylab::workflow::Cancel::default())
            .entries
            .len(),
        2
    );
    assert_eq!(
        auditorylab::recipe::load_file(&file).unwrap().title(),
        "Other application"
    );
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn native_readonly_labels_escape_controls_and_bidi_without_rewriting_editable_content() {
    let temp = tempfile::tempdir().unwrap();
    let root = auditorylab::library::LibraryRoot::explicit(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = auditorylab::library::LibraryStore::new(root).unwrap();
    let raw = "Label \u{202e}User\u{001b}";
    let mut draft = auditorylab::editing::Draft::new().unwrap();
    draft.set_title(raw.into());
    let record = store
        .apply(
            auditorylab::library::Mutation::Add(std::sync::Arc::new(draft.recipe())),
            &auditorylab::workflow::Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store.clone(),
        )
        .unwrap(),
    );
    h.wait_for_files();
    let ui = h.ui();
    ui.invoke_collection_filter(2, 0);
    h.wait_for_files();
    assert_eq!(ui.get_sound_title(), "Label \\u{202e}User\\u{1b}");
    assert!(
        !ui.get_catalog()
            .row_data(0)
            .unwrap()
            .text
            .contains('\u{202e}')
    );
    ui.invoke_edit_saved(ui.get_library_key(), ui.get_library_token());
    h.settle();
    assert_eq!(ui.get_title_field().text, raw);
    assert_eq!(
        store
            .load(&record.key)
            .unwrap()
            .snapshot
            .recipe()
            .description
            .as_ref()
            .unwrap()
            .title,
        raw
    );
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn native_library_keyboard_has_one_tab_stop_and_keeps_selected_key_through_sort() {
    let temp = tempfile::tempdir().unwrap();
    let root = auditorylab::library::LibraryRoot::explicit(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = auditorylab::library::LibraryStore::new(root).unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store,
        )
        .unwrap(),
    );
    h.wait_for_files();
    let ui = h.ui();
    let label = ui
        .get_catalog()
        .row_data(usize::try_from(ui.get_selected()).unwrap())
        .unwrap()
        .text;
    let row = labelled(ui, &label);
    let focus = row
        .visit_descendants(|item| {
            if item
                .downcast::<slint::private_unstable_api::re_exports::FocusScope>()
                .is_some()
            {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap();
    WindowInner::from_pub(ui.window()).set_focus_item(&focus, true, FocusReason::Programmatic);
    h.settle();
    let first = ui.get_library_key();
    h.key(Key::DownArrow);
    assert_ne!(ui.get_library_key(), first);
    let selected = ui.get_library_key();
    assert_eq!(ui.get_focused_library_key(), selected);
    ui.invoke_collection_filter(1, 2);
    h.settle();
    assert_eq!(ui.get_library_key(), selected);
    assert_eq!(ui.get_focused_library_key(), selected);
    h.key(Key::Tab);
    assert!(
        ui.get_focused_library_key().is_empty(),
        "Tab must leave the results instead of traversing every row"
    );
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

fn reveal_library_control(h: &Harness, prefix: &str) -> ItemRc {
    for _ in 0..8 {
        let item = labelled(h.ui(), prefix);
        let position = centre(&item);
        if item.is_visible() && (0.0..850.0).contains(&position.y) {
            return item;
        }
        h.ui()
            .window()
            .dispatch_event(WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(780.0, 400.0),
                delta_x: 0.0,
                delta_y: 400.0 - position.y,
            });
        h.settle();
    }
    panic!("control {prefix} did not scroll into view");
}

#[test]
fn compact_empty_filters_and_recovery_remain_reachable_through_real_controls() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let temp = tempfile::tempdir().unwrap();
    let root = auditorylab::library::LibraryRoot::explicit(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = auditorylab::library::LibraryStore::new(root).unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store,
        )
        .unwrap(),
    );
    h.wait_for_files();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window().set_size(slint::LogicalSize::new(800.0, 900.0));
    h.settle();
    assert!(ui.get_compact());
    let open_tools = || {
        let button = action_button(ui, "Filters / recovery…");
        let position = centre(&button);
        assert!(position.x >= 0.0 && position.x < 800.0 && position.y < 900.0);
        drag(ui, position, position);
        h.settle();
        assert!(ui.get_library_detail());
        assert!(ui.get_library_tools_open(), "entering tools must never hide them");
    };
    open_tools();
    labelled(ui, "More filters and grouping").accessible_action(&AccessibilityAction::Default);
    h.settle();
    ui.set_study_query("missing-study".into());
    ui.set_analysis_choice(2);
    ui.set_created_from("2026-01-01".into());
    reveal_library_control(&h, "Apply filters").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert_eq!(ui.get_catalog().row_count(), 0);
    let committed = ui.get_library_count();
    assert!(
        committed.contains("Study: missing-study")
            && committed.contains("Analysis: Partial")
            && committed.contains("Created from inclusive:")
    );
    ui.set_study_query("UNSUBMITTED".into());
    ui.set_created_from("invalid".into());
    reveal_library_control(&h, "Apply filters").accessible_action(&AccessibilityAction::Default);
    assert_eq!(
        ui.get_library_count(),
        committed,
        "invalid staged fields cannot replace applied summary"
    );
    reveal_library_control(&h, "Back to recipes").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert!(!ui.get_library_detail());
    assert!(labelled(ui, "Applied library filters").is_visible());
    // Keyboard activation reaches the same production button with no result row.
    let tools = action_button(ui, "Filters / recovery…");
    WindowInner::from_pub(ui.window()).set_focus_item(&tools, true, FocusReason::Programmatic);
    h.key(Key::Return);
    assert!(ui.get_library_detail());
    reveal_library_control(&h, "Back to recipes").accessible_action(&AccessibilityAction::Default);
    h.settle();
    let clear = centre(&action_button(ui, "Clear all filters"));
    drag(ui, clear, clear);
    h.settle();
    assert_eq!(ui.get_catalog().row_count(), 23);
    assert!(ui.get_library_count().contains("Applied filters: None"));
    for scope in [2, 3, 4] {
        ui.invoke_collection_filter(scope, 2);
        h.settle();
        assert_eq!(ui.get_catalog().row_count(), 0);
        open_tools();
        if !ui.get_storage_open() {
            reveal_library_control(&h, "Library storage and recovery")
                .accessible_action(&AccessibilityAction::Default);
        }
        h.settle();
        assert!(reveal_library_control(&h, "Initialize configured library").is_visible());
        reveal_library_control(&h, "Back to recipes")
            .accessible_action(&AccessibilityAction::Default);
        h.settle();
        let refresh = centre(&action_button(ui, "Refresh"));
        drag(ui, refresh, refresh);
        h.wait_for_files();
        assert_eq!(
            h.adapter.inspect(|w| w.catalog_query().sort),
            auditorylab::collection::SortBy::Title
        );
        assert_eq!(ui.get_collection_choice(), scope);
    }
    ui.invoke_collection_filter(2, 0);
    h.settle();
    action_button(ui, "Create sound").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert_eq!(h.adapter.inspect(|w| w.view()), View::Workbench);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn compact_unavailable_root_keeps_recovery_and_non_authoritative_counts_visible() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("library");
    let root = auditorylab::library::LibraryRoot::explicit(path.clone()).unwrap();
    root.initialize().unwrap();
    let store = auditorylab::library::LibraryStore::new(root).unwrap();
    std::fs::rename(&path, temp.path().join("offline")).unwrap();
    std::fs::write(&path, "not a directory").unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store,
        )
        .unwrap(),
    );
    h.wait_for_files();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window().set_size(slint::LogicalSize::new(800.0, 900.0));
    ui.invoke_collection_filter(2, 0);
    h.settle();
    assert_eq!(ui.get_catalog().row_count(), 0);
    for size in [13.0, 26.0] {
        ui.set_ui_size(size);
        ui.window().set_size(slint::LogicalSize::new(800.0, 600.0));
        h.settle();
        for name in [
            "Filters / recovery…",
            "Refresh",
            "Clear all filters",
            "Open Recipe…",
            "Create sound",
            "Play selected sound",
        ] {
            let position = centre(&action_button(ui, name));
            assert!(
                position.x >= 0.0 && position.x < 800.0 && position.y >= 0.0 && position.y < 600.0,
                "{name} must remain reachable at {size}px, got {position:?}"
            );
        }
    }
    ui.set_ui_size(13.0);
    ui.window().set_size(slint::LogicalSize::new(800.0, 900.0));
    h.settle();

    assert!(ui.get_library_count().contains("Inventory incomplete"));
    assert!(ui.get_library_alert().contains("incomplete"));
    action_button(ui, "Filters / recovery…").accessible_action(&AccessibilityAction::Default);
    h.settle();
    labelled(ui, "Library storage and recovery").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert!(reveal_library_control(&h, "Initialize configured library").is_visible());
}

#[test]
fn compact_back_keeps_result_scroll_and_large_text_rows_wrap_with_summary_dates() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let temp = tempfile::tempdir().unwrap();
    let store = auditorylab::library::LibraryStore::new(
        auditorylab::library::LibraryRoot::explicit(temp.path().join("library")).unwrap(),
    )
    .unwrap();
    store.root().initialize().unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store,
        )
        .unwrap(),
    );
    h.wait_for_files();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window().set_size(slint::LogicalSize::new(800.0, 900.0));
    h.settle();
    std::thread::sleep(Duration::from_millis(15));
    h.settle();
    ui.window().dispatch_event(WindowEvent::PointerScrolled {
        position: slint::LogicalPosition::new(700.0, 650.0),
        delta_x: 0.0,
        delta_y: -350.0,
    });
    h.settle();
    let scroll = ui.get_library_scroll_y();
    assert!(scroll < 0.0);
    let key = ui.get_library_key();
    action_button(ui, "Filters / recovery…").accessible_action(&AccessibilityAction::Default);
    h.settle();
    reveal_library_control(&h, "Back to recipes").accessible_action(&AccessibilityAction::Default);
    // A late native frame must not let the enclosing restore timer race
    // the selected row's deferred initial reveal.
    std::thread::sleep(Duration::from_millis(30));
    h.settle();
    std::thread::sleep(Duration::from_millis(20));
    h.settle();
    assert!(
        (ui.get_library_scroll_y() - scroll).abs() < 1.0,
        "Back keeps scroll {scroll}, got {}",
        ui.get_library_scroll_y()
    );
    assert_eq!(ui.get_library_key(), key);
    action_button(ui, "Filters / recovery…").accessible_action(&AccessibilityAction::Default);
    h.settle();
    ui.invoke_more_filters(
        "no-such-study".into(),
        0,
        0,
        0,
        0,
        0,
        "".into(),
        "".into(),
        "".into(),
        "".into(),
        "".into(),
        "".into(),
    );
    h.settle();
    reveal_library_control(&h, "Back to recipes").accessible_action(&AccessibilityAction::Default);
    // A late native frame must not let the enclosing restore timer race
    // the selected row's deferred initial reveal.
    std::thread::sleep(Duration::from_millis(30));
    h.settle();
    std::thread::sleep(Duration::from_millis(20));
    h.settle();
    assert_eq!(ui.get_catalog().row_count(), 0);
    assert!(
        ui.get_library_scroll_y().abs() < 1.0,
        "empty results must clamp the previous result scroll, got {}",
        ui.get_library_scroll_y()
    );
    action_button(ui, "Clear all filters").accessible_action(&AccessibilityAction::Default);
    h.settle();
    ui.set_ui_size(26.0);
    h.settle();
    let label = ui.get_catalog().row_data(0).unwrap().text;
    assert!(label.contains("Duration") && label.contains("Created: Unknown"));
    let row = labelled(ui, &label);
    let row_rect = row.geometry();
    assert!(
        row_rect.size.width <= 800.0 && row_rect.size.height > 26.0 * 4.5,
        "multi-line row grows rather than clipping its new metadata"
    );
    let child = row.visit_descendants(|item| {
        if item
            .accessible_string_property(AccessibleStringProperty::Label)
            .as_deref()
            == Some(ui.get_catalog().row_data(0).unwrap().title.as_str())
            && item.geometry().size.width < row_rect.size.width
        {
            ControlFlow::Break(item.clone())
        } else {
            ControlFlow::Continue(())
        }
    });
    let child = child.expect("wrapped row text is present within the row");
    assert!(child.geometry().size.height <= row_rect.size.height);
}

#[test]
fn local_dates_expose_exact_utc_and_reset_sort_preserves_committed_filters() {
    use auditorylab::library::{LibraryKey, LibraryRoot, LibraryStore, Mutation};
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let temp = tempfile::tempdir().unwrap();
    let store =
        LibraryStore::new(LibraryRoot::explicit(temp.path().join("library")).unwrap()).unwrap();
    store.root().initialize().unwrap();
    let draft = auditorylab::editing::Draft::from_snapshot(
        &auditorylab::recipe::builtin("ioannou-2015-binaural-2-hz").unwrap(),
    )
    .unwrap();
    let mut recipe = draft.recipe();
    let created = "2026-10-25T00:30:00.123456789Z".parse().unwrap();
    recipe.document.created_at = Some(created);
    recipe.document.modified_at = None;
    let id = recipe.document.id;
    store
        .apply(
            Mutation::Add(std::sync::Arc::new(recipe)),
            &auditorylab::workflow::Cancel::default(),
        )
        .unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store,
        )
        .unwrap(),
    );
    h.wait_for_files();
    let ui = h.ui();
    ui.invoke_collection_filter(2, 3);
    h.wait_for_files();
    let row = ui.get_catalog().row_data(0).unwrap().text;
    assert!(
        row.contains("Edited: Unknown") && row.contains("Added here:") && row.contains("Created:")
    );
    assert!(
        ui.get_library_dates()
            .contains(&auditorylab::calendar::local_timestamp(created))
    );
    assert!(
        ui.get_library_dates_utc()
            .contains("2026-10-25T00:30:00.123456789Z")
    );
    assert!(ui.get_library_dates_utc().contains("Modified: Unknown"));
    ui.invoke_search("2".into());
    h.settle();
    let key = LibraryKey::User(id).to_string();
    assert_eq!(ui.get_library_key(), key);
    action_button(ui, "Filters / recovery…").accessible_action(&AccessibilityAction::Default);
    h.settle();
    action_button(ui, "Reset sort").accessible_action(&AccessibilityAction::Default);
    h.settle();
    action_button(ui, "Filters / recovery…").accessible_action(&AccessibilityAction::Default);
    h.settle();
    h.adapter.inspect(|w| {
        let q = w.catalog_query();
        assert_eq!(q.scope, auditorylab::collection::Scope::User);
        assert_eq!(q.sort, auditorylab::collection::SortBy::Automatic);
        assert!(q.direction.is_none());
        assert_eq!(q.search, "2");
    });
    reveal_library_control(&h, "Exact UTC dates").accessible_action(&AccessibilityAction::Default);
    h.settle();
    let exact = field(ui, "Exact UTC recipe and membership dates");
    assert!(
        exact.geometry().size.height >= ui.get_ui_size() * 3.0,
        "UTC text must have visible selectable geometry"
    );
    let before = text_of(&exact);
    WindowInner::from_pub(ui.window()).set_focus_item(&exact, true, FocusReason::Programmatic);
    h.key("x");
    assert_eq!(text_of(&exact), before, "UTC presentation is read-only");
    assert!(text_of(&exact).contains("2026-10-25T00:30:00.123456789Z"));
}

#[test]
fn changing_display_vocabulary_preserves_typed_library_search() {
    let temp = tempfile::tempdir().unwrap();
    let root = auditorylab::library::LibraryRoot::explicit(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = auditorylab::library::LibraryStore::new(root).unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store,
        )
        .unwrap(),
    );
    h.wait_for_files();
    h.ui().invoke_search("clicks 40".into());
    h.wait_for_files();
    let before = h
        .ui()
        .get_catalog()
        .iter()
        .map(|row| row.binding)
        .collect::<Vec<_>>();
    assert_eq!(before.len(), 1);
    h.ui()
        .global::<crate::generated::Terms>()
        .set_caption_structural_method("Different display wording".into());
    h.ui()
        .global::<crate::generated::Terms>()
        .set_collection_library("Different collection wording".into());
    h.ui().invoke_search("".into());
    h.wait_for_files();
    assert!(h.ui().get_catalog().row_count() > before.len());
    h.ui().invoke_search("clicks 40".into());
    h.wait_for_files();
    let after = h
        .ui()
        .get_catalog()
        .iter()
        .map(|row| row.binding)
        .collect::<Vec<_>>();
    assert_eq!(after, before);
}

#[test]
fn filter_controls_map_to_independent_typed_queries() {
    use auditorylab::collection::{CatalogQuery, DateRange, Direction, GroupBy, Scope, SortBy};
    use auditorylab::core::nominal::{Band, Method};
    use auditorylab::query::RhythmQuery;
    let temp = tempfile::tempdir().unwrap();
    let store = auditorylab::library::LibraryStore::new(
        auditorylab::library::LibraryRoot::explicit(temp.path().join("library")).unwrap(),
    )
    .unwrap();
    store.root().initialize().unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab::recipe::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store,
        )
        .unwrap(),
    );
    h.wait_for_files();
    let ui = h.ui();
    for (choice, scope) in [
        (0, Scope::All),
        (1, Scope::BuiltIn),
        (2, Scope::User),
        (3, Scope::OpenFiles),
        (4, Scope::Trash),
    ] {
        for (sort_choice, sort) in [
            (0, SortBy::Automatic),
            (1, SortBy::Rate),
            (2, SortBy::Title),
            (3, SortBy::Created),
            (4, SortBy::Modified),
            (5, SortBy::Added),
            (6, SortBy::Study),
            (7, SortBy::Collection),
        ] {
            ui.invoke_collection_filter(choice, sort_choice);
            h.wait_for_files();
            let expected = CatalogQuery {
                scope,
                sort,
                ..CatalogQuery::default()
            };
            h.adapter
                .inspect(|w| assert_eq!(w.catalog_query(), &expected));
        }
    }
    ui.invoke_collection_filter(0, 0);
    h.wait_for_files();
    for (band_choice, band) in [
        (1, Band::Delta),
        (2, Band::Theta),
        (3, Band::Alpha),
        (4, Band::Beta),
        (5, Band::Gamma),
    ] {
        for (method_choice, method) in [
            (1, Method::Binaural),
            (2, Method::Monaural),
            (3, Method::Clicks),
            (4, Method::Gate),
            (5, Method::AmplitudeModulation),
            (6, Method::SteadyTone),
        ] {
            ui.invoke_rhythm_filter(band_choice, method_choice);
            h.wait_for_files();
            let expected = CatalogQuery {
                rhythm: RhythmQuery {
                    bands: vec![band],
                    methods: vec![method],
                    state: None,
                },
                ..CatalogQuery::default()
            };
            h.adapter
                .inspect(|w| assert_eq!(w.catalog_query(), &expected));
        }
    }
    ui.invoke_rhythm_filter(0, 0);
    h.wait_for_files();
    ui.invoke_more_filters(
        "key:source".into(),
        2,
        2,
        2,
        2,
        3,
        "2026-01-01".into(),
        "2026-01-02".into(),
        "2026-03-01".into(),
        "2026-03-02".into(),
        "2026-05-01".into(),
        "2026-05-02".into(),
    );
    h.wait_for_files();
    let expected = CatalogQuery {
        studies: vec!["key:source".into()],
        populations: vec![auditorylab::core::Population::Mouse],
        roles: vec![auditorylab::core::ConditionRole::Comparison],
        analysis: Some(auditorylab::core::nominal::Completeness::Partial),
        direction: Some(Direction::Desc),
        group_by: GroupBy::Study,
        created: auditorylab::calendar::local_range(
            auditorylab::calendar::CalendarField::Created,
            "2026-01-01",
            "2026-01-02",
        )
        .unwrap(),
        modified: auditorylab::calendar::local_range(
            auditorylab::calendar::CalendarField::Modified,
            "2026-03-01",
            "2026-03-02",
        )
        .unwrap(),
        added: auditorylab::calendar::local_range(
            auditorylab::calendar::CalendarField::Added,
            "2026-05-01",
            "2026-05-02",
        )
        .unwrap(),
        ..CatalogQuery::default()
    };
    h.adapter
        .inspect(|w| assert_eq!(w.catalog_query(), &expected));
    ui.invoke_clear_library_filters();
    h.wait_for_files();
    let cleared = CatalogQuery {
        direction: Some(Direction::Desc),
        group_by: GroupBy::Study,
        created: DateRange::default(),
        ..CatalogQuery::default()
    };
    h.adapter
        .inspect(|w| assert_eq!(w.catalog_query(), &cleared));
}

#[test]
fn real_save_capture_and_delayed_completion_preserve_newer_edits_and_close_prompting() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_commit_metadata(
        ui.get_document(),
        MetadataKey::Title,
        "Captured by Save".into(),
    );
    ui.invoke_save_recipe();
    let save = take_job(&h, JobKind::File);
    assert!(h.take().is_empty());
    let document = ui.get_document();
    ui.invoke_commit_metadata(
        ui.get_document(),
        MetadataKey::Title,
        "Newer unsaved edits".into(),
    );
    // Execute the real capture while withholding its Finished response. Physical
    // thread joining is the Runner's app-unit contract, not simulated by this test.
    let finished = save.execute(|_| panic!("a save emits no preparation"));
    let saved = h
        .store()
        .scan(&auditorylab::workflow::Cancel::default())
        .entries
        .remove(0);
    assert_eq!(saved.title, "Captured by Save");
    assert!(h.adapter.inspect(|w| w.file_busy()));
    assert!(ui.get_dirty());
    ui.invoke_quit();
    assert!(h.adapter.inspect(|w| w.action_pending()));
    assert!(
        h.take().is_empty(),
        "Close cannot resolve before completion is delivered"
    );
    h.adapter.deliver(finished);
    let [Request::Unsaved(prompt)] = h.take()[..] else {
        panic!("newer edits still need an explicit close decision");
    };
    assert_eq!(ui.get_document(), document);
    assert_eq!(ui.get_title_field().text, "Newer unsaved edits");
    assert!(ui.get_dirty());
    h.adapter.respond(prompt, PromptAnswer::Cancel);
    assert_eq!(h.take(), [Request::Close(false)]);
    assert_eq!(ui.get_title_field().text, "Newer unsaved edits");
}

#[test]
fn background_refresh_does_not_hide_a_failed_captured_save() {
    use auditorylab::library::Mutation;
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_commit_metadata(
        ui.get_document(),
        MetadataKey::Title,
        "Initial saved title".into(),
    );
    ui.invoke_save_recipe();
    h.wait_for_files();
    let saved = h
        .store()
        .scan(&auditorylab::workflow::Cancel::default())
        .entries
        .remove(0);
    ui.invoke_commit_metadata(
        ui.get_document(),
        MetadataKey::Title,
        "Retained draft".into(),
    );
    ui.invoke_save_recipe();
    let save = take_job(&h, JobKind::File);
    h.store()
        .apply(
            Mutation::Rename {
                key: saved.key.clone(),
                expect: saved.version_token,
                title: "Other writer".into(),
            },
            &auditorylab::workflow::Cancel::default(),
        )
        .unwrap();
    h.adapter.command(Workspace::refresh_library);
    finish_file(&h, save);
    h.wait_for_files();
    assert!(ui.get_dirty());
    assert_eq!(ui.get_title_field().text, "Retained draft");
    assert_eq!(
        h.store().load(&saved.key).unwrap().snapshot.title(),
        "Other writer"
    );
    let details = h.adapter.inspect(|w| w.action_view().operation_details);
    assert!(
        details.to_lowercase().contains("conflict") || details.to_lowercase().contains("changed"),
        "foreground save failure remains visible after background reads: {details}"
    );
}

#[test]
fn inclusive_date_errors_name_each_visible_pair_and_keep_inputs_and_query() {
    let h = Harness::new();
    let ui = h.ui();
    for (field, label) in [(0, "Created"), (1, "Modified"), (2, "Added here")] {
        ui.invoke_clear_library_filters();
        h.wait_for_files();
        ui.set_created_from("".into());
        ui.set_created_through("".into());
        ui.set_modified_from("".into());
        ui.set_modified_through("".into());
        ui.set_added_from("".into());
        ui.set_added_through("".into());
        let previous = h.adapter.inspect(|w| w.catalog_query().clone());
        let mut from = ["", "", ""];
        let mut through = ["", "", ""];
        from[field] = "2026-10-26";
        through[field] = "2026-10-25";
        match field {
            0 => {
                ui.set_created_from(from[field].into());
                ui.set_created_through(through[field].into());
            }
            1 => {
                ui.set_modified_from(from[field].into());
                ui.set_modified_through(through[field].into());
            }
            _ => {
                ui.set_added_from(from[field].into());
                ui.set_added_through(through[field].into());
            }
        }
        ui.invoke_more_filters(
            "".into(),
            0,
            0,
            0,
            0,
            0,
            from[0].into(),
            through[0].into(),
            from[1].into(),
            through[1].into(),
            from[2].into(),
            through[2].into(),
        );
        h.wait_for_files();
        h.adapter.inspect(|w| {
            assert_eq!(w.catalog_query(), &previous);
            let error = w.library_alert();
            assert!(error.contains(&format!(
                "{label} through (2026-10-25) must be on or after {label} from (2026-10-26)."
            )));
            assert!(!error.contains("before"));
        });
        let values = match field {
            0 => (ui.get_created_from(), ui.get_created_through()),
            1 => (ui.get_modified_from(), ui.get_modified_through()),
            _ => (ui.get_added_from(), ui.get_added_through()),
        };
        assert_eq!(
            (values.0.as_str(), values.1.as_str()),
            ("2026-10-26", "2026-10-25")
        );
        from[field] = "2026-10-25";
        match field {
            0 => ui.set_created_from(from[field].into()),
            1 => ui.set_modified_from(from[field].into()),
            _ => ui.set_added_from(from[field].into()),
        }
        ui.invoke_more_filters(
            "".into(),
            0,
            0,
            0,
            0,
            0,
            from[0].into(),
            through[0].into(),
            from[1].into(),
            through[1].into(),
            from[2].into(),
            through[2].into(),
        );
        h.wait_for_files();
        h.adapter.inspect(|w| {
            assert!(w.library_alert().is_empty());
            let query = w.catalog_query();
            let range = match field {
                0 => &query.created,
                1 => &query.modified,
                _ => &query.added,
            };
            assert!(range.from.unwrap() < range.before.unwrap());
        });
    }
}


#[test]
fn wide_library_navigation_uses_real_shared_filters_and_structured_cards() {
    use auditorylab::core::nominal::Band;
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window().set_size(slint::LogicalSize::new(1440.0, 1000.0));
    h.settle();
    assert!(ui.get_library_wide());
    assert_eq!(ui.get_band_navigation().row_count(), 10);
    for (index, band) in Band::ALL.iter().enumerate() {
        let facet = ui.get_band_navigation().row_data(index + 1).unwrap();
        assert_eq!(facet.label, band.to_string());
        assert_eq!(facet.detail, band.interval());
        let item = labelled(ui, &facet.text);
        assert!(item.is_visible(), "band navigation is not a closed dropdown");
        assert!(item.geometry().size.height > 0.0);
    }
    let delta = ui.get_band_navigation().row_data(1).unwrap();
    labelled(ui, &delta.text).accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert_eq!(h.adapter.inspect(|w| w.catalog_query().rhythm.bands.clone()), vec![Band::Delta]);
    assert_eq!(ui.get_band_choice(), 1);
    assert_eq!(ui.get_catalog().row_count(), 1);
    let row = ui.get_catalog().row_data(0).unwrap();
    assert!(!row.title.is_empty());
    assert!(row.collection.contains("Read only"));
    assert_eq!(row.bands, "Delta");
    assert!(row.duration.contains("1 min"));
    assert!(row.dates.contains("Unknown"));
    assert!(ui.get_library_classification().contains("not measured brain activity"));
    assert!(ui.get_library_ownership().contains("Read only"));
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    // The next query originates outside the widget: its selected state must follow.
    ui.invoke_rhythm_filter(3, 0);
    h.settle();
    assert_eq!(ui.get_band_choice(), 3);
    assert!(h.adapter.inspect(|w| w.catalog_query().rhythm.bands == [Band::Alpha]));
}

#[test]
fn wide_library_group_headings_have_no_selectable_identity() {
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window().set_size(slint::LogicalSize::new(1440.0, 900.0));
    ui.invoke_more_filters("".into(), 0, 0, 0, 0, 1,
        "".into(), "".into(), "".into(), "".into(), "".into(), "".into());
    h.settle();
    let rows = ui.get_catalog();
    assert!(rows.iter().any(|row| row.binding.is_empty()));
    assert!(rows.iter().filter(|row| row.binding.is_empty()).all(|row| row.collection.is_empty() && row.duration.is_empty()));
    let before = ui.get_library_key();
    ui.invoke_select("".into());
    assert_eq!(ui.get_library_key(), before);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
#[ignore = "explicit silent native visual evidence, not a device test"]
fn library_review_screenshots() {
    use std::io::Write;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    let destination = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/evidence/library-experience");
    std::fs::create_dir_all(&destination).unwrap();
    for (name, width, height) in [
        ("library-startup", 1280_u32, 900_u32),
        ("library-with-monitor", 1280_u32, 900_u32),
        ("library-wide", 1440_u32, 1000_u32),
        ("library-medium", 1024, 768),
        ("library-compact", 800, 600),
    ] {
        ui.set_monitor_open(name == "library-with-monitor");
        ui.window().set_size(slint::PhysicalSize::new(width, height));
        h.settle();
        let pixels = ui.window().take_snapshot().unwrap();
        let mut file = std::fs::File::create(destination.join(format!("{name}.ppm"))).unwrap();
        write!(file, "P6\n{} {}\n255\n", pixels.width(), pixels.height()).unwrap();
        for pixel in pixels.as_slice() {
            file.write_all(&[pixel.r, pixel.g, pixel.b]).unwrap();
        }
    }
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn applied_filters_remain_readable_when_library_tools_are_closed() {
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    for width in [1440.0, 1100.0, 800.0] {
        ui.window().set_size(slint::LogicalSize::new(width, 900.0));
        ui.set_library_tools_open(false);
        ui.invoke_more_filters("ioannou-2015".into(), 0, 0, 0, 0, 0,
            "".into(), "".into(), "".into(), "".into(), "".into(), "".into());
        h.settle();
        let summary = labelled(ui, "Applied library filters");
        // Selection reveal can scroll past the summary. It must remain readable
        // through the real list scroll control without opening secondary tools.
        let list = labelled(ui, "Library results");
        for _ in 0..8 {
            if summary.is_visible() {
                break;
            }
            ui.window().dispatch_event(WindowEvent::PointerScrolled {
                position: centre(&list),
                delta_x: 0.0,
                delta_y: 400.0,
            });
            h.settle();
        }
        assert!(summary.is_visible(), "applied filters are reachable at width {width}");
        assert!(summary.accessible_string_property(AccessibleStringProperty::Label)
            .is_some_and(|text| text.contains("Study: ioannou-2015")));
        assert!(!ui.get_library_tools_open());
    }
}


#[test]
fn startup_exposes_all_five_bands_and_monitor_is_an_explicit_choice() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let (workspace, _directory, _store) = bound_workspace(
        auditorylab::recipe::catalog().unwrap(),
        Capabilities::no_device_playback(),
    );
    let h = Harness::with_startup_workspace(workspace);
    let ui = h.ui();
    assert!(!ui.get_monitor_open(), "startup must leave room for discovery");
    ui.window().set_size(slint::LogicalSize::new(1280.0, 900.0));
    h.settle();
    assert!(ui.get_library_wide());
    let navigation = labelled(ui, "Browse by rhythm");
    let bounds = navigation.geometry();
    let origin = navigation.map_to_window(bounds.origin);
    for index in 1..=5 {
        let facet = ui.get_band_navigation().row_data(index).unwrap();
        let item = labelled(ui, &facet.text);
        let item_bounds = item.geometry();
        let item_origin = item.map_to_window(item_bounds.origin);
        assert!(item.is_visible());
        assert!(item_origin.y >= origin.y);
        assert!(item_origin.y + item_bounds.size.height <= origin.y + bounds.size.height,
            "{} must be visible without scrolling at the initial size", facet.label);
    }
    let selected = ui.get_library_key();
    action_button(ui, "Show output monitor").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert!(ui.get_monitor_open());
    assert_eq!(ui.get_library_key(), selected);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    action_button(ui, "Hide output monitor").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert!(!ui.get_monitor_open());
    assert_eq!(ui.get_library_key(), selected);
}
