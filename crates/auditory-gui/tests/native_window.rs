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
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("AUDITORY_GUI_WINDOW_TEST").as_deref() != Some(std::ffi::OsStr::new("1")) {
        println!(
            "native_window: SKIPPED (set AUDITORY_GUI_WINDOW_TEST=1 with a native/declared virtual display)"
        );
        return Ok(());
    }
    let window = generated_ui::MainWindow::new()?;
    window.show()?;
    let shown = std::rc::Rc::new(std::cell::Cell::new(false));
    let observed = std::rc::Rc::clone(&shown);
    let weak = window.as_weak();
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(1),
        move || {
            if let Some(window) = weak.upgrade() {
                if !observed.replace(true) {
                    assert!(window.window().is_visible());
                    let native_parent = window.window().window_handle();
                    assert!(window.window().size().width > 0 && window.window().size().height > 0);
                    println!(
                        "NATIVE_WINDOW_STARTED: visible native window and parent handle; scale={}",
                        window.window().scale_factor()
                    );
                    drop(native_parent);
                } else {
                    let _ = window.hide();
                    let _ = slint::quit_event_loop();
                }
            }
        },
    );
    slint::run_event_loop()?;
    timer.stop();
    if !shown.get() {
        return Err("native startup did not observe a shown window".into());
    }
    Ok(())
}
