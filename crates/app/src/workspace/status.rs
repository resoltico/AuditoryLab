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

/// Operation occupying the file task slot.
#[derive(Debug, Clone, Copy)]
pub enum FileAction {
    /// Read portable input without granting managed membership.
    Import,
    /// Read or mutate the persistent managed library.
    Library,
    /// Publish captured portable content with unchanged identity and history.
    RecipeExport,
    /// Render and publish a finite audio file.
    AudioExport,
}
/// Source and publication capability captured by a running file task.
#[derive(Debug, Clone)]
pub struct FileActivity {
    /// Operation captured before worker execution.
    pub kind: FileAction,
    /// Captured source or destination path; navigation cannot retarget it.
    pub path: PathBuf,
    /// Source title captured at admission, when the operation has one.
    pub title: Option<String>,
    /// Whether the admitted operation can change persistent storage.
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
/// Retained result of the latest bounded managed-library scan.
#[derive(Debug, Clone, Default)]
pub struct InventoryStatus {
    /// Every eligible managed record was examined within the scan limits.
    pub complete: bool,
    /// The configured root lacks an initialized supported layout.
    pub not_initialized: bool,
    /// Readable unique managed records observed in this scan.
    pub count: usize,
    /// Typed nonfatal causes retained with their storage context.
    pub diagnostics: Vec<LibraryDiagnostic>,
    /// Owned staging paths retained for inspection and recovery.
    pub stages: Vec<std::path::PathBuf>,
}

/// A committed publication whose editor follow-up needs attention.
#[derive(Debug, Clone)]
pub enum SaveFeedback {
    /// Publication succeeded, but the saved sound is invalid at the diagnostic rate.
    InvalidAtRate {
        /// Sample rate used for this validation result.
        rate: crate::core::SampleRate,
        /// Original typed cause retained after completion.
        error: crate::document::error::RenditionError,
    },
    /// Publication succeeded, but refreshing the editor failed.
    EditorRefresh(crate::document::error::EditorError),
    /// Trash committed, but creating the replacement blank editor failed.
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

/// Captured managed-library completion and recovery state.
#[derive(Debug, Clone)]
pub enum LibraryStatus {
    /// Bounded scan results, including incomplete observations.
    Inventory(Arc<InventoryStatus>),
    /// Selection reloaded without replacing the draft or playing source.
    Loaded,
    /// A managed publication completed with confirmed mutation truth.
    Written {
        /// Managed identity captured by the admitted operation.
        key: LibraryKey,
        /// Confirmed publication state, independent of report output.
        status: MutationStatus,
        /// Publication captured the editor document rather than only a library mutation.
        document: bool,
        /// Editor follow-up failures after confirmed storage publication.
        feedback: Vec<SaveFeedback>,
        /// Typed nonfatal causes retained with their storage context.
        diagnostics: Vec<crate::storage::warning::StorageWarning>,
    },
    /// A captured external-file save completed.
    ExternalWritten {
        /// Confirmed publication state, independent of report output.
        status: MutationStatus,
        /// Typed nonfatal causes retained with their storage context.
        diagnostics: Vec<crate::storage::warning::StorageWarning>,
    },
    /// The admitted operation returned its typed failure.
    Failed {
        /// Original typed cause retained after completion.
        error: Error,
        /// The admitted operation could publish persistent changes.
        publishing: bool,
        /// Root and selector captured before worker execution.
        context: LibraryContext,
    },
    /// The completion belongs to a different document identity.
    InvalidOwner,
}

/// Nonfatal coordinator events retained for the next projection.
#[derive(Debug, Clone, Copy)]
pub enum FileNotice {
    /// The retained source remains independent of later navigation.
    RetainedSource,
    /// Navigation changed while the worker retained its original input.
    ChangedSelection,
    /// The saved entry is no longer present in the current inventory.
    MissingSavedEntry,
    /// A captured version saved while later edits remain unsaved.
    SavedSnapshotStillDirty,
    /// An audio export was cancelled before publication.
    ExportCancelled,
}
/// Current file-slot activity, completion or admission refusal.
#[derive(Debug, Clone, Default)]
pub enum FileStatus {
    /// No operation currently contributes a status.
    #[default]
    Idle,
    /// An admitted operation still occupies its joined task slot.
    Running(FileActivity),
    /// Admission denied a command without starting new work.
    Refused(Refusal),
    /// The admitted operation returned its typed failure.
    Failed(Error),
    /// An opaque native panel failure supplied by the shell, not composed status wording.
    DialogFailed {
        /// Opaque native-panel cause supplied by the shell.
        cause: String,
    },
    /// Worker execution failed independently of the admitted operation.
    WorkerFailed {
        /// Source and capability captured before thread launch.
        activity: FileActivity,
        /// Original typed cause retained after completion.
        error: WorkerFailure,
    },
    /// Portable input was captured as a temporary source.
    Imported {
        /// Immutable imported source retained independently of navigation.
        snapshot: RecipeSnapshot,
        /// Current query restrictions hide the imported source.
        hidden: bool,
    },
    /// Captured portable content was published to an external destination.
    RecipeExported {
        /// Captured source or destination path; navigation cannot retarget it.
        path: PathBuf,
        /// Confirmed post-publication durability, cleanup or chronology conditions.
        warnings: Vec<crate::storage::warning::StorageWarning>,
    },
    /// A finite WAV file was published with measured encoding facts.
    Exported {
        /// Captured source or destination path; navigation cannot retarget it.
        path: PathBuf,
        /// Exact finite encoding and publication facts.
        exported: Exported,
    },
    /// Read or mutate the persistent managed library.
    Library(LibraryStatus),
    /// A nonfatal coordinator event remains visible.
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
/// Most recent editor admission result.
#[derive(Debug, Clone, Default)]
pub enum EditStatus {
    /// No operation currently contributes a status.
    #[default]
    Idle,
    /// Admission denied a command without starting new work.
    Refused(Refusal),
}

/// Current audio completion, admission or live-update result.
#[derive(Debug, Clone, Default)]
pub enum AudioStatus {
    /// No operation currently contributes a status.
    #[default]
    Idle,
    /// Admission denied a command without starting new work.
    Refused(Refusal),
    /// The bound session refused a whole-target live update.
    LiveUpdate(LiveUpdateError),
    /// A joined audio session supplied its terminal report.
    Ended {
        /// Terminal report from the joined audio session.
        report: PlaybackReport,
        /// Whether this session supplied generated monitor data.
        monitored: bool,
    },
    /// The admitted operation returned its typed failure.
    Failed(Error),
    /// Worker execution failed independently of the admitted operation.
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
