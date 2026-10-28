//! Shared collection rows, query descriptions and document-location wording.
use super::workspace_text::{LibraryFacetText, LibraryRowText};
use crate::AudioSummary;
use crate::DisplayText;
use crate::Explain;
use crate::vocabulary::NumberStyle;
use crate::*;
use auditorylab_app::discovery::collection::CatalogQuery;
use auditorylab_app::discovery::collection::EntrySummary;
use auditorylab_app::discovery::query::{CalendarField, RhythmState, Scope};
use auditorylab_app::document::origin::LibraryKey;
use auditorylab_app::document::recipe::RecipeSnapshot;
use auditorylab_app::services::library::LibraryDiagnostic;
use auditorylab_app::vocabulary::Status;
/// Render a semantic row, without executing queries or selecting recipe ownership.
pub fn row(
    row: &auditorylab_app::workspace::LibraryRowView,
    sort: auditorylab_app::discovery::query::SortBy,
) -> LibraryRowText {
    use auditorylab_app::workspace::LibraryRowView as Row;
    match row {
        Row::Entry {
            entry,
            requires_read,
            ..
        } => entry_row(entry, sort, *requires_read),
        Row::Group(group) => heading_row(format!(
            "{} · {} unique sounds",
            group_label(&group.label),
            group.keys.len()
        )),
        Row::Unreadable => {
            heading_row("Unreadable storage entries · not included in sound counts".into())
        }
        Row::Diagnostic(problem) => heading_row(format!(
            "Needs review · {}\n{}",
            problem.storage_id,
            problem.message()
        )),
    }
}
fn entry_row(
    e: &EntrySummary,
    sort: auditorylab_app::discovery::query::SortBy,
    requires_read: bool,
) -> LibraryRowText {
    use auditorylab_app::discovery::query::SortBy;
    let (date_label, date) = match sort {
        SortBy::Created => (CalendarField::Created.label(), e.created_at),
        SortBy::Modified => (auditorylab_core::vocabulary::EDITED.label, e.modified_at),
        SortBy::Added => (CalendarField::Added.label(), e.added_at),
        _ if matches!(e.key, LibraryKey::User(_) | LibraryKey::External(_)) => {
            (auditorylab_core::vocabulary::EDITED.label, e.modified_at)
        }
        _ => (CalendarField::Created.label(), e.created_at),
    };
    let shown = |date: Option<auditorylab_core::Timestamp>| {
        date.map_or_else(
            || Status::Unknown.label().into(),
            crate::calendar_text::local_timestamp,
        )
    };
    let mut dates = format!("{date_label}: {}", shown(date));
    if matches!(e.key, LibraryKey::User(_)) {
        if date_label != auditorylab_core::vocabulary::EDITED.label {
            dates.push_str(&format!(
                "\n{}: {}",
                auditorylab_core::vocabulary::EDITED.label,
                shown(e.modified_at)
            ));
        }
        if e.modified_at.is_none() && date_label != CalendarField::Added.label() {
            dates.push_str(&format!(
                "\n{}: {}",
                CalendarField::Added.label(),
                shown(e.added_at)
            ));
        }
    }
    let collection = format!(
        "{}{}",
        match e.key {
            LibraryKey::BuiltIn(_) => Scope::BuiltIn.label(),
            LibraryKey::User(_) if e.trashed => Scope::Trash.label(),
            LibraryKey::User(_) => Scope::User.label(),
            LibraryKey::External(_) => "Open file · Temporary",
        },
        if requires_read {
            " · Last observed · Needs review"
        } else {
            ""
        }
    );
    let title = e.title.display_scalar();
    let summary = e.nominal.summary();
    let duration = format!(
        "Duration {}",
        NumberStyle::Duration.format(e.duration_seconds)
    );
    let mut memberships: Vec<String> = e.nominal.bands.iter().map(ToString::to_string).collect();
    if e.nominal.outside_bands() {
        memberships.push(RhythmState::OutsideBands.label().into());
    }
    if e.nominal.no_rhythm() {
        memberships.push(RhythmState::None.label().into());
    }
    if e.nominal.multiple() {
        memberships.push(RhythmState::Multiple.label().into());
    }
    if e.nominal.completeness != auditorylab_core::rhythm::Completeness::Complete {
        memberships.push(RhythmState::NeedsReview.label().into());
    }
    let bands = memberships.join(" · ");
    LibraryRowText {
        key: e.key.to_string(),
        text: format!("{title} · {collection}\n{summary}\n{bands}\n{duration}\n{dates}"),
        title,
        collection,
        summary,
        bands,
        duration,
        dates,
    }
}
fn heading_row(text: String) -> LibraryRowText {
    LibraryRowText {
        key: String::new(),
        title: text.clone(),
        text,
        collection: String::new(),
        summary: String::new(),
        bands: String::new(),
        duration: String::new(),
        dates: String::new(),
    }
}
/// Render already-counted navigation memberships.
#[must_use]
pub fn facets(
    counts: &auditorylab_app::workspace::FacetCounts,
) -> (Vec<LibraryFacetText>, Vec<String>) {
    (
        counts.bands.iter().map(facet).collect(),
        counts.methods.iter().map(|f| facet(f).text).collect(),
    )
}
/// Render one counted navigation membership without assigning its control position.
#[must_use]
pub fn facet(facet: &auditorylab_app::workspace::LibraryFacetView) -> LibraryFacetText {
    use auditorylab_app::workspace::FacetKind;

    let (label, detail) = match facet.kind {
        FacetKind::AllSounds => (auditorylab_app::vocabulary::ALL_SOUNDS.label.into(), ""),
        FacetKind::AllMethods => (auditorylab_app::vocabulary::ALL_METHODS.label.into(), ""),
        FacetKind::Band(band) => (band.to_string(), band.interval()),
        FacetKind::State(state) => (state.label().into(), ""),
        FacetKind::Method(method) => (method.label().into(), ""),
    };
    let count = if facet.complete {
        facet.count.to_string()
    } else {
        format!("{} observed", facet.count)
    };
    let text = if detail.is_empty() {
        format!("{label} · {count}")
    } else {
        format!("{label} · {detail} · {count}")
    };
    LibraryFacetText {
        label,
        detail: detail.into(),
        count,
        text,
    }
}
pub(crate) fn overview(query: &CatalogQuery, visible: usize, complete: bool) -> String {
    let q = query;
    let sounds = if visible == 1 {
        "unique sound"
    } else {
        "unique sounds"
    };
    let count = if complete {
        format!("{} {sounds}", visible)
    } else {
        format!("{} observed {sounds} · Inventory incomplete", visible)
    };
    if *q != q.cleared_filters() {
        format!("{count} · Filters active")
    } else {
        count
    }
}
pub(crate) fn count(query: &CatalogQuery, visible: usize, complete: bool) -> String {
    {
        let q = query;
        let (sort, direction) = q.effective_sort();
        let mut lines = vec![format!(
            "{} · {} · Sort {} {} · Group {}",
            overview(query, visible, complete),
            q.scope.label(),
            sort.label(),
            direction.label(),
            q.group_by.label()
        )];
        let mut restrictions = Vec::new();
        if !q.search.is_empty() {
            restrictions.push(format!("Search: {}", q.search.display_scalar()));
        }
        if q.include_external {
            restrictions.push("Include temporary Open files".into());
        }
        for band in &q.rhythm.bands {
            restrictions.push(format!("Band: {band} · {}", band.interval()));
        }
        for method in &q.rhythm.methods {
            restrictions.push(format!("Method: {}", method.label()));
        }
        if let Some(state) = q.rhythm.state {
            let label = state.label();
            restrictions.push(format!("Rhythm state: {label}"));
        }
        for study in &q.studies {
            restrictions.push(format!("Study: {}", (study).display_scalar()));
        }
        for population in &q.populations {
            restrictions.push(format!("Population: {}", population.label()));
        }
        for role in &q.roles {
            restrictions.push(format!("Condition role: {}", role.label()));
        }
        if let Some(analysis) = q.analysis {
            restrictions.push(format!("Analysis: {}", analysis.label()));
        }
        for (name, range) in [
            (CalendarField::Created.label(), &q.created),
            (auditorylab_core::vocabulary::EDITED.label, &q.modified),
            (CalendarField::Added.label(), &q.added),
        ] {
            for (label, value) in [
                ("from inclusive", range.from),
                ("before exclusive", range.before),
            ] {
                if let Some(value) = value {
                    restrictions.push(format!(
                        "{name} {label}: {} · UTC {value}",
                        crate::calendar_text::local_timestamp(value)
                    ));
                }
            }
        }
        if restrictions.is_empty() {
            lines.push("Applied filters: None".into());
        } else {
            lines.push("Applied filters (committed query):".into());
            lines.extend(restrictions);
        }
        lines.join("\n")
    }
}
pub(crate) fn empty_message(
    query: &CatalogQuery,
    entries: &[EntrySummary],
    visible: usize,
    complete: bool,
) -> &'static str {
    if visible > 0 {
        return "";
    }
    if !matches!(query.scope, Scope::BuiltIn | Scope::OpenFiles) && !complete {
        return crate::vocabulary::INVENTORY_INCOMPLETE;
    }
    if query.cleared_filters().count(entries).unwrap_or(0) > 0 {
        return crate::vocabulary::EMPTY_FILTERED_LIBRARY;
    }
    match query.scope {
        Scope::User => {
            "Your User Library is empty. Create a sound or edit a Built-in copy to start your collection."
        }
        Scope::OpenFiles => "No open files. Open a portable recipe to use it for this session.",
        Scope::Trash => "Trash is empty. Deleted User sounds appear here.",
        Scope::All | Scope::BuiltIn => {
            "No sounds in this collection. Open a recipe or create a sound."
        }
    }
}
pub(crate) fn alert(
    problem: Option<&LibraryDiagnostic>,
    filter_error: Option<&auditorylab_app::discovery::query::QueryError>,
    read_failure: Option<&LibraryDiagnostic<auditorylab_app::workspace::TaskFailure>>,
    complete: bool,
) -> String {
    let mut notices = Vec::new();
    if let Some(problem) = problem {
        notices.push(format!("User Library unavailable: {}. Configure an absolute AUDITORYLAB_LIBRARY_DIR and restart; drafts are retained.",problem.message()));
    }
    if let Some(error) = filter_error {
        notices.push(error.explain());
    }
    if let Some(problem) = read_failure {
        notices.push(format!(
            "Library read failed · {}: {}",
            problem.storage_id,
            problem.message()
        ));
    }
    if !complete {
        notices.push("Library inventory incomplete · previous unseen rows are last observed snapshots. Refresh before acting; no empty-library claim is implied.".into());
    }
    notices.join("\n")
}
pub(crate) fn dates(
    entry: Option<&EntrySummary>,
    snapshot: Option<&RecipeSnapshot>,
    utc: bool,
) -> String {
    entry.map_or(String::new(), |e| {
        let shown = |date: Option<auditorylab_core::Timestamp>| {
            date.map_or_else(
                || Status::Unknown.label().into(),
                |d| {
                    if utc {
                        d.to_string()
                    } else {
                        crate::calendar_text::local_timestamp(d)
                    }
                },
            )
        };
        format!(
            "Created: {}\nModified: {}\nAdded here: {}{}{}",
            shown(e.created_at),
            shown(e.modified_at),
            if matches!(e.key, LibraryKey::User(_)) {
                shown(e.added_at)
            } else {
                auditorylab_core::vocabulary::NOT_APPLICABLE.label.into()
            },
            e.trashed_at.map_or(String::new(), |d| format!(
                "\nTrashed here: {}",
                shown(Some(d))
            )),
            snapshot.map_or(String::new(), |s| {
                auditorylab_app::services::history_warnings(s.recipe())
                    .iter()
                    .map(|w| format!("\n{w}"))
                    .collect::<String>()
            })
        )
    })
}
pub(crate) fn document_location(
    problem: Option<&LibraryDiagnostic>,
    detached: Option<&LibraryKey>,
    external: Option<&auditorylab_app::document::origin::ObservedFile>,
    managed: Option<(&LibraryKey, auditorylab_app::document::origin::VersionToken)>,
) -> (&'static str, String) {
    if let Some(error) = problem {
        return (
            "User Library unavailable",
            format!("Unsaved · User Library unavailable: {}", error.message()),
        );
    }
    if let Some(key) = detached {
        return (
            "Detached draft · save an independent copy or restore/edit saved entry",
            format!(
                "Detached draft · {key} · restore/edit the saved version or Save independent copy"
            ),
        );
    }
    if let Some(external) = external {
        return (
            "External file",
            format!(
                "External file · {} · observed {}",
                external.path().display(),
                external.fingerprint()
            ),
        );
    }
    if let Some((key, token)) = managed {
        return ("User Library", format!("User Library · {key} · {token}"));
    }
    (
        "Unsaved · Save to User Library",
        "Unsaved · Save to User Library".into(),
    )
}

