// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Typed operation state. Only projections format these values for display.
use super::{
    Refusal,
    tasks::{FileKind, WorkerFailure},
};
use crate::{
    error::{Error, ErrorKind},
    library::{LibraryDiagnostic, LibraryKey, MutationStatus},
    recipe::RecipeSnapshot,
    session::{LiveUpdateError, PlaybackReport},
    wav::Exported,
};
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone, Copy)]
pub(super) enum FileAction {
    Import,
    Library,
    RecipeExport,
    AudioExport,
}
#[derive(Debug, Clone)]
pub(super) struct FileActivity {
    pub kind: FileAction,
    pub path: PathBuf,
    pub title: Option<String>,
    pub may_publish: bool,
}
impl FileActivity {
    pub(super) fn from_kind(kind: &FileKind) -> Self {
        match kind {
            FileKind::Import(path) => Self {
                kind: FileAction::Import,
                path: path.clone(),
                title: None,
                may_publish: false,
            },
            FileKind::Managed(job) => Self {
                kind: FileAction::Library,
                path: job.store.root().path().into(),
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
    fn description(&self) -> String {
        match self.kind {
            FileAction::Import => format!("Opening {}.", self.path.display()),
            FileAction::Library => "Updating the persistent library.".into(),
            FileAction::RecipeExport => {
                format!("Exporting portable recipe to {}.", self.path.display())
            }
            FileAction::AudioExport => format!(
                "Exporting {} to {}.",
                self.title.as_deref().unwrap_or("sound"),
                self.path.display()
            ),
        }
    }
}
#[derive(Debug, Clone, Default)]
pub(super) struct InventoryStatus {
    pub complete: bool,
    pub not_initialized: bool,
    pub count: usize,
    pub diagnostics: Vec<LibraryDiagnostic>,
    pub stages: Vec<String>,
}
impl InventoryStatus {
    pub(super) fn description(&self) -> String {
        let mut text = format!(
            "{} · {} readable User entries{}",
            if self.complete {
                "Inventory complete"
            } else {
                "Inventory incomplete · unseen previous rows retained; refresh before acting"
            },
            self.count,
            if self.not_initialized {
                " · Not initialized"
            } else {
                ""
            }
        );
        for problem in &self.diagnostics {
            text.push_str(&format!("\n{}: {}", problem.code, problem.message));
        }
        if !self.stages.is_empty() {
            text.push_str(&format!(
                "\n{} staging artifacts retained for recovery.",
                self.stages.len()
            ));
        }
        text
    }
}
#[derive(Debug)]
pub(super) enum SaveFeedback {
    InvalidAtRate {
        rate: crate::core::SampleRate,
        error: Error,
    },
    EditorRefresh(Error),
    EditorReset(Error),
}
impl SaveFeedback {
    fn description(&self) -> String {
        match self {
            Self::InvalidAtRate { rate, error } => format!(
                "Saved with validation errors at {} Hz: {error}. This finite document can be repaired later.",
                rate.hz()
            ),
            Self::EditorReset(error) => format!(
                "Trash confirmed, but a blank editor could not be created: {error}. Open draft retained; dependent action stopped."
            ),
            Self::EditorRefresh(error) => {
                format!("Saved, but the editor could not refresh: {error}. Open draft retained.")
            }
        }
    }
}
/// Location and identity captured by the immutable library request.
#[derive(Debug, Clone)]
pub(super) struct LibraryContext {
    pub path: PathBuf,
    pub selector: Option<LibraryKey>,
}
impl LibraryContext {
    pub(super) fn description(&self) -> String {
        self.selector.as_ref().map_or_else(
            || format!("Location: {}", self.path.display()),
            |key| format!("Selector: {key}\nLocation: {}", self.path.display()),
        )
    }
}
#[derive(Debug)]
pub(super) enum LibraryStatus {
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
impl LibraryStatus {
    fn description(&self) -> String {
        match self {
            Self::Inventory(status) => status.description(),
            Self::Loaded => {
                "Saved version loaded; open draft and playing snapshot retained.".into()
            }
            Self::Written {
                key,
                status,
                document,
                diagnostics,
                feedback,
            } => format!(
                "{} · {key}\n{}{}",
                status.label(),
                if *document {
                    "Saved captured version; later edits remain independent. Use Show saved if filters hide it."
                } else {
                    "Library operation confirmed."
                },
                diagnostics
                    .iter()
                    .cloned()
                    .chain(feedback.iter().map(SaveFeedback::description))
                    .map(|d| format!("\n{d}"))
                    .collect::<String>()
            ),
            Self::ExternalWritten {
                status,
                diagnostics,
            } => format!(
                "{} · External file saved from captured draft.\n{}",
                status.label(),
                diagnostics.join("\n")
            ),
            Self::Failed {
                error,
                publishing,
                context,
            } => {
                let recovery = "Draft retained. Refresh and reload the saved version, keep the draft, or save an independent copy.";
                let heading = match error.kind() {
                    ErrorKind::Conflict => {
                        "Saved version or identity conflicts with the captured operation"
                    }
                    ErrorKind::HistoryConflict => {
                        "Saved identity or creation history conflicts with the captured operation"
                    }
                    _ if *publishing => {
                        "Library operation failed; inspect this location before retrying"
                    }
                    _ => "Library read failed",
                };
                let publication = if matches!(
                    error.kind(),
                    ErrorKind::Conflict | ErrorKind::HistoryConflict
                ) {
                    "\nThe conflicting operation did not publish a replacement."
                } else {
                    ""
                };
                format!(
                    "{heading}\n{}\n{error}{publication}\n{recovery}",
                    context.description()
                )
            }
            Self::InvalidOwner => {
                "Saved content belongs to another sound; this sound’s saved state was not changed."
                    .into()
            }
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub(super) enum FileNotice {
    RetainedSource,
    ChangedSelection,
    MissingSavedEntry,
    SavedSnapshotStillDirty,
    ExportCancelled,
}
#[derive(Default)]
pub(super) enum FileStatus {
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
    pub(super) fn library_failure_notice(&self) -> Option<String> {
        match self {
            Self::Library(status @ LibraryStatus::Failed { .. }) => Some(status.description()),
            _ => None,
        }
    }
    pub(super) fn conclusion(
        &self,
        operation: super::tasks::Conclusion,
    ) -> super::tasks::Conclusion {
        if self.editor_reset_error().is_some() {
            super::tasks::Conclusion::Failed
        } else {
            operation
        }
    }
    pub(super) fn editor_reset_error(&self) -> Option<&Error> {
        if let Self::Library(LibraryStatus::Written { feedback, .. }) = self {
            feedback.iter().find_map(|item| match item {
                SaveFeedback::EditorReset(error) => Some(error),
                _ => None,
            })
        } else {
            None
        }
    }
    pub(super) fn description(&self) -> String {
        match self {
        Self::Idle=>String::new(),Self::Running(activity)=>activity.description(),Self::Refused(refusal)=>super::views::refusal_text(refusal),
        Self::Failed(error)=>error.to_string(),Self::DialogFailed {cause}=>cause.clone(),
        Self::WorkerFailed {activity,error}=>format!("{} · {}{}",error.description(),activity.path.display(),if activity.may_publish && !matches!(error,WorkerFailure::Launch(_)) {"\nDraft retained. Inspect the affected path before retrying; a failed worker does not establish that storage was unchanged."}else{""}),
        Self::Imported {snapshot,hidden}=>format!("Opened {}.{}",snapshot.title(),if *hidden {"\nHidden by current filters; clear filters to show the imported recipe."}else{""}),
        Self::RecipeExported {path,warnings}=>format!("Exported captured portable recipe to {}.{}",path.display(),warnings.iter().map(|warning|format!("\n{warning}")).collect::<String>()),
        Self::Exported {path,exported}=>super::views::exported_text(path,exported),Self::Library(status)=>status.description(),
        Self::Notice(notice)=>match notice {
            FileNotice::RetainedSource=>"Showing the retained source snapshot; filters explicitly cleared. This does not reload or reconstruct an ancestor.".into(),
            FileNotice::ChangedSelection=>"Saved selection changed. Refresh before editing this version.".into(),
            FileNotice::MissingSavedEntry=>"The saved entry is no longer in the observed inventory; refresh before locating it.".into(),
            FileNotice::SavedSnapshotStillDirty=>crate::presentation::SAVED_SNAPSHOT_STILL_DIRTY.into(),
            FileNotice::ExportCancelled=>crate::presentation::EXPORT_CANCELLED.into(),
        },
    }
    }
}
#[derive(Default)]
pub(super) enum EditStatus {
    #[default]
    Idle,
    Refused(Refusal),
}
impl EditStatus {
    pub(super) fn description(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Refused(refusal) => super::views::refusal_text(refusal),
        }
    }
}
#[derive(Default)]
pub(super) enum AudioStatus {
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
impl AudioStatus {
    pub(super) fn description(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Refused(refusal) => super::views::refusal_text(refusal),
            Self::LiveUpdate(error) => error.to_string(),
            Self::Ended { report, monitored } => super::views::playback_text(report, *monitored),
            Self::Failed(error) => error.to_string(),
            Self::WorkerFailed(error) => error.description(),
        }
    }
}
