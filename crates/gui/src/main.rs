//! Desktop interface to Auditory Lab.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod app;
mod scope;
mod worker;
#[allow(
    unsafe_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::todo,
    reason = "Slint generated code"
)]
mod generated {
    slint::include_modules!();
}
use app::{App, Operation};
use generated::{AboutWindow, MainWindow, Row};
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
use worker::{Message, Work, Worker};
struct Desktop {
    updating: Cell<bool>,
    catalog_dirty: Cell<bool>,
    app: RefCell<App>,
    worker: RefCell<Option<Worker>>,
    ui: slint::Weak<MainWindow>,
    timer: slint::Timer,
    bins: RefCell<Vec<[auditorylab::Extent; 2]>>,
    rows: Rc<VecModel<Row>>,
}
impl Desktop {
    fn refresh(&self) -> Duration {
        let Some(ui) = self.ui.upgrade() else {
            return Duration::from_millis(50);
        };
        self.updating.set(true);
        let mut done = false;
        if let Some(worker) = self.worker.borrow().as_ref() {
            loop {
                let message = match worker.receive() {
                    Ok(Some(message)) => message,
                    Ok(None) => break,
                    Err(error) => {
                        if !done {
                            self.app.borrow_mut().status = error;
                        }
                        done = true;
                        break;
                    }
                };
                let mut app = self.app.borrow_mut();
                match message {
                    Message::Ready(info, sound) => {
                        app.status = format!(
                            "{} · {} Hz · {} channels · {}",
                            info.name.unwrap_or_else(|| "Default output".into()),
                            info.sample_rate.hz(),
                            info.channels,
                            info.format
                        );
                        app.playback = Some(sound);
                    }
                    Message::LiveReady(info, control) => {
                        app.status = format!(
                            "{} · {} Hz · {} channels · {}",
                            info.name.unwrap_or_else(|| "Default output".into()),
                            info.sample_rate.hz(),
                            info.channels,
                            info.format
                        );
                        let rate = app.rate;
                        app.workbench.ready(control, rate);
                    }
                    Message::Imported(result) => {
                        self.catalog_dirty.set(app.imported(result));
                        done = true;
                    }
                    Message::Done(result) => {
                        app.status = match result {
                            Ok(text) => text,
                            Err(e) if e.kind() == auditorylab::ErrorKind::Cancelled => {
                                "Stopped.".into()
                            }
                            Err(e) => e.to_string(),
                        };
                        done = true;
                    }
                }
            }
        }
        if done {
            if let Some(worker) = self.worker.borrow_mut().take()
                && let Err(e) = worker.join()
            {
                self.app.borrow_mut().status = e;
            }
            self.app.borrow_mut().finish();
        }
        let app = self.app.borrow();
        if app.closing && app.operation.is_none() {
            let _ = slint::quit_event_loop();
            return Duration::from_millis(50);
        }
        if self.catalog_dirty.replace(false) {
            ui.set_catalog(ModelRc::new(VecModel::from(
                app.presets
                    .iter()
                    .map(|p| slint::StandardListViewItem::from(app::label(p).as_str()))
                    .collect::<Vec<_>>(),
            )));
        }
        ui.set_selected(i32::try_from(app.selected).unwrap_or(0));
        ui.set_sound_title(app.preset().title().into());
        ui.set_studied(
            app.preset()
                .about()
                .and_then(|a| a.studied_for.as_deref())
                .unwrap_or("")
                .into(),
        );
        ui.set_summary(
            app.preset()
                .about()
                .and_then(|a| a.summary.as_deref())
                .unwrap_or("")
                .into(),
        );
        ui.set_details(app::details(app.preset(), app.rate).into());
        ui.set_busy(app.operation.is_some());
        ui.set_live(app.operation == Some(Operation::Live));
        ui.set_tab_index(i32::from(app.editing));
        ui.set_draft_title(app.workbench.draft.title().into());
        ui.set_draft_summary(app.workbench.draft.summary().into());
        ui.set_duration(app.workbench.draft.duration_seconds().to_string().into());
        ui.set_fade(app.workbench.draft.fade_seconds().to_string().into());
        let mut metadata_errors = app
            .workbench
            .metadata_errors
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let timing = auditorylab::Stimulus {
            duration_seconds: app.workbench.draft.duration_seconds(),
            fade_seconds: app.workbench.draft.fade_seconds(),
            components: Vec::new(),
        };
        if let Err(error) = timing.signal(app.rate) {
            metadata_errors.push(error.to_string());
        }
        ui.set_metadata_error(metadata_errors.join("; ").into());
        ui.set_peak(app.workbench.peak.as_str().into());
        self.sync_rows(&app);
        let validation = if app.editing {
            app.workbench.draft.preset().at(app.rate)
        } else {
            app.preset().at(app.rate)
        };
        ui.set_can_export(
            (validation.is_ok() && (!app.editing || app.workbench.can_save()))
                || (app.editing && app.workbench.has_pending()),
        );
        ui.set_can_save(app.workbench.can_save() || app.workbench.has_pending());
        ui.set_can_play(if app.editing {
            app.workbench.has_pending()
                || app.workbench.parse_errors.is_empty()
                    && app
                        .workbench
                        .draft
                        .target(app.workbench.rate(app.rate))
                        .is_ok()
        } else {
            validation.is_ok()
        });
        let status = if app.editing {
            app.status.clone()
        } else {
            match validation {
                Ok(_) => app.status.clone(),
                Err(error) if app.status.is_empty() => error.to_string(),
                Err(error) => format!("{}\n{error}", app.status),
            }
        };
        ui.set_status(status.into());
        let started = Instant::now();
        let preview = if app.editing {
            app.workbench.preview.clone()
        } else {
            app.sound().ok()
        };
        if let Some(sound) = preview {
            let signal = sound.signal();
            let belongs = app.operation.is_some() && app.operation_editing == app.editing;
            let position = if belongs {
                self.worker
                    .borrow()
                    .as_ref()
                    .map_or(0, |w| w.progress.frames())
            } else {
                0
            };
            let elapsed = f64::from(
                u32::try_from(position / u64::from(signal.sample_rate().hz())).unwrap_or(u32::MAX),
            );
            let live = app.editing && app.operation == Some(Operation::Live);
            ui.set_elapsed(
                if live {
                    auditorylab::human_duration(elapsed)
                } else {
                    format!(
                        "{} / {}",
                        auditorylab::human_duration(elapsed),
                        auditorylab::human_duration(if app.editing {
                            app.workbench.draft.duration_seconds()
                        } else {
                            signal.duration_seconds()
                        })
                    )
                }
                .into(),
            );
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                reason = "bounded presentation ratio"
            )]
            let progress = (position as f64
                / (if app.editing {
                    app.workbench.draft.duration_seconds() * f64::from(app.rate.hz())
                } else {
                    signal.frames() as f64
                })) as f32;
            ui.set_progress(if live || !progress.is_finite() {
                0.0
            } else {
                progress.clamp(0.0, 1.0)
            });
            let ms =
                [2_u64, 10, 40, 100][usize::try_from(ui.get_window_index()).unwrap_or(2).min(3)];
            let frames = ms * u64::from(signal.sample_rate().hz()) / 1000;
            let start = app::scope_start(
                &sound,
                belongs && matches!(app.operation, Some(Operation::Play | Operation::Live)),
                position,
                frames,
            );
            let visible =
                if belongs && matches!(app.operation, Some(Operation::Play | Operation::Live)) {
                    frames.min(position)
                } else {
                    frames
                };
            auditorylab::trace(signal, start, visible, 512, &mut self.bins.borrow_mut());
            let [left, right] = scope::paths(&self.bins.borrow());
            ui.set_left_path(left.into());
            ui.set_right_path(right.into());
        } else {
            ui.set_left_path("".into());
            ui.set_right_path("".into());
            ui.set_elapsed("".into());
            ui.set_progress(0.0);
        }
        self.updating.set(false);
        Duration::from_millis(50).max(started.elapsed() * 20)
    }
    fn reset_editors(&self) {
        self.rows.set_vec(Vec::new());
        if let Some(ui) = self.ui.upgrade() {
            ui.set_draft_reset(ui.get_draft_reset().wrapping_add(1));
            ui.set_editor_scroll_y(0.0);
        }
    }
    fn sync_rows(&self, app: &App) {
        let ids: Vec<_> = app
            .workbench
            .draft
            .components()
            .iter()
            .map(|(id, _)| id.get().to_string())
            .collect();
        let mut index = 0;
        while index < self.rows.row_count() {
            if !ids.iter().any(|id| {
                self.rows
                    .row_data(index)
                    .is_some_and(|row| row.id.as_str() == id)
            }) {
                self.rows.remove(index);
            } else {
                index += 1;
            }
        }
        for (index, (id, c)) in app.workbench.draft.components().iter().enumerate() {
            let rate = app.workbench.rate(app.rate);
            let mut row = Row {
                id: id.get().to_string().into(),
                number: i32::try_from(index + 1).unwrap_or(16),
                ears: match c.ears {
                    auditorylab::Ears::Both => 0,
                    auditorylab::Ears::Left => 1,
                    auditorylab::Ears::Right => 2,
                },
                source: i32::from(matches!(c.carrier, auditorylab::Carrier::Click { .. })),
                modulation: match c.modulation {
                    None => 0,
                    Some(auditorylab::Modulation::Sinusoidal { .. }) => 1,
                    Some(auditorylab::Modulation::Rectangular { .. }) => 2,
                },
                error: app
                    .workbench
                    .messages
                    .get(id)
                    .map_or("", String::as_str)
                    .into(),
                ..Row::default()
            };
            row.text0 = app::value(c, auditorylab::Setting::ALL[0])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos0 = position(
                auditorylab::Setting::ALL[0],
                rate,
                app::value(c, auditorylab::Setting::ALL[0]).unwrap_or(0.0),
            );
            row.text1 = app::value(c, auditorylab::Setting::ALL[1])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos1 = position(
                auditorylab::Setting::ALL[1],
                rate,
                app::value(c, auditorylab::Setting::ALL[1]).unwrap_or(0.0),
            );
            row.text2 = app::value(c, auditorylab::Setting::ALL[2])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos2 = position(
                auditorylab::Setting::ALL[2],
                rate,
                app::value(c, auditorylab::Setting::ALL[2]).unwrap_or(0.0),
            );
            row.text3 = app::value(c, auditorylab::Setting::ALL[3])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos3 = position(
                auditorylab::Setting::ALL[3],
                rate,
                app::value(c, auditorylab::Setting::ALL[3]).unwrap_or(0.0),
            );
            row.text4 = app::value(c, auditorylab::Setting::ALL[4])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos4 = position(
                auditorylab::Setting::ALL[4],
                rate,
                app::value(c, auditorylab::Setting::ALL[4]).unwrap_or(0.0),
            );
            row.text5 = app::value(c, auditorylab::Setting::ALL[5])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos5 = position(
                auditorylab::Setting::ALL[5],
                rate,
                app::value(c, auditorylab::Setting::ALL[5]).unwrap_or(0.0),
            );
            row.text6 = app::value(c, auditorylab::Setting::ALL[6])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos6 = position(
                auditorylab::Setting::ALL[6],
                rate,
                app::value(c, auditorylab::Setting::ALL[6]).unwrap_or(0.0),
            );
            row.text7 = app::value(c, auditorylab::Setting::ALL[7])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos7 = position(
                auditorylab::Setting::ALL[7],
                rate,
                app::value(c, auditorylab::Setting::ALL[7]).unwrap_or(0.0),
            );
            if index < self.rows.row_count() {
                if self.rows.row_data(index).as_ref() != Some(&row) {
                    self.rows.set_row_data(index, row);
                }
            } else {
                self.rows.push(row);
            }
        }
    }
    fn edit(self: &Rc<Self>, id: &str, setting: i32, text: &str) {
        let mut app = self.app.borrow_mut();
        if app.operation.is_some() && app.operation != Some(Operation::Live) {
            return;
        }
        let Some(setting) = usize::try_from(setting)
            .ok()
            .and_then(|i| auditorylab::Setting::ALL.get(i))
            .copied()
        else {
            return;
        };
        if let Some(id) = app.workbench.id(id) {
            let rate = app.rate;
            app.workbench.edit(id, setting, text, rate);
        }
        drop(app);
        self.refresh();
    }
    fn schedule(self: &Rc<Self>) {
        let interval = self.refresh();
        if self.app.borrow().operation.is_some() {
            let this = Rc::clone(self);
            self.timer
                .start(slint::TimerMode::SingleShot, interval, move || {
                    this.schedule()
                });
        }
    }
    fn start(self: &Rc<Self>, op: Operation, work: Work) {
        if self.app.borrow_mut().start(op) {
            *self.worker.borrow_mut() = Some(Worker::start(work));
            self.schedule();
        }
    }
}
#[allow(
    clippy::cast_possible_truncation,
    reason = "normalized slider positions fit f32"
)]
fn position(setting: auditorylab::Setting, rate: auditorylab::SampleRate, value: f64) -> f32 {
    app::slider_position(setting, rate, value) as f32
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args == ["--version"] {
        println!("{}", auditorylab::version_line("auditorylab-gui"));
        return Ok(());
    }
    if !args.is_empty() {
        return Err("Use auditorylab-gui without arguments or with --version.".into());
    }
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("femtovg".into())
        .select()?;
    let ui = MainWindow::new()?;
    ui.set_guidance(
        format!(
            "{} {}",
            auditorylab::Notice::LowStartingVolume,
            auditorylab::Notice::HeadphonesForSeparateEars
        )
        .into(),
    );
    let about = AboutWindow::new()?;
    about.set_identity(format!("{}\n\nhttps://github.com/resoltico/AuditoryLab\n\nAuditory Lab: MPL-2.0\nSlint: Royalty-free Desktop, Mobile, and Web Applications License 2.0\nThird-party licenses: THIRD-PARTY-NOTICES.txt in the download.",auditorylab::version_line("auditorylab-gui")).into());
    let desktop = Rc::new(Desktop {
        updating: Cell::new(false),
        catalog_dirty: Cell::new(true),
        app: RefCell::new(App::new(auditorylab::catalog()?)),
        worker: RefCell::new(None),
        ui: ui.as_weak(),
        timer: slint::Timer::default(),
        bins: RefCell::new(Vec::new()),
        rows: Rc::new(VecModel::default()),
    });
    ui.set_rows(ModelRc::from(desktop.rows.clone()));
    let d = desktop.clone();
    ui.on_select(move |index| {
        if d.updating.get() {
            return;
        }
        if let Ok(index) = usize::try_from(index) {
            d.app.borrow_mut().select(index);
            d.refresh();
        }
    });
    let d = desktop.clone();
    ui.on_rate_changed(move |index| {
        let rate = [44100, 48000, 88200, 96000, 192000][usize::try_from(index).unwrap_or(1).min(4)];
        if let Ok(rate) = auditorylab::SampleRate::new(rate) {
            let mut app = d.app.borrow_mut();
            app.rate = rate;
            app.workbench.refresh(rate);
            drop(app);
            d.refresh();
        }
    });
    let d = desktop.clone();
    ui.on_window_changed(move || {
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_play_stop(move || {
        if let Some(worker) = d.worker.borrow().as_ref() {
            worker.cancel.request();
            return;
        }
        let mut app = d.app.borrow_mut();
        let rate = app.rate;
        app.workbench.commit_pending(rate);
        if app.editing
            && (!app.workbench.parse_errors.is_empty() || app.workbench.draft.target(rate).is_err())
        {
            drop(app);
            d.refresh();
            return;
        }
        let (operation, work) = if app.editing {
            (Operation::Live, Work::Live(app.workbench.draft.clone()))
        } else {
            (Operation::Play, Work::Play(app.preset().clone()))
        };
        drop(app);
        d.start(operation, work);
    });
    let d = desktop.clone();
    ui.on_open_recipe(move || {
        if d.app.borrow().operation.is_some() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("TOML recipe", &["toml"])
            .pick_file()
        {
            d.start(Operation::Import, Work::Import(path));
        } else {
            d.app.borrow_mut().status =
                "No recipe selected. If no dialog opened, use auditorylab inspect --file PATH."
                    .into();
            d.refresh();
        }
    });
    let d = desktop.clone();
    ui.on_export_wav(move || {
        if d.app.borrow().operation.is_some() {
            return;
        }
        let mut app = d.app.borrow_mut();
        let rate = app.rate;
        app.workbench.commit_pending(rate);
        if app.editing && !app.workbench.can_save() {
            drop(app);
            d.refresh();
            return;
        }
        let name = match (app.editing, app.preset().origin()) {
            (false, auditorylab::Origin::BuiltIn(id)) => format!("{id}.wav"),
            _ => "sound.wav".into(),
        };
        let sound = if app.editing {
            app.workbench.draft.preset().at(app.rate)
        } else {
            app.preset().at(app.rate)
        };
        drop(app);
        let sound = match sound {
            Ok(sound) => sound,
            Err(error) => {
                d.app.borrow_mut().status = error.to_string();
                d.refresh();
                return;
            }
        };
        d.refresh();
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(name)
            .add_filter("WAV audio", &["wav"])
            .save_file()
        {
            d.start(Operation::Export, Work::Export(sound, path));
        }
    });
    let d = desktop.clone();
    ui.on_tab_changed(move |index| {
        if !d.updating.get() {
            d.app.borrow_mut().editing = index == 1;
            d.refresh();
        }
    });
    let d = desktop.clone();
    ui.on_open_workbench(move || {
        let replace = {
            let app = d.app.borrow();
            app.operation.is_none() && app.preset().stimulus().components.len() <= 16
        };
        if replace {
            d.reset_editors();
        }
        d.app.borrow_mut().open_workbench();
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_new_draft(move || {
        let mut app = d.app.borrow_mut();
        if app.operation.is_some() {
            return;
        }
        d.reset_editors();
        app.workbench = app::Workbench::new(auditorylab::Draft::new(), app.rate);
        app.editing = true;
        drop(app);
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_add_component(move |kind| {
        let mut app = d.app.borrow_mut();
        if app.operation.is_some() && app.operation != Some(Operation::Live) {
            return;
        }
        if let Err(e) = app.workbench.draft.add(if kind == 1 {
            auditorylab::SourceKind::Clicks
        } else {
            auditorylab::SourceKind::Tone
        }) {
            app.status = e.to_string();
        }
        let rate = app.rate;
        app.workbench.refresh(rate);
        drop(app);
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_remove_component(move |text| {
        let mut app = d.app.borrow_mut();
        if app.operation.is_some() && app.operation != Some(Operation::Live) {
            return;
        }
        if let Some(id) = app.workbench.id(&text) {
            let rate = app.rate;
            app.workbench.remove(id, rate);
        }
        drop(app);
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_choice(move |text, choice, index| {
        if d.updating.get() {
            return;
        }
        let mut app = d.app.borrow_mut();
        if app.operation.is_some() && app.operation != Some(Operation::Live) {
            return;
        }
        if let Some(id) = app.workbench.id(&text) {
            let rate = app.rate;
            app.workbench.choice(id, choice, index, rate);
        }
        drop(app);
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_edit(move |id, setting, text| {
        d.edit(&id, setting, &text);
    });
    let d = desktop.clone();
    ui.on_slide(move |id, setting, position| {
        if d.updating.get() {
            return;
        }
        if let Some(setting_value) = usize::try_from(setting)
            .ok()
            .and_then(|i| auditorylab::Setting::ALL.get(i))
            .copied()
        {
            let app = d.app.borrow();
            let value = app::slider_value(
                setting_value,
                app.workbench.rate(app.rate),
                f64::from(position),
            );
            drop(app);
            d.edit(&id, setting, &value.to_string());
        }
    });
    let d = desktop.clone();
    ui.on_metadata(move |key, text| {
        let mut app = d.app.borrow_mut();
        if app.operation.is_some() && app.operation != Some(Operation::Live) {
            return;
        }
        app.workbench.metadata(&key, &text);
        drop(app);
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_stage(move |text, setting, value| {
        let mut app = d.app.borrow_mut();
        if app.operation.is_some() && app.operation != Some(Operation::Live) {
            return;
        }
        if let Some(id) = app.workbench.id(&text)
            && let Some(setting) = usize::try_from(setting)
                .ok()
                .and_then(|i| auditorylab::Setting::ALL.get(i))
                .copied()
        {
            app.workbench
                .pending
                .insert((id, setting), value.to_string());
        }
        drop(app);
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_stage_metadata(move |key, value| {
        let mut app = d.app.borrow_mut();
        if app.operation.is_none() || app.operation == Some(Operation::Live) {
            app.workbench
                .pending_metadata
                .insert(key.to_string(), value.to_string());
        }
        drop(app);
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_save_recipe(move || {
        let mut app = d.app.borrow_mut();
        if app.operation.is_some() {
            return;
        }
        let rate = app.rate;
        app.workbench.commit_pending(rate);
        if !app.workbench.can_save() {
            drop(app);
            d.refresh();
            return;
        }
        let draft = app.workbench.draft.clone();
        drop(app);
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("sound.toml")
            .add_filter("TOML recipe", &["toml"])
            .save_file()
        {
            d.start(Operation::Save, Work::Save(draft, path));
        }
    });
    let weak = about.as_weak();
    ui.on_about(move || {
        if let Some(about) = weak.upgrade() {
            let _ = about.show();
        }
    });
    let d = desktop.clone();
    ui.window().on_close_requested(move || {
        if let Some(worker) = d.worker.borrow().as_ref() {
            worker.cancel.request();
            d.app.borrow_mut().closing = true;
            slint::CloseRequestResponse::KeepWindowShown
        } else {
            slint::CloseRequestResponse::HideWindow
        }
    });
    desktop.refresh();
    ui.run()?;
    Ok(())
}
