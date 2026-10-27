//! Shared collection rows, query descriptions and document-location wording.
use crate::discovery::collection::{CatalogQuery, EntrySummary, Scope};
use crate::document::recipe::RecipeSnapshot;
use crate::presentation::{AudioSummary, DisplayText, vocabulary::NumberStyle};
use crate::storage::library::{LibraryDiagnostic, LibraryKey};
use crate::workspace::{LibraryFacetView, LibraryRowView};
pub(crate) fn rows(
    query: &CatalogQuery,
    entries: &[EntrySummary],
    diagnostics: &[LibraryDiagnostic],
    selection: Option<&LibraryKey>,
    requires_read: impl Fn(&LibraryKey) -> bool,
) -> (Vec<LibraryRowView>, Option<usize>) {
    let label = |e: &EntrySummary| {
        use crate::discovery::collection::SortBy;
        let sort = query.effective_sort().0;
        let (date_label, date) = match sort {
            SortBy::Created => ("Created", e.created_at),
            SortBy::Modified => ("Edited", e.modified_at),
            SortBy::Added => ("Added here", e.added_at),
            _ if matches!(e.key, LibraryKey::User(_) | LibraryKey::External(_)) => {
                ("Edited", e.modified_at)
            }
            _ => ("Created", e.created_at),
        };
        let shown = |date: Option<crate::core::Timestamp>| {
            date.map_or_else(
                || "Unknown".into(),
                crate::discovery::calendar::local_timestamp,
            )
        };
        let mut dates = format!("{date_label}: {}", shown(date));
        if matches!(e.key, LibraryKey::User(_)) {
            if date_label != "Edited" {
                dates.push_str(&format!("\nEdited: {}", shown(e.modified_at)));
            }
            if e.modified_at.is_none() && date_label != "Added here" {
                dates.push_str(&format!("\nAdded here: {}", shown(e.added_at)));
            }
        }
        let collection = format!(
            "{}{}",
            match e.key {
                LibraryKey::BuiltIn(_) => "Built-in · Read only",
                LibraryKey::User(_) if e.trashed => "Trash",
                LibraryKey::User(_) => "User",
                LibraryKey::External(_) => "Open file · Temporary",
            },
            if requires_read(&e.key) {
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
        let mut memberships: Vec<String> =
            e.nominal.bands.iter().map(ToString::to_string).collect();
        if e.nominal.outside_bands() {
            memberships.push("Other rates".into());
        }
        if e.nominal.no_rhythm() {
            memberships.push("No rhythmic rate".into());
        }
        if e.nominal.multiple() {
            memberships.push("Multiple rhythms".into());
        }
        if e.nominal.completeness != crate::core::rhythm::Completeness::Complete {
            memberships.push("Needs review".into());
        }
        let bands = memberships.join(" · ");
        LibraryRowView {
            built_in: matches!(e.key, LibraryKey::BuiltIn(_)),
            key: e.key.to_string(),
            text: format!("{title} · {collection}\n{summary}\n{bands}\n{duration}\n{dates}"),
            title,
            collection,
            summary,
            bands,
            duration,
            dates,
        }
    };
    let mut rows = Vec::new();
    if let Ok(result) = query.execute(entries) {
        if result.groups.is_empty() {
            for e in result.entries {
                rows.push(label(e));
            }
        } else {
            for group in result.groups {
                rows.push(heading_row(format!(
                    "{} · {} unique sounds",
                    group.label,
                    group.keys.len()
                )));
                for key in group.keys {
                    if let Some(e) = entries.iter().find(|e| e.key == key) {
                        rows.push(label(e));
                    }
                }
            }
        }
    }
    let diagnostics: Vec<_> = diagnostics
        .iter()
        .filter(|problem| problem.storage_id.ends_with(".toml"))
        .collect();
    if !matches!(
        query.scope,
        Scope::BuiltIn | Scope::OpenFiles | Scope::Trash
    ) && !diagnostics.is_empty()
    {
        rows.push(heading_row(
            "Unreadable storage entries · not included in sound counts".into(),
        ));
        for problem in diagnostics {
            rows.push(heading_row(format!(
                "Needs review · {}\n{}",
                problem.storage_id,
                problem.message()
            )));
        }
    }
    let selected = selection.and_then(|key| rows.iter().position(|row| row.key == key.to_string()));
    (rows, selected)
}
fn heading_row(text: String) -> LibraryRowView {
    LibraryRowView {
        built_in: false,
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
pub(crate) fn facets(
    query: &CatalogQuery,
    entries: &[EntrySummary],
    complete: bool,
) -> (Vec<LibraryFacetView>, Vec<String>) {
    use crate::core::rhythm::{Band, Method};
    use crate::discovery::query::RhythmState;
    let facet = |query: &CatalogQuery, label: String, detail: &str| {
        let count = query.count(entries).unwrap_or(0);
        let count = if complete {
            count.to_string()
        } else {
            format!("{count} observed")
        };
        let text = if detail.is_empty() {
            format!("{label} · {count}")
        } else {
            format!("{label} · {detail} · {count}")
        };
        LibraryFacetView {
            label,
            detail: detail.into(),
            count,
            text,
        }
    };
    let base = query.clone();
    let mut unrestricted = base.clone();
    unrestricted.rhythm.bands.clear();
    unrestricted.rhythm.state = None;
    let mut bands = vec![facet(&unrestricted, "All sounds".into(), "")];
    for band in Band::ALL {
        let mut query = unrestricted.clone();
        query.rhythm.bands = vec![band];
        bands.push(facet(&query, band.to_string(), band.interval()));
    }
    for state in RhythmState::ALL {
        let mut query = unrestricted.clone();
        query.rhythm.state = Some(state);
        bands.push(facet(&query, state.label().into(), ""));
    }
    let mut unrestricted = base;
    unrestricted.rhythm.methods.clear();
    let mut methods = vec![facet(&unrestricted, "All methods".into(), "").text];
    for method in Method::ALL {
        let mut query = unrestricted.clone();
        query.rhythm.methods = vec![method];
        methods.push(facet(&query, method.label().into(), "").text);
    }
    (bands, methods)
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
            ("Created", &q.created),
            ("Edited", &q.modified),
            ("Added here", &q.added),
        ] {
            for (label, value) in [
                ("from inclusive", range.from),
                ("before exclusive", range.before),
            ] {
                if let Some(value) = value {
                    restrictions.push(format!(
                        "{name} {label}: {} · UTC {value}",
                        crate::discovery::calendar::local_timestamp(value)
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
    use crate::presentation::vocabulary::Status;
    if visible > 0 {
        return "";
    }
    if !matches!(query.scope, Scope::BuiltIn | Scope::OpenFiles) && !complete {
        return Status::InventoryIncomplete.label();
    }
    if query.cleared_filters().count(entries).unwrap_or(0) > 0 {
        return Status::EmptyFilteredLibrary.label();
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
    filter_error: Option<&crate::error::QueryError>,
    read_failure: Option<&LibraryDiagnostic>,
    complete: bool,
) -> String {
    let mut notices = Vec::new();
    if let Some(problem) = problem {
        notices.push(format!("User Library unavailable: {}. Configure an absolute AUDITORYLAB_LIBRARY_DIR and restart; drafts are retained.",problem.message()));
    }
    if let Some(error) = filter_error {
        notices.push(error.to_string());
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
        let shown = |date: Option<crate::core::Timestamp>| {
            date.map_or_else(
                || "Unknown".into(),
                |d| {
                    if utc {
                        d.to_string()
                    } else {
                        crate::discovery::calendar::local_timestamp(d)
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
                "Not applicable".into()
            },
            e.trashed_at.map_or(String::new(), |d| format!(
                "\nTrashed here: {}",
                shown(Some(d))
            )),
            snapshot.map_or(String::new(), |s| {
                crate::storage::library::history_warnings(s.recipe())
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
    external: Option<&crate::storage::external::ObservedFile>,
    managed: Option<(&LibraryKey, crate::storage::library::VersionToken)>,
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

pub(crate) fn orphan_membership(id: crate::core::DocumentUuid) -> String {
    format!("membership for {id} has no recipe; the next locked write will drop it")
}
pub(crate) fn unavailable_history_clock(error: &crate::error::ClockError) -> String {
    format!("Cannot compare declared history with the observed clock: {error}")
}

impl crate::storage::library::WriteOutcome {
    /// Concise confirmed publication result with an unwrapped comparison token.
    #[must_use]
    pub fn description(&self) -> String {
        let mut text = format!("{}: {}", self.status.label(), self.key);
        if let Some(record) = &self.record {
            text.push_str(&format!("\nVersion token: {}", record.version_token));
        }
        for diagnostic in &self.diagnostics {
            text.push('\n');
            text.push_str(diagnostic);
        }
        text
    }
    /// Confirmed truth retained when completion output fails.
    #[must_use]
    pub fn completion_failure(&self, error: impl std::fmt::Display) -> String {
        format!(
            "Operation confirmed\n{}\nCompletion output failed: {error}",
            self.description()
        )
    }
}
pub(crate) fn removed_memberships(removed: usize) -> String {
    format!("Removed {removed} orphan membership timestamp rows")
}
pub(crate) fn membership_update_failed(error: &crate::error::LibraryError) -> String {
    format!("Saved; membership timestamps could not be updated: {error}")
}
pub(crate) fn membership_read_failed(error: &crate::error::LibraryError) -> String {
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
