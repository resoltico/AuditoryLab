//! Workspace commands and path-independent action callbacks.
use super::*;
use auditorylab_app::workspace::{
    AudioCommand, Command, EditorCommand, FileCommand, LibraryCommand, Navigation, OutputCommand,
};
use auditorylab_app::workspace::{FilterInput, QueryFacet};
use auditorylab_presentation::DisplayText;

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
    on!(on_select, |a, address| {
        if let Some(entry) = library::entry_handle(&address) {
            a.command(|w| w.apply(Command::Library(LibraryCommand::Select(entry))));
        }
    });
    on!(on_collection_filter, |a, scope, sort| {
        if let (Some(scope), Some(sort)) = (
            a.facet_key(QueryFacet::Scope, scope),
            a.facet_key(QueryFacet::Sort, sort),
        ) {
            a.command(|w| {
                w.apply(Command::Library(LibraryCommand::SetFacets(vec![
                    (QueryFacet::Scope, scope),
                    (QueryFacet::Sort, sort),
                ])))
            });
        }
    });
    on!(on_more_filters, |a,
                          study,
                          population,
                          role,
                          analysis,
                          direction,
                          group,
                          cf,
                          ct,
                          mf,
                          mt,
                          af,
                          at| {
        let facets = [
            (QueryFacet::Population, population),
            (QueryFacet::Role, role),
            (QueryFacet::Analysis, analysis),
            (QueryFacet::Direction, direction),
            (QueryFacet::Group, group),
        ]
        .into_iter()
        .map(|(facet, index)| a.facet_key(facet, index).map(|key| (facet, key)))
        .collect::<Option<Vec<_>>>();
        let Some(facets) = facets else {
            return;
        };
        let applied = a
            .command(|w| {
                w.apply(Command::Library(LibraryCommand::SetFilters(Box::new(
                    FilterInput {
                        study: study.into(),
                        facets,
                        dates: [
                            [cf.into(), ct.into()],
                            [mf.into(), mt.into()],
                            [af.into(), at.into()],
                        ],
                    },
                ))))
            })
            .is_some_and(|result| result.is_ok());
        if applied && let Some(ui) = a.main.upgrade() {
            ui.invoke_close_library_filters();
        }
    });
    on!(on_clear_library_filters, |a| a.command(
        |w| w.apply(Command::Library(LibraryCommand::ClearFilters))
    ));
    on!(on_refresh_library, |a| a.command(
        |w| w.apply(Command::Library(LibraryCommand::Refresh))
    ));
    on!(on_reset_library_sort, |a| a.command(
        |w| w.apply(Command::Library(LibraryCommand::ResetSort))
    ));
    on!(on_initialize_library, |a| a.command(
        |w| w.apply(Command::Library(LibraryCommand::Initialize))
    ));
    on!(on_edit_saved, |a, address| {
        if let Some(entry) = library::entry_handle(&address) {
            a.command(|w| w.apply(Command::Library(LibraryCommand::EditEntry(entry))));
        }
    });
    on!(on_save_copy, |a| a
        .command(|w| w.apply(Command::File(FileCommand::SaveCopy))));
    on!(on_add_to_library, |a, address| {
        if let Some(entry) = library::entry_handle(&address) {
            a.command(|w| w.apply(Command::Library(LibraryCommand::AddEntry(entry))));
        }
    });
    on!(on_show_saved, |a| a.command(
        |w| w.apply(Command::Library(LibraryCommand::ShowSaved))
    ));
    on!(on_library_action, |a, address, action, title| {
        use auditorylab_app::workspace::EntryMutation;
        let mutation = match action {
            crate::generated::LibraryMutationAction::Rename => EntryMutation::Rename {
                title: title.into(),
            },
            crate::generated::LibraryMutationAction::Trash => EntryMutation::Trash,
            crate::generated::LibraryMutationAction::Restore => EntryMutation::Restore,
            crate::generated::LibraryMutationAction::RequestPurge => EntryMutation::RequestPurge,
        };
        if let Some(entry) = library::entry_handle(&address) {
            a.command(|w| w.apply(Command::Library(LibraryCommand::Mutate { entry, mutation })));
        }
    });
    on!(on_navigate_library, |a, direction| a.command(|w| w.apply(
        Command::Library(LibraryCommand::Navigate(match direction {
            -2 => Navigation::First,
            -1 => Navigation::Previous,
            2 => Navigation::Last,
            _ => Navigation::Next,
        }))
    )));
    on!(on_search, |a, query| a.command(|w| w.apply(
        Command::Library(LibraryCommand::Search(query.to_string()))
    )));
    on!(on_rhythm_filter, |a, band, method| {
        if let (Some(band), Some(method)) = (
            a.facet_key(QueryFacet::Band, band),
            a.facet_key(QueryFacet::Method, method),
        ) {
            a.command(|w| {
                w.apply(Command::Library(LibraryCommand::SetFacets(vec![
                    (QueryFacet::Band, band),
                    (QueryFacet::Method, method),
                ])))
            });
        }
    });
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
                a.command(|w| w.apply(Command::SetView(view)));
            }
        });
    });
    on!(on_rate_changed, |a, index| {
        if let Some(rate) = choice(&Workspace::export_rates(), index) {
            a.command(|w| w.apply(Command::Editor(EditorCommand::SetExportRate(rate))));
        }
    });
    on!(on_play_stop, |a| a
        .command(|w| w.apply(Command::Audio(AudioCommand::Toggle))));
    on!(on_play_recipe, |a| a
        .command(|w| w.apply(Command::Audio(AudioCommand::Play))));
    on!(on_stop_audio, |a| a
        .command(|w| w.apply(Command::Audio(AudioCommand::Stop))));
    on!(on_preview_selected, |a| {
        if let Some(ui) = a.main.upgrade() {
            ui.set_monitor_open(true);
        }
        a.command(|w| w.apply(Command::Output(OutputCommand::Preview)));
    });
    on!(on_open_recipe, |a| a
        .command(|w| w.apply(Command::File(FileCommand::Import))));
    on!(on_save_recipe, |a| a
        .command(|w| w.apply(Command::File(FileCommand::Save))));
    on!(on_export_recipe, |a| a.command(
        |w| w.apply(Command::File(FileCommand::ExportRecipe))
    ));
    on!(on_close_open_file, |a, address| {
        if let Some(entry) = library::entry_handle(&address) {
            a.command(|w| w.apply(Command::Library(LibraryCommand::CloseEntry(entry))));
        }
    });
    on!(on_export_wav, |a| a
        .command(|w| w.apply(Command::File(FileCommand::ExportWav))));
    on!(on_cancel_file, |a| a.command(
        |w| w.apply(Command::File(FileCommand::CancelExport))
    ));
    on!(on_open_workbench, |a| a.command(
        |w| w.apply(Command::Editor(EditorCommand::CopySelected))
    ));
    on!(on_new_draft, |a| a
        .command(|w| w.apply(Command::Editor(EditorCommand::New))));
    on!(on_based_on_source, |a| {
        let revealed = a.command(|w| {
            let found = w.editor().draft().based_on().is_some();
            let _ = w.apply(Command::Editor(EditorCommand::ShowBasedOnSource));
            found
        });
        if revealed == Some(true)
            && let Some(ui) = a.main.upgrade()
        {
            reset_filter_inputs(&ui);
        }
    });
    on!(on_quit, |a| a.command(|w| w.apply(Command::Close)));
    on!(on_about, |a| a.show_about());
    on!(on_open_study, |a| {
        let url = a.workspace.borrow_mut().as_mut().and_then(|w| {
            auditorylab_presentation::workspace_text::library_view(&w.library_view())
                .study_url
                .clone()
        });
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
        let library = a
            .workspace
            .borrow()
            .as_ref()
            .map(|w| auditorylab_presentation::workspace_text::library_view(&w.library_view()));
        if let (Some(window), Some(ui), Some(library)) =
            (a.details.upgrade(), a.main.upgrade(), library)
        {
            window.set_heading(
                crate::terms::Caption::sound_details(&library.title.display_scalar()).into(),
            );
            window.set_can_refresh(false);
            window.set_content(read_only(&library.details));
            present_details(&window, &ui);
        }
    });
    on!(on_hold_changed, |a| {
        if let Some(ui) = a.main.upgrade() {
            let held = ui.get_held();
            a.command(|w| w.apply(Command::Output(OutputCommand::SetHold(held))));
        }
    });
    on!(on_follow_motion, |a| a.command(
        |w| w.apply(Command::Output(OutputCommand::FollowMotion))
    ));
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
            a.command(|w| w.apply(Command::Close));
        }
        slint::CloseRequestResponse::KeepWindowShown
    });
}

// Only explicit reveal/reset commands clear pending native filter entry. Ordinary
// selection, worker and telemetry projections preserve unfinished date/study text.
pub(super) fn reset_filter_inputs(ui: &MainWindow) {
    ui.set_study_query("".into());
    ui.set_population_choice(0);
    ui.set_role_choice(0);
    ui.set_analysis_choice(0);
    ui.set_direction_choice(0);
    ui.set_group_choice(0);
    ui.set_created_from("".into());
    ui.set_created_through("".into());
    ui.set_modified_from("".into());
    ui.set_modified_through("".into());
    ui.set_added_from("".into());
    ui.set_added_through("".into());
}
