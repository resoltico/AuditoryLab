//! The one binding between the Slint windows and the shared workspace.
//!
//! The adapter turns generated callbacks into workspace commands, performs the
//! workspace's effects through a [`Shell`], and projects workspace views into
//! persistent Slint models. Startup and the offscreen tests install exactly
//! this wiring through [`SlintAdapter::install`].
//!
//! Rendered fields carry a binding: an integer naming one document, or one
//! component of one document. A binding is never reused, so a callback that
//! arrives after its document was replaced still names the old document and
//! the workspace refuses it.
use crate::generated::{
    AboutWindow, AudioControlsWindow, ComponentRow, DetailsWindow, MainWindow, MetadataKey,
    MetadataRow, ModelLane, MonitorWindow, ParameterKey, ParameterRow,
};
use crate::{details_parent, motion, scope, typography};
use auditorylab::workspace::{
    ComponentView, EditorView, Effect, MetadataView, MotionPreference, ParameterView, PathPurpose,
    PromptId, Response, View, Workspace,
};
use auditorylab::{
    CarrierKind, ComponentId, DocumentId, Ears, EditingField, FieldHandle, MetadataField,
    ModelView, ModulationKind, ParameterId, PlaybackPhase, WaveformCapture, WaveformScale,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
    sync::Arc,
    time::Duration,
};

/// Waveform spans offered by the monitor, in milliseconds.
const SPANS_MS: [u32; 4] = [2, 10, 40, 100];
/// Waveform scales offered by the monitor, with their labels.
const SCALES: [(WaveformScale, &str); 2] = [
    (WaveformScale::Fixed, "Fixed ±1"),
    (WaveformScale::Fit, "Fit signal"),
];
/// Tick intervals while work is running and while idle.
const ACTIVE_TICK: Duration = Duration::from_millis(50);
const IDLE_TICK: Duration = Duration::from_millis(200);

/// What the workspace asks of the platform: dialogs, file panels and the
/// answer to a close request. Answers return through [`SlintAdapter::respond`].
pub(crate) trait Shell {
    /// Ask whether to save, discard or keep unsaved changes.
    fn ask_unsaved(&self, adapter: &Rc<SlintAdapter>, prompt: PromptId);
    /// Show a file panel.
    fn choose_path(
        &self,
        adapter: &Rc<SlintAdapter>,
        prompt: PromptId,
        purpose: PathPurpose,
        suggested_name: Option<String>,
    );
    /// The application may close now, or must stay open.
    fn close_resolved(&self, adapter: &SlintAdapter, approved: bool);
}

/// The windows the adapter drives. The caller keeps them alive.
pub(crate) struct Windows {
    pub(crate) main: MainWindow,
    pub(crate) about: AboutWindow,
    pub(crate) monitor: MonitorWindow,
    pub(crate) details: DetailsWindow,
    pub(crate) audio_controls: AudioControlsWindow,
}

impl Windows {
    pub(crate) fn new() -> Result<Self, slint::PlatformError> {
        Ok(Self {
            main: MainWindow::new()?,
            about: AboutWindow::new()?,
            monitor: MonitorWindow::new()?,
            details: DetailsWindow::new()?,
            audio_controls: AudioControlsWindow::new()?,
        })
    }
}

/// Native preference listeners. Tests run without them.
pub(crate) struct Preferences {
    typography: typography::TypographyService,
    font: typography::Typography,
    motion: motion::Service,
}

impl Preferences {
    pub(crate) fn start() -> Self {
        Self {
            typography: typography::TypographyService::start(),
            font: typography::resolve(),
            motion: motion::Service::start(),
        }
    }

