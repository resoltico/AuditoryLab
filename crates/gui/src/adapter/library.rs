//! Library model updates depend only on an immutable library projection.
use super::*;
use auditorylab::presentation::DisplayText;
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
                .map(|row| CatalogRow {
                    text: read_only(&row.text),
                    binding: row.key.as_str().into(),
                    title: read_only(&row.title),
                    collection: read_only(&row.collection),
                    summary: read_only(&row.summary),
                    bands: read_only(&row.bands),
                    duration: read_only(&row.duration),
                    dates: read_only(&row.dates),
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
            ui.set_library_overview(read_only(&w.library_overview()));
            let strings = |labels: Vec<String>| {
                ModelRc::new(VecModel::from(
                    labels
                        .into_iter()
                        .map(SharedString::from)
                        .collect::<Vec<_>>(),
                ))
            };
            let (bands, methods) = w.library_facets();
            ui.set_band_labels(strings(
                bands.iter().map(|band| band.text.clone()).collect(),
            ));
            ui.set_band_navigation(ModelRc::new(VecModel::from(
                bands
                    .into_iter()
                    .map(|band| crate::generated::LibraryFacet {
                        label: band.label.into(),
                        detail: band.detail.into(),
                        count: band.count.into(),
                        text: band.text.into(),
                    })
                    .collect::<Vec<_>>(),
            )));
            ui.set_method_labels(strings(methods));
            ui.set_calendar_context(auditorylab::calendar::local_context().into());
            {
                let query = w.catalog_query();
                use auditorylab::collection::{Scope, SortBy};
                use auditorylab::core::nominal::{Band, Method};
                use auditorylab::query::RhythmState;
                let band = query
                    .rhythm
                    .bands
                    .first()
                    .and_then(|band| Band::ALL.iter().position(|value| value == band))
                    .and_then(|index| i32::try_from(index + 1).ok())
                    .unwrap_or(match query.rhythm.state {
                        Some(RhythmState::OutsideBands) => 6,
                        Some(RhythmState::None) => 7,
                        Some(RhythmState::Multiple) => 8,
                        Some(RhythmState::NeedsReview) => 9,
                        None => 0,
                    });
                ui.set_band_choice(band);
                ui.set_method_choice(
                    query
                        .rhythm
                        .methods
                        .first()
                        .and_then(|method| Method::ALL.iter().position(|value| value == method))
                        .and_then(|index| i32::try_from(index + 1).ok())
                        .unwrap_or(0),
                );
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
        ui.set_has_results(library.rows.iter().any(|row| !row.key.is_empty()));
        ui.set_selected(
            library
                .selected
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
        ui.set_sound_title(library.title.display_scalar().into());
        ui.set_structure(read_only(&library.structure));
        ui.set_library_classification(read_only(&library.classification));
        ui.set_library_ownership(read_only(&library.ownership));
        ui.set_studied(read_only(&library.studied));
        ui.set_recipe_timing(read_only(&library.timing));
        ui.set_summary(read_only(&library.summary));
        ui.set_details(read_only(&library.details));
        ui.set_can_open_study(library.study_url.is_some());
        *self.library.borrow_mut() = Some(library);
    }
}
