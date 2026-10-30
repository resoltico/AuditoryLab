//! Production-binding integration tests: generated callbacks, wired by
//! [`SlintAdapter::install`] exactly as at startup, drive a real workspace.
//! Dialogs are answered through a recording shell; audio is the portable
//! workspace's typed refusal, so no device is touched.
use crate::adapter::{Shell, SlintAdapter, Windows};
use crate::generated::MainWindow;
use crate::test_platform::{install, settle};
use auditorylab_app::core::ParameterId;
use auditorylab_app::discovery::query::{Direction, GroupBy, Scope, SortBy};
use auditorylab_app::document::editing::{EditingField, MetadataField};
use auditorylab_app::document::origin::LibraryKey;
use auditorylab_app::workspace::{
    AudioCommand, Capabilities, Command, EditorCommand, Job, JobKind, LibraryCommand, PathPurpose,
    PromptAnswer, PromptId, View, Workspace,
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
    catalog: Vec<auditorylab_app::document::recipe::RecipeSnapshot>,
    capabilities: Capabilities,
) -> (
    Workspace,
    tempfile::TempDir,
    auditorylab_app::services::library::Library,
) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&base).unwrap();
    let directory = tempfile::tempdir_in(base).unwrap();
    let store =
        auditorylab_app::services::library::Library::at_path(directory.path().join("library"))
            .unwrap();
    store.initialize().unwrap();
    let workspace = Workspace::with_library(catalog, capabilities, store.clone()).unwrap();
    (workspace, directory, store)
}

pub(crate) struct Harness {
    pub(crate) windows: Windows,
    pub(crate) adapter: Rc<SlintAdapter>,
    requests: Rc<RefCell<Vec<Request>>>,
    library: Option<(
        tempfile::TempDir,
        auditorylab_app::services::library::Library,
    )>,
}

impl Harness {
    pub(crate) fn new() -> Self {
        Self::with_catalog(
            auditorylab_app::services::catalog().unwrap(),
            Capabilities::no_device_playback(),
        )
    }
    pub(crate) fn with_catalog(
        catalog: Vec<auditorylab_app::document::recipe::RecipeSnapshot>,
        capabilities: Capabilities,
    ) -> Self {
        let (workspace, directory, store) = bound_workspace(catalog, capabilities);
        let mut harness = Self::with_workspace(workspace);
        harness.library = Some((directory, store));
        harness
    }
    fn store(&self) -> &auditorylab_app::services::library::Library {
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
            Box::new(crate::adapter::tests::DeterministicExecutor::default()),
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
        self.ui().invoke_tab_changed(1);
        self.settle();
        assert_eq!(self.adapter.inspect(|w| w.view()), View::Workbench);
    }

    fn take(&self) -> Vec<Request> {
        std::mem::take(&mut *self.requests.borrow_mut())
    }

    fn component_key(&self, index: usize) -> slint::SharedString {
        self.ui().get_components().row_data(index).unwrap().key
    }