    fn shutdown(&mut self) {
        self.typography.shutdown();
        self.motion.shutdown();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Binding {
    Document(DocumentId),
    Component(DocumentId, ComponentId),
}

/// Bindings handed to rendered rows; never reused.
#[derive(Default)]
struct Bindings {
    next: i32,
    map: HashMap<i32, Binding>,
}

impl Bindings {
    fn allocate(&mut self, binding: Binding) -> i32 {
        // Exhaustion leaves rows unbound; their callbacks are then refused.
        let Some(next) = self.next.checked_add(1) else {
            return -1;
        };
        self.next = next;
        self.map.insert(next, binding);
        next
    }

    fn get(&self, binding: i32) -> Option<Binding> {
        self.map.get(&binding).copied()
    }
}

/// One rendered component and its nested parameter model.
struct ComponentEntry {
    id: ComponentId,
    binding: i32,
    parameters: Rc<VecModel<ParameterRow>>,
}

/// The adapter between Slint and the workspace.
pub(crate) struct SlintAdapter {
    workspace: RefCell<Option<Workspace>>,
    shell: Box<dyn Shell>,
    main: slint::Weak<MainWindow>,
    about: slint::Weak<AboutWindow>,
    monitor: slint::Weak<MonitorWindow>,
    details: slint::Weak<DetailsWindow>,
    audio_controls: slint::Weak<AudioControlsWindow>,
    components: Rc<VecModel<ComponentRow>>,
    entries: RefCell<Vec<ComponentEntry>>,
    catalog: Rc<VecModel<slint::StandardListViewItem>>,
    lanes: Rc<VecModel<ModelLane>>,
    bindings: RefCell<Bindings>,
    document: Cell<Option<(DocumentId, i32)>>,
    projected: Cell<Option<(u64, Option<PlaybackPhase>)>>,
    capture: RefCell<Option<Arc<WaveformCapture>>>,
    model: RefCell<Option<Arc<ModelView>>>,
    updating: Cell<bool>,
    dialog_open: Cell<bool>,
    monitor_window_visible: Cell<bool>,
    audio_controls_visible: Cell<bool>,
    preferences: RefCell<Option<Preferences>>,
    timer: slint::Timer,
    deferred: slint::Timer,
    #[cfg(target_os = "macos")]
    termination: RefCell<Option<slint::winit_030::winit::platform::macos::TerminationRequest>>,
    #[cfg(test)]
    pub(crate) counts: TestCounts,
}

/// Work the adapter did, for checking that unchanged state does none.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct TestCounts {
    pub(crate) document_projections: Cell<u64>,
    pub(crate) path_builds: Cell<u64>,
}

impl SlintAdapter {
    /// Install the production bindings on `windows`, project the workspace
    /// and start polling.
    pub(crate) fn install(
        windows: &Windows,
        workspace: Workspace,
        shell: Box<dyn Shell>,
        preferences: Option<Preferences>,
    ) -> Rc<Self> {
        let ui = &windows.main;
        let adapter = Rc::new(Self {
            workspace: RefCell::new(Some(workspace)),
            shell,
            main: ui.as_weak(),
            about: windows.about.as_weak(),
            monitor: windows.monitor.as_weak(),
            details: windows.details.as_weak(),
            audio_controls: windows.audio_controls.as_weak(),
            components: Rc::new(VecModel::default()),
            entries: RefCell::new(Vec::new()),
            catalog: Rc::new(VecModel::default()),
            lanes: Rc::new(VecModel::default()),
            bindings: RefCell::new(Bindings::default()),
            document: Cell::new(None),
            projected: Cell::new(None),
            capture: RefCell::new(None),
            model: RefCell::new(None),
            updating: Cell::new(false),
            dialog_open: Cell::new(false),
            monitor_window_visible: Cell::new(false),
            audio_controls_visible: Cell::new(false),
            preferences: RefCell::new(preferences),
            timer: slint::Timer::default(),
            deferred: slint::Timer::default(),
            #[cfg(target_os = "macos")]
            termination: RefCell::new(None),
            #[cfg(test)]
            counts: TestCounts::default(),
        });
        set_choices(ui, &windows.monitor);
        ui.set_components(ModelRc::from(adapter.components.clone()));
        ui.set_catalog(ModelRc::from(adapter.catalog.clone()));
        ui.set_model_lanes(ModelRc::from(adapter.lanes.clone()));
        ui.set_guidance(
            format!(
                "{} {}",
                auditorylab::Notice::LowStartingVolume,
                auditorylab::Notice::HeadphonesForSeparateEars
            )
            .into(),
        );
        ui.set_native_mac(cfg!(target_os = "macos"));
        windows.about.set_identity(about_identity().into());
        if let Some(preferences) = adapter.preferences.borrow().as_ref() {
            apply_font(windows, &preferences.font);
        }
        install_editing(&adapter, ui);
        install_actions(&adapter, ui);
        install_monitor(&adapter, windows);
        install_auxiliary(&adapter, windows);
        adapter.after_command();
        let weak = Rc::downgrade(&adapter);
        adapter
            .timer
            .start(slint::TimerMode::Repeated, IDLE_TICK, move || {
                if let Some(adapter) = weak.upgrade() {
                    adapter.tick();
                }
            });
        adapter
    }

    /// Run `command` on the workspace, then perform its effects and project.
    /// Callbacks raised while projecting are not commands and are ignored.
    pub(crate) fn command<R>(
        self: &Rc<Self>,
        command: impl FnOnce(&mut Workspace) -> R,
    ) -> Option<R> {
        if self.updating.get() {
            return None;
        }
        let result = self.workspace.borrow_mut().as_mut().map(command);
        self.after_command();
        result
    }

    /// Answer a prompt; answers to prompts no longer open change nothing.
    pub(crate) fn respond(self: &Rc<Self>, prompt: PromptId, response: Response) {
        self.dialog_open.set(false);
        if let Some(workspace) = self.workspace.borrow_mut().as_mut() {
            workspace.respond(prompt, response);
        }
        self.after_command();
    }

    /// Poll tasks and native preferences, then project what changed.
    pub(crate) fn tick(self: &Rc<Self>) {
        if self.updating.get() {
            return;
        }
        self.drain_preferences();
        if let Some(workspace) = self.workspace.borrow_mut().as_mut() {
            workspace.poll();
        }
        self.after_command();
    }

    fn after_command(self: &Rc<Self>) {
        let effects = self
            .workspace
            .borrow_mut()
            .as_mut()
            .map(Workspace::take_effects)
            .unwrap_or_default();
        for effect in effects {
            self.perform(effect);
        }
        self.project();
    }

    fn perform(self: &Rc<Self>, effect: Effect) {
        match effect {
            Effect::AskUnsaved(prompt) => {
                self.dialog_open.set(true);
                self.shell.ask_unsaved(self, prompt);
            }
            Effect::ChoosePath {
                prompt,
                purpose,
                suggested_name,
            } => {
                self.dialog_open.set(true);
                self.shell
                    .choose_path(self, prompt, purpose, suggested_name);
            }
            Effect::CloseResolved { approved } => self.shell.close_resolved(self, approved),
            Effect::ShowView(view) => {
                if let Some(ui) = self.main.upgrade() {
                    ui.set_tab_index(match view {
                        View::Library => 0,
                        View::Workbench => 1,
                    });
                }
            }
        }
    }

    /// The current audio session's Stop control, for a native control bound
    /// to exactly that session.
    pub(crate) fn audio_control(&self) -> Option<auditorylab::PlaybackControl> {
        self.workspace
            .borrow()
            .as_ref()
            .and_then(Workspace::audio_control)
    }

    /// Inspect the workspace directly, bypassing the bindings under test.
    #[cfg(test)]
    pub(crate) fn inspect<R>(&self, f: impl FnOnce(&mut Workspace) -> R) -> R {
        f(self
            .workspace
            .borrow_mut()
            .as_mut()
            .expect("the workspace is open until shutdown"))
    }

    pub(crate) fn main_window(&self) -> Option<MainWindow> {
        self.main.upgrade()
    }

    /// Hold a native termination request until the shared close decision.
    #[cfg(target_os = "macos")]
    pub(crate) fn request_termination(
        self: &Rc<Self>,
        request: slint::winit_030::winit::platform::macos::TerminationRequest,
    ) {
        // A newer request supersedes an unanswered older one; the backend
        // already treats the older token as stale.
        *self.termination.borrow_mut() = Some(request);
        self.command(Workspace::close);
    }

