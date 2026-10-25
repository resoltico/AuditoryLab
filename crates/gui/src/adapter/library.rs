//! Library model updates depend only on an immutable library projection.
use super::*;
impl SlintAdapter {
    pub(super) fn project_library(
        &self,
        ui: &MainWindow,
        library: Arc<auditorylab::workspace::LibraryView>,
    ) {
        if self
            .library
            .borrow()
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &library))
        {
            return;
        }
        #[cfg(test)]
        self.counts
            .library_projections
            .set(self.counts.library_projections.get() + 1);
        sync_rows(
            &self.catalog,
            library
                .rows
                .iter()
                .zip(&library.keys)
                .map(|(label, key)| CatalogRow {
                    text: read_only(label),
                    binding: key.as_str().into(),
                })
                .collect(),
        );
        if let Some(w) = self.workspace.borrow().as_ref() {
            let (key, token, trashed) = w.library_binding();
            ui.set_library_key(key.into());
            ui.set_library_token(token.into());
            ui.set_library_trashed(trashed);
            ui.set_library_status(read_only(&w.library_status()));
            ui.set_library_alert(read_only(&w.library_alert()));
            ui.set_library_dates(read_only(&w.library_dates()));
            ui.set_library_dates_utc(read_only(&w.library_dates_utc()));
            ui.set_library_count(read_only(&w.library_count()));
            let strings = |labels: Vec<String>| {
                ModelRc::new(VecModel::from(
                    labels
                        .into_iter()
                        .map(SharedString::from)
                        .collect::<Vec<_>>(),
                ))
            };
            let (bands, methods) = w.library_facet_labels();
            ui.set_band_labels(strings(bands));
            ui.set_method_labels(strings(methods));
            ui.set_calendar_context(auditorylab::calendar::local_context().into());
            if let Some(query) = w.catalog_query() {
                use auditorylab::collection::{Scope, SortBy};
                ui.set_collection_choice(match query.scope {
                    Scope::All => 0,
                    Scope::BuiltIn => 1,
                    Scope::User => 2,
                    Scope::OpenFiles => 3,
                    Scope::Trash => 4,
                });
                ui.set_sort_choice(match query.sort {
                    SortBy::Automatic => 0,
                    SortBy::Rate => 1,
                    SortBy::Title => 2,
                    SortBy::Created => 3,
                    SortBy::Modified => 4,
                    SortBy::Added => 5,
                    SortBy::Study => 6,
                    SortBy::Collection => 7,
                });
            }
        }
        // Virtual group/diagnostic rows do not create a selectable sound.
        ui.set_has_results(library.keys.iter().any(|key| !key.is_empty()));
        ui.set_selected(
            library
                .selected
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
        ui.set_sound_title(auditorylab::presentation::display_scalar(&library.title).into());
        ui.set_structure(read_only(&library.structure));
        ui.set_studied(read_only(&library.studied));
        ui.set_recipe_timing(read_only(&library.timing));
        ui.set_summary(read_only(&library.summary));
        ui.set_details(read_only(&library.details));
        ui.set_can_open_study(library.study_url.is_some());
        *self.library.borrow_mut() = Some(library);
    }
}
