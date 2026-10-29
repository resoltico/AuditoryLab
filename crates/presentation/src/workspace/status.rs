//! Shared status and refusal sentences for interactive frontends.
use super::Explain;
use std::fmt::Write as _;

use auditorylab_app::outcome::Outcome;
use auditorylab_app::workspace::WorkerFailure;
use auditorylab_app::workspace::output::PreviewIssue;
use auditorylab_app::workspace::status::{
    AudioStatus, EditStatus, FileAction, FileActivity, FileNotice, FileStatus, InventoryStatus,
    LibraryContext, LibraryStatus, SaveFeedback,
};
use auditorylab_core::diagnostic::Code;

pub(crate) fn shutdown_reset_failure(
    error: &auditorylab_app::document::error::EditorError,
    context: &str,
) -> String {
    format!(
        "Trash confirmed; editor reset failed: {error}{context}",
        error = error.explain()
    )
}

pub fn output_issue_description(value: &auditorylab_app::workspace::output::OutputIssue) -> String {
    match value {
        auditorylab_app::workspace::output::OutputIssue::Capture(error) => {
            crate::capture_error_description(*(error))
        }
        auditorylab_app::workspace::output::OutputIssue::Preview(reason) => {
            crate::workspace::status::preview_issue_description(reason)
        }
    }
}

pub fn job_context_description(value: &auditorylab_app::workspace::JobContext) -> String {
    match value {
        auditorylab_app::workspace::JobContext::Library(context) => {
            format!(
                " · {}",
                crate::workspace::status::library_context_description(context)
            )
        }
        auditorylab_app::workspace::JobContext::File(path) => format!(" · {}", path.display()),
        auditorylab_app::workspace::JobContext::Audio(title) => format!(" · {title}"),
        auditorylab_app::workspace::JobContext::Unknown => String::new(),
    }
}

pub fn preview_issue_description(value: &PreviewIssue) -> String {
    match value {
        PreviewIssue::Snapshot(error) => error.explain(),
        PreviewIssue::Acoustic {
            title,
            rate,
            error: Some(error),
        } => crate::sound_error_at_rate(error, title, *rate),
        PreviewIssue::Acoustic {
            title,
            rate,
            error: None,
        } => format!("Cannot generate a preview of {title} at {} Hz.", rate.hz()),
    }
}

pub fn worker_failure_description(value: &WorkerFailure) -> String {
    match value {
        WorkerFailure::Launch(error) => {
            format!("Cannot start worker: {error}", error = error.explain())
        }
        WorkerFailure::Panicked => crate::WORKER_FAILED.into(),
        WorkerFailure::InvalidOutcome => {
            "The worker returned a result for another operation.".into()
        }
    }
}

pub fn audio_status_description(value: &AudioStatus) -> String {
    match value {
        AudioStatus::Idle => String::new(),
        AudioStatus::Refused(refusal) => crate::workspace::refusal_text(refusal),
        AudioStatus::LiveUpdate(error) => error.explain(),
        AudioStatus::Ended { report, monitored } => {
            crate::workspace::playback_text(report, *monitored)
        }
        AudioStatus::Failed(error) => error.explain(),
        AudioStatus::WorkerFailed(error) => {
            crate::workspace::status::worker_failure_description(error)
        }
    }
}

pub fn edit_status_description(value: &EditStatus) -> String {
    match value {
        EditStatus::Idle => String::new(),
        EditStatus::Refused(refusal) => crate::workspace::refusal_text(refusal),
    }
}

pub fn file_status_description(value: &FileStatus) -> String {
    match value {
        FileStatus::Idle => String::new(),
        FileStatus::Running(activity) => {
            crate::workspace::status::file_activity_description(activity)
        }
        FileStatus::Refused(refusal) => crate::workspace::refusal_text(refusal),
        FileStatus::Failed(error) => error.explain(),
        FileStatus::DialogFailed { cause } => cause.clone(),
        FileStatus::WorkerFailed { activity, error } => format!(
            "{} · {}{}",
            crate::workspace::status::worker_failure_description(error),
            activity.path.display(),
            if activity.may_publish && !matches!(error, WorkerFailure::Launch(_)) {
                "\nDraft \
            retained. Inspect the affected path before retrying; a failed worker \
            does not establish that storage was unchanged."
            } else {
                ""
            }
        ),
        FileStatus::Imported { snapshot, hidden } => format!(
            "Opened {}.{}",
            snapshot.title(),
            if *hidden {
                "\nHidden \
            by current filters; clear filters to show the imported recipe."
            } else {
                ""
            }
        ),
        FileStatus::RecipeExported { path, warnings } => format!(
            "Exported captured portable recipe to \
            {}.{}",
            path.display(),
            warnings.iter().fold(String::new(), |mut text, warning| {
                use std::fmt::Write as _;
                let _ = write!(text, "\n{}", crate::errors::publication::warning(warning));
                text
            })
        ),
        FileStatus::Exported { path, exported } => crate::workspace::exported_text(path, exported),
        FileStatus::Library(status) => crate::workspace::status::library_status_description(status),
        FileStatus::Notice(notice) => match notice {
            FileNotice::RetainedSource => {
                "Showing the retained source snapshot; filters explicitly \
                cleared. This does not reload or reconstruct an ancestor."
                    .into()
            }
            FileNotice::ChangedSelection => "Saved selection changed. Refresh before editing this \
                version."
                .into(),
            FileNotice::MissingSavedEntry => "The saved entry is no longer in the observed \
                inventory; refresh before locating it."
                .into(),
            FileNotice::SavedSnapshotStillDirty => crate::SAVED_SNAPSHOT_STILL_DIRTY.into(),
            FileNotice::ExportCancelled => crate::EXPORT_CANCELLED.into(),
        },
    }
}