pub(crate) const MODIFICATION_PRECEDES_CREATION: &str =
    "declared modification precedes creation; history was not clamped.";

pub(crate) const CLOCK_PREDATES_HISTORY: &str =
    "observed clock is earlier than prior history; exact bytes remain the conflict authority.";

pub(crate) const FUTURE_HISTORY: &str = "declared history lies in the future; it was preserved.";

pub(crate) const UNKNOWN_ROOT_ENTRY: &str = "unknown root entry retained";

pub(crate) const UNKNOWN_ENTRY: &str = "unknown entry retained";

pub(crate) const UUID_FILENAME_REQUIRED: &str = "expected a canonical UUID.toml filename";

pub(crate) const RESERVED_BUILTIN_IDENTITY: &str = "managed file uses a reserved built-in UUID";

pub(crate) const INTERRUPTED_MOVE: &str =
    "same-file links in both directories; Trash is authoritative until relocation completes";

pub(crate) const DUPLICATE_IDENTITY: &str =
    "different files with the same UUID exist in recipes and trash; both were retained";

pub(crate) fn orphan_membership(id: auditorylab_core::DocumentUuid) -> String {
    format!("membership for {id} has no recipe; the next locked write will drop it")
}
pub(crate) fn unavailable_history_clock(error: &auditorylab_app::clock::ClockError) -> String {
    format!(
        "Cannot compare declared history with the observed clock: {}",
        error.explain()
    )
}

