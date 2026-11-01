//! Completed library/operations descriptions.
use crate::Explain;
use crate::numbers::NumberStyle;
use auditorylab_app::discovery::collection::EntrySummary;
use auditorylab_app::services::library::Initialization;
use auditorylab_app::services::library::Inventory;
use auditorylab_app::services::library::RootInfo;
use auditorylab_app::vocabulary::Status;
use auditorylab_core::Timestamp;
use std::fmt::Write as _;
fn date(value: Option<Timestamp>) -> String {
    value.map_or_else(|| Status::Unknown.label().into(), |value| value.to_string())
}

/// Name of the configuration source without exposing its internal enum spelling.
pub fn root_source_label(value: auditorylab_app::services::library::RootSource) -> &'static str {
    match value {
        auditorylab_app::services::library::RootSource::CommandLine => "Command line",
        auditorylab_app::services::library::RootSource::Environment => "Environment",
        auditorylab_app::services::library::RootSource::Default => "Native application data",
    }
}

/// Summary of a bounded storage inspection, without suppressing incompleteness.
pub fn inventory_description(value: &Inventory) -> String {
    format!(
        "Library check: {} managed records · complete={}\n",
        value.entries.len(),
        value.complete
    )
}

/// Whether this request initialized the selected root or found it initialized.
pub fn initialization_description(value: &Initialization) -> String {
    format!(
        "Library {}: {}\n",
        if value.created {
            "initialized"
        } else {
            "already initialized"
        },
        value.path.display()
    )
}

/// An inspection failure preserves what is known and marks initialization unverified.
pub fn location_inspection_failure(
    value: &auditorylab_app::services::library::Location,
    exists: Option<bool>,
    error: &auditorylab_app::services::library::LibraryError,
) -> String {
    format!(
        "{}\nExists: {} · Initialized: unverified · Source: {}\n{}\n",
        crate::display_scalar(&(value.path().display().to_string())),
        exists.map_or_else(|| Status::Unknown.label().into(), |value| value.to_string()),
        crate::library::root_source_label(value.source()),
        error.explain()
    )
}

/// Human-readable root inspection, without changing initialization state.
pub fn root_info_description(value: &RootInfo) -> String {
    format!(
        "{}\nExists: {} · Initialized: {} · Source: {}\n",
        crate::display_scalar(&(value.path.display().to_string())),
        value.exists,
        value.initialized,
        crate::library::root_source_label(value.source)
    )
}

/// One discovery row: title, selector, rhythm, duration and purpose. Built-in rows
/// show dates only while the listing sorts by a date, as the desktop does.
pub fn entry_summary_list_description(
    value: &EntrySummary,
    sort: auditorylab_app::discovery::query::SortBy,
) -> String {
    let mut text = format!(
        "{}\n  {}\n  {} · {}\n  {}\n",
        crate::display_scalar(&(value.title)),
        value.key,
        crate::analysis_summary(&(value.nominal)),
        crate::numbers::format(NumberStyle::Duration, value.duration_seconds),
        crate::display_scalar(&crate::library::entry_studied_for(value))
    );
    if !matches!(
        value.key,
        auditorylab_app::document::origin::LibraryKey::BuiltIn(_)
    ) || sort.calendar_field().is_some()
    {
        let _ = writeln!(
            text,
            "  Created {} · Edited {} · Added here {}",
            date(value.created_at),
            date(value.modified_at),
            date(value.added_at)
        );
    }
    text
}

/// Membership details shown next to the selected portable description.
pub fn entry_summary_membership_description(value: &EntrySummary) -> String {
    if !matches!(
        value.key,
        auditorylab_app::document::origin::LibraryKey::User(_)
    ) {
        return String::new();
    }
    format!(
        "Added here: {}\nTrash: {}\nVersion token: {}\n",
        date(value.added_at),
        if value.trashed {
            date(value.trashed_at)
        } else {
            Status::None.label().into()
        },
        value
            .version_token
            .as_ref()
            .map_or_else(|| Status::None.label().into(), ToString::to_string)
    )
}

/// Incomplete discovery that found nothing cannot establish an empty library.
pub const INVENTORY_UNREADABLE: &str = "Incomplete inventory: the library could not be fully \
    read, so no results does not mean an empty library.";
/// Incomplete discovery cannot establish an empty library.
pub const INVENTORY_INCOMPLETE: &str =
    "Incomplete inventory: valid results above do not establish an empty/complete library.";

/// No saved recipe matches the current query.
pub const EMPTY_FILTERED_LIBRARY: &str =
    "No recipes match the current filters. Clear the restrictions to see every recipe.";
