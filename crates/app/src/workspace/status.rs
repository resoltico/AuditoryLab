// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Typed operation state. Only projections format these values for display.
use super::{
    Refusal,
    tasks::{FileKind, WorkerFailure},
};
use crate::{
    audio::session::{LiveUpdateError, PlaybackReport},
    document::recipe::RecipeSnapshot,
    error::OperationError as Error,
    storage::library::{LibraryDiagnostic, LibraryKey, MutationStatus},
    storage::wav::Exported,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone, Copy)]
pub(crate) enum FileAction {
    Import,
    Library,
    RecipeExport,
    AudioExport,
}
#[derive(Debug, Clone)]
pub(crate) struct FileActivity {
    pub kind: FileAction,
    pub path: PathBuf,
    pub title: Option<String>,
    pub may_publish: bool,
}
impl FileActivity {
    pub(crate) fn from_kind(kind: &FileKind) -> Self {
        match kind {
            FileKind::Import(path) => Self {
                kind: FileAction::Import,
                path: path.clone(),
                title: None,
                may_publish: false,
            },
            FileKind::Managed(job) => Self {
                kind: FileAction::Library,
                path: job.store.path().into(),
                title: None,
                may_publish: job.may_publish(),
            },
            FileKind::ExportRecipe { path, .. } => Self {
                kind: FileAction::RecipeExport,
                path: path.clone(),
                title: None,
                may_publish: true,
            },
            FileKind::Export { path, title, .. } => Self {
                kind: FileAction::AudioExport,
                path: path.clone(),
                title: Some(title.clone()),
                may_publish: true,
            },
        }
    }
}
#[derive(Debug, Clone, Default)]
pub(crate) struct InventoryStatus {
    pub complete: bool,
    pub not_initialized: bool,
    pub count: usize,
    pub diagnostics: Vec<LibraryDiagnostic>,
    pub stages: Vec<String>,
}

#[derive(Debug)]
pub(crate) enum SaveFeedback {
    InvalidAtRate {
        rate: crate::core::SampleRate,
        error: crate::error::LoadError,
    },
    EditorRefresh(crate::error::EditorError),
    EditorReset(crate::error::EditorError),
}

/// Location and identity captured by the immutable library request.
#[derive(Debug, Clone)]
pub(crate) struct LibraryContext {
    pub path: PathBuf,
    pub selector: Option<LibraryKey>,
}

#[derive(Debug)]
pub(crate) enum LibraryStatus {
    Inventory(Arc<InventoryStatus>),
    Loaded,
    Written {
        key: LibraryKey,
        status: MutationStatus,
        document: bool,
        feedback: Vec<SaveFeedback>,
        diagnostics: Vec<String>,
    },
    ExternalWritten {
        status: MutationStatus,
        diagnostics: Vec<String>,
    },
    Failed {
        error: Error,
        publishing: bool,
        context: LibraryContext,
    },
    InvalidOwner,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum FileNotice {
    RetainedSource,
    ChangedSelection,
    MissingSavedEntry,
    SavedSnapshotStillDirty,
    ExportCancelled,
}
#[derive(Default)]
pub(crate) enum FileStatus {
    #[default]
    Idle,
    Running(FileActivity),
    Refused(Refusal),
    Failed(Error),
    /// An opaque native panel failure supplied by the shell, not composed status wording.
    DialogFailed {
        cause: String,
    },
    WorkerFailed {
        activity: FileActivity,
        error: WorkerFailure,
    },
    Imported {
        snapshot: RecipeSnapshot,
        hidden: bool,
    },
    RecipeExported {
        path: PathBuf,
        warnings: Vec<String>,
    },
    Exported {
        path: PathBuf,
        exported: Exported,
    },
    Library(LibraryStatus),
    Notice(FileNotice),
}
impl FileStatus {
    pub(crate) fn conclusion(
        &self,
        operation: super::tasks::Conclusion,
    ) -> super::tasks::Conclusion {
        if self.editor_reset_error().is_some() {
            super::tasks::Conclusion::Failed
        } else {
            operation
        }
    }
    pub(crate) fn editor_reset_error(&self) -> Option<&crate::error::EditorError> {
        if let Self::Library(LibraryStatus::Written { feedback, .. }) = self {
            feedback.iter().find_map(|item| match item {
                SaveFeedback::EditorReset(error) => Some(error),
                _ => None,
            })
        } else {
            None
        }
    }
}
#[derive(Default)]
pub(crate) enum EditStatus {
    #[default]
    Idle,
    Refused(Refusal),
}

#[derive(Default)]
pub(crate) enum AudioStatus {
    #[default]
    Idle,
    Refused(Refusal),
    LiveUpdate(LiveUpdateError),
    Ended {
        report: PlaybackReport,
        monitored: bool,
    },
    Failed(Error),
    WorkerFailed(WorkerFailure),
}