/// A confirmed library write, named by the sound's title when it has one.
fn written_headline(
    key: &auditorylab_app::document::origin::LibraryKey,
    title: Option<&str>,
    status: auditorylab_app::services::library::MutationStatus,
    document: bool,
) -> String {
    use auditorylab_app::services::library::MutationStatus;
    let name = || title.map_or_else(|| key.to_string(), crate::display_scalar);
    match (document, title) {
        (true, Some(title)) if status == MutationStatus::Published => format!(
            "Saved “{}” to the User Library. Later edits stay separate until you save \
                again; use Show saved if filters hide it.",
            crate::display_scalar(title)
        ),
        (true, _) => format!(
            "{} · {}. Later edits stay separate until you save again; use Show saved if \
                filters hide it.",
            status.label(),
            name()
        ),
        (false, _) => format!("{} · {}", status.label(), name()),
    }
}

pub fn library_status_description(value: &LibraryStatus) -> String {
    match value {
        LibraryStatus::Inventory(status) => {
            crate::workspace::status::inventory_status_description(status)
        }
        LibraryStatus::Loaded => {
            "Saved version loaded; open draft and playing snapshot retained.".into()
        }
        LibraryStatus::Written {
            key,
            title,
            status,
            document,
            diagnostics,
            feedback,
        } => format!(
            "{}{}",
            written_headline(key, title.as_deref(), *status, *document),
            diagnostics
                .iter()
                .map(crate::errors::publication::warning)
                .chain(
                    feedback
                        .iter()
                        .map(crate::workspace::status::save_feedback_description)
                )
                .fold(String::new(), |mut text, detail| {
                    text.push('\n');
                    text.push_str(&detail);
                    text
                })
        ),
        LibraryStatus::ExternalWritten {
            status,
            diagnostics,
        } => format!(
            "{} · External file saved from captured draft.\n{}",
            status.label(),
            diagnostics
                .iter()
                .map(crate::errors::publication::warning)
                .collect::<Vec<_>>()
                .join("\n")
        ),
        LibraryStatus::Failed {
            error,
            publishing,
            context,
        } => {
            let recovery = "Draft retained. Refresh and reload the saved version, keep the draft, \
                or save an independent copy.";
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
                crate::workspace::status::library_context_description(context),
                error = error.explain()
            )
        }
        LibraryStatus::InvalidOwner => {
            "Saved content belongs to another sound; this sound’s saved state was not changed."
                .into()
        }
    }
}

pub fn library_context_description(value: &LibraryContext) -> String {
    value.selector.as_ref().map_or_else(
        || format!("Location: {}", value.path.display()),
        |key| format!("Selector: {key}\nLocation: {}", value.path.display()),
    )
}

pub fn save_feedback_description(value: &SaveFeedback) -> String {
    match value {
        SaveFeedback::InvalidAtRate { rate, error } => format!(
            "Saved with validation errors at {} Hz: {error}. This finite document \
                can be repaired later.",
            rate.hz(),
            error = error.explain()
        ),
        SaveFeedback::EditorReset(error) => format!(
            "Trash confirmed, but a blank editor could not be created: {error}. \
                Open draft retained; dependent action stopped.",
            error = error.explain()
        ),
        SaveFeedback::EditorRefresh(error) => {
            format!(
                "Saved, but the editor could not refresh: {error}. Open draft retained.",
                error = error.explain()
            )
        }
    }
}

pub fn inventory_status_description(value: &InventoryStatus) -> String {
    let mut text = format!(
        "{} · {} readable User entries{}",
        if value.complete {
            "Inventory complete"
        } else {
            "Inventory incomplete · unseen previous rows retained; refresh before acting"
        },
        value.count,
        if value.not_initialized {
            " · Not initialized"
        } else {
            ""
        }
    );
    for problem in &value.diagnostics {
        let _ = write!(
            text,
            "\n{}: {}",
            problem.code(),
            crate::errors::library_diagnostic_message(problem)
        );
    }
    if !value.stages.is_empty() {
        let _ = write!(
            text,
            "\n{} staging artifacts retained for recovery.",
            value.stages.len()
        );
    }
    text
}

pub fn file_activity_description(value: &FileActivity) -> String {
    let operation = value.kind.operation().label();
    match value.kind {
        FileAction::Import | FileAction::RecipeExport => {
            format!("{operation}: {}.", value.path.display())
        }
        FileAction::Library(_) => format!("{operation} · persistent library."),
        FileAction::AudioExport => format!(
            "{operation}: {} to {}.",
            value.title.as_deref().unwrap_or("sound"),
            value.path.display()
        ),
    }
}