    /// The pending native termination request, if any.
    #[cfg(target_os = "macos")]
    pub(crate) fn take_termination(
        &self,
    ) -> Option<slint::winit_030::winit::platform::macos::TerminationRequest> {
        self.termination.borrow_mut().take()
    }

    pub(crate) fn show_about(&self) {
        if let Some(ui) = self.main.upgrade() {
            ui.set_edits_suspended(true);
        }
        if let Some(about) = self.about.upgrade() {
            if let Err(error) = about.show() {
                eprintln!("About: {error}");
            }
            about.invoke_focus_close();
        }
    }

    /// Stop polling, end every task and native listener, and report actual failures.
    pub(crate) fn shutdown(&self) -> Result<(), String> {
        self.timer.stop();
        self.deferred.stop();
        if let Some(mut preferences) = self.preferences.borrow_mut().take() {
            preferences.shutdown();
        }
        self.workspace
            .borrow_mut()
            .take()
            .map_or(Ok(()), Workspace::shutdown)
    }

    fn resolve(&self, binding: i32) -> Option<Binding> {
        self.bindings.borrow().get(binding)
    }

    fn parameter_handle(&self, binding: i32, key: ParameterKey) -> Option<FieldHandle> {
        match self.resolve(binding)? {
            Binding::Component(document, id) => Some(FieldHandle {
                document,
                field: EditingField::Parameter(id, parameter_id(key)),
            }),
            Binding::Document(_) => None,
        }
    }

    fn metadata_handle(&self, binding: i32, key: MetadataKey) -> Option<FieldHandle> {
        match self.resolve(binding)? {
            Binding::Document(document) => Some(FieldHandle {
                document,
                field: EditingField::Metadata(metadata_field(key)),
            }),
            Binding::Component(..) => None,
        }
    }

    fn component(&self, binding: i32) -> Option<(DocumentId, ComponentId)> {
        match self.resolve(binding)? {
            Binding::Component(document, id) => Some((document, id)),
            Binding::Document(_) => None,
        }
    }

    fn drain_preferences(&self) {
        let mut preferences = self.preferences.borrow_mut();
        let Some(preferences) = preferences.as_mut() else {
            return;
        };
        if let Some(font) = preferences.typography.latest()
            && preferences.font != font
        {
            eprintln!(
                "Typography: {} · {} logical px · {}",
                font.family, font.size, font.origin
            );
            preferences.font = font;
            if let (Some(main), Some(about), Some(monitor), Some(details), Some(controls)) = (
                self.main.upgrade(),
                self.about.upgrade(),
                self.monitor.upgrade(),
                self.details.upgrade(),
                self.audio_controls.upgrade(),
            ) {
                apply_font(
                    &Windows {
                        main,
                        about,
                        monitor,
                        details,
                        audio_controls: controls,
                    },
                    &preferences.font,
                );
            }
        }
        if let Some(motion) = preferences.motion.latest()
            && let Some(workspace) = self.workspace.borrow_mut().as_mut()
        {
            workspace.set_motion_preference(match motion {
                motion::Preference::Reduced => MotionPreference::Reduced,
                motion::Preference::Full => MotionPreference::Enabled,
                motion::Preference::Unknown => MotionPreference::Unknown,
            });
        }
    }

    // ---- projection ----

    fn project(self: &Rc<Self>) {
        let Some(ui) = self.main.upgrade() else {
            return;
        };
        if self.updating.replace(true) {
            return;
        }
        let state = self
            .workspace
            .borrow()
            .as_ref()
            .map(|w| (w.revision(), w.audio_phase()));
        let Some(state) = state else {
            self.updating.set(false);
            return;
        };
        if self.projected.get() != Some(state) {
            self.projected.set(Some(state));
            self.project_document(&ui);
        }
        self.project_telemetry(&ui);
        self.sync_auxiliary(&ui);
        self.schedule();
        self.updating.set(false);
    }

    fn schedule(&self) {
        let busy = self.workspace.borrow().as_ref().is_some_and(|w| {
            w.audio_busy() || w.file_busy() || w.action_pending() || w.prompt_open()
        });
        let interval = if busy { ACTIVE_TICK } else { IDLE_TICK };
        if self.timer.interval() != interval {
            self.timer.set_interval(interval);
        }
    }

