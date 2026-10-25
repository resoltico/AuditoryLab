//! Human-facing operation descriptions owned by their application data.
use super::{AudioSummary, DisplayText};
use crate::collection::{EntrySummary, Scope};
use crate::core::Timestamp;
use crate::library::{Initialization, Inventory, LibraryRoot, RootInfo, WriteOutcome};
use crate::recipe::Rendition;
use crate::vocabulary::{Caption, Collection, NumberStyle, Status};
use crate::wav::Exported;
use std::path::Path;

fn date(value: Option<Timestamp>) -> String {
    value.map_or_else(|| Status::Unknown.label().into(), |value| value.to_string())
}

impl Scope {
    /// Stable command-line collection value, independent of presentation wording.
    #[must_use]
    pub const fn cli_key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::BuiltIn => "builtin",
            Self::User => "user",
            Self::OpenFiles => "open-files",
            Self::Trash => "trash",
        }
    }
    /// Shared collection name used by controls and value help.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::All => Collection::AllBuiltInUser.label(),
            Self::BuiltIn => Collection::BuiltInReadOnly.label(),
            Self::User => Collection::User.label(),
            Self::OpenFiles => Collection::OpenFilesTemporary.label(),
            Self::Trash => Collection::Trash.label(),
        }
    }
}
impl EntrySummary {
    /// One complete discovery row, including explicit declared and membership dates.
    #[must_use]
    pub fn list_description(&self) -> String {
        format!(
            "{}\n  {}\n  {} · {}\n  Created {} · Edited {} · Added here {}\n",
            self.title.display_scalar(),
            self.key,
            self.nominal.summary(),
            NumberStyle::Duration.format(self.duration_seconds),
            date(self.created_at),
            date(self.modified_at),
            date(self.added_at)
        )
    }
    /// Membership details shown next to the selected portable description.
    #[must_use]
    pub fn membership_description(&self) -> String {
        if self.added_at.is_none() {
            return String::new();
        }
        format!(
            "Added here: {}\nTrash: {}\nVersion token: {}\n",
            date(self.added_at),
            date(self.trashed_at),
            self.version_token
                .as_ref()
                .map_or_else(|| Status::None.label().into(), ToString::to_string)
        )
    }
}
impl RootInfo {
    /// Human-readable root inspection, without changing initialization state.
    #[must_use]
    pub fn description(&self) -> String {
        format!(
            "{}\nExists: {} · Initialized: {} · Source: {}\n",
            self.path.display(),
            self.exists,
            self.initialized,
            self.source
        )
    }
}
impl LibraryRoot {
    /// An inspection failure preserves what is known and marks initialization unverified.
    #[must_use]
    pub fn inspection_failure(&self, exists: Option<bool>, error: &crate::error::Error) -> String {
        format!(
            "{}\nExists: {:?} · Initialized: unverified · Source: {}\n{}\n",
            self.path().display(),
            exists,
            self.source(),
            error
        )
    }
}
impl Initialization {
    /// Whether this request initialized the selected root or found it initialized.
    #[must_use]
    pub fn description(&self) -> String {
        format!(
            "Library {}: {}\n",
            if self.created {
                "initialized"
            } else {
                "already initialized"
            },
            self.path.display()
        )
    }
}
impl Inventory {
    /// Summary of a bounded storage inspection, without suppressing incompleteness.
    #[must_use]
    pub fn description(&self) -> String {
        format!(
            "Library check: {} managed records · complete={}\n",
            self.entries.len(),
            self.complete
        )
    }
}
impl Rendition {
    /// Explain the captured export and selected rate before writing.
    #[must_use]
    pub fn export_preamble(&self, path: &Path) -> String {
        format!(
            "Exporting {} to {path:?} at {} Hz. Press Ctrl-C to stop.",
            self.snapshot().title(),
            self.signal().sample_rate().hz()
        )
    }
}
impl Exported {
    /// Explain the actual completed WAV container and frame count.
    #[must_use]
    pub fn description(&self, path: &Path) -> String {
        format!(
            "Exported {} frames to {path:?} ({}).",
            self.frames, self.container
        )
    }
}
impl WriteOutcome {
    /// The authoritative publication outcome and any associated recovery information.
    #[must_use]
    pub fn description(&self, root: &Path) -> String {
        let mut text = format!("{}: {}", self.status.label(), self.receipt.key);
        for diagnostic in &self.diagnostics {
            text.push('\n');
            text.push_str(&diagnostic.display_text());
        }
        if !self.confirmed() {
            text.push('\n');
            text.push_str(&self.recovery_description(root));
        }
        text
    }
    /// Comparison values needed when the current publication protocol cannot confirm an outcome.
    #[must_use]
    pub fn recovery_description(&self, root: &Path) -> String {
        let receipt = &self.receipt;
        let previous = receipt
            .previous
            .as_ref()
            .map_or_else(|| "None (no previous record)".into(), ToString::to_string);
        let candidate = receipt.candidate_token.as_ref().map_or_else(
            || "None (purge has no candidate record)".into(),
            ToString::to_string,
        );
        let stage = receipt.owned_stage.as_ref().map_or_else(
            || Status::None.label().into(),
            |stage| stage.display_scalar(),
        );
        let root = root.display().to_string().display_scalar();
        format!(
            "Recovery receipt\nLibrary root: {root}\nOperation revision: {}\nSelector: {}\nPrevious version token: {previous}\nCandidate version token: {candidate}\nOwned stage: {stage}\nRecheck in this same library root with: auditorylab describe {} --format json\nCompare the observed version_token with this candidate/previous token before deciding another operation. Do not blindly retry. Absence alone does not attribute a purge.",
            receipt.operation_revision, receipt.key, receipt.key
        )
    }
    /// Completion output failure cannot change the already established publication truth.
    #[must_use]
    pub fn reporting_failure(&self, root: &Path, error: &std::io::Error) -> String {
        let mut report = self.description(root);
        if self.confirmed() {
            report.push('\n');
            report.push_str(&self.recovery_description(root));
        }
        let truth = if self.confirmed() {
            Status::OperationConfirmed
        } else {
            Status::PublicationUnconfirmed
        }
        .label();
        format!("{truth}\n{report}\nCompletion output failed: {error}")
    }
}
impl crate::reports::Report {
    /// A structured completion's notification failure preserves the report and receipt.
    #[must_use]
    pub fn reporting_failure(&self, error: &std::io::Error) -> String {
        let truth = if self.complete {
            Status::OperationConfirmed
        } else {
            Status::PublicationUnconfirmed
        }
        .label();
        let receipt = serde_json::to_string(&self.receipt)
            .unwrap_or_else(|error| format!("receipt rendering failed: {error}"));
        format!("{truth}; completion output failed: {error}; receipt {receipt}")
    }
}
impl Caption {
    /// Confirm completed export of the exact captured recipe text.
    #[must_use]
    pub fn recipe_exported(path: &Path) -> String {
        format!("Exported recipe to {path:?}.")
    }
    /// A storage stage retained for diagnosis.
    #[must_use]
    pub fn retained_stage(stage: &str) -> String {
        format!("Retained stage: {}\n", stage.display_scalar())
    }
}

