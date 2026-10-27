//! The one binding between the Slint windows and the shared workspace.
//!
//! The adapter turns generated callbacks into workspace commands, performs the
//! workspace's effects through a [`Shell`], and projects workspace views into
//! persistent Slint models. Startup and the offscreen tests install exactly
//! this wiring through [`SlintAdapter::install`].
//!
//! Rendered fields carry application-owned opaque row keys. The editor validates
//! the document, component and generation, so late callbacks cannot address a
//! replacement document or a restored component lifetime.
use auditorylab_app::workspace::monitor::WaveformScale;
use auditorylab_app::workspace::{ComponentView, EditorView, MetadataView, ParameterView};
use auditorylab_presentation::DisplayText;
use auditorylab_presentation::LibraryPresentation;
use auditorylab_presentation::vocabulary::NumberStyle;
use auditorylab_presentation::waveform_view::*;
use auditorylab_presentation::*;
mod actions;
mod auxiliary;
mod editing;
mod library;
mod wake;
use crate::generated::{
    AboutWindow, AudioControlsWindow, CatalogRow, ComponentRow, DetailsWindow, MainWindow,
    MetadataKey, MetadataRow, ModelLane, MonitorWindow, ParameterKey, ParameterRow,
};
use crate::{details_parent, motion, scope, typography};
use actions::install_actions;
use auditorylab_app::core::{CarrierKind, Ears, ModulationKind, ParameterId};
use auditorylab_app::document::editing::{EditingField, FieldHandle, MetadataField, RowKey};
use auditorylab_app::services::playback::PlaybackPhase;
use auditorylab_app::workspace::{
    Command, Effect, Executor, LibraryCommand, MotionPreference, OutputCommand, PathPurpose,
    PromptAnswer, PromptId, Response, View, Workspace,
};
use auxiliary::{install_auxiliary, install_monitor};
use editing::install_editing;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
    time::Duration,
};
use {
    auditorylab_app::workspace::monitor::ModelView,
    auditorylab_app::workspace::monitor::WaveformCapture,
};

use auditorylab_app::workspace::controls::MONITOR_SPANS_MS as SPANS_MS;
/// Redraw cadence while a session is bound or the monitor is visible.
const ACTIVE_TICK: Duration = Duration::from_millis(50);

/// What the workspace asks of the platform: dialogs, file panels and the
/// answer to a close request. Answers return through [`SlintAdapter::respond`].
pub(crate) trait Shell {
    /// Ask whether to save, discard or keep unsaved changes.
    fn ask_unsaved(&self, adapter: &Rc<SlintAdapter>, prompt: PromptId);
    /// Confirm permanent deletion of an exact identity.
    fn ask_purge(&self, adapter: &Rc<SlintAdapter>, prompt: PromptId, key: String, title: String);
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
    pub(crate) licenses: crate::generated::LicenseWindow,
    pub(crate) monitor: MonitorWindow,
    pub(crate) details: DetailsWindow,
    pub(crate) audio_controls: AudioControlsWindow,
}