    fn project_document(&self, ui: &MainWindow) {
        #[cfg(test)]
        self.counts
            .document_projections
            .set(self.counts.document_projections.get() + 1);
        let views = self.workspace.borrow_mut().as_mut().map(|w| {
            (
                w.editor_view(),
                w.library_view(),
                w.action_view(),
                w.export_rate(),
            )
        });
        let Some((editor, library, actions, rate)) = views else {
            return;
        };
        self.reconcile_document(ui, &editor);
        ui.set_dirty(editor.dirty);
        ui.set_draft_brief(editor.brief.as_str().into());
        ui.set_peak(editor.peak.as_str().into());
        ui.set_metadata_error(editor.metadata_error.as_str().into());
        ui.set_based_on_title(editor.based_on_title.as_str().into());
        ui.set_based_on(editor.based_on.as_str().into());
        ui.set_retained_warning(editor.retained_warning.as_str().into());

        sync_rows(
            &self.catalog,
            library
                .rows
                .iter()
                .map(|label| slint::StandardListViewItem::from(label.as_str()))
                .collect(),
        );
        ui.set_has_results(!library.rows.is_empty());
        ui.set_selected(
            library
                .selected
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
        ui.set_sound_title(library.title.as_str().into());
        ui.set_structure(library.structure.as_str().into());
        ui.set_studied(library.studied.as_str().into());
        ui.set_recipe_timing(library.timing.as_str().into());
        ui.set_summary(library.summary.as_str().into());
        ui.set_details(library.details.as_str().into());
        ui.set_can_open_study(library.study_url.is_some());
        if ui.get_query() != self.query() {
            ui.set_query(self.query().into());
        }

        ui.set_can_play(actions.can_play);
        ui.set_play_reason(actions.play_reason.as_str().into());
        ui.set_can_save(actions.can_save);
        ui.set_save_reason(actions.save_reason.as_str().into());
        ui.set_can_export(actions.can_export);
        ui.set_export_reason(actions.export_reason.as_str().into());
        ui.set_can_edit(actions.can_edit);
        ui.set_busy(actions.file_busy);
        ui.set_playback_busy(actions.playback_busy);
        ui.set_exporting(actions.exporting);
        ui.set_transport_enabled(actions.transport_enabled);
        ui.set_transport_action(actions.transport_label.as_str().into());
        ui.set_play_target(actions.play_target.as_str().into());
        ui.set_file_activity(actions.file_activity.as_str().into());
        ui.set_status(
            [actions.file_status.as_str(), actions.audio_status.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
                .into(),
        );
        if let Some(index) = Workspace::export_rates().iter().position(|r| *r == rate)
            && let Ok(index) = i32::try_from(index)
        {
            ui.set_rate_index(index);
        }
    }

    fn query(&self) -> String {
        self.workspace
            .borrow()
            .as_ref()
            .map(|w| w.query().to_owned())
            .unwrap_or_default()
    }

    /// Reconcile the document's rows by identity. Only a replaced document
    /// resets the models; otherwise rows are inserted, removed or updated
    /// one by one, so surviving fields keep their widgets, focus and caret.
    fn reconcile_document(&self, ui: &MainWindow, editor: &EditorView) {
        let replaced = self
            .document
            .get()
            .is_none_or(|(d, _)| d != editor.document);
        if replaced {
            // Rebinding: no automatic blur commit may reach the new document.
            let suspended = ui.get_edits_suspended();
            ui.set_edits_suspended(true);
            let binding = self
                .bindings
                .borrow_mut()
                .allocate(Binding::Document(editor.document));
            self.document.set(Some((editor.document, binding)));
            let mut entries = self.entries.borrow_mut();
            entries.clear();
            let rows: Vec<_> = editor
                .components
                .iter()
                .map(|component| {
                    let entry = self.new_entry(editor.document, component);
                    let row = component_row(&entry, component);
                    entries.push(entry);
                    row
                })
                .collect();
            self.components.set_vec(rows);
            set_metadata(ui, &editor.metadata);
            ui.set_document(binding);
            ui.set_document_reset(ui.get_document_reset().wrapping_add(1));
            ui.set_editor_scroll_y(0.0);
            ui.set_edits_suspended(suspended);
            return;
        }
        set_metadata(ui, &editor.metadata);
        let mut entries = self.entries.borrow_mut();
        let mut index = 0;
        while index < entries.len() {
            if editor.components.iter().any(|c| c.id == entries[index].id) {
                index += 1;
            } else {
                entries.remove(index);
                self.components.remove(index);
            }
        }
        for (index, component) in editor.components.iter().enumerate() {
            match entries.iter().position(|e| e.id == component.id) {
                Some(found) if found == index => {}
                Some(found) => {
                    let entry = entries.remove(found);
                    let row = self.components.row_data(found);
                    self.components.remove(found);
                    entries.insert(index, entry);
                    if let Some(row) = row {
                        self.components.insert(index, row);
                    }
                }
                None => {
                    let entry = self.new_entry(editor.document, component);
                    self.components
                        .insert(index, component_row(&entry, component));
                    entries.insert(index, entry);
                    continue;
                }
            }
            let entry = &entries[index];
            reconcile_parameters(&entry.parameters, &component.parameters);
            let row = component_row(entry, component);
            if self.components.row_data(index).as_ref() != Some(&row) {
                self.components.set_row_data(index, row);
            }
        }
    }

    fn new_entry(&self, document: DocumentId, component: &ComponentView) -> ComponentEntry {
        ComponentEntry {
            id: component.id,
            binding: self
                .bindings
                .borrow_mut()
                .allocate(Binding::Component(document, component.id)),
            parameters: Rc::new(VecModel::from(
                component
                    .parameters
                    .iter()
                    .map(parameter_row)
                    .collect::<Vec<_>>(),
            )),
        }
    }

    fn project_telemetry(&self, ui: &MainWindow) {
        let views = self.workspace.borrow().as_ref().map(|w| {
            (
                w.transport_view(),
                w.output_view(),
                w.model_view(),
                w.export_progress(),
            )
        });
        let Some((transport, output, model, progress)) = views else {
            return;
        };
        ui.set_player_title(transport.player_title.as_str().into());
        ui.set_elapsed(transport.elapsed.as_str().into());
        ui.set_generated(transport.generated.as_str().into());
        ui.set_generated_details(transport.generated_details.as_str().into());
        ui.set_device(transport.device.as_str().into());
        ui.set_output_state(transport.output_state.into());
        ui.set_receipt_age(transport.receipt_age.as_str().into());
        ui.set_monitor_warning(transport.warning.0.as_str().into());
        ui.set_monitor_warning_live(transport.warning.1);
        ui.set_backend_warning(transport.backend_warning.as_str().into());
        ui.set_monitor_state(transport.monitor_state.as_str().into());
        ui.set_meters(transport.meters.as_str().into());
        ui.set_waveform_caption(transport.caption.as_str().into());
        ui.set_model_caption(transport.model_caption.as_str().into());
        ui.set_motion_reason(transport.motion.as_str().into());
        ui.set_motion_overridden(transport.motion_overridden);
        ui.set_held(output.held);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "A 0..=1 display ratio rounds to the renderer's f32"
        )]
        ui.set_progress(progress as f32);
        let changed = match (&*self.capture.borrow(), &output.capture) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if changed {
            // Paths are rebuilt only for another capture, never for
            // receipt age or other telemetry.
            #[cfg(test)]
            self.counts
                .path_builds
                .set(self.counts.path_builds.get() + 1);
            if let Some(capture) = &output.capture {
                let [left, right] = scope::paths(capture.bins(), capture.limit());
                ui.set_left_path(left.into());
                ui.set_right_path(right.into());
                ui.set_scale_label(auditorylab::compact_number(capture.limit()).into());
                ui.set_capture_duration(format!("{:.2} ms", capture.milliseconds()).into());
            } else {
                ui.set_left_path(SharedString::new());
                ui.set_right_path(SharedString::new());
            }
            *self.capture.borrow_mut() = output.capture;
        }
        let model_changed = match (&*self.model.borrow(), &model) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if model_changed {
            sync_rows(
                &self.lanes,
                model.as_ref().map_or_else(Vec::new, |model| {
                    model
                        .projection
                        .lanes
                        .iter()
                        .map(|lane| {
                            let stereo: Vec<_> = lane.columns.iter().map(|e| [*e; 2]).collect();
                            let [trace, _] = scope::paths(&stereo, 1.0);
                            ModelLane {
                                component: i32::try_from(lane.id.get()).unwrap_or(-1),
                                label: ModelView::lane_label(lane).into(),
                                trace: trace.into(),
                            }
                        })
                        .collect()
                }),
            );
            *self.model.borrow_mut() = model;
        }
        ui.set_typography_origin(
            self.preferences
                .borrow()
                .as_ref()
                .map_or_else(String::new, |p| {
                    format!(
                        "{} · {} logical px · {}",
                        p.font.family, p.font.size, p.font.origin
                    )
                })
                .into(),
        );
    }

