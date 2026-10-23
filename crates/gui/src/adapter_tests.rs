//! Production-binding integration tests: generated callbacks, wired by
//! [`SlintAdapter::install`] exactly as at startup, drive a real workspace.
//! Dialogs are answered through a recording shell; audio is the portable
//! workspace's typed refusal, so no device is touched.
use crate::adapter::{Shell, SlintAdapter, Windows, parameter_id, parameter_key};
use crate::generated::{MainWindow, MetadataKey, ParameterKey};
use crate::test_platform::{install, settle};
use auditorylab::workspace::{Capabilities, PathPurpose, PromptId, Response, View, Workspace};
use auditorylab::{EditingField, MetadataField, ParameterId};
use slint::platform::{Key, WindowEvent};
use slint::private_unstable_api::re_exports::{
    AccessibleStringProperty, FocusReason, ItemRc, TextInput, WindowInner,
};
use slint::{ComponentHandle, Model};
use std::{cell::RefCell, ops::ControlFlow, rc::Rc, time::Duration};

#[derive(Debug, Clone, PartialEq)]
enum Request {
    Unsaved(PromptId),
    Path(PromptId, PathPurpose, Option<String>),
    Close(bool),
}

struct Recorder(Rc<RefCell<Vec<Request>>>);

impl Shell for Recorder {
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

pub(crate) struct Harness {
    pub(crate) windows: Windows,
    pub(crate) adapter: Rc<SlintAdapter>,
    requests: Rc<RefCell<Vec<Request>>>,
}

impl Harness {
    pub(crate) fn new() -> Self {
        install();
        let windows = Windows::new().unwrap();
        let workspace =
            Workspace::new(auditorylab::catalog().unwrap(), Capabilities::portable()).unwrap();
        let requests = Rc::new(RefCell::new(Vec::new()));
        let adapter = SlintAdapter::install(
            &windows,
            workspace,
            Box::new(Recorder(requests.clone())),
            None,
        );
        windows.main.show().unwrap();
        windows
            .main
            .window()
            .set_size(slint::LogicalSize::new(1100.0, 900.0));
        windows
            .main
            .window()
            .dispatch_event(WindowEvent::WindowActiveChanged(true));
        settle(windows.main.window());
        Self {
            windows,
            adapter,
            requests,
        }
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

    fn component_binding(&self, index: usize) -> i32 {
        self.ui().get_components().row_data(index).unwrap().binding
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
        let (cid, _) = w.editor().draft().components()[component];
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
    let binding = h.component_binding(0);
    let frequency = h.adapter.inspect(parameter(0, ParameterId::ToneFrequency));

    ui.invoke_stage_parameter(binding, ParameterKey::ToneFrequency, "bad".into());
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

    ui.invoke_commit_parameter(binding, ParameterKey::ToneFrequency, "3750".into());
    assert_eq!(
        h.adapter.inspect(|w| w.editor().draft().value(frequency)),
        Some(3750.0)
    );
    assert!(ui.get_can_save());

    ui.invoke_stage_parameter(binding, ParameterKey::ToneFrequency, "12".into());
    ui.invoke_restore_parameter(binding, ParameterKey::ToneFrequency);
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

    ui.invoke_set_modulation(binding, 1);
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
    ui.invoke_set_ears(binding, 1);
    assert_eq!(ui.get_components().row_data(0).unwrap().ears, 1);
    ui.invoke_remove_component(binding);
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
    let [Request::Path(prompt, PathPurpose::SaveRecipe, _)] = h.take()[..] else {
        panic!("Save asks for a destination");
    };
    let saved = directory.path().join("probe.toml");
    h.adapter
        .respond(prompt, Response::Path(Some(saved.clone())));
    h.wait_for_files();
    assert!(saved.is_file());
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
        .respond(prompt, Response::Path(Some(exported.clone())));
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
    h.adapter.respond(prompt, Response::Cancel);
    assert_eq!(ui.get_document(), before, "Cancel keeps the document");
    ui.invoke_new_draft();
    let [Request::Unsaved(prompt)] = h.take()[..] else {
        panic!("asked again");
    };
    h.adapter.respond(prompt, Response::Discard);
    h.adapter.respond(prompt, Response::Save);
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
    let binding = h.component_binding(0);
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
            ui.invoke_set_carrier(binding, *carrier);
            ui.invoke_set_modulation(binding, *modulation);
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
                assert_eq!(shown.label, auditorylab::labeled(id));
                assert_eq!(shown.help, id.descriptor().help);
                assert_eq!(
                    shown.has_slider,
                    auditorylab::range(id, Workspace::export_rates()[1]).is_some()
                );
                ui.invoke_stage_parameter(binding, shown.key, "1x".into());
                let field = h.adapter.inspect(parameter(0, id));
                assert_eq!(
                    h.adapter
                        .inspect(|w| w.editor().staged(field).map(str::to_owned)),
                    Some("1x".to_owned()),
                    "{:?} stages its own field",
                    shown.key
                );
                ui.invoke_restore_parameter(binding, shown.key);
            }
        }
    }
    // A key the component does not have is refused, not redirected.
    ui.invoke_set_carrier(binding, 0);
    ui.invoke_set_modulation(binding, 0);
    ui.invoke_stage_parameter(binding, ParameterKey::GateRate, "5".into());
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
    let input = h.field("Click rhythm · layer 2: Click rate (Hz)");
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

