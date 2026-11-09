//! A debug-only one-shot native surface lifetime witness, using the production window.
use crate::generated::MainWindow;
use slint::ComponentHandle;
use std::{
    cell::Cell,
    io::Write,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    FirstFrame,
    Scheduled,
    AwaitingRecreatedFrame,
    Done,
}
fn trace(path: &std::path::Path, event: &str) -> std::io::Result<()> {
    writeln!(
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?,
        "{event}"
    )
}
fn failed(path: &std::path::Path, error: impl std::fmt::Display) {
    let _ = trace(path, &format!("failed: {error}"));
    let _ = slint::quit_event_loop();
}

fn await_parent(
    main: slint::Weak<MainWindow>,
    path: PathBuf,
    phase: Rc<Cell<Phase>>,
    deadline: Instant,
) {
    slint::Timer::single_shot(Duration::from_millis(200), move || {
        match std::fs::read(path.with_file_name("surface-parent-confirmed")) {
            Ok(bytes) if bytes == b"native-parent-confirmed\n" => {}
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && Instant::now() < deadline =>
            {
                await_parent(main, path, phase, deadline);
                return;
            }
            Ok(_) => {
                failed(&path, "invalid parent-confirmation gate");
                return;
            }
            Err(error) => {
                failed(&path, error);
                return;
            }
        }
        let Some(main) = main.upgrade() else {
            failed(&path, "main window disappeared");
            return;
        };
        if let Err(error) = main.hide() {
            failed(&path, error);
            return;
        }
        if let Err(error) = trace(&path, "hidden") {
            failed(&path, error);
            return;
        }
        slint::Timer::single_shot(Duration::from_millis(200), move || {
            phase.set(Phase::AwaitingRecreatedFrame);
            if let Err(error) = main.show() {
                failed(&path, error);
            } else if let Err(error) = trace(&path, "shown") {
                failed(&path, error);
            }
        });
    });
}

pub(crate) fn install(
    main: &MainWindow,
    directory: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(directory)?;
    let path = directory.join("surface-lifetime.txt");
    // Refuse stale evidence rather than mistaking another run's trace for this run.
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let phase = Rc::new(Cell::new(Phase::FirstFrame));
    let weak = main.as_weak();
    main.window().set_rendering_notifier(move |state, api| {
        if !matches!(state, slint::RenderingState::AfterRendering) {
            return;
        }
        if !matches!(api, slint::GraphicsAPI::NativeOpenGL { .. }) {
            phase.set(Phase::Done);
            let path = path.clone();
            slint::Timer::single_shot(Duration::ZERO, move || {
                failed(&path, "native GL was not selected");
            });
            return;
        }
        match phase.get() {
            Phase::FirstFrame => {
                phase.set(Phase::Scheduled);
                let path = path.clone();
                let weak = weak.clone();
                let phase = phase.clone();
                // Defer all I/O and window changes until after rendering/presentation.
                slint::Timer::single_shot(Duration::ZERO, move || {
                    if let Err(error) = trace(&path, "first-frame-native-gl") {
                        failed(&path, error);
                        return;
                    }
                    // Only a confirmed real native portal parent permits this one-shot
                    // lifetime probe. Polling/deadline state is private to this QA run.
                    await_parent(weak, path, phase, Instant::now() + Duration::from_secs(30));
                });
            }
            Phase::AwaitingRecreatedFrame => {
                phase.set(Phase::Done);
                let path = path.clone();
                slint::Timer::single_shot(Duration::ZERO, move || {
                    if let Err(error) = trace(&path, "recreated-frame-native-gl") {
                        failed(&path, error);
                    }
                });
            }
            Phase::Scheduled | Phase::Done => {}
        }
    })?;
    Ok(())
}
