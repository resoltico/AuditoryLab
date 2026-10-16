// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Native desktop adapter over the backend-opaque auditory-app boundary.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod controller;
mod paths;
mod presentation;
mod signal;
mod signal_geometry;
mod worker;
#[allow(
    unsafe_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::todo,
    reason = "Slint generated vtables, internal exports and embedding hooks; no handwritten items in this module"
)]
mod generated_ui {
    slint::include_modules!();
}
use auditory_app::{BuildIdentity, Product, SampleRate};
use controller::Controller;
use generated_ui::{AboutWindow, MainWindow};
use slint::winit_030::WinitWindowAccessor;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};
fn main() -> std::process::ExitCode {
    match start() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Auditory Lab GUI: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
fn start() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args == [std::ffi::OsString::from("--build-info")] {
        println!("{}", BuildIdentity::for_product(Product::Gui).to_json());
        return Ok(());
    }
    if args.len() == 2 && args[0] == "--check-package" {
        return check_package(PathBuf::from(&args[1]));
    }
    if !args.is_empty() {
        return Err("Use auditorylab-gui without arguments, --build-info, or --check-package EMPTY_DIRECTORY.".into());
    }
    let ui = MainWindow::new()?;
    let about = AboutWindow::new()?;
    let identity = BuildIdentity::for_product(Product::Gui);
    let source = identity.source_revision().map_or_else(
        || "Local/unqualified: source is the builder's accompanying working tree; no immutable source revision is claimed.".to_owned(),
        |revision| format!("Exact build source: {}/tree/{revision}", env!("CARGO_PKG_REPOSITORY")),
    );
    about.set_identity(format!("{}\n\n{source}\n\nAuditory Lab source: MPL-2.0.\nSlint: selected Royalty-free Desktop, Mobile, and Web Applications License 2.0.\nFull third-party terms are in THIRD-PARTY-NOTICES.txt.", identity.summary()).into());
    ui.set_rate(SampleRate::DEFAULT.hz().to_string().into());
    let controller = Rc::new(RefCell::new(Controller::default()));
    let timer = Rc::new(slint::Timer::default());
    let bindings = Rc::new(Bindings {
        controller: Rc::clone(&controller),
        window: ui.as_weak(),
        about: about.as_weak(),
        timer: Rc::clone(&timer),
        rendered_signal: RefCell::new(None),
        detail_rendered: Cell::new(false),
        reported_occluded: Rc::new(Cell::new(false)),
    });
    bind(&ui, &bindings);
    ui.show()?;
    let reported_occluded = Rc::clone(&bindings.reported_occluded);
    ui.window().on_winit_window_event(move |_, event| {
        if let slint::winit_030::winit::event::WindowEvent::Occluded(occluded) = event {
            reported_occluded.set(*occluded);
        }
        slint::winit_030::EventResult::Propagate
    });
    let weak = Rc::downgrade(&bindings);
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(40),
        move || {
            if let Some(bindings) = weak.upgrade() {
                if let Some(ui) = bindings.window.upgrade() {
                    let eligible = ui.window().is_visible()
                        && !ui.window().is_minimized()
                        && !bindings.reported_occluded.get()
                        && ui
                            .window()
                            .with_winit_window(|window| window.has_focus())
                            .unwrap_or(false);
                    bindings.controller.borrow_mut().signal.poll(eligible);
                }
                let changed = bindings.controller.borrow_mut().poll();
                bindings.present_and_schedule(changed);
                if bindings
                    .controller
                    .borrow_mut()
                    .grant_start_after_ui_apply()
                {
                    bindings.present_and_schedule(true);
                }
            }
        },
    );
    bindings.update(|state, _| {
        state.start(worker::Job::Catalog);
    });
    let outcome = slint::run_event_loop();
    timer.stop();
    controller.borrow_mut().shutdown_after_event_loop();
    if let Err(error) = outcome {
        return Err(format!(
            "GUI event loop failed: {error}\nLast operation context: {}",
            controller.borrow().visible_activity()
        )
        .into());
    }
    Ok(())
}
fn apply(ui: &MainWindow, state: &Controller) {
    let mut labels: Vec<slint::SharedString> = state
        .catalog
        .iter()
        .map(|description| presentation::catalog_label(description).into())
        .collect();
    if let Some(imported) = &state.imported {
        labels.push(
            format!(
                "Imported: {}",
                presentation::catalog_label(imported.description())
            )
            .into(),
        );
    }
    if ui.get_catalog().iter().collect::<Vec<_>>() != labels {
        ui.set_catalog(ModelRc::new(VecModel::from(labels)));
    }
    ui.set_selected(i32::try_from(state.selected_index).unwrap_or(0));
    ui.set_source_title(
        state
            .selection
            .as_ref()
            .map_or_else(
                || "Loading catalog".to_owned(),
                |selection| presentation::source_title(&selection.description),
            )
            .into(),
    );
    ui.set_description(
        state
            .selection
            .as_ref()
            .map_or_else(
                || "Loading catalog; no audio starts automatically.".into(),
                |selection| presentation::description(&selection.description),
            )
            .into(),
    );
    ui.set_status(
        format!(
            "{}{}",
            state.state_label(),
            if state.dialog {
                " — native dialog open"
            } else {
                ""
            }
        )
        .into(),
    );
    ui.set_activity(state.visible_activity().into());
    ui.set_busy(state.busy());
    ui.set_stoppable(state.stoppable());
    ui.set_selection_locked(state.selection_locked());
    ui.set_monitoring(state.signal.available());
    ui.set_frozen(state.signal.frozen);
    ui.set_input_path(state.input_path.display().into());
    ui.set_input_editing(state.input_path.editing());
    ui.set_folder_path(state.folder.display().into());
    ui.set_folder_editing(state.folder.editing());
}
fn rate(text: &str) -> Result<SampleRate, String> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Enter a whole decimal sample rate in hertz.".into());
    }
    let hz = text
        .parse::<u32>()
        .map_err(|_| "Enter a whole decimal sample rate in hertz.".to_owned())?;
    SampleRate::new(hz).map_err(|error| error.to_string())
}
struct Bindings {
    controller: Rc<RefCell<Controller>>,
    window: slint::Weak<MainWindow>,
    about: slint::Weak<AboutWindow>,
    timer: Rc<slint::Timer>,
    rendered_signal: RefCell<Option<Arc<auditory_app::SignalSnapshot>>>,
    detail_rendered: Cell<bool>,
    reported_occluded: Rc<Cell<bool>>,
}
impl Bindings {
    fn update(&self, action: impl FnOnce(&mut Controller, &MainWindow)) {
        let Some(ui) = self.window.upgrade() else {
            return;
        };
        action(&mut self.controller.borrow_mut(), &ui);
        self.present_and_schedule(true);
    }
    fn present_and_schedule(&self, changed: bool) {
        let Some(ui) = self.window.upgrade() else {
            return;
        };
        let mut state = self.controller.borrow_mut();
        if changed {
            apply(&ui, &state);
        }
        self.present_signal(&ui, &mut state.signal);
        if state.close_ready() {
            let _ = ui.hide();
            if let Some(about) = self.about.upgrade() {
                let _ = about.hide();
            }
            self.timer.stop();
            let _ = slint::quit_event_loop();
        } else if state.polling_needed() {
            if !self.timer.running() {
                self.timer.restart();
            }
        } else {
            // No idle polling, model recreation or periodic repaint. Native
            // dialog futures wake this binding when their actual result arrives.
            self.timer.stop();
        }
    }
    fn present_signal(&self, ui: &MainWindow, signal: &mut signal::SignalDisplay) {
        let mut rendered = self.rendered_signal.borrow_mut();
        let changed = signal.snapshot.as_ref().is_some_and(|snapshot| {
            rendered
                .as_ref()
                .is_none_or(|previous| !Arc::ptr_eq(previous, snapshot))
        });
        let rebuild = changed || (ui.get_detail_visible() && !self.detail_rendered.get());
        let frame = if rebuild {
            signal
                .snapshot
                .as_ref()
                .map(|snapshot| SignalFrame::new(snapshot, ui.get_detail_visible()))
                .transpose()
        } else {
            Ok(None)
        };
        // Build all fallible strings before any setter: no mixed-snapshot paths,
        // endpoint labels or captions can become visible on a capacity failure.
        let context = presentation::signal_context(signal);
        match (frame, context) {
            (Ok(frame), Ok(context)) => {
                if let Some(frame) = frame {
                    frame.apply(ui);
                    *rendered = signal.snapshot.as_ref().map(Arc::clone);
                    self.detail_rendered.set(ui.get_detail_visible());
                } else if signal.snapshot.is_none() && rendered.is_some() {
                    clear_signal_drawing(ui);
                    *rendered = None;
                    self.detail_rendered.set(false);
                } else if !ui.get_detail_visible() && self.detail_rendered.replace(false) {
                    ui.set_detail_left("".into());
                    ui.set_detail_right("".into());
                    ui.set_detail_start("".into());
                    ui.set_detail_end("".into());
                    ui.set_detail_context("".into());
                }
                ui.set_signal_context(context.into());
            }
            _ => {
                signal.rendering_failed();
                clear_signal_drawing(ui);
                *rendered = None;
                self.detail_rendered.set(false);
                ui.set_signal_context(signal_geometry::CAPACITY_NOTICE.into());
            }
        }
    }
}
struct SignalFrame {
    peak: [String; 2],
    detail: [String; 2],
    peak_endpoints: [String; 2],
    detail_endpoints: [String; 2],
    peak_caption: String,
    detail_caption: String,
    through: String,
}
impl SignalFrame {
    fn new(
        snapshot: &auditory_app::SignalSnapshot,
        detail_visible: bool,
    ) -> Result<Self, signal_geometry::CapacityError> {
        let rate = snapshot.sample_rate();
        Ok(Self {
            peak: signal_geometry::paths(snapshot.peak_range())?,
            detail: if detail_visible {
                signal_geometry::paths(snapshot.detail())?
            } else {
                [String::new(), String::new()]
            },
            peak_endpoints: presentation::signal_endpoints(snapshot.peak_range(), rate)?,
            detail_endpoints: if detail_visible {
                presentation::signal_endpoints(snapshot.detail(), rate)?
            } else {
                [String::new(), String::new()]
            },
            peak_caption: presentation::signal_range(snapshot.peak_range(), rate)?,
            detail_caption: if detail_visible {
                presentation::signal_range(snapshot.detail(), rate)?
            } else {
                String::new()
            },
            through: snapshot.progress().observed_through().map_or_else(
                || Ok("No observed samples".into()),
                |frame| presentation::signal_position(frame, rate),
            )?,
        })
    }
    fn apply(self, ui: &MainWindow) {
        let [left, right] = self.peak;
        ui.set_peak_left(left.into());
        ui.set_peak_right(right.into());
        let [left, right] = self.detail;
        ui.set_detail_left(left.into());
        ui.set_detail_right(right.into());
        let [start, end] = self.peak_endpoints;
        ui.set_peak_start(start.into());
        ui.set_peak_end(end.into());
        let [start, end] = self.detail_endpoints;
        ui.set_detail_start(start.into());
        ui.set_detail_end(end.into());
        ui.set_peak_context(self.peak_caption.into());
        ui.set_detail_context(self.detail_caption.into());
        ui.set_signal_through(self.through.into());
    }
}
fn clear_signal_drawing(ui: &MainWindow) {
    ui.set_peak_left("".into());
    ui.set_peak_right("".into());
    ui.set_peak_start("".into());
    ui.set_peak_end("".into());
    ui.set_detail_left("".into());
    ui.set_detail_right("".into());
    ui.set_detail_start("".into());
    ui.set_detail_end("".into());
    ui.set_peak_context("".into());
    ui.set_detail_context("".into());
    ui.set_signal_through("No observed samples".into());
}

