//! Desktop interface to Auditory Lab.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod adapter;
#[cfg(test)]
mod adapter_tests;
mod details_parent;
mod dialogs;
#[cfg(test)]
mod focus_reflow_tests;
#[cfg(test)]
mod focus_scroll_tests;
mod licenses;
mod motion;
#[cfg(target_os = "macos")]
mod native_application;
mod scope;
#[cfg(test)]
mod test_platform;
#[cfg(test)]
mod text_edit_tests;
mod typography;
#[cfg(test)]
mod window_tests;
#[expect(
    unreachable_pub,
    clippy::unwrap_used,
    clippy::todo,
    reason = "Slint 1.18.1 generates unreachable public UI items, unwraps and TODO branches; removing these exceptions produces compiler/Clippy errors in generated output. This module contains only include_modules"
)]
mod generated {
    slint::include_modules!();
}
use adapter::{Preferences, SlintAdapter, Windows};
use auditorylab::workspace::{Capabilities, Workspace};
use generated::{DetailsWindow, MainWindow};

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
    let selector = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("femtovg".into());
    #[cfg(target_os = "macos")]
    let (selector, handler_slot) = {
        let (handler, slot) = native_application::ApplicationHandler::new();
        (
            selector.with_winit_custom_application_handler(handler),
            slot,
        )
    };
    selector.select()?;
    let windows = Windows::new()?;
    let workspace = Workspace::new(auditorylab::catalog()?, Capabilities::available())?;
    let adapter = SlintAdapter::install(
        &windows,
        workspace,
        Box::new(dialogs::NativeShell),
        Some(Preferences::start()),
    );
    #[cfg(target_os = "macos")]
    {
        *handler_slot.borrow_mut() = std::rc::Rc::downgrade(&adapter);
    }
    adapter.show_main()?;
    let result = slint::run_event_loop_until_quit();
    // Leaving the loop by any route still ends every task and reports failures.
    let shutdown = adapter.shutdown();
    if let Err(error) = &shutdown {
        eprintln!("Shutdown: {error}");
    }
    result?;
    shutdown.map_err(Into::into)
}
