//! Desktop interface to Auditory Lab.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod actions;
mod app;
mod dialogs;
mod motion;
#[cfg(target_os = "macos")]
mod native_quit;
mod scope;
mod worker;
use actions::{Action, Continuation};
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
    time::Duration,
};
use worker::{Message, Work, Worker};
const EXPORT_RATES: [u32; 5] = [44100, 48000, 88200, 96000, 192000];
struct Desktop {
    updating: Cell<bool>,
    catalog_dirty: Cell<bool>,
    app: RefCell<App>,
    worker: RefCell<Option<Worker>>,
    ui: slint::Weak<MainWindow>,
    timer: slint::Timer,
    bins: RefCell<Vec<[auditorylab::Extent; 2]>>,
    rows: Rc<VecModel<Row>>,
    monitor: RefCell<Option<auditorylab::MonitorReader>>,
    catalog_indices: RefCell<Vec<usize>>,
    query: RefCell<String>,
    after: RefCell<Option<Continuation>>,
    session_position: Cell<u64>,
    session_title: RefCell<String>,
    frozen_span: Cell<Option<(u64, u64)>>,
    dialog_active: Cell<bool>,
    operation_frames: Cell<u64>,
}
impl Desktop {
    fn refresh(self: &Rc<Self>) -> Duration {
        let Some(ui) = self.ui.upgrade() else {
            return Duration::from_millis(50);
        };
        self.updating.set(true);
        let mut done = false;
        let mut success = true;
        if let Some(worker) = self.worker.borrow().as_ref() {
            loop {
                let message = match worker.receive() {
                    Ok(Some(message)) => message,
                    Ok(None) => break,
                    Err(error) => {
                        if !done {
                            self.app.borrow_mut().status = error;
                            success = false;
                        }
                        done = true;
                        break;
                    }
                };
                let mut app = self.app.borrow_mut();
                match message {
                    Message::Ready(reader) => {
                        *self.monitor.borrow_mut() = Some(reader);
                        self.frozen_span.set(None);
                        ui.set_left_path("".into());
                        ui.set_right_path("".into());
                    }
                    Message::LiveReady(control, reader) => {
                        let rate = app.rate;
                        app.workbench.ready(control, rate);
                        *self.monitor.borrow_mut() = Some(reader);
                        self.frozen_span.set(None);
                        ui.set_left_path("".into());
                        ui.set_right_path("".into());
                    }
                    Message::Imported(result) => {
                        if app.imported(result) {
                            self.catalog_dirty.set(true);
                            *self.query.borrow_mut() = String::new();
                            ui.set_query("".into());
                        }
                        done = true;
                    }
                    Message::Saved(result) => {
                        match result {
                            Ok((draft, path)) => {
                                let invalid = draft.preset().at(app.rate).is_err();
                                app.workbench.saved = draft;
                                app.status = format!(
                                    "{} to {path:?}.",
                                    if invalid {
                                        "Saved with validation errors"
                                    } else {
                                        "Saved recipe"
                                    }
                                );
                            }
                            Err(error) => {
                                app.status = error.to_string();
                                success = false;
                            }
                        }
                        done = true;
                    }
                    Message::Done(result) => {
                        app.status = match result {
                            Ok(text) => text,
                            Err(error) if error.kind() == auditorylab::ErrorKind::Cancelled => {
                                "Stopped.".into()
                            }
                            Err(error) => {
                                success = false;
                                error.to_string()
                            }
                        };
                        done = true;
                    }
                }
            }
        }
        if done {
            if let Some(worker) = self.worker.borrow_mut().take()
                && let Err(error) = worker.join()
            {
                self.app.borrow_mut().status = error;
                success = false;
            }
            self.app.borrow_mut().finish();
            let after = self.after.borrow_mut().take();
            if success && let Some(after) = after {
                self.continue_with(after);
            } else if !success {
                reply_native_quit(false);
            }
        }
        let app = self.app.borrow();
        if self.catalog_dirty.replace(false) {
            let query = self.query.borrow();
            let indices: Vec<_> = app
                .presets
                .iter()
                .enumerate()
                .filter(|(_, p)| auditorylab::catalog_matches(p, &query))
                .map(|(i, _)| i)
                .collect();
            ui.set_catalog(ModelRc::new(VecModel::from(
                indices
                    .iter()
                    .map(|&i| {
                        slint::StandardListViewItem::from(
                            app::label(&app.presets[i], app.rate).as_str(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )));
            *self.catalog_indices.borrow_mut() = indices;
        }
        let indices = self.catalog_indices.borrow();
        ui.set_has_results(!indices.is_empty());
        ui.set_selected(
            indices
                .iter()
                .position(|&i| i == app.selected)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
        ui.set_sound_title(app.preset().title().into());
        ui.set_source_origin(
            match app.preset().origin() {
                auditorylab::Origin::File(_) => format!("Imported · {}", app.preset().origin()),
                origin => origin.to_string(),
            }
            .into(),
        );
        ui.set_recipe_timing(
            format!(
                "Duration {} · fade {} at each edge",
                auditorylab::human_duration(app.preset().stimulus().duration_seconds),
                auditorylab::human_duration(app.preset().stimulus().fade_seconds)
            )
            .into(),
        );
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
        ui.set_structure(
            app.preset()
                .at(app.rate)
                .map_or_else(
                    |e| e.to_string(),
                    |s| auditorylab::sound_description(&s, false),
                )
                .into(),
        );
        ui.set_details(app::details(app.preset(), app.rate).into());
        let audio = matches!(app.operation, Some(Operation::Play | Operation::Live));
        ui.set_busy(app.operation.is_some());
        ui.set_live(app.operation == Some(Operation::Live));
        ui.set_playback_busy(audio);
        ui.set_can_cancel(audio || app.operation == Some(Operation::Export));
        ui.set_operation_control(
            match app.operation {
                Some(Operation::Play | Operation::Live) => "Stop audio",
                Some(Operation::Export) => "Cancel export",
                Some(Operation::Save) => "Saving…",
                Some(Operation::Import) => "Opening…",
                None => "",
            }
            .into(),
        );
        ui.set_can_edit(
            (app.operation.is_none() || audio)
                && self.after.borrow().is_none()
                && !self.dialog_active.get(),
        );
        ui.set_draft_title(app.workbench.draft.title().into());
        ui.set_draft_summary(app.workbench.draft.summary().into());
        ui.set_duration(app.workbench.draft.duration_seconds().to_string().into());
        ui.set_fade(app.workbench.draft.fade_seconds().to_string().into());
        ui.set_dirty(app.workbench.dirty());
        ui.set_based_on_title(
            app.workbench
                .draft
                .based_on()
                .map_or(String::new(), |p| {
                    format!("Based on “{}” · Source research in Recipe notes", p.title())
                })
                .into(),
        );
        ui.set_based_on(
            app.workbench
                .draft
                .based_on()
                .map_or(String::new(), |p| {
                    format!(
                        "Based on “{}” · {}\n{}\nResearch findings describe the source recipe.",
                        p.title(),
                        p.about()
                            .and_then(|a| a.citation.as_deref())
                            .unwrap_or("source recipe"),
                        p.about().and_then(|a| a.summary.as_deref()).unwrap_or("")
                    )
                })
                .into(),
        );
        let mut errors = app
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
        if let Err(e) = timing.signal(app.rate) {
            errors.push(e.to_string());
        }
        ui.set_metadata_error(errors.join("; ").into());
        ui.set_peak(app.workbench.peak.as_str().into());
        self.sync_rows(&app);
        let validation = if app.editing {
            app.workbench.draft.preset().at(app.rate)
        } else {
            app.preset().at(app.rate)
        };
        ui.set_can_export(
            validation.is_ok() && (!app.editing || app.workbench.can_save())
                || app.editing && app.workbench.has_pending(),
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
        let retained = if app.workbench.has_pending() {
            "Pending edits — not yet applied".into()
        } else if app.workbench.application_error.is_some()
            || !app.workbench.parse_errors.is_empty()
            || app
                .workbench
                .draft
                .target(app.workbench.rate(app.rate))
                .is_err()
        {
            if app.operation == Some(Operation::Live)
                && self.monitor.borrow().as_ref().is_some_and(|reader| {
                    reader
                        .status()
                        .observation
                        .is_some_and(|o| o.active_revision > 0)
                })
            {
                format!(
                    "Edits not applied — playing the last valid settings. {}",
                    app.workbench.peak
                )
            } else {
                format!(
                    "Edits not applied — generated settings not available yet. {}",
                    app.workbench.peak
                )
            }
        } else {
            String::new()
        };
        ui.set_retained_warning(retained.into());
        ui.set_status(app.status.as_str().into());
        ui.set_operation_reason(if app.operation.is_some() {if audio {"Audio continues independently of browsing. Save/import/export wait until Stop."} else {"An operation is active. Browsing remains available; mutations wait for cleanup."}} else {""}.into());
        if audio && let Some(worker) = self.worker.borrow().as_ref() {
            self.session_position.set(worker.progress.frames());
        }
        let window =
            [2_u32, 10, 40, 100][usize::try_from(ui.get_window_index()).unwrap_or(2).min(3)];
        let mut monitor = self.monitor.borrow_mut();
        ui.set_retained_session(monitor.is_some());
        if let Some(reader) = monitor.as_mut() {
            reader.poll(window);
            let status = reader.status();
            let descriptor = reader.descriptor();
            let phase = status.phase;
            ui.set_player_title(
                format!(
                    "{phase:?} · {} · {}",
                    descriptor.title,
                    if descriptor.frames.is_none() {
                        "Live · until stopped"
                    } else {
                        "Finite recipe"
                    }
                )
                .into(),
            );
            ui.set_player_state(format!("{phase:?}").into());
            let position = if phase == auditorylab::PlaybackPhase::Completed {
                descriptor.frames.unwrap_or(self.session_position.get())
            } else {
                self.session_position.get()
            };
            let elapsed = auditorylab::playback_elapsed(descriptor.output.sample_rate, position);
            ui.set_elapsed(
                descriptor
                    .frames
                    .map_or_else(
                        || elapsed.clone(),
                        |frames| {
                            format!(
                                "{} / {}",
                                elapsed.clone(),
                                auditorylab::human_duration(
                                    descriptor.output.sample_rate.seconds_in(frames)
                                )
                            )
                        },
                    )
                    .into(),
            );
            ui.set_generated(
                format!(
                    "Generated audio · {}",
                    auditorylab::generated_description(status.observation.as_ref())
                )
                .into(),
            );
            ui.set_generated_details(status.observation.map_or(String::new(),|o|format!("Observed generated frame {} · accepted revision {} · active revision {} · settled revision {}\n{}",o.frame,status.accepted_revision,o.active_revision,o.settled_revision,auditorylab::generated_details(&o))).into());
            ui.set_device(
                format!(
                    "{} · {} Hz · {} channels · {} · output elapsed is estimated",
                    descriptor
                        .output
                        .name
                        .as_deref()
                        .unwrap_or("Default output"),
                    descriptor.output.sample_rate.hz(),
                    descriptor.output.channels,
                    descriptor.output.format
                )
                .into(),
            );
            if !ui.get_frozen() || self.frozen_span.get().is_none() {
                auditorylab::sample_trace(reader.samples(), 512, &mut self.bins.borrow_mut());
                let [left, right] = scope::paths(&self.bins.borrow());
                ui.set_left_path(left.into());
                ui.set_right_path(right.into());
                if reader.samples().is_empty() {
                    ui.set_meters("Waiting for window samples".into());
                } else {
                    let meters = auditorylab::sample_meters(reader.samples());
                    ui.set_meters(
                        format!(
                            "Window sample peak L {} / R {} dBFS · RMS L {} / R {} dBFS",
                            auditorylab::format_dbfs(meters[0].peak_dbfs),
                            auditorylab::format_dbfs(meters[1].peak_dbfs),
                            auditorylab::format_dbfs(meters[0].rms_dbfs),
                            auditorylab::format_dbfs(meters[1].rms_dbfs)
                        )
                        .into(),
                    );
                }
                if !reader.samples().is_empty() {
                    self.frozen_span
                        .set(Some((status.window_start, status.window_end)));
                }
            }
            let (start, end) = self.frozen_span.get().unwrap_or((0, 0));
            let coverage = descriptor
                .output
                .sample_rate
                .seconds_in(end.saturating_sub(start))
                * 1000.0;
            ui.set_cycle_hint(
                auditorylab::window_cycle_hint(status.observation.as_ref(), coverage)
                    .unwrap_or_default()
                    .into(),
            );
            let stale = reader.freshness() == auditorylab::ObservationFreshness::Stale;
            let label = if status.disabled {
                "Monitoring unavailable (poisoned lock)"
            } else if status.sequence == 0 {
                if matches!(
                    phase,
                    auditorylab::PlaybackPhase::Completed
                        | auditorylab::PlaybackPhase::Stopped
                        | auditorylab::PlaybackPhase::Failed
                ) {
                    "No generated window captured"
                } else {
                    "Waiting for samples"
                }
            } else if matches!(
                phase,
                auditorylab::PlaybackPhase::Completed
                    | auditorylab::PlaybackPhase::Draining
                    | auditorylab::PlaybackPhase::Stopped
                    | auditorylab::PlaybackPhase::Failed
            ) {
                if ui.get_frozen() {
                    "Frozen retained window"
                } else {
                    "Last generated window"
                }
            } else if ui.get_frozen() {
                "Frozen"
            } else if stale {
                "Stale"
            } else if status.gaps > 0 && coverage < f64::from(window) {
                "Display gap"
            } else {
                "Live"
            };
            ui.set_monitor_state(format!("{label} · {coverage:.2} ms · source frames {start}..{end} · status age {} ms · gaps {} · backend underruns {}",reader.age().map_or(0,|age|age.as_millis()),status.gaps,status.underruns).into());
        } else {
            ui.set_player_title(
                if audio {
                    format!("Preparing · {}", self.session_title.borrow())
                } else {
                    if app.editing {
                        "Draft preview · not playing".into()
                    } else {
                        "Recipe preview · not playing".into()
                    }
                }
                .into(),
            );
            ui.set_player_state(if audio { "Preparing" } else { "Stopped" }.into());
            ui.set_elapsed("".into());
            ui.set_generated("".into());
            ui.set_generated_details("".into());
            ui.set_meters("".into());
            ui.set_cycle_hint("".into());
            ui.set_device(format!("Preview/export sample rate: {} Hz", app.rate.hz()).into());
            if audio {
                ui.set_left_path("".into());
                ui.set_right_path("".into());
            } else {
                if let Ok(sound) = validation {
                    let signal = sound.signal();
                    let frames = u64::from(window) * u64::from(signal.sample_rate().hz()) / 1000;
                    auditorylab::trace(
                        signal,
                        signal.fade_frames(),
                        frames,
                        512,
                        &mut self.bins.borrow_mut(),
                    );
                    let [left, right] = scope::paths(&self.bins.borrow());
                    ui.set_left_path(left.into());
                    ui.set_right_path(right.into());
                } else {
                    ui.set_left_path("".into());
                    ui.set_right_path("".into());
                }
            }
            ui.set_monitor_state(
                if audio {
                    "Waiting for generated samples"
                } else {
                    "Static preview · not playing"
                }
                .into(),
            );
        }
        let progress = self
            .worker
            .borrow()
            .as_ref()
            .map_or(0, |w| w.progress.frames());
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            reason = "bounded integral frame counts and presentation ratio"
        )]
        let progress_fraction =
            if app.operation == Some(Operation::Export) && self.operation_frames.get() > 0 {
                (progress as f64 / self.operation_frames.get() as f64) as f32
            } else {
                0.0
            };
        ui.set_progress(progress_fraction.clamp(0.0, 1.0));
        self.updating.set(false);
        Duration::from_millis(50)
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
            row.text0 = auditorylab::setting_value(c, auditorylab::Setting::ALL[0])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos0 = position(
                auditorylab::Setting::ALL[0],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[0]).unwrap_or(0.0),
            );
            row.text1 = auditorylab::setting_value(c, auditorylab::Setting::ALL[1])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos1 = position(
                auditorylab::Setting::ALL[1],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[1]).unwrap_or(0.0),
            );
            row.text2 = auditorylab::setting_value(c, auditorylab::Setting::ALL[2])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos2 = position(
                auditorylab::Setting::ALL[2],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[2]).unwrap_or(0.0),
            );
            row.text3 = auditorylab::setting_value(c, auditorylab::Setting::ALL[3])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos3 = position(
                auditorylab::Setting::ALL[3],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[3]).unwrap_or(0.0),
            );
            row.text4 = auditorylab::setting_value(c, auditorylab::Setting::ALL[4])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos4 = position(
                auditorylab::Setting::ALL[4],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[4]).unwrap_or(0.0),
            );
            row.text5 = auditorylab::setting_value(c, auditorylab::Setting::ALL[5])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos5 = position(
                auditorylab::Setting::ALL[5],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[5]).unwrap_or(0.0),
            );
            row.text6 = auditorylab::setting_value(c, auditorylab::Setting::ALL[6])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos6 = position(
                auditorylab::Setting::ALL[6],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[6]).unwrap_or(0.0),
            );
            row.text7 = auditorylab::setting_value(c, auditorylab::Setting::ALL[7])
                .map_or_else(String::new, |v| v.to_string())
                .into();
            row.pos7 = position(
                auditorylab::Setting::ALL[7],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[7]).unwrap_or(0.0),
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
        if self.updating.get() {
            return;
        }
        let mut app = self.app.borrow_mut();
        if app.operation.is_some()
            && !matches!(app.operation, Some(Operation::Live | Operation::Play))
        {
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
            self.app.borrow_mut().status = match &work {
                Work::Export(sound, path) => {
                    format!("Exporting {} to {path:?}.", sound.preset().title())
                }
                Work::Save(draft, path) => format!("Saving {} to {path:?}.", draft.title()),
                Work::Import(path) => format!("Opening {path:?}."),
                _ => String::new(),
            };
            self.operation_frames.set(match &work {
                Work::Export(sound, _) => sound.signal().frames(),
                _ => 0,
            });
            if matches!(op, Operation::Play | Operation::Live) {
                *self.session_title.borrow_mut() = match &work {
                    Work::Play(p) => p.title(),
                    Work::Live(d) => d.title().into(),
                    _ => String::new(),
                };
                *self.monitor.borrow_mut() = None;
                self.frozen_span.set(None);
                self.session_position.set(0);
            }
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
    auditorylab::slider_position(setting, rate, value) as f32
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
fn open_source() {
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
    if let Err(error) = command.arg(auditorylab::SOURCE_URL).spawn() {
        eprintln!("Cannot open source link: {error}");
    }
}
fn reply_native_quit(approve: bool) -> bool {
    #[cfg(target_os = "macos")]
    {
        native_quit::reply(approve)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = approve;
        false
    }
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
    ui.set_export_rates(ModelRc::new(VecModel::from(
        EXPORT_RATES
            .iter()
            .map(|rate| format!("{rate} Hz").into())
            .collect::<Vec<slint::SharedString>>(),
    )));
    ui.set_rate_index(
        i32::try_from(
            EXPORT_RATES
                .iter()
                .position(|rate| *rate == auditorylab::SampleRate::DEFAULT.hz())
                .unwrap_or(0),
        )
        .unwrap_or(0),
    );
    ui.set_guidance(
        format!(
            "{} {}",
            auditorylab::Notice::LowStartingVolume,
            auditorylab::Notice::HeadphonesForSeparateEars
        )
        .into(),
    );
    let about = AboutWindow::new()?;
    ui.set_about_identity(about_identity().into());
    ui.set_frozen(motion::reduced());
    about.set_identity(about_identity().into());
    let desktop = Rc::new(Desktop {
        updating: Cell::new(false),
        catalog_dirty: Cell::new(true),
        app: RefCell::new(App::new(auditorylab::catalog()?)),
        worker: RefCell::new(None),
        ui: ui.as_weak(),
        timer: slint::Timer::default(),
        bins: RefCell::new(Vec::new()),
        rows: Rc::new(VecModel::default()),
        monitor: RefCell::new(None),
        catalog_indices: RefCell::new(Vec::new()),
        query: RefCell::new(String::new()),
        after: RefCell::new(None),
        session_position: Cell::new(0),
        session_title: RefCell::new(String::new()),
        frozen_span: Cell::new(None),
        dialog_active: Cell::new(false),
        operation_frames: Cell::new(0),
    });
    ui.set_rows(ModelRc::from(desktop.rows.clone()));
    let d = desktop.clone();
    ui.on_select(move |index| {
        if d.updating.get() {
            return;
        }
        if let Some(index) = usize::try_from(index)
            .ok()
            .and_then(|i| d.catalog_indices.borrow().get(i).copied())
        {
            d.app.borrow_mut().select(index);
            d.refresh();
        }
    });
    let d = desktop.clone();
    ui.on_rate_changed(move |index| {
        let Some(rate) = usize::try_from(index)
            .ok()
            .and_then(|i| EXPORT_RATES.get(i))
            .copied()
        else {
            return;
        };
        if let Ok(rate) = auditorylab::SampleRate::new(rate) {
            let mut app = d.app.borrow_mut();
            app.rate = rate;
            app.workbench.refresh(rate);
            d.catalog_dirty.set(true);
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
        {
            if let Some(worker) = d.worker.borrow().as_ref() {
                worker.cancel.request();
            }
        }
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_open_recipe(move || d.choose_import());
    let d = desktop.clone();
    ui.on_export_wav(move || d.choose_export());
    let d = desktop.clone();
    ui.on_tab_changed(move |index| {
        if !d.updating.get() {
            d.app.borrow_mut().editing = index == 1;
            d.refresh();
        }
    });
    let d = desktop.clone();
    ui.on_open_workbench(move || {
        let preset = d.app.borrow().preset().clone();
        d.request_action(Action::Copy(preset));
    });
    let d = desktop.clone();
    ui.on_new_draft(move || {
        d.request_action(Action::New);
    });
    let d = desktop.clone();
    ui.on_add_component(move |kind| {
        let mut app = d.app.borrow_mut();
        if app.operation.is_some()
            && !matches!(app.operation, Some(Operation::Live | Operation::Play))
        {
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
        if app.operation.is_some()
            && !matches!(app.operation, Some(Operation::Live | Operation::Play))
        {
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
        if app.operation.is_some()
            && !matches!(app.operation, Some(Operation::Live | Operation::Play))
        {
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
            let value = auditorylab::slider_value(
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
        if d.updating.get() {
            return;
        }
        let mut app = d.app.borrow_mut();
        if app.operation.is_some()
            && !matches!(app.operation, Some(Operation::Live | Operation::Play))
        {
            return;
        }
        app.workbench.metadata(&key, &text);
        drop(app);
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_stage(move |text, setting, value| {
        if d.updating.get() {
            return;
        }
        let mut app = d.app.borrow_mut();
        if app.operation.is_some()
            && !matches!(app.operation, Some(Operation::Live | Operation::Play))
        {
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
        if d.updating.get() {
            return;
        }
        let mut app = d.app.borrow_mut();
        if app.operation.is_none()
            || matches!(app.operation, Some(Operation::Live | Operation::Play))
        {
            app.workbench
                .pending_metadata
                .insert(key.to_string(), value.to_string());
        }
        drop(app);
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_save_recipe(move || d.choose_save());
    let weak = about.as_weak();
    ui.on_about(move || {
        if let Some(about) = weak.upgrade() {
            let _ = about.show();
        }
    });
    let d = desktop.clone();
    ui.window().on_close_requested(move || {
        d.request_action(Action::Close);
        slint::CloseRequestResponse::KeepWindowShown
    });
    let d = desktop.clone();
    ui.on_quit(move || {
        d.request_action(Action::Close);
    });
    let d = desktop.clone();
    ui.on_search(move |query| {
        *d.query.borrow_mut() = query.to_string();
        let mut app = d.app.borrow_mut();
        if !auditorylab::catalog_matches(app.preset(), &query)
            && let Some(i) = app
                .presets
                .iter()
                .position(|p| auditorylab::catalog_matches(p, &query))
        {
            app.select(i);
        }
        drop(app);
        d.catalog_dirty.set(true);
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_preview_selected(move || {
        if d.worker.borrow().is_none() {
            *d.monitor.borrow_mut() = None;
            d.refresh();
        }
    });
    let d = desktop.clone();
    ui.on_play_recipe(move || {
        let preset = d.app.borrow().preset().clone();
        d.replace_playback(Work::Play(preset));
    });
    let d = desktop.clone();
    ui.on_play_live(move || {
        let mut app = d.app.borrow_mut();
        let rate = app.rate;
        app.workbench.commit_pending(rate);
        let draft = app.workbench.draft.clone();
        drop(app);
        d.replace_playback(Work::Live(draft));
    });
    let d = desktop.clone();
    ui.on_based_on_source(move || {
        let source = d.app.borrow().workbench.draft.based_on().cloned();
        if let Some(source) = source {
            let mut app = d.app.borrow_mut();
            let i = app
                .presets
                .iter()
                .position(|p| p.origin() == source.origin())
                .unwrap_or_else(|| {
                    app.presets.push(source);
                    app.presets.len() - 1
                });
            app.select(i);
            app.editing = false;
            drop(app);
            *d.query.borrow_mut() = String::new();
            if let Some(ui) = d.ui.upgrade() {
                ui.set_query("".into());
                ui.set_tab_index(0);
            }
            d.catalog_dirty.set(true);
            d.refresh();
        }
    });
    ui.on_open_source(open_source);
    about.on_open_source(open_source);
    #[cfg(target_os = "macos")]
    {
        ui.show()?;
        native_quit::install(ui.as_weak())?;
    }
    desktop.refresh();
    ui.run()?;
    Ok(())
}
