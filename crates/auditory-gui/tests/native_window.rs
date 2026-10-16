// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Explicit nonshipping native-window check; ordinary tests do not open displays.
#[allow(
    unsafe_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::todo,
    reason = "The same Slint-generated-only UI as the executable, not handwritten code"
)]
mod generated_ui {
    slint::include_modules!();
}
use slint::ComponentHandle;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
#[derive(Default)]
struct Startup {
    snapshot_at: Option<Instant>,
    error: Option<String>,
}
fn snapshot_witness(window: &generated_ui::MainWindow) -> Result<(), Box<dyn std::error::Error>> {
    let size = window.window().size();
    if !window.window().is_visible() || size.width == 0 || size.height == 0 {
        return Err("native startup window is not shown with positive dimensions".into());
    }
    let native_parent = window.window().window_handle();
    let image = window.window().take_snapshot()?;
    if image.width() != size.width || image.height() != size.height {
        return Err("native startup snapshot dimensions differ from the shown window".into());
    }
    println!(
        "NATIVE_WINDOW_STARTED: visible native window and parent handle; scale={}",
        window.window().scale_factor()
    );
    println!(
        "NATIVE_WINDOW_SNAPSHOT_RENDERED: {}x{}",
        image.width(),
        image.height()
    );
    drop(native_parent);
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("AUDITORY_GUI_WINDOW_TEST").as_deref() != Some(std::ffi::OsStr::new("1")) {
        println!(
            "native_window: SKIPPED (set AUDITORY_GUI_WINDOW_TEST=1 with a native/declared virtual display)"
        );
        return Ok(());
    }
    let window = generated_ui::MainWindow::new()?;
    window.show()?;
    let startup = Rc::new(RefCell::new(Startup::default()));
    let observed = Rc::clone(&startup);
    let weak = window.as_weak();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_secs(1),
        move || {
            if let Some(window) = weak.upgrade() {
                let mut state = observed.borrow_mut();
                if let Some(rendered_at) = state.snapshot_at {
                    // The pinned collector starts its one-second reporting timer
                    // only during rendering. Keep two full seconds after the
                    // render witness so closing cannot race its first report.
                    if rendered_at.elapsed() >= Duration::from_secs(2) {
                        if let Err(error) = window.hide() {
                            state.error = Some(error.to_string());
                        }
                        let _ = slint::quit_event_loop();
                    }
                } else {
                    match snapshot_witness(&window) {
                        Ok(()) => {
                            state.snapshot_at = Some(Instant::now());
                            window.window().request_redraw();
                        }
                        Err(error) => {
                            state.error = Some(error.to_string());
                            let _ = slint::quit_event_loop();
                        }
                    }
                }
            }
        },
    );
    slint::run_event_loop()?;
    timer.stop();
    let state = startup.borrow();
    if let Some(error) = &state.error {
        return Err(error.clone().into());
    }
    if state.snapshot_at.is_none() {
        return Err("native startup did not render its snapshot witness".into());
    }
    Ok(())
}
