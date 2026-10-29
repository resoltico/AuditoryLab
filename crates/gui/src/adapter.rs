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
use auditorylab_app::vocabulary::{Operation, ViewCaption};
use auditorylab_app::workspace::ControlMenu;
use auditorylab_app::workspace::{ComponentView, EditorView, MetadataView, ParameterView};
use slint::winit_030::WinitWindowAccessor;

use auditorylab_presentation::numbers::NumberStyle;
mod actions;
mod auxiliary;
mod editing;
mod library;
mod projection;
mod wake;
use crate::generated::{
    AboutWindow, AudioControlsWindow, CatalogRow, ComponentRow, DetailsWindow, MainWindow,
    MetadataRow, ModelLane, MonitorWindow, ParameterRow,
};
use crate::{details_parent, motion, scope, typography};
use actions::install_actions;
use auditorylab_app::document::editing::{EditingField, FieldHandle, MetadataField, RowKey};
use auditorylab_app::services::playback::PlaybackPhase;
use auditorylab_app::workspace::{
    Command, Effect, Executor, LibraryCommand, MotionPreference, OutputCommand, PathPurpose,
    PromptAnswer, PromptId, View, Workspace,
};
use auxiliary::{install_auxiliary, install_monitor};
use editing::install_editing;
#[cfg(test)]
pub(crate) use projection::project_captions;
use projection::{read_only, sync_rows};
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
            crate::native_text::initialize_identity(&terms);
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
    control_choices: RefCell<Vec<auditorylab_app::workspace::ControlChoices>>,
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
            control_choices: RefCell::new(Vec::new()),
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
        ui.set_components(ModelRc::from(adapter.components.clone()));
        ui.set_catalog(ModelRc::from(adapter.catalog.clone()));
        ui.set_model_lanes(ModelRc::from(adapter.lanes.clone()));
        ui.set_transport_labels(ModelRc::new(VecModel::from(
            crate::labels::transport_labels()
                .map(SharedString::from)
                .to_vec(),
        )));
        ui.set_guidance(auditorylab_presentation::Notice::general_guidance().into());
        ui.set_native_mac(cfg!(target_os = "macos"));
        windows.about.set_identity(
            auditorylab_presentation::build_info::build_identity_about_version(
                auditorylab_app::identity::BUILD_IDENTITY,
            )
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

            monitor.invoke_sync_controls();
            if adapter.monitor_activation_pending.replace(false) {
                adapter.restore_main_after_monitor.set(false);
                monitor
                    .window()
                    .with_winit_window(slint::winit_030::winit::window::Window::focus_window);
            } else if adapter.restore_main_after_monitor.replace(false) {
                main.window()
                    .with_winit_window(slint::winit_030::winit::window::Window::focus_window);
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
            let _ = workspace.apply(Command::Answer {
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
        }
        self.after_command();
    }

    fn after_command(self: &Rc<Self>) {
        let effects = self
            .workspace
            .borrow_mut()
            .as_mut()
            .map(|workspace| {
                // Commands must refresh pure source models even when monitoring has
                // stopped the redraw timer. Worker ticks use this same completion path.
                workspace.poll();
                workspace.take_effects()
            })
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
        let title = auditorylab_presentation::display_scalar(&(title));
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
        self.sync_audio_controls(ui);
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
            auditorylab_presentation::workspace::desktop_diagnostics_text(
                &(auditorylab_presentation::DesktopDiagnostics {
                    player: &ui.get_player_title(),
                    generated: &ui.get_generated_details(),
                    monitor: &ui.get_monitor_state(),
                    device: &ui.get_device(),
                    status: &ui.get_status(),
                    auxiliary_error: &ui.get_auxiliary_error(),
                    motion: &ui.get_motion_reason(),
                    typography: &ui.get_typography_origin(),
                    model: &ui.get_model_caption(),
                }),
            )
            .into(),
        );
    }
}

// ---- callback installation ----

impl SlintAdapter {
    fn apply_waveform_choice(self: &Rc<Self>) {
        let Some(ui) = self.main.upgrade() else {
            return;
        };
        self.choose_control(
            auditorylab_app::workspace::ControlMenu::MonitorSpan,
            None,
            ui.get_window_index(),
        );
        self.choose_control(
            auditorylab_app::workspace::ControlMenu::WaveformScale,
            None,
            ui.get_scale_index(),
        );
    }

    fn choose_control(
        self: &Rc<Self>,
        menu: auditorylab_app::workspace::ControlMenu,
        row: Option<&str>,
        index: i32,
    ) {
        let command = self
            .control_choices
            .borrow()
            .iter()
            .find(|choices| {
                choices.menu == menu && row.is_none_or(|row| row == choices.row.to_string())
            })
            .and_then(|choices| {
                usize::try_from(index)
                    .ok()
                    .and_then(|i| choices.view.options.get(i))
                    .map(|option| Command::ChooseControl {
                        row: choices.row,
                        value: option.key,
                    })
            });
        if let Some(command) = command {
            self.command(|w| w.apply(command));
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

    window
        .window()
        .with_winit_window(slint::winit_030::winit::window::Window::focus_window);
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

impl SlintAdapter {
    fn sync_audio_controls(&self, ui: &MainWindow) {
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
    }
}

#[cfg(test)]
pub(crate) mod tests;
