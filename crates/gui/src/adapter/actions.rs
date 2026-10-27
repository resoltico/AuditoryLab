//! Workspace commands and path-independent action callbacks.
use super::*;
use auditorylab_app::core::query::{Direction, GroupBy, RhythmState, Scope, SortBy};
use auditorylab_app::presentation::DisplayText;

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
    on!(on_select, |a, binding| {
        a.command(|w| w.select_library_key(&binding));
    });
    on!(on_collection_filter, |a, scope, sort| {
        a.command(|w| {
            {
                let mut query = w.catalog_query().clone();
                query.scope = choice(&Scope::ALL, scope).unwrap_or(Scope::All);
                query.sort = choice(&SortBy::ALL, sort).unwrap_or(SortBy::Automatic);
                if let Err(error) = w.set_catalog_query(query) {
                    return Err(auditorylab_app::workspace::Refusal::Document(Box::new(
                        error.into(),
                    )));
                }
            }
            Ok(())
        });
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
        let applied = a
            .command(|w| {
                let mut query = w.catalog_query().clone();
                let ranges = (|| -> Result<_, auditorylab_app::discovery::query::QueryError> {
                    Ok([
                        auditorylab_app::discovery::calendar::local_range(
                            auditorylab_app::discovery::calendar::CalendarField::Created,
                            &cf,
                            &ct,
                        )?,
                        auditorylab_app::discovery::calendar::local_range(
                            auditorylab_app::discovery::calendar::CalendarField::Modified,
                            &mf,
                            &mt,
                        )?,
                        auditorylab_app::discovery::calendar::local_range(
                            auditorylab_app::discovery::calendar::CalendarField::Added,
                            &af,
                            &at,
                        )?,
                    ])
                })();
                let [created, modified, added] = match ranges {
                    Ok(ranges) => ranges,
                    Err(error) => {
                        w.library_filter_error(error);
                        return false;
                    }
                };
                use auditorylab_app::core::{ConditionRole, Population};
                query.created = created;
                query.modified = modified;
                query.added = added;
                query.studies = if study.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![study.to_string()]
                };
                query.populations = choice(&Population::ALL, population - 1)
                    .into_iter()
                    .collect();
                query.roles = choice(&ConditionRole::ALL, role - 1).into_iter().collect();
                query.analysis = choice(
                    &auditorylab_app::core::rhythm::Completeness::ALL,
                    analysis - 1,
                );
                query.direction = choice(&Direction::ALL, direction - 1);
                query.group_by = choice(&GroupBy::ALL, group).unwrap_or(GroupBy::None);
                match w.set_catalog_query(query) {
                    Ok(()) => true,
                    Err(error) => {
                        w.library_filter_error(error);
                        false
                    }
                }
            })
            .unwrap_or(false);
        if applied && let Some(ui) = a.main.upgrade() {
            ui.invoke_close_library_filters();
        }
    });
    on!(on_clear_library_filters, |a| a.command(|w| {
        {
            let query = w.catalog_query().clone();
            let cleared = query.cleared_filters();
            if let Err(error) = w.set_catalog_query(cleared) {
                w.library_filter_error(error);
            }
        }
    }));
    on!(on_refresh_library, |a| a
        .command(Workspace::refresh_library));
    on!(on_reset_library_sort, |a| a.command(|w| {
        {
            let mut query = w.catalog_query().clone();
            query.sort = auditorylab_app::core::query::SortBy::Automatic;
            query.direction = None;
            if let Err(error) = w.set_catalog_query(query) {
                w.library_filter_error(error);
            }
        }
    }));
    on!(on_initialize_library, |a| a
        .command(Workspace::initialize_library));
    on!(on_edit_saved, |a, key, token| a
        .command(|w| w.edit_library_binding(&key, &token)));
    on!(on_save_copy, |a| a
        .command(Workspace::save_independent_copy));
    on!(on_add_to_library, |a, key| a
        .command(|w| w.add_library_binding(&key)));
    on!(on_show_saved, |a| {
        if a.command(Workspace::show_saved) == Some(true)
            && let Some(ui) = a.main.upgrade()
        {
            reset_filter_inputs(&ui);
        }
    });
    on!(on_library_action, |a, key, token, action, title| a.command(
        |w| w.mutate_library(&key, &token, &action, &title, false)
    ));
    on!(on_navigate_library, |a, direction| a
        .command(|w| w.navigate_library(direction)));
    on!(on_search, |a, query| a.command(|w| w.set_query(&query)));
    on!(on_rhythm_filter, |a, band, method| {
        use auditorylab_app::core::rhythm::{Band, Method};
        use auditorylab_app::discovery::query::RhythmQuery;
        let mut query = RhythmQuery::default();
        match band {
            0 => {}
            1..=5 => {
                if let Some(band) = choice(&Band::ALL, band - 1) {
                    query.bands.push(band);
                }
            }
            6 => query.state = Some(RhythmState::OutsideBands),
            7 => query.state = Some(RhythmState::None),
            8 => query.state = Some(RhythmState::Multiple),
            9 => query.state = Some(RhythmState::NeedsReview),
            _ => return,
        }
        if method > 0 {
            let Some(method) = choice(&Method::ALL, method - 1) else {
                return;
            };
            query.methods.push(method);
        }
        a.command(|w| w.set_rhythm_query(query));
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
    on!(on_preview_selected, |a| {
        if let Some(ui) = a.main.upgrade() {
            ui.set_monitor_open(true);
        }
        a.command(Workspace::preview_current);
    });
    on!(on_open_recipe, |a| a.command(Workspace::import));
    on!(on_save_recipe, |a| a.command(Workspace::save));
    on!(on_export_recipe, |a| a.command(Workspace::export_recipe));
    on!(on_close_open_file, |a, key| a
        .command(|w| w.close_open_file_binding(&key)));
    on!(on_export_wav, |a| a.command(Workspace::export));
    on!(on_cancel_file, |a| a.command(Workspace::cancel_export));
    on!(on_open_workbench, |a| a.command(Workspace::copy_selected));
    on!(on_new_draft, |a| a.command(Workspace::new_document));
    on!(on_based_on_source, |a| {
        let revealed = a.command(|w| {
            let found = w.editor().draft().based_on().is_some();
            w.show_based_on_source();
            found
        });
        if revealed == Some(true)
            && let Some(ui) = a.main.upgrade()
        {
            reset_filter_inputs(&ui);
        }
    });
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

// Only explicit reveal/reset commands clear pending native filter entry. Ordinary
// selection, worker and telemetry projections preserve unfinished date/study text.
fn reset_filter_inputs(ui: &MainWindow) {
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