    fn keys(&self, index: usize) -> Vec<ParameterId> {
        let row = self.ui().get_components().row_data(index).unwrap();
        row.parameters
            .iter()
            .map(|p| field_parameter(&p.key))
            .collect()
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

fn find_control(ui: &MainWindow, predicate: impl Fn(&ItemRc) -> bool) -> Option<ItemRc> {
    let window = WindowInner::from_pub(ui.window());
    let mut roots: Vec<_> = window
        .active_popups()
        .iter()
        .rev()
        .map(|popup| ItemRc::new(popup.component.clone(), 0))
        .collect();
    roots.push(window.window_item_rc().unwrap());
    roots.into_iter().find_map(|root| {
        root.visit_descendants(|item| {
            if predicate(item) {
                ControlFlow::Break(item.clone())
            } else {
                ControlFlow::Continue(())
            }
        })
    })
}

/// The text input inside the control whose accessible label is `label`.
pub(crate) fn field(ui: &MainWindow, label: &str) -> ItemRc {
    let owner = find_control(ui, |item| {
        item.accessible_string_property(AccessibleStringProperty::Label)
            .as_deref()
            == Some(label)
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
        [ParameterId::ToneFrequency, ParameterId::PeakLevel]
    );
    let binding = h.component_key(0);
    let frequency = h.adapter.inspect(parameter(0, ParameterId::ToneFrequency));

    ui.invoke_stage_parameter(
        parameter_field(&binding.clone(), ParameterId::ToneFrequency),
        "bad".into(),
    );
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

    ui.invoke_commit_parameter(
        parameter_field(&binding.clone(), ParameterId::ToneFrequency),
        "3750".into(),
    );
    assert_eq!(
        h.adapter.inspect(|w| w.editor().draft().value(frequency)),
        Some(3750.0)
    );
    assert!(ui.get_can_save());

    ui.invoke_stage_parameter(
        parameter_field(&binding.clone(), ParameterId::ToneFrequency),
        "12".into(),
    );
    ui.invoke_restore_parameter(parameter_field(
        &binding.clone(),
        ParameterId::ToneFrequency,
    ));
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

    assert_metadata_and_component_callbacks(&h);
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
    let inventory = h.store().scan(&auditorylab_app::job::Cancel::default());
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
    ui.invoke_add_component(ui.get_document(), 0);
    // Independent oracle: the fields each kind has.
    let carriers = [
        (0, vec![ParameterId::ToneFrequency]),
        (
            1,
            vec![
                ParameterId::SourcePulseRate,
                ParameterId::SourcePulseDuration,
            ],
        ),
    ];
    let modulations = [
        (0, vec![]),
        (
            1,
            vec![ParameterId::ModulationRate, ParameterId::ModulationDepth],
        ),
        (
            2,
            vec![
                ParameterId::ModulationPulseRate,
                ParameterId::ModulationPulseDuration,
            ],
        ),
    ];
    for (carrier, carrier_keys) in &carriers {
        for (modulation, modulation_keys) in &modulations {
            ui.invoke_set_carrier(h.component_key(0), *carrier);
            ui.invoke_set_modulation(h.component_key(0), *modulation);
            let mut expected = carrier_keys.clone();
            expected.push(ParameterId::PeakLevel);
            expected.extend(modulation_keys);
            assert_eq!(
                h.keys(0),
                expected,
                "carrier {carrier} modulation {modulation}"
            );
            let row = ui.get_components().row_data(0).unwrap();
            for shown in row.parameters.iter() {
                let id = field_parameter(&shown.key);
                assert_eq!(
                    shown.label,
                    auditorylab_presentation::parameter_input_label(id)
                );
                assert_eq!(shown.help, auditorylab_presentation::parameter_help(id));
                assert_eq!(
                    shown.has_slider,
                    auditorylab_app::workspace::controls::range(id, Workspace::export_rates()[1])
                        .is_some()
                );
                ui.invoke_stage_parameter(shown.key.clone(), "1x".into());
                let field = h.adapter.inspect(parameter(0, id));
                assert_eq!(
                    h.adapter
                        .inspect(|w| w.editor().staged(field).map(str::to_owned)),
                    Some("1x".to_owned()),
                    "{:?} stages its own field",
                    shown.key
                );
                ui.invoke_restore_parameter(shown.key.clone());
            }
        }
    }
    // A key the component does not have is refused, not redirected.
    ui.invoke_set_carrier(h.component_key(0), 0);
    ui.invoke_set_modulation(h.component_key(0), 0);
    ui.invoke_stage_parameter(
        parameter_field(&h.component_key(0), ParameterId::ModulationPulseRate),
        "5".into(),
    );
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
    let input = h.field("Pulse train · Component 2: Source pulse rate (Hz)");
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
    ui.invoke_set_routing(first.clone(), 2);
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
        field(ui, "Pulse train · Component 2: Source pulse rate (Hz)"),
        input,
        "the moved row keeps its authored identity and the same widget"
    );
    let rate = h
        .adapter
        .inspect(parameter(0, ParameterId::SourcePulseRate));
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
        parameter_field(&old_component.clone(), ParameterId::ToneFrequency),
        "999".into(),
    );
    ui.invoke_commit_metadata(
        metadata_field_handle(&old_document.clone(), MetadataField::Title),
        "Old".into(),
    );
    ui.invoke_stage_metadata(
        metadata_field_handle(&old_document.clone(), MetadataField::Summary),
        "Old".into(),
    );
    ui.invoke_add_component(old_document.clone(), 1);
    h.adapter.inspect(|w| {
        assert_eq!(w.editor().draft().title(), fresh_title);
        assert_eq!(w.editor().draft().components(), []);
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
        .invoke_select(h.ui().get_catalog().row_data(1).unwrap().entry);
    h.adapter.tick();
    h.adapter.tick();
    assert_eq!(h.adapter.counts.path_builds.get(), paths + 1);
    assert!(h.adapter.counts.document_projections.get() > documents);
}

#[test]
fn an_explicit_tab_choice_applies_before_the_next_projection() {
    let h = Harness::new();
    let ui = h.ui();
    ui.invoke_tab_changed(1);
    // The user intent is synchronous; projection cannot schedule another choice.
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
    let label = "Steady tone · Component 1: Tone frequency (Hz) slider";
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
    ui.invoke_commit_parameter(
        parameter_field(&binding.clone(), ParameterId::ToneFrequency),
        "5000".into(),
    );
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
        let mut draft = auditorylab_app::document::editing::Draft::new().unwrap();
        draft.set_title(format!("A earlier opened recipe {index:02}"));
        let path = directory.path().join(format!("earlier-{index}.toml"));
        std::fs::write(&path, draft.recipe().unwrap().to_toml().unwrap()).unwrap();
        ui.invoke_open_recipe();
        answer_path(&h, &path);
        h.wait_for_files();
    }
    ui.set_library_scroll_y(0.0);
    h.settle();
    let path = directory.path().join("imported.toml");
    std::fs::write(&path, {
        let mut recipe = crate::test_recipes::silent(1.0, 0.01).into_builder();
        recipe.content.description = Some(auditorylab_app::core::provenance::Description {
            title: "Imported probe".into(),
            notes: None,
        });
        recipe.build().unwrap().to_toml().unwrap()
    })
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
        let bottom =
            (row_origin.y + row_bounds.size.height).min(list_origin.y + list_bounds.size.height);
        if bottom - top > 8.0 {
            pointer = Some(slint::LogicalPosition::new(
                row_origin.x + row_bounds.size.width / 2.0,
                f32::midpoint(top, bottom),
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
        "Removed Steady tone · Component 1."
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
fn component_options_are_visible_and_sized_to_their_content() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    h.settle();
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
    let choices: Vec<_> = ["route", "carrier", "modulation"]
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
    assert!(remove_x > choices[0].0, "Remove sits in the component header, right of the choices");
}

/// The first item whose accessible label starts with `prefix`.
fn labelled(ui: &MainWindow, prefix: &str) -> ItemRc {
    find_control(ui, |item| {
        item.accessible_string_property(AccessibleStringProperty::Label)
            .is_some_and(|label| label.starts_with(prefix))
    })
    .unwrap_or_else(|| panic!("an item labelled {prefix:?}"))
}

fn centre(item: &ItemRc) -> slint::LogicalPosition {
    let geometry = item.geometry();
    let origin = item.map_to_native_window(geometry.origin);
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
        auditorylab_app::identity::BUILD_IDENTITY
            .revision
            .map_or_else(
                || format!(
                    "Version {}",
                    auditorylab_app::identity::BUILD_IDENTITY.version
                ),
                |revision| format!(
                    "Version {} ({})",
                    auditorylab_app::identity::BUILD_IDENTITY.version,
                    &revision[..12]
                )
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
        ui.invoke_commit_parameter(
            parameter_field(&previous.clone(), ParameterId::ToneFrequency),
            "999".into(),
        );
        assert_eq!(
            h.adapter.inspect(|w| w
                .editor()
                .draft()
                .component(identity)
                .unwrap()
                .get(ParameterId::ToneFrequency)),
            Some(finite(440.0))
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
    ui.invoke_commit_parameter(
        parameter_field(&binding.clone(), ParameterId::ToneFrequency),
        "880".into(),
    );
    ui.invoke_stage_parameter(
        parameter_field(&binding.clone(), ParameterId::ToneFrequency),
        "900".into(),
    );
    for _ in 0..20 {
        h.adapter.tick();
    }
    assert_eq!(h.adapter.counts.library_projections.get(), libraries);
    ui.invoke_select(ui.get_catalog().row_data(1).unwrap().entry);
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
    let mut draft = auditorylab_app::document::editing::Draft::new().unwrap();
    draft.set_duration_seconds(finite(0.01));
    draft.set_fade_seconds(finite(0.0));
    let h = Harness::with_catalog(vec![draft.snapshot().unwrap()], Capabilities::available());
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
    assert!(
        h.adapter
            .command(|w| w.apply(Command::Audio(AudioCommand::Stop)))
            .unwrap()
            .is_ok()
    );
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
fn library_play_command_admits_play_during_save() {
    assert_library_play_during_file(true);
}
#[test]
fn library_play_command_admits_play_during_import() {
    assert_library_play_during_file(false);
}

fn action_button(ui: &MainWindow, label: &str) -> ItemRc {
    find_control(ui, |item| {
        item.is_visible()
            && item.is_accessible()
            && item
                .accessible_string_property(AccessibleStringProperty::Label)
                .is_some_and(|text| text == label)
    })
    .unwrap_or_else(|| panic!("visible accessible action {label:?}"))
}

#[test]
fn toolbar_controls_share_one_height_and_centre_line() {
    use slint::private_unstable_api::re_exports::AccessibleRole;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window().set_size(slint::LogicalSize::new(1280.0, 900.0));
    for text_size in [13.0, 26.0] {
        ui.set_ui_size(text_size);
        for tab in [0, 1] {
            ui.invoke_tab_changed(tab);
            h.settle();
            let bounds = |item: &ItemRc| {
                let geometry = item.geometry();
                let origin = item.map_to_window(geometry.origin);
                (origin.y, geometry.size.height)
            };
            let (tab_y, tab_height) = bounds(&labelled(ui, "Library"));
            let window = WindowInner::from_pub(ui.window());
            let mut row = Vec::new();
            let _ = window.window_item_rc().unwrap().visit_descendants(|item| {
                if item.is_visible()
                    && item.is_accessible()
                    && matches!(
                        item.accessible_role(),
                        AccessibleRole::Button | AccessibleRole::Tab
                    )
                {
                    let (y, height) = bounds(item);
                    if y < tab_y + tab_height && y + height > tab_y {
                        let label = item
                            .accessible_string_property(AccessibleStringProperty::Label)
                            .unwrap_or_default();
                        row.push((label, y, height));
                    }
                }
                ControlFlow::<()>::Continue(())
            });
            assert!(row.len() >= 3, "tabs and page actions share the row: {row:?}");
            for (label, y, height) in &row {
                assert!(
                    (height - tab_height).abs() < 0.5 && (y - tab_y).abs() < 0.5,
                    "{label:?} at y={y} h={height}; tabs at y={tab_y} h={tab_height} \
                     (text {text_size}, tab {tab})"
                );
            }
        }
    }
}

fn assert_library_play_during_file(save: bool) {
    let (h, dir) = controlled_harness();
    let ui = h.ui();
    let path = dir.path().join("sound.toml");
    let file = if save {
        h.workbench();
        ui.invoke_save_recipe();
        assert!(h.take().is_empty(), "managed Save opens no external panel");
        let file = take_job(&h, JobKind::File);
        ui.invoke_tab_changed(0);
        h.settle();
        file
    } else {
        std::fs::write(
            &path,
            crate::test_recipes::silent(0.01, 0.0).to_toml().unwrap(),
        )
        .unwrap();
        ui.invoke_open_recipe();
        assert!(
            !ui.get_can_play(),
            "a native path prompt blocks new playback"
        );
        ui.invoke_play_recipe();
        assert!(!h.adapter.inspect(|w| w.audio_busy()));
        answer_path(&h, &path);
        take_job(&h, JobKind::File)
    };
    assert!(h.adapter.inspect(|w| w.file_busy()));
    assert!(ui.get_can_play(), "Save/Import can coexist with audio");
    assert_eq!(ui.get_library_play_action(), "Play selected sound");
    ui.invoke_play_recipe();
    assert!(
        h.adapter.inspect(|w| w.audio_busy()),
        "the retained Library command dispatches Play"
    );
    let audio = take_job(&h, JobKind::Audio);
    assert_eq!(ui.get_library_play_action(), "Stop current & play recipe");
    finish_stopped_audio(&h, audio);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    finish_file(&h, file);
    h.wait_for_files();
}

#[test]
fn library_play_command_obeys_export_prompt_pending_and_capability_refusals() {
    let (h, dir) = controlled_harness();
    let ui = h.ui();
    ui.invoke_export_wav();
    assert!(!ui.get_can_play());
    ui.invoke_play_recipe();
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    answer_path(&h, &dir.path().join("sound.wav"));
    let export = take_job(&h, JobKind::File);
    assert!(!ui.get_can_play());
    ui.invoke_play_recipe();
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    finish_file(&h, export);
    tick_until(&h, || !h.adapter.inspect(|w| w.file_busy()));
    ui.invoke_play_recipe();
    let audio = take_job(&h, JobKind::Audio);
    ui.invoke_play_recipe();
    assert!(h.adapter.inspect(|w| w.action_pending()));
    assert!(!ui.get_can_play());
    ui.invoke_play_recipe();
    finish_stopped_audio(&h, audio);
    tick_until(&h, || !h.adapter.inspect(|w| w.action_pending()));
    let replacement = take_job(&h, JobKind::Audio);
    finish_stopped_audio(&h, replacement);
    tick_until(&h, || !h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn library_play_command_obeys_unavailable_playback_capability() {
    let h = Harness::with_catalog(
        vec![
            auditorylab_app::document::editing::Draft::new()
                .unwrap()
                .snapshot()
                .unwrap(),
        ],
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
    h.ui().invoke_play_recipe();
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn notes_return_stages_newlines_and_save_preserves_exact_multiline_text() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
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
    assert_eq!(h.take(), [] as [Request; 0]);
    h.wait_for_files();
    let entry = h
        .store()
        .scan(&auditorylab_app::job::Cancel::default())
        .entries
        .remove(0);
    let saved = h.store().load(&entry.key).unwrap().snapshot;
    assert_eq!(
        auditorylab_presentation::recipe::snapshot_about(&(saved))
            .unwrap()
            .summary
            .as_deref(),
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
        auditorylab_app::services::catalog().unwrap(),
        Capabilities::no_device_playback(),
    );
    let adapter = SlintAdapter::install(
        &windows,
        workspace,
        Box::new(Recorder(Rc::new(RefCell::new(Vec::new())))),
        Box::new(crate::adapter::tests::DeterministicExecutor::default()),
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
                .map(|row| auditorylab_presentation::library::row(row, w.library_view().sort))
                .filter(|row| !row.key.is_empty())
                .count(),
            1
        );
        assert_eq!(
            w.selected().unwrap().origin(),
            &auditorylab_app::document::recipe::Origin::BuiltIn("ioannou-2015-binaural-2-hz")
        );
    });
    ui.invoke_rhythm_filter(5, 6); // A steady carrier cannot borrow Gamma.
    assert!(!ui.get_has_results());
    ui.invoke_rhythm_filter(0, 0);
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 1); // PulseTrain.
    let binding = h.component_key(0);
    ui.invoke_commit_parameter(
        parameter_field(&binding.clone(), ParameterId::SourcePulseRate),
        "2".into(),
    );
    assert!(ui.get_draft_brief().contains("Applied rhythm · Delta"));
    ui.invoke_stage_parameter(
        parameter_field(&binding.clone(), ParameterId::SourcePulseRate),
        "bad".into(),
    );
    assert!(ui.get_draft_brief().contains("Applied rhythm · Delta"));
    ui.invoke_commit_parameter(
        parameter_field(&binding.clone(), ParameterId::SourcePulseRate),
        "6".into(),
    );
    assert!(ui.get_draft_brief().contains("Applied rhythm · Theta"));
    assert!(ui.get_draft_brief().contains("Saved version: Not saved"));
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn catalog_callback_keeps_snapshot_identity_after_filter_reorders_rows() {
    let h = Harness::new();
    let ui = h.ui();
    let stale = ui.get_catalog().row_data(0).unwrap().entry;
    ui.invoke_rhythm_filter(1, 1);
    let selected = h.adapter.inspect(|w| w.selected().unwrap().id());
    ui.invoke_select(stale.clone());
    assert_eq!(h.adapter.inspect(|w| w.selected().unwrap().id()), selected);
    ui.invoke_rhythm_filter(0, 0);
    let current = ui.get_library_entry();
    ui.invoke_select(stale.clone());
    assert_eq!(ui.get_library_entry(), current);
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
            ParameterId::PairFirstFrequency,
            ParameterId::PairOffset,
            ParameterId::PairFirstLevel,
            ParameterId::PairSecondLevel
        ]
    );
    let binding = h.component_key(0);
    ui.invoke_commit_parameter(
        parameter_field(&binding.clone(), ParameterId::PairOffset),
        "-6".into(),
    );
    assert!(
        ui.get_components()
            .row_data(0)
            .unwrap()
            .context
            .contains("194 Hz")
    );
    assert!(ui.get_draft_brief().contains("Theta"));
    ui.invoke_commit_parameter(
        parameter_field(&binding.clone(), ParameterId::PairFirstFrequency),
        "24000".into(),
    );
    let rows = ui.get_components().row_data(0).unwrap().parameters;
    for key in [ParameterId::PairFirstFrequency, ParameterId::PairOffset] {
        assert!(
            rows.iter()
                .find(|p| field_parameter(&p.key) == key)
                .is_some_and(|p| !p.diagnostic.is_empty())
        );
    }
    assert!(!ui.get_can_export());
    assert!(
        ui.get_can_save(),
        "finite rate-invalid pairs remain serializable for repair"
    );
    ui.invoke_commit_parameter(
        parameter_field(&binding.clone(), ParameterId::PairFirstFrequency),
        "200.00000000000003".into(),
    );
    let value = h.adapter.inspect(|w| {
        w.editor().draft().components()[0]
            .form
            .get(ParameterId::PairFirstFrequency)
            .unwrap()
    });
    assert_eq!(
        value.get().to_bits(),
        200.000_000_000_000_03_f64.to_bits(),
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
    let mut recipe = auditorylab_app::document::editing::Draft::new()
        .unwrap()
        .recipe()
        .unwrap()
        .into_builder();
    recipe.content.sound = {
        let fixture = auditorylab_app::core::test_support::FiniteFixture::new(
            recipe.content.timing.duration_seconds,
            recipe.content.timing.fade_seconds,
            vec![
                auditorylab_app::core::Component {
                    id: auditorylab_app::core::ComponentId::new(u32::MAX).unwrap(),
                    form: auditorylab_app::core::ComponentForm {
                        source: auditorylab_app::core::SoundSource::BinauralPair {
                            first_hz: finite(200.0),
                            offset_hz: finite(2.0),
                            first_peak_dbfs: finite(-30.0),
                            second_peak_dbfs: finite(-30.0),
                        },
                        modulation: None,
                    },
                },
                auditorylab_app::core::Component {
                    id: auditorylab_app::core::ComponentId::new(u32::MAX - 1).unwrap(),
                    form: auditorylab_app::workspace::controls::default_component(
                        auditorylab_app::core::CarrierKind::PulseTrain,
                    ),
                },
            ],
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
        recipe.content.timing = fixture.timing;
        fixture.sound
    };
    let snapshot = auditorylab_app::document::recipe::RecipeSnapshot::new(
        auditorylab_app::document::recipe::Origin::Stream("large owners".into()),
        recipe.build().unwrap(),
    );
    let h = Harness::with_catalog(vec![snapshot], Capabilities::no_device_playback());
    h.ui().invoke_collection_filter(3, 0);
    h.wait_for_files();
    h.adapter
        .command(|w| w.apply(Command::Editor(EditorCommand::CopySelected)));
    h.workbench();
    let rows = h.ui().get_components();
    assert_eq!(rows.row_data(0).unwrap().id, u32::MAX.to_string());
    assert_eq!(rows.row_data(1).unwrap().id, (u32::MAX - 1).to_string());
    let first = h.component_key(0);
    let second = h.component_key(1);
    assert_ne!(first, second);
    h.ui().invoke_commit_parameter(
        parameter_field(&first.clone(), ParameterId::PairOffset),
        "6".into(),
    );
    assert_eq!(
        h.adapter.inspect(|w| w.editor().draft().components()[1]
            .form
            .get(ParameterId::SourcePulseRate)),
        Some(finite(40.0))
    );
}

#[test]
fn persistent_library_slint_bindings_publish_exact_draft_and_keep_filter_identity() {
    let temp = tempfile::tempdir().unwrap();
    let store =
        auditorylab_app::services::library::Library::at_path(temp.path().join("library")).unwrap();
    store.initialize().unwrap();
    let workspace = Workspace::with_library(
        auditorylab_app::services::catalog().unwrap(),
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
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
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
        .scan(&auditorylab_app::job::Cancel::default())
        .entries
        .remove(0);
    assert_eq!(first.title, "Native managed copy");
    assert!(first.created_at.is_some());
    ui.invoke_commit_parameter(
        parameter_field(&binding.clone(), ParameterId::PairOffset),
        "6".into(),
    );
    assert!(ui.get_draft_brief().contains("Applied rhythm · Theta"));
    assert!(ui.get_draft_brief().contains("Saved version: Delta"));
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
    assert_managed_trash_controls(&h, &store);
}

#[test]
fn persistent_native_filters_use_shared_query_and_invalid_dates_preserve_it() {
    let temp = tempfile::tempdir().unwrap();
    let store =
        auditorylab_app::services::library::Library::at_path(temp.path().join("library")).unwrap();
    store.initialize().unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab_app::services::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store,
        )
        .unwrap(),
    );
    h.wait_for_files();
    let ui = h.ui();
    ui.invoke_collection_filter(1, sort_choice(ui, SortBy::Title));
    ui.invoke_more_filters(
        "".into(),
        0,
        0,
        0,
        0,
        1,
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
    );
    let expected = h.adapter.inspect(|w| {
        w.catalog_query()
            .clone()
            .execute(
                &w.catalog()
                    .iter()
                    .map(|s| {
                        auditorylab_app::discovery::collection::EntrySummary::from_snapshot(
                            auditorylab_app::document::origin::LibraryKey::BuiltIn(
                                match s.origin() {
                                    auditorylab_app::document::recipe::Origin::BuiltIn(id) => {
                                        auditorylab_app::core::catalog::CatalogId::new(id).unwrap()
                                    }
                                    _ => unreachable!(),
                                },
                            ),
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
    assert_invalid_native_dates(&h);
}

slint::slint! {
    import { BoundButton } from "ui/library-controls.slint";
    export { EntryAddress, LibraryMutationAction } from "ui/library-actions.slint";
    import { EntryAddress, LibraryMutationAction } from "ui/library-actions.slint";
    export component LibraryGestureProbe inherits Window {
        width:240px;height:100px;
        in-out property <EntryAddress> entry;
        in-out property <LibraryMutationAction> operation;
        callback delivered(EntryAddress,LibraryMutationAction);
        BoundButton {x:10px;y:10px;width:220px;height:50px;text:"Bound library action";
            entry:root.entry;action:root.operation;dispatch(e,a)=>{root.delivered(e,a);}}
    }
}
#[test]
fn native_library_pointer_gesture_keeps_handle_and_action_through_rebind() {
    install();
    let probe = LibraryGestureProbe::new().unwrap();
    let first = EntryAddress {
        table: slint::ModelRc::new(slint::VecModel::from(vec![1])),
        row: 0,
    };
    probe.set_entry(first.clone());
    probe.set_operation(LibraryMutationAction::Trash);
    let received = Rc::new(RefCell::new(None));
    let result = received.clone();
    probe.on_delivered(move |entry, action| {
        *result.borrow_mut() = Some((entry, action));
    });
    probe.show().unwrap();
    settle(probe.window());
    let position = slint::LogicalPosition::new(50.0, 30.0);
    probe.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    probe.set_entry(EntryAddress {
        table: slint::ModelRc::new(slint::VecModel::from(vec![2])),
        row: 0,
    });
    probe.set_operation(LibraryMutationAction::Restore);
    settle(probe.window());
    probe.window().dispatch_event(WindowEvent::PointerReleased {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    assert_eq!(
        *received.borrow(),
        Some((first, LibraryMutationAction::Trash))
    );
}
#[test]
fn persistent_native_transport_stays_inside_800_by_600_with_large_text() {
    let temp = tempfile::tempdir().unwrap();
    let root =
        auditorylab_app::services::library::Library::at_path(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = root;
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab_app::services::catalog().unwrap(),
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
            h.ui().invoke_tab_changed(tab);
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
    let root =
        auditorylab_app::services::library::Library::at_path(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = root;
    let file = temp.path().join("external.toml");
    let mut draft = auditorylab_app::document::editing::Draft::new().unwrap();
    draft.set_title("External original".into());
    let original = draft.recipe().unwrap();
    std::fs::write(&file, original.to_toml().unwrap()).unwrap();
    let initial_bytes = std::fs::read(&file).unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab_app::services::catalog().unwrap(),
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
    h.adapter.reverse_rendered_choices();
    h.settle();
    assert_ne!(ui.get_collection_choice(), 3);
    assert!(ui.get_open_files_scope());
    for label in [
        "Close open file",
        "Edit external file",
        "Add to User Library",
    ] {
        assert!(
            find_control(ui, |item| item
                .accessible_string_property(AccessibleStringProperty::Label)
                .as_deref()
                == Some(label))
            .is_some(),
            "Open-files action {label} survives scope reordering"
        );
    }
    assert!(
        store
            .scan(&auditorylab_app::job::Cancel::default())
            .entries
            .is_empty()
    );
    let external_entry = ui.get_library_entry();
    ui.invoke_add_to_library(external_entry);
    h.wait_for_files();
    assert_eq!(
        store
            .scan(&auditorylab_app::job::Cancel::default())
            .entries
            .len(),
        1
    );
    assert_eq!(std::fs::read(&file).unwrap(), initial_bytes);
    ui.invoke_edit_saved(ui.get_library_entry());
    h.settle();
    assert!(ui.get_draft_location().starts_with("External file"));
    assert_eq!(
        h.adapter
            .inspect(|w| w.editor().draft().recipe().unwrap().document().id),
        original.document().id
    );
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "External edited".into(),
    );
    ui.invoke_save_recipe();
    h.wait_for_files();
    assert_eq!(
        auditorylab_app::services::open_file(&file).unwrap().title(),
        "External edited"
    );
    assert_eq!(
        store.scan(&auditorylab_app::job::Cancel::default()).entries[0].title,
        "External original"
    );
    assert!(!ui.get_dirty());
    assert_external_conflict_and_copy(&h, &file, &original, &store);
}

#[test]
fn native_readonly_labels_escape_controls_and_bidi_without_rewriting_editable_content() {
    let temp = tempfile::tempdir().unwrap();
    let root =
        auditorylab_app::services::library::Library::at_path(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = root;
    let raw = "Label \u{202e}User\u{001b}";
    let mut draft = auditorylab_app::document::editing::Draft::new().unwrap();
    draft.set_title(raw.into());
    let record = store
        .apply(
            auditorylab_app::services::library::Mutation::Add(std::sync::Arc::new(
                draft.recipe().unwrap(),
            )),
            &auditorylab_app::job::Cancel::default(),
        )
        .unwrap()
        .record
        .unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab_app::services::catalog().unwrap(),
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
    ui.invoke_edit_saved(ui.get_library_entry());
    h.settle();
    assert_eq!(ui.get_title_field().text, raw);
    assert_eq!(
        store
            .load(&record.key)
            .unwrap()
            .snapshot
            .recipe()
            .description()
            .unwrap()
            .title,
        raw
    );
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn native_library_keyboard_has_one_tab_stop_and_keeps_selected_key_through_sort() {
    let temp = tempfile::tempdir().unwrap();
    let root =
        auditorylab_app::services::library::Library::at_path(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = root;
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab_app::services::catalog().unwrap(),
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
    ui.invoke_collection_filter(1, sort_choice(ui, SortBy::Title));
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
    let root =
        auditorylab_app::services::library::Library::at_path(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = root;
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab_app::services::catalog().unwrap(),
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
        let button = action_button(ui, "Filters");
        let position = centre(&button);
        assert!(position.x >= 0.0 && position.x < 800.0 && position.y < 900.0);
        drag(ui, position, position);
        h.settle();
        assert!(!ui.get_library_detail());
        assert!(
            ui.get_library_tools_open(),
            "entering tools must never hide them"
        );
    };
    open_tools();

    ui.set_study_query("missing-study".into());
    ui.set_analysis_choice(2);
    ui.set_created_from("2026-01-01".into());
    reveal_library_control(&h, "Apply filters").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert_eq!(ui.get_catalog().row_count(), 0);
    let committed = ui.get_library_count();
    assert!(
        committed.contains("Study: missing-study")
            && committed.contains("Analysis: Invalid")
            && committed.contains("Created from inclusive:")
    );
    assert!(
        !ui.get_library_tools_open(),
        "successful Apply closes the popup"
    );
    open_tools();
    ui.set_study_query("UNSUBMITTED".into());
    ui.set_created_from("invalid".into());
    reveal_library_control(&h, "Apply filters").accessible_action(&AccessibilityAction::Default);
    assert_eq!(
        ui.get_library_count(),
        committed,
        "invalid staged fields cannot replace applied summary"
    );
    assert!(
        ui.get_library_tools_open(),
        "invalid filters stay open for correction"
    );
    ui.invoke_close_library_filters();
    h.settle();
    assert!(!ui.get_library_detail());
    assert!(labelled(ui, "Applied library filters").is_visible());
    // Keyboard activation reaches the same production button with no result row.
    let tools = action_button(ui, "Filters");
    WindowInner::from_pub(ui.window()).set_focus_item(&tools, true, FocusReason::Programmatic);
    h.key(Key::Return);
    assert!(ui.get_library_tools_open());
    ui.invoke_close_library_filters();
    h.settle();
    let clear = centre(&action_button(ui, "Clear all filters"));
    drag(ui, clear, clear);
    h.settle();
    assert_eq!(ui.get_catalog().row_count(), 23);
    assert!(ui.get_library_count().contains("Applied filters: None"));
    assert_empty_scope_recovery(&h);
}

#[test]
fn compact_unavailable_root_keeps_recovery_and_non_authoritative_counts_visible() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("library");
    let root = auditorylab_app::services::library::Library::at_path(path.clone()).unwrap();
    root.initialize().unwrap();
    let store = root;
    std::fs::rename(&path, temp.path().join("offline")).unwrap();
    std::fs::write(&path, "not a directory").unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab_app::services::catalog().unwrap(),
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
    ui.set_query("missing".into());
    ui.invoke_search("missing".into());
    h.settle();
    assert!(ui.get_library_filters_active());
    assert_unavailable_root_sizes(&h);
    ui.set_ui_size(13.0);
    ui.window().set_size(slint::LogicalSize::new(800.0, 900.0));
    h.settle();

    assert!(ui.get_library_count().contains("Inventory incomplete"));
    assert!(ui.get_library_alert().contains("incomplete"));
    assert!(
        ui.get_library_empty_message()
            .contains("Incomplete inventory")
    );
    action_button(ui, "Library options…").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert!(action_button(ui, "Refresh").is_visible());
    assert!(action_button(ui, "Clear all filters").is_visible());
    labelled(ui, "Library storage and recovery").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert!(reveal_library_control(&h, "Initialize configured library").is_visible());
}

#[test]
fn compact_back_keeps_result_scroll_and_large_text_rows_wrap_with_summary_dates() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let temp = tempfile::tempdir().unwrap();
    let store =
        auditorylab_app::services::library::Library::at_path(temp.path().join("library")).unwrap();
    store.initialize().unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab_app::services::catalog().unwrap(),
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
    action_button(ui, "Filters").accessible_action(&AccessibilityAction::Default);
    h.settle();
    ui.invoke_close_library_filters();
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
    action_button(ui, "Filters").accessible_action(&AccessibilityAction::Default);
    h.settle();
    ui.invoke_more_filters(
        "no-such-study".into(),
        0,
        0,
        0,
        0,
        0,
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
    );
    h.settle();
    ui.invoke_close_library_filters();
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
    assert_large_text_catalog_row(&h);
}

#[test]
fn local_dates_expose_exact_utc_and_reset_sort_preserves_committed_filters() {
    use auditorylab_app::services::library::Mutation;
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let temp = tempfile::tempdir().unwrap();
    let store =
        auditorylab_app::services::library::Library::at_path(temp.path().join("library")).unwrap();
    store.initialize().unwrap();
    let draft = auditorylab_app::document::editing::Draft::from_snapshot(
        &auditorylab_app::services::builtin("ioannou-2015-binaural-2-hz").unwrap(),
    )
    .unwrap();
    let mut recipe = draft.recipe().unwrap().into_builder();
    let created = "2026-10-25T00:30:00.123456789Z".parse().unwrap();
    recipe.document.created_at = Some(created);
    recipe.document.modified_at = None;
    let id = recipe.document.id;
    store
        .apply(
            Mutation::Add(std::sync::Arc::new(recipe.build().unwrap())),
            &auditorylab_app::job::Cancel::default(),
        )
        .unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab_app::services::catalog().unwrap(),
            Capabilities::no_device_playback(),
            store,
        )
        .unwrap(),
    );
    h.wait_for_files();
    let ui = h.ui();
    ui.invoke_collection_filter(2, sort_choice(ui, SortBy::Created));
    h.wait_for_files();
    let row = ui.get_catalog().row_data(0).unwrap().text;
    assert!(
        row.contains("Edited: Unknown") && row.contains("Added here:") && row.contains("Created:")
    );
    assert!(
        ui.get_library_dates()
            .contains(&auditorylab_presentation::library::calendar::local_timestamp(created))
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
    action_button(ui, "Library options…").accessible_action(&AccessibilityAction::Default);
    h.settle();
    action_button(ui, "Reset sort").accessible_action(&AccessibilityAction::Default);
    h.settle();
    h.key(Key::Escape);
    h.adapter.inspect(|w| {
        let q = w.catalog_query();
        assert_eq!(q.scope, auditorylab_app::discovery::query::Scope::User);
        assert_eq!(q.sort, auditorylab_app::discovery::query::SortBy::Automatic);
        assert!(q.direction.is_none());
        assert_eq!(q.search, "2");
    });
    reveal_library_control(&h, "Exact UTC recipe and membership dates");
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
    let root =
        auditorylab_app::services::library::Library::at_path(temp.path().join("library")).unwrap();
    root.initialize().unwrap();
    let store = root;
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab_app::services::catalog().unwrap(),
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
        .global::<crate::generated::ViewCaptions>()
        .set_structural_method("Different display wording".into());
    h.ui()
        .global::<crate::generated::ViewCaptions>()
        .set_library("Different collection wording".into());
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
    use auditorylab_app::core::rhythm::{Band, Method};
    use auditorylab_app::discovery::collection::CatalogQuery;
    use auditorylab_app::discovery::query::RhythmQuery;

    let temp = tempfile::tempdir().unwrap();
    let store =
        auditorylab_app::services::library::Library::at_path(temp.path().join("library")).unwrap();
    store.initialize().unwrap();
    let h = Harness::with_workspace(
        Workspace::with_library(
            auditorylab_app::services::catalog().unwrap(),
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
        for sort in SortBy::ALL {
            ui.invoke_collection_filter(choice, sort_choice(ui, sort));
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
    for (index, method) in [
        Method::Binaural,
        Method::Monaural,
        Method::PulseTrain,
        Method::Pulsed,
        Method::Sinusoidal,
        Method::SteadyTone,
    ]
    .iter()
    .enumerate()
    {
        let label = ui.get_method_labels().row_data(index + 1).unwrap();
        assert!(label.starts_with(method.label()), "{method:?}: {label}");
    }
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
            (3, Method::PulseTrain),
            (4, Method::Pulsed),
            (5, Method::Sinusoidal),
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
    assert_evidence_date_filters(&h);
}

#[test]
fn real_save_capture_and_delayed_completion_preserve_newer_edits_and_close_prompting() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "Captured by Save".into(),
    );
    ui.invoke_save_recipe();
    let save = take_job(&h, JobKind::File);
    assert_eq!(h.take(), [] as [Request; 0]);
    let document = ui.get_document();
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "Newer unsaved edits".into(),
    );
    // Execute the real capture while withholding its Finished response. Physical
    // thread joining is the Runner's app-unit contract, not simulated by this test.
    let finished = save.execute(|_| panic!("a save emits no preparation"));
    let saved = h
        .store()
        .scan(&auditorylab_app::job::Cancel::default())
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
    use auditorylab_app::services::library::Mutation;

    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "Initial saved title".into(),
    );
    ui.invoke_save_recipe();
    h.wait_for_files();
    let saved = h
        .store()
        .scan(&auditorylab_app::job::Cancel::default())
        .entries
        .remove(0);
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
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
            &auditorylab_app::job::Cancel::default(),
        )
        .unwrap();
    h.adapter
        .command(|w| w.apply(Command::Library(LibraryCommand::Refresh)));
    finish_file(&h, save);
    h.wait_for_files();
    assert!(ui.get_dirty());
    assert_eq!(ui.get_title_field().text, "Retained draft");
    assert_eq!(
        h.store().load(&saved.key).unwrap().snapshot.title(),
        "Other writer"
    );
    let details = h.adapter.inspect(|w| {
        auditorylab_presentation::workspace::action_view(&w.action_view()).operation_details
    });
    assert!(
        details.to_lowercase().contains("conflict") || details.to_lowercase().contains("changed"),
        "foreground save failure remains visible after background reads: {details}"
    );
}

#[test]
fn inclusive_date_errors_name_each_visible_pair_and_keep_inputs_and_query() {
    let h = Harness::new();
    for (field, label) in [(0, "Created"), (1, "Modified"), (2, "Added here")] {
        assert_inclusive_date_error(&h, field, label);
    }
}

#[test]
fn wide_library_navigation_uses_real_shared_filters_and_structured_cards() {
    use auditorylab_app::core::rhythm::Band;
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window()
        .set_size(slint::LogicalSize::new(1440.0, 1000.0));
    h.settle();
    assert!(ui.get_library_wide());
    assert_eq!(ui.get_band_navigation().row_count(), 10);
    for (index, band) in [
        Band::Delta,
        Band::Theta,
        Band::Alpha,
        Band::Beta,
        Band::Gamma,
    ]
    .iter()
    .enumerate()
    {
        let facet = ui.get_band_navigation().row_data(index + 1).unwrap();
        assert_eq!(facet.label, band.to_string());
        assert_eq!(facet.detail, band.interval());
        let item = labelled(ui, &facet.text);
        assert!(
            item.is_visible(),
            "band navigation is not a closed dropdown"
        );
        assert!(item.geometry().size.height > 0.0);
    }
    let delta = ui.get_band_navigation().row_data(1).unwrap();
    labelled(ui, &delta.text).accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert_eq!(
        h.adapter
            .inspect(|w| w.catalog_query().rhythm.bands.clone()),
        vec![Band::Delta]
    );
    assert_eq!(ui.get_band_choice(), 1);
    assert_eq!(ui.get_catalog().row_count(), 1);
    let row = ui.get_catalog().row_data(0).unwrap();
    assert!(!row.title.is_empty());
    assert!(row.collection.contains("Read only"));
    assert_eq!(row.bands, "Delta");
    assert!(row.duration.contains("1 min"));
    assert!(row.dates.contains("Unknown"));
    assert!(
        ui.get_library_classification()
            .contains("not measured brain activity")
    );
    assert!(ui.get_library_ownership().contains("Read only"));
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    // The next query originates outside the widget: its selected state must follow.
    ui.invoke_rhythm_filter(3, 0);
    h.settle();
    assert_eq!(ui.get_band_choice(), 3);
    assert!(
        h.adapter
            .inspect(|w| w.catalog_query().rhythm.bands == [Band::Alpha])
    );
}

#[test]
fn wide_library_group_headings_have_no_selectable_identity() {
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window().set_size(slint::LogicalSize::new(1440.0, 900.0));
    ui.invoke_more_filters(
        "".into(),
        0,
        0,
        0,
        0,
        1,
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
    );
    h.settle();
    let rows = ui.get_catalog();
    assert!(rows.iter().any(|row| row.binding.is_empty()));
    assert!(
        rows.iter()
            .filter(|row| row.binding.is_empty())
            .all(|row| row.collection.is_empty() && row.duration.is_empty())
    );
    let before = ui.get_library_key();
    ui.invoke_select(crate::generated::EntryAddress::default());
    assert_eq!(ui.get_library_key(), before);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

// Exercise presentation geometry without claiming a running audio device or worker.
fn present_active_status_for_layout(ui: &MainWindow) {
    ui.set_playback_busy(true);
    ui.set_player_title("Playing · captured sound".into());
    let stop = auditorylab_presentation::workspace::operation_label(
        auditorylab_app::vocabulary::Operation::Stop,
    );
    ui.set_transport_action(stop.into());
    ui.set_transport_enabled(true);
    ui.set_transport_labels(Rc::new(slint::VecModel::from(vec![stop.into()])).into());
    ui.global::<crate::generated::Actions>()
        .set_stop(crate::generated::Action {
            label: stop.into(),
            enabled: true,
        });
    ui.set_operation_summary("Exporting a captured sound".into());
    ui.set_status("Background task status".into());
    ui.set_exporting(true);
    ui.global::<crate::generated::Actions>()
        .set_cancel_export(crate::generated::Action {
            label: "Cancel export".into(),
            enabled: true,
        });
}

#[test]
#[ignore = "explicit silent desktop visual evidence, not a device test"]
fn desktop_review_screenshots() {
    let h = Harness::new();
    let destination = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/evidence/desktop-review");
    std::fs::create_dir_all(&destination).unwrap();
    render_library_review(&h, &destination);
    render_learning_review(&h, &destination);
    render_workbench_review(&h, &destination);
    assert!(!h.adapter.inspect(|w| w.audio_busy() || w.file_busy()));
}

fn save_desktop_review_frame(ui: &MainWindow, destination: &std::path::Path, name: &str) {
    use std::io::Write;
    // Explicit renderer inputs must stay coherent after settling real
    // telemetry; this text does not claim hardware playback is occurring.
    let layout_audio_active = ui.get_playback_busy();
    if layout_audio_active {
        ui.set_player_title("Audio task active (layout fixture)".into());
        assert_eq!(ui.get_player_title(), "Audio task active (layout fixture)");
    }
    let pixels = ui.window().take_snapshot().unwrap();
    if layout_audio_active {
        assert!(ui.get_playback_busy());
        assert_eq!(ui.get_player_title(), "Audio task active (layout fixture)");
    }
    let file = std::fs::File::create(destination.join(format!("{name}.ppm"))).unwrap();
    let mut file = std::io::BufWriter::new(file);
    write!(file, "P6\n{} {}\n255\n", pixels.width(), pixels.height()).unwrap();
    for pixel in pixels.as_slice() {
        file.write_all(&[pixel.r, pixel.g, pixel.b]).unwrap();
    }
    file.flush().unwrap();
}

fn render_library_review(h: &Harness, destination: &std::path::Path) {
    let ui = h.ui();
    ui.set_monitor_open(false);
    for (name, width, height) in [
        ("library-startup", 1280_u32, 900_u32),
        ("library-with-monitor", 1280_u32, 900_u32),
        ("library-wide", 1440_u32, 1000_u32),
        ("library-medium", 1024, 768),
        ("library-compact", 800, 600),
    ] {
        ui.set_monitor_open(name == "library-with-monitor");
        ui.window()
            .set_size(slint::PhysicalSize::new(width, height));
        h.settle();
        save_desktop_review_frame(ui, destination, name);
    }
}

fn render_workbench_review(h: &Harness, destination: &std::path::Path) {
    use slint::private_unstable_api::re_exports::ColorScheme;
    let ui = h.ui();
    ui.set_monitor_open(false);
    WindowInner::from_pub(ui.window())
        .context()
        .set_color_scheme(ColorScheme::Light);
    ui.set_ui_size(13.0);
    ui.window().set_size(slint::LogicalSize::new(1280.0, 900.0));
    h.workbench();
    save_desktop_review_frame(ui, destination, "workbench-empty");
    ui.invoke_open_learning(3);
    h.settle();
    ui.invoke_create_learning_example();
    h.settle();
    save_desktop_review_frame(ui, destination, "workbench-example");
    ui.set_ui_size(26.0);
    ui.window().set_size(slint::LogicalSize::new(800.0, 600.0));
    h.settle();
    save_desktop_review_frame(ui, destination, "workbench-example-large");
    ui.set_ui_size(13.0);
    ui.invoke_tab_changed(0);
    h.settle();
}

fn render_learning_review(h: &Harness, destination: &std::path::Path) {
    use auditorylab_presentation::Explain;
    use slint::private_unstable_api::re_exports::ColorScheme;
    let ui = h.ui();
    ui.set_monitor_open(false);
    let window = WindowInner::from_pub(ui.window());
    let notice = auditorylab_app::onboarding::Problem::Unsupported {
        path: std::path::PathBuf::from(
            "/Users/example/Library/Application Support/org.resoltico.auditorylab/onboarding-seen",
        ),
    }
    .explain();
    for (appearance, scheme) in [("light", ColorScheme::Light), ("dark", ColorScheme::Dark)] {
        window.context().set_color_scheme(scheme);
        for (size, width, height, text) in [
            ("regular", 1280.0, 900.0, 13.0),
            ("large", 680.0, 600.0, 26.0),
        ] {
            ui.window().set_size(slint::LogicalSize::new(width, height));
            ui.set_ui_size(text);
            for (name, topic) in [
                ("overview", 0),
                ("spectrum", 4),
                ("pitch", 2),
                ("sound-types", 3),
            ] {
                ui.invoke_open_learning(topic);
                h.settle();
                save_desktop_review_frame(
                    ui,
                    destination,
                    &format!("guide-{name}-{appearance}-{size}"),
                );
            }
            if size == "regular" {
                render_construction_review(h, destination, appearance);
            } else {
                render_active_learning_review(h, destination, appearance);
            }
            ui.set_onboarding_notice(notice.clone().into());
            if size == "large" {
                present_active_status_for_layout(ui);
            }
            ui.invoke_show_onboarding_notice();
            h.settle();
            assert!(ui.get_onboarding_notice_open());
            save_desktop_review_frame(
                ui,
                destination,
                &format!("preference-notice-{appearance}-{size}"),
            );
            h.key(Key::Escape);
            ui.set_onboarding_notice("".into());
            // Real lesson commands reset the explicit renderer-only active state.
            ui.invoke_open_learning(1);
            ui.invoke_open_learning(0);
            h.settle();
        }
        ui.window().set_size(slint::LogicalSize::new(2000.0, 600.0));
        ui.set_ui_size(26.0);
        ui.invoke_open_learning(3);
        ui.set_onboarding_notice(notice.clone().into());
        present_active_status_for_layout(ui);
        h.settle();
        assert!(
            ui.get_learning_compact(),
            "short viewport keeps all guide topics available"
        );
        save_desktop_review_frame(
            ui,
            destination,
            &format!("guide-active-wide-short-{appearance}"),
        );
        ui.invoke_show_onboarding_notice();
        h.settle();
        save_desktop_review_frame(
            ui,
            destination,
            &format!("preference-notice-active-wide-short-{appearance}"),
        );
        h.key(Key::Escape);
        ui.set_onboarding_notice("".into());
        ui.invoke_open_learning(1);
        ui.invoke_open_learning(0);
        h.settle();
    }
}

fn render_construction_review(h: &Harness, destination: &std::path::Path, appearance: &str) {
    use auditorylab_app::learning::Construction;
    let ui = h.ui();
    for construction in [
        Construction::MonauralPair,
        Construction::BinauralPair,
        Construction::PulseTrain,
    ] {
        ui.invoke_select_learning_construction(i32::try_from(construction.index()).unwrap());
        h.settle();
        save_desktop_review_frame(
            ui,
            destination,
            &format!("guide-{}-{appearance}-regular", construction.key()),
        );
    }
    ui.invoke_select_learning_construction(0);
}

fn render_active_learning_review(h: &Harness, destination: &std::path::Path, appearance: &str) {
    let ui = h.ui();
    present_active_status_for_layout(ui);
    h.settle();
    assert!(action_button(ui, "Stop audio").is_visible());
    assert!(action_button(ui, "Cancel export").is_visible());
    save_desktop_review_frame(
        ui,
        destination,
        &format!("guide-active-status-{appearance}-large"),
    );
    ui.set_monitor_open(true);
    h.settle();
    present_active_status_for_layout(ui);
    h.settle();
    assert!(!ui.get_drawer_fits());
    assert!(action_button(ui, "Hide output monitor").is_visible());
    save_desktop_review_frame(
        ui,
        destination,
        &format!("guide-active-monitor-{appearance}-large"),
    );
    ui.set_monitor_open(false);
    ui.invoke_open_learning(0);
}

#[test]
fn applied_filters_remain_readable_when_library_tools_are_closed() {
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    for width in [1440.0, 1100.0, 800.0] {
        ui.window().set_size(slint::LogicalSize::new(width, 900.0));
        ui.invoke_close_library_filters();
        ui.invoke_more_filters(
            "ioannou-2015".into(),
            0,
            0,
            0,
            0,
            0,
            crate::generated::DateInput {
                from_text: "".into(),
                through_text: "".into(),
            },
            crate::generated::DateInput {
                from_text: "".into(),
                through_text: "".into(),
            },
            crate::generated::DateInput {
                from_text: "".into(),
                through_text: "".into(),
            },
        );
        h.settle();
        ui.invoke_open_query_details();
        h.settle();
        let summary = field(ui, "Committed library query");
        assert!(
            summary.is_visible(),
            "committed query remains readable at width {width}"
        );
        assert!(text_of(&summary).contains("Study: ioannou-2015"));
        assert!(ui.get_library_query_open());
        ui.invoke_close_query_details();
        h.settle();
        assert!(!ui.get_library_tools_open());
    }
}

#[test]
fn startup_exposes_all_five_bands_and_monitor_is_an_explicit_choice() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let (workspace, _directory, _store) = bound_workspace(
        auditorylab_app::services::catalog().unwrap(),
        Capabilities::no_device_playback(),
    );
    let h = Harness::with_startup_workspace(workspace);
    let identity = &auditorylab_app::identity::BUILD_IDENTITY;
    for terms in [
        h.windows.main.global::<crate::generated::Terms>(),
        h.windows.about.global::<crate::generated::Terms>(),
    ] {
        assert_eq!(terms.get_caption_product_name().as_str(), identity.name);
        assert_eq!(terms.get_caption_copyright().as_str(), identity.copyright);
        assert_eq!(
            terms.get_caption_about_product().as_str(),
            auditorylab_presentation::build_info::about_title()
        );
    }

    let ui = h.ui();
    assert!(
        !ui.get_monitor_open(),
        "startup must leave room for discovery"
    );
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
        assert!(
            item_origin.y + item_bounds.size.height <= origin.y + bounds.size.height,
            "{} must be visible without scrolling at the initial size",
            facet.label
        );
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

#[test]
fn library_ux_preview_reveals_monitor_and_quiet_rows_preserve_identity() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window().set_size(slint::LogicalSize::new(1280.0, 900.0));
    h.settle();
    let selected = ui.get_library_key();
    let count = labelled(ui, "Applied library filters");
    let count_bounds = count.geometry();
    let count_y = count.map_to_window(count_bounds.origin).y;
    let results = labelled(ui, "Library results");
    let results_y = results.map_to_window(results.geometry().origin).y;
    assert!(
        count_y + count_bounds.size.height <= results_y + 1.0,
        "the result count stays in the fixed header"
    );
    ui.invoke_navigate_library(2);
    h.settle();
    let count = labelled(ui, "Applied library filters");
    assert_eq!(
        count.map_to_window(count.geometry().origin).y,
        count_y,
        "scrolling/selecting the last result does not move the count"
    );
    ui.invoke_select(
        ui.get_catalog()
            .iter()
            .find(|row| row.binding == selected)
            .unwrap()
            .entry,
    );
    h.settle();
    assert!(
        find_control(ui, |item| item
            .accessible_string_property(AccessibleStringProperty::Label)
            .as_deref()
            == Some("Auditory Lab"))
        .is_none(),
        "the window title is the only product heading"
    );
    let row = ui.get_catalog().row_data(0).unwrap();
    assert!(
        row.built_in && row.text.contains("Read only") && row.text.contains("Created: Unknown")
    );
    assert!(
        find_control(ui, |item| item.is_visible()
            && item
                .accessible_string_property(AccessibleStringProperty::Label)
                .as_deref()
                == Some("Created: Unknown"))
        .is_none(),
        "unknown Built-in dates belong in technical details"
    );
    assert!(
        labelled(ui, "Playback transport").geometry().size.height < 80.0,
        "idle transport must not reserve empty outcome rows"
    );
    assert!(
        find_control(ui, |item| item.is_visible()
            && item
                .accessible_string_property(AccessibleStringProperty::Label)
                .as_deref()
                == Some("Listen"))
        .is_none()
    );
    let preview = action_button(ui, "Preview waveform");
    assert!(centre(&preview).y < centre(&labelled(ui, "What was studied")).y);
    preview.accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert!(ui.get_monitor_open());
    assert_eq!(ui.get_library_key(), selected);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn library_ux_empty_collection_is_distinct_from_filtered_or_incomplete_results() {
    let h = Harness::new();
    h.wait_for_files();
    let ui = h.ui();
    for (scope, message) in [
        (2, "Your User Library is empty"),
        (3, "No open files"),
        (4, "Trash is empty"),
    ] {
        ui.invoke_collection_filter(scope, 0);
        h.settle();
        assert!(ui.get_library_empty_message().starts_with(message));
        assert!(!ui.get_library_filters_active());
        assert!(!ui.get_library_empty_message().contains("Clear"));
    }
    ui.invoke_collection_filter(1, 0);
    ui.invoke_search("no-such-sound".into());
    h.settle();
    assert!(ui.get_library_empty_message().contains("No recipes match"));
    assert!(ui.get_library_filters_active());
    assert!(ui.get_library_overview().starts_with("0 unique sounds"));
    ui.invoke_search("ioannou 2".into());
    h.settle();
    assert!(ui.get_library_overview().starts_with("1 unique sound ·"));
    assert!(!ui.get_library_overview().contains("1 unique sounds"));
}

#[test]
fn library_ux_filters_overlay_keeps_selection_and_geometry_and_escape_closes_it() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    h.settle();
    let key = ui.get_library_key();
    let title = ui.get_sound_title();
    let geometry = labelled(ui, &title).map_to_window(labelled(ui, &title).geometry().origin);
    action_button(ui, "Filters").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert!(ui.get_library_tools_open());
    assert_eq!(ui.get_library_key(), key);
    let after = labelled(ui, &title).map_to_window(labelled(ui, &title).geometry().origin);
    assert_eq!(
        geometry, after,
        "filters must not push the selected sound down"
    );
    assert!(field(ui, "Study evidence filter").is_visible());
    h.key(Key::Escape);
    assert!(!ui.get_library_tools_open());
    assert_eq!(ui.get_library_key(), key);
}

#[test]
fn library_ux_visible_component_choices_add_every_kind_to_the_current_document() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let h = Harness::new();
    h.workbench();
    let ui = h.ui();
    for (index, label) in [
        "Add steady tone",
        "Add pulse train",
        "Add binaural pair",
        "Add monaural pair",
        "Add pulsed tone (isochronic)",
    ]
    .into_iter()
    .enumerate()
    {
        reveal_library_control(&h, label).accessible_action(&AccessibilityAction::Default);
        h.settle();
        assert_eq!(ui.get_components().row_count(), index + 1);
    }
    let previous = ui.get_document();
    ui.invoke_new_draft();
    let [Request::Unsaved(prompt)] = h.take()[..] else {
        panic!("dirty draft must be protected");
    };
    h.adapter.respond(prompt, PromptAnswer::Discard);
    h.settle();
    assert_ne!(ui.get_document(), previous);
    // A late add addressed to the replaced document is refused.
    ui.invoke_add_component(previous, 0);
    h.settle();
    assert_eq!(ui.get_components().row_count(), 0);
    action_button(ui, "Add steady tone").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert_eq!(ui.get_components().row_count(), 1, "the visible choice targets the current document");
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn library_ux_coupled_errors_appear_once_by_the_fields_and_clear_after_correction() {
    let h = Harness::new();
    h.workbench();
    let ui = h.ui();
    ui.invoke_add_component(ui.get_document(), 2);
    h.settle();
    let owner = h.component_key(0);
    ui.invoke_commit_parameter(
        parameter_field(&owner.clone(), ParameterId::PairOffset),
        "50000".into(),
    );
    h.settle();
    let row = ui.get_components().row_data(0).unwrap();
    let errors: Vec<_> = row
        .parameters
        .iter()
        .filter(|p| !p.diagnostic.is_empty())
        .collect();
    assert_eq!(
        errors.len(),
        2,
        "both coupled inputs must remain identified"
    );
    assert_eq!(errors.iter().filter(|p| p.show_diagnostic).count(), 1);
    assert!(
        errors[0].diagnostic.contains("50200")
            && errors[0].diagnostic.contains("24000")
            && errors[0].diagnostic.contains("export")
    );
    assert!(
        row.error.is_empty(),
        "field diagnostics must not be repeated below the component"
    );
    ui.invoke_commit_parameter(parameter_field(&owner, ParameterId::PairOffset), "2".into());
    h.settle();
    assert!(
        ui.get_components()
            .row_data(0)
            .unwrap()
            .parameters
            .iter()
            .all(|p| p.diagnostic.is_empty() && !p.show_diagnostic)
    );
}

#[test]
fn library_ux_file_menu_keeps_playback_and_independent_copy_commands() {
    let (h, _directory) = controlled_harness();
    let ui = h.ui();
    let point = centre(&action_button(ui, "File"));
    drag(ui, point, point);
    h.settle();
    assert!(action_button(ui, "Play selected sound").is_visible());
    let point = centre(&action_button(ui, "Play selected sound"));
    drag(ui, point, point);
    h.settle();
    assert!(h.adapter.inspect(|w| w.audio_busy()));
    let audio = take_job(&h, JobKind::Audio);
    finish_stopped_audio(&h, audio);
    h.settle();
    h.workbench();
    let point = centre(&action_button(ui, "File"));
    drag(ui, point, point);
    h.settle();
    let point = centre(&action_button(ui, "Save independent copy"));
    drag(ui, point, point);
    // Capture the admitted job before timer polling can execute file jobs.
    assert!(h.adapter.inspect(|w| w.file_busy()));
    let file = take_job(&h, JobKind::File);
    finish_file(&h, file);
    h.wait_for_files();
}

#[test]
fn workspace_navigation_owns_real_panels_and_supports_keyboard_switching() {
    let h = Harness::new();
    let ui = h.ui();
    let panel = find_control(ui, |item| {
        item.is_visible()
            && item
                .accessible_string_property(AccessibleStringProperty::Label)
                .as_deref()
                == Some("Library")
            && item
                .visit_descendants(|child| {
                    if child
                        .accessible_string_property(AccessibleStringProperty::Label)
                        .as_deref()
                        == Some("Find a recipe")
                    {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                })
                .is_some()
    })
    .expect("the named Library panel owns its real controls");
    assert!(panel.is_accessible());
    let tabs = labelled(ui, "Workspace navigation");
    WindowInner::from_pub(ui.window()).set_focus_item(&tabs, true, FocusReason::Programmatic);
    h.key(Key::RightArrow);
    assert_eq!(h.adapter.inspect(|w| w.view()), View::Workbench);
    assert!(
        find_control(ui, |item| item.is_visible()
            && item
                .accessible_string_property(AccessibleStringProperty::Label)
                .as_deref()
                == Some("Workbench")
            && item
                .visit_descendants(|child| {
                    if child
                        .accessible_string_property(AccessibleStringProperty::Label)
                        .as_deref()
                        == Some("Sound title")
                    {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                })
                .is_some())
        .is_some()
    );
    h.key(Key::LeftArrow);
    assert_eq!(h.adapter.inspect(|w| w.view()), View::Library);
}

#[cfg(test)]
const fn finite(value: f64) -> auditorylab_app::core::Finite {
    match auditorylab_app::core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

#[test]
fn runtime_filter_choices_cover_every_offered_facet_and_absence() {
    use auditorylab_app::core::rhythm::Completeness;
    use auditorylab_app::core::{provenance::ConditionRole, provenance::Population};
    use auditorylab_app::discovery::query::{Direction, GroupBy};

    let h = Harness::new();
    for (index, expected) in std::iter::once(None)
        .chain(
            [
                Population::Human,
                Population::Mouse,
                Population::OtherAnimal,
                Population::Mixed,
                Population::NotApplicable,
                Population::Unspecified,
            ]
            .into_iter()
            .map(Some),
        )
        .enumerate()
    {
        apply_runtime_facets(&h, i32::try_from(index).unwrap(), 0, 0, 0, 0);
        h.adapter.inspect(|w| {
            assert_eq!(
                w.catalog_query().populations,
                expected.into_iter().collect::<Vec<_>>()
            );
        });
    }
    for (index, expected) in std::iter::once(None)
        .chain(
            [
                ConditionRole::Test,
                ConditionRole::Comparison,
                ConditionRole::Mixed,
                ConditionRole::Unspecified,
            ]
            .into_iter()
            .map(Some),
        )
        .enumerate()
    {
        apply_runtime_facets(&h, 0, i32::try_from(index).unwrap(), 0, 0, 0);
        h.adapter.inspect(|w| {
            assert_eq!(
                w.catalog_query().roles,
                expected.into_iter().collect::<Vec<_>>()
            );
        });
    }
    for (index, expected) in std::iter::once(None)
        .chain(
            [Completeness::Complete, Completeness::Invalid]
                .into_iter()
                .map(Some),
        )
        .enumerate()
    {
        apply_runtime_facets(&h, 0, 0, i32::try_from(index).unwrap(), 0, 0);
        h.adapter
            .inspect(|w| assert_eq!(w.catalog_query().analysis, expected));
    }
    for (index, expected) in std::iter::once(None)
        .chain([Direction::Asc, Direction::Desc].into_iter().map(Some))
        .enumerate()
    {
        apply_runtime_facets(&h, 0, 0, 0, i32::try_from(index).unwrap(), 0);
        h.adapter
            .inspect(|w| assert_eq!(w.catalog_query().direction, expected));
    }
    for (index, expected) in [
        GroupBy::None,
        GroupBy::Band,
        GroupBy::Method,
        GroupBy::Study,
        GroupBy::Collection,
    ]
    .into_iter()
    .enumerate()
    {
        apply_runtime_facets(&h, 0, 0, 0, 0, i32::try_from(index).unwrap());
        h.adapter
            .inspect(|w| assert_eq!(w.catalog_query().group_by, expected));
    }
    assert_invalid_runtime_facet(&h);
}

#[test]
fn real_worker_completion_reaches_projection_without_a_timer_and_wakes_coalesce() {
    let events = crate::test_platform::install_with_events();
    let (workspace, _directory, store) = bound_workspace(
        auditorylab_app::services::catalog().unwrap(),
        Capabilities::no_device_playback(),
    );
    let windows = Windows::new().unwrap();
    // No worker exists in this phase, so unrelated notifications cannot satisfy coalescing.
    let wake = crate::adapter::tests::isolated_waker(&windows.main);
    wake();
    wake();
    let event = events.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(
        events.try_recv().is_err(),
        "two wakes schedule exactly one event"
    );
    event();
    let adapter = SlintAdapter::install(
        &windows,
        workspace,
        Box::new(Recorder(Rc::new(RefCell::new(Vec::new())))),
        Box::new(auditorylab_app::workspace::Runner::new()),
        None,
    );
    let pump = || {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while adapter.inspect(|workspace| workspace.file_busy()) {
            assert!(
                !adapter.timer_running(),
                "file work never enables the redraw timer"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not notify the event loop"
            );
            events
                .recv_timeout(Duration::from_secs(5))
                .expect("actual Slint proxy notification")();
        }
        while let Ok(event) = events.try_recv() {
            event();
        }
    };
    pump();
    assert!(!adapter.timer_running());
    let before = adapter.counts.document_projections.get();
    windows.main.invoke_new_draft();
    windows.main.invoke_save_recipe();
    assert!(adapter.inspect(|workspace| workspace.file_busy()));
    pump();
    assert!(!adapter.inspect(|workspace| workspace.editor().dirty()));
    assert_eq!(
        store
            .scan(&auditorylab_app::job::Cancel::default())
            .entries
            .len(),
        1
    );
    assert!(adapter.counts.document_projections.get() > before);
    assert!(!adapter.timer_running());
    let wake = adapter.test_waker();
    adapter.shutdown().unwrap();
    let projected = adapter.counts.document_projections.get();
    while let Ok(event) = events.try_recv() {
        event();
    }
    assert_eq!(adapter.counts.document_projections.get(), projected);
    wake();
    assert!(
        events.try_recv().is_err(),
        "shutdown rejects later notifications"
    );
}

#[test]
fn app_facet_selections_project_without_overwriting_pending_native_filter_edits() {
    use auditorylab_app::core::rhythm::Completeness;
    use auditorylab_app::core::{provenance::ConditionRole, provenance::Population};
    use auditorylab_app::discovery::collection::CatalogQuery;

    let (mut workspace, _root, _store) = bound_workspace(
        auditorylab_app::services::catalog().unwrap(),
        Capabilities::no_device_playback(),
    );
    workspace
        .apply(Command::Library(LibraryCommand::SetQuery(Box::new(
            CatalogQuery {
                scope: Scope::BuiltIn,
                populations: vec![Population::Mouse],
                roles: vec![ConditionRole::Comparison],
                analysis: Some(Completeness::Invalid),
                direction: Some(Direction::Desc),
                group_by: GroupBy::Study,
                ..CatalogQuery::default()
            },
        ))))
        .unwrap();
    let h = Harness::with_startup_workspace(workspace);
    let ui = h.ui();
    assert_eq!(ui.get_population_choice(), 2);
    assert_eq!(ui.get_role_choice(), 2);
    assert_eq!(ui.get_analysis_choice(), 2);
    assert_eq!(ui.get_direction_choice(), 2);
    assert_eq!(ui.get_group_choice(), 3);
    // Advanced fields stage locally until Apply. Unrelated projection keeps that input.
    ui.set_population_choice(1);
    ui.set_created_from("unfinished date".into());
    ui.invoke_navigate_library(1);
    h.settle();
    assert_eq!(ui.get_population_choice(), 1);
    assert_eq!(ui.get_created_from(), "unfinished date");
    ui.invoke_reset_library_sort();
    h.settle();
    assert_eq!(ui.get_direction_choice(), 0);
    assert_eq!(ui.get_group_choice(), 3);
    assert_eq!(ui.get_population_choice(), 1);
    assert_eq!(ui.get_created_from(), "unfinished date");
    ui.invoke_clear_library_filters();
    h.settle();
    assert_eq!(ui.get_population_choice(), 0);
    assert_eq!(ui.get_role_choice(), 0);
    assert_eq!(ui.get_analysis_choice(), 0);
    assert_eq!(ui.get_created_from(), "");
    assert_eq!(ui.get_group_choice(), 3);
    h.adapter.inspect(|workspace| {
        assert_eq!(
            workspace.catalog_query().populations,
            [] as [auditorylab_core::provenance::Population; 0]
        );
        assert_eq!(workspace.catalog_query().group_by, GroupBy::Study);
        assert_eq!(workspace.catalog_query().direction, None);
    });
}

#[test]
fn runtime_query_controls_keep_semantics_when_rendered_options_are_reordered() {
    use auditorylab_app::core::rhythm::Band;
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window().set_size(slint::LogicalSize::new(800.0, 600.0));
    h.settle();
    h.adapter.reverse_rendered_choices();
    h.settle();
    assert_ne!(
        ui.get_band_choice(),
        0,
        "any is deliberately not at index zero"
    );
    assert!(!ui.get_rhythm_filtered());
    assert_eq!(ui.get_library_heading(), "All sounds");
    assert!(
        find_control(ui, |item| item
            .accessible_string_property(AccessibleStringProperty::Label)
            .as_deref()
            == Some("Clear rhythm filters"))
        .is_none()
    );
    ui.window()
        .set_size(slint::LogicalSize::new(1440.0, 1000.0));
    h.settle();
    h.adapter.reverse_rendered_choices();
    h.settle();
    let gamma = ui
        .get_band_navigation()
        .iter()
        .find(|row| row.label == "Gamma")
        .unwrap();
    assert_ne!(
        gamma.choice_index, 5,
        "the navigation must not rely on canonical band positions"
    );
    labelled(ui, &gamma.text).accessible_action(&AccessibilityAction::Default);
    h.settle();
    h.adapter
        .inspect(|workspace| assert_eq!(workspace.catalog_query().rhythm.bands, vec![Band::Gamma]));
    assert_eq!(ui.get_library_heading(), "Gamma");
    assert_reordered_sort_and_clear(&h);
}

fn parameter_field(row: &str, parameter: ParameterId) -> slint::SharedString {
    row.parse::<auditorylab_app::document::editing::RowKey>()
        .unwrap()
        .parameter(parameter)
        .unwrap()
        .to_string()
        .into()
}
fn metadata_field_handle(row: &str, field: MetadataField) -> slint::SharedString {
    row.parse::<auditorylab_app::document::editing::RowKey>()
        .unwrap()
        .metadata(field)
        .unwrap()
        .to_string()
        .into()
}
fn field_parameter(handle: &str) -> ParameterId {
    let field = handle
        .parse::<auditorylab_app::document::editing::FieldHandle>()
        .unwrap();
    match field.field {
        auditorylab_app::document::editing::EditingField::Parameter(_, parameter) => parameter,
        auditorylab_app::document::editing::EditingField::Metadata(_) => {
            panic!("expected a component parameter")
        }
    }
}

fn assert_metadata_and_component_callbacks(h: &Harness) {
    let ui = h.ui();
    let binding = h.component_key(0);
    let title = EditingField::Metadata(MetadataField::Title);
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "Probe".into(),
    );
    assert_eq!(
        h.adapter.inspect(|w| w.editor().draft().title().to_owned()),
        "Probe"
    );
    assert_eq!(ui.get_title_field().text, "Probe");
    ui.invoke_stage_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "Draft".into(),
    );
    assert_eq!(
        h.adapter
            .inspect(|w| w.editor().staged(title).map(str::to_owned)),
        Some("Draft".to_owned())
    );
    ui.invoke_restore_metadata(metadata_field_handle(
        &ui.get_document(),
        MetadataField::Title,
    ));
    assert_eq!(ui.get_title_field().text, "Probe");

    ui.invoke_set_modulation(binding.clone(), 1);
    assert_eq!(ui.get_components().row_data(0).unwrap().modulation, 1);
    assert_eq!(
        h.keys(0),
        [
            ParameterId::ToneFrequency,
            ParameterId::PeakLevel,
            ParameterId::ModulationRate,
            ParameterId::ModulationDepth
        ]
    );
    ui.invoke_set_routing(h.component_key(0), 1);
    assert_eq!(ui.get_components().row_data(0).unwrap().route, 1);
    ui.invoke_remove_component(h.component_key(0));
    assert_eq!(ui.get_components().row_count(), 0);
}

fn assert_managed_trash_controls(h: &Harness, store: &auditorylab_app::services::library::Library) {
    let ui = h.ui();
    let key = ui.get_library_key();
    ui.invoke_library_action(
        ui.get_library_entry(),
        crate::generated::LibraryMutationAction::Trash,
        "".into(),
    );
    h.wait_for_files();
    ui.invoke_collection_filter(4, 0);
    h.wait_for_files();
    assert!(ui.get_library_trashed());
    assert!(!ui.get_can_play());
    ui.invoke_library_action(
        ui.get_library_entry(),
        crate::generated::LibraryMutationAction::Restore,
        "".into(),
    );
    h.wait_for_files();
    ui.invoke_collection_filter(2, 0);
    h.wait_for_files();
    ui.invoke_library_action(
        ui.get_library_entry(),
        crate::generated::LibraryMutationAction::Trash,
        "".into(),
    );
    h.wait_for_files();
    ui.invoke_collection_filter(4, 0);
    h.wait_for_files();
    ui.invoke_library_action(
        ui.get_library_entry(),
        crate::generated::LibraryMutationAction::RequestPurge,
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
            .scan(&auditorylab_app::job::Cancel::default())
            .entries
            .is_empty()
    );
}

fn assert_invalid_native_dates(h: &Harness) {
    let ui = h.ui();
    ui.invoke_more_filters(
        "".into(),
        0,
        0,
        0,
        0,
        1,
        crate::generated::DateInput {
            from_text: "2026-10-24".into(),
            through_text: "2026-10-23".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
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

fn assert_external_conflict_and_copy(
    h: &Harness,
    file: &std::path::Path,
    original: &auditorylab_app::core::Recipe,
    store: &auditorylab_app::services::library::Library,
) {
    let ui = h.ui();
    let mut other = auditorylab_app::services::open_file(file)
        .unwrap()
        .recipe()
        .clone()
        .into_builder();
    other.content.description.as_mut().unwrap().title = "Other application".into();
    std::fs::write(file, other.build().unwrap().to_toml().unwrap()).unwrap();
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "Retained after conflict".into(),
    );
    ui.invoke_save_recipe();
    h.wait_for_files();
    assert!(ui.get_dirty());
    assert!(ui.get_status().contains("changed identity or bytes"));
    assert_eq!(
        auditorylab_app::services::open_file(file).unwrap().title(),
        "Other application"
    );
    ui.invoke_save_copy();
    h.wait_for_files();
    assert!(!ui.get_dirty());
    assert_ne!(
        h.adapter
            .inspect(|w| w.editor().draft().recipe().unwrap().document().id),
        original.document().id
    );
    assert_eq!(
        store
            .scan(&auditorylab_app::job::Cancel::default())
            .entries
            .len(),
        2
    );
    assert_eq!(
        auditorylab_app::services::open_file(file).unwrap().title(),
        "Other application"
    );
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

fn assert_empty_scope_recovery(h: &Harness) {
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let ui = h.ui();

    for scope in [2, 3, 4] {
        ui.invoke_collection_filter(scope, sort_choice(ui, SortBy::Title));
        h.settle();
        assert_eq!(ui.get_catalog().row_count(), 0);
        action_button(ui, "Library options…").accessible_action(&AccessibilityAction::Default);
        h.settle();
        assert!(reveal_library_control(h, "Initialize configured library").is_visible());
        ui.invoke_close_library_filters();
        h.settle();
        let refresh = centre(&action_button(ui, "Refresh"));
        drag(ui, refresh, refresh);
        h.wait_for_files();
        assert_eq!(
            h.adapter.inspect(|w| w.catalog_query().sort),
            auditorylab_app::discovery::query::SortBy::Title
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

fn assert_unavailable_root_sizes(h: &Harness) {
    let ui = h.ui();
    for size in [13.0, 26.0] {
        ui.set_ui_size(size);
        ui.window().set_size(slint::LogicalSize::new(800.0, 600.0));
        h.settle();
        for name in [
            "Filters",
            "Library options…",
            "Clear all filters",
            "Open recipe…",
            "Create sound",
            "Play selected sound",
        ] {
            let item = if matches!(name, "Clear all filters" | "Open recipe…" | "Create sound") {
                find_control(ui, |item| {
                    item.is_accessible()
                        && item
                            .accessible_string_property(AccessibleStringProperty::Label)
                            .as_deref()
                            == Some(name)
                })
                .expect("scrollable action exists")
            } else {
                action_button(ui, name)
            };
            if matches!(name, "Clear all filters" | "Open recipe…" | "Create sound") {
                let owns_focus = || {
                    let focused = WindowInner::from_pub(ui.window())
                        .focus_item
                        .borrow()
                        .upgrade();
                    focused.is_some_and(|focus| {
                        focus == item
                            || item
                                .visit_descendants(|child| {
                                    if child == &focus {
                                        ControlFlow::Break(())
                                    } else {
                                        ControlFlow::Continue(())
                                    }
                                })
                                .is_some()
                    })
                };
                for _ in 0..80 {
                    if owns_focus() {
                        break;
                    }
                    h.key(Key::Tab);
                }
                assert!(owns_focus(), "Tab must reach {name} at {size}px");
                h.settle();
                let scroll = labelled(ui, "Library results");
                let viewport = scroll
                    .visit_descendants(|child| {
                        if child
                            .downcast::<slint::private_unstable_api::re_exports::Flickable>()
                            .is_some()
                        {
                            ControlFlow::Break(child.clone())
                        } else {
                            ControlFlow::Continue(())
                        }
                    })
                    .expect("result scroll exposes its actual viewport");
                let view_bounds = viewport.geometry();
                let view_origin = viewport.map_to_native_window(view_bounds.origin);
                let bounds = item.geometry();
                let origin = item.map_to_native_window(bounds.origin);
                assert!(
                    origin.y >= view_origin.y
                        && origin.y + bounds.size.height <= view_origin.y + view_bounds.size.height,
                    "{name} must be revealed inside the result viewport at {size}px: \
                        item={origin:?} size={:?}; view={view_origin:?} size={:?}; scroll={}",
                    bounds.size,
                    view_bounds.size,
                    ui.get_library_scroll_y()
                );
            }
            let position = centre(&item);
            assert!(
                position.x >= 0.0 && position.x < 800.0 && position.y >= 0.0 && position.y < 600.0,
                "{name} must remain reachable at {size}px, got {position:?}"
            );
        }
    }
}

fn assert_large_text_catalog_row(h: &Harness) {
    let ui = h.ui();
    ui.set_ui_size(26.0);
    h.settle();
    let label = ui.get_catalog().row_data(0).unwrap().text;
    assert!(label.contains("Duration") && label.contains("Created: Unknown"));
    let row = labelled(ui, &label);
    let row_rect = row.geometry();
    assert!(
        row_rect.size.width <= 800.0 && row_rect.size.height >= 26.0 * 3.5,
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

fn assert_evidence_date_filters(h: &Harness) {
    use auditorylab_app::discovery::collection::{CatalogQuery, DateRange};

    let ui = h.ui();

    ui.invoke_rhythm_filter(0, 0);
    h.wait_for_files();
    ui.invoke_more_filters(
        "key:source".into(),
        2,
        2,
        2,
        2,
        3,
        crate::generated::DateInput {
            from_text: "2026-01-01".into(),
            through_text: "2026-01-02".into(),
        },
        crate::generated::DateInput {
            from_text: "2026-03-01".into(),
            through_text: "2026-03-02".into(),
        },
        crate::generated::DateInput {
            from_text: "2026-05-01".into(),
            through_text: "2026-05-02".into(),
        },
    );
    h.wait_for_files();
    let expected = CatalogQuery {
        studies: vec!["key:source".into()],
        populations: vec![auditorylab_app::core::provenance::Population::Mouse],
        roles: vec![auditorylab_app::core::provenance::ConditionRole::Comparison],
        analysis: Some(auditorylab_app::core::rhythm::Completeness::Invalid),
        direction: Some(Direction::Desc),
        group_by: GroupBy::Study,
        created: auditorylab_app::discovery::calendar::local_range(
            auditorylab_app::discovery::query::CalendarField::Created,
            "2026-01-01",
            "2026-01-02",
        )
        .unwrap(),
        modified: auditorylab_app::discovery::calendar::local_range(
            auditorylab_app::discovery::query::CalendarField::Modified,
            "2026-03-01",
            "2026-03-02",
        )
        .unwrap(),
        added: auditorylab_app::discovery::calendar::local_range(
            auditorylab_app::discovery::query::CalendarField::Added,
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

fn assert_inclusive_date_error(h: &Harness, field: usize, label: &str) {
    let ui = h.ui();

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
    submit_date_inputs(ui, from, through);
    h.wait_for_files();
    h.adapter.inspect(|w| {
        assert_eq!(w.catalog_query(), &previous);
        let error = auditorylab_presentation::library::workspace_library_alert(w);
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
    submit_date_inputs(ui, from, through);
    h.wait_for_files();
    h.adapter.inspect(|w| {
        assert_eq!(
            auditorylab_presentation::library::workspace_library_alert(w),
            ""
        );
        let query = w.catalog_query();
        let range = match field {
            0 => &query.created,
            1 => &query.modified,
            _ => &query.added,
        };
        assert!(range.from.unwrap() < range.before.unwrap());
    });
}

fn apply_runtime_facets(
    h: &Harness,
    population: i32,
    role: i32,
    analysis: i32,
    direction: i32,
    group: i32,
) {
    let ui = h.ui();

    ui.invoke_more_filters(
        "".into(),
        population,
        role,
        analysis,
        direction,
        group,
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
    );
    h.wait_for_files();
}

/// The rendered sort menu position whose visible label names this sort.
fn sort_choice(ui: &MainWindow, sort: SortBy) -> i32 {
    let labels = ui.get_sort_labels();
    let index = (0..labels.row_count())
        .find(|&row| {
            labels
                .row_data(row)
                .is_some_and(|label| label == sort.label())
        })
        .unwrap_or_else(|| panic!("{} is not offered in the sort menu", sort.key()));
    i32::try_from(index).unwrap()
}

fn assert_reordered_sort_and_clear(h: &Harness) {
    use auditorylab_app::discovery::collection::CatalogQuery;
    use slint::private_unstable_api::re_exports::AccessibilityAction;

    let ui = h.ui();

    let mut query = h
        .adapter
        .inspect(|workspace| workspace.catalog_query().clone());
    query.scope = Scope::BuiltIn;
    query.sort = SortBy::Title;
    query.direction = Some(Direction::Desc);
    h.adapter.command(|workspace| {
        workspace.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))
    });
    h.adapter.reverse_rendered_choices();
    h.settle();
    assert_eq!(
        ui.get_sort_choice(),
        5,
        "Title is deliberately at a formerly date-specific position"
    );
    assert!(!ui.get_show_built_in_dates());
    // Built-in rows omit dates unless the sort is by a date; the selected
    // sound's own Source and dates section still names them.
    assert!(
        labelled(ui, "Library results")
            .visit_descendants(|item| if item
                .accessible_string_property(AccessibleStringProperty::Label)
                .is_some_and(|label| label.starts_with("Created:"))
            {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            })
            .is_none()
    );
    let added = ui
        .get_sort_labels()
        .iter()
        .position(|label| label == SortBy::Added.label())
        .unwrap();
    ui.invoke_collection_filter(ui.get_collection_choice(), i32::try_from(added).unwrap());
    h.settle();
    h.adapter.reverse_rendered_choices();
    h.settle();
    assert_eq!(
        ui.get_sort_choice(),
        2,
        "Added is deliberately outside the old date-sort positions"
    );
    assert!(ui.get_show_built_in_dates());
    assert!(labelled(ui, "Added here:").is_visible());
    ui.window().set_size(slint::LogicalSize::new(800.0, 600.0));
    h.settle();
    h.adapter.reverse_rendered_choices();
    h.settle();
    let before = h
        .adapter
        .inspect(|workspace| workspace.catalog_query().clone());
    labelled(ui, "Clear rhythm filters").accessible_action(&AccessibilityAction::Default);
    h.settle();
    let expected = CatalogQuery {
        rhythm: auditorylab_app::discovery::query::RhythmQuery::default(),
        ..before
    };
    h.adapter
        .inspect(|workspace| assert_eq!(workspace.catalog_query(), &expected));
    assert!(!ui.get_rhythm_filtered());
    assert_eq!(ui.get_library_heading(), Scope::BuiltIn.label());
    ui.window()
        .set_size(slint::LogicalSize::new(1440.0, 1000.0));
    h.settle();
    h.adapter.reverse_rendered_choices();
    h.settle();
    assert_ne!(ui.get_band_choice(), 0);
    // Select the actual enlarged heading, not the same caption on a navigation row.
    assert!(
        find_control(ui, |item| item
            .accessible_string_property(AccessibleStringProperty::Label)
            .as_deref()
            == Some(Scope::BuiltIn.label())
            && item.geometry().size.height > ui.get_ui_size() * 1.5)
        .is_some()
    );
}

fn assert_invalid_runtime_facet(h: &Harness) {
    let ui = h.ui();
    apply_runtime_facets(h, 0, 0, 0, 0, 0);
    ui.invoke_more_filters(
        "".into(),
        999,
        0,
        0,
        0,
        0,
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
        crate::generated::DateInput {
            from_text: "".into(),
            through_text: "".into(),
        },
    );
    h.adapter.inspect(|w| {
        assert_eq!(
            w.catalog_query(),
            &auditorylab_app::discovery::collection::CatalogQuery::default()
        );
    });
}

fn submit_date_inputs(ui: &MainWindow, from: [&str; 3], through: [&str; 3]) {
    ui.invoke_more_filters(
        "".into(),
        0,
        0,
        0,
        0,
        0,
        crate::generated::DateInput {
            from_text: from[0].into(),
            through_text: through[0].into(),
        },
        crate::generated::DateInput {
            from_text: from[1].into(),
            through_text: through[1].into(),
        },
        crate::generated::DateInput {
            from_text: from[2].into(),
            through_text: through[2].into(),
        },
    );
}

#[test]
fn edit_menu_commands_reach_the_focused_text_field() {
    let h = Harness::new();
    let ui = h.ui();
    let search = labelled(ui, "Find a recipe");
    let point = centre(&search);
    drag(ui, point, point);
    for character in ["a", "b", "c"] {
        h.key(character);
    }
    let value = || {
        search
            .accessible_string_property(AccessibleStringProperty::Value)
            .unwrap_or_default()
    };
    assert_eq!(value(), "abc");
    // Edit > Select All, then typing replaces the whole field.
    ui.invoke_edit_shortcut("a".into());
    h.settle();
    h.key("q");
    assert_eq!(
        value(),
        "q",
        "Select All from the menu selected the focused field"
    );
}

#[test]
fn pulse_mode_and_shape_callbacks_preserve_surviving_widgets_and_retire_old_interpretations() {
    use auditorylab_app::document::construction::CreationKind;
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    let index = CreationKind::ALL
        .iter()
        .position(|kind| *kind == CreationKind::PulsedTone)
        .unwrap();
    ui.invoke_add_component(ui.get_document(), i32::try_from(index).unwrap());
    h.settle();
    let old = h.component_key(0);
    let row = ui.get_components().row_data(0).unwrap();
    assert_eq!(row.modulation_width, 1);
    assert_eq!(row.modulation_shape, 1);
    let label = format!(
        "{}: {}",
        row.name,
        auditorylab_presentation::parameter_input_label(ParameterId::ToneFrequency)
    );
    let input = h.field(&label);
    let inner = WindowInner::from_pub(ui.window());
    inner.set_focus_item(&input, true, FocusReason::Programmatic);
    h.key(Key::End);
    h.key("x");
    let before = text_of(&input);
    let cursor = input
        .downcast::<TextInput>()
        .unwrap()
        .as_pin_ref()
        .cursor_position_byte_offset();
    ui.invoke_set_pulse_option(old.clone(), 2, 0);
    h.settle();
    assert!(h.keys(0).contains(&ParameterId::ModulationPulseDuration));
    assert!(!h.keys(0).contains(&ParameterId::ModulationDuty));
    assert_eq!(inner.focus_item.borrow().upgrade().as_ref(), Some(&input));
    assert_eq!(text_of(&input), before);
    assert_eq!(
        input
            .downcast::<TextInput>()
            .unwrap()
            .as_pin_ref()
            .cursor_position_byte_offset(),
        cursor
    );
    // The old focused field is still continuously applicable: another native keystroke remains
    // admitted.
    h.key("y");
    let staged = text_of(&input);
    let mut expected = before.clone();
    expected.insert(usize::try_from(cursor).unwrap(), 'y');
    assert_eq!(staged, expected);
    assert_eq!(
        h.adapter.inspect(|w| {
            let field = parameter(0, ParameterId::ToneFrequency)(w);
            w.editor().staged(field).map(str::to_owned)
        }),
        Some(staged)
    );
    ui.invoke_set_pulse_option(h.component_key(0), 2, 1);
    h.settle();
    ui.invoke_commit_parameter(
        parameter_field(&old, ParameterId::ModulationDuty),
        "25".into(),
    );
    assert_eq!(
        h.adapter.inspect(|w| w.editor().draft().components()[0]
            .form
            .get(ParameterId::ModulationDuty)),
        Some(finite(0.5))
    );
    ui.invoke_set_pulse_option(h.component_key(0), 3, 0);
    h.settle();
    assert!(!h.keys(0).contains(&ParameterId::ModulationRise));
    ui.invoke_set_pulse_option(h.component_key(0), 3, 1);
    h.settle();
    assert_eq!(
        h.adapter.inspect(|w| w.editor().draft().components()[0]
            .form
            .get(ParameterId::ModulationRise)),
        Some(finite(0.05 * 0.1))
    );
    assert!(
        h.adapter.inspect(|w| w.audio_phase().is_none()),
        "creation and interpretation edits never start playback"
    );
}

#[test]
fn closed_monitor_commands_refresh_calculated_models_without_a_redraw_tick() {
    use auditorylab_app::workspace::monitor::ModelSource;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    h.adapter.tick();
    h.settle();
    assert_eq!(
        h.adapter.inspect(|w| w.model_view().unwrap().source),
        ModelSource::Recipe
    );
    let library_caption = ui.get_model_caption();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 4);
    // No adapter.tick: closed-monitor editing must not depend on a redraw timer.
    assert_eq!(
        h.adapter.inspect(|w| w.model_view().unwrap().source),
        ModelSource::Draft
    );
    assert_ne!(ui.get_model_caption(), library_caption);
    assert!(ui.get_model_caption().contains("Untitled sound"));
    assert_eq!(ui.get_model_lanes().row_count(), 1);
    assert!(
        ui.get_model_lanes()
            .row_data(0)
            .unwrap()
            .label
            .contains("10 Hz pulsed modulation")
    );
    ui.invoke_commit_parameter(
        parameter_field(&h.component_key(0), ParameterId::ModulationPulseRate),
        "8".into(),
    );
    assert!(
        ui.get_model_lanes()
            .row_data(0)
            .unwrap()
            .label
            .contains("8 Hz pulsed modulation")
    );
    ui.invoke_commit_parameter(
        parameter_field(&h.component_key(0), ParameterId::ToneFrequency),
        "0".into(),
    );
    assert!(h.adapter.inspect(|w| w.model_view().is_none()));
    assert_eq!(ui.get_model_lanes().row_count(), 0);
    ui.invoke_tab_changed(0);
    h.settle();
    assert_eq!(
        h.adapter.inspect(|w| w.model_view().unwrap().source),
        ModelSource::Recipe
    );
    assert_eq!(ui.get_model_caption(), library_caption);
    ui.invoke_tab_changed(1);
    h.settle();
    assert!(h.adapter.inspect(|w| w.model_view().is_none()));
    assert_eq!(ui.get_model_lanes().row_count(), 0);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn workbench_and_library_show_every_section_without_disclosures() {
    use slint::private_unstable_api::re_exports::AccessibleRole;
    let h = Harness::new();
    let ui = h.ui();
    let disclosures = || {
        let window = WindowInner::from_pub(ui.window());
        let mut found = Vec::new();
        let _ = window.window_item_rc().unwrap().visit_descendants(|item| {
            let label = item
                .accessible_string_property(AccessibleStringProperty::Label)
                .unwrap_or_default();
            if item.is_visible()
                && (item.accessible_role() == AccessibleRole::Button
                    && (label.ends_with(" · collapsed") || label.ends_with(" · expanded")))
            {
                found.push(label);
            }
            ControlFlow::<()>::Continue(())
        });
        found
    };
    assert_eq!(disclosures(), Vec::<slint::SharedString>::new(), "Library");
    for label in ["Export WAV…", "Open study source", "Exact UTC recipe and membership dates"] {
        assert!(reveal_library_control(&h, label).is_visible(), "{label} shows without a toggle");
    }
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 4);
    h.settle();
    assert_eq!(disclosures(), Vec::<slint::SharedString>::new(), "Workbench");
    for label in ["Custom recipe notes", "Saved duration", "Add steady tone"] {
        assert!(reveal_library_control(&h, label).is_visible(), "{label} shows without a toggle");
    }
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn learning_callbacks_use_real_workspace_admission_and_captured_creation() {
    use auditorylab_app::learning::Topic;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    let query = ui.get_query();
    let selection = ui.get_library_key();
    let document = ui.get_document();
    ui.invoke_open_learning(0);
    h.settle();
    assert!(ui.get_learning_visible());
    assert_eq!(
        h.adapter.inspect(|w| w.learning().unwrap().topic),
        Topic::Overview
    );
    assert!(!ui.get_can_play() && !ui.get_can_export() && !ui.get_can_save());
    ui.invoke_play_recipe();
    ui.invoke_save_recipe();
    ui.invoke_export_wav();
    assert_eq!(h.take().len(), 0);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    for topic in 0..5 {
        ui.invoke_open_learning(topic);
        h.settle();
        assert_eq!(ui.get_learning_topic(), topic);
        assert!(!ui.get_learning_body().is_empty());
    }
    for (band, rate) in ["2", "6", "10", "20", "32"].into_iter().enumerate() {
        ui.invoke_select_learning_band(i32::try_from(band).unwrap());
        h.settle();
        assert!(
            ui.get_learning_band_description()
                .contains(&format!("{rate} cycles per second"))
        );
        assert!(ui.get_learning_body().contains("one second"));
    }
    ui.invoke_browse_learning_library();
    h.settle();
    assert!(!ui.get_learning_visible());
    assert_eq!(ui.get_query(), query);
    assert_eq!(ui.get_library_key(), selection);
    assert_eq!(ui.get_document(), document);
    ui.invoke_open_learning(0);
    ui.invoke_create_learning_example();
    h.settle();
    assert!(!ui.get_learning_visible());
    assert_eq!(h.adapter.inspect(|w| w.view()), View::Workbench);
    assert_eq!(
        h.adapter.inspect(|w| w.editor().draft().components().len()),
        1
    );
    assert_ne!(ui.get_document(), document);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    assert_eq!(h.take().len(), 0);
}

#[test]
fn learning_native_topic_controls_fit_narrow_large_text_and_remain_revisitable() {
    use auditorylab_app::learning::Topic;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window().set_size(slint::LogicalSize::new(680.0, 600.0));
    for size in [13.0, 19.5, 26.0] {
        ui.set_ui_size(size);
        for topic in Topic::ALL {
            ui.invoke_open_learning(i32::try_from(topic.index()).unwrap());
            h.settle();
            let control = labelled(
                ui,
                auditorylab_presentation::learning::navigation_label(topic),
            );
            let position = centre(&control);
            assert!(position.x > 0.0 && position.x < 680.0);
            assert!(position.y > 0.0 && position.y < 600.0);
            assert!(control.geometry().size.width <= 680.0);
            assert!(control.geometry().size.height >= size);
        }
        action_button(ui, "Library").accessible_action(
            &slint::private_unstable_api::re_exports::AccessibilityAction::Default,
        );
        h.settle();
        assert!(!ui.get_learning_visible());
    }
}

#[test]
fn learning_palette_swap_preserves_native_diagram_content_draft_and_focus() {
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.invoke_open_learning(4);
    h.settle();
    ui.invoke_select_learning_band(4);
    h.settle();
    let document = ui.get_document();
    let description = ui.get_learning_band_description();
    let trace = ui.get_learning_band_items().row_data(4).unwrap().trace;
    let window = WindowInner::from_pub(ui.window());
    let focus = window.focus_item.borrow().upgrade();
    window
        .context()
        .set_color_scheme(slint::private_unstable_api::re_exports::ColorScheme::Light);
    h.settle();
    let light = ui.window().take_snapshot().unwrap();
    window
        .context()
        .set_color_scheme(slint::private_unstable_api::re_exports::ColorScheme::Dark);
    h.settle();
    let dark = ui.window().take_snapshot().unwrap();
    assert_ne!(light.as_slice(), dark.as_slice());
    assert_eq!(window.focus_item.borrow().upgrade(), focus);
    assert_eq!(ui.get_learning_topic(), 4);
    assert_eq!(ui.get_learning_band(), 4);
    assert_eq!(ui.get_learning_band_description(), description);
    assert_eq!(
        ui.get_learning_band_items().row_data(4).unwrap().trace,
        trace
    );
    assert_eq!(ui.get_document(), document);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn learning_keyboard_navigation_uses_visible_topics_and_returns_to_retained_work() {
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.invoke_focus_workspace_navigation();
    h.key(Key::End);
    assert!(ui.get_learning_visible());
    assert_eq!(ui.get_learning_topic(), 0);
    ui.invoke_focus_learning();
    h.key(Key::DownArrow);
    assert_eq!(ui.get_learning_topic(), 1);
    h.key(Key::DownArrow);
    assert_eq!(ui.get_learning_topic(), 2);
    assert_eq!(
        labelled(ui, "Pitch and rhythm")
            .accessible_string_property(AccessibleStringProperty::ItemSelected)
            .as_deref(),
        Some("true")
    );
    ui.invoke_focus_workspace_navigation();
    h.key(Key::Home);
    assert!(!ui.get_learning_visible());
    assert_eq!(h.adapter.inspect(|w| w.view()), View::Library);
    ui.invoke_resume_learning();
    h.settle();
    assert_eq!(
        ui.get_learning_topic(),
        2,
        "session reentry resumes the lesson"
    );
}

#[test]
fn learning_spectrum_is_native_and_current_tab_does_not_reset_its_lesson() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    let tab = |label: &str| {
        find_control(ui, |item| {
            item.accessible_string_property(AccessibleStringProperty::Label)
                .as_deref()
                == Some(label)
                && item
                    .accessible_string_property(AccessibleStringProperty::ItemSelectable)
                    .as_deref()
                    == Some("true")
        })
        .unwrap()
    };
    tab("Start here").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert_eq!(
        tab("Start here")
            .accessible_string_property(AccessibleStringProperty::ItemSelected)
            .as_deref(),
        Some("true")
    );
    ui.invoke_open_learning(4);
    ui.invoke_select_learning_band(4);
    h.settle();
    let diagram = labelled(ui, "Five normalized illustrative sine waves");
    assert!(
        diagram.geometry().size.width >= 700.0,
        "spectrum fills the reading column"
    );
    assert_eq!(ui.get_learning_band_items().row_count(), 5);
    let delta = ui.get_learning_band_items().row_data(0).unwrap();
    assert!(
        delta.trace.contains("L128 0.0000"),
        "quarter cycle reaches +1 at plot top"
    );
    assert!(
        delta.trace.contains("L256 50.0000"),
        "half cycle crosses zero at center"
    );
    assert!(
        delta.trace.contains("L384 100.0000"),
        "third quarter reaches −1 at bottom"
    );
    let gamma = ui.get_learning_band_items().row_data(4).unwrap();
    assert_eq!(gamma.name, "Gamma");
    assert_eq!(gamma.range, ">30–80 Hz");
    assert_eq!(gamma.rate, "32 cycles/s");
    assert!(gamma.trace.starts_with("M0 50.0000L1"));
    assert!(gamma.trace.ends_with("L1024 50.0000"));
    tab("Start here").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert_eq!(
        ui.get_learning_topic(),
        4,
        "current destination does not reset the lesson"
    );
    assert_eq!(ui.get_learning_band(), 4);
    ui.invoke_create_learning_example();
    h.settle();
    assert_eq!(
        tab("Workbench")
            .accessible_string_property(AccessibleStringProperty::ItemSelected)
            .as_deref(),
        Some("true")
    );
    assert_eq!(
        tab("Start here")
            .accessible_string_property(AccessibleStringProperty::ItemSelected)
            .as_deref(),
        Some("false")
    );
}

#[test]
fn newer_learning_navigation_supersedes_queued_working_tab() {
    let h = Harness::new();
    let ui = h.ui();
    let directory = h.library.as_ref().unwrap().0.path().join("preferences");
    let preference =
        auditorylab_app::onboarding::Preference::at_config_directory(directory.clone());
    h.adapter.set_onboarding_preference(Some(
        auditorylab_app::onboarding::Preference::at_config_directory(directory),
    ));
    ui.set_monitor_open(false);
    h.settle();
    ui.invoke_tab_changed(1);
    ui.invoke_open_learning(0);
    assert!(ui.get_learning_visible());
    h.settle();
    assert!(
        ui.get_learning_visible(),
        "older deferred Workbench intent must not dismiss newer Start here"
    );
    assert!(
        !preference.seen().unwrap(),
        "no stale intent may remember a departure"
    );
    ui.invoke_tab_changed(0);
    h.settle();
    assert!(
        preference.seen().unwrap(),
        "explicit work entry still remembers departure"
    );
}

#[test]
fn navigation_intents_apply_once_in_order_and_projection_never_departs_learning() {
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    for view in [0, 1] {
        ui.invoke_tab_changed(view);
        ui.invoke_open_learning(0);
        let revision = h.adapter.inspect(|w| w.revision());
        h.settle();
        assert!(ui.get_learning_visible());
        assert_eq!(
            h.adapter.inspect(|w| w.revision()),
            revision,
            "settling a pure projection emits no departure or stale navigation"
        );
        // Changing a projected field is not a manual activation.
        ui.set_tab_index(1 - view);
        h.adapter.tick();
        h.settle();
        assert!(ui.get_learning_visible());
        assert_eq!(h.adapter.inspect(|w| w.revision()), revision);
        ui.invoke_tab_changed(view);
        assert!(
            !ui.get_learning_visible(),
            "explicit work entry applies synchronously"
        );
        assert_eq!(
            h.adapter.inspect(|w| w.view()),
            if view == 0 {
                View::Library
            } else {
                View::Workbench
            }
        );
        let departed_revision = h.adapter.inspect(|w| w.revision());
        h.settle();
        assert!(!ui.get_learning_visible());
        assert_eq!(
            h.adapter.inspect(|w| w.revision()),
            departed_revision,
            "one activation has no duplicate callback after projection"
        );
    }
    ui.invoke_open_learning(0);
    ui.invoke_tab_changed(1);
    ui.invoke_tab_changed(0);
    h.settle();
    assert_eq!(h.adapter.inspect(|w| w.view()), View::Library);
    assert!(!ui.get_learning_visible());
    ui.invoke_open_learning(0);
    ui.invoke_tab_changed(-1);
    ui.invoke_tab_changed(2);
    h.settle();
    assert!(
        ui.get_learning_visible(),
        "invalid working targets cannot mark onboarding seen"
    );
}

#[test]
fn learning_construction_picker_projects_real_sound_and_envelope_without_document_actions() {
    use auditorylab_app::learning::{Construction, Topic};
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    let document = ui.get_document();
    ui.invoke_open_learning(i32::try_from(Topic::PitchAndRhythm.index()).unwrap());
    h.settle();
    let pitch = ui.get_learning_traces();
    assert!(pitch.row_count() >= 2);
    assert!(pitch.row_data(0).unwrap().label.contains("10 ms"));
    assert!(pitch.iter().all(|lane| !lane.trace.is_empty()));
    ui.invoke_open_learning(i32::try_from(Topic::SoundTypes.index()).unwrap());
    h.settle();
    for (construction, title) in Construction::ALL.into_iter().zip([
        "Steady tone",
        "Pulsed tone (isochronic)",
        "Sinusoidal amplitude modulation",
        "Binaural pair",
        "Monaural pair",
        "Pulse train",
    ]) {
        ui.invoke_select_learning_construction(i32::try_from(construction.index()).unwrap());
        h.settle();
        assert_eq!(selected_construction(ui).as_deref(), Some(title));
        let traces = ui.get_learning_traces();
        assert!(traces.iter().all(|lane| !lane.trace.is_empty()));
        let waveforms = traces
            .iter()
            .filter(|lane| !lane.unipolar)
            .collect::<Vec<_>>();
        assert_eq!(waveforms.len(), 1, "stereo is shown in one plot");
        if construction == Construction::BinauralPair {
            assert!(waveforms[0].label.contains("Left") && waveforms[0].label.contains("Right"));
            assert!(!waveforms[0].overlay.is_empty(), "the right channel is drawn over the left");
            assert_ne!(waveforms[0].trace, waveforms[0].overlay);
        } else {
            assert!(waveforms[0].label.contains("Both channels"));
            assert!(waveforms[0].overlay.is_empty());
        }
        if matches!(
            construction,
            Construction::MonauralPair | Construction::BinauralPair
        ) {
            assert!(
                traces.iter().all(|lane| !lane.unipolar),
                "individual constant tone gain is not a combined pair envelope"
            );
        } else {
            assert!(traces.iter().any(|lane| lane.unipolar));
        }
        if construction == Construction::PulseTrain {
            assert!(
                traces
                    .iter()
                    .any(|lane| lane.label.contains("When each source pulse is on"))
            );
        }
        assert!(!ui.get_learning_example_summary().is_empty());
        assert_eq!(
            h.adapter.inspect(|w| w.learning().unwrap().construction),
            construction
        );
        assert_eq!(ui.get_document(), document);
        assert!(!h.adapter.inspect(|w| w.audio_busy() || w.file_busy()));
        assert_eq!(h.take().len(), 0);
    }
    ui.invoke_open_learning(i32::try_from(Topic::Overview.index()).unwrap());
    ui.invoke_open_learning(i32::try_from(Topic::SoundTypes.index()).unwrap());
    h.settle();
    assert_eq!(selected_construction(ui).as_deref(), Some("Pulse train"));
}

#[test]
fn onboarding_publication_notice_uses_shared_prose_and_does_not_block_work() {
    let h = Harness::new();
    let ui = h.ui();
    let directory = h.library.as_ref().unwrap().0.path().join("preferences");
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("onboarding-seen");
    std::fs::write(&path, [0u8; 129]).unwrap();
    h.adapter.set_onboarding_preference(Some(
        auditorylab_app::onboarding::Preference::at_config_directory(directory),
    ));
    ui.invoke_open_learning(
        i32::try_from(auditorylab_app::learning::Topic::Overview.index()).unwrap(),
    );
    ui.invoke_browse_learning_library();
    h.settle();
    assert!(!ui.get_learning_visible());
    assert_eq!(h.adapter.inspect(|w| w.view()), View::Library);
    let notice = ui.get_onboarding_notice();
    assert!(notice.contains(path.to_str().unwrap()), "{notice}");
    assert!(
        notice.contains("exceeds the bounded read limit"),
        "{notice}"
    );
    assert!(notice.contains("continue working"), "{notice}");
    assert!(!notice.contains("file.read_limit"), "{notice}");
    assert!(labelled(ui, "Start here preference needs attention.").is_visible());
    ui.invoke_show_onboarding_notice();
    h.settle();
    assert_eq!(
        text_of(&field(
            ui,
            "Full preference notice · Path, cause and recovery"
        )),
        notice.as_str()
    );
    h.key(Key::Escape);
    assert_eq!(std::fs::read(path).unwrap(), [0u8; 129]);
    assert!(!h.adapter.inspect(|w| w.audio_busy() || w.file_busy()));
}

#[test]
fn lesson_switch_resets_real_scroll_but_background_projection_and_resume_preserve_it() {
    use auditorylab_app::workspace::OutputCommand;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.window().set_size(slint::LogicalSize::new(680.0, 600.0));
    ui.invoke_open_learning(4);
    h.settle();
    ui.set_learning_scroll_y(-120.0);
    h.settle();
    assert!(
        ui.get_learning_scroll_y() < -1.0,
        "exercise an actually scrolled lesson"
    );
    ui.invoke_open_learning(3);
    h.settle();
    assert_eq!(
        ui.get_learning_scroll_y(),
        0.0,
        "new topic begins at its heading"
    );
    ui.set_learning_scroll_y(-120.0);
    h.settle();
    let y = ui.get_learning_scroll_y();
    assert!(y < -1.0);
    let topics = ui.get_learning_topics();
    let bands = ui.get_learning_band_items();
    let choices = ui.get_learning_constructions();
    let traces = ui.get_learning_traces();
    ui.invoke_resume_learning();
    h.adapter
        .command(|w| w.apply(Command::Output(OutputCommand::SetHold(true))));
    h.settle();
    assert_eq!(ui.get_learning_scroll_y(), y);
    assert!(
        ui.get_learning_topics() == topics,
        "topic model is retained"
    );
    assert!(
        ui.get_learning_band_items() == bands,
        "native diagram source is retained"
    );
    assert!(
        ui.get_learning_constructions() == choices,
        "picker model is retained"
    );
    assert!(
        ui.get_learning_traces() == traces,
        "unrelated updates do not rebuild plots"
    );
}

#[test]
fn selected_learning_example_action_opens_that_construction_silently() {
    use auditorylab_app::learning::Construction;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    for construction in Construction::ALL {
        ui.invoke_open_learning(3);
        ui.invoke_select_learning_construction(i32::try_from(construction.index()).unwrap());
        h.settle();
        assert_eq!(
            ui.get_learning_example_action(),
            "Open this example in Workbench"
        );
        ui.invoke_create_learning_example();
        h.settle();
        let requests = h.take();
        if let [Request::Unsaved(prompt)] = requests.as_slice() {
            h.adapter.respond(*prompt, PromptAnswer::Discard);
            h.settle();
        } else {
            assert!(
                requests.is_empty(),
                "only an unsaved-document decision is expected"
            );
        }
        assert_eq!(
            h.adapter
                .inspect(|w| w.editor().draft().components()[0].form),
            construction.form()
        );
        assert!(!ui.get_learning_visible());
        assert!(!h.adapter.inspect(|w| w.audio_busy() || w.file_busy()));
        let remaining = h.take();
        assert!(
            remaining.is_empty(),
            "unexpected follow-up requests: {remaining:?}"
        );
    }
}

#[test]
fn empty_workbench_first_action_opens_real_component_creation() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let h = Harness::new();
    h.workbench();
    let ui = h.ui();
    assert_eq!(ui.get_components().row_count(), 0);
    assert!(labelled(ui, "Build a sound").is_visible());
    action_button(ui, "Add steady tone").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert_eq!(ui.get_components().row_count(), 1);
    assert_eq!(
        ui.get_components().row_data(0).unwrap().heading,
        "Steady tone"
    );
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn library_export_actions_are_visible_and_open_the_real_path_flow() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    let key = ui.get_library_key();
    action_button(ui, "Export WAV…").accessible_action(&AccessibilityAction::Default);
    h.settle();
    let [Request::Path(prompt, PathPurpose::ExportWav, _)] = h.take()[..] else {
        panic!("the native export action must open its real path flow");
    };
    h.adapter.respond(prompt, PromptAnswer::Cancel);
    h.settle();
    assert_eq!(ui.get_library_key(), key);
    assert!(!h.adapter.inspect(|w| w.audio_busy() || w.file_busy()));
    assert!(action_button(ui, "Export portable recipe…").is_visible());
}

#[test]
fn minimum_window_keeps_a_useful_lesson_viewport_with_large_text_and_active_status() {
    let h = Harness::new();
    let ui = h.ui();
    ui.window().set_size(slint::LogicalSize::new(680.0, 600.0));
    ui.set_ui_size(26.0);
    for monitor_open in [false, true] {
        ui.invoke_tab_changed(0);
        ui.set_monitor_open(monitor_open);
        h.settle();
        ui.invoke_open_learning(4);
        h.settle();
        // Renderer geometry inputs: no device is opened or playback claimed.
        present_active_status_for_layout(ui);
        h.settle();
        let reader = labelled(ui, "Lesson content");
        let geometry = reader.geometry();
        assert!(
            geometry.size.width >= 300.0,
            "reading column remains usable (monitor={monitor_open}): {geometry:?}"
        );
        assert!(
            geometry.size.height >= 150.0,
            "reading viewport remains usable (monitor={monitor_open}): {geometry:?}"
        );
        let top = centre(&reader).y - geometry.size.height / 2.0;
        let bottom = top + geometry.size.height;
        ui.set_learning_scroll_y(-120.0);
        h.settle();
        ui.invoke_open_learning(3);
        present_active_status_for_layout(ui);
        h.settle();
        let heading = labelled(ui, "Sound constructions");
        let heading_y = centre(&heading).y - heading.geometry().size.height / 2.0;
        assert!(
            heading_y >= top - 1.0 && heading_y < bottom,
            "new heading starts inside reader: heading={heading_y}, reader={top}..{bottom}"
        );
        assert!(labelled(ui, "Playback transport").is_visible());
        let mut controls = vec!["Stop audio", "Cancel export"];
        if monitor_open {
            controls.push("Hide output monitor");
            assert!(
                !ui.get_drawer_fits(),
                "minimum window uses the separate monitor"
            );
        }
        for name in controls {
            let item = action_button(ui, name);
            let center = centre(&item);
            let size = item.geometry().size;
            let left = center.x - size.width / 2.0;
            let right = center.x + size.width / 2.0;
            let top = center.y - size.height / 2.0;
            let bottom = center.y + size.height / 2.0;
            assert!(
                left >= 0.0 && right <= 680.0 && top >= 0.0 && bottom <= 600.0,
                "essential control stays in window: {name} at {left},{top}..{right},{bottom}"
            );
        }
    }
}

#[test]
fn guide_navigation_never_overlays_the_lesson_reader() {
    use auditorylab_app::learning::Topic;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    // Float32 coordinate transforms can differ by a few ten-thousandths of a pixel.
    let epsilon = 0.01;
    let bounds = |item: &ItemRc| {
        let size = item.geometry().size;
        let center = centre(item);
        (
            center.x - size.width / 2.0,
            center.y - size.height / 2.0,
            center.x + size.width / 2.0,
            center.y + size.height / 2.0,
        )
    };
    for (width, height) in [(1280.0, 900.0), (680.0, 600.0), (2000.0, 600.0)] {
        ui.window().set_size(slint::LogicalSize::new(width, height));
        for text in [13.0, 26.0] {
            ui.set_ui_size(text);
            ui.invoke_open_learning(4);
            h.settle();
            let nav = bounds(&labelled(ui, "Learning topics"));
            let reader = bounds(&labelled(ui, "Lesson content"));
            assert!(reader.2 > reader.0 && reader.3 > reader.1);
            if ui.get_learning_compact() {
                assert!(
                    nav.3 <= reader.1 + epsilon,
                    "compact topics end before reader: nav={nav:?}, reader={reader:?}"
                );
            } else {
                assert!(
                    nav.2 <= reader.0 + epsilon,
                    "wide topics end beside reader: nav={nav:?}, reader={reader:?}"
                );
            }
            for topic in Topic::ALL {
                let label = auditorylab_presentation::learning::navigation_label(topic);
                let index = topic.index().to_string();
                let item = find_control(ui, |item| {
                    item.is_visible()
                        && item
                            .accessible_string_property(AccessibleStringProperty::Label)
                            .as_deref()
                            == Some(label)
                        && item
                            .accessible_string_property(AccessibleStringProperty::ItemIndex)
                            .as_deref()
                            == Some(index.as_str())
                })
                .unwrap_or_else(|| panic!("visible topic tab {label}"));
                let tab = bounds(&item);
                assert!(
                    tab.0 + epsilon >= nav.0
                        && tab.1 + epsilon >= nav.1
                        && tab.2 <= nav.2 + epsilon
                        && tab.3 <= nav.3 + epsilon,
                    "entire topic fits navigation: {label}, tab={tab:?}, navigation={nav:?}"
                );
                assert!(
                    tab.0 >= -epsilon
                        && tab.1 >= -epsilon
                        && tab.2 <= width + epsilon
                        && tab.3 <= height + epsilon
                );
            }
            let workspace = bounds(&labelled(ui, "Workspace navigation"));
            assert!(workspace.0 >= -epsilon && workspace.1 >= -epsilon);
            assert!(workspace.2 <= width + epsilon && workspace.3 <= height + epsilon);
            assert!(
                workspace.3 <= nav.1 + epsilon && workspace.3 <= reader.1 + epsilon,
                "explicit workspace exits stay above the guide content"
            );
        }
    }
}

fn activate_focused_learning_example(h: &Harness) -> ItemRc {
    let ui = h.ui();
    let action = action_button(ui, "Open this example in Workbench");
    let window = WindowInner::from_pub(ui.window());
    window.set_focus_item(&action, true, FocusReason::Programmatic);
    h.settle();
    let focused = window
        .focus_item
        .borrow()
        .upgrade()
        .expect("button forwards keyboard focus");
    assert!(
        action
            .visit_descendants(|item| {
                if item == &focused {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .is_some(),
        "focused item belongs to the actual example button"
    );
    assert!(focused.is_visible());
    h.key(Key::Return);
    focused
}

fn assert_workbench_title_has_focus(h: &Harness) {
    let ui = h.ui();
    assert!(!ui.get_learning_visible());
    let title = field(ui, "Sound title");
    let focus = WindowInner::from_pub(ui.window())
        .focus_item
        .borrow()
        .upgrade();
    assert_eq!(
        focus,
        Some(title),
        "successful handoff focuses the actual title input"
    );
    assert!(focus.unwrap().is_visible());
}

#[test]
fn clean_learning_example_handoff_focuses_the_actual_workbench_title() {
    let h = Harness::new();
    h.ui().set_monitor_open(false);
    h.ui().invoke_open_learning(3);
    h.settle();
    activate_focused_learning_example(&h);
    assert_workbench_title_has_focus(&h);
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
}

#[test]
fn dirty_example_cancel_retains_guide_focus_then_discard_focuses_workbench() {
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    h.workbench();
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "Retain this draft".into(),
    );
    let original = ui.get_document();
    ui.invoke_open_learning(3);
    h.settle();
    let create = activate_focused_learning_example(&h);
    let [Request::Unsaved(prompt)] = h.take()[..] else {
        panic!("protect dirty work");
    };
    assert_eq!(ui.get_document(), original);
    assert!(ui.get_learning_visible());
    assert_eq!(
        WindowInner::from_pub(ui.window())
            .focus_item
            .borrow()
            .upgrade(),
        Some(create.clone())
    );
    h.adapter.respond(prompt, PromptAnswer::Cancel);
    h.settle();
    assert_eq!(ui.get_document(), original);
    assert!(ui.get_learning_visible());
    assert_eq!(
        WindowInner::from_pub(ui.window())
            .focus_item
            .borrow()
            .upgrade(),
        Some(create)
    );
    activate_focused_learning_example(&h);
    let [Request::Unsaved(prompt)] = h.take()[..] else {
        panic!("protect dirty work again");
    };
    h.adapter.respond(prompt, PromptAnswer::Discard);
    h.settle();
    assert_ne!(ui.get_document(), original);
    assert_workbench_title_has_focus(&h);
}

#[test]
fn dirty_example_save_focuses_workbench_only_after_the_real_save_completes() {
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    h.workbench();
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "Save this draft first".into(),
    );
    let original = ui.get_document();
    ui.invoke_open_learning(3);
    h.settle();
    let create = activate_focused_learning_example(&h);
    let [Request::Unsaved(prompt)] = h.take()[..] else {
        panic!("protect dirty work");
    };
    h.adapter.respond(prompt, PromptAnswer::Save);
    let save = take_job(&h, JobKind::File);
    assert_eq!(ui.get_document(), original);
    assert!(ui.get_learning_visible());
    assert_eq!(
        WindowInner::from_pub(ui.window())
            .focus_item
            .borrow()
            .upgrade(),
        Some(create)
    );
    finish_file(&h, save);
    h.wait_for_files();
    h.settle();
    assert_ne!(ui.get_document(), original);
    assert_workbench_title_has_focus(&h);
    assert_eq!(
        h.store()
            .scan(&auditorylab_app::job::Cancel::default())
            .entries
            .len(),
        1
    );
}

#[test]
fn failed_real_save_keeps_the_guide_document_and_visible_example_focus() {
    use auditorylab_app::services::library::Mutation;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    h.workbench();
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "Initial saved sound".into(),
    );
    ui.invoke_save_recipe();
    h.wait_for_files();
    let saved = h
        .store()
        .scan(&auditorylab_app::job::Cancel::default())
        .entries
        .remove(0);
    ui.invoke_commit_metadata(
        metadata_field_handle(&ui.get_document(), MetadataField::Title),
        "Unsaved changes".into(),
    );
    let original = ui.get_document();
    ui.invoke_open_learning(3);
    h.settle();
    let create = activate_focused_learning_example(&h);
    let [Request::Unsaved(prompt)] = h.take()[..] else {
        panic!("protect dirty work");
    };
    h.adapter.respond(prompt, PromptAnswer::Save);
    let save = take_job(&h, JobKind::File);
    h.store()
        .apply(
            Mutation::Rename {
                key: saved.key.clone(),
                expect: saved.version_token,
                title: "Another writer".into(),
            },
            &auditorylab_app::job::Cancel::default(),
        )
        .unwrap();
    finish_file(&h, save);
    h.wait_for_files();
    h.settle();
    assert_eq!(ui.get_document(), original);
    assert!(ui.get_learning_visible() && ui.get_dirty());
    assert_eq!(
        WindowInner::from_pub(ui.window())
            .focus_item
            .borrow()
            .upgrade(),
        Some(create)
    );
    assert_eq!(
        h.store().load(&saved.key).unwrap().snapshot.title(),
        "Another writer"
    );
    let summary = ui.get_operation_summary();
    let details = ui.get_status();
    assert!(
        summary.contains("Saved version or identity conflicts with the captured operation")
            && summary.contains(&saved.key.to_string()),
        "current failure summary: {summary}; details: {details}"
    );
    assert!(
        details.contains("saved recipe changed or disappeared"),
        "current failure summary: {summary}; details: {details}"
    );
    assert!(
        details.contains(&saved.key.to_string()),
        "current failure summary: {summary}; details: {details}"
    );
    assert!(
        details.contains("retain the draft and reload or save a copy"),
        "current failure summary: {summary}; details: {details}"
    );
}

#[test]
fn preference_notice_keeps_guide_space_and_preserves_accessible_full_recovery_details() {
    use auditorylab_app::onboarding::Problem;
    use auditorylab_app::services::IoOperation;
    use auditorylab_presentation::Explain;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.set_ui_size(26.0);
    let path = std::path::PathBuf::from(format!(
        "/Users/example/{}/Application Support/org.resoltico.auditorylab/onboarding-seen",
        "a-long-but-supported-profile-name/".repeat(8)
    ));
    let blocked = h
        .library
        .as_ref()
        .unwrap()
        .0
        .path()
        .join("blocked-preference-config");
    std::fs::write(&blocked, b"retain existing content").unwrap();
    let preference = auditorylab_app::onboarding::Preference::at_config_directory(blocked.clone());
    let read_problem = preference.seen().unwrap_err();
    let write_problem = preference.mark_seen().unwrap_err();
    assert!(matches!(
        read_problem,
        Problem::File {
            operation: IoOperation::OpenDirectory,
            ..
        }
    ));
    assert!(matches!(
        write_problem,
        Problem::File {
            operation: IoOperation::CreateDirectory,
            ..
        }
    ));
    let problems = [
        (Problem::Unsupported { path: path.clone() }, path),
        (read_problem, blocked.clone()),
        (write_problem, blocked.clone()),
    ];
    for (width, height) in [(680.0, 600.0), (2000.0, 600.0)] {
        ui.window().set_size(slint::LogicalSize::new(width, height));
        ui.invoke_open_learning(4);
        h.settle();
        for (problem, expected_path) in &problems {
            // Shared production prose, with explicit renderer state rather than
            // pretending to change OS permissions or run an audio device.
            let notice = problem.explain();
            ui.set_onboarding_notice(notice.clone().into());
            present_active_status_for_layout(ui);
            h.settle();
            let reader = labelled(ui, "Lesson content");
            assert!(
                reader.geometry().size.height >= 150.0,
                "actionable error must leave the lesson usable: {:?}",
                reader.geometry()
            );
            let full = exercise_preference_notice_popup(&h);
            assert_eq!(full, notice);
            assert!(full.contains(expected_path.to_str().unwrap()));
            for label in ["Stop audio", "Cancel export"] {
                let item = action_button(ui, label);
                let center = centre(&item);
                let size = item.geometry().size;
                assert!(
                    center.x - size.width / 2.0 >= 0.0
                        && center.x + size.width / 2.0 <= width
                        && center.y - size.height / 2.0 >= 0.0
                        && center.y + size.height / 2.0 <= height,
                    "notice leaves full essential action inside the window: {label}"
                );
            }
        }
    }
    assert_eq!(std::fs::read(blocked).unwrap(), b"retain existing content");
}

fn exercise_preference_notice_popup(h: &Harness) -> String {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let ui = h.ui();
    let create = action_button(ui, "Start here preference notice · Details");
    WindowInner::from_pub(h.ui().window()).set_focus_item(&create, true, FocusReason::Programmatic);
    h.settle();
    let opener_focus = WindowInner::from_pub(h.ui().window())
        .focus_item
        .borrow()
        .upgrade()
        .expect("notice button forwards focus");
    assert!(
        create
            .visit_descendants(|item| {
                if item == &opener_focus {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .is_some(),
        "focus belongs to the actual notice opener"
    );
    create.accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert!(ui.get_onboarding_notice_open());
    assert!(active_popup_action(ui, "Stop audio").is_visible());
    assert!(active_popup_action(ui, "Cancel export").is_visible());
    let full = text_of(&field(
        ui,
        "Full preference notice · Path, cause and recovery",
    ));
    h.key(Key::Escape);
    assert!(!ui.get_onboarding_notice_open());
    assert_eq!(
        WindowInner::from_pub(h.ui().window())
            .focus_item
            .borrow()
            .upgrade(),
        Some(opener_focus.clone())
    );
    create.accessible_action(&AccessibilityAction::Default);
    h.settle();
    action_button(ui, "Close").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert!(!ui.get_onboarding_notice_open());
    assert_eq!(ui.get_onboarding_notice(), full.as_str());
    assert_eq!(
        WindowInner::from_pub(h.ui().window())
            .focus_item
            .borrow()
            .upgrade(),
        Some(opener_focus)
    );
    full
}

#[test]
fn guide_sources_are_named_and_separate_from_application_documentation() {
    use auditorylab_app::learning::LearningSource;
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    ui.invoke_open_learning(4);
    h.settle();
    assert_eq!(ui.get_learning_sources().row_count(), 3);
    for label in [
        "IFCN EEG glossary",
        "Memory study (PubMed)",
        "NIH sleep stages",
        "Desktop guide",
    ] {
        assert!(reveal_lesson_control(&h, label).is_visible());
    }
    // The source targets are typed application facts, independent of the app's
    // published revision. Do not launch external browsers from an offline test.
    assert_eq!(
        LearningSource::MemoryStudy.url(),
        "https://pubmed.ncbi.nlm.nih.gov/16837600/"
    );
}

fn active_popup_action(ui: &MainWindow, label: &str) -> ItemRc {
    let window = WindowInner::from_pub(ui.window());
    window
        .active_popups()
        .iter()
        .rev()
        .find_map(|popup| {
            let root = ItemRc::new(popup.component.clone(), 0);
            root.visit_descendants(|item| {
                if item.is_visible()
                    && item.is_accessible()
                    && item
                        .accessible_string_property(AccessibleStringProperty::Label)
                        .as_deref()
                        == Some(label)
                {
                    ControlFlow::Break(item.clone())
                } else {
                    ControlFlow::Continue(())
                }
            })
        })
        .unwrap_or_else(|| panic!("action inside the active popup: {label}"))
}

#[test]
fn preference_popup_stop_targets_audio_and_a_late_stop_never_starts_the_workbench() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let h = Harness::with_catalog(
        auditorylab_app::services::catalog().unwrap(),
        Capabilities::available(),
    );
    let ui = h.ui();
    ui.set_monitor_open(false);
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    ui.invoke_play_stop();
    let audio = take_job(&h, JobKind::Audio);
    assert!(h.adapter.inspect(|w| w.audio_busy()));
    ui.invoke_open_learning(0);
    ui.set_onboarding_notice("Preference recovery details".into());
    ui.invoke_show_onboarding_notice();
    h.settle();
    let stop = active_popup_action(ui, "Stop audio");
    stop.accessible_action(&AccessibilityAction::Default);
    assert_eq!(
        h.adapter.inspect(|w| w.audio_phase()),
        Some(auditorylab_app::services::playback::PlaybackPhase::Stopping),
        "the popup action itself must request Stop before the job runs"
    );
    // No second Stop helper: a broken popup binding must fail above, before any
    // device attempt. The actual stopped job exits before preparation.
    let finished = audio.execute(|_| panic!("popup-stopped audio must not prepare a device"));
    h.adapter.deliver(finished);
    h.settle();
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    ui.invoke_tab_changed(1);
    h.settle();
    assert!(
        ui.get_can_play(),
        "exercise a valid Workbench target after completion"
    );
    assert_eq!(h.adapter.inspect(|w| w.audio_phase()), None);
    assert!(
        ui.get_onboarding_notice_open(),
        "exercise the still-open popup"
    );
    // Restore only the obsolete offered Stop state. The actual workspace is
    // idle with a playable target, so a Play/Stop toggle would start new audio.
    ui.set_playback_busy(true);
    ui.global::<crate::generated::Actions>()
        .set_stop(crate::generated::Action {
            label: auditorylab_presentation::workspace::operation_label(
                auditorylab_app::vocabulary::Operation::Stop,
            )
            .into(),
            enabled: true,
        });
    h.settle();
    let stale_stop = active_popup_action(ui, "Stop audio");
    assert_eq!(stale_stop, stop, "the same popup action remains offered");
    assert_eq!(h.adapter.inspect(|w| w.audio_phase()), None);
    stale_stop.accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert!(!h.adapter.inspect(|w| w.audio_busy()));
    let jobs = h.adapter.take_jobs();
    assert!(
        jobs.is_empty(),
        "a stale Stop must not create new work: {jobs:?}"
    );
}

/// The label of the checked choice in the visible construction group.
fn selected_construction(ui: &MainWindow) -> Option<String> {
    let group = find_control(ui, |item| {
        item.accessible_string_property(AccessibleStringProperty::Label)
            .as_deref()
            == Some("Sound construction")
    })
    .expect("the construction choices form one group");
    let mut checked = Vec::new();
    let _ = group.visit_descendants(|item| {
        if item.is_visible()
            && item
                .accessible_string_property(AccessibleStringProperty::Checked)
                .as_deref()
                == Some("true")
        {
            checked.extend(
                item.accessible_string_property(AccessibleStringProperty::Label)
                    .map(|label| label.to_string()),
            );
        }
        ControlFlow::<()>::Continue(())
    });
    assert!(checked.len() <= 1, "one construction is checked: {checked:?}");
    checked.pop()
}

fn reveal_lesson_control(h: &Harness, label: &str) -> ItemRc {
    for _ in 0..8 {
        let item = find_control(h.ui(), |item| {
            item.accessible_string_property(AccessibleStringProperty::Label)
                .as_deref()
                == Some(label)
        })
        .unwrap_or_else(|| panic!("lesson control exists: {label}"));
        let reader = labelled(h.ui(), "Lesson content");
        let center = centre(&item);
        let viewport = centre(&reader);
        let height = reader.geometry().size.height;
        let item_height = item.geometry().size.height;
        if item.is_visible()
            && center.y - item_height / 2.0 >= viewport.y - height / 2.0
            && center.y + item_height / 2.0 <= viewport.y + height / 2.0
        {
            return item;
        }
        h.ui()
            .window()
            .dispatch_event(WindowEvent::PointerScrolled {
                position: viewport,
                delta_x: 0.0,
                delta_y: viewport.y - center.y,
            });
        h.settle();
    }
    panic!("lesson control can be revealed through its real scroll view: {label}");
}

#[test]
fn preference_popup_cancel_is_the_only_cancellation_of_a_real_captured_export() {
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    // The fixture owns a 10 ms silent recipe: even the no-cancel negative case
    // would remain bounded, and no audio device is involved.
    let (h, directory) = controlled_harness();
    let ui = h.ui();
    ui.set_monitor_open(false);
    let path = directory.path().join("sound.wav");
    ui.invoke_export_wav();
    answer_path(&h, &path);
    let export = take_job(&h, JobKind::File);
    assert!(h.adapter.inspect(|w| w.file_busy()));
    ui.invoke_open_learning(0);
    ui.set_onboarding_notice("Preference recovery details".into());
    ui.invoke_show_onboarding_notice();
    h.settle();
    active_popup_action(ui, "Cancel export").accessible_action(&AccessibilityAction::Default);
    let finished = export.execute(|_| panic!("a file export cannot prepare audio"));
    h.adapter.deliver(finished);
    h.wait_for_files();
    h.settle();
    assert!(!path.exists(), "cancelled export must not publish output");
    assert!(!h.adapter.inspect(|w| w.file_busy() || w.audio_busy()));
    let summary = ui.get_operation_summary();
    let details = ui.get_status();
    assert!(
        summary.contains("Export cancelled; no file was written."),
        "popup cancellation summary: {summary}; details: {details}"
    );
}

#[test]
fn learning_intro_follows_its_heading_without_space_for_hidden_controls() {
    use auditorylab_app::learning::Topic;
    use slint::private_unstable_api::re_exports::AccessibleRole;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    for (width, height) in [(1280.0, 900.0), (680.0, 600.0)] {
        ui.window().set_size(slint::LogicalSize::new(width, height));
        for text_size in [13.0, 26.0] {
            ui.set_ui_size(text_size);
            for topic in [Topic::Overview, Topic::PitchAndRhythm] {
                ui.invoke_open_learning(i32::try_from(topic.index()).unwrap());
                h.settle();
                let reader = labelled(ui, "Lesson content");
                let reader_center = centre(&reader);
                let reader_height = reader.geometry().size.height;
                let text = |label: &str| {
                    reader
                        .visit_descendants(|item| {
                            if item.is_accessible()
                                && item.accessible_role() == AccessibleRole::Text
                                && item
                                    .accessible_string_property(AccessibleStringProperty::ItemIndex)
                                    .is_none()
                                && item
                                    .accessible_string_property(AccessibleStringProperty::Label)
                                    .as_deref()
                                    == Some(label)
                            {
                                ControlFlow::Break(item.clone())
                            } else {
                                ControlFlow::Continue(())
                            }
                        })
                        .unwrap_or_else(|| panic!("exact text inside the lesson reader: {label}"))
                };
                let heading = text(auditorylab_presentation::learning::heading(topic));
                let body = text(auditorylab_presentation::learning::body(topic));
                let heading_bottom = centre(&heading).y + heading.geometry().size.height / 2.0;
                let body_top = centre(&body).y - body.geometry().size.height / 2.0;
                let gap = body_top - heading_bottom;
                assert!(
                    (-0.01..=32.0).contains(&gap),
                    "intro follows heading without hidden-control space: topic={topic:?}, \
                    size={width}x{height}, text={text_size}, gap={gap}"
                );
                assert!(
                    body_top < reader_center.y + reader_height / 2.0,
                    "the introduction starts inside the first viewport"
                );
            }
        }
    }
}

#[test]
fn repaired_preference_clears_its_old_notice_and_open_popup_on_successful_departure() {
    use auditorylab_app::onboarding::Preference;
    use auditorylab_presentation::Explain;
    use slint::private_unstable_api::re_exports::AccessibilityAction;
    let h = Harness::new();
    let ui = h.ui();
    ui.set_monitor_open(false);
    let directory = h.library.as_ref().unwrap().0.path().join("preferences");
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("onboarding-seen");
    std::fs::write(&path, b"unsupported preference").unwrap();
    let preference = Preference::at_config_directory(directory.clone());
    let notice = preference.seen().unwrap_err().explain();
    ui.set_onboarding_notice(notice.clone().into());
    h.adapter
        .set_onboarding_preference(Some(Preference::at_config_directory(directory)));
    ui.invoke_open_learning(4);
    h.settle();
    ui.invoke_show_onboarding_notice();
    h.settle();
    assert!(ui.get_onboarding_notice_open());
    assert_eq!(ui.get_onboarding_notice(), notice.as_str());
    assert_eq!(std::fs::read(&path).unwrap(), b"unsupported preference");
    // Repair only the owned temporary flag before the first deliberate exit.
    std::fs::remove_file(path).unwrap();
    action_button(ui, "Workbench").accessible_action(&AccessibilityAction::Default);
    h.settle();
    assert_eq!(h.adapter.inspect(|w| w.view()), View::Workbench);
    assert!(!ui.get_learning_visible());
    assert!(
        preference.seen().unwrap(),
        "the real departure successfully remembered orientation"
    );
    assert_eq!(
        ui.get_onboarding_notice(),
        "",
        "the old read failure is no longer current"
    );
    assert!(
        !ui.get_onboarding_notice_open(),
        "obsolete popup closes with its resolved notice"
    );
    let focus = WindowInner::from_pub(ui.window())
        .focus_item
        .borrow()
        .upgrade()
        .expect("successful global exit retains a focus destination");
    assert!(focus.is_visible());
    let navigation = labelled(ui, "Workspace navigation");
    assert!(
        focus == navigation
            || navigation
                .visit_descendants(|item| {
                    if item == &focus {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                })
                .is_some(),
        "resolved-notice cleanup preserves the explicit workspace exit focus"
    );
}