    /// Show the focused monitor window when the drawer does not fit, and the
    /// separate Stop window while a dialog may block the main window.
    fn sync_auxiliary(&self, ui: &MainWindow) {
        let focused_monitor = ui.get_monitor_open() && !ui.get_drawer_fits();
        if let Some(window) = self.monitor.upgrade() {
            window.set_data(ui.get_monitor_data());
            if focused_monitor != self.monitor_window_visible.replace(focused_monitor) {
                if focused_monitor {
                    let _ = window.show();
                } else {
                    use slint::winit_030::{WinitWindowAccessor, winit};
                    // Hiding can transfer native focus; sample ownership first.
                    let was_focused = window
                        .window()
                        .with_winit_window(winit::window::Window::has_focus)
                        .unwrap_or(false);
                    if let Some(details) = self.details.upgrade()
                        && details.window().is_visible()
                    {
                        details_parent::bind(&details, ui);
                    }
                    hide_monitor_window(ui, &window, was_focused);
                }
            }
        }
        let audio = ui.get_playback_busy();
        let controls = self.dialog_open.get() && audio && !cfg!(target_os = "macos");
        if let Some(window) = self.audio_controls.upgrade() {
            window.set_source(ui.get_player_title());
            window.set_elapsed(ui.get_elapsed());
            window.set_active(audio);
            if controls != self.audio_controls_visible.replace(controls) {
                if controls {
                    let _ = window.show();
                } else {
                    let _ = window.hide();
                }
            }
        }
    }

    fn show_diagnostics<C: ComponentHandle + 'static>(&self, parent: &C) {
        self.refresh_diagnostics();
        if let Some(window) = self.details.upgrade() {
            present_details(&window, parent);
        }
    }

    fn refresh_diagnostics(&self) {
        let (Some(ui), Some(window)) = (self.main.upgrade(), self.details.upgrade()) else {
            return;
        };
        window.set_heading("Audio details · snapshot".into());
        window.set_can_refresh(true);
        window.set_content(
            format!(
                "{}\n{}\n{}\n{}\n{}\n{}\nTypography: {}\n{}",
                ui.get_player_title(),
                ui.get_generated_details(),
                ui.get_monitor_state(),
                ui.get_device(),
                ui.get_status(),
                ui.get_motion_reason(),
                ui.get_typography_origin(),
                ui.get_model_caption()
            )
            .into(),
        );
    }
}

// ---- semantic binding conversions ----

/// The exhaustive core-to-binding mapping: a new quantity must be added here.
pub(crate) const fn parameter_key(id: ParameterId) -> ParameterKey {
    match id {
        ParameterId::ToneFrequency => ParameterKey::ToneFrequency,
        ParameterId::ClickRate => ParameterKey::ClickRate,
        ParameterId::ClickWidth => ParameterKey::ClickWidth,
        ParameterId::PeakLevel => ParameterKey::PeakLevel,
        ParameterId::AmplitudeModulationRate => ParameterKey::AmplitudeModulationRate,
        ParameterId::ModulationDepth => ParameterKey::ModulationDepth,
        ParameterId::GateRate => ParameterKey::GateRate,
        ParameterId::GateWidth => ParameterKey::GateWidth,
        ParameterId::Duration => ParameterKey::Duration,
        ParameterId::Fade => ParameterKey::Fade,
    }
}

pub(crate) const fn parameter_id(key: ParameterKey) -> ParameterId {
    match key {
        ParameterKey::ToneFrequency => ParameterId::ToneFrequency,
        ParameterKey::ClickRate => ParameterId::ClickRate,
        ParameterKey::ClickWidth => ParameterId::ClickWidth,
        ParameterKey::PeakLevel => ParameterId::PeakLevel,
        ParameterKey::AmplitudeModulationRate => ParameterId::AmplitudeModulationRate,
        ParameterKey::ModulationDepth => ParameterId::ModulationDepth,
        ParameterKey::GateRate => ParameterId::GateRate,
        ParameterKey::GateWidth => ParameterId::GateWidth,
        ParameterKey::Duration => ParameterId::Duration,
        ParameterKey::Fade => ParameterId::Fade,
    }
}

const fn metadata_field(key: MetadataKey) -> MetadataField {
    match key {
        MetadataKey::Title => MetadataField::Title,
        MetadataKey::Summary => MetadataField::Summary,
        MetadataKey::Duration => MetadataField::Duration,
        MetadataKey::Fade => MetadataField::Fade,
    }
}

fn index_of<T: PartialEq>(all: &[T], value: &T) -> i32 {
    all.iter()
        .position(|v| v == value)
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1)
}

fn choice<T: Copy>(all: &[T], index: i32) -> Option<T> {
    usize::try_from(index)
        .ok()
        .and_then(|i| all.get(i))
        .copied()
}