impl Windows {
    pub(crate) fn new() -> Result<Self, slint::PlatformError> {
        let windows = Self {
            main: MainWindow::new()?,
            about: AboutWindow::new()?,
            licenses: crate::generated::LicenseWindow::new()?,
            monitor: MonitorWindow::new()?,
            details: DetailsWindow::new()?,
            audio_controls: AudioControlsWindow::new()?,
        };
        for terms in [
            windows.main.global::<crate::generated::Terms>(),
            windows.about.global::<crate::generated::Terms>(),
        ] {
            crate::native_text::initialize_identity(terms);
        }
        Ok(windows)
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

/// One rendered component and its nested parameter model.
struct ComponentEntry {
    key: RowKey,
    parameters: Rc<VecModel<ParameterRow>>,
}

/// The adapter between Slint and the workspace.
pub(crate) struct SlintAdapter {
    workspace: RefCell<Option<Workspace>>,
    executor: RefCell<Option<Box<dyn Executor>>>,
    shell: Box<dyn Shell>,
    main: slint::Weak<MainWindow>,
    about: slint::Weak<AboutWindow>,
    licenses: slint::Weak<crate::generated::LicenseWindow>,
    monitor: slint::Weak<MonitorWindow>,
    details: slint::Weak<DetailsWindow>,
    audio_controls: slint::Weak<AudioControlsWindow>,
    components: Rc<VecModel<ComponentRow>>,
    entries: RefCell<Vec<ComponentEntry>>,
    catalog: Rc<VecModel<CatalogRow>>,
    library: RefCell<Option<Arc<auditorylab_app::workspace::LibraryView>>>,
    entry_handles: RefCell<Option<ModelRc<i32>>>,
    choices: RefCell<
        Vec<(
            auditorylab_app::workspace::QueryFacet,
            auditorylab_app::workspace::ChoiceView,
        )>,
    >,
    lanes: Rc<VecModel<ModelLane>>,
    document: Cell<Option<RowKey>>,
    projected: Cell<Option<(u64, Option<PlaybackPhase>)>>,
    capture: RefCell<Option<Arc<WaveformCapture>>>,
    model: RefCell<Option<Arc<ModelView>>>,
    updating: Cell<bool>,
    dialog_open: Cell<bool>,
    monitor_window_visible: Cell<bool>,
    monitor_activation_pending: Cell<bool>,
    monitor_requested: Cell<Option<bool>>,
    main_ready: Cell<bool>,
    restore_main_after_monitor: Cell<bool>,
    monitor_error: RefCell<String>,
    controls_error: RefCell<String>,
    audio_controls_visible: Cell<bool>,
    preferences: RefCell<Option<Preferences>>,
    wake: Arc<wake::Wake>,
    timer: slint::Timer,
    deferred: slint::Timer,
    #[cfg(target_os = "macos")]
    termination: RefCell<Option<slint::winit_030::winit::platform::macos::TerminationRequest>>,
    pub(crate) counts: ProjectionCounts,
}

/// Work the adapter did, for checking that unchanged state does none.
#[derive(Default)]
pub(crate) struct ProjectionCounts {
    pub(crate) document_projections: Cell<u64>,
    pub(crate) path_builds: Cell<u64>,
    pub(crate) library_projections: Cell<u64>,
}

impl SlintAdapter {
    /// Install the production bindings on `windows`, project the workspace
    /// and start polling.
    pub(crate) fn install(
        windows: &Windows,
        workspace: Workspace,
        shell: Box<dyn Shell>,
        mut executor: Box<dyn Executor>,
        preferences: Option<Preferences>,
    ) -> Rc<Self> {
        let ui = &windows.main;
        let wake = wake::Wake::new(ui);
        executor.set_waker(wake.callback());
        if let Some(preferences) = &preferences {
            preferences.typography.set_waker(wake.callback());
            preferences.motion.set_waker(wake.callback());
        }
        let adapter = Rc::new(Self {
            workspace: RefCell::new(Some(workspace)),
            executor: RefCell::new(Some(executor)),
            shell,
            main: ui.as_weak(),
            about: windows.about.as_weak(),
            licenses: windows.licenses.as_weak(),
            monitor: windows.monitor.as_weak(),
            details: windows.details.as_weak(),
            audio_controls: windows.audio_controls.as_weak(),
            components: Rc::new(VecModel::default()),
            entries: RefCell::new(Vec::new()),
            catalog: Rc::new(VecModel::default()),
            library: RefCell::new(None),
            entry_handles: RefCell::new(None),
            choices: RefCell::new(Vec::new()),
            lanes: Rc::new(VecModel::default()),
            document: Cell::new(None),
            projected: Cell::new(None),
            capture: RefCell::new(None),
            model: RefCell::new(None),
            updating: Cell::new(false),
            dialog_open: Cell::new(false),
            monitor_window_visible: Cell::new(false),
            monitor_activation_pending: Cell::new(false),
            monitor_requested: Cell::new(None),
            main_ready: Cell::new(false),
            restore_main_after_monitor: Cell::new(false),
            monitor_error: RefCell::new(String::new()),
            controls_error: RefCell::new(String::new()),
            audio_controls_visible: Cell::new(false),
            preferences: RefCell::new(preferences),
            wake,
            timer: slint::Timer::default(),
            deferred: slint::Timer::default(),
            #[cfg(target_os = "macos")]
            termination: RefCell::new(None),
            counts: ProjectionCounts::default(),
        });
        set_choices(ui, &windows.monitor);
        ui.set_window_index(index_of(
            &SPANS_MS,
            &auditorylab_app::workspace::controls::DEFAULT_MONITOR_SPAN_MS,
        ));
        ui.set_scale_index(index_of(
            &WaveformScale::ALL,
            &auditorylab_app::workspace::controls::DEFAULT_MONITOR_SCALE,
        ));
        ui.set_components(ModelRc::from(adapter.components.clone()));
        ui.set_catalog(ModelRc::from(adapter.catalog.clone()));
        ui.set_model_lanes(ModelRc::from(adapter.lanes.clone()));
        ui.set_guidance(auditorylab_presentation::Notice::general_guidance().into());
        ui.set_native_mac(cfg!(target_os = "macos"));
        windows.about.set_identity(
            auditorylab_app::identity::BUILD_IDENTITY
                .about_version()
                .into(),
        );
        windows
            .licenses
            .set_licenses(include_str!("../../../LICENSE").into());
        if let Some(preferences) = adapter.preferences.borrow().as_ref() {
            apply_font(windows, &preferences.font);
        }
        install_editing(&adapter, ui);
        install_actions(&adapter, ui);
        install_monitor(&adapter, windows);
        install_auxiliary(&adapter, windows);
        let weak = Rc::downgrade(&adapter);
        ui.on_worker_ready(move || {
            if let Some(adapter) = weak.upgrade() {
                adapter.tick();
            }
        });
        adapter.tick();
        adapter
    }

    /// Run `command` on the workspace, then perform its effects and project.
    /// Callbacks raised while projecting are not commands and are ignored.
    /// Show main and wait for its first native redraw, not a pre-resume timer.
    pub(crate) fn show_main(self: &Rc<Self>) -> Result<(), slint::PlatformError> {
        if let Some(main) = self.main.upgrade() {
            use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
            let weak = Rc::downgrade(self);
            main.window().on_winit_window_event(move |_, event| {
                if let Some(adapter) = weak.upgrade() {
                    adapter.main_window_event(event);
                }
                EventResult::Propagate
            });
            if let Some(monitor) = self.monitor.upgrade() {
                let weak = Rc::downgrade(self);
                monitor.window().on_winit_window_event(move |_, event| {
                    if matches!(event, winit::event::WindowEvent::RedrawRequested)
                        && let Some(adapter) = weak.upgrade()
                        && (adapter.monitor_activation_pending.get()
                            || adapter.restore_main_after_monitor.get())
                    {
                        adapter.monitor_frame_ready();
                    }
                    EventResult::Propagate
                });
            }
            main.show()?;
        }
        Ok(())
    }

    // Winit exposes one filter per window. Keep focus suspension and the first
    // frame gate together so installing either cannot replace the other.
    pub(crate) fn main_window_event(
        self: &Rc<Self>,
        event: &slint::winit_030::winit::event::WindowEvent,
    ) {
        use slint::winit_030::winit::event::WindowEvent;
        match event {
            WindowEvent::Focused(focused) => {
                if *focused {
                    self.command(|w| w.apply(Command::Library(LibraryCommand::Refresh)));
                }
                if let Some(main) = self.main.upgrade() {
                    main.set_edits_suspended(!focused);
                }
            }
            WindowEvent::RedrawRequested if !self.main_ready.get() => self.main_frame_ready(),
            _ => {}
        }
    }

    /// Reconcile after the first redraw has completed; offscreen hosts call
    /// this after their first software frame. No native focus policy is mocked.
    pub(crate) fn main_frame_ready(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(adapter) = weak.upgrade()
                && !adapter.main_ready.replace(true)
            {
                adapter.project();
            }
        });
    }

