//! Desktop interface to Auditory Lab.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod actions;
mod app;
mod details_parent;
mod dialogs;
#[cfg(test)]
mod focus_reflow_tests;
#[cfg(test)]
mod focus_scroll_tests;
mod motion;
#[cfg(target_os = "macos")]
mod native_application;
mod scope;
#[cfg(test)]
mod text_edit_tests;
mod typography;
mod worker;
use actions::{Action, Continuation};
#[expect(
    unreachable_pub,
    clippy::unwrap_used,
    clippy::todo,
    reason = "quality:gui-main-generated"
)]
mod generated {
    slint::include_modules!();
}
use app::{App, Operation};
use generated::{
    AboutWindow, AudioControlsWindow, DetailsWindow, MainWindow, ModelRow, MonitorWindow, Row,
};
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
    audio_worker: RefCell<Option<Worker>>,
    file_worker: RefCell<Option<Worker>>,
    ui: slint::Weak<MainWindow>,
    timer: slint::Timer,
    capture: RefCell<Option<auditorylab::WaveformCapture>>,
    model: RefCell<Option<auditorylab::ModelView>>,
    typography: RefCell<typography::TypographyService>,
    font: RefCell<typography::Typography>,
    motion: RefCell<motion::Service>,
    motion_preference: Cell<motion::Preference>,
    motion_override: Cell<Option<bool>>,
    about: slint::Weak<AboutWindow>,
    monitor_window: slint::Weak<MonitorWindow>,
    details_window: slint::Weak<DetailsWindow>,
    audio_controls: slint::Weak<AudioControlsWindow>,
    monitor_window_visible: Cell<bool>,
    audio_controls_visible: Cell<bool>,
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
fn present_details<C: ComponentHandle + 'static>(window: &DetailsWindow, parent: &C) {
    if let Err(error) = window.show() {
        eprintln!("Details: {error}");
        return;
    }
    details_parent::bind(window, parent);
    window.invoke_focus_close();
}
fn hide_monitor_window(ui: &MainWindow, monitor: &MonitorWindow, was_focused: bool) {
    let _ = monitor.hide();
    if was_focused {
        ui.invoke_focus_monitor_opener();
    }
}
fn sync_monitor_warnings(
    ui: &MainWindow,
    warning: Option<auditorylab::MonitorWarning>,
    capture_error: Option<&str>,
    underruns: u64,
    clipped: bool,
) {
    let mut text = capture_error.map_or_else(
        || warning.map_or_else(String::new, |warning| warning.message().into()),
        str::to_owned,
    );
    let mut live = capture_error.is_some()
        || warning.is_some_and(auditorylab::MonitorWarning::announces_changes);
    if clipped {
        let advisory = auditorylab::MonitorWarning::FixedRangeExceeded;
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(advisory.message());
        live |= advisory.announces_changes();
    }
    ui.set_monitor_warning(text.into());
    ui.set_monitor_warning_live(live);
    ui.set_backend_warning(auditorylab::output_underrun_warning(underruns).into());
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
        if let Some(preference) = self.motion.borrow().latest() {
            self.motion_preference.set(preference);
            if self.motion_override.get().is_none() {
                ui.set_held(preference == motion::Preference::Reduced);
            }
        }
        ui.set_motion_reason(
            format!(
                "{}{}",
                self.motion_preference.get().description(),
                match self.motion_override.get() {
                    Some(true) => " User chose Hold display.",
                    Some(false) => " User chose Return to live.",
                    None => "",
                }
            )
            .into(),
        );
        ui.set_motion_overridden(self.motion_override.get().is_some());
        let font = self.font.borrow();
        if let Some(window) = self.monitor_window.upgrade() {
            window.set_ui_family(font.family.as_str().into());
            window.set_ui_size(font.size);
        }
        if let Some(window) = self.details_window.upgrade() {
            window.set_ui_family(font.family.as_str().into());
            window.set_ui_size(font.size);
        }
        if let Some(window) = self.audio_controls.upgrade() {
            window.set_ui_family(font.family.as_str().into());
            window.set_ui_size(font.size);
        }
        drop(font);
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
    fn poll_task(&self, audio: bool, ui: &MainWindow) -> Option<bool> {
        let slot = if audio {
            &self.audio_worker
        } else {
            &self.file_worker
        };
        let op = if audio {
            self.app.borrow().operation
        } else {
            self.app.borrow().file_operation
        }?;
        let mut done = false;
        let mut success = true;
        if let Some(worker) = slot.borrow().as_ref() {
            loop {
                let message = match worker.receive() {
                    Ok(Some(message)) => message,
                    Ok(None) => break,
                    Err(error) => {
                        if !done {
                            self.task_status(audio, error);
                            success = false;
                        }
                        done = true;
                        break;
                    }
                };
                match message {
                    Message::Ready(reader) => {
                        *self.monitor.borrow_mut() = Some(reader);
                        if !ui.get_held() {
                            *self.capture.borrow_mut() = None;
                        }
                    }
                    Message::LiveReady(control, reader, owner) => {
                        let mut app = self.app.borrow_mut();
                        let rate = app.rate;
                        app.workbench.ready(&owner, control, rate);
                        *self.monitor.borrow_mut() = Some(reader);
                        if !ui.get_held() {
                            *self.capture.borrow_mut() = None;
                        }
                    }
                    Message::Imported(result) => {
                        success = result.is_ok();
                        if self.app.borrow_mut().imported(result) {
                            self.catalog_dirty.set(true);
                            *self.query.borrow_mut() = String::new();
                            ui.set_query("".into());
                        }
                        done = true;
                    }
                    Message::Saved(owner, result) => {
                        match result {
                            Ok((draft, path)) => {
                                let mut app = self.app.borrow_mut();
                                let invalid = draft.preset().at(app.rate).is_err();
                                let same_draft = app.workbench.record_save(&owner, draft);
                                app.status = format!(
                                    "{} to {path:?}.{}",
                                    if invalid {
                                        "Saved with validation errors"
                                    } else {
                                        "Saved sound"
                                    },
                                    if same_draft {
                                        ""
                                    } else {
                                        " The current sound is a different draft; its saved state is unchanged."
                                    }
                                );
                            }
                            Err(error) => {
                                self.task_status(audio, error.to_string());
                                success = false;
                            }
                        }
                        done = true;
                    }
                    Message::Done(result) => {
                        let text = match result {
                            Ok(text) => text,
                            Err(e) if e.kind() == auditorylab::ErrorKind::Cancelled => {
                                if audio {
                                    "Stopped.".into()
                                } else {
                                    "File operation cancelled.".into()
                                }
                            }
                            Err(e) => {
                                success = false;
                                e.to_string()
                            }
                        };
                        self.task_status(audio, text);
                        done = true;
                    }
                }
            }
        }
        if !done {
            return None;
        }
        if let Some(worker) = slot.borrow_mut().take()
            && let Err(error) = worker.join()
        {
            self.task_status(audio, error);
            success = false;
        }
        if audio && !success {
            if let Some(reader) = self.monitor.borrow().as_ref() {
                reader.worker_exited_with_error();
            } else {
                self.preparation_failed.set(true);
            }
        }
        self.app.borrow_mut().finish(op);
        Some(success)
    }
    fn task_status(&self, audio: bool, status: String) {
        let mut app = self.app.borrow_mut();
        if audio {
            app.audio_status = status;
        } else {
            app.status = status;
        }
    }
    fn refresh(self: &Rc<Self>) -> Duration {
        if self.updating.replace(true) {
            return Duration::from_millis(50);
        }
        self.refresh_typography();
        let Some(ui) = self.ui.upgrade() else {
            self.updating.set(false);
            return Duration::from_millis(50);
        };
        let audio_result = self.poll_task(true, &ui);
        let file_result = self.poll_task(false, &ui);
        let continuation = actions::poll_continuation(
            &mut self.after.borrow_mut(),
            self.audio_worker.borrow().is_some(),
            self.file_worker.borrow().is_some(),
            audio_result,
            file_result,
        );
        match continuation {
            actions::ContinuationPoll::Failed => {
                reply_native_quit(false);
            }
            actions::ContinuationPoll::Ready(after) => self.continue_with(after),
            actions::ContinuationPoll::Waiting => {}
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
        ui.set_busy(app.file_operation.is_some() || self.dialog_active.get());

        ui.set_playback_busy(audio);
        ui.set_exporting(app.file_operation == Some(Operation::Export));
        ui.set_transport_enabled(audio || app.file_operation == Some(Operation::Export));
        ui.set_transport_action(if audio { "Stop audio" } else { "" }.into());
        ui.set_can_edit(
            !matches!(
                app.file_operation,
                Some(Operation::Export | Operation::Import)
            ) && self.after.borrow().is_none()
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
        let pending = app.workbench.pending_view(app.workbench.rate(app.rate));
        let input_view = pending.as_ref().unwrap_or(&app.workbench);
        let mut errors = input_view
            .metadata_errors
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let timing = input_view.draft.saved_audio_diagnostics(app.rate);
        let duration_error = input_view
            .metadata_errors
            .get("duration")
            .cloned()
            .or(timing.duration);
        let fade_error = input_view
            .metadata_errors
            .get("fade")
            .cloned()
            .or(timing.fade);
        for error in [&duration_error, &fade_error].into_iter().flatten() {
            if !errors.contains(error) {
                errors.push(error.clone());
            }
        }
        ui.set_duration_error(duration_error.unwrap_or_default().into());
        ui.set_fade_error(fade_error.unwrap_or_default().into());
        ui.set_metadata_error(errors.join("; ").into());
        ui.set_peak(if pending.is_some() {
            format!("Pending validation · {}", input_view.peak).into()
        } else {
            input_view.peak.as_str().into()
        });
        self.sync_rows(&app, input_view);
        let validation = if app.editing {
            app.workbench.draft.preset().at(app.rate)
        } else {
            app.preset().at(app.rate)
        };
        sync_action_buttons(&ui, &app, self.dialog_active.get());
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
        let unrelated_audio = audio
            && app.editing
            && !self
                .audio_worker
                .borrow()
                .as_ref()
                .is_some_and(|worker| worker.owns_draft(&app.workbench.identity));
        ui.set_retained_warning(if unrelated_audio {
            format!("Editing a separate sound; current audio remains “{}”. Stop, then Play this draft to replace it.{}{}", self.session_title.borrow(), if retained.is_empty() { "" } else { "\n" }, retained)
        } else { retained }.into());
        ui.set_status(
            [app.status.as_str(), app.audio_status.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
                .into(),
        );
        ui.set_file_activity(if let Some(op) = app.file_operation {
            match op { Operation::Save => "Saving an immutable sound snapshot; audio and later edits remain independent.", Operation::Export => "Exporting WAV. Cancel export is separate from Stop audio.", _ => "Opening sound; the playing source remains unchanged." }
        } else { "" }.into());
        if audio && let Some(worker) = self.audio_worker.borrow().as_ref() {
            self.session_position.set(worker.progress.frames());
        }
        let window =
            [2_u32, 10, 40, 100][usize::try_from(ui.get_window_index()).unwrap_or(2).min(3)];
        let mut current_capture_error = None;
        let mut current_monitor_warning = None;
        let mut backend_underruns = 0;
        let mut monitor = self.monitor.borrow_mut();

        if let Some(reader) = monitor.as_mut() {
            reader.poll(window);
            let status = reader.status();
            let descriptor = reader.descriptor();
            let phase = status.phase;
            if audio && phase == auditorylab::PlaybackPhase::Preparing {
                ui.set_transport_action("Cancel".into());
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
            if !ui.get_held() || self.capture.borrow().is_none() {
                match auditorylab::WaveformCapture::monitored(reader, scale) {
                    Ok(Some(capture)) => *self.capture.borrow_mut() = Some(capture),
                    Ok(None) => {}
                    Err(error) => {
                        current_capture_error = Some(format!(
                            "Invalid waveform capture ({error:?}); retaining the last valid window"
                        ))
                    }
                }
            }
            current_monitor_warning = auditorylab::monitor_warning(reader, window);
            backend_underruns = status.underruns;
            ui.set_monitor_state(format!("Current session {}: observed frame {:?} · status age {:?} · gaps {} · output underruns {}",status.session_id,status.observation.map(|o|o.frame),reader.age(),status.gaps,status.underruns).into());
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

            ui.set_elapsed("".into());
            ui.set_generated("".into());
            ui.set_generated_details("".into());
            ui.set_meters("".into());

            ui.set_device(format!("Preview/export sample rate: {} Hz", app.rate.hz()).into());
            if !audio
                && !self.preparation_failed.get()
                && (!ui.get_held() || self.capture.borrow().is_none())
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
            ui.set_monitor_state(
                if audio {
                    "Waiting for generated samples"
                } else {
                    "Seekable preview; not generated playback"
                }
                .into(),
            );
        }
        sync_monitor_warnings(
            &ui,
            current_monitor_warning,
            current_capture_error.as_deref(),
            backend_underruns,
            self.capture
                .borrow()
                .as_ref()
                .is_some_and(auditorylab::WaveformCapture::clipped),
        );
        let output_state = auditorylab::output_monitor_state(
            monitor.as_ref(),
            self.capture.borrow().as_ref(),
            ui.get_held(),
            audio,
        );
        ui.set_output_state(
            if current_capture_error.is_some() {
                auditorylab::OutputMonitorState::Unavailable.label()
            } else {
                output_state.label()
            }
            .into(),
        );
        ui.set_receipt_age(
            monitor
                .as_ref()
                .and_then(|r| r.age())
                .map_or_else(
                    || "No generated publication received".into(),
                    |age| {
                        let label = if ui.get_held() {
                            "Current session receipt"
                        } else {
                            "Receipt"
                        };
                        format!("{label} {} ms ago", age.as_millis())
                    },
                )
                .into(),
        );
        let ended = monitor.as_ref().is_some_and(|r| {
            matches!(
                r.status().phase,
                auditorylab::PlaybackPhase::Stopped
                    | auditorylab::PlaybackPhase::Completed
                    | auditorylab::PlaybackPhase::Failed
            )
        });
        if let Some(capture) = self.capture.borrow().as_ref() {
            ui.set_capture_duration(format!("{:.2} ms", capture.milliseconds()).into());
            let [left, right] = scope::paths(capture.bins(), capture.limit());
            ui.set_left_path(left.into());
            ui.set_right_path(right.into());
            ui.set_scale_label(auditorylab::compact_number(capture.limit()).into());
            ui.set_waveform_caption(capture.label(ui.get_held(), ended).into());
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
                    capture.caption(ui.get_held(), ended),
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
        let model_target = if app.editing {
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
                        " · static pattern explanation",
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
                                    id: l.id.to_string().into(),
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

        let progress = self
            .file_worker
            .borrow()
            .as_ref()
            .map_or(0, |w| w.progress.frames());
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            reason = "quality:gui-main-progress-fraction"
        )]
        let progress_fraction =
            if app.file_operation == Some(Operation::Export) && self.operation_frames.get() > 0 {
                (progress as f64 / self.operation_frames.get() as f64) as f32
            } else {
                0.0
            };
        ui.set_progress(progress_fraction.clamp(0.0, 1.0));
        self.sync_auxiliary(&ui, audio);
        self.updating.set(false);
        Duration::from_millis(50)
    }
    fn sync_auxiliary(&self, ui: &MainWindow, audio: bool) {
        let focused_monitor = ui.get_monitor_open() && !ui.get_drawer_fits();
        if let Some(window) = self.monitor_window.upgrade() {
            window.set_data(ui.get_monitor_data());
            if focused_monitor != self.monitor_window_visible.replace(focused_monitor) {
                if focused_monitor {
                    let _ = window.show();
                } else {
                    // Hiding can transfer native focus; sample ownership first.
                    let was_focused = window
                        .window()
                        .with_winit_window(winit::window::Window::has_focus)
                        .unwrap_or(false);
                    if let Some(details) = self.details_window.upgrade()
                        && details.window().is_visible()
                    {
                        details_parent::bind(&details, ui);
                    }
                    hide_monitor_window(ui, &window, was_focused);
                }
            }
        }
        #[cfg(target_os = "macos")]
        native_application::audio_cancel(
            self.audio_worker
                .borrow()
                .as_ref()
                .map(|worker| worker.cancel.clone()),
        );
        let controls = self.dialog_active.get() && audio && !cfg!(target_os = "macos");
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
        if let Some(window) = self.details_window.upgrade() {
            present_details(&window, parent);
        }
    }
    fn refresh_diagnostics(&self) {
        let (Some(ui), Some(window)) = (self.ui.upgrade(), self.details_window.upgrade()) else {
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
    fn sync_rows(&self, app: &App, input_view: &app::Workbench) {
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
                error: input_view
                    .messages
                    .get(id)
                    .map_or("", String::as_str)
                    .into(),
                name: format!("{} · layer {}", auditorylab::component_role(c), index + 1).into(),
                heading: auditorylab::component_role(c).into(),
                context: c
                    .facts(rate)
                    .map(|f| auditorylab::component_description(&f, false))
                    .unwrap_or_else(|e| e.to_string())
                    .into(),
                ..Row::default()
            };
            row.error0 = input_view
                .field_error(*id, auditorylab::Setting::ALL[0], rate)
                .into();
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
            row.error1 = input_view
                .field_error(*id, auditorylab::Setting::ALL[1], rate)
                .into();
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
            row.error2 = input_view
                .field_error(*id, auditorylab::Setting::ALL[2], rate)
                .into();
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
            row.error3 = input_view
                .field_error(*id, auditorylab::Setting::ALL[3], rate)
                .into();
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
            row.error4 = input_view
                .field_error(*id, auditorylab::Setting::ALL[4], rate)
                .into();
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
            row.error5 = input_view
                .field_error(*id, auditorylab::Setting::ALL[5], rate)
                .into();
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
            row.error6 = input_view
                .field_error(*id, auditorylab::Setting::ALL[6], rate)
                .into();
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
            row.error7 = input_view
                .field_error(*id, auditorylab::Setting::ALL[7], rate)
                .into();
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
        if matches!(
            app.file_operation,
            Some(Operation::Export | Operation::Import)
        ) {
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
        if self.app.borrow().operation.is_some()
            || self.app.borrow().file_operation.is_some()
            || self.dialog_active.get()
        {
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
            let text = match &work {
                Work::Export(sound, path) => {
                    format!("Exporting {} to {path:?}.", sound.preset().title())
                }
                Work::Save(draft, path, _) => format!("Saving {} to {path:?}.", draft.title()),
                Work::Import(path) => format!("Opening {path:?}."),
                _ => String::new(),
            };
            self.task_status(op.is_audio(), text);
            if !op.is_audio() {
                self.operation_frames.set(match &work {
                    Work::Export(sound, _) => sound.signal().frames(),
                    _ => 0,
                });
            }
            if matches!(op, Operation::Play | Operation::Live) {
                *self.session_title.borrow_mut() = match &work {
                    Work::Play(p) => p.title(),
                    Work::Live(d, _) => d.title().into(),
                    _ => String::new(),
                };
                *self.monitor.borrow_mut() = None;
                if self.ui.upgrade().is_none_or(|ui| !ui.get_held()) {
                    *self.capture.borrow_mut() = None;
                }
                self.session_position.set(0);
            }
            let slot = if op.is_audio() {
                &self.audio_worker
            } else {
                &self.file_worker
            };
            *slot.borrow_mut() = Some(Worker::start(work));
            self.schedule();
        }
    }
}
/// Both the toolbar and persistent transport consume this exact candidate admission.
fn sync_action_buttons(ui: &MainWindow, app: &App, dialog_active: bool) {
    let candidate = app.workbench.action_draft(app.workbench.rate(app.rate));
    let playable = if app.editing {
        candidate
            .as_ref()
            .is_some_and(|draft| draft.target(app.workbench.rate(app.rate)).is_ok())
    } else {
        app.preset().at(app.rate).is_ok()
    };
    let action_available = !dialog_active
        && !matches!(
            app.file_operation,
            Some(Operation::Export | Operation::Import)
        );
    ui.set_can_play(playable && action_available);
    let save_error = candidate.as_ref().map_or_else(
        || Some("Complete or correct pending numeric fields before saving.".to_owned()),
        |draft| draft.to_toml().err().map(|e| e.to_string()),
    );
    ui.set_can_save(save_error.is_none());
    ui.set_save_reason(save_error.unwrap_or_default().into());
    ui.set_can_export(
        app.operation.is_none()
            && if app.editing {
                candidate
                    .as_ref()
                    .is_some_and(|draft| draft.preset().at(app.rate).is_ok())
            } else {
                app.preset().at(app.rate).is_ok()
            },
    );
    ui.set_play_target(if app.editing {
        app.workbench.draft.title().into()
    } else {
        app.preset().title().into()
    });
    ui.set_transport_enabled(app.operation.is_some() || playable && action_available);
    ui.set_transport_action(
        if app.operation.is_some() {
            "Stop audio"
        } else if app.editing {
            "Play live"
        } else {
            "Play selected sound"
        }
        .into(),
    );
}
#[expect(clippy::cast_possible_truncation, reason = "quality:gui-main-position")]
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
    let result = run();
    #[cfg(target_os = "windows")]
    if let Err(error) = &result {
        let _ = rfd::MessageDialog::new()
            .set_title("Auditory Lab error")
            .set_description(format!("Auditory Lab could not continue.\n\n{error}"))
            .set_level(rfd::MessageLevel::Error)
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }
    result
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
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
    let monitor_window = MonitorWindow::new()?;
    let details_window = DetailsWindow::new()?;
    let audio_controls = AudioControlsWindow::new()?;
    let font = typography::resolve();
    ui.set_ui_family(font.family.as_str().into());
    ui.set_ui_size(font.size);
    about.set_ui_family(font.family.as_str().into());
    about.set_ui_size(font.size);
    ui.set_native_mac(cfg!(target_os = "macos"));
    ui.set_held(false);
    about.set_identity(about_identity().into());
    let desktop = Rc::new(Desktop {
        updating: Cell::new(false),
        catalog_dirty: Cell::new(true),
        app: RefCell::new(App::new(auditorylab::catalog()?)),
        audio_worker: RefCell::new(None),
        file_worker: RefCell::new(None),
        ui: ui.as_weak(),
        timer: slint::Timer::default(),
        capture: RefCell::new(None),
        model: RefCell::new(None),
        typography: RefCell::new(typography::TypographyService::start()),
        font: RefCell::new(font),
        motion: RefCell::new(motion::Service::start()),
        motion_preference: Cell::new(motion::Preference::Unknown),
        motion_override: Cell::new(None),
        about: about.as_weak(),
        monitor_window: monitor_window.as_weak(),
        details_window: details_window.as_weak(),
        audio_controls: audio_controls.as_weak(),
        monitor_window_visible: Cell::new(false),
        audio_controls_visible: Cell::new(false),
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
    let viewport_refresh = Rc::new(slint::Timer::default());
    let d = desktop.clone();
    let deferred_refresh = viewport_refresh.clone();
    ui.on_window_changed(move || {
        if !d.updating.get() {
            let d = Rc::downgrade(&d);
            // Layout change handlers also run inside drawing. Native window
            // transitions must wait until the current graphics frame completes.
            deferred_refresh.start(slint::TimerMode::SingleShot, Duration::ZERO, move || {
                if let Some(d) = d.upgrade() {
                    d.refresh();
                }
            });
        }
    });
    let d = desktop.clone();
    ui.on_hold_changed(move || {
        if let Some(ui) = d.ui.upgrade() {
            d.motion_override.set(Some(ui.get_held()));
        }
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_follow_motion(move || {
        d.motion_override.set(None);
        if let Some(ui) = d.ui.upgrade() {
            ui.set_held(d.motion_preference.get() == motion::Preference::Reduced);
        }
        d.refresh();
    });
    let d = desktop.clone();
    ui.on_stop_audio({
        let d = desktop.clone();
        move || {
            if let Some(worker) = d.audio_worker.borrow().as_ref() {
                worker.cancel.request();
            }
        }
    });
    ui.on_play_stop(move || {
        let cancelled = {
            let worker = d.audio_worker.borrow();
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
                Work::Live(app.workbench.draft.clone(), app.workbench.identity.clone())
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
    ui.on_cancel_file(move || {
        if let Some(worker) = d.file_worker.borrow().as_ref() {
            worker.cancel.request();
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
        if matches!(
            app.file_operation,
            Some(Operation::Export | Operation::Import)
        ) {
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
        if matches!(
            app.file_operation,
            Some(Operation::Export | Operation::Import)
        ) {
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
        if matches!(
            app.file_operation,
            Some(Operation::Export | Operation::Import)
        ) {
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
        if matches!(
            app.file_operation,
            Some(Operation::Export | Operation::Import)
        ) {
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
        if matches!(
            app.file_operation,
            Some(Operation::Export | Operation::Import)
        ) {
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
        if !matches!(
            app.file_operation,
            Some(Operation::Export | Operation::Import)
        ) {
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
        if d.audio_worker.borrow().is_none() {
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

    let d = desktop.clone();
    ui.on_show_diagnostics(move || {
        if let Some(ui) = d.ui.upgrade() {
            d.show_diagnostics(&ui);
        }
    });
    let d = desktop.clone();
    ui.on_show_research(move || {
        let (Some(window), Some(ui)) = (d.details_window.upgrade(), d.ui.upgrade()) else {
            return;
        };
        let app = d.app.borrow();
        window.set_heading(format!("Sound details · {}", app.preset().title()).into());
        window.set_can_refresh(false);
        window.set_content(app::details(app.preset(), app.rate).into());
        present_details(&window, &ui);
    });
    let weak = details_window.as_weak();
    details_window.on_dismiss(move || {
        if let Some(window) = weak.upgrade() {
            let _ = window.hide();
        }
    });
    let d = desktop.clone();
    details_window.on_refresh_content(move || d.refresh_diagnostics());
    let d = desktop.clone();
    monitor_window.on_span(move |index| {
        if let Some(ui) = d.ui.upgrade() {
            ui.set_window_index(index);
        }
        d.refresh();
    });
    let d = desktop.clone();
    monitor_window.on_scale(move |index| {
        if let Some(ui) = d.ui.upgrade() {
            ui.set_scale_index(index);
        }
        d.refresh();
    });
    let weak = ui.as_weak();
    monitor_window.on_hold(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_held(!ui.get_held());
            ui.invoke_hold_changed();
        }
    });
    let weak = ui.as_weak();
    monitor_window.on_follow(move || {
        if let Some(ui) = weak.upgrade() {
            ui.invoke_follow_motion();
        }
    });
    let d = desktop.clone();
    monitor_window.on_details(move || {
        if let Some(monitor) = d.monitor_window.upgrade() {
            d.show_diagnostics(&monitor);
        }
    });
    let d = desktop.clone();
    monitor_window.on_dismiss(move || {
        if let Some(ui) = d.ui.upgrade() {
            ui.set_monitor_open(false);
            ui.invoke_focus_monitor_opener();
        }
        d.refresh();
    });
    let weak = monitor_window.as_weak();
    monitor_window.window().on_close_requested(move || {
        if let Some(window) = weak.upgrade() {
            window.invoke_dismiss();
        }
        slint::CloseRequestResponse::KeepWindowShown
    });
    let d = desktop.clone();
    audio_controls.on_stop(move || {
        if let Some(worker) = d.audio_worker.borrow().as_ref() {
            worker.cancel.request();
        }
    });
    audio_controls
        .window()
        .on_close_requested(|| slint::CloseRequestResponse::KeepWindowShown);
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
        move || {
            d.refresh();
        },
    );
    desktop.refresh();
    let result = ui.run();
    native_ui_timer.stop();
    viewport_refresh.stop();
    for slot in [&desktop.audio_worker, &desktop.file_worker] {
        if let Some(worker) = slot.borrow().as_ref() {
            worker.cancel.request();
        }
    }
    let mut cleanup_errors = Vec::new();
    for slot in [&desktop.audio_worker, &desktop.file_worker] {
        if let Some(worker) = slot.borrow_mut().take()
            && let Err(error) = worker.shutdown()
        {
            eprintln!("Shutdown: {error}");
            cleanup_errors.push(error);
        }
    }
    desktop.timer.stop();
    desktop.typography.borrow_mut().shutdown();
    desktop.motion.borrow_mut().shutdown();
    #[cfg(target_os = "macos")]
    native_application::shutdown();
    result?;
    if !cleanup_errors.is_empty() {
        return Err(cleanup_errors.join("\n").into());
    }
    Ok(())
}

#[cfg(test)]
mod action_wiring_tests {
    use super::*;
    #[cfg(target_os = "macos")]
    fn assert_native_font_metrics() {
        slint::slint! {
            export component FontProbe inherits Window {
                in property <string> family;
                in property <length> size;
                in property <string> sample;
                out property <length> x-height: glyph.font-metrics.x-height;
                out property <length> advance: glyph.preferred-width;
                glyph := Text { text: root.sample; font-family: root.family; font-size: root.size; }
            }
        }
        let probe = FontProbe::new().unwrap();
        probe.set_family(typography::resolve().family.into());
        probe.set_sample("x".repeat(100).into());
        for scale in [1.0, 2.0] {
            probe
                .window()
                .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
                    scale_factor: scale,
                });
            for size in [13.0, 26.0, 40.0] {
                probe.set_size(size);
                let native = objc2_app_kit::NSFont::systemFontOfSize(f64::from(size));
                let glyph = native.glyphWithName(&objc2_foundation::NSString::from_str("x"));
                let native_height = native.xHeight();
                let native_advance = native.advancementForGlyph(glyph).width * 100.0;
                let height = f64::from(probe.get_x_height());
                let advance = f64::from(probe.get_advance());
                eprintln!(
                    "SF {size} at {scale}x: Slint x-height {height}, native {native_height}; Slint advance {advance}, native {native_advance}"
                );
                assert!(
                    (height - native_height).abs() < 0.015,
                    "native SF x-height at {size}"
                );
                assert!(
                    (advance - native_advance).abs() < 1.1,
                    "native SF glyph advance at {size}"
                );
            }
        }
    }
    #[test]
    fn corrected_pending_frequency_enables_the_actual_transport_and_save_properties() {
        use slint::platform::{
            LayoutConstraints, Platform, PlatformError, Renderer, WindowAdapter, WindowProperties,
            software_renderer::SoftwareRenderer,
        };
        struct Offscreen {
            auxiliary_constraints: Rc<RefCell<Option<LayoutConstraints>>>,
        }
        struct OffscreenWindow {
            window: slint::Window,
            renderer: SoftwareRenderer,
            size: Cell<slint::PhysicalSize>,
            auxiliary_constraints: Rc<RefCell<Option<LayoutConstraints>>>,
        }
        impl WindowAdapter for OffscreenWindow {
            fn window(&self) -> &slint::Window {
                &self.window
            }
            fn renderer(&self) -> &dyn Renderer {
                &self.renderer
            }
            fn size(&self) -> slint::PhysicalSize {
                self.size.get()
            }
            fn set_size(&self, size: slint::WindowSize) {
                let scale = self.window.scale_factor();
                self.size.set(size.to_physical(scale));
                self.window
                    .dispatch_event(slint::platform::WindowEvent::Resized {
                        size: size.to_logical(scale),
                    });
            }
            fn update_window_properties(&self, properties: WindowProperties<'_>) {
                if matches!(
                    properties.title().as_str(),
                    "Audio controls" | "Output monitor"
                ) {
                    *self.auxiliary_constraints.borrow_mut() =
                        Some(properties.layout_constraints());
                }
            }
        }
        impl Platform for Offscreen {
            fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
                Ok(Rc::new_cyclic(|weak| OffscreenWindow {
                    window: slint::Window::new(weak.clone()),
                    renderer: SoftwareRenderer::new(),
                    size: Cell::default(),
                    auxiliary_constraints: self.auxiliary_constraints.clone(),
                }))
            }
        }
        let auxiliary_constraints = Rc::new(RefCell::new(None));
        slint::platform::set_platform(Box::new(Offscreen {
            auxiliary_constraints: auxiliary_constraints.clone(),
        }))
        .unwrap();
        assert_audio_controls_keep_stop_visible(&auxiliary_constraints);
        crate::text_edit_tests::assert_cursor_rect_scrolls_into_view();
        crate::focus_scroll_tests::assert_control_focus_scrolls_into_view();
        assert_platform_modifiers_drive_text_shortcuts();
        #[cfg(target_os = "macos")]
        assert_native_font_metrics();
        let ui = MainWindow::new().unwrap();
        let mut app = App::new(auditorylab::catalog().unwrap());
        app.editing = true;
        let id = app
            .workbench
            .draft
            .add(auditorylab::SourceKind::Tone)
            .unwrap();
        app.workbench
            .edit(id, auditorylab::Setting::Frequency, "bad", app.rate);
        sync_action_buttons(&ui, &app, false);
        assert!(!ui.get_can_play());
        assert!(!ui.get_transport_enabled());
        assert!(!ui.get_can_save());
        app.workbench
            .pending
            .insert((id, auditorylab::Setting::Frequency), "3750".into());
        // Negative control: the old committed-error branch still rejects this valid pending correction.
        assert!(!app.workbench.parse_errors.is_empty());
        sync_action_buttons(&ui, &app, false);
        assert!(ui.get_can_play());
        assert!(
            ui.get_transport_enabled(),
            "the persistent transport must share candidate admission"
        );
        assert!(ui.get_can_save());
        assert_eq!(ui.get_transport_action(), "Play live");
        app.workbench
            .pending
            .insert((id, auditorylab::Setting::Frequency), "NaN".into());
        sync_action_buttons(&ui, &app, false);
        assert!(!ui.get_transport_enabled());
        assert!(!ui.get_can_save());
        app.workbench
            .pending
            .insert((id, auditorylab::Setting::Frequency), "3750".into());
        app.workbench.commit_pending(app.rate);
        assert!(app.workbench.parse_errors.is_empty());
        app.start(Operation::Live);
        app.start(Operation::Save);
        sync_action_buttons(&ui, &app, true);
        assert!(
            ui.get_transport_enabled(),
            "Stop remains enabled during the real dialog reservation"
        );
        assert_eq!(ui.get_transport_action(), "Stop audio");
        ui.show().unwrap();
        ui.window().set_size(slint::PhysicalSize::new(1100, 900));
        slint::platform::update_timers_and_animations();
        assert!(ui.get_drawer_fits());
        ui.set_scale_label("≈0.0316228".into());
        ui.set_waveform_caption(
            "Preview · not playing · Untitled sound · 40.00 ms · Fit ±≈0.0316228".into(),
        );
        ui.set_meters(
            "Captured sample peak L -30.0 / R -30.0 dBFS · RMS L -33.0 / R -33.0 dBFS".into(),
        );
        ui.set_motion_reason(
            "System motion preference permits live drawing. User chose Return to live.".into(),
        );
        ui.set_monitor_warning("Capture coverage is incomplete; waiting for fresh samples.".into());
        ui.invoke_resize_monitor(240.0);
        slint::platform::update_timers_and_animations();
        assert!(ui.get_drawer_fits());
        assert!(
            ui.get_effective_drawer_height() > 240.0,
            "readable axis labels and current explanations must raise the drawer's content minimum"
        );
        ui.set_motion_reason("".into());
        ui.set_monitor_warning("".into());
        ui.window().set_size(slint::PhysicalSize::new(680, 700));
        slint::platform::update_timers_and_animations();
        // The readable content minimum may exceed the viewport budget. In that
        // case the drawer is hidden and its calculated content height is not a
        // displayed pane; the focused monitor preserves the open choice.
        for requested_height in [240.0, 360.0] {
            ui.invoke_resize_monitor(requested_height);
            slint::platform::update_timers_and_animations();
            if ui.get_drawer_fits() {
                assert!(
                    (240.0..=285.0).contains(&ui.get_effective_drawer_height()),
                    "a visible minimum-window drawer must retain document space"
                );
            } else {
                assert!(
                    ui.get_monitor_open(),
                    "focused fallback preserves the open choice"
                );
            }
        }
        ui.set_ui_size(26.0);
        slint::platform::update_timers_and_animations();
        assert!(
            !ui.get_drawer_fits(),
            "enlarged text must use the focused monitor window"
        );
        assert!(
            ui.get_monitor_open(),
            "responsive fallback must preserve the user's open choice"
        );
        assert_monitor_handoff_preserves_numeric_entry(&ui);
        crate::focus_reflow_tests::assert_pending_entry_survives_font_reflow(&ui);
        assert_monitor_warning_accessibility(&ui);
        assert_monitor_lane_geometry(&ui, &auxiliary_constraints);
        assert_monitor_layout_survives_warning_changes(&ui);
        ui.hide().unwrap();
    }

    fn assert_monitor_layout_survives_warning_changes(ui: &MainWindow) {
        fn settle(ui: &MainWindow) {
            slint::platform::update_timers_and_animations();
            // Materialize actual conditional delegates, as the native frame does.
            slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window())
                .draw_contents(|_, _| ());
            let _ = ui.get_drawer_fits();
            slint::platform::update_timers_and_animations();
        }
        ui.set_monitor_open(true);
        ui.set_motion_reason("System motion preference permits live drawing.".into());
        ui.invoke_resize_monitor(270.0);
        let mut observed_fit = false;
        let mut observed_fallback = false;
        let mut changed_warning_bounds = false;
        for size in [13.0, 26.0, 40.0] {
            ui.set_ui_size(size);
            for width in [680, 1100] {
                for height in (700..=1200).step_by(5) {
                    ui.window()
                        .set_size(slint::PhysicalSize::new(width, height));
                    sync_monitor_warnings(ui, None, None, 0, false);
                    settle(ui);
                    let fits = ui.get_drawer_fits();
                    let baseline = ui.get_effective_drawer_height();
                    observed_fit |= fits;
                    observed_fallback |= !fits;
                    for (warning, underruns, clipped) in [
                        (
                            Some(auditorylab::MonitorWarning::IncompleteCoverage),
                            0,
                            false,
                        ),
                        (None, 1, false),
                        (
                            Some(auditorylab::MonitorWarning::IncompleteCoverage),
                            1,
                            true,
                        ),
                    ] {
                        sync_monitor_warnings(ui, warning, None, underruns, clipped);
                        settle(ui);
                        assert_eq!(
                            ui.get_drawer_fits(),
                            fits,
                            "warning refresh must not move the monitor between windows at font{size}, {width}x{height}"
                        );
                        changed_warning_bounds |= ui.get_effective_drawer_height() != baseline;
                    }
                }
            }
        }
        assert!(
            !changed_warning_bounds,
            "warning rows must keep stable readable bounds"
        );
        assert!(
            observed_fit && observed_fallback,
            "layout control must exercise both drawer and focused-window configurations"
        );
        ui.set_ui_size(13.0);
        ui.window().set_size(slint::PhysicalSize::new(1100, 900));
        sync_monitor_warnings(ui, None, None, 0, false);
        settle(ui);
        assert!(ui.get_drawer_fits());
        eprintln!(
            "stable-warning drawer13px1100x900={} logicalpx",
            ui.get_effective_drawer_height()
        );
        assert!(
            (240.0..=285.0).contains(&ui.get_effective_drawer_height()),
            "stable warnings must preserve the normal bounded drawer target"
        );
    }

    fn assert_monitor_lane_geometry(
        ui: &MainWindow,
        constraints: &Rc<RefCell<Option<slint::platform::LayoutConstraints>>>,
    ) {
        use slint::private_unstable_api::re_exports::{
            AccessibleStringProperty, ItemRc, Path, WindowInner,
        };
        use std::ops::ControlFlow;
        fn bounds(item: &ItemRc) -> (f32, f32, f32, f32) {
            let geometry = item.geometry();
            let origin = item.map_to_window(geometry.origin);
            (
                origin.x,
                origin.y,
                geometry.size.width,
                geometry.size.height,
            )
        }
        fn settle(window: &slint::Window) {
            slint::platform::update_timers_and_animations();
            WindowInner::from_pub(window).draw_contents(|_, _| ());
            slint::platform::update_timers_and_animations();
        }
        fn check(window: &slint::Window, size: f32, context: &str, expected_controls: usize) {
            let viewport = window.size().to_logical(window.scale_factor());
            let root = WindowInner::from_pub(window).window_item_rc().unwrap();
            let mut lanes = Vec::new();
            let mut controls = 0;
            let _ = root.visit_descendants(|item| {
                if context == "drawer" && !item.is_visible() { return ControlFlow::<()>::Continue(()) }
                if let Some(label) = item.accessible_string_property(AccessibleStringProperty::Label) {
                    if label.starts_with("Left samples,") || label.starts_with("Right samples,") {
                        lanes.push(item.clone());
                    }
                    if matches!(label.as_str(), "Output monitor" | "Waveform time window" | "Waveform amplitude scale" | "Hold display" | "Return to live" | "Full audio diagnostics and capture source" | "Hide output monitor" | "Follow system motion preference") {
                        let (x,y,width,height) = bounds(item);
                        assert!(width > 0.0 && height > 0.0, "{context}: {label} needs a usable extent");
                        if context == "drawer" {
                            assert!(x >= 0.0 && y >= 0.0 && x+width <= viewport.width+0.01 && y+height <= viewport.height+0.01, "{context} at{size}px: {label} must remain inside its window, got{:?} in{viewport:?}", (x,y,width,height));
                        }
                        controls += 1;
                    }
                }
                ControlFlow::<()>::Continue(())
            });
            assert_eq!(
                controls, expected_controls,
                "all monitor header and chart controls remain reachable"
            );
            assert_eq!(lanes.len(), 2, "both ear lanes remain visible at {size}px");
            let mut plots = Vec::new();
            for lane in lanes {
                let lane_bounds = bounds(&lane);
                let mut labels = Vec::new();
                let mut plot = None;
                let _ = lane.visit_descendants(|item| {
                    if item.downcast::<Path>().is_some() {
                        plot = Some(bounds(item));
                    }
                    if let Some(label) =
                        item.accessible_string_property(AccessibleStringProperty::Label)
                        && !label.is_empty()
                    {
                        labels.push((label.to_string(), bounds(item)));
                    }
                    ControlFlow::<()>::Continue(())
                });
                let title = labels
                    .iter()
                    .find(|(label, _)| matches!(label.as_str(), "Left" | "Right"))
                    .unwrap()
                    .1;
                let positive = labels
                    .iter()
                    .find(|(label, _)| label.starts_with('+'))
                    .unwrap()
                    .1;
                let zero = labels.iter().find(|(label, _)| label == "0").unwrap().1;
                let negative = labels
                    .iter()
                    .find(|(label, _)| label.starts_with('−'))
                    .unwrap()
                    .1;
                let plot = plot.expect("the actual generated lane contains its sample path");
                plots.push(plot);
                eprintln!(
                    "Lane {context} at{size}px viewport{viewport:?}: lane={lane_bounds:?}, title={title:?}, +={positive:?}, 0={zero:?}, -={negative:?}, plot={plot:?}"
                );
                assert!(
                    title.0 + title.2 <= positive.0,
                    "ear label and numeric ticks need separate columns"
                );
                assert!(
                    (title.1 + title.3 / 2.0 - zero.1 - zero.3 / 2.0).abs() < 0.01,
                    "ear label stays centered on the numeric scale"
                );
                for tick in [positive, zero, negative] {
                    assert!(
                        tick.0 + tick.2 <= plot.0,
                        "full numeric labels remain outside the plot"
                    );
                    assert!(
                        tick.1 >= lane_bounds.1
                            && tick.1 + tick.3 <= lane_bounds.1 + lane_bounds.3 + 0.01,
                        "numeric labels remain inside their lane"
                    );
                }
                assert!(
                    plot.2 >= plot.3 * 2.0 - 0.01
                        && plot.0 + plot.2 <= lane_bounds.0 + lane_bounds.2 + 0.01,
                    "the plot must fit beside its readable gutter with at least twice its height in time-axis space"
                );
                assert!((positive.1 + positive.3 / 2.0 - plot.1).abs() < 0.01);
                assert!(
                    (negative.1 + negative.3 / 2.0 - plot.1 - plot.3).abs() < 0.01,
                    "the plot retains the full distance between the positive and negative ticks"
                );
            }
            assert!(
                (plots[0].0 - plots[1].0).abs() < 0.01
                    && (plots[0].2 - plots[1].2).abs() < 0.01
                    && (plots[0].3 - plots[1].3).abs() < 0.01,
                "both ears must retain the same plot origin, time scale, and amplitude height"
            );
        }
        ui.set_monitor_open(true);
        ui.invoke_resize_monitor(270.0);
        let window = MonitorWindow::new().unwrap();
        window.set_ui_family(ui.get_ui_family());
        let mut drawer_cases = 0;
        for size in [40.0, 32.666_668, 26.0, 13.0] {
            window.set_ui_size(size);
            window.show().unwrap();
            settle(window.window());
            let minimum = constraints.borrow().unwrap().min.unwrap();
            assert_eq!(
                minimum,
                slint::LogicalSize::new(680.0, 400.0),
                "focused content must not force a window beyond the desktop"
            );
            for (width, height) in [(680.0_f32, 700.0), (1280.0, 800.0)] {
                window
                    .window()
                    .set_size(slint::LogicalSize::new(width, height));
                for state in ["Live", "Held", "Unavailable"] {
                    let mut data = ui.get_monitor_data();
                    data.state = state.into();
                    data.age = "Current session receipt 123456 milliseconds ago; full receipt provenance remains accessible".into();
                    data.held = state == "Held";
                    data.overridden = true;
                    data.motion =
                        "System Reduce Motion is enabled: display held by default.".into();
                    data.backendwarning =
                        "Backend-reported output underruns: 8 · see details.".into();
                    window.set_data(data.clone());
                    settle(window.window());
                    check(window.window(), size, "focused", 7);
                    assert_focused_monitor_reachable(&window, size, &data);
                }
                ui.set_ui_size(size);
                ui.window().set_size(slint::LogicalSize::new(width, 1200.0));
                settle(ui.window());
                if ui.get_drawer_fits() {
                    drawer_cases += 1;
                    check(ui.window(), size, "drawer", 7);
                } else {
                    assert!(
                        ui.get_monitor_open(),
                        "responsive fallback preserves the open monitor choice"
                    );
                }
            }
            window.hide().unwrap();
        }
        assert!(
            drawer_cases > 0,
            "geometry checks must include the visible drawer"
        );
        ui.set_ui_size(13.0);
        ui.window().set_size(slint::LogicalSize::new(1100.0, 900.0));
    }

    fn assert_focused_monitor_reachable(
        window: &MonitorWindow,
        size: f32,
        data: &generated::MonitorData,
    ) {
        use slint::platform::{Key, WindowEvent};
        use slint::private_unstable_api::re_exports::{
            AccessibleStringProperty, ItemRc, Orientation, WindowInner,
        };
        use std::collections::BTreeSet;
        use std::ops::ControlFlow;
        fn settle(window: &slint::Window) {
            slint::platform::update_timers_and_animations();
            WindowInner::from_pub(window).draw_contents(|_, _| ());
            slint::platform::update_timers_and_animations();
        }
        fn bounds(item: &ItemRc) -> (f32, f32, f32, f32) {
            let geometry = item.geometry();
            let origin = item.map_to_window(geometry.origin);
            (
                origin.x,
                origin.y,
                geometry.size.width,
                geometry.size.height,
            )
        }
        fn key(window: &slint::Window, key: Key) {
            let text: slint::SharedString = key.into();
            window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            window.dispatch_event(WindowEvent::KeyReleased { text });
            settle(window);
        }
        let inner = WindowInner::from_pub(window.window());
        let root = inner.window_item_rc().unwrap();
        let find = |label: &str| {
            let mut matches = Vec::new();
            let _ = root.visit_descendants(|item| {
                if item
                    .accessible_string_property(AccessibleStringProperty::Label)
                    .is_some_and(|value| value == label)
                {
                    matches.push(item.clone());
                }
                ControlFlow::<()>::Continue(())
            });
            assert_eq!(matches.len(), 1, "one actual monitor item for {label}");
            matches.pop().unwrap()
        };
        let state = find(data.state.as_str());
        let state_bounds = bounds(&state);
        let state_intrinsic = state.borrow().as_ref().layout_info(
            Orientation::Horizontal,
            state_bounds.3,
            &inner.window_adapter(),
            &state,
        );
        assert!(
            state_bounds.2 + 0.1 >= state_intrinsic.preferred,
            "{} must retain its complete shaped width {} at {size}px: {state_bounds:?}",
            data.state,
            state_intrinsic.preferred
        );
        assert!(
            state_bounds.3 > size * 0.8,
            "monitor state cannot collapse vertically"
        );
        let age = find(data.age.as_str());
        let age_bounds = bounds(&age);
        let age_intrinsic = age.borrow().as_ref().layout_info(
            Orientation::Horizontal,
            age_bounds.3,
            &inner.window_adapter(),
            &age,
        );
        assert!(
            age_bounds.2 > 0.0 && age_bounds.2 + 0.1 >= age_intrinsic.min,
            "receipt age retains at least its shaped elision marker"
        );
        let age_line = age.borrow().as_ref().layout_info(
            Orientation::Vertical,
            age_bounds.2,
            &inner.window_adapter(),
            &age,
        );
        assert!(
            age_line.preferred <= size * 1.7 && age_bounds.3 + 0.1 >= age_line.preferred,
            "receipt age must fit its complete single shaped line at {size}px: {age_bounds:?}, {age_line:?}"
        );
        let labels = [
            "Full audio diagnostics and capture source",
            "Hide output monitor",
            "Waveform time window",
            "Waveform amplitude scale",
            if data.held {
                "Return to live"
            } else {
                "Hold display"
            },
            "Follow system motion preference",
        ];
        let controls: Vec<_> = labels.iter().map(|label| (*label, find(label))).collect();
        slint::slint! {
            import { Button } from "std-widgets.slint";
            export component MonitorButtonMeasure inherits Window {
                in property <string> family;
                in property <length> size;
                default-font-family:root.family;default-font-size:root.size;
                out property <length> natural-height:max(measure.min-height,measure.preferred-height);
                out property <length> widget-minimum-height:measure.min-height;
                out property <length> widget-preferred-height:measure.preferred-height;
                measure := Button {text:"Details…";}
            }
        }
        let measure = MonitorButtonMeasure::new().unwrap();
        measure.set_family(window.get_ui_family());
        measure.set_size(size);
        let natural_height = measure.get_natural_height();
        eprintln!(
            "Actual widget height oracle {size}px: minimum={}, preferred={}, admitted={natural_height}",
            measure.get_widget_minimum_height(),
            measure.get_widget_preferred_height()
        );
        assert!(
            natural_height > 0.0,
            "standard button height oracle is meaningful"
        );
        for (label, item) in &controls[..2] {
            assert!(
                bounds(item).3 <= natural_height + 0.1,
                "{label} must retain its natural height at {size}px: actual={:?}, preferred={natural_height}",
                bounds(item)
            );
        }
        let viewport = window
            .window()
            .size()
            .to_logical(window.window().scale_factor());
        let mut seen = BTreeSet::new();
        for reverse in [false, true] {
            let mut direction_seen = BTreeSet::new();
            if reverse {
                window.window().dispatch_event(WindowEvent::KeyPressed {
                    text: Key::Shift.into(),
                });
            }
            for _ in 0..18 {
                key(window.window(), Key::Tab);
                let focused = inner
                    .focus_item
                    .borrow()
                    .upgrade()
                    .expect("Tab focuses a monitor control");
                for (label, item) in &controls {
                    let mut contains_focus = *item == focused;
                    let _ = item.visit_descendants(|child| {
                        contains_focus |= *child == focused;
                        ControlFlow::<()>::Continue(())
                    });
                    if contains_focus {
                        let (x, y, width, height) = bounds(item);
                        if x < -0.1
                            || y < -0.1
                            || x + width > viewport.width + 0.1
                            || y + height > viewport.height + 0.1
                        {
                            eprintln!(
                                "Focus reveal diagnostic: focused={:?}, control={:?}",
                                bounds(&focused),
                                bounds(item)
                            );
                            let _ = root.visit_descendants(|candidate| {
                                if let Some(flick) = candidate.downcast::<slint::private_unstable_api::re_exports::Flickable>() {
                                    let flick = flick.as_pin_ref();
                                    eprintln!("Flickable {:?} content={:?}x{:?}, offset={:?},{:?}", bounds(candidate), flick.content_width(), flick.content_height(), flick.content_x(), flick.content_y());
                                }
                                ControlFlow::<()>::Continue(())
                            });
                        }
                        assert!(
                            x >= -0.1
                                && y >= -0.1
                                && x + width <= viewport.width + 0.1
                                && y + height <= viewport.height + 0.1,
                            "Tab reverse={reverse} must reveal {label} at {size}px: {:?} in {viewport:?}",
                            (x, y, width, height)
                        );
                        seen.insert(*label);
                        direction_seen.insert(*label);
                    }
                }
            }
            for (label, _) in &controls {
                if data.held
                    && matches!(*label, "Waveform time window" | "Waveform amplitude scale")
                {
                    continue;
                }
                assert!(
                    direction_seen.contains(label),
                    "Tab reverse={reverse} must reach {label}"
                );
            }
            if reverse {
                window.window().dispatch_event(WindowEvent::KeyReleased {
                    text: Key::Shift.into(),
                });
            }
        }
        for (label, _) in &controls {
            if data.held && matches!(*label, "Waveform time window" | "Waveform amplitude scale") {
                continue;
            }
            assert!(
                seen.contains(label),
                "actual Tab traversal must reach {label}"
            );
        }
        let flickable = root
            .visit_descendants(|item| {
                item.downcast::<slint::private_unstable_api::re_exports::Flickable>()
                    .map_or(ControlFlow::Continue(()), |flickable| {
                        ControlFlow::Break(flickable)
                    })
            })
            .unwrap();
        let flickable = flickable.as_pin_ref();
        let bottom = (viewport.height - flickable.content_height().get()).min(0.0);
        for _ in 0..8 {
            let before = flickable.content_y().get();
            key(window.window(), Key::PageDown);
            let after = flickable.content_y().get();
            assert!(
                after >= bottom - 0.1 && after <= before + 0.1 && after <= 0.1,
                "PageDown must advance within its actual scroll bounds"
            );
            if (after - before).abs() < 0.1 {
                break;
            }
        }
        assert!(
            (flickable.content_y().get() - bottom).abs() < 0.1,
            "keyboard paging must reach the warning end of the focused monitor"
        );
        key(window.window(), Key::PageUp);
        assert!(flickable.content_y().get() >= bottom - 0.1 && flickable.content_y().get() <= 0.1);
        window.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Control.into(),
        });
        key(window.window(), Key::End);
        window.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Control.into(),
        });
        settle(window.window());
        for text in [&data.motion, &data.backendwarning] {
            let (x, y, width, height) = bounds(&find(text.as_str()));
            assert!(
                width > 0.0
                    && height > 0.0
                    && x < viewport.width
                    && x + width > 0.0
                    && y >= -0.1
                    && y + height <= viewport.height + 0.1,
                "Ctrl/Cmd End must reach warning text with only the keyboard"
            );
        }
        window.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Control.into(),
        });
        key(window.window(), Key::Home);
        window.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Control.into(),
        });
        settle(window.window());
        let (x, y, width, height) = bounds(&find(data.state.as_str()));
        assert!(
            x >= -0.1
                && y >= -0.1
                && x + width <= viewport.width + 0.1
                && y + height <= viewport.height + 0.1,
            "Ctrl/Cmd Home must return left/top to the complete monitor state"
        );
        window
            .window()
            .dispatch_event(WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(viewport.width / 2.0, viewport.height / 2.0),
                delta_x: -100_000.0,
                delta_y: -100_000.0,
            });
        settle(window.window());
        for text in [&data.motion, &data.backendwarning] {
            let (x, y, width, height) = bounds(&find(text.as_str()));
            assert!(
                width > 0.0
                    && height > 0.0
                    && x < viewport.width
                    && x + width > 0.0
                    && y >= -0.1
                    && y + height <= viewport.height + 0.1,
                "scrolling must reach the complete warning row at {size}px: {:?}",
                (x, y, width, height)
            );
        }
        eprintln!(
            "Focused monitor {size}px {viewport:?} state={} reachable controls={seen:?}",
            data.state
        );
    }

    fn assert_monitor_warning_accessibility(ui: &MainWindow) {
        use slint::private_unstable_api::re_exports::{AccessibleStringProperty, WindowInner};
        use std::ops::ControlFlow;
        fn reveal_warnings(window: &slint::Window) {
            slint::platform::update_timers_and_animations();
            WindowInner::from_pub(window).draw_contents(|_, _| ());
            let viewport = window.size().to_logical(window.scale_factor());
            window.dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(viewport.width / 2.0, viewport.height / 2.0),
                delta_x: 0.0,
                delta_y: -100_000.0,
            });
            slint::platform::update_timers_and_animations();
            WindowInner::from_pub(window).draw_contents(|_, _| ());
        }
        let window = MonitorWindow::new().unwrap();
        window.show().unwrap();
        window
            .window()
            .set_size(slint::LogicalSize::new(950.0, 420.0));
        let root = WindowInner::from_pub(window.window())
            .window_item_rc()
            .unwrap();
        let find = |label: &str| {
            root.visit_descendants(|item| {
                if item.is_visible()
                    && item.is_accessible()
                    && item
                        .accessible_string_property(AccessibleStringProperty::Label)
                        .is_some_and(|value| value == label)
                {
                    ControlFlow::Break(item.clone())
                } else {
                    ControlFlow::Continue(())
                }
            })
        };
        let backend_text = "Backend-reported output underruns: 1 · see details.";
        sync_monitor_warnings(ui, None, None, 1, false);
        window.set_data(ui.get_monitor_data());
        reveal_warnings(window.window());
        let backend = find(backend_text).expect("independent backend warning must be accessible");
        assert_eq!(
            backend
                .accessible_string_property(AccessibleStringProperty::LiveRegion)
                .as_deref(),
            Some("polite")
        );
        for _ in 0..10 {
            let partial = auditorylab::MonitorWarning::IncompleteCoverage;
            sync_monitor_warnings(ui, Some(partial), None, 1, false);
            window.set_data(ui.get_monitor_data());
            reveal_warnings(window.window());
            let coverage =
                find(partial.message()).expect("current partial coverage remains readable");
            assert_eq!(
                coverage
                    .accessible_string_property(AccessibleStringProperty::LiveRegion)
                    .as_deref(),
                Some("off"),
                "transient coverage must not generate automatic speech"
            );
            assert!(
                find(backend_text).is_some_and(|item| item == backend),
                "coverage changes must not recreate the backend warning node"
            );
            sync_monitor_warnings(ui, None, None, 1, false);
            window.set_data(ui.get_monitor_data());
            reveal_warnings(window.window());
            assert!(find(partial.message()).is_none());
            assert!(find(backend_text).is_some_and(|item| item == backend));
        }
        for critical in [
            auditorylab::MonitorWarning::Stale,
            auditorylab::MonitorWarning::Unavailable,
        ] {
            sync_monitor_warnings(ui, Some(critical), None, 1, false);
            window.set_data(ui.get_monitor_data());
            reveal_warnings(window.window());
            let warning = find(critical.message()).unwrap();
            assert_eq!(
                warning
                    .accessible_string_property(AccessibleStringProperty::LiveRegion)
                    .as_deref(),
                Some("polite")
            );
            assert!(find(backend_text).is_some_and(|item| item == backend));
        }
        let capture_error =
            "Invalid waveform capture (non-finite samples); retaining the last valid window";
        sync_monitor_warnings(ui, None, Some(capture_error), 1, false);
        window.set_data(ui.get_monitor_data());
        slint::platform::update_timers_and_animations();
        assert_eq!(
            find(capture_error)
                .unwrap()
                .accessible_string_property(AccessibleStringProperty::LiveRegion)
                .as_deref(),
            Some("polite")
        );
        sync_monitor_warnings(ui, None, None, 1, true);
        window.set_data(ui.get_monitor_data());
        slint::platform::update_timers_and_animations();
        let advisory = find("Waveform exceeds the fixed display range; exact levels are retained. Use Fit to view the full extent.").unwrap();
        assert_eq!(
            advisory
                .accessible_string_property(AccessibleStringProperty::LiveRegion)
                .as_deref(),
            Some("off")
        );
        assert!(find(backend_text).is_some_and(|item| item == backend));
        sync_monitor_warnings(ui, None, None, 0, false);
        window.set_data(ui.get_monitor_data());
        slint::platform::update_timers_and_animations();
        assert!(find(backend_text).is_none());
        window.hide().unwrap();
    }

    fn assert_platform_modifiers_drive_text_shortcuts() {
        use slint::platform::{Key, WindowEvent};
        use slint::private_unstable_api::re_exports::{KeyboardModifiers, WindowInner};
        slint::slint! {
            export component ModifierProbe inherits Window {
                width:400px;height:80px;
                in-out property <string> value:"baseline";
                out property <bool> pressed-control;
                out property <bool> pressed-alt;
                out property <bool> pressed-shift;
                out property <bool> pressed-meta;
                out property <bool> released-control;
                public function focus-field(){field.focus();}
                field := TextInput {
                    text <=> root.value;
                    key-pressed(e)=>{
                        root.pressed-control=e.modifiers.control;
                        root.pressed-alt=e.modifiers.alt;
                        root.pressed-shift=e.modifiers.shift;
                        root.pressed-meta=e.modifiers.meta;
                        return reject;
                    }
                    key-released(e)=>{root.released-control=e.modifiers.control;return reject;}
                }
            }
        }
        fn press(probe: &ModifierProbe, text: impl Into<slint::SharedString>) {
            probe
                .window()
                .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
        }
        fn release(probe: &ModifierProbe, text: impl Into<slint::SharedString>) {
            probe
                .window()
                .dispatch_event(WindowEvent::KeyReleased { text: text.into() });
        }
        fn key(probe: &ModifierProbe, text: impl Into<slint::SharedString>) {
            let text = text.into();
            press(probe, text.clone());
            release(probe, text);
        }
        fn reset(probe: &ModifierProbe) {
            probe
                .window()
                .dispatch_event(WindowEvent::WindowActiveChanged(false));
            probe
                .window()
                .dispatch_event(WindowEvent::WindowActiveChanged(true));
            probe.invoke_focus_field();
        }
        let probe = ModifierProbe::new().unwrap();
        probe.show().unwrap();
        reset(&probe);
        let inner = WindowInner::from_pub(probe.window());
        let mut command = KeyboardModifiers::default();
        command.control = true;
        inner.set_keyboard_modifiers(KeyboardModifiers::default());
        press(&probe, Key::Shift);
        assert!(
            probe.get_pressed_shift(),
            "physical Shift callback precedes its aggregate report"
        );
        release(&probe, Key::Shift);
        press(&probe, Key::Control);
        assert!(probe.get_pressed_control());
        release(&probe, Key::Control);
        press(&probe, Key::Meta);
        assert!(probe.get_pressed_meta());
        release(&probe, Key::Meta);
        press(&probe, Key::Alt);
        assert!(probe.get_pressed_alt());
        release(&probe, Key::Alt);
        inner.set_keyboard_modifiers(command);
        key(&probe, "a");
        inner.set_keyboard_modifiers(KeyboardModifiers::default());
        key(&probe, "x");
        assert_eq!(
            probe.get_value(),
            "x",
            "flag-only command selects actual TextInput content"
        );
        key(&probe, "a");
        assert_eq!(
            probe.get_value(),
            "xa",
            "released platform flags cannot stick on plain input"
        );

        // Without platform reports, preserve the physical left/right tracker.
        reset(&probe);
        press(&probe, Key::Control);
        press(&probe, Key::ControlR);
        release(&probe, Key::Control);
        assert!(probe.get_released_control());
        release(&probe, Key::ControlR);
        assert!(!probe.get_released_control());

        // Winit can report physical releases before the next aggregate flags.
        // Keep the last authoritative aggregate until that following report.
        inner.set_keyboard_modifiers(command);
        press(&probe, Key::Control);
        press(&probe, Key::ControlR);
        release(&probe, Key::Control);
        assert!(probe.get_released_control());
        release(&probe, Key::ControlR);
        assert!(
            !probe.get_released_control(),
            "modifier callback retains immediate physical family"
        );
        key(&probe, "b");
        assert!(
            probe.get_pressed_control(),
            "ordinary input retains the last platform aggregate"
        );
        inner.set_keyboard_modifiers(KeyboardModifiers::default());
        key(&probe, "b");
        assert!(!probe.get_pressed_control());
        inner.set_keyboard_modifiers(command);
        press(&probe, Key::Control);
        press(&probe, Key::ControlR);
        inner.set_keyboard_modifiers(KeyboardModifiers::default());
        release(&probe, Key::Control);
        assert!(
            !probe.get_released_control(),
            "aggregate false clears both known sides before release"
        );
        release(&probe, Key::ControlR);
        assert!(!probe.get_released_control());

        let mut all = KeyboardModifiers::default();
        all.control = true;
        all.alt = true;
        all.shift = true;
        all.meta = true;
        inner.set_keyboard_modifiers(all);
        key(&probe, "b");
        assert!(
            probe.get_pressed_control()
                && probe.get_pressed_alt()
                && probe.get_pressed_shift()
                && probe.get_pressed_meta()
        );
        reset(&probe);
        key(&probe, "b");
        assert!(
            !probe.get_pressed_control()
                && !probe.get_pressed_alt()
                && !probe.get_pressed_shift()
                && !probe.get_pressed_meta(),
            "focus loss clears aggregate and physical state"
        );

        #[cfg(target_os = "windows")]
        {
            // Clearing aggregate Alt also clears a prior physical AltGr flag.
            // Otherwise the existing Ctrl+Alt text heuristic would be skipped.
            press(&probe, Key::AltGr);
            inner.set_keyboard_modifiers(KeyboardModifiers::default());
            let mut control_alt = KeyboardModifiers::default();
            control_alt.control = true;
            control_alt.alt = true;
            inner.set_keyboard_modifiers(control_alt);
            key(&probe, "@");
            assert!(!probe.get_pressed_control() && !probe.get_pressed_alt());
        }
        probe
            .window()
            .dispatch_event(WindowEvent::WindowActiveChanged(false));
        probe.hide().unwrap();
    }

    fn assert_audio_controls_keep_stop_visible(
        constraints: &Rc<RefCell<Option<slint::platform::LayoutConstraints>>>,
    ) {
        use slint::platform::{PointerEventButton, WindowEvent};
        let controls = AudioControlsWindow::new().unwrap();
        controls.set_ui_family(typography::resolve().family.into());
        controls.set_active(true);
        controls.set_elapsed("59:59:59 / 99:59:59".into());
        let stops = Rc::new(Cell::new(0));
        let calls = stops.clone();
        controls.on_stop(move || calls.set(calls.get() + 1));
        let mut previous_height = 0.0;
        for (index, size) in [13.0, 26.0, 40.0].into_iter().enumerate() {
            controls.set_ui_size(size);
            controls.set_source("Short source".into());
            controls.show().unwrap();
            let short = constraints.borrow().unwrap().min.unwrap();
            controls.hide().unwrap();
            controls.set_source(
                "A long playing source with a detailed session description "
                    .repeat(80)
                    .into(),
            );
            controls.show().unwrap();
            let minimum = constraints.borrow().unwrap().min.unwrap();
            eprintln!("Audio controls at {size}px: native minimum {minimum:?}");
            assert!(
                minimum.width >= 320.0,
                "native minimum must keep the audio controls usable"
            );
            assert_eq!(
                minimum, short,
                "source length must not force an unbounded window"
            );
            assert!(
                minimum.height > previous_height,
                "content minimum must grow with native text metrics"
            );
            previous_height = minimum.height;
            controls.window().set_size(minimum);
            slint::platform::update_timers_and_animations();
            // Click inside the bottom row of the actual generated window at its native minimum.
            let position = slint::LogicalPosition::new(minimum.width / 2.0, minimum.height - 24.0);
            controls
                .window()
                .dispatch_event(WindowEvent::PointerPressed {
                    position,
                    button: PointerEventButton::Left,
                });
            controls
                .window()
                .dispatch_event(WindowEvent::PointerReleased {
                    position,
                    button: PointerEventButton::Left,
                });
            assert_eq!(
                stops.get(),
                index + 1,
                "Stop must be clickable inside the visible client bounds"
            );
            controls.hide().unwrap();
        }
    }

    fn assert_monitor_handoff_preserves_numeric_entry(ui: &MainWindow) {
        use slint::platform::{Key, WindowEvent};
        fn key(ui: &MainWindow, text: impl Into<slint::SharedString>) {
            let text = text.into();
            ui.window()
                .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            ui.window()
                .dispatch_event(WindowEvent::KeyReleased { text });
            slint::platform::update_timers_and_animations();
        }
        let pending = Rc::new(RefCell::new(None::<String>));
        let commits = Rc::new(RefCell::new(Vec::<String>::new()));
        let restores = Rc::new(Cell::new(0));
        let p = pending.clone();
        ui.on_stage(move |id, setting, value| {
            if id == "focus-fixture" && setting == 0 {
                *p.borrow_mut() = Some(value.to_string());
            }
        });
        let c = commits.clone();
        ui.on_edit(move |id, setting, value| {
            if id == "focus-fixture" && setting == 0 {
                c.borrow_mut().push(value.to_string());
            }
        });
        let p = pending.clone();
        let r = restores.clone();
        ui.on_restore(move |id, setting| {
            assert_eq!(id, "focus-fixture");
            assert_eq!(setting, 0);
            *p.borrow_mut() = None;
            r.set(r.get() + 1);
            "440".into()
        });
        ui.set_tab_index(1);
        ui.set_can_edit(true);
        ui.set_ui_size(13.0);
        ui.window().set_size(slint::PhysicalSize::new(1100, 900));
        ui.set_rows(ModelRc::new(VecModel::from(vec![Row {
            id: "focus-fixture".into(),
            text0: "440".into(),
            text2: "0".into(),
            text3: "0".into(),
            text4: "0".into(),
            ..Row::default()
        }])));
        ui.set_edits_suspended(false);
        ui.window()
            .dispatch_event(WindowEvent::WindowActiveChanged(true));
        ui.invoke_focus_monitor_opener();
        // Navigate the real component tree, proving the target by its stage callback.
        for _ in 0..40 {
            key(ui, Key::Tab);
            for _ in 0..4 {
                key(ui, Key::RightArrow);
            }
            key(ui, "1");
            if pending.borrow().is_some() {
                break;
            }
            key(ui, Key::Escape);
        }
        assert_eq!(pending.borrow().as_deref(), Some("4401"));
        assert!(commits.borrow().is_empty());
        let monitor = MonitorWindow::new().unwrap();
        monitor.show().unwrap();
        hide_monitor_window(ui, &monitor, false);
        slint::platform::update_timers_and_animations();
        assert!(
            commits.borrow().is_empty(),
            "hiding an unfocused monitor must not commit a main-window field"
        );
        key(ui, Key::Escape);
        assert_eq!(restores.get(), 1);
        assert!(pending.borrow().is_none());
        for _ in 0..4 {
            key(ui, Key::RightArrow);
        }
        key(ui, "e");
        assert_eq!(pending.borrow().as_deref(), Some("440e"));
        monitor.show().unwrap();
        hide_monitor_window(ui, &monitor, false);
        slint::platform::update_timers_and_animations();
        assert!(
            commits.borrow().is_empty(),
            "incomplete numeric entry must survive responsive hiding"
        );
        key(ui, Key::Escape);
        assert_eq!(restores.get(), 2);
        assert!(pending.borrow().is_none());
        // Native deactivation suspends edit commits before Slint delivers focus loss.
        for _ in 0..4 {
            key(ui, Key::RightArrow);
        }
        key(ui, "1");
        ui.set_edits_suspended(true);
        ui.window()
            .dispatch_event(WindowEvent::WindowActiveChanged(false));
        monitor.show().unwrap();
        hide_monitor_window(ui, &monitor, true);
        ui.window()
            .dispatch_event(WindowEvent::WindowActiveChanged(true));
        ui.set_edits_suspended(false);
        slint::platform::update_timers_and_animations();
        assert!(commits.borrow().is_empty());
        let staged_before = pending.borrow().clone();
        key(ui, "2");
        assert_eq!(
            *pending.borrow(),
            staged_before,
            "opener handoff must leave the numeric field unfocused"
        );
    }
}
