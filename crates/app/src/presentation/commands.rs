//! Human-facing operation descriptions owned by their application data.
use super::{AudioSummary, DisplayText};
use crate::core::Timestamp;
use crate::core::vocabulary::Status;
use crate::discovery::collection::EntrySummary;
use crate::document::recipe::Rendition;
use crate::presentation::vocabulary::NumberStyle;
use crate::storage::library::{Initialization, Inventory, RootInfo};
use crate::storage::wav::Exported;
use std::path::Path;

fn date(value: Option<Timestamp>) -> String {
    value.map_or_else(|| Status::Unknown.label().into(), |value| value.to_string())
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
        if !matches!(self.key, crate::storage::library::LibraryKey::User(_)) {
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
impl RootInfo {
    /// Human-readable root inspection, without changing initialization state.
    #[must_use]
    pub fn description(&self) -> String {
        format!(
            "{}\nExists: {} · Initialized: {} · Source: {}\n",
            self.path.display().to_string().display_scalar(),
            self.exists,
            self.initialized,
            self.source
        )
    }
}
impl crate::services::library::Location {
    /// An inspection failure preserves what is known and marks initialization unverified.
    #[must_use]
    pub fn inspection_failure(
        &self,
        exists: Option<bool>,
        error: &crate::error::LibraryError,
    ) -> String {
        format!(
            "{}\nExists: {} · Initialized: unverified · Source: {}\n{}\n",
            self.path().display().to_string().display_scalar(),
            exists.map_or_else(|| Status::Unknown.label().into(), |value| value.to_string()),
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
            "Exporting {} to {} at {} Hz. Press Ctrl-C to stop.",
            self.snapshot().title(),
            path.display().to_string().display_scalar(),
            self.signal().sample_rate().hz()
        )
    }
}
impl Exported {
    /// Explain the actual completed WAV container and frame count.
    #[must_use]
    pub fn description(&self, path: &Path) -> String {
        format!(
            "Exported {} frames to {} ({}).",
            self.frames,
            path.display().to_string().display_scalar(),
            self.container
        )
    }
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

impl crate::storage::library::RootSource {
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

impl std::fmt::Display for crate::storage::library::RootSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}