    /// Native backends may reveal a window on its first frame; complete
    /// activation/restoration only after that frame, not immediately after show.
    fn monitor_frame_ready(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        slint::Timer::single_shot(Duration::ZERO, move || {
            let Some(adapter) = weak.upgrade() else {
                return;
            };
            let (Some(main), Some(monitor)) = (adapter.main.upgrade(), adapter.monitor.upgrade())
            else {
                return;
            };
            if !main.get_monitor_open()
                || main.get_drawer_fits()
                || !adapter.monitor_window_visible.get()
            {
                return;
            }
            use slint::winit_030::WinitWindowAccessor;
            monitor.invoke_sync_controls();
            if adapter.monitor_activation_pending.replace(false) {
                adapter.restore_main_after_monitor.set(false);
                monitor
                    .window()
                    .with_winit_window(|native| native.focus_window());
            } else if adapter.restore_main_after_monitor.replace(false) {
                main.window()
                    .with_winit_window(|native| native.focus_window());
            }
        });
    }

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
    pub(crate) fn respond(self: &Rc<Self>, prompt: PromptId, response: PromptAnswer) {
        self.dialog_open.set(false);
        if let Some(workspace) = self.workspace.borrow_mut().as_mut() {
            workspace.respond(Response::Prompt {
                prompt,
                answer: response,
            });
        }
        self.after_command();
    }

