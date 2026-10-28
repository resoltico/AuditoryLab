//! Presentation number styles and operation wording.
use crate::DisplayText;

use auditorylab_app::vocabulary::Operation;
/// Incomplete discovery cannot establish an empty library.
pub const INVENTORY_INCOMPLETE: &str =
    "Incomplete inventory: valid results above do not establish an empty/complete library.";
/// No saved recipe matches the current query.
pub const EMPTY_FILTERED_LIBRARY: &str =
    "No recipes match the current filters. Clear the restrictions to see every recipe.";

/// Labels for Library playback and deliberate session replacement.
#[must_use]
pub const fn library_play_labels() -> [&'static str; 2] {
    [
        Operation::PlaySelectedSound.label(),
        Operation::StopCurrentAndPlayRecipe.label(),
    ]
}
/// Labels needed to size the transport across its states.
#[must_use]
pub const fn transport_labels() -> [&'static str; 3] {
    [
        Operation::PlaySelectedSound.label(),
        Operation::PlayLive.label(),
        Operation::Stop.label(),
    ]
}

/// Explanation for permanently deleting a trashed entry.
#[must_use]
pub fn purge_description(title: &str, key: &str) -> String {
    let title = (title).display_scalar();
    format!(
        "Delete “{title}”\n{key}\nThis permanently removes this trashed User entry. External files and independent copies remain unchanged."
    )
}
/// Save/discard prompt for the current document.
#[must_use]
pub fn unsaved_description(title: &str) -> String {
    format!("Save changes to “{title}” before continuing?")
}
/// Add-component label derived from its actual default role.
#[must_use]
pub fn add_component(kind: auditorylab_core::CarrierKind) -> String {
    format!(
        "Add {}",
        auditorylab_app::workspace::controls::default_component(kind)
            .heading()
            .to_lowercase()
    )
}

/// Add-pair wording rendered from the owning construction term.
#[must_use]
pub fn add_pair(kind: auditorylab_core::PairKind) -> String {
    format!("Add {}", kind.phrase())
}

/// Numeric display policy, separate from exact model values and input text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberStyle {
    /// Requested durations with meaningful units and explicit rounding.
    Duration,
    /// Whole elapsed transport time.
    Clock,
    /// Three significant digits for a waveform scale.
    AxisLimit,
    /// One-decimal digital level, with a silence symbol.
    Decibels,
    /// Compact significant digits with explicit approximation.
    Compact,
    /// Captured monitor window with two decimal places and its unit.
    CapturedMilliseconds,
    /// Available monitor time-window choices and their unit.
    Milliseconds,
    /// A sample-rate choice and its unit.
    Hertz,
}
