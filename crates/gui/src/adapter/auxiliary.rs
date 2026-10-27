//! Monitor and auxiliary window callbacks.
use super::*;
use auditorylab_app::workspace::{AudioCommand, Command, OutputCommand};

pub(super) fn install_monitor(adapter: &Rc<SlintAdapter>, windows: &Windows) {
    let monitor = &windows.monitor;
    let weak = Rc::downgrade(adapter);
    monitor.on_span(move |index| {
        if let Some(a) = weak.upgrade() {
            if let Some(ui) = a.main.upgrade() {
                ui.set_window_index(index);
            }
            a.apply_waveform_choice();
        }
    });
    let weak = Rc::downgrade(adapter);
    monitor.on_scale(move |index| {
        if let Some(a) = weak.upgrade() {
            if let Some(ui) = a.main.upgrade() {
                ui.set_scale_index(index);
            }
            a.apply_waveform_choice();
        }
    });
    let weak = Rc::downgrade(adapter);
    monitor.on_hold(move || {
        if let Some(a) = weak.upgrade()
            && let Some(ui) = a.main.upgrade()
        {
            let held = !ui.get_held();
            a.command(|w| w.apply(Command::Output(OutputCommand::SetHold(held))));
        }
    });
    let weak = Rc::downgrade(adapter);
    monitor.on_follow(move || {
        if let Some(a) = weak.upgrade() {
            a.command(|w| w.apply(Command::Output(OutputCommand::FollowMotion)));
        }
    });
    let weak = Rc::downgrade(adapter);
    monitor.on_details(move || {
        if let Some(a) = weak.upgrade()
            && let Some(monitor) = a.monitor.upgrade()
        {
            a.show_diagnostics(&monitor);
        }
    });
    let weak = Rc::downgrade(adapter);
    monitor.on_dismiss(move || {
        if let Some(a) = weak.upgrade() {
            if let Some(ui) = a.main.upgrade() {
                ui.set_monitor_open(false);
                ui.invoke_focus_monitor_opener();
            }
            a.project();
        }
    });
    let weak = monitor.as_weak();
    monitor.window().on_close_requested(move || {
        if let Some(window) = weak.upgrade() {
            window.invoke_dismiss();
        }
        slint::CloseRequestResponse::KeepWindowShown
    });
}

pub(super) fn install_auxiliary(adapter: &Rc<SlintAdapter>, windows: &Windows) {
    let weak = Rc::downgrade(adapter);
    windows.audio_controls.on_stop(move || {
        if let Some(a) = weak.upgrade() {
            a.command(|w| w.apply(Command::Audio(AudioCommand::Stop)));
        }
    });
    windows
        .audio_controls
        .window()
        .on_close_requested(|| slint::CloseRequestResponse::KeepWindowShown);
    let about = windows.about.as_weak();
    windows.about.on_dismiss(move || {
        if let Some(about) = about.upgrade() {
            let _ = about.hide();
        }
    });
    windows
        .about
        .on_open_source(|| open_web(auditorylab_app::identity::SOURCE_URL));
    let licenses = windows.licenses.as_weak();
    windows.about.on_open_licenses(move || {
        if let Some(window) = licenses.upgrade() {
            match window.show() {
                Ok(()) => window.invoke_focus_start(),
                Err(error) => eprintln!(
                    "{}",
                    crate::native_text::InterfaceOperation::Licenses.failure(error)
                ),
            }
        }
    });
    let licenses = windows.licenses.as_weak();
    windows.licenses.on_dismiss(move || {
        if let Some(window) = licenses.upgrade() {
            let _ = window.hide();
        }
    });
    let licenses = windows.licenses.as_weak();
    windows.licenses.on_third_party_licenses_requested(move || {
        if let Some(window) = licenses.upgrade() {
            match crate::licenses::notices_directory() {
                Ok(path) => match path.to_str() {
                    Some(path) => {
                        window.set_notice(crate::terms::Caption::BundledNoticesHelp.label().into());
                        open_web(path);
                    }
                    None => {
                        window.set_notice(crate::terms::Caption::NoticesPathError.label().into())
                    }
                },
                Err(error) => window.set_notice(error.into()),
            }
        }
    });
    let details = windows.details.as_weak();
    windows.details.on_dismiss(move || {
        if let Some(window) = details.upgrade() {
            let _ = window.hide();
        }
    });
    let weak = Rc::downgrade(adapter);
    windows.details.on_refresh_content(move || {
        if let Some(a) = weak.upgrade() {
            a.refresh_diagnostics();
        }
    });
}

// ---- window helpers ----
