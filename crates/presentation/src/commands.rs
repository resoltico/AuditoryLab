//! Human-facing operation descriptions owned by their application data.
use super::{AudioSummary, DisplayText};
use crate::vocabulary::NumberStyle;
use crate::*;
use auditorylab_app::discovery::collection::EntrySummary;
use auditorylab_app::services::Exported;
use auditorylab_app::services::library::Initialization;
use auditorylab_app::services::library::Inventory;
use auditorylab_app::services::library::RootInfo;
use auditorylab_app::vocabulary::Status;
use auditorylab_core::Timestamp;
use std::path::Path;

fn date(value: Option<Timestamp>) -> String {
    value.map_or_else(|| Status::Unknown.label().into(), |value| value.to_string())
}

/// Confirm completed export of the exact captured recipe text.
#[must_use]
pub fn recipe_exported(path: &Path) -> String {
    format!(
        "Exported recipe to {}.",
        path.display().to_string().display_scalar()
    )
}
/// A storage stage retained for diagnosis.
#[must_use]
pub fn retained_stage(stage: &str) -> String {
    format!("Retained stage: {}\n", stage.display_scalar())
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

pub trait RootSourceText {
    /// Name of the configuration source without exposing its internal enum spelling.
    #[must_use]
    fn label(self) -> &'static str;
}
impl RootSourceText for auditorylab_app::services::library::RootSource {
    /// Name of the configuration source without exposing its internal enum spelling.
    fn label(self) -> &'static str {
        match self {
            Self::CommandLine => "Command line",
            Self::Environment => "Environment",
            Self::Default => "Native application data",
        }
    }
}

pub trait ExportedText {
    /// Explain the actual completed WAV container and frame count.
    #[must_use]
    fn description(&self, path: &Path) -> String;
}
impl ExportedText for Exported {
    /// Explain the actual completed WAV container and frame count.
    fn description(&self, path: &Path) -> String {
        format!(
            "Exported {} frames to {} ({}).",
            self.frames,
            path.display().to_string().display_scalar(),
            self.container
        )
    }
}

pub trait InventoryText {
    /// Summary of a bounded storage inspection, without suppressing incompleteness.
    #[must_use]
    fn description(&self) -> String;
}
impl InventoryText for Inventory {
    /// Summary of a bounded storage inspection, without suppressing incompleteness.
    fn description(&self) -> String {
        format!(
            "Library check: {} managed records · complete={}\n",
            self.entries.len(),
            self.complete
        )
    }
}

pub trait InitializationText {
    /// Whether this request initialized the selected root or found it initialized.
    #[must_use]
    fn description(&self) -> String;
}
impl InitializationText for Initialization {
    /// Whether this request initialized the selected root or found it initialized.
    fn description(&self) -> String {
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

pub trait LibraryLocationText {
    /// An inspection failure preserves what is known and marks initialization unverified.
    #[must_use]
    fn inspection_failure(
        &self,
        exists: Option<bool>,
        error: &auditorylab_app::services::library::LibraryError,
    ) -> String;
}
impl LibraryLocationText for auditorylab_app::services::library::Location {
    /// An inspection failure preserves what is known and marks initialization unverified.
    fn inspection_failure(
        &self,
        exists: Option<bool>,
        error: &auditorylab_app::services::library::LibraryError,
    ) -> String {
        format!(
            "{}\nExists: {} · Initialized: unverified · Source: {}\n{}\n",
            self.path().display().to_string().display_scalar(),
            exists.map_or_else(|| Status::Unknown.label().into(), |value| value.to_string()),
            self.source().label(),
            error.explain()
        )
    }
}

pub trait RootInfoText {
    /// Human-readable root inspection, without changing initialization state.
    #[must_use]
    fn description(&self) -> String;
}
impl RootInfoText for RootInfo {
    /// Human-readable root inspection, without changing initialization state.
    fn description(&self) -> String {
        format!(
            "{}\nExists: {} · Initialized: {} · Source: {}\n",
            self.path.display().to_string().display_scalar(),
            self.exists,
            self.initialized,
            self.source.label()
        )
    }
}

pub trait EntrySummaryText {
    /// One complete discovery row, including explicit declared and membership dates.
    #[must_use]
    fn list_description(&self) -> String;

    /// Membership details shown next to the selected portable description.
    #[must_use]
    fn membership_description(&self) -> String;
}
impl EntrySummaryText for EntrySummary {
    /// One complete discovery row, including explicit declared and membership dates.
    fn list_description(&self) -> String {
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
    fn membership_description(&self) -> String {
        if !matches!(
            self.key,
            auditorylab_app::document::origin::LibraryKey::User(_)
        ) {
            return String::new();
        }
        format!(
            "Added here: {}\nTrash: {}\nVersion token: {}\n",
            date(self.added_at),
            if self.trashed {
                date(self.trashed_at)
            } else {
                Status::None.label().into()
            },
            self.version_token
                .as_ref()
                .map_or_else(|| Status::None.label().into(), ToString::to_string)
        )
    }
}