    let first = h.component_binding(0);
    let third = h.component_binding(2);
    // Structural commands change other rows without committing staged text.
    ui.invoke_set_ears(first, 2);
    ui.invoke_set_modulation(third, 2);
    ui.invoke_remove_component(first);
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
        field(ui, "Click rhythm · layer 1: Click rate (Hz)"),
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
    let old_component = h.component_binding(0);
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
    h.adapter.respond(prompt, Response::Discard);
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
    ui.invoke_commit_parameter(old_component, ParameterKey::ToneFrequency, "999".into());
    ui.invoke_commit_metadata(old_document, MetadataKey::Title, "Old".into());
    ui.invoke_stage_metadata(old_document, MetadataKey::Summary, "Old".into());
    ui.invoke_add_component(old_document, 1);
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
    h.ui().invoke_select(1);
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
    let label = "Steady tone · layer 1: Tone frequency (Hz) slider";
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
    let binding = h.component_binding(0);
    ui.invoke_commit_parameter(binding, ParameterKey::ToneFrequency, "5000".into());
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
    let path = directory.path().join("imported.toml");
    std::fs::write(
        &path,
        "schema_version = 2\nduration_seconds = 1.0\nfade_seconds = 0.01\n\n[about]\ntitle = \"Imported probe\"\n",
    )
    .unwrap();
    ui.invoke_open_recipe();
    let [Request::Path(prompt, PathPurpose::OpenRecipe, _)] = h.take()[..] else {
        panic!("Open asks for a file");
    };
    h.adapter.respond(prompt, Response::Path(Some(path)));
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
    let second = h.adapter.inspect(|w| w.catalog()[1].title());
    let row = labelled(ui, &second);
    let centre = centre(&row);
    drag(ui, centre, centre);
    h.settle();
    assert_eq!(ui.get_selected(), 1);
    assert_eq!(h.adapter.inspect(|w| w.selected().unwrap().title()), second);
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
fn a_removed_layer_returns_through_undo_remove() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    ui.invoke_add_component(ui.get_document(), 1);
    h.settle();
    let first = h.adapter.inspect(|w| w.editor().draft().components()[0].0);
    assert_eq!(ui.get_removed_component(), "");
    ui.invoke_remove_component(h.component_binding(0));
    h.settle();
    assert_eq!(ui.get_components().row_count(), 1);
    assert_eq!(ui.get_removed_component(), "Removed Steady tone · layer 1.");
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
        h.adapter.inspect(|w| w.editor().draft().components()[0].0),
        first
    );
    assert_eq!(ui.get_removed_component(), "");
}

#[test]
fn layer_options_are_sized_to_their_content() {
    let h = Harness::new();
    let ui = h.ui();
    h.workbench();
    ui.invoke_add_component(ui.get_document(), 0);
    h.settle();
    let options = centre(&labelled(ui, "Routing, modulation and layer options"));
    drag(ui, options, options);
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
