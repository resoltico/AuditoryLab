//! Workspace commands and path-independent action callbacks.
use super::*;

pub(super) fn install_actions(adapter: &Rc<SlintAdapter>, ui: &MainWindow) {
    macro_rules! on {
        ($setter:ident, |$a:ident $(, $arg:ident)*| $body:expr) => {{
            let weak: Weak<SlintAdapter> = Rc::downgrade(adapter);
            ui.$setter(move |$($arg),*| {
                if let Some($a) = weak.upgrade() {
                    $body;
                }
            });
        }};
    }
    on!(on_select, |a, index| {
        if let Ok(index) = usize::try_from(index) {
            a.command(|w| w.select(index));
        }
    });
    on!(on_search, |a, query| a.command(|w| w.set_query(&query)));
    on!(on_tab_changed, |a, index| {
        // The tab's change handler can run while a projection is writing
        // properties, when commands are ignored; apply the choice after it.
        let view = if index == 1 {
            View::Workbench
        } else {
            View::Library
        };
        let weak = Rc::downgrade(&a);
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(a) = weak.upgrade() {
                a.command(|w| w.set_view(view));
            }
        });
    });
    on!(on_rate_changed, |a, index| {
        if let Some(rate) = choice(&Workspace::export_rates(), index) {
            a.command(|w| w.set_export_rate(rate));
        }
    });
    on!(on_play_stop, |a| a.command(Workspace::play_stop));
    on!(on_play_recipe, |a| a.command(Workspace::play));
    on!(on_stop_audio, |a| a.command(Workspace::stop));
    on!(on_preview_selected, |a| a
        .command(Workspace::preview_current));
    on!(on_open_recipe, |a| a.command(Workspace::import));
    on!(on_save_recipe, |a| a.command(Workspace::save));
    on!(on_export_wav, |a| a.command(Workspace::export));
    on!(on_cancel_file, |a| a.command(Workspace::cancel_export));
    on!(on_open_workbench, |a| a.command(Workspace::copy_selected));
    on!(on_new_draft, |a| a.command(Workspace::new_document));
    on!(on_based_on_source, |a| a
        .command(Workspace::show_based_on_source));
    on!(on_quit, |a| a.command(Workspace::close));
    on!(on_about, |a| a.show_about());
    on!(on_open_study, |a| {
        let url = a
            .workspace
            .borrow_mut()
            .as_mut()
            .and_then(|w| w.library_view().study_url.clone());
        if let Some(url) = url {
            open_web(&url);
        }
    });
    on!(on_show_diagnostics, |a| {
        if let Some(ui) = a.main.upgrade() {
            a.show_diagnostics(&ui);
        }
    });
    on!(on_show_research, |a| {
        let library = a.workspace.borrow().as_ref().map(Workspace::library_view);
        if let (Some(window), Some(ui), Some(library)) =
            (a.details.upgrade(), a.main.upgrade(), library)
        {
            window.set_heading(format!("Sound details · {}", library.title).into());
            window.set_can_refresh(false);
            window.set_content(library.details.as_str().into());
            present_details(&window, &ui);
        }
    });
    on!(on_hold_changed, |a| {
        if let Some(ui) = a.main.upgrade() {
            let held = ui.get_held();
            a.command(|w| w.set_hold(held));
        }
    });
    on!(on_follow_motion, |a| a.command(Workspace::follow_motion));
    on!(on_window_changed, |a| {
        // Layout change handlers also run while drawing; native window
        // transitions wait until the current frame completes.
        let weak = Rc::downgrade(&a);
        a.deferred
            .start(slint::TimerMode::SingleShot, Duration::ZERO, move || {
                if let Some(a) = weak.upgrade() {
                    a.apply_waveform_choice();
                }
            });
    });
    let weak = Rc::downgrade(adapter);
    ui.window().on_close_requested(move || {
        if let Some(a) = weak.upgrade() {
            a.command(Workspace::close);
        }
        slint::CloseRequestResponse::KeepWindowShown
    });
}