fn bind(ui: &MainWindow, bindings: &Rc<Bindings>) {
    let b = Rc::clone(bindings);
    ui.on_detail_visibility_changed(move || b.update(|_, _| {}));
    let b = Rc::clone(bindings);
    ui.on_freeze_display(move || b.update(|state, _| state.signal.toggle_freeze()));
    let b = Rc::clone(bindings);
    ui.on_refresh_display(move || b.update(|state, _| state.signal.refresh()));
    let b = Rc::clone(bindings);
    ui.on_motion_changed(move |reduced| {
        b.update(|state, _| state.signal.set_reduced_motion(reduced))
    });
    let b = Rc::clone(bindings);
    ui.on_detail_window(move |index| b.update(|state, _| state.signal.detail_window(index)));
    let b = Rc::clone(bindings);
    ui.on_select(move |index| {
        b.update(|state, _| {
            if let Ok(index) = usize::try_from(index) {
                state.select(index);
            }
        })
    });
    let b = Rc::clone(bindings);
    ui.on_edit_input(move || b.update(|state, _| state.input_path.begin_edit()));
    let b = Rc::clone(bindings);
    ui.on_input_changed(move |text| b.update(|state, _| state.input_path.edit(text.to_string())));
    let b = Rc::clone(bindings);
    ui.on_cancel_input_edit(move || b.update(|state, _| state.input_path.cancel_edit()));
    let b = Rc::clone(bindings);
    ui.on_edit_folder(move || b.update(|state, _| state.folder.begin_edit()));
    let b = Rc::clone(bindings);
    ui.on_folder_changed(move |text| b.update(|state, _| state.folder.edit(text.to_string())));
    let b = Rc::clone(bindings);
    ui.on_cancel_folder_edit(move || b.update(|state, _| state.folder.cancel_edit()));
    let b = Rc::clone(bindings);
    ui.on_import_recipe(move || {
        b.update(|state, _| match state.input_path.path() {
            Ok(path) => {
                state.start(worker::Job::Import(path));
            }
            Err(error) => state.fail(error),
        })
    });
    let b = Rc::clone(bindings);
    ui.on_inspect(move || {
        b.update(|state, ui| match rate(&ui.get_rate()) {
            Ok(rate) => {
                if let Some(selection) = state.selection.clone() {
                    state.start(worker::Job::Inspect(selection.input, rate));
                }
            }
            Err(error) => state.fail(error),
        })
    });
    let b = Rc::clone(bindings);
    ui.on_play(move || {
        b.update(|state, ui| {
            if state.busy() { return; }
            if let Some(selection) = state.selection.clone() {
                if signal_geometry::validate_source(&selection.description).is_err() {
                    state.notify_presentation_error("Signal source context exceeds the display reserve. Use a shorter source path or CLI playback.".into());
                    return;
                }
                b.rendered_signal.borrow_mut().take();
                b.detail_rendered.set(false);
                clear_signal_drawing(ui);
                state.start(worker::Job::Play(selection.input));
            }
        })
    });
    let b = Rc::clone(bindings);
    ui.on_stop(move || b.update(|state, _| state.stop()));
    let b = Rc::clone(bindings);
    ui.on_export_wav(move || {
        b.update(|state, ui| {
            let request = rate(&ui.get_rate()).and_then(|rate| {
                paths::export_destination(&state.folder, &ui.get_filename())
                    .map(|path| (rate, path))
            });
            match request {
                Ok((rate, path)) => {
                    if let Some(selection) = state.selection.clone() {
                        state.start(worker::Job::Export(selection.input, rate, path));
                    }
                }
                Err(error) => state.fail(error),
            }
        })
    });
    let b = Rc::clone(bindings);
    ui.on_about(move || {
        if let Some(about) = b.about.upgrade()
            && let Err(error) = about.show()
        {
            b.update(|state, _| {
                state.notify_presentation_error(format!("Cannot open About: {error}"))
            });
        }
    });
    let b = Rc::clone(bindings);
    ui.window().on_close_requested(move || {
        b.update(|state, _| {
            state.request_close();
        });
        // The binding hides only after result+worker join and dialog settlement.
        slint::CloseRequestResponse::KeepWindowShown
    });
    let b = Rc::clone(bindings);
    ui.on_browse_input(move || dialog(&b, false));
    let b = Rc::clone(bindings);
    ui.on_browse_folder(move || dialog(&b, true));
}
fn dialog(bindings: &Rc<Bindings>, folder: bool) {
    if bindings.controller.borrow().busy() {
        return;
    }
    let Some(ui) = bindings.window.upgrade() else {
        return;
    };
    let parent = ui.window().window_handle();
    bindings.update(|state, _| state.dialog = true);
    let dialog = rfd::AsyncFileDialog::new().set_parent(&parent);
    let dialog = if folder {
        dialog
    } else {
        dialog
            .add_filter("TOML recipe", &["toml"])
            .add_filter("All files", &["*"])
    };
    let complete = Rc::clone(bindings);
    if let Err(error) = slint::spawn_local(async move {
        let chosen = if folder {
            dialog.pick_folder().await
        } else {
            dialog.pick_file().await
        };
        complete.update(|state, _| {
            state.dialog_finished(chosen.map(|file| file.path().to_owned()), folder)
        });
        drop(parent);
    }) {
        bindings.update(|state,_|{state.dialog=false;state.fail(format!("Cannot start parented native chooser: {error}. Manual path entry remains available."));});
    }
}
fn check_package(directory: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    if !directory.is_dir() || std::fs::read_dir(&directory)?.next().is_some() {
        return Err("--check-package requires an existing empty owned directory.".into());
    }
    let descriptions = auditory_app::catalog()?;
    if descriptions.is_empty() {
        return Err("Catalog is unexpectedly empty.".into());
    }
    let preset = auditory_app::load_bytes(
        b"schema_version=2\nduration_seconds=0.000020833333333333333\nfade_seconds=0",
        auditory_app::Origin::Reader("package check".into()),
    )?;
    let facts = preset.inspect(SampleRate::DEFAULT)?;
    let path = directory.join("silence.wav");
    let report = preset
        .prepare_export(SampleRate::DEFAULT, &path)?
        .execute(&auditory_app::Cancellation::default())?;
    if report.exported().frames != facts.frames() || facts.frames() != 1 {
        return Err("Package check frame report mismatch.".into());
    }
    println!(
        "{{\"frames\":{},\"sample_rate\":{},\"file\":\"silence.wav\"}}",
        report.exported().frames,
        report.exported().sample_rate.hz()
    );
    Ok(())
}
