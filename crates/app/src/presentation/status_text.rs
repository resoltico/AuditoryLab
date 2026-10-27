//! Shared status and refusal sentences for interactive frontends.
use crate::error::Outcome;
use crate::presentation::AudioErrorText;
use crate::workspace::WorkerFailure;
use crate::workspace::output::PreviewIssue;
use crate::workspace::status::*;
impl FileActivity {
    pub(crate) fn description(&self) -> String {
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
impl InventoryStatus {
    pub(crate) fn description(&self) -> String {
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
            text.push_str(&format!("\n{}: {}", problem.code, problem.message()));
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
impl SaveFeedback {
    pub(crate) fn description(&self) -> String {
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
impl LibraryContext {
    pub(crate) fn description(&self) -> String {
        self.selector.as_ref().map_or_else(
            || format!("Location: {}", self.path.display()),
            |key| format!("Selector: {key}\nLocation: {}", self.path.display()),
        )
    }
}
impl LibraryStatus {
    pub(crate) fn description(&self) -> String {
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
                let heading = match error.outcome() {
                    Outcome::Conflict if *publishing => {
                        "Saved version or identity conflicts with the captured operation"
                    }
                    Outcome::HistoryConflict if *publishing => {
                        "Saved identity or creation history conflicts with the captured operation"
                    }
                    Outcome::Conflict | Outcome::HistoryConflict => "Library read conflict",
                    Outcome::Write | Outcome::Unreadable | Outcome::IncompleteInventory
                        if *publishing =>
                    {
                        "Library operation failed; inspect this location before retrying"
                    }
                    Outcome::Busy => "Library is busy; retry after the current operation",
                    Outcome::LockUnavailable => "Library locking is unavailable",
                    Outcome::Cancelled => "Library operation cancelled",
                    Outcome::ReadOnlyBuiltIn => "Built-in recipes are read-only",
                    _ if *publishing => "Library operation refused",
                    _ => "Library read failed",
                };
                let heading = context
                    .selector
                    .as_ref()
                    .map_or_else(|| heading.to_owned(), |key| format!("{heading} · {key}"));
                let publication = if *publishing
                    && matches!(
                        error.outcome(),
                        Outcome::Conflict | Outcome::HistoryConflict
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
impl FileStatus {
    pub(crate) fn library_failure_notice(&self) -> Option<String> {
        match self {
            Self::Library(status @ LibraryStatus::Failed { .. }) => Some(status.description()),
            _ => None,
        }
    }
}
impl FileStatus {
    pub(crate) fn description(&self) -> String {
        match self {
        Self::Idle=>String::new(),Self::Running(activity)=>activity.description(),Self::Refused(refusal)=>super::workspace_text::refusal_text(refusal),
        Self::Failed(error)=>error.to_string(),Self::DialogFailed {cause}=>cause.clone(),
        Self::WorkerFailed {activity,error}=>format!("{} · {}{}",error.description(),activity.path.display(),if activity.may_publish && !matches!(error,WorkerFailure::Launch(_)) {"\nDraft retained. Inspect the affected path before retrying; a failed worker does not establish that storage was unchanged."}else{""}),
        Self::Imported {snapshot,hidden}=>format!("Opened {}.{}",snapshot.title(),if *hidden {"\nHidden by current filters; clear filters to show the imported recipe."}else{""}),
        Self::RecipeExported {path,warnings}=>format!("Exported captured portable recipe to {}.{}",path.display(),warnings.iter().map(|warning|format!("\n{warning}")).collect::<String>()),
        Self::Exported {path,exported}=>super::workspace_text::exported_text(path,exported),Self::Library(status)=>status.description(),
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
impl EditStatus {
    pub(crate) fn description(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Refused(refusal) => super::workspace_text::refusal_text(refusal),
        }
    }
}
impl AudioStatus {
    pub(crate) fn description(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Refused(refusal) => super::workspace_text::refusal_text(refusal),
            Self::LiveUpdate(error) => error.to_string(),
            Self::Ended { report, monitored } => {
                super::workspace_text::playback_text(report, *monitored)
            }
            Self::Failed(error) => error.to_string(),
            Self::WorkerFailed(error) => error.description(),
        }
    }
}

impl WorkerFailure {
    pub(crate) fn description(&self) -> String {
        match self {
            Self::Launch(error) => format!("Cannot start worker: {error}"),
            Self::Panicked => crate::presentation::WORKER_FAILED.into(),
            Self::InvalidOutcome => "The worker returned a result for another operation.".into(),
        }
    }
}
impl PreviewIssue {
    pub(crate) fn description(&self) -> String {
        match self {
            Self::Snapshot(error) => error.to_string(),
            Self::Acoustic {
                title,
                rate,
                error: Some(error),
            } => error.at_rate(title, *rate),
            Self::Acoustic {
                title,
                rate,
                error: None,
            } => format!("Cannot generate a preview of {title} at {} Hz.", rate.hz()),
        }
    }
}

pub(crate) fn shutdown_reset_failure(error: &crate::error::EditorError, context: &str) -> String {
    format!("Trash confirmed; editor reset failed: {error}{context}")
}
