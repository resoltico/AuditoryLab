//! The occupied file slot and file-operation admission/transitions.
use super::*;
#[derive(Default)]
pub(super) struct FileState {
    slot: Option<FileTask>,
    status: FileStatus,
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
    pub(super) fn idle_admission(&self) -> Result<(), Refusal> {
        if self.busy() {
            return Err(Refusal::FileBusy);
        }
        Ok(())
    }

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

    pub(super) const fn task(&self) -> Option<&FileTask> {
        self.slot.as_ref()
    }
    pub(super) const fn status(&self) -> &FileStatus {
        &self.status
    }
    pub(super) fn record_status(&mut self, status: FileStatus) {
        self.status = status;
    }

    pub(super) fn apply(&mut self, command: FileCommand) -> Result<Option<FileRequest>, Refusal> {
        Ok(match command {
            FileCommand::Import => Some(FileRequest::Import),
            FileCommand::Save => Some(FileRequest::Save),
            FileCommand::SaveCopy => Some(FileRequest::SaveCopy),
            FileCommand::ExportRecipe => Some(FileRequest::ExportRecipe),
            FileCommand::ExportWav => Some(FileRequest::ExportWav),
            FileCommand::CancelExport => {
                self.cancel_export()?;
                None
            }
        })
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

pub(super) enum FileRequest {
    Import,
    Save,
    SaveCopy,
    ExportRecipe,
    ExportWav,
}