    /// Poll tasks and native preferences, then project what changed.
    pub(crate) fn tick(self: &Rc<Self>) {
        if self.updating.get() {
            return;
        }
        self.drain_preferences();
        let responses = self
            .executor
            .borrow_mut()
            .as_mut()
            .map(|executor| executor.poll())
            .unwrap_or_default();
        if let Some(workspace) = self.workspace.borrow_mut().as_mut() {
            for response in responses {
                workspace.respond(response);
            }
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
            Effect::Run(job) => {
                if let Some(executor) = self.executor.borrow_mut().as_mut() {
                    executor.start(job);
                }
            }
            Effect::AskPurge { prompt, key, title } => {
                self.dialog_open.set(true);
                self.shell.ask_purge(self, prompt, key, title);
            }
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
            Effect::ResetLibraryInputs => {
                let updating = self.updating.replace(true);
                if let (Some(ui), Some(workspace)) =
                    (self.main.upgrade(), self.workspace.borrow().as_ref())
                {
                    actions::reset_filter_inputs(&ui, workspace);
                }
                self.updating.set(updating);
            }
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
    pub(crate) fn audio_control(
        &self,
    ) -> Option<auditorylab_app::services::playback::PlaybackControl> {
        self.workspace
            .borrow()
            .as_ref()
            .and_then(Workspace::audio_control)
    }

    /// The open document's title, shortened for a dialog.
    pub(crate) fn document_title(&self) -> String {
        let title = self
            .workspace
            .borrow()
            .as_ref()
            .map(|w| w.editor().draft().title().to_owned())
            .unwrap_or_default();
        let title = title.display_scalar();
        match title.char_indices().nth(60) {
            Some((cut, _)) => format!("{}…", &title[..cut]),
            None => title,
        }
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
        self.command(|w| w.apply(Command::Close));
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
                eprintln!(
                    "{}",
                    crate::native_text::InterfaceOperation::About.failure(error)
                );
            }
            about.invoke_focus_website();
        }
    }

    /// Stop polling, end every task and native listener, and report actual failures.
    pub(crate) fn shutdown(&self) -> Result<(), String> {
        self.wake.stop();
        self.timer.stop();
        self.deferred.stop();
        if let Some(mut preferences) = self.preferences.borrow_mut().take() {
            preferences.shutdown();
        }
        match (
            self.executor.borrow_mut().take(),
            self.workspace.borrow_mut().take(),
        ) {
            (Some(executor), Some(workspace)) => executor
                .shutdown(workspace)
                .map_err(|error| auditorylab_presentation::Explain::explain(&error)),
            _ => Ok(()),
        }
    }

