//! Library model updates depend only on an immutable library projection.
use super::*;
use auditorylab_app::workspace::{ChoiceView, FacetCounts, QueryFacet};
use auditorylab_presentation::DisplayText;
use auditorylab_presentation::LibraryPresentation;
struct EntryHandles(Vec<Option<auditorylab_app::workspace::EntryHandle>>);
impl slint::Model for EntryHandles {
    type Data = i32;
    fn row_count(&self) -> usize {
        self.0.len()
    }
    fn row_data(&self, row: usize) -> Option<i32> {
        (row < self.0.len())
            .then(|| i32::try_from(row).ok())
            .flatten()
    }
    fn model_tracker(&self) -> &dyn slint::ModelTracker {
        &()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
pub(super) fn entry_handle(
    address: &crate::generated::EntryAddress,
) -> Option<auditorylab_app::workspace::EntryHandle> {
    address
        .table
        .as_any()
        .downcast_ref::<EntryHandles>()?
        .0
        .get(usize::try_from(address.row).ok()?)?
        .clone()
}

pub(super) fn select_choice(ui: &MainWindow, facet: QueryFacet, view: &ChoiceView) {
    let Some(selected) = view
        .options
        .iter()
        .position(|option| option.selected)
        .and_then(|index| i32::try_from(index).ok())
    else {
        return;
    };
    match facet {
        QueryFacet::Scope => ui.set_collection_choice(selected),
        QueryFacet::Sort => ui.set_sort_choice(selected),
        QueryFacet::Direction => ui.set_direction_choice(selected),
        QueryFacet::Group => ui.set_group_choice(selected),
        QueryFacet::Band => ui.set_band_choice(selected),
        QueryFacet::Method => ui.set_method_choice(selected),
        QueryFacet::Population => ui.set_population_choice(selected),
        QueryFacet::Role => ui.set_role_choice(selected),
        QueryFacet::Analysis => ui.set_analysis_choice(selected),
    }
}
impl SlintAdapter {
    /// Both navigation and combo rows address the same actual rendered options.
    pub(super) fn project_choice_models(
        &self,
        ui: &MainWindow,
        counts: &FacetCounts,
        choices: Vec<(QueryFacet, ChoiceView)>,
    ) {
        let strings = |labels: Vec<String>| {
            ModelRc::new(VecModel::from(
                labels
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            ))
        };
        let bands = counts
            .bands
            .iter()
            .map(|count| {
                (
                    count.kind.key(),
                    auditorylab_presentation::library_text::facet(count),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let methods = counts
            .methods
            .iter()
            .map(|count| {
                (
                    count.kind.key(),
                    auditorylab_presentation::library_text::facet(count).text,
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        for (facet, view) in &choices {
            let labels = strings(
                view.options
                    .iter()
                    .map(|option| option.label.to_string())
                    .collect(),
            );
            let app_selection_changed = self
                .choices
                .borrow()
                .iter()
                .find(|(kind, _)| kind == facet)
                .is_none_or(|(_, old)| old != view);
            match facet {
                QueryFacet::Scope => ui.set_collection_labels(labels),
                QueryFacet::Sort => ui.set_sort_labels(labels),
                QueryFacet::Direction => ui.set_direction_labels(labels),
                QueryFacet::Group => ui.set_group_labels(labels),
                QueryFacet::Population => ui.set_population_labels(labels),
                QueryFacet::Role => ui.set_role_labels(labels),
                QueryFacet::Analysis => ui.set_analysis_labels(labels),
                QueryFacet::Method => ui.set_method_labels(strings(
                    view.options
                        .iter()
                        .map(|option| {
                            methods
                                .get(option.key)
                                .cloned()
                                .unwrap_or_else(|| option.label.into())
                        })
                        .collect(),
                )),
                QueryFacet::Band => {
                    let navigation = view
                        .options
                        .iter()
                        .enumerate()
                        .map(|(index, option)| {
                            let text = bands.get(option.key);
                            crate::generated::LibraryFacet {
                                choice_index: i32::try_from(index).unwrap_or(-1),
                                selected: option.selected,
                                label: text.map_or(option.label, |text| text.label.as_str()).into(),
                                detail: text.map_or("", |text| text.detail.as_str()).into(),
                                count: text
                                    .map_or(
                                        auditorylab_app::core::vocabulary::Status::Unknown.label(),
                                        |text| text.count.as_str(),
                                    )
                                    .into(),
                                text: text.map_or(option.label, |text| text.text.as_str()).into(),
                            }
                        })
                        .collect::<Vec<_>>();
                    ui.set_band_labels(strings(
                        navigation.iter().map(|row| row.text.to_string()).collect(),
                    ));
                    ui.set_band_navigation(ModelRc::new(VecModel::from(navigation)));
                }
            }
            if app_selection_changed
                || matches!(
                    facet,
                    QueryFacet::Scope | QueryFacet::Sort | QueryFacet::Band | QueryFacet::Method
                )
            {
                select_choice(ui, *facet, view);
            }
        }
        *self.choices.borrow_mut() = choices;
    }
    pub(super) fn facet_key(&self, facet: QueryFacet, index: i32) -> Option<String> {
        self.choices
            .borrow()
            .iter()
            .find(|(kind, _)| *kind == facet)?
            .1
            .options
            .get(usize::try_from(index).ok()?)
            .map(|option| option.key.to_string())
    }

    pub(super) fn project_library(
        &self,
        ui: &MainWindow,
        library: Arc<auditorylab_app::workspace::LibraryView>,
    ) {
        if self
            .library
            .borrow()
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &library))
        {
            return;
        }
        let source = library;
        let handles = source
            .rows
            .iter()
            .map(|row| match row {
                auditorylab_app::workspace::LibraryRowView::Entry { handle, .. } => {
                    Some(handle.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let table = self
            .entry_handles
            .borrow()
            .as_ref()
            .filter(|table| {
                table
                    .as_any()
                    .downcast_ref::<EntryHandles>()
                    .is_some_and(|old| old.0 == handles)
            })
            .cloned()
            .unwrap_or_else(|| ModelRc::new(EntryHandles(handles)));
        *self.entry_handles.borrow_mut() = Some(table.clone());
        ui.set_library_entry(crate::generated::EntryAddress {
            table: table.clone(),
            row: source
                .selected
                .and_then(|row| i32::try_from(row).ok())
                .unwrap_or(-1),
        });
        let library = auditorylab_presentation::workspace_text::library_view(&source);
        ui.set_library_heading(read_only(&library.heading));
        ui.set_rhythm_filtered(source.query.rhythm_filtered());
        ui.set_open_files_scope(
            source.query.scope == auditorylab_app::core::query::Scope::OpenFiles,
        );
        ui.set_show_built_in_dates(matches!(
            source.sort,
            auditorylab_app::core::query::SortBy::Created
                | auditorylab_app::core::query::SortBy::Modified
                | auditorylab_app::core::query::SortBy::Added
        ));
        self.counts
            .library_projections
            .set(self.counts.library_projections.get() + 1);
        sync_rows(
            &self.catalog,
            source
                .rows
                .iter()
                .enumerate()
                .map(|(index, source_row)| {
                    let row=auditorylab_presentation::library_text::row(source_row,source.sort);
                    CatalogRow {
                    entry: crate::generated::EntryAddress {
                        table: table.clone(),
                        row: i32::try_from(index).unwrap_or(-1),
                    },
                    built_in: matches!(source_row,auditorylab_app::workspace::LibraryRowView::Entry {entry,..} if matches!(entry.key,auditorylab_app::document::origin::LibraryKey::BuiltIn(_))),
                    text: read_only(&row.text),
                    binding: row.key.as_str().into(),
                    title: read_only(&row.title),
                    collection: read_only(&row.collection),
                    summary: read_only(&row.summary),
                    bands: read_only(&row.bands),
                    duration: read_only(&row.duration),
                    dates: read_only(&row.dates),
                }})
                .collect(),
        );
        if let Some(w) = self.workspace.borrow().as_ref() {
            let binding =
                auditorylab_presentation::library_text::selection_binding(w.library_binding());
            ui.set_library_key(binding.key.into());
            ui.set_library_token(binding.token.into());
            ui.set_library_trashed(binding.trashed);
            ui.set_library_status(read_only(&w.library_status()));
            ui.set_library_alert(read_only(&w.library_alert()));
            ui.set_library_dates(read_only(&w.library_dates()));
            ui.set_library_dates_utc(read_only(&w.library_dates_utc()));
            ui.set_library_count(read_only(&w.library_count()));
            ui.set_library_overview(read_only(&w.library_overview()));
            ui.set_library_empty_message(read_only(w.library_empty_message()));
            ui.set_library_filters_active(
                *w.catalog_query() != w.catalog_query().cleared_filters(),
            );
            let choices = [
                QueryFacet::Scope,
                QueryFacet::Sort,
                QueryFacet::Direction,
                QueryFacet::Group,
                QueryFacet::Band,
                QueryFacet::Method,
                QueryFacet::Population,
                QueryFacet::Role,
                QueryFacet::Analysis,
            ]
            .into_iter()
            .map(|facet| (facet, w.library_choices(facet)))
            .collect();
            self.project_choice_models(ui, &w.library_facets(), choices);
            ui.set_calendar_context(
                auditorylab_presentation::calendar_text::local_context().into(),
            );
        }
        // Virtual group/diagnostic rows do not create a selectable sound.
        let has_results = source.rows.iter().any(|row| {
            matches!(
                row,
                auditorylab_app::workspace::LibraryRowView::Entry { .. }
            )
        });
        ui.set_has_results(has_results);
        if !has_results {
            ui.set_library_detail(false);
        }
        ui.set_selected(
            source
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
        ui.set_can_open_study(!library.study_url.is_empty());
        *self.library.borrow_mut() = Some(source);
    }
}
