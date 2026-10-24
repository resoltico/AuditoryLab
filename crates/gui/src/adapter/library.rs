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
                .map(|label| slint::StandardListViewItem::from(label.as_str()))
                .collect(),
        );
        ui.set_has_results(!library.rows.is_empty());
        ui.set_selected(
            library
                .selected
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
        ui.set_sound_title(library.title.as_str().into());
        ui.set_structure(library.structure.as_str().into());
        ui.set_studied(library.studied.as_str().into());
        ui.set_recipe_timing(library.timing.as_str().into());
        ui.set_summary(library.summary.as_str().into());
        ui.set_details(library.details.as_str().into());
        ui.set_can_open_study(library.study_url.is_some());
        *self.library.borrow_mut() = Some(library);
    }
}