pub(crate) fn removed_memberships(removed: usize) -> String {
    format!("Removed {removed} orphan membership timestamp rows")
}
pub(crate) fn membership_update_failed(
    error: &auditorylab_app::services::library::LibraryError,
) -> String {
    format!("Saved; membership timestamps could not be updated: {error}")
}
pub(crate) fn membership_read_failed(
    error: &auditorylab_app::services::library::LibraryError,
) -> String {
    format!("Saved; membership dates are Unknown because their state could not be read: {error}")
}
pub(crate) fn recovered_move(name: &std::ffi::OsStr) -> String {
    format!(
        "Recovered interrupted move for {}; Trash remains authoritative",
        name.to_string_lossy()
    )
}
pub(crate) fn removed_stage(path: &std::path::Path) -> String {
    format!("Removed abandoned stage {}", path.display())
}

pub(crate) fn inventory_notice(
    notice: &auditorylab_app::services::library::InventoryNotice,
) -> String {
    use auditorylab_app::services::library::InventoryNotice as N;
    match notice {
        N::UnknownRootEntry => UNKNOWN_ROOT_ENTRY.into(),
        N::UnknownEntry => UNKNOWN_ENTRY.into(),
        N::UuidFilenameRequired => UUID_FILENAME_REQUIRED.into(),
        N::ReservedBuiltinIdentity => RESERVED_BUILTIN_IDENTITY.into(),
        N::InterruptedMove => INTERRUPTED_MOVE.into(),
        N::DuplicateIdentity => DUPLICATE_IDENTITY.into(),
        N::OrphanMembership(id) => orphan_membership(*id),
    }
}