/// A selectable snapshot of the information shown by the desktop diagnostics reader.
#[derive(Debug)]
pub struct DesktopDiagnostics<'a> {
    /// Captured transport identity and phase.
    pub player: &'a str,
    /// Captured generated voice details.
    pub generated: &'a str,
    /// Captured monitor provenance.
    pub monitor: &'a str,
    /// Actual output device information.
    pub device: &'a str,
    /// Current task/outcome description.
    pub status: &'a str,
    /// Failure to present an auxiliary window, if any.
    pub auxiliary_error: &'a str,
    /// Native motion preference explanation.
    pub motion: &'a str,
    /// Native typography origin and size.
    pub typography: &'a str,
    /// Static model provenance.
    pub model: &'a str,
}
impl DesktopDiagnostics<'_> {
    /// Complete selectable content; frontends render it without composing headings.
    #[must_use]
    pub fn text(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\nTypography: {}\n{}",
            self.player,
            self.generated,
            self.monitor,
            self.device,
            self.status,
            self.auxiliary_error,
            self.motion,
            self.typography,
            self.model
        )
    }
}

impl crate::library::MutationStatus {
    /// User-facing publication status without exposing an internal enum name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Published => Status::Saved.label(),
            Self::PublishedWithWarning => Status::SavedWithWarning.label(),
            Self::Unchanged => Status::Unchanged.label(),
            Self::PublicationUnconfirmed => Status::PublicationNotConfirmed.label(),
        }
    }
}

impl crate::library::RootSource {
    /// Name of the configuration source without exposing its internal enum spelling.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CommandLine => "Command line",
            Self::Environment => "Environment",
            Self::Default => "Native application data",
        }
    }
}

impl std::fmt::Display for crate::library::RootSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl crate::collection::SortBy {
    /// Shared name of the primary sort field.
    #[must_use]
    pub const fn label(self) -> &'static str {
        use crate::vocabulary::Facet;
        match self {
            Self::Automatic => Caption::AutomaticOrder.label(),
            Self::Rate => Caption::NominalRate.label(),
            Self::Title => Caption::Title.label(),
            Self::Created => Caption::Created.label(),
            Self::Modified => Caption::Modified.label(),
            Self::Added => Facet::AddedHere.label(),
            Self::Study => Caption::Study.label(),
            Self::Collection => Caption::Collection.label(),
        }
    }
}
impl crate::collection::Direction {
    /// Shared name of a primary sort direction.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Asc => Caption::Ascending.label(),
            Self::Desc => Caption::Descending.label(),
        }
    }
}
impl crate::collection::GroupBy {
    /// Shared name of the result grouping.
    #[must_use]
    pub const fn label(self) -> &'static str {
        use crate::vocabulary::Facet;
        match self {
            Self::None => Facet::NoGrouping.label(),
            Self::Band => Caption::BandGroups.label(),
            Self::Method => Caption::MethodGroups.label(),
            Self::Study => Caption::StudyGroups.label(),
            Self::Collection => Caption::CollectionGroups.label(),
        }
    }
}
