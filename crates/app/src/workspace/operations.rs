//! Cross-state operation preparation delegates policy to the owning substate.
use super::*;
impl Workspace {
    pub(super) fn copy_admission(&self) -> Result<(), Refusal> {
        self.start_allowed()?;
        self.library.copy_admission()
    }
    pub(super) fn copy_content_admission(snapshot: &RecipeSnapshot) -> Result<(), Refusal> {
        library::LibraryState::copy_recipe_admission(snapshot.recipe())
    }
    pub(super) fn import_admission(&self) -> Result<(), Refusal> {
        self.files
            .idle_admission(&self.prompts, self.pending.is_some())
    }
    pub(super) fn prepare_save(&mut self) -> Result<Arc<Recipe>, Refusal> {
        self.library.save_destination_admission()?;
        self.import_admission()?;
        self.editor.prepare_save()
    }
    pub(super) fn prepare_save_copy(&mut self) -> Result<Arc<Recipe>, Refusal> {
        self.library.save_destination_admission()?;
        self.import_admission()?;
        let recipe = self.editor.prepare_recipe()?;
        library::LibraryState::copy_recipe_admission(&recipe)?;
        Ok(recipe)
    }

    pub(super) fn prepare_export(&mut self) -> Result<file::PreparedExport, Refusal> {
        self.import_admission()?;
        self.audio.idle_admission()?;
        match self.view {
            View::Library => self.library.prepare_export(self.editor.export_rate),
            View::Workbench => self.editor.prepare_export(),
        }
    }
    pub(super) fn prepare_export_recipe(&mut self) -> Result<(Arc<str>, String), Refusal> {
        self.import_admission()?;
        match self.view {
            View::Library => self.library.prepare_recipe_text(),
            View::Workbench => self.editor.prepare_recipe_text(),
        }
    }
    pub(super) fn prepare_audio(&mut self) -> Result<AudioRequest, Refusal> {
        self.audio
            .play_admission(&self.prompts, self.pending.is_some(), &self.files)?;
        match self.view {
            View::Library => self.library.prepare_audio(),
            View::Workbench => self.editor.prepare_live(),
        }
    }
    pub(super) fn preview_admission(&self) -> Result<(), Refusal> {
        OutputState::preview_admission(&self.audio)
    }
    pub(super) fn component_add_admission(
        &self,
        row: RowKey,
        kind: CarrierKind,
    ) -> Result<(), Refusal> {
        self.editor.addition_admission(
            row,
            controls::default_component(kind),
            &self.prompts,
            self.pending.is_some(),
            &self.files,
        )
    }
    pub(super) fn pair_add_admission(
        &self,
        row: RowKey,
        kind: crate::core::PairKind,
    ) -> Result<(), Refusal> {
        self.editor.addition_admission(
            row,
            controls::default_pair(kind),
            &self.prompts,
            self.pending.is_some(),
            &self.files,
        )
    }
    pub(super) fn restore_component_admission(&self, row: RowKey) -> Result<(), Refusal> {
        self.editor
            .restore_admission(row, &self.prompts, self.pending.is_some(), &self.files)
    }
}

impl Workspace {
    pub(super) fn library_job_admission(&self) -> Result<(), Refusal> {
        self.import_admission()?;
        self.library.save_destination_admission()
    }
    pub(super) fn prepare_add_to_library(&self) -> Result<Arc<Recipe>, Refusal> {
        let recipe = self.library.prepare_add()?;
        self.library_job_admission()?;
        Ok(recipe)
    }
    pub(super) fn trash_request_admission(
        &self,
        key: &crate::services::library::LibraryKey,
    ) -> Result<(), Refusal> {
        key.require_user()
            .map_err(|error| Refusal::Document(Box::new(error.into())))?;
        if self.editor.dirty_managed(key) {
            self.start_allowed()
        } else {
            self.library_job_admission()
        }
    }
}