/// Human library and document-location projections over typed coordinator data.
pub trait LibraryPresentation {
    /// Configured root and retained inventory causes.
    fn library_status(&self) -> String;
    /// Current availability and incomplete-read notices.
    fn library_alert(&self) -> String;
    /// Query restrictions and unique result count.
    fn library_overview(&self) -> String;
    /// Detailed unique counts and active restrictions.
    fn library_count(&self) -> String;
    /// Explanation of an empty result under the current completeness state.
    fn library_empty_message(&self) -> &'static str;
    /// Portable and local selected dates in the system time zone.
    fn library_dates(&self) -> String;
    /// Exact UTC instants for selected portable and local dates.
    fn library_dates_utc(&self) -> String;
    /// Current document location, independent of browsing and playback.
    fn document_location(&self) -> String;
    /// Short badge for the current document location.
    fn document_location_label(&self) -> &'static str;
}
impl LibraryPresentation for auditorylab_app::workspace::Workspace {
    fn library_status(&self) -> String {
        let d = self.library_data();
        match d.path {
            Some(path) => format!(
                "{}\n{}{}",
                path.display(),
                d.inventory.description(),
                d.read_failure.map_or(String::new(), |problem| format!(
                    "\n{}: {}",
                    problem.storage_id,
                    problem.message()
                ))
            ),
            None => d
                .problem
                .map(LibraryDiagnostic::message)
                .unwrap_or_default(),
        }
    }
    fn library_alert(&self) -> String {
        let d = self.library_data();
        alert(
            d.problem,
            d.filter_error,
            d.read_failure,
            d.inventory.complete,
        )
    }
    fn library_overview(&self) -> String {
        let d = self.library_data();
        overview(d.query, d.visible, d.inventory.complete)
    }
    fn library_count(&self) -> String {
        let d = self.library_data();
        count(d.query, d.visible, d.inventory.complete)
    }
    fn library_empty_message(&self) -> &'static str {
        let d = self.library_data();
        empty_message(d.query, d.entries, d.visible, d.inventory.complete)
    }
    fn library_dates(&self) -> String {
        let d = self.library_data();
        dates(d.selected_entry, d.selected, false)
    }
    fn library_dates_utc(&self) -> String {
        let d = self.library_data();
        dates(d.selected_entry, d.selected, true)
    }
    fn document_location(&self) -> String {
        let d = self.library_data();
        document_location(d.problem, d.detached, d.external, d.managed).1
    }
    fn document_location_label(&self) -> &'static str {
        let d = self.library_data();
        document_location(d.problem, d.detached, d.external, d.managed).0
    }
}

