//! The occupied file slot and file-operation admission/transitions.
use super::*;
#[derive(Default)]
pub(super) struct FileState {
    pub(super) slot: Option<FileTask>,
    pub(super) status: FileStatus,
}
impl FileState {
    pub(super) const fn busy(&self) -> bool {
        self.slot.is_some()
    }
    pub(super) fn exporting(&self) -> bool {
        self.slot
            .as_ref()
            .is_some_and(|task| matches!(task.kind, FileKind::Export { .. }))
    }
    pub(super) fn blocks_edit(&self) -> bool {
        self.slot
            .as_ref()
            .is_some_and(|task| matches!(task.kind, FileKind::Export { .. } | FileKind::Import(_)))
    }
    pub(super) fn idle_admission(&self, prompts: &Prompts, pending: bool) -> Result<(), Refusal> {
        prompts.admit_start(pending)?;
        if self.busy() {
            return Err(Refusal::FileBusy);
        }
        Ok(())
    }
}

impl FileState {
    pub(super) fn start(&mut self, id: TaskId, kind: FileKind) -> Job {
        assert!(
            self.slot.is_none(),
            "file slot is released only by its joined completion"
        );
        if !kind.background_read() {
            self.status = FileStatus::Running(FileActivity::from_kind(&kind));
        }
        let job = Job::file(id, kind.clone());
        self.slot = Some(FileTask { id, kind });
        job
    }
    pub(super) fn cancel_admission(&self) -> Result<&Cancel, Refusal> {
        match self.slot.as_ref().map(|task| &task.kind) {
            Some(FileKind::Export { cancel, .. }) => Ok(cancel),
            _ => Err(Refusal::NoExport),
        }
    }
    pub(super) fn cancel_export(&self) -> Result<(), Refusal> {
        self.cancel_admission()?.request();
        Ok(())
    }

    pub(super) fn take_finished(&mut self, id: TaskId) -> Option<FileTask> {
        if self.slot.as_ref().is_some_and(|task| task.id == id) {
            self.slot.take()
        } else {
            None
        }
    }
}

pub(super) enum PreparedExport {
    Library(Rendition, String),
    Draft {
        origin: crate::document::recipe::Origin,
        recipe: Recipe,
        signal: Arc<auditorylab_core::Signal>,
        name: String,
    },
}

impl PreparedExport {
    pub(super) fn capture(self) -> (Rendition, String) {
        match self {
            Self::Library(sound, name) => (sound, name),
            Self::Draft {
                origin,
                recipe,
                signal,
                name,
            } => (
                Rendition::from_parts(RecipeSnapshot::new(origin, recipe), signal),
                name,
            ),
        }
    }
}

impl Workspace {
    /// Whether a file task occupies its slot.
    #[must_use]
    pub const fn file_busy(&self) -> bool {
        self.files.busy()
    }
    /// Whether the running file task is an export.
    #[must_use]
    pub fn exporting(&self) -> bool {
        self.files.exporting()
    }
    /// Choose a recipe file to open. Opening may run beside audio.
    ///
    /// # Errors
    /// Returns the admission refusal.
    pub fn import(&mut self) -> Result<(), Refusal> {
        self.import_admission()?;
        let id = self.prompt(PromptState::Import);
        self.effects.push(Effect::ChoosePath {
            prompt: id,
            purpose: PathPurpose::OpenRecipe,
            suggested_name: None,
        });
        self.changed();
        Ok(())
    }
    /// Commit and capture the open document, then choose where to save it.
    /// The captured content is what is written, whatever is edited later.
    ///
    /// # Errors
    /// Returns the admission or readiness refusal.
    pub fn save(&mut self) -> Result<(), Refusal> {
        let result = self.save_inner();
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused((refusal).clone());
        }
        self.changed();
        result
    }
    pub(super) fn save_inner(&mut self) -> Result<(), Refusal> {
        let content = self.prepare_save()?;
        self.editor.commit()?;
        self.submit_live();
        self.start_managed_save(self.editor.session.document(), content)?;
        Ok(())
    }
    /// Export captured portable content without saving or forking the document.
    ///
    /// # Errors
    /// Refuses occupied slots or an unserializable draft; no playback is started.
    pub fn export_recipe(&mut self) -> Result<(), Refusal> {
        let (text, title) = self.prepare_export_recipe()?;
        let prompt = self.prompt(PromptState::ExportRecipe(text));
        self.effects.push(Effect::ChoosePath {
            prompt,
            purpose: PathPurpose::ExportRecipe,
            suggested_name: Some(file_name(&title, "toml")),
        });
        self.changed();
        Ok(())
    }
    /// Validate the current source at the selected export rate, then choose
    /// where to write it. Export needs idle audio and an idle file slot.
    ///
    /// # Errors
    /// Returns the admission or validity refusal.
    pub fn export(&mut self) -> Result<(), Refusal> {
        let result = self.export_inner();
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused((refusal).clone());
        }
        self.changed();
        result
    }
    pub(super) fn export_inner(&mut self) -> Result<(), Refusal> {
        let prepared = self.prepare_export()?;
        if self.view == View::Workbench {
            self.editor.commit()?;
            self.submit_live();
        }
        let (sound, name) = prepared.capture();
        let id = self.prompt(PromptState::Export(sound));
        self.effects.push(Effect::ChoosePath {
            prompt: id,
            purpose: PathPurpose::ExportWav,
            suggested_name: Some(name),
        });
        Ok(())
    }
    /// Cancel the running export. Audio is unaffected.
    ///
    /// # Errors
    /// Returns [`Refusal::NoExport`] when no export runs.
    pub fn cancel_export(&mut self) -> Result<(), Refusal> {
        self.files.cancel_export()
    }
}