fn set_choices(ui: &MainWindow, monitor: &MonitorWindow) {
    let strings = |labels: Vec<String>| {
        ModelRc::new(VecModel::from(
            labels
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        ))
    };
    ui.set_ears_choices(strings(
        Ears::ALL.iter().map(|e| e.label().into()).collect(),
    ));
    ui.set_carrier_choices(strings(
        CarrierKind::ALL.iter().map(|k| k.label().into()).collect(),
    ));
    ui.set_add_choices(strings(
        CarrierKind::ALL
            .iter()
            .map(|k| {
                format!(
                    "Add {}",
                    auditorylab::default_component(*k)
                        .role()
                        .label()
                        .to_lowercase()
                )
            })
            .collect(),
    ));
    ui.set_modulation_choices(strings(
        std::iter::once(ModulationKind::NONE_LABEL.to_owned())
            .chain(ModulationKind::ALL.iter().map(|k| k.label().into()))
            .collect(),
    ));
    let spans = strings(SPANS_MS.iter().map(|ms| format!("{ms} ms")).collect());
    let scales = strings(
        SCALES
            .iter()
            .map(|(_, label)| (*label).to_owned())
            .collect(),
    );
    ui.set_span_choices(spans.clone());
    ui.set_scale_choices(scales.clone());
    monitor.set_span_choices(spans);
    monitor.set_scale_choices(scales);
    ui.set_export_rates(strings(
        Workspace::export_rates()
            .iter()
            .map(|rate| format!("{} Hz", rate.hz()))
            .collect(),
    ));
}

fn parameter_row(view: &ParameterView) -> ParameterRow {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "A normalized slider position rounds to the renderer's f32; values stay binary64"
    )]
    let position = view.slider.unwrap_or(0.0) as f32;
    ParameterRow {
        key: parameter_key(view.parameter),
        label: view.label.as_str().into(),
        help: view.help.into(),
        text: view.text.as_str().into(),
        staged: view.staged,
        has_slider: view.slider.is_some(),
        position,
        diagnostic: view.diagnostic.as_str().into(),
    }
}

fn component_row(entry: &ComponentEntry, view: &ComponentView) -> ComponentRow {
    ComponentRow {
        binding: entry.binding,
        id: i32::try_from(view.id.get()).unwrap_or(-1),
        ears: index_of(&Ears::ALL, &view.ears),
        carrier: index_of(&CarrierKind::ALL, &view.carrier),
        modulation: view
            .modulation
            .map_or(0, |kind| index_of(&ModulationKind::ALL, &kind) + 1),
        heading: view.heading.into(),
        name: view.name.as_str().into(),
        context: view.context.as_str().into(),
        error: view.error.as_str().into(),
        parameters: ModelRc::from(entry.parameters.clone()),
    }
}

/// Reconcile one component's parameters by key with minimal notifications.
fn reconcile_parameters(model: &VecModel<ParameterRow>, views: &[ParameterView]) {
    let rows: Vec<_> = views.iter().map(parameter_row).collect();
    let mut index = 0;
    while index < model.row_count() {
        if model
            .row_data(index)
            .is_some_and(|row| rows.iter().any(|r| r.key == row.key))
        {
            index += 1;
        } else {
            model.remove(index);
        }
    }
    for (index, row) in rows.into_iter().enumerate() {
        let found = (index..model.row_count())
            .find(|&i| model.row_data(i).is_some_and(|r| r.key == row.key));
        match found {
            Some(i) if i == index => {
                if model.row_data(index).as_ref() != Some(&row) {
                    model.set_row_data(index, row);
                }
            }
            Some(i) => {
                model.remove(i);
                model.insert(index, row);
            }
            None => model.insert(index, row),
        }
    }
}

fn metadata_row(view: &MetadataView) -> MetadataRow {
    MetadataRow {
        label: view.label.as_str().into(),
        text: view.text.as_str().into(),
        staged: view.staged,
        diagnostic: view.diagnostic.as_str().into(),
    }
}

fn set_metadata(ui: &MainWindow, metadata: &[MetadataView; 4]) {
    for view in metadata {
        let EditingField::Metadata(field) = view.handle.field else {
            continue;
        };
        let row = metadata_row(view);
        match field {
            MetadataField::Title => ui.set_title_field(row),
            MetadataField::Summary => ui.set_summary_field(row),
            MetadataField::Duration => ui.set_duration_field(row),
            MetadataField::Fade => ui.set_fade_field(row),
        }
    }
}

/// Positional update for read-only lists whose rows have no identity of their own.
fn sync_rows<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    while model.row_count() > rows.len() {
        model.remove(model.row_count() - 1);
    }
    for (index, row) in rows.into_iter().enumerate() {
        if index < model.row_count() {
            if model.row_data(index).as_ref() != Some(&row) {
                model.set_row_data(index, row);
            }
        } else {
            model.push(row);
        }
    }
}

// ---- callback installation ----