/// Label for a virtual group membership, independent of its result keys.
#[must_use]
pub fn group_label(label: &auditorylab_app::discovery::collection::GroupLabel) -> String {
    use auditorylab_app::discovery::collection::Collection;
    use auditorylab_app::discovery::collection::GroupLabel as L;
    match label {
        L::Band(band) => band.to_string(),
        L::State(state) => state.label().into(),
        L::Method(method) => method.label().into(),
        L::NoMethod => "No declared method".into(),
        L::Study(label) => label.clone(),
        L::NoStudy => "No current study declaration".into(),
        L::Collection(collection) => match collection {
            Collection::BuiltIn => "Built-in",
            Collection::User => Scope::User.label(),
            Collection::External => "Open files",
        }
        .into(),
    }
}

pub trait WriteOutcomeText {
    /// Concise confirmed publication result with an unwrapped comparison token.
    #[must_use]
    fn description(&self) -> String;

    /// Confirmed truth retained when completion output fails.
    #[must_use]
    fn completion_failure(&self, error: impl std::fmt::Display) -> String;
}
impl WriteOutcomeText for auditorylab_app::services::library::WriteOutcome {
    /// Concise confirmed publication result with an unwrapped comparison token.
    fn description(&self) -> String {
        let mut text = format!("{}: {}", self.status.label(), self.key);
        if let Some(record) = &self.record {
            text.push_str(&format!("\nVersion token: {}", record.version_token));
        }
        for diagnostic in &self.diagnostics {
            text.push('\n');
            text.push_str(&super::publication_text::warning(diagnostic));
        }
        text
    }

    /// Confirmed truth retained when completion output fails.
    fn completion_failure(&self, error: impl std::fmt::Display) -> String {
        format!(
            "Operation confirmed\n{}\nCompletion output failed: {error}",
            self.description()
        )
    }
}

/// Encoded selection fields for frontends whose binding properties are strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionBindingText {
    /// Canonical runtime key, empty only when no indexed selection exists.
    pub key: String,
    /// Canonical content token, empty when no readable managed token exists.
    pub token: String,
    /// Directory-owned state; false when no indexed selection exists.
    pub trashed: bool,
}
/// Encode one already-selected identity without choosing or admitting an action.
#[must_use]
pub fn selection_binding(
    binding: Option<auditorylab_app::workspace::SelectionBinding<'_>>,
) -> SelectionBindingText {
    binding.map_or_else(
        || SelectionBindingText {
            key: String::new(),
            token: String::new(),
            trashed: false,
        },
        |binding| SelectionBindingText {
            key: binding.key.to_string(),
            token: binding
                .version_token
                .map(|token| token.to_string())
                .unwrap_or_default(),
            trashed: binding.trashed,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_inventory_and_read_failures_stay_visible() {
        use auditorylab_app::services::library::DiagnosticDetail;
        use auditorylab_app::workspace::{TaskFailure, WorkerFailure};
        let problem = LibraryDiagnostic {
            storage_id: "captured entry".into(),

            detail: DiagnosticDetail::Failure(Box::new(TaskFailure::Worker(
                WorkerFailure::InvalidOutcome,
            ))),
        };
        assert!(alert(None, None, Some(&problem), false).contains("read failed"));
        assert!(
            auditorylab_app::workspace::status::InventoryStatus::default()
                .description()
                .contains("incomplete")
        );
    }
}
