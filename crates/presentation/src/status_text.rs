//! Shared status and refusal sentences for interactive frontends.
use super::Explain;
use crate::AudioErrorText;
use crate::*;
use auditorylab_app::outcome::Outcome;
use auditorylab_app::workspace::WorkerFailure;
use auditorylab_app::workspace::output::PreviewIssue;
use auditorylab_app::workspace::status::*;

pub(crate) fn shutdown_reset_failure(
    error: &auditorylab_app::document::error::EditorError,
    context: &str,
) -> String {
    format!(
        "Trash confirmed; editor reset failed: {error}{context}",
        error = error.explain()
    )
}

impl super::Explain for auditorylab_app::workspace::TaskFailure {
    fn explain(&self) -> String {
        match self {
            Self::Operation(error) => error.explain(),
            Self::Worker(error) => error.description(),
        }
    }
}

impl super::Explain for auditorylab_app::workspace::ShutdownError {
    fn explain(&self) -> String {
        use auditorylab_app::workspace::ShutdownIssue;
        use auditorylab_app::workspace::TaskFailure;
        self.issues
            .iter()
            .map(|issue| match issue {
                ShutdownIssue::Library(library) => LibraryStatus::Failed {
                    error: library.error.clone(),
                    publishing: library.publishing,
                    context: library.context.clone(),
                }
                .description(),
                ShutdownIssue::Task { error, context } => {
                    let error = match error {
                        TaskFailure::Operation(error) => error.explain(),
                        TaskFailure::Worker(error) => error.description(),
                    };
                    format!("{error}{}", context.description())
                }
                ShutdownIssue::EditorReset { error, context } => {
                    shutdown_reset_failure(error, &context.description())
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub trait OutputIssueText {
    /// Explain why a capture is unavailable while retaining its original cause.
    fn description(&self) -> String;
}
impl OutputIssueText for auditorylab_app::workspace::output::OutputIssue {
    fn description(&self) -> String {
        match self {
            Self::Capture(error) => error.description(),
            Self::Preview(reason) => reason.description(),
        }
    }
}

pub trait JobContextText {
    /// Show the operation address retained before applying its shutdown completion.
    fn description(&self) -> String;
}
impl JobContextText for auditorylab_app::workspace::JobContext {
    fn description(&self) -> String {
        match self {
            Self::Library(context) => format!(" · {}", context.description()),
            Self::File(path) => format!(" · {}", path.display()),
            Self::Audio(title) => format!(" · {title}"),
            Self::Unknown => String::new(),
        }
    }
}

pub trait PreviewIssueText {
    /// Explain an unavailable preview using the captured source title and sample rate.
    fn description(&self) -> String;
}
impl PreviewIssueText for PreviewIssue {
    fn description(&self) -> String {
        match self {
            Self::Snapshot(error) => error.explain(),
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

pub trait WorkerFailureText {
    /// Explain execution failure separately from an operation returning an error.
    fn description(&self) -> String;
}
impl WorkerFailureText for WorkerFailure {
    fn description(&self) -> String {
        match self {
            Self::Launch(error) => format!("Cannot start worker: {error}", error = error.explain()),
            Self::Panicked => crate::WORKER_FAILED.into(),
            Self::InvalidOutcome => "The worker returned a result for another operation.".into(),
        }
    }
}

pub trait AudioStatusText {
    /// Describe joined output or a refused live update without inferring playback from monitoring.
    fn description(&self) -> String;
}
impl AudioStatusText for AudioStatus {
    fn description(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Refused(refusal) => super::workspace_text::refusal_text(refusal),
            Self::LiveUpdate(error) => error.explain(),
            Self::Ended { report, monitored } => {
                super::workspace_text::playback_text(report, *monitored)
            }
            Self::Failed(error) => error.explain(),
            Self::WorkerFailed(error) => error.description(),
        }
    }
}

pub trait EditStatusText {
    /// Explain the latest editor admission refusal without changing staged input.
    fn description(&self) -> String;
}
impl EditStatusText for EditStatus {
    fn description(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Refused(refusal) => super::workspace_text::refusal_text(refusal),
        }
    }
}

pub trait FileStatusText {
    /// Describe the current worker, refusal or completion with its retained typed cause.
    fn description(&self) -> String;
}
impl FileStatusText for FileStatus {
    fn description(&self) -> String {
        match self {
        Self::Idle=>String::new(),Self::Running(activity)=>activity.description(),Self::Refused(refusal)=>super::workspace_text::refusal_text(refusal),
        Self::Failed(error)=>error.explain(),Self::DialogFailed {cause}=>cause.clone(),
        Self::WorkerFailed {activity,error}=>format!("{} · {}{}",error.description(),activity.path.display(),if activity.may_publish && !matches!(error,WorkerFailure::Launch(_)) {"\nDraft retained. Inspect the affected path before retrying; a failed worker does not establish that storage was unchanged."}else{""}),
        Self::Imported {snapshot,hidden}=>format!("Opened {}.{}",snapshot.title(),if *hidden {"\nHidden by current filters; clear filters to show the imported recipe."}else{""}),
        Self::RecipeExported {path,warnings}=>format!("Exported captured portable recipe to {}.{}",path.display(),warnings.iter().map(|warning|format!("\n{}", super::publication_text::warning(warning))).collect::<String>()),
        Self::Exported {path,exported}=>super::workspace_text::exported_text(path,exported),Self::Library(status)=>status.description(),
        Self::Notice(notice)=>match notice {
            FileNotice::RetainedSource=>"Showing the retained source snapshot; filters explicitly cleared. This does not reload or reconstruct an ancestor.".into(),
            FileNotice::ChangedSelection=>"Saved selection changed. Refresh before editing this version.".into(),
            FileNotice::MissingSavedEntry=>"The saved entry is no longer in the observed inventory; refresh before locating it.".into(),
            FileNotice::SavedSnapshotStillDirty=>crate::SAVED_SNAPSHOT_STILL_DIRTY.into(),
            FileNotice::ExportCancelled=>crate::EXPORT_CANCELLED.into(),
        },
    }
    }
}

pub trait LibraryStatusText {
    /// Describe confirmed mutation truth, retained input and applicable recovery actions.
    fn description(&self) -> String;
}
impl LibraryStatusText for LibraryStatus {
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
                    .map(super::publication_text::warning)
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
                diagnostics
                    .iter()
                    .map(super::publication_text::warning)
                    .collect::<Vec<_>>()
                    .join("\n")
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
                    context.description(),
                    error = error.explain()
                )
            }
            Self::InvalidOwner => {
                "Saved content belongs to another sound; this sound’s saved state was not changed."
                    .into()
            }
        }
    }
}

pub trait LibraryContextText {
    /// Show the captured selector and root independently of the current selection.
    fn description(&self) -> String;
}
impl LibraryContextText for LibraryContext {
    fn description(&self) -> String {
        self.selector.as_ref().map_or_else(
            || format!("Location: {}", self.path.display()),
            |key| format!("Selector: {key}\nLocation: {}", self.path.display()),
        )
    }
}

pub trait SaveFeedbackText {
    /// Explain an editor follow-up failure while retaining confirmed publication truth.
    fn description(&self) -> String;
}
impl SaveFeedbackText for SaveFeedback {
    fn description(&self) -> String {
        match self {
            Self::InvalidAtRate { rate, error } => format!(
                "Saved with validation errors at {} Hz: {error}. This finite document can be repaired later.",
                rate.hz(),
                error = error.explain()
            ),
            Self::EditorReset(error) => format!(
                "Trash confirmed, but a blank editor could not be created: {error}. Open draft retained; dependent action stopped.",
                error = error.explain()
            ),
            Self::EditorRefresh(error) => {
                format!(
                    "Saved, but the editor could not refresh: {error}. Open draft retained.",
                    error = error.explain()
                )
            }
        }
    }
}

pub trait InventoryStatusText {
    /// Report observed counts and completeness without treating unseen entries as absent.
    fn description(&self) -> String;
}
impl InventoryStatusText for InventoryStatus {
    fn description(&self) -> String {
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

pub trait FileActivityText {
    /// Describe the captured source and publication activity while its task occupies the slot.
    fn description(&self) -> String;
}
impl FileActivityText for FileActivity {
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
