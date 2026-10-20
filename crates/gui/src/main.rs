//! Desktop interface to Auditory Lab.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod actions;
mod app;
mod dialogs;
mod motion;
#[cfg(target_os = "macos")]
mod native_application;
mod scope;
mod typography;
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
use generated::{AboutWindow, MainWindow, ModelRow, Row};
use slint::winit_030::{EventResult as WinitEventResult, WinitWindowAccessor, winit};
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
    capture: RefCell<Option<auditorylab::WaveformCapture>>,
    model: RefCell<Option<auditorylab::ModelView>>,
    typography: RefCell<typography::TypographyService>,
    font: RefCell<typography::Typography>,
    about: slint::Weak<AboutWindow>,
    preparation_failed: Cell<bool>,
    rows: Rc<VecModel<Row>>,
    monitor: RefCell<Option<auditorylab::MonitorReader>>,
    catalog_indices: RefCell<Vec<usize>>,
    query: RefCell<String>,
    after: RefCell<Option<Continuation>>,
    session_position: Cell<u64>,
    session_title: RefCell<String>,

    dialog_active: Cell<bool>,
    operation_frames: Cell<u64>,
}
impl Desktop {
    fn refresh_typography(&self) {
        #[cfg(target_os = "macos")]
        if let Err(error) = native_application::refresh_about_menu() {
            eprintln!("Native About: {error}");
        }
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        if let Some(font) = self.typography.borrow().latest()
            && *self.font.borrow() != font
        {
            eprintln!(
                "Typography: {} · {} logical px · {}",
                font.family, font.size, font.origin
            );
            ui.set_ui_family(font.family.as_str().into());
            ui.set_ui_size(font.size);
            if let Some(about) = self.about.upgrade() {
                about.set_ui_family(font.family.as_str().into());
                about.set_ui_size(font.size);
            }
            *self.font.borrow_mut() = font;
        }
        ui.set_typography_origin(
            format!(
                "{} · {} logical px · {}",
                self.font.borrow().family,
                self.font.borrow().size,
                self.font.borrow().origin
            )
            .into(),
        );
    }
    fn refresh(self: &Rc<Self>) -> Duration {
        self.refresh_typography();
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
                        *self.capture.borrow_mut() = None;
                        ui.set_left_path("".into());
                        ui.set_right_path("".into());
                    }
                    Message::LiveReady(control, reader) => {
                        let rate = app.rate;
                        app.workbench.ready(control, rate);
                        *self.monitor.borrow_mut() = Some(reader);
                        *self.capture.borrow_mut() = None;
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
            if !success
                && matches!(
                    self.app.borrow().operation,
                    Some(Operation::Play | Operation::Live)
                )
            {
                if let Some(reader) = self.monitor.borrow().as_ref() {
                    reader.worker_exited_with_error();
                } else {
                    self.preparation_failed.set(true);
                }
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
        ui.set_can_open_study(
            app.preset()
                .about()
                .and_then(|a| a.source_url.as_deref())
                .is_some_and(|url| url.starts_with("https://") || url.starts_with("http://")),
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
                .map_or_else(|e| e.to_string(), |s| auditorylab::sound_brief(&s))
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
        self.sync_metadata(&ui, &app.workbench);
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
            if audio && phase == auditorylab::PlaybackPhase::Preparing {
                ui.set_operation_control("Cancel".into());
            }
            ui.set_player_title(
                format!(
                    "{} · {} · {}",
                    auditorylab::phase_text(phase),
                    descriptor.title,
                    if descriptor.frames.is_none() {
                        if matches!(
                            phase,
                            auditorylab::PlaybackPhase::Preparing
                                | auditorylab::PlaybackPhase::Running
                                | auditorylab::PlaybackPhase::Stopping
                                | auditorylab::PlaybackPhase::Draining
                        ) {
                            "Live · until stopped"
                        } else {
                            "Last live session"
                        }
                    } else {
                        "Finite recipe"
                    }
                )
                .into(),
            );
            ui.set_player_state(auditorylab::phase_text(phase).into());
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
            ui.set_generated(reader.description().into());
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
            let scale = if ui.get_scale_index() == 1 {
                auditorylab::WaveformScale::Fit
            } else {
                auditorylab::WaveformScale::Fixed
            };
            let mut capture_error = None;
            if !ui.get_frozen() || self.capture.borrow().is_none() {
                match auditorylab::WaveformCapture::monitored(reader, scale) {
                    Ok(Some(capture)) => *self.capture.borrow_mut() = Some(capture),
                    Ok(None) => {}
                    Err(error) => {
                        capture_error = Some(format!(
                            "Invalid waveform capture ({error:?}); retaining the last valid window"
                        ))
                    }
                }
            }
            let warning = if let Some(error) = capture_error {
                error
            } else if status.disabled {
                "Monitoring unavailable; generated audio continues independently".into()
            } else if reader.freshness() == auditorylab::ObservationFreshness::Stale && audio {
                "Monitoring is stale; the retained window is not current output".into()
            } else if status.underruns > 0 {
                format!("Output underruns: {} · see audio details", status.underruns)
            } else if status.gaps > 0 {
                format!(
                    "Monitoring gaps: {} · distinct from output underruns",
                    status.gaps
                )
            } else {
                String::new()
            };
            ui.set_monitor_warning(warning.into());
            ui.set_monitor_state(format!("Current status: observed frame {:?} · status age {:?} · gaps {} · output underruns {}",status.observation.map(|o|o.frame),reader.age(),status.gaps,status.underruns).into());
        } else {
            ui.set_player_title(
                if self.preparation_failed.get() {
                    format!("Failed · {}", self.session_title.borrow())
                } else if audio {
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
            ui.set_player_state(
                if self.preparation_failed.get() {
                    "Failed"
                } else if audio {
                    "Preparing"
                } else {
                    "Stopped"
                }
                .into(),
            );
            ui.set_elapsed("".into());
            ui.set_generated("".into());
            ui.set_generated_details("".into());
            ui.set_meters("".into());
            ui.set_cycle_hint("".into());
            ui.set_device(format!("Preview/export sample rate: {} Hz", app.rate.hz()).into());
            if !audio
                && !self.preparation_failed.get()
                && (!ui.get_frozen() || self.capture.borrow().is_none())
                && let Ok(sound) = validation
            {
                let scale = if ui.get_scale_index() == 1 {
                    auditorylab::WaveformScale::Fit
                } else {
                    auditorylab::WaveformScale::Fixed
                };
                *self.capture.borrow_mut() =
                    Some(auditorylab::WaveformCapture::preview(&sound, window, scale));
            }
            ui.set_monitor_warning("".into());
            ui.set_monitor_state(
                if audio {
                    "Waiting for generated samples"
                } else {
                    "Seekable preview; not generated playback"
                }
                .into(),
            );
        }
        let ended = monitor.as_ref().is_some_and(|r| {
            matches!(
                r.status().phase,
                auditorylab::PlaybackPhase::Stopped
                    | auditorylab::PlaybackPhase::Completed
                    | auditorylab::PlaybackPhase::Failed
            )
        });
        if let Some(capture) = self.capture.borrow().as_ref() {
            let [left, right] = scope::paths(capture.bins(), capture.limit());
            ui.set_left_path(left.into());
            ui.set_right_path(right.into());
            ui.set_scale_label(auditorylab::compact_number(capture.limit()).into());
            if capture.clipped() {
                ui.set_monitor_warning("Waveform exceeds the fixed display range; exact levels are retained. Use Fit to view the full extent.".into());
            }
            ui.set_waveform_caption(capture.label(ui.get_frozen(), ended).into());
            ui.set_meters(
                format!(
                    "Captured sample peak L {} / R {} dBFS · RMS L {} / R {} dBFS",
                    auditorylab::format_dbfs(capture.meters()[0].peak_dbfs),
                    auditorylab::format_dbfs(capture.meters()[1].peak_dbfs),
                    auditorylab::format_dbfs(capture.meters()[0].rms_dbfs),
                    auditorylab::format_dbfs(capture.meters()[1].rms_dbfs)
                )
                .into(),
            );
            ui.set_monitor_state(
                format!(
                    "{}\n{}",
                    capture.caption(ui.get_frozen(), ended),
                    ui.get_monitor_state()
                )
                .into(),
            );
        } else {
            ui.set_left_path("".into());
            ui.set_right_path("".into());
            ui.set_meters("".into());
            ui.set_waveform_caption(
                if audio {
                    "Waiting for generated waveform samples"
                } else {
                    "No waveform capture available. Preview a valid draft or recipe."
                }
                .into(),
            );
        }
        let model_target = if let Some(reader) = monitor.as_ref() {
            let (revision, target) = reader.model_target();
            Some((
                target,
                reader.descriptor().title.clone(),
                if reader.descriptor().frames.is_some() {
                    auditorylab::ModelSource::Recipe
                } else {
                    auditorylab::ModelSource::Accepted(revision)
                },
            ))
        } else if app.editing {
            app.workbench.last_target.map(|t| {
                (
                    t,
                    app.workbench.draft.title().to_owned(),
                    auditorylab::ModelSource::Draft,
                )
            })
        } else {
            auditorylab::recipe_target(app.preset(), app.rate)
                .ok()
                .map(|t| (t, app.preset().title(), auditorylab::ModelSource::Recipe))
        };
        if let Some((target, title, source)) = model_target {
            let changed = self
                .model
                .borrow()
                .as_ref()
                .is_none_or(|m| m.target != target || m.source != source || m.title != title);
            if changed {
                *self.model.borrow_mut() = Some(auditorylab::ModelView::new(target, title, source));
            }
            if let Some(model) = self.model.borrow().as_ref() {
                ui.set_model_caption(
                    format!(
                        "{}{}\n{}{}",
                        model.caption(),
                        monitor
                            .as_ref()
                            .map(|r| model.generation_note(r.status()))
                            .unwrap_or_default(),
                        model.summary.description(),
                        if ui.get_retained_warning().is_empty()
                            || matches!(model.source, auditorylab::ModelSource::Recipe)
                        {
                            ""
                        } else {
                            " · edits not applied"
                        }
                    )
                    .into(),
                );
                if changed {
                    ui.set_model_rows(ModelRc::new(VecModel::from(
                        model
                            .projection
                            .lanes
                            .iter()
                            .map(|l| {
                                let stereo: Vec<_> = l.columns.iter().map(|e| [*e; 2]).collect();
                                ModelRow {
                                    label: auditorylab::ModelView::lane_label(l).into(),
                                    trace: scope::paths(&stereo, 1.0)[0].clone().into(),
                                }
                            })
                            .collect::<Vec<_>>(),
                    )));
                }
            }
        } else {
            *self.model.borrow_mut() = None;
            ui.set_model_caption("No valid timing model available".into());
            ui.set_model_rows(ModelRc::default());
        }
        ui.set_draft_brief(
            app.workbench
                .last_target
                .map(|t| auditorylab::SoundSummary::target(&t).description())
                .unwrap_or_else(|| "No valid draft model; correct the fields".into())
                .into(),
        );
        ui.set_retained_session(monitor.is_some() || self.capture.borrow().is_some());
        if app.operation.is_none() {
            ui.set_operation_control(
                if app.editing {
                    "Play live"
                } else {
                    "Play selected recipe"
                }
                .into(),
            );
            ui.set_can_cancel(if app.editing {
                app.workbench.parse_errors.is_empty()
                    && app
                        .workbench
                        .draft
                        .target(app.workbench.rate(app.rate))
                        .is_ok()
            } else {
                app.preset().at(app.rate).is_ok()
            });
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
    fn sync_metadata(&self, ui: &MainWindow, w: &app::Workbench) {
        let display =
            |key: &str, value: String| w.pending_metadata.get(key).cloned().unwrap_or(value);
        ui.set_title_staged(w.pending_metadata.contains_key("title"));
        ui.set_summary_staged(w.pending_metadata.contains_key("summary"));
        ui.set_duration_staged(w.pending_metadata.contains_key("duration"));
        ui.set_fade_staged(w.pending_metadata.contains_key("fade"));
        ui.set_draft_title(display("title", w.draft.title().to_owned()).into());
        ui.set_draft_summary(display("summary", w.draft.summary().to_owned()).into());
        ui.set_duration(display("duration", w.draft.duration_seconds().to_string()).into());
        ui.set_fade(display("fade", w.draft.fade_seconds().to_string()).into());
    }
    fn reset_metadata(&self) {
        if let Some(ui) = self.ui.upgrade() {
            self.sync_metadata(&ui, &self.app.borrow().workbench);
            ui.set_draft_reset(ui.get_draft_reset().wrapping_add(1));
        }
    }
    fn restore_field(self: &Rc<Self>, text: &str, index: i32) -> String {
        let value = {
            let mut app = self.app.borrow_mut();
            let Some(id) = app.workbench.id(text) else {
                return String::new();
            };
            let Some(setting) = usize::try_from(index)
                .ok()
                .and_then(|i| auditorylab::Setting::ALL.get(i))
                .copied()
            else {
                return String::new();
            };
            app.workbench.pending.remove(&(id, setting));
            app.workbench.parse_errors.remove(&(id, setting));
            let value = app
                .workbench
                .draft
                .components()
                .iter()
                .find(|(key, _)| *key == id)
                .and_then(|(_, c)| auditorylab::setting_value(c, setting))
                .map(|v| v.to_string())
                .unwrap_or_default();
            let rate = app.rate;
            app.workbench.refresh(rate);
            value
        };
        self.refresh();
        value
    }
    fn restore_metadata(self: &Rc<Self>, key: &str) -> String {
        {
            let mut app = self.app.borrow_mut();
            app.workbench.pending_metadata.remove(key);
            if key == "duration" {
                app.workbench.metadata_errors.remove("duration");
            } else if key == "fade" {
                app.workbench.metadata_errors.remove("fade");
            }
        }
        self.refresh();
        let app = self.app.borrow();
        match key {
            "title" => app.workbench.draft.title().to_owned(),
            "summary" => app.workbench.draft.summary().to_owned(),
            "duration" => app.workbench.draft.duration_seconds().to_string(),
            "fade" => app.workbench.draft.fade_seconds().to_string(),
            _ => String::new(),
        }
    }
    fn reset_editors(&self) {
        self.rows.set_vec(Vec::new());
        if let Some(ui) = self.ui.upgrade() {
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
                heading: auditorylab::component_role(c).into(),
                context: c
                    .facts(rate)
                    .map(|f| auditorylab::component_description(&f, false))
                    .unwrap_or_else(|e| e.to_string())
                    .into(),
                ..Row::default()
            };
            row.text0 = app
                .workbench
                .pending
                .get(&(*id, auditorylab::Setting::ALL[0]))
                .cloned()
                .unwrap_or_else(|| {
                    auditorylab::setting_value(c, auditorylab::Setting::ALL[0])
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                })
                .into();
            row.staged0 = app
                .workbench
                .pending
                .contains_key(&(*id, auditorylab::Setting::ALL[0]));
            row.pos0 = position(
                auditorylab::Setting::ALL[0],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[0]).unwrap_or(0.0),
            );
            row.text1 = app
                .workbench
                .pending
                .get(&(*id, auditorylab::Setting::ALL[1]))
                .cloned()
                .unwrap_or_else(|| {
                    auditorylab::setting_value(c, auditorylab::Setting::ALL[1])
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                })
                .into();
            row.staged1 = app
                .workbench
                .pending
                .contains_key(&(*id, auditorylab::Setting::ALL[1]));
            row.pos1 = position(
                auditorylab::Setting::ALL[1],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[1]).unwrap_or(0.0),
            );
            row.text2 = app
                .workbench
                .pending
                .get(&(*id, auditorylab::Setting::ALL[2]))
                .cloned()
                .unwrap_or_else(|| {
                    auditorylab::setting_value(c, auditorylab::Setting::ALL[2])
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                })
                .into();
            row.staged2 = app
                .workbench
                .pending
                .contains_key(&(*id, auditorylab::Setting::ALL[2]));
            row.pos2 = position(
                auditorylab::Setting::ALL[2],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[2]).unwrap_or(0.0),
            );
            row.text3 = app
                .workbench
                .pending
                .get(&(*id, auditorylab::Setting::ALL[3]))
                .cloned()
                .unwrap_or_else(|| {
                    auditorylab::setting_value(c, auditorylab::Setting::ALL[3])
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                })
                .into();
            row.staged3 = app
                .workbench
                .pending
                .contains_key(&(*id, auditorylab::Setting::ALL[3]));
            row.pos3 = position(
                auditorylab::Setting::ALL[3],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[3]).unwrap_or(0.0),
            );
            row.text4 = app
                .workbench
                .pending
                .get(&(*id, auditorylab::Setting::ALL[4]))
                .cloned()
                .unwrap_or_else(|| {
                    auditorylab::setting_value(c, auditorylab::Setting::ALL[4])
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                })
                .into();
            row.staged4 = app
                .workbench
                .pending
                .contains_key(&(*id, auditorylab::Setting::ALL[4]));
            row.pos4 = position(
                auditorylab::Setting::ALL[4],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[4]).unwrap_or(0.0),
            );
            row.text5 = app
                .workbench
                .pending
                .get(&(*id, auditorylab::Setting::ALL[5]))
                .cloned()
                .unwrap_or_else(|| {
                    auditorylab::setting_value(c, auditorylab::Setting::ALL[5])
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                })
                .into();
            row.staged5 = app
                .workbench
                .pending
                .contains_key(&(*id, auditorylab::Setting::ALL[5]));
            row.pos5 = position(
                auditorylab::Setting::ALL[5],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[5]).unwrap_or(0.0),
            );
            row.text6 = app
                .workbench
                .pending
                .get(&(*id, auditorylab::Setting::ALL[6]))
                .cloned()
                .unwrap_or_else(|| {
                    auditorylab::setting_value(c, auditorylab::Setting::ALL[6])
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                })
                .into();
            row.staged6 = app
                .workbench
                .pending
                .contains_key(&(*id, auditorylab::Setting::ALL[6]));
            row.pos6 = position(
                auditorylab::Setting::ALL[6],
                rate,
                auditorylab::setting_value(c, auditorylab::Setting::ALL[6]).unwrap_or(0.0),
            );
            row.text7 = app
                .workbench
                .pending
                .get(&(*id, auditorylab::Setting::ALL[7]))
                .cloned()
                .unwrap_or_else(|| {
                    auditorylab::setting_value(c, auditorylab::Setting::ALL[7])
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                })
                .into();
            row.staged7 = app
                .workbench
                .pending
                .contains_key(&(*id, auditorylab::Setting::ALL[7]));
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
        if matches!(op, Operation::Play | Operation::Live) {
            self.preparation_failed.set(false);
        }

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
                *self.capture.borrow_mut() = None;
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
    open_web(auditorylab::SOURCE_URL);
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
        eprintln!("Cannot open source link: {error}");
    }
}
fn reply_native_quit(approve: bool) -> bool {
    #[cfg(target_os = "macos")]
    {
        native_application::reply_to_termination(approve)
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
    let font = typography::resolve();
    ui.set_ui_family(font.family.as_str().into());
    ui.set_ui_size(font.size);
    about.set_ui_family(font.family.as_str().into());
    about.set_ui_size(font.size);
    ui.set_native_mac(cfg!(target_os = "macos"));
    ui.set_frozen(motion::reduced());
    about.set_identity(about_identity().into());
    let desktop = Rc::new(Desktop {
        updating: Cell::new(false),
        catalog_dirty: Cell::new(true),
        app: RefCell::new(App::new(auditorylab::catalog()?)),
        worker: RefCell::new(None),
        ui: ui.as_weak(),
        timer: slint::Timer::default(),
        capture: RefCell::new(None),
        model: RefCell::new(None),
        typography: RefCell::new(typography::TypographyService::start()),
        font: RefCell::new(font),
        about: about.as_weak(),
        preparation_failed: Cell::new(false),
        rows: Rc::new(VecModel::default()),
        monitor: RefCell::new(None),
        catalog_indices: RefCell::new(Vec::new()),
        query: RefCell::new(String::new()),
        after: RefCell::new(None),
        session_position: Cell::new(0),
        session_title: RefCell::new(String::new()),

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
    ui.on_stop_audio({
        let d = desktop.clone();
        move || {
            if let Some(worker) = d.worker.borrow().as_ref() {
                worker.cancel.request();
            }
        }
    });
    ui.on_play_stop(move || {
        let cancelled = {
            let worker = d.worker.borrow();
            if let Some(worker) = worker.as_ref() {
                worker.cancel.request();
                true
            } else {
                false
            }
        };
        if cancelled {
            d.refresh();
            return;
        }
        let work = {
            let mut app = d.app.borrow_mut();
            if app.editing {
                let rate = app.rate;
                app.workbench.commit_pending(rate);
                Work::Live(app.workbench.draft.clone())
            } else {
                Work::Play(app.preset().clone())
            }
        };
        d.replace_playback(work);
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
    let main_weak = ui.as_weak();
    ui.on_about(move || {
        if let Some(ui) = main_weak.upgrade() {
            ui.set_edits_suspended(true);
        }
        if let Some(about) = weak.upgrade() {
            if let Err(error) = about.show() {
                eprintln!("About: {error}");
            }
            about.invoke_focus_close();
        }
    });
    let weak = about.as_weak();
    about.on_dismiss(move || {
        if let Some(about) = weak.upgrade() {
            let _ = about.hide();
        }
    });
    ui.window().on_winit_window_event({
        let weak = ui.as_weak();
        move |_, event| {
            if let winit::event::WindowEvent::Focused(focused) = event
                && let Some(ui) = weak.upgrade()
            {
                ui.set_edits_suspended(!focused);
            }
            WinitEventResult::Propagate
        }
    });
    let d = desktop.clone();
    ui.on_restore(move |id, setting| d.restore_field(&id, setting).into());
    let d = desktop.clone();
    ui.on_restore_metadata(move |key| d.restore_metadata(&key).into());
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
    ui.on_open_study(move || {
        if let Some(url) = d
            .app
            .borrow()
            .preset()
            .about()
            .and_then(|a| a.source_url.as_deref())
            && (url.starts_with("https://") || url.starts_with("http://"))
        {
            open_web(url);
        }
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
            d.preparation_failed.set(false);
            *d.capture.borrow_mut() = None;
            d.refresh();
        }
    });
    let d = desktop.clone();
    ui.on_play_recipe(move || {
        let preset = d.app.borrow().preset().clone();
        d.replace_playback(Work::Play(preset));
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

    about.on_open_source(open_source);
    #[cfg(target_os = "macos")]
    {
        ui.show()?;
        native_application::install(ui.as_weak())?;
    }
    // Native preference delivery also runs while idle. Queries stay on the
    // listener thread; this drains only its coalesced value and repairs rebuilt menus.
    let native_ui_timer = slint::Timer::default();
    let d = desktop.clone();
    native_ui_timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(200),
        move || d.refresh_typography(),
    );
    desktop.refresh();
    let result = ui.run();
    native_ui_timer.stop();
    desktop.timer.stop();
    desktop.typography.borrow_mut().shutdown();
    result?;
    Ok(())
}
