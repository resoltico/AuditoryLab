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
use generated::{AboutWindow, MainWindow};
use slint::{ComponentHandle, ModelRc, VecModel};
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
}
impl Desktop {
    fn refresh(&self) -> Duration {
        let Some(ui) = self.ui.upgrade() else {
            return Duration::from_millis(50);
        };
        self.updating.set(true);
        let mut done = false;
        if let Some(worker) = self.worker.borrow().as_ref() {
            while let Ok(message) = worker.messages.try_recv() {
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
                    Message::Imported(result) => {
                        match result {
                            Ok(preset) => {
                                app.imported(preset);
                                self.catalog_dirty.set(true);
                            }
                            Err(e) => app.status = e.to_string(),
                        }
                        done = true;
                    }
                    Message::Done(result) => {
                        app.status = match result {
                            Ok(text) => text,
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
        ui.set_sound_title(app::title(app.preset()).into());
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
        ui.set_can_export(app.preset().at(app.rate).is_ok());
        ui.set_status(app.status.as_str().into());
        let started = Instant::now();
        if let Ok(sound) = app.sound() {
            let signal = sound.signal();
            let position = self
                .worker
                .borrow()
                .as_ref()
                .map_or(0, |w| w.progress.frames());
            let elapsed = f64::from(
                u32::try_from(position / u64::from(signal.sample_rate().hz())).unwrap_or(u32::MAX),
            );
            ui.set_elapsed(format!("{elapsed:.0} / {:.0} s", signal.duration_seconds()).into());
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                reason = "bounded presentation ratio"
            )]
            let progress = (position as f64 / (signal.frames() as f64)) as f32;
            ui.set_progress(progress.clamp(0.0, 1.0));
            let ms =
                [2_u64, 10, 40, 100][usize::try_from(ui.get_window_index()).unwrap_or(2).min(3)];
            let frames = ms * u64::from(signal.sample_rate().hz()) / 1000;
            let start = app::scope_start(
                &sound,
                app.operation == Some(Operation::Play),
                position,
                frames,
            );
            let visible = if app.operation == Some(Operation::Play) {
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
        }
        self.updating.set(false);
        Duration::from_millis(50).max(started.elapsed() * 20)
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
    });
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
            d.app.borrow_mut().rate = rate;
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
        let preset = d.app.borrow().preset().clone();
        d.start(Operation::Play, Work::Play(preset));
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
        }
    });
    let d = desktop.clone();
    ui.on_export_wav(move || {
        if d.app.borrow().operation.is_some() {
            return;
        }
        let app = d.app.borrow();
        let name = match app.preset().origin() {
            auditorylab::Origin::BuiltIn(id) => format!("{id}.wav"),
            _ => "sound.wav".into(),
        };
        let sound = app.preset().at(app.rate);
        drop(app);
        if let Ok(sound) = sound
            && let Some(path) = rfd::FileDialog::new()
                .set_file_name(name)
                .add_filter("WAV audio", &["wav"])
                .save_file()
        {
            d.start(Operation::Export, Work::Export(sound, path));
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