fn install_editing(adapter: &Rc<SlintAdapter>, ui: &MainWindow) {
    let weak = Rc::downgrade(adapter);
    let with = move |f: &dyn Fn(&Rc<SlintAdapter>)| {
        if let Some(adapter) = weak.upgrade() {
            f(&adapter);
        }
    };
    let w = with.clone();
    ui.on_stage_parameter(move |binding, key, text| {
        w(&|a| {
            if let Some(handle) = a.parameter_handle(binding, key) {
                a.command(|w| w.stage(handle, &text));
            }
        });
    });
    let w = with.clone();
    ui.on_commit_parameter(move |binding, key, text| {
        w(&|a| {
            if let Some(handle) = a.parameter_handle(binding, key) {
                a.command(|w| w.commit_field(handle, &text));
            }
        });
    });
    let w = with.clone();
    ui.on_restore_parameter(move |binding, key| {
        w(&|a| {
            if let Some(handle) = a.parameter_handle(binding, key) {
                a.command(|w| w.restore(handle));
            }
        });
    });
    let w = with.clone();
    ui.on_slide(move |binding, key, position| {
        w(&|a| {
            if let Some(handle) = a.parameter_handle(binding, key) {
                a.command(|w| w.slide(handle, f64::from(position)));
            }
        });
    });
    let w = with.clone();
    ui.on_stage_metadata(move |binding, key, text| {
        w(&|a| {
            if let Some(handle) = a.metadata_handle(binding, key) {
                a.command(|w| w.stage(handle, &text));
            }
        });
    });
    let w = with.clone();
    ui.on_commit_metadata(move |binding, key, text| {
        w(&|a| {
            if let Some(handle) = a.metadata_handle(binding, key) {
                a.command(|w| w.commit_field(handle, &text));
            }
        });
    });
    let w = with.clone();
    ui.on_restore_metadata(move |binding, key| {
        w(&|a| {
            if let Some(handle) = a.metadata_handle(binding, key) {
                a.command(|w| w.restore(handle));
            }
        });
    });
    let w = with.clone();
    ui.on_set_ears(move |binding, index| {
        w(&|a| {
            if let (Some((document, id)), Some(ears)) =
                (a.component(binding), choice(&Ears::ALL, index))
            {
                a.command(|w| w.set_ears(document, id, ears));
            }
        });
    });
    let w = with.clone();
    ui.on_set_carrier(move |binding, index| {
        w(&|a| {
            if let (Some((document, id)), Some(kind)) =
                (a.component(binding), choice(&CarrierKind::ALL, index))
            {
                a.command(|w| w.set_carrier(document, id, kind));
            }
        });
    });
    let w = with.clone();
    ui.on_set_modulation(move |binding, index| {
        w(&|a| {
            let kind = match index {
                0 => Some(None),
                i => choice(&ModulationKind::ALL, i - 1).map(Some),
            };
            if let (Some((document, id)), Some(kind)) = (a.component(binding), kind) {
                a.command(|w| w.set_modulation(document, id, kind));
            }
        });
    });
    let w = with.clone();
    ui.on_remove_component(move |binding| {
        w(&|a| {
            if let Some((document, id)) = a.component(binding) {
                a.command(|w| w.remove_component(document, id));
            }
        });
    });
    let w = with;
    ui.on_add_component(move |binding, index| {
        w(&|a| {
            if let (Some(Binding::Document(document)), Some(kind)) =
                (a.resolve(binding), choice(&CarrierKind::ALL, index))
            {
                a.command(|w| w.add_component(document, kind));
            }
        });
    });
}

fn install_actions(adapter: &Rc<SlintAdapter>, ui: &MainWindow) {
    macro_rules! on {
        ($setter:ident, |$a:ident $(, $arg:ident)*| $body:expr) => {{
            let weak: Weak<SlintAdapter> = Rc::downgrade(adapter);
            ui.$setter(move |$($arg),*| {
                if let Some($a) = weak.upgrade() {
                    $body;
                }
            });
        }};
    }
    on!(on_select, |a, index| {
        if let Ok(index) = usize::try_from(index) {
            a.command(|w| w.select(index));
        }
    });
    on!(on_search, |a, query| a.command(|w| w.set_query(&query)));
    on!(on_tab_changed, |a, index| {
        // The tab's change handler can run while a projection is writing
        // properties, when commands are ignored; apply the choice after it.
        let view = if index == 1 {
            View::Workbench
        } else {
            View::Library
        };
        let weak = Rc::downgrade(&a);
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(a) = weak.upgrade() {
                a.command(|w| w.set_view(view));
            }
        });
    });
    on!(on_rate_changed, |a, index| {
        if let Some(rate) = choice(&Workspace::export_rates(), index) {
            a.command(|w| w.set_export_rate(rate));
        }
    });
    on!(on_play_stop, |a| a.command(|w| {
        if w.audio_busy() {
            let _ = w.stop();
        } else {
            let _ = w.play();
        }
    }));
    on!(on_play_recipe, |a| a.command(Workspace::play));
    on!(on_stop_audio, |a| a.command(Workspace::stop));
    on!(on_preview_selected, |a| a
        .command(Workspace::preview_current));
    on!(on_open_recipe, |a| a.command(Workspace::import));
    on!(on_save_recipe, |a| a.command(Workspace::save));
    on!(on_export_wav, |a| a.command(Workspace::export));
    on!(on_cancel_file, |a| a.command(Workspace::cancel_export));
    on!(on_open_workbench, |a| a.command(Workspace::copy_selected));
    on!(on_new_draft, |a| a.command(Workspace::new_document));
    on!(on_based_on_source, |a| a
        .command(Workspace::show_based_on_source));
    on!(on_quit, |a| a.command(Workspace::close));
    on!(on_about, |a| a.show_about());
    on!(on_open_study, |a| {
        let url = a
            .workspace
            .borrow_mut()
            .as_mut()
            .and_then(|w| w.library_view().study_url);
        if let Some(url) = url {
            open_web(&url);
        }
    });
    on!(on_show_diagnostics, |a| {
        if let Some(ui) = a.main.upgrade() {
            a.show_diagnostics(&ui);
        }
    });
    on!(on_show_research, |a| {
        let library = a.workspace.borrow().as_ref().map(Workspace::library_view);
        if let (Some(window), Some(ui), Some(library)) =
            (a.details.upgrade(), a.main.upgrade(), library)
        {
            window.set_heading(format!("Sound details · {}", library.title).into());
            window.set_can_refresh(false);
            window.set_content(library.details.into());
            present_details(&window, &ui);
        }
    });
    on!(on_hold_changed, |a| {
        if let Some(ui) = a.main.upgrade() {
            let held = ui.get_held();
            a.command(|w| w.set_hold(held));
        }
    });
    on!(on_follow_motion, |a| a.command(Workspace::follow_motion));
    on!(on_window_changed, |a| {
        // Layout change handlers also run while drawing; native window
        // transitions wait until the current frame completes.
        let weak = Rc::downgrade(&a);
        a.deferred
            .start(slint::TimerMode::SingleShot, Duration::ZERO, move || {
                if let Some(a) = weak.upgrade() {
                    a.apply_waveform_choice();
                }
            });
    });
    let weak = Rc::downgrade(adapter);
    ui.window().on_close_requested(move || {
        if let Some(a) = weak.upgrade() {
            a.command(Workspace::close);
        }
        slint::CloseRequestResponse::KeepWindowShown
    });
    let weak = ui.as_weak();
    slint::winit_030::WinitWindowAccessor::on_winit_window_event(ui.window(), move |_, event| {
        use slint::winit_030::{EventResult, winit};
        if let winit::event::WindowEvent::Focused(focused) = event
            && let Some(ui) = weak.upgrade()
        {
            ui.set_edits_suspended(!focused);
        }
        EventResult::Propagate
    });
}

