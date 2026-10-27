//! Typed operation state. Only projections format these values for display.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::{
    Refusal,
    tasks::{FileKind, WorkerFailure},
};
use crate::document::origin::LibraryKey;
use std::{path::PathBuf, sync::Arc};
use {
    crate::audio::session::LiveUpdateError, crate::audio::session::PlaybackReport,
    crate::document::recipe::RecipeSnapshot, crate::services::OperationError as Error,
    crate::storage::library::LibraryDiagnostic, crate::storage::library::MutationStatus,
    crate::storage::wav::Exported,
};

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
    pub stages: Vec<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub(crate) enum SaveFeedback {
    InvalidAtRate {
        rate: crate::core::SampleRate,
        error: crate::document::error::RenditionError,
    },
    EditorRefresh(crate::document::error::EditorError),
    EditorReset(crate::document::error::EditorError),
}

/// Location and identity captured by the immutable library request.
#[derive(Debug, Clone)]
pub struct LibraryContext {
    /// Root used by the admitted operation.
    pub path: PathBuf,
    /// Selector captured before execution.
    pub selector: Option<LibraryKey>,
}

#[derive(Debug, Clone)]
pub(crate) enum LibraryStatus {
    Inventory(Arc<InventoryStatus>),
    Loaded,
    Written {
        key: LibraryKey,
        status: MutationStatus,
        document: bool,
        feedback: Vec<SaveFeedback>,
        diagnostics: Vec<crate::storage::warning::StorageWarning>,
    },
    ExternalWritten {
        status: MutationStatus,
        diagnostics: Vec<crate::storage::warning::StorageWarning>,
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
#[derive(Debug, Clone, Default)]
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
        warnings: Vec<crate::storage::warning::StorageWarning>,
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
    pub(crate) fn editor_reset_error(&self) -> Option<&crate::document::error::EditorError> {
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
#[derive(Debug, Clone, Default)]
pub(crate) enum EditStatus {
    #[default]
    Idle,
    Refused(Refusal),
}

#[derive(Debug, Clone, Default)]
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

/// Failure of an admitted inventory read, including worker execution.
#[derive(Debug, Clone)]
pub enum ReadFailure {
    /// Operation returned an exact typed cause.
    Operation(crate::services::OperationError),
    /// Thread creation failed before execution.
    Launch(crate::services::IoCause),
    /// Worker panicked, so its result cannot be trusted.
    Panicked,
    /// Completion did not match the admitted operation.
    InvalidOutcome,
}

impl std::fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Operation(..) => "read.operation",
            Self::Launch(..) => "read.launch",
            Self::Panicked => "read.panicked",
            Self::InvalidOutcome => "read.invalid_outcome",
        })
    }
}