    fn drain_preferences(&self) {
        let mut preferences = self.preferences.borrow_mut();
        let Some(preferences) = preferences.as_mut() else {
            return;
        };
        #[cfg(target_os = "linux")]
        if let Some(main) = self.main.upgrade() {
            preferences
                .typography
                .bind_window(main.window(), &preferences.font);
        }
        if let Some(font) = preferences.typography.latest()
            && preferences.font != font
        {
            eprintln!(
                "{}",
                crate::terms::Caption::typography(&font.family, font.size, &font.origin)
            );
            preferences.font = font;
            if let (
                Some(main),
                Some(about),
                Some(licenses),
                Some(monitor),
                Some(details),
                Some(controls),
            ) = (
                self.main.upgrade(),
                self.about.upgrade(),
                self.licenses.upgrade(),
                self.monitor.upgrade(),
                self.details.upgrade(),
                self.audio_controls.upgrade(),
            ) {
                apply_font(
                    &Windows {
                        main,
                        about,
                        licenses,
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
            let _ = workspace.apply(Command::Output(OutputCommand::SetMotion(match motion {
                motion::Preference::Reduced => MotionPreference::Reduced,
                motion::Preference::Full => MotionPreference::Enabled,
                motion::Preference::Unknown => MotionPreference::Unknown,
            })));
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

    fn schedule(self: &Rc<Self>) {
        let redraw = self
            .main
            .upgrade()
            .is_some_and(|main| main.get_monitor_open())
            || self.monitor_window_visible.get()
            || self.workspace.borrow().as_ref().is_some_and(|workspace| {
                workspace.audio_busy() && workspace.transport_view().monitor.is_some()
            });
        if redraw {
            if !self.timer.running() {
                let weak = Rc::downgrade(self);
                self.timer
                    .start(slint::TimerMode::Repeated, ACTIVE_TICK, move || {
                        if let Some(adapter) = weak.upgrade() {
                            adapter.tick();
                        }
                    });
            }
        } else {
            self.timer.stop();
        }
    }

    fn project_document(&self, ui: &MainWindow) {
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
        if let Some(workspace) = self.workspace.borrow().as_ref() {
            ui.set_draft_location(workspace.document_location_label().into());
            ui.set_draft_location_details(read_only(&workspace.document_location()));
        }
        ui.set_dirty(editor.dirty);
        let editor_text = auditorylab_presentation::workspace_text::editor_view(&editor);
        ui.set_draft_brief(read_only(&editor_text.brief));
        ui.set_peak(read_only(&editor_text.peak));
        ui.set_metadata_error(read_only(&editor_text.metadata_error));
        ui.set_based_on_title(read_only(&editor_text.based_on_title));
        ui.set_based_on(read_only(&editor_text.based_on));
        ui.set_retained_warning(read_only(&editor_text.retained_warning));

        self.project_library(ui, library);
        if ui.get_query() != self.query() {
            ui.set_query(self.query().into());
        }

        let action_text = auditorylab_presentation::workspace_text::action_view(&actions);
        ui.set_can_play(actions.can_play);
        ui.set_library_play_action(action_text.library_play_label.into());
        ui.set_can_import(actions.can_import);
        ui.set_can_manage_library(actions.can_manage_library);
        ui.set_can_add_to_library(actions.can_add_to_library);
        ui.set_can_request_trash(actions.can_request_trash);
        ui.set_can_new(actions.can_new);
        ui.set_can_copy(actions.can_copy);
        ui.set_copy_reason(read_only(&action_text.copy_reason));
        ui.set_operation_summary(read_only(&action_text.operation_summary));
        ui.set_operation_outcome(read_only(&action_text.operation_outcome));
        ui.set_can_preview(actions.can_preview);
        ui.set_play_reason(read_only(&action_text.play_reason));
        ui.set_can_save(actions.can_save);
        ui.set_can_save_copy(actions.can_save_copy);
        ui.set_can_export_recipe(actions.can_export_recipe);
        ui.set_save_reason(read_only(&action_text.save_reason));
        ui.set_can_export(actions.can_export);
        ui.set_export_reason(read_only(&action_text.export_reason));
        ui.set_can_edit(actions.can_edit);
        ui.set_can_add_component(actions.can_add_component);
        ui.set_can_add_pair(actions.can_add_pair);
        ui.set_pair_add_reason(read_only(&action_text.pair_add_reason));
        ui.set_add_reason(read_only(&action_text.add_component_reason));
        ui.set_removed_component(read_only(&action_text.removed_component));
        ui.set_can_restore_component(actions.can_restore_component);
        ui.set_playback_busy(actions.playback_busy);
        ui.set_exporting(actions.exporting);
        ui.set_transport_enabled(actions.transport_enabled);
        ui.set_transport_action(read_only(&action_text.transport_label));
        ui.set_play_target(read_only(&action_text.play_target));
        ui.set_file_activity(read_only(&action_text.file_activity));
        ui.set_status(read_only(&action_text.operation_details));
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
        let replaced = self.document.get().is_none_or(|key| key != editor.key);
        if replaced {
            // Rebinding: no automatic blur commit may reach the new document.
            let suspended = ui.get_edits_suspended();
            ui.set_edits_suspended(true);
            self.document.set(Some(editor.key));
            let mut entries = self.entries.borrow_mut();
            entries.clear();
            let rows: Vec<_> = editor
                .components
                .iter()
                .map(|component| {
                    let entry = Self::new_entry(component, editor);
                    let row = component_row(&entry, component, editor);
                    entries.push(entry);
                    row
                })
                .collect();
            self.components.set_vec(rows);
            set_metadata(ui, editor);
            ui.set_document(editor.key.to_string().into());
            ui.set_document_reset(ui.get_document_reset().wrapping_add(1));
            ui.set_editor_scroll_y(0.0);
            ui.set_edits_suspended(suspended);
            return;
        }
        set_metadata(ui, editor);
        let mut entries = self.entries.borrow_mut();
        let mut index = 0;
        while index < entries.len() {
            if editor
                .components
                .iter()
                .any(|c| c.key == entries[index].key)
            {
                index += 1;
            } else {
                entries.remove(index);
                self.components.remove(index);
            }
        }
        for (index, component) in editor.components.iter().enumerate() {
            match entries.iter().position(|e| e.key == component.key) {
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
                    let entry = Self::new_entry(component, editor);
                    self.components
                        .insert(index, component_row(&entry, component, editor));
                    entries.insert(index, entry);
                    continue;
                }
            }
            let entry = &entries[index];
            reconcile_parameters(&entry.parameters, &component.parameters, editor);
            let row = component_row(entry, component, editor);
            if self.components.row_data(index).as_ref() != Some(&row) {
                self.components.set_row_data(index, row);
            }
        }
    }

    fn new_entry(component: &ComponentView, editor: &EditorView) -> ComponentEntry {
        ComponentEntry {
            key: component.key,
            parameters: Rc::new(VecModel::from(
                component
                    .parameters
                    .iter()
                    .map(|field| parameter_row(field, editor))
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
        let transport_text = auditorylab_presentation::workspace_text::transport_view(&transport);
        ui.set_player_title(read_only(&transport_text.player_title));
        ui.set_elapsed(read_only(&transport_text.elapsed));
        ui.set_generated(read_only(&transport_text.generated));
        ui.set_generated_details(read_only(&transport_text.generated_details));
        ui.set_device(read_only(&transport_text.device));
        ui.set_output_state(transport_text.output_state.into());
        ui.set_receipt_age(read_only(&transport_text.receipt_age));
        ui.set_monitor_warning(read_only(&transport_text.warning));
        ui.set_monitor_warning_live(
            transport.output.unavailable.is_some()
                || transport
                    .monitor
                    .as_ref()
                    .and_then(|monitor| monitor.warning)
                    .is_some_and(
                        auditorylab_app::workspace::monitor::MonitorWarning::announces_changes,
                    ),
        );
        ui.set_backend_warning(read_only(&transport_text.backend_warning));
        ui.set_monitor_state(read_only(&transport_text.monitor_state));
        ui.set_meters(read_only(&transport_text.meters));
        ui.set_waveform_caption(read_only(&transport_text.caption));
        ui.set_model_caption(read_only(&transport_text.model_caption));
        ui.set_motion_reason(read_only(&transport_text.motion));
        ui.set_motion_overridden(transport.hold_override.is_some());
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
            self.counts
                .path_builds
                .set(self.counts.path_builds.get() + 1);
            if let Some(capture) = &output.capture {
                let [left, right] = scope::paths(capture.bins(), capture.limit());
                ui.set_left_path(left.into());
                ui.set_right_path(right.into());
                ui.set_scale_label(NumberStyle::AxisLimit.format(capture.limit()).into());
                ui.set_capture_duration(
                    NumberStyle::CapturedMilliseconds
                        .format(capture.milliseconds())
                        .into(),
                );
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
                                component: lane.id.get().to_string().into(),
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
                    crate::terms::Caption::typography_values(
                        &p.font.family,
                        p.font.size,
                        &p.font.origin,
                    )
                })
                .into(),
        );
    }

    /// Show the focused monitor window when the drawer does not fit, and the
    /// separate Stop window while a dialog may block the main window.
    fn sync_auxiliary(&self, ui: &MainWindow) {
        if !self.main_ready.get() || !ui.window().is_visible() {
            return;
        }
        let requested = ui.get_monitor_open();
        if self.monitor_requested.replace(Some(requested)) != Some(requested) {
            self.monitor_activation_pending.set(requested);
        }
        let focused_monitor = requested && !ui.get_drawer_fits();
        if let Some(window) = self.monitor.upgrade() {
            window.set_data(ui.get_monitor_data());
            // ComboBox initialization may write its local index; reapply the
            // authoritative projection without replacing widgets or firing actions.
            window.invoke_sync_controls();
            if focused_monitor
                && (!self.monitor_window_visible.get() || !window.window().is_visible())
            {
                use slint::winit_030::{WinitWindowAccessor, winit};
                let preserve_main = !self.monitor_activation_pending.get()
                    && ui
                        .window()
                        .with_winit_window(winit::window::Window::has_focus)
                        .unwrap_or(false);
                match window.show() {
                    Ok(()) => {
                        window.invoke_sync_controls();
                        self.monitor_window_visible.set(true);
                        self.monitor_error.borrow_mut().clear();
                        self.restore_main_after_monitor.set(preserve_main);
                    }
                    Err(error) => {
                        self.monitor_window_visible.set(false);
                        *self.monitor_error.borrow_mut() =
                            crate::native_text::InterfaceOperation::ShowMonitor.failure(error);
                    }
                }
            } else if !focused_monitor
                && (self.monitor_window_visible.get() || window.window().is_visible())
            {
                use slint::winit_030::{WinitWindowAccessor, winit};
                let was_focused = window
                    .window()
                    .with_winit_window(winit::window::Window::has_focus)
                    .unwrap_or(false);
                if let Some(details) = self.details.upgrade()
                    && details.window().is_visible()
                {
                    details_parent::bind(&details, ui);
                }
                match hide_monitor_window(ui, &window, was_focused) {
                    Ok(()) => {
                        self.monitor_window_visible.set(false);
                        self.monitor_error.borrow_mut().clear();
                    }
                    Err(error) => {
                        *self.monitor_error.borrow_mut() =
                            crate::native_text::InterfaceOperation::HideMonitor.failure(error);
                    }
                }
            }
            if !focused_monitor
                && !self.monitor_window_visible.get()
                && !window.window().is_visible()
            {
                self.monitor_error.borrow_mut().clear();
            }
            if !focused_monitor {
                self.monitor_activation_pending.set(false);
                self.restore_main_after_monitor.set(false);
            }
        }
        let audio = ui.get_playback_busy();
        let controls = self.dialog_open.get() && audio && !cfg!(target_os = "macos");
        if let Some(window) = self.audio_controls.upgrade() {
            window.set_source(ui.get_player_title());
            window.set_elapsed(ui.get_elapsed());
            window.set_active(audio);
            if controls != self.audio_controls_visible.get() {
                let result = if controls {
                    window.show()
                } else {
                    window.hide()
                };
                match result {
                    Ok(()) => {
                        self.audio_controls_visible.set(controls);
                        self.controls_error.borrow_mut().clear();
                    }
                    Err(error) => {
                        *self.controls_error.borrow_mut() =
                            crate::native_text::InterfaceOperation::AudioControls.failure(error);
                    }
                }
            }
        }
        ui.set_auxiliary_error(
            [
                self.monitor_error.borrow().as_str(),
                self.controls_error.borrow().as_str(),
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
            .into(),
        );
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
        window.set_heading(crate::terms::Caption::AudioSnapshot.label().into());
        window.set_can_refresh(true);
        window.set_content(
            auditorylab_presentation::DesktopDiagnostics {
                player: &ui.get_player_title(),
                generated: &ui.get_generated_details(),
                monitor: &ui.get_monitor_state(),
                device: &ui.get_device(),
                status: &ui.get_status(),
                auxiliary_error: &ui.get_auxiliary_error(),
                motion: &ui.get_motion_reason(),
                typography: &ui.get_typography_origin(),
                model: &ui.get_model_caption(),
            }
            .text()
            .into(),
        );
    }
}

fn parameter_handle(key: &str, parameter: ParameterKey) -> Option<FieldHandle> {
    key.parse::<RowKey>()
        .ok()?
        .parameter(parameter_id(parameter))
}
fn metadata_handle(key: &str, field: MetadataKey) -> Option<FieldHandle> {
    key.parse::<RowKey>().ok()?.metadata(metadata_field(field))
}

// ---- semantic binding conversions ----

// Generated from the owning parameter inventory by the build script.
include!(concat!(env!("OUT_DIR"), "/parameter_bindings.rs"));

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
            .map(|k| auditorylab_presentation::vocabulary::add_component(*k))
            .collect(),
    ));
    ui.set_add_binaural_label(
        auditorylab_presentation::vocabulary::add_pair(auditorylab_app::core::PairKind::Binaural)
            .into(),
    );
    ui.set_add_monaural_label(
        auditorylab_presentation::vocabulary::add_pair(auditorylab_app::core::PairKind::Monaural)
            .into(),
    );
    ui.set_modulation_choices(strings(
        auditorylab_app::workspace::controls::modulation_choices()
            .into_iter()
            .map(|kind| {
                kind.map_or(
                    auditorylab_app::core::vocabulary::NO_MODULATION.label,
                    ModulationKind::label,
                )
                .to_owned()
            })
            .collect(),
    ));
    let spans = strings(
        SPANS_MS
            .iter()
            .map(|ms| NumberStyle::Milliseconds.format(f64::from(*ms)))
            .collect(),
    );
    let scales = strings(
        WaveformScale::ALL
            .iter()
            .map(|scale| scale.label().to_owned())
            .collect(),
    );
    ui.set_span_choices(spans.clone());
    ui.set_scale_choices(scales.clone());
    monitor.set_span_choices(spans);
    monitor.set_scale_choices(scales);
    ui.set_transport_labels(strings(
        auditorylab_presentation::vocabulary::transport_labels()
            .iter()
            .map(|label| (*label).to_owned())
            .collect(),
    ));
    ui.set_export_rates(strings(
        Workspace::export_rates()
            .iter()
            .map(|rate| NumberStyle::Hertz.format(f64::from(rate.hz())))
            .collect(),
    ));
}

fn parameter_row(view: &ParameterView, editor: &EditorView) -> ParameterRow {
    let text = auditorylab_presentation::workspace_text::parameter_text(view, editor);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "A normalized slider position rounds to the renderer's f32; values stay binary64"
    )]
    let position = view.slider.unwrap_or(0.0) as f32;
    ParameterRow {
        key: parameter_key(view.parameter),
        label: text.label.as_str().into(),
        help: text.help.into(),
        text: view.text.as_str().into(),
        staged: view.staged,
        has_slider: view.slider.is_some(),
        position,
        diagnostic: text.diagnostic.as_str().into(),
        show_diagnostic: !text.diagnostic.is_empty(),
    }
}

fn component_row(
    entry: &ComponentEntry,
    view: &ComponentView,
    editor: &EditorView,
) -> ComponentRow {
    let text = auditorylab_presentation::workspace_text::component_text(view, editor);
    ComponentRow {
        key: entry.key.to_string().into(),
        id: view.id.get().to_string().into(),
        ears: index_of(&Ears::ALL, &view.committed.route().unwrap_or(Ears::Both)),
        carrier: view
            .committed
            .primitive_kind()
            .map_or(-1, |kind| index_of(&CarrierKind::ALL, &kind)),
        routing_editable: view.committed.route().is_some(),
        modulation: index_of(
            &auditorylab_app::workspace::controls::modulation_choices(),
            &view.committed.modulation_kind(),
        ),
        heading: text.heading.into(),
        name: text.name.as_str().into(),
        context: text.context.as_str().into(),
        error: text.error.as_str().into(),
        parameters: ModelRc::from(entry.parameters.clone()),
    }
}

/// Reconcile one component's parameters by key with minimal notifications.
fn reconcile_parameters(
    model: &VecModel<ParameterRow>,
    views: &[ParameterView],
    editor: &EditorView,
) {
    let mut rows: Vec<_> = views
        .iter()
        .map(|field| parameter_row(field, editor))
        .collect();
    let mut shown = std::collections::BTreeSet::new();
    for row in rows.iter_mut().rev() {
        row.show_diagnostic = !row.diagnostic.is_empty() && shown.insert(row.diagnostic.clone());
    }
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

fn metadata_row(view: &MetadataView, editor: &EditorView) -> MetadataRow {
    let text = auditorylab_presentation::workspace_text::metadata_text(view, editor);
    MetadataRow {
        label: text.label.as_str().into(),
        text: view.text.as_str().into(),
        staged: view.staged,
        diagnostic: text.diagnostic.as_str().into(),
    }
}

fn set_metadata(ui: &MainWindow, editor: &EditorView) {
    for view in &editor.metadata {
        let EditingField::Metadata(field) = view.handle.field else {
            continue;
        };
        let row = metadata_row(view, editor);
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

impl SlintAdapter {
    fn apply_waveform_choice(self: &Rc<Self>) {
        let Some(ui) = self.main.upgrade() else {
            return;
        };
        let span = choice(&SPANS_MS, ui.get_window_index());
        let scale = choice(&WaveformScale::ALL, ui.get_scale_index());
        if let (Some(span), Some(scale)) = (span, scale) {
            self.command(|w| {
                w.apply(Command::Output(OutputCommand::SetWaveform {
                    span_ms: span,
                    scale,
                }))
            });
        } else {
            self.project();
        }
    }
}

fn apply_font(windows: &Windows, font: &typography::Typography) {
    let family: SharedString = font.family.as_str().into();
    windows.main.set_ui_family(family.clone());
    windows.main.set_ui_size(font.size);
    windows.about.set_ui_size(font.size);
    windows.licenses.set_ui_family(family.clone());
    windows.licenses.set_ui_size(font.size);
    windows.monitor.set_ui_family(family.clone());
    windows.monitor.set_ui_size(font.size);
    windows.details.set_ui_family(family.clone());
    windows.details.set_ui_size(font.size);
    windows.audio_controls.set_ui_family(family);
    windows.audio_controls.set_ui_size(font.size);
}

fn present_details<C: ComponentHandle + 'static>(window: &DetailsWindow, parent: &C) {
    if let Err(error) = window.show() {
        eprintln!(
            "{}",
            crate::native_text::InterfaceOperation::Details.failure(error)
        );
        return;
    }
    details_parent::bind(window, parent);
    // show() is a no-op for an already-shown modeless reader. This is an
    // explicit user request, unlike automatic monitor reflow.
    use slint::winit_030::WinitWindowAccessor;
    window
        .window()
        .with_winit_window(|native| native.focus_window());
    window.invoke_focus_close();
}

pub(crate) fn hide_monitor_window(
    ui: &MainWindow,
    monitor: &MonitorWindow,
    was_focused: bool,
) -> Result<(), slint::PlatformError> {
    monitor.hide()?;
    if was_focused {
        ui.invoke_focus_monitor_opener();
    }
    Ok(())
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
        eprintln!(
            "{}",
            crate::native_text::InterfaceOperation::OpenLink.failure(error)
        );
    }
}

// Keep display escaping separate from lossless editable field models and bindings.
fn read_only(text: &str) -> SharedString {
    (text).display_text().into()
}

#[cfg(test)]
#[path = "adapter_test_support.rs"]
pub(crate) mod tests;