impl SlintAdapter {
    fn apply_waveform_choice(self: &Rc<Self>) {
        let Some(ui) = self.main.upgrade() else {
            return;
        };
        let span = choice(&SPANS_MS, ui.get_window_index());
        let scale = choice(&SCALES, ui.get_scale_index()).map(|(scale, _)| scale);
        if let (Some(span), Some(scale)) = (span, scale) {
            self.command(|w| w.set_waveform(span, scale));
        } else {
            self.project();
        }
    }
}

fn install_monitor(adapter: &Rc<SlintAdapter>, windows: &Windows) {
    let monitor = &windows.monitor;
    let weak = Rc::downgrade(adapter);
    monitor.on_span(move |index| {
        if let Some(a) = weak.upgrade() {
            if let Some(ui) = a.main.upgrade() {
                ui.set_window_index(index);
            }
            a.apply_waveform_choice();
        }
    });
    let weak = Rc::downgrade(adapter);
    monitor.on_scale(move |index| {
        if let Some(a) = weak.upgrade() {
            if let Some(ui) = a.main.upgrade() {
                ui.set_scale_index(index);
            }
            a.apply_waveform_choice();
        }
    });
    let weak = Rc::downgrade(adapter);
    monitor.on_hold(move || {
        if let Some(a) = weak.upgrade()
            && let Some(ui) = a.main.upgrade()
        {
            let held = !ui.get_held();
            a.command(|w| w.set_hold(held));
        }
    });
    let weak = Rc::downgrade(adapter);
    monitor.on_follow(move || {
        if let Some(a) = weak.upgrade() {
            a.command(Workspace::follow_motion);
        }
    });
    let weak = Rc::downgrade(adapter);
    monitor.on_details(move || {
        if let Some(a) = weak.upgrade()
            && let Some(monitor) = a.monitor.upgrade()
        {
            a.show_diagnostics(&monitor);
        }
    });
    let weak = Rc::downgrade(adapter);
    monitor.on_dismiss(move || {
        if let Some(a) = weak.upgrade() {
            if let Some(ui) = a.main.upgrade() {
                ui.set_monitor_open(false);
                ui.invoke_focus_monitor_opener();
            }
            a.project();
        }
    });
    let weak = monitor.as_weak();
    monitor.window().on_close_requested(move || {
        if let Some(window) = weak.upgrade() {
            window.invoke_dismiss();
        }
        slint::CloseRequestResponse::KeepWindowShown
    });
}

fn install_auxiliary(adapter: &Rc<SlintAdapter>, windows: &Windows) {
    let weak = Rc::downgrade(adapter);
    windows.audio_controls.on_stop(move || {
        if let Some(a) = weak.upgrade() {
            a.command(Workspace::stop);
        }
    });
    windows
        .audio_controls
        .window()
        .on_close_requested(|| slint::CloseRequestResponse::KeepWindowShown);
    let about = windows.about.as_weak();
    windows.about.on_dismiss(move || {
        if let Some(about) = about.upgrade() {
            let _ = about.hide();
        }
    });
    windows
        .about
        .on_open_source(|| open_web(auditorylab::SOURCE_URL));
    let details = windows.details.as_weak();
    windows.details.on_dismiss(move || {
        if let Some(window) = details.upgrade() {
            let _ = window.hide();
        }
    });
    let weak = Rc::downgrade(adapter);
    windows.details.on_refresh_content(move || {
        if let Some(a) = weak.upgrade() {
            a.refresh_diagnostics();
        }
    });
}

// ---- window helpers ----

fn apply_font(windows: &Windows, font: &typography::Typography) {
    let family: SharedString = font.family.as_str().into();
    windows.main.set_ui_family(family.clone());
    windows.main.set_ui_size(font.size);
    windows.about.set_ui_family(family.clone());
    windows.about.set_ui_size(font.size);
    windows.monitor.set_ui_family(family.clone());
    windows.monitor.set_ui_size(font.size);
    windows.details.set_ui_family(family.clone());
    windows.details.set_ui_size(font.size);
    windows.audio_controls.set_ui_family(family);
    windows.audio_controls.set_ui_size(font.size);
}

fn present_details<C: ComponentHandle + 'static>(window: &DetailsWindow, parent: &C) {
    if let Err(error) = window.show() {
        eprintln!("Details: {error}");
        return;
    }
    details_parent::bind(window, parent);
    window.invoke_focus_close();
}

pub(crate) fn hide_monitor_window(ui: &MainWindow, monitor: &MonitorWindow, was_focused: bool) {
    let _ = monitor.hide();
    if was_focused {
        ui.invoke_focus_monitor_opener();
    }
}

fn about_identity() -> String {
    let identity = auditorylab::BUILD_IDENTITY;
    format!(
        "Version {} · Build {}\n{}\n\nAuditory Lab: MPL-2.0\nSlint: Royalty-free Desktop, Mobile, and Web Applications License 2.0\nThird-party licenses: THIRD-PARTY-NOTICES.txt in the download.",
        identity.version,
        identity.build,
        identity
            .revision
            .map_or(String::new(), |r| format!("Source revision: {r}"))
    )
}

fn open_web(url: &str) {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    if let Err(error) = command.arg(url).spawn() {
        eprintln!("Cannot open link: {error}");
    }
}
