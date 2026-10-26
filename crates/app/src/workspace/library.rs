// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Managed-library state and immutable worker requests. Only selected User content
//! is retained in full; index rows are disposable summaries.
use super::*;
use crate::collection::{CatalogQuery, EntrySummary, Scope};
use crate::library::{
    Inventory, LibraryKey, LibraryStore, LoadedRecord, Mutation, MutationKind, VersionToken,
    WriteOutcome,
};
use crate::presentation::{AudioSummary, DisplayText};
use crate::vocabulary::{NumberStyle, Term};

/// A real configured library capability or a typed configuration failure.
#[derive(Debug)]
pub enum LibraryBinding {
    /// A persistent store whose filesystem work is performed by jobs.
    Store(LibraryStore),
    /// Invalid/unavailable configuration; Built-ins remain usable.
    Unavailable(crate::library::LibraryDiagnostic),
}
impl LibraryBinding {
    /// Resolve native configuration outside the sans-IO coordinator.
    ///
    /// # Errors
    /// Refuses invalid shipped identity metadata while retaining configuration failures as data.
    pub fn configured() -> Result<Self, Error> {
        match crate::library::LibraryRoot::resolve(None) {
            Ok(root) => LibraryStore::new(root).map(Self::Store),
            Err(error) => Ok(Self::Unavailable(crate::library::LibraryDiagnostic {
                storage_id: "configured root".into(),
                code: "configuration".into(),
                message: error.to_string(),
            })),
        }
    }
    pub(super) fn store(&self) -> Option<&LibraryStore> {
        match self {
            Self::Store(store) => Some(store),
            Self::Unavailable(_) => None,
        }
    }
    pub(super) fn problem(&self) -> Option<&crate::library::LibraryDiagnostic> {
        match self {
            Self::Unavailable(problem) => Some(problem),
            Self::Store(_) => None,
        }
    }
}
#[derive(Debug)]
pub(super) struct LibraryState {
    pub(super) binding: LibraryBinding,
    pub(super) catalog: Vec<RecipeSnapshot>,
    pub(super) cache: std::cell::RefCell<Option<(views::LibraryViewKey, Arc<LibraryView>)>>,
    #[cfg(test)]
    pub(super) evaluations: std::cell::Cell<u64>,
    entries: Vec<EntrySummary>,
    snapshots: Vec<(LibraryKey, crate::recipe::SnapshotId)>,
    pub(super) query: CatalogQuery,
    visible: Vec<LibraryKey>,
    selection: Option<LibraryKey>,
    loaded: Option<LoadedRecord>,
    refresh: bool,
    load_attempted: Option<LibraryKey>,
    inventory: Arc<InventoryStatus>,
    read_failure: Option<crate::library::LibraryDiagnostic>,
    filter_error: Option<Error>,
    unreadable: std::collections::BTreeSet<LibraryKey>,
    stale: std::collections::BTreeSet<LibraryKey>,
    root_unavailable: bool,
    pub(super) revision: u64,
    last_saved: Option<LibraryKey>,
}
impl LibraryState {
    pub(super) fn new(
        binding: LibraryBinding,
        catalog: Vec<RecipeSnapshot>,
    ) -> Result<Self, Error> {
        let available = matches!(binding, LibraryBinding::Store(_));
        let mut result = Self {
            refresh: available,
            binding,
            catalog: Vec::with_capacity(catalog.len()),
            cache: std::cell::RefCell::new(None),
            #[cfg(test)]
            evaluations: std::cell::Cell::new(0),
            entries: Vec::new(),
            snapshots: Vec::new(),
            query: CatalogQuery::default(),
            visible: Vec::new(),
            selection: None,
            loaded: None,
            load_attempted: None,
            inventory: Arc::new(InventoryStatus::default()),
            read_failure: None,
            filter_error: None,
            unreadable: std::collections::BTreeSet::new(),
            stale: std::collections::BTreeSet::new(),
            root_unavailable: !available,
            revision: 0,
            last_saved: None,
        };
        for snapshot in catalog {
            result.register(&snapshot)?;
            result.catalog.push(snapshot);
        }
        result.refilter();
        Ok(result)
    }
    pub(super) fn record_read_status(&mut self, job: &LibraryJob, status: &FileStatus) {
        let cause = match status {
            FileStatus::Library(LibraryStatus::Failed { error, .. })
            | FileStatus::Failed(error) => Some(error.to_string()),
            FileStatus::WorkerFailed { error, .. } => Some(error.description()),
            _ => None,
        };
        if let Some(message) = cause {
            let storage_id = match &job.operation {
                Operation::Load(key) => {
                    self.unreadable.insert(key.clone());
                    key.to_string()
                }
                _ => job.store.root().path().display().to_string(),
            };
            self.read_failure = Some(crate::library::LibraryDiagnostic {
                storage_id,
                code: "read_failure".into(),
                message,
            });
            Arc::make_mut(&mut self.inventory).complete = false;
            self.revision += 1;
        } else if self.read_failure.take().is_some() {
            self.revision += 1;
        }
    }
    fn register(&mut self, snapshot: &RecipeSnapshot) -> Result<(), Error> {
        let key = match snapshot.origin() {
            crate::recipe::Origin::BuiltIn(id) => LibraryKey::BuiltIn((*id).into()),
            crate::recipe::Origin::User { .. } => return Ok(()),
            _ => crate::library::ExternalId::allocate().map(LibraryKey::External)?,
        };
        // Reopening a path replaces its temporary snapshot without changing other keys.
        self.snapshots.push((key.clone(), snapshot.id()));
        self.entries
            .push(EntrySummary::from_snapshot(key, snapshot, None));
        Ok(())
    }

    fn requires_read(&self, key: &LibraryKey) -> bool {
        matches!(key, LibraryKey::User(_))
            && (self.root_unavailable || self.unreadable.contains(key) || self.stale.contains(key))
    }
    pub(super) fn selected_unreadable(&self) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|key| self.requires_read(key))
    }
    pub(super) fn selected_user(&self) -> bool {
        matches!(self.selection, Some(LibraryKey::User(_)))
    }
    pub(super) fn store_clone(&self) -> Option<LibraryStore> {
        self.binding.store().cloned()
    }
    pub(super) fn selected_text(&self) -> Option<Arc<str>> {
        self.loaded
            .as_ref()
            .filter(|r| Some(&r.key) == self.selection.as_ref())
            .map(|r| r.recipe_toml.clone())
    }
    fn select(&mut self, selection: Option<LibraryKey>) {
        if self.selection != selection {
            self.selection = selection;
            self.loaded = None;
            self.load_attempted = None;
        }
    }
    pub(super) fn refilter(&mut self) {
        // Query edits are validated before assignment; execution cannot fail here.
        self.visible = self.query.execute(&self.entries).map_or_else(
            |_| Vec::new(),
            |r| r.entries.iter().map(|e| e.key.clone()).collect(),
        );
        if self
            .selection
            .as_ref()
            .is_none_or(|key| !self.visible.contains(key))
        {
            self.select(self.visible.first().cloned());
        }
        self.revision += 1;
    }
    pub(super) fn selected(&self) -> Option<&RecipeSnapshot> {
        let key = self
            .selection
            .as_ref()
            .filter(|key| self.visible.contains(key))?;
        if matches!(key, LibraryKey::User(_)) {
            let record = self.loaded.as_ref().filter(|r| &r.key == key)?;
            let entry = self.entries.iter().find(|e| &e.key == key)?;
            return (entry.version_token.as_ref() == Some(&record.version_token))
                .then_some(&record.snapshot);
        }
        let (_, id) = self.snapshots.iter().rev().find(|(k, _)| k == key)?;
        self.catalog.iter().find(|s| s.id() == *id)
    }
    pub(super) fn rows(&self) -> (Vec<LibraryRowView>, Option<usize>) {
        let label = |e: &EntrySummary| {
            use crate::collection::SortBy;
            let sort = self.query.effective_sort().0;
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
                date.map_or_else(|| "Unknown".into(), crate::calendar::local_timestamp)
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
                if self.requires_read(&e.key) {
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
            if e.nominal.completeness != crate::core::nominal::Completeness::Complete {
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
        if let Ok(result) = self.query.execute(&self.entries) {
            if result.groups.is_empty() {
                for e in result.entries {
                    rows.push(label(e));
                }
            } else {
                for group in result.groups {
                    rows.push(Self::heading_row(format!(
                        "{} · {} unique sounds",
                        group.label,
                        group.keys.len()
                    )));
                    for key in group.keys {
                        if let Some(e) = self.entries.iter().find(|e| e.key == key) {
                            rows.push(label(e));
                        }
                    }
                }
            }
        }
        let diagnostics: Vec<_> = self
            .inventory
            .diagnostics
            .iter()
            .filter(|problem| problem.storage_id.ends_with(".toml"))
            .collect();
        if !matches!(
            self.query.scope,
            Scope::BuiltIn | Scope::OpenFiles | Scope::Trash
        ) && !diagnostics.is_empty()
        {
            rows.push(Self::heading_row(
                "Unreadable storage entries · not included in sound counts".into(),
            ));
            for problem in diagnostics {
                rows.push(Self::heading_row(format!(
                    "Needs review · {}\n{}",
                    problem.storage_id, problem.message
                )));
            }
        }
        let selected = self
            .selection
            .as_ref()
            .and_then(|key| rows.iter().position(|row| row.key == key.to_string()));
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
    fn accept_record(&mut self, record: LoadedRecord) {
        self.unreadable.remove(&record.key);
        self.stale.remove(&record.key);
        self.root_unavailable = false;
        self.entries.retain(|e| e.key != record.key);
        self.entries.push(record.summary());
        self.refilter();
        if self.selection.as_ref() == Some(&record.key) {
            self.loaded = Some(record);
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LibraryJob {
    pub(super) store: LibraryStore,
    operation: Operation,
}
#[derive(Debug, Clone)]
enum Operation {
    Scan,
    DiscardTrash {
        key: LibraryKey,
        expect: VersionToken,
        document: DocumentId,
        draft: u64,
        input: u64,
    },
    ExternalSave {
        document: DocumentId,
        expected: crate::external::ObservedFile,
        recipe: Arc<Recipe>,
    },

    Initialize,
    Load(LibraryKey),
    Write {
        document: Option<DocumentId>,
        mutation: Mutation,
    },
}
#[derive(Debug)]
/// Concrete result of an immutable library operation.
pub enum LibraryOutcome {
    /// Bounded storage inventory.
    Inventory(Inventory),
    /// Confirmed external save.
    ExternalWritten(Box<crate::external::ExternalOutcome>),
    /// Captured selected record.
    Loaded(Box<LoadedRecord>),
    /// Verified managed mutation.
    Written(Box<WriteOutcome>),
}
impl LibraryJob {
    pub(super) fn context(&self) -> status::LibraryContext {
        let selector = match &self.operation {
            Operation::Load(key) | Operation::DiscardTrash { key, .. } => Some(key.clone()),
            Operation::Write { mutation, .. } => Some(match mutation {
                Mutation::Create(recipe) | Mutation::Add(recipe) => {
                    LibraryKey::User(recipe.document().id)
                }
                Mutation::Update { key, .. }
                | Mutation::Rename { key, .. }
                | Mutation::Trash { key, .. }
                | Mutation::Restore { key, .. }
                | Mutation::Purge { key, .. } => key.clone(),
            }),
            Operation::Scan | Operation::Initialize | Operation::ExternalSave { .. } => None,
        };
        let path = match &self.operation {
            Operation::ExternalSave { expected, .. } => expected.path().to_owned(),
            _ => self.store.root().path().to_owned(),
        };
        status::LibraryContext { path, selector }
    }
    pub(super) fn background_read(&self) -> bool {
        matches!(self.operation, Operation::Scan | Operation::Load(_))
    }
    pub(super) fn may_publish(&self) -> bool {
        matches!(
            self.operation,
            Operation::Write { .. }
                | Operation::ExternalSave { .. }
                | Operation::DiscardTrash { .. }
        )
    }

    pub(super) fn accepts(&self, outcome: &LibraryOutcome) -> bool {
        let record_matches = |record: &LoadedRecord, key: &LibraryKey| {
            record.key == *key
                && matches!(key,LibraryKey::User(id) if record.snapshot.recipe().document().id==*id)
                && matches!(record.snapshot.origin(),crate::recipe::Origin::User {id,version} if key==&LibraryKey::User(*id) && version==&record.version_token)
        };
        let written_matches = |outcome: &WriteOutcome, key: &LibraryKey, kind: MutationKind| {
            outcome.key == *key
                && outcome.kind == kind
                && match (&outcome.record, kind) {
                    (None, MutationKind::Purge) => true,
                    (Some(record), kind) if kind != MutationKind::Purge => {
                        record_matches(record, key)
                            && match kind {
                                MutationKind::Trash => record.trashed,
                                MutationKind::Restore => !record.trashed,
                                _ => true,
                            }
                    }
                    _ => false,
                }
        };
        match (&self.operation, outcome) {
            (Operation::Scan | Operation::Initialize, LibraryOutcome::Inventory(_)) => true,
            (Operation::Load(key), LibraryOutcome::Loaded(record)) => record_matches(record, key),
            (Operation::DiscardTrash { key, .. }, LibraryOutcome::Written(outcome)) => {
                written_matches(outcome, key, MutationKind::Trash)
            }
            (Operation::Write { mutation, .. }, LibraryOutcome::Written(outcome)) => {
                let (key, kind) = match mutation {
                    Mutation::Create(recipe) => {
                        (LibraryKey::User(recipe.document().id), MutationKind::Create)
                    }
                    Mutation::Add(recipe) => {
                        (LibraryKey::User(recipe.document().id), MutationKind::Add)
                    }
                    Mutation::Update { key, .. } => (key.clone(), MutationKind::Update),
                    Mutation::Rename { key, .. } => (key.clone(), MutationKind::Rename),
                    Mutation::Trash { key, .. } => (key.clone(), MutationKind::Trash),
                    Mutation::Restore { key, .. } => (key.clone(), MutationKind::Restore),
                    Mutation::Purge { key, .. } => (key.clone(), MutationKind::Purge),
                };
                written_matches(outcome, &key, kind)
            }
            (
                Operation::ExternalSave {
                    expected, recipe, ..
                },
                LibraryOutcome::ExternalWritten(outcome),
            ) => {
                outcome.previous.matches(expected)
                    && outcome.recipe.document().id == recipe.document().id
                    && outcome.recipe.document().created_at == recipe.document().created_at
            }
            _ => false,
        }
    }
    pub(super) fn execute(self) -> Result<LibraryOutcome, Error> {
        match self.operation {
            Operation::DiscardTrash { key, expect, .. } => self
                .store
                .apply(
                    Mutation::Trash {
                        key,
                        expect: Some(expect),
                    },
                    &Cancel::default(),
                )
                .map(|r| LibraryOutcome::Written(Box::new(r))),
            Operation::ExternalSave {
                expected, recipe, ..
            } => crate::external::ExternalWriter
                .save(&expected, recipe, &Cancel::default())
                .map(|r| LibraryOutcome::ExternalWritten(Box::new(r))),
            Operation::Scan => Ok(LibraryOutcome::Inventory(
                self.store.scan(&Cancel::default()),
            )),
            Operation::Initialize => {
                let initialized = self.store.root().initialize()?;
                let mut inventory = self.store.scan(&Cancel::default());
                inventory
                    .diagnostics
                    .extend(initialized.warnings.into_iter().map(|message| {
                        crate::library::LibraryDiagnostic {
                            storage_id: "library.toml".into(),
                            code: "initialization_warning".into(),
                            message,
                        }
                    }));
                Ok(LibraryOutcome::Inventory(inventory))
            }
            Operation::Load(key) => self
                .store
                .load(&key)
                .map(|r| LibraryOutcome::Loaded(Box::new(r))),
            Operation::Write { mutation, .. } => self
                .store
                .apply(mutation, &Cancel::default())
                .map(|r| LibraryOutcome::Written(Box::new(r))),
        }
    }
}
impl Workspace {
    /// Incomplete storage state remains visible outside the storage disclosure.
    #[must_use]
    pub fn library_alert(&self) -> String {
        let mut notices = Vec::new();
        if let Some(problem) = self.library.binding.problem() {
            notices.push(format!("User Library unavailable: {}. Configure an absolute AUDITORYLAB_LIBRARY_DIR and restart; drafts are retained.",problem.message));
        }
        if let Some(error) = &self.library.filter_error {
            notices.push(error.to_string());
        }
        if let Some(problem) = &self.library.read_failure {
            notices.push(format!(
                "Library read failed · {}: {}",
                problem.storage_id, problem.message
            ));
        }
        if !self.library.inventory.complete {
            notices.push("Library inventory incomplete · previous unseen rows are last observed snapshots. Refresh before acting; no empty-library claim is implied.".into());
        }
        notices.join("\n")
    }
    /// Create a desktop workspace connected to the configured persistent root.
    /// Filesystem reads run on the existing file worker; initialization is explicit
    /// for overrides. A missing default root is created by the first publication.
    ///
    /// # Errors
    /// Returns identity or curated-catalog errors without falling back to file saves.
    pub fn with_library(
        catalog: Vec<RecipeSnapshot>,
        capabilities: Capabilities,
        store: LibraryStore,
    ) -> Result<Self, Error> {
        Self::with_binding(catalog, capabilities, LibraryBinding::Store(store))
    }
    pub(super) fn reveal_retained_source(&mut self, snapshot: RecipeSnapshot) {
        let p = &mut self.library;
        let key = if let Some((key, _)) = p.snapshots.iter().find(|(_, id)| *id == snapshot.id()) {
            key.clone()
        } else {
            let key = match crate::library::ExternalId::allocate() {
                Ok(id) => LibraryKey::External(id),
                Err(error) => {
                    self.files.status = FileStatus::Failed(error);
                    self.changed();
                    return;
                }
            };
            p.snapshots.push((key.clone(), snapshot.id()));
            p.entries
                .push(EntrySummary::from_snapshot(key.clone(), &snapshot, None));
            p.catalog.push(snapshot);
            key
        };
        p.query = CatalogQuery {
            scope: if matches!(key, LibraryKey::BuiltIn(_)) {
                Scope::BuiltIn
            } else {
                Scope::OpenFiles
            },
            ..CatalogQuery::default()
        };
        p.refilter();
        p.select(Some(key));
        self.library.query.search.clear();
        self.library.query.rhythm = crate::query::RhythmQuery::default();
        self.view = View::Library;
        self.effects.push(Effect::ShowView(View::Library));
        self.files.status = FileStatus::Notice(FileNotice::RetainedSource);
        self.changed();
    }
    /// Surface a native filter error while retaining the prior valid query.
    pub fn library_filter_error(&mut self, error: Error) {
        self.library.filter_error = Some(error);
        self.library.revision += 1;
        self.changed();
    }
    /// Disjunctive unique counts from the shared query, with reference intervals.
    #[must_use]
    pub fn library_facets(&self) -> (Vec<LibraryFacetView>, Vec<String>) {
        use crate::core::nominal::{Band, Method};
        use crate::query::RhythmState;
        let facet = |query: &CatalogQuery, label: String, detail: &str| {
            let count = query.count(&self.library.entries).unwrap_or(0);
            let count = if self.library.inventory.complete {
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
        let base = self.catalog_query().clone();
        let mut unrestricted = base.clone();
        unrestricted.rhythm.bands.clear();
        unrestricted.rhythm.state = None;
        let mut bands = vec![facet(&unrestricted, "All sounds".into(), "")];
        for band in Band::ALL {
            let mut query = unrestricted.clone();
            query.rhythm.bands = vec![band];
            bands.push(facet(&query, band.to_string(), band.interval()));
        }
        for state in [
            RhythmState::OutsideBands,
            RhythmState::None,
            RhythmState::Multiple,
            RhythmState::NeedsReview,
        ] {
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
            methods.push(facet(&query, method.key().into(), "").text);
        }
        (bands, methods)
    }
    /// Concise authoritative query summary for the results header.
    #[must_use]
    pub fn library_overview(&self) -> String {
        let q = &self.library.query;
        let sounds = if self.library.visible.len() == 1 {
            "unique sound"
        } else {
            "unique sounds"
        };
        let count = if self.library.inventory.complete {
            format!("{} {sounds}", self.library.visible.len())
        } else {
            format!(
                "{} observed {sounds} · Inventory incomplete",
                self.library.visible.len()
            )
        };
        if *q != q.cleared_filters() {
            format!("{count} · Filters active")
        } else {
            count
        }
    }
    /// Unique counts and active restrictions; group repeats are never counted as sounds.
    #[must_use]
    pub fn library_count(&self) -> String {
        {
            let q = &self.library.query;
            let (sort, direction) = q.effective_sort();
            let mut lines = vec![format!(
                "{} · {} · Sort {} {} · Group {}",
                self.library_overview(),
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
                restrictions.push(format!("Method: {}", method.key()));
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
                            crate::calendar::local_timestamp(value)
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
    /// Explain an empty result without treating an incomplete scan as an empty collection.
    #[must_use]
    pub fn library_empty_message(&self) -> &'static str {
        use crate::vocabulary::Status;
        if !self.library.visible.is_empty() {
            return "";
        }
        if !matches!(self.library.query.scope, Scope::BuiltIn | Scope::OpenFiles)
            && !self.library.inventory.complete
        {
            return Status::InventoryIncomplete.label();
        }
        if self
            .library
            .query
            .cleared_filters()
            .count(&self.library.entries)
            .unwrap_or(0)
            > 0
        {
            return Status::EmptyFilteredLibrary.label();
        }
        match self.library.query.scope {
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

    /// Current persistent query; native filters and the CLI use the same contract.
    #[must_use]
    pub fn catalog_query(&self) -> &CatalogQuery {
        &self.library.query
    }
    /// Replace valid collection, date, evidence and sorting restrictions together.
    ///
    /// # Errors
    /// An invalid range leaves the previous query and selection intact.
    pub fn set_catalog_query(&mut self, query: CatalogQuery) -> Result<(), Error> {
        query.execute(&self.library.entries)?;
        self.library.filter_error = None;
        self.library.query = query;
        self.library.refilter();
        self.load_library_selection();
        self.changed();
        Ok(())
    }
    /// Select only a key that still belongs to the rendered query. Old row
    /// positions and stale callbacks cannot address replacement entries.
    pub fn select_library_key(&mut self, key: &str) {
        let p = &mut self.library;
        if let Some(found) = p.visible.iter().find(|entry| entry.to_string() == key) {
            p.select(Some(found.clone()));
            p.revision += 1;
        }
        self.load_library_selection();
        self.changed();
    }
    /// Keyboard movement through unique visible sounds, skipping virtual headings.
    pub fn navigate_library(&mut self, direction: i32) {
        let p = &mut self.library;
        let current = p
            .selection
            .as_ref()
            .and_then(|key| p.visible.iter().position(|k| k == key))
            .unwrap_or(0);
        let index = match direction {
            -2 => 0,
            2 => p.visible.len().saturating_sub(1),
            -1 => current.saturating_sub(1),
            _ => current
                .saturating_add(1)
                .min(p.visible.len().saturating_sub(1)),
        };
        if let Some(key) = p.visible.get(index) {
            p.select(Some(key.clone()));
            p.revision += 1;
        }
        self.load_library_selection();
        self.changed();
    }
    /// Request a rescan without dropping stale rows or the open draft on failure.
    pub fn refresh_library(&mut self) {
        {
            let p = &mut self.library;
            p.refresh = true;
            p.load_attempted = None;
        }
        self.continue_library();
        self.changed();
    }
    /// Explicitly initialize an override root. No scan initializes storage.
    pub fn initialize_library(&mut self) -> Result<(), Refusal> {
        self.managed_job(Operation::Initialize)?;
        Ok(())
    }
    /// Open the captured User version in the editor, retaining its identity.
    /// Unsaved changes use the existing Save/Discard/Cancel decision.
    pub fn edit_selected(&mut self) {
        if let Some(snapshot) = self
            .selected()
            .filter(|s| s.external_file().is_some())
            .cloned()
        {
            self.request(DocumentAction::EditExternal(snapshot));
            return;
        }
        let record = {
            self.library
                .loaded
                .as_ref()
                .filter(|r| Some(&r.key) == self.library.selection.as_ref())
        }
        .cloned();
        if let Some(record) = record.filter(|r| !r.trashed) {
            self.request(DocumentAction::EditManaged(record));
        } else {
            self.copy_selected();
        }
    }
    /// Edit only the managed version bound to the activated control.
    pub fn edit_library_binding(&mut self, key: &str, token: &str) {
        if token.is_empty()
            && self
                .library
                .selection
                .as_ref()
                .is_some_and(|k| k.to_string() == key)
            && self.selected().is_some_and(|s| s.external_file().is_some())
        {
            self.edit_selected();
            return;
        }
        let record = self
            .library
            .loaded
            .as_ref()
            .filter(|r| !self.library.requires_read(&r.key))
            .filter(|r| {
                r.key.to_string() == key && r.version_token.to_string() == token && !r.trashed
            })
            .cloned();
        if let Some(record) = record {
            self.request(DocumentAction::EditManaged(record));
        } else {
            self.files.status = FileStatus::Notice(FileNotice::ChangedSelection);
            self.changed();
        }
    }
    /// Save the current complete draft under a fresh independent User identity.
    ///
    /// # Errors
    /// Refuses incomplete drafts or occupied slots without replacing the document.
    pub fn save_independent_copy(&mut self) -> Result<(), Refusal> {
        let result = self.save_independent_copy_inner();
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused((refusal).clone());
        }
        self.changed();
        result
    }
    fn save_independent_copy_inner(&mut self) -> Result<(), Refusal> {
        self.import_admission()?;
        let content = self.prepare_recipe()?;
        let snapshot = RecipeSnapshot::new(
            crate::recipe::Origin::Draft(self.editor.session.draft().title().into()),
            (*content).clone(),
        );
        let draft =
            Draft::from_snapshot(&snapshot).map_err(|e| Refusal::Document(e.to_string()))?;

        self.replace_document(draft);
        self.save()
    }
    /// Add only the temporary snapshot named by the activated control.
    ///
    /// # Errors
    /// Refuses a changed/hidden selection rather than adding a replacement row.
    pub fn add_library_binding(&mut self, key: &str) -> Result<(), Refusal> {
        let result = self.add_library_binding_inner(key);
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused((refusal).clone());
        }
        self.changed();
        result
    }
    fn add_library_binding_inner(&mut self, key: &str) -> Result<(), Refusal> {
        if self.library_binding().0 != key {
            return Err(Refusal::Document(
                "Open file selection changed; activate Add again.".into(),
            ));
        }
        self.add_selected_to_library()
    }
    /// Add a temporary opened document to User without modifying its source file.
    ///
    /// # Errors
    /// Refuses non-temporary selections or occupied task slots.
    pub fn add_selected_to_library(&mut self) -> Result<(), Refusal> {
        let snapshot = self.selected().ok_or(Refusal::NoSelection)?;
        if !matches!(
            snapshot.origin(),
            crate::recipe::Origin::File(_) | crate::recipe::Origin::Stream(_)
        ) {
            return Err(Refusal::Document(
                "Select an Open file to add; use Edit a copy for Built-in content.".into(),
            ));
        }
        let recipe = Arc::new(snapshot.recipe().clone());
        self.managed_job(Operation::Write {
            document: None,
            mutation: Mutation::Add(recipe),
        })?;
        Ok(())
    }
    pub(super) fn discard_then_trash(&mut self, key: LibraryKey, expect: VersionToken) {
        let operation = Operation::DiscardTrash {
            key,
            expect,
            document: self.editor.session.document(),
            draft: self.editor.session.draft_revision(),
            input: self.editor.session.input_revision(),
        };
        if let Err(refusal) = self.managed_job(operation) {
            self.files.status = FileStatus::Refused(refusal.clone());
        }
    }
    /// Execute one managed action using the key and token captured by the UI.
    ///
    /// # Errors
    /// Built-ins, stale gestures, unsafe dirty-document removal and invalid input
    /// are refused before filesystem access. Purge requires separate confirmation.
    pub fn mutate_library(
        &mut self,
        key: &str,
        token: &str,
        action: &str,
        title: &str,
        confirmed: bool,
    ) -> Result<(), Refusal> {
        let result = self.mutate_library_inner(key, token, action, title, confirmed);
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused((refusal).clone());
        }
        self.changed();
        result
    }
    fn mutate_library_inner(
        &mut self,
        key: &str,
        token: &str,
        action: &str,
        title: &str,
        confirmed: bool,
    ) -> Result<(), Refusal> {
        let key: LibraryKey = key
            .parse()
            .map_err(|e: Error| Refusal::Document(e.to_string()))?;
        key.require_user()
            .map_err(|e| Refusal::Document(e.to_string()))?;
        let expect: VersionToken = token
            .parse()
            .map_err(|e: Error| Refusal::Document(e.to_string()))?;
        if action == "trash"
            && self.editor.session.dirty()
            && self
                .editor
                .location
                .managed()
                .is_some_and(|(saved, _)| saved == &key)
        {
            self.request(DocumentAction::TrashSaved { key, expect });
            return Ok(());
        }
        if action == "purge" && !confirmed {
            self.start_allowed()?;
            if self.files.slot.is_some() {
                return Err(Refusal::FileBusy);
            }
            let entry = {
                self.library.entries.iter().find(|e| {
                    e.key == key && e.version_token.as_ref() == Some(&expect) && e.trashed
                })
            }
            .ok_or(Refusal::Document(
                "Select the current trashed version before permanent deletion.".into(),
            ))?;
            let title = entry.title.clone();
            let prompt = self.prompt(PromptState::Purge {
                key: key.clone(),
                expect,
            });
            self.effects.push(Effect::AskPurge {
                prompt,
                key: key.to_string(),
                title,
            });
            return Ok(());
        }
        let mutation = match action {
            "rename" => Mutation::Rename {
                key,
                expect: Some(expect),
                title: title.into(),
            },
            "trash" => Mutation::Trash {
                key,
                expect: Some(expect),
            },
            "restore" => Mutation::Restore {
                key,
                expect: Some(expect),
            },
            "purge" => Mutation::Purge {
                key,
                expect: Some(expect),
                confirmed,
            },
            _ => return Err(Refusal::Document("Unknown library action.".into())),
        };
        self.managed_job(Operation::Write {
            document: None,
            mutation,
        })?;
        Ok(())
    }
    /// Close only a still-selected temporary entry captured at activation.
    pub fn close_open_file_binding(&mut self, key: &str) {
        if self.library_binding().0 == key {
            self.close_open_file();
        }
    }
    /// Close the selected temporary entry without changing its external file.
    pub fn close_open_file(&mut self) {
        let p = &mut self.library;
        let Some(key @ LibraryKey::External(_)) = p.selection.clone() else {
            return;
        };
        let ids: Vec<_> = p
            .snapshots
            .iter()
            .filter(|(k, _)| k == &key)
            .map(|(_, id)| *id)
            .collect();
        p.catalog.retain(|s| !ids.contains(&s.id()));
        p.snapshots.retain(|(k, _)| k != &key);
        p.entries.retain(|e| e.key != key);
        p.refilter();
        self.changed();
    }
    /// Reveal the last confirmed saved key explicitly, clearing filters only on
    /// this command; saving itself preserves the current query.
    pub fn show_saved(&mut self) -> bool {
        let p = &mut self.library;
        let Some(key) = p.last_saved.clone() else {
            return false;
        };
        let Some(entry) = p.entries.iter().find(|e| e.key == key) else {
            self.files.status = FileStatus::Notice(FileNotice::MissingSavedEntry);
            self.changed();
            return false;
        };
        p.query = CatalogQuery {
            scope: if entry.trashed {
                Scope::Trash
            } else {
                Scope::User
            },
            ..CatalogQuery::default()
        };
        p.refilter();
        p.select(Some(key));
        self.library.query.search.clear();
        self.library.query.rhythm = crate::query::RhythmQuery::default();
        self.load_library_selection();
        self.changed();
        true
    }
    pub(super) fn selected_unreadable(&self) -> bool {
        self.library.selected_unreadable()
    }
    pub(super) fn selected_trashed(&self) -> bool {
        {
            self.library
                .loaded
                .as_ref()
                .filter(|r| Some(&r.key) == self.library.selection.as_ref())
        }
        .is_some_and(|r| r.trashed)
    }
    /// Root location and current inventory diagnostics.
    #[must_use]
    pub fn library_status(&self) -> String {
        match &self.library.binding {
            LibraryBinding::Store(store) => format!(
                "{}\n{}{}",
                store.root().path().display(),
                self.library.inventory.description(),
                self.library
                    .read_failure
                    .as_ref()
                    .map_or(String::new(), |problem| format!(
                        "\n{}: {}",
                        problem.storage_id, problem.message
                    ))
            ),
            LibraryBinding::Unavailable(problem) => problem.message.clone(),
        }
    }
    /// Portable history and local membership are displayed with distinct labels.
    #[must_use]
    pub fn library_dates(&self) -> String {
        self.library_dates_shown(false)
    }
    /// Exact canonical UTC instants, available separately from local presentation.
    #[must_use]
    pub fn library_dates_utc(&self) -> String {
        self.library_dates_shown(true)
    }
    fn library_dates_shown(&self, utc: bool) -> String {
        {
            self.library
                .selection
                .as_ref()
                .and_then(|key| self.library.entries.iter().find(|e| &e.key == key))
        }
        .map_or(String::new(), |e| {
            let shown = |date: Option<crate::core::Timestamp>| {
                date.map_or_else(
                    || "Unknown".into(),
                    |d| {
                        if utc {
                            d.to_string()
                        } else {
                            crate::calendar::local_timestamp(d)
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
                self.selected()
                    .map_or(String::new(), |s| crate::library::history_warnings(
                        s.recipe()
                    )
                    .iter()
                    .map(|w| format!("\n{w}"))
                    .collect::<String>())
            )
        })
    }
    /// Captured selected managed key/token for action binding, or empty values.
    #[must_use]
    pub fn library_binding(&self) -> (String, String, bool) {
        let p = &self.library;
        p.selection
            .as_ref()
            .and_then(|key| p.entries.iter().find(|entry| &entry.key == key))
            .map_or((String::new(), String::new(), false), |entry| {
                (
                    entry.key.to_string(),
                    if p.selected_unreadable() {
                        String::new()
                    } else {
                        entry
                            .version_token
                            .as_ref()
                            .map_or(String::new(), ToString::to_string)
                    },
                    entry.trashed,
                )
            })
    }
    fn managed_job(&mut self, operation: Operation) -> Result<TaskId, Refusal> {
        self.start_allowed()?;
        if self.files.slot.is_some() {
            return Err(Refusal::FileBusy);
        }
        let p = &self.library;

        let store = p
            .binding
            .store()
            .ok_or_else(|| {
                Refusal::LibraryUnavailable(
                    p.binding
                        .problem()
                        .unwrap_or_else(|| unreachable!("unavailable binding"))
                        .clone(),
                )
            })?
            .clone();
        let id = self.start_file(FileKind::Managed(LibraryJob { store, operation }));
        self.changed();
        Ok(id)
    }
    pub(super) fn start_managed_save(
        &mut self,
        document: DocumentId,
        recipe: Arc<Recipe>,
    ) -> Result<TaskId, Refusal> {
        let p = &self.library;
        if let Some(key) = self.editor.location.detached() {
            return Err(Refusal::Document(format!(
                "Detached draft from {key}. Edit the restored saved entry or Save independent copy; the old identity is not recreated."
            )));
        }
        if let Some(expected) = self.editor.location.external().cloned() {
            return self.managed_job(Operation::ExternalSave {
                document,
                expected,
                recipe,
            });
        }
        if self.editor.location.managed().is_none()
            && p.entries
                .iter()
                .any(|e| e.key == LibraryKey::User(recipe.document().id))
        {
            return Err(Refusal::Document("Detached draft belongs to an existing saved identity. Restore/edit that version or Save independent copy.".into()));
        }
        let mutation = self.editor.location.managed().map_or_else(
            || Mutation::Create(recipe.clone()),
            |(key, expect)| Mutation::Update {
                key: key.clone(),
                expect: Some(expect),
                recipe: recipe.clone(),
            },
        );
        self.managed_job(Operation::Write {
            document: Some(document),
            mutation,
        })
    }
    pub(super) fn continue_library(&mut self) {
        if self.exiting {
            return;
        }
        if self.files.slot.is_some() || self.prompt.is_some() || self.pending.is_some() {
            return;
        }
        if self.library.refresh {
            self.library.refresh = false;
            let _ = self.managed_job(Operation::Scan);
            return;
        }
        self.load_library_selection();
    }
    pub(super) fn load_library_selection(&mut self) {
        if self.files.slot.is_some() || self.prompt.is_some() || self.pending.is_some() {
            return;
        }
        let p = &mut self.library;
        let Some(key @ LibraryKey::User(_)) = p.selection.clone() else {
            return;
        };
        if p.load_attempted.as_ref() == Some(&key) {
            return;
        }
        if !p.requires_read(&key)
            && p.loaded.as_ref().is_some_and(|r| {
                r.key == key
                    && p.entries.iter().any(|e| {
                        e.key == key
                            && e.version_token.as_ref() == Some(&r.version_token)
                            && e.trashed == r.trashed
                            && e.added_at == r.added_at
                            && e.trashed_at == r.trashed_at
                    })
            })
        {
            return;
        }
        p.load_attempted = Some(key.clone());
        let _ = self.managed_job(Operation::Load(key));
    }
    pub(super) fn register_external(&mut self, snapshot: &RecipeSnapshot) {
        {
            let p = &mut self.library;
            // Drop superseded temporary entries for the same opened path.
            let old_ids: Vec<_> = p
                .catalog
                .iter()
                .filter(|s| s.origin() == snapshot.origin())
                .map(RecipeSnapshot::id)
                .collect();
            let old_keys: Vec<_> = p
                .snapshots
                .iter()
                .filter(|(_, id)| old_ids.contains(id))
                .map(|(k, _)| k.clone())
                .collect();
            p.entries.retain(|e| !old_keys.contains(&e.key));
            p.snapshots.retain(|(k, _)| !old_keys.contains(k));
            match p.register(snapshot) {
                Ok(()) => {
                    p.query.scope = Scope::OpenFiles;
                    p.refilter();
                    p.select(p.snapshots.last().map(|(k, _)| k.clone()));
                }
                Err(e) => self.files.status = FileStatus::Failed(e),
            }
        }
    }
    pub(super) fn finish_managed(
        &mut self,
        job: &LibraryJob,
        result: Result<LibraryOutcome, Error>,
    ) -> (FileStatus, Option<bool>) {
        let document = match job.operation {
            Operation::Write { document, .. } => document,
            Operation::ExternalSave { document, .. } => Some(document),
            _ => None,
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                return (
                    FileStatus::Library(LibraryStatus::Failed {
                        error,
                        publishing: job.may_publish(),
                        context: job.context(),
                    }),
                    document.map(|_| false),
                );
            }
        };
        match result {
            LibraryOutcome::ExternalWritten(outcome) => {
                self.accept_external_outcome(document, *outcome)
            }

            LibraryOutcome::Inventory(inventory) => {
                let p = &mut self.library;
                p.read_failure = None;
                p.inventory = Arc::new(InventoryStatus {
                    complete: inventory.complete,
                    not_initialized: inventory.not_initialized,
                    count: inventory.entries.len(),
                    diagnostics: inventory.diagnostics.clone(),
                    stages: inventory.stages.clone(),
                });
                p.root_unavailable = inventory.diagnostics.iter().any(|d| d.code == "root");
                p.unreadable = inventory
                    .diagnostics
                    .iter()
                    .filter_map(|d| {
                        d.storage_id
                            .strip_suffix(".toml")
                            .and_then(|s| s.parse().ok())
                            .map(LibraryKey::User)
                    })
                    .collect();
                let observed = inventory
                    .entries
                    .iter()
                    .map(|e| e.key.clone())
                    .collect::<std::collections::BTreeSet<_>>();
                p.stale = if inventory.complete {
                    std::collections::BTreeSet::new()
                } else {
                    p.entries
                        .iter()
                        .filter(|e| {
                            matches!(e.key, LibraryKey::User(_)) && !observed.contains(&e.key)
                        })
                        .map(|e| e.key.clone())
                        .collect()
                };
                if inventory.complete {
                    p.entries.retain(|e| !matches!(e.key, LibraryKey::User(_)));
                    p.entries.extend(inventory.entries);
                } else {
                    let mut entries = std::mem::take(&mut p.entries)
                        .into_iter()
                        .map(|e| (e.key.clone(), e))
                        .collect::<std::collections::BTreeMap<_, _>>();
                    for entry in inventory.entries {
                        entries.insert(entry.key.clone(), entry);
                    }
                    p.entries = entries.into_values().collect();
                }

                p.load_attempted = None;
                p.refilter();
                (
                    FileStatus::Library(LibraryStatus::Inventory(p.inventory.clone())),
                    None,
                )
            }
            LibraryOutcome::Loaded(record) => {
                let p = &mut self.library;
                p.accept_record(*record);
                (FileStatus::Library(LibraryStatus::Loaded), None)
            }
            LibraryOutcome::Written(outcome) => {
                let mut result = self.accept_managed_outcome(document, *outcome);
                if let Operation::DiscardTrash {
                    document,
                    draft,
                    input,
                    ..
                } = job.operation
                    && self.editor.session.document() == document
                    && self.editor.session.draft_revision() == draft
                    && self.editor.session.input_revision() == input
                {
                    result = self
                        .finish_discard_reset(result, Draft::new().and_then(EditorSession::new));
                }
                result
            }
        }
    }
    fn finish_discard_reset(
        &mut self,
        mut result: (FileStatus, Option<bool>),
        editor: Result<EditorSession, Error>,
    ) -> (FileStatus, Option<bool>) {
        match editor {
            Ok(editor) => self.install_document(editor),
            Err(error) => {
                if let FileStatus::Library(LibraryStatus::Written { feedback, .. }) = &mut result.0
                {
                    feedback.push(SaveFeedback::EditorReset(error));
                }
                result.1 = Some(false);
            }
        }
        result
    }

    fn accept_external_outcome(
        &mut self,
        document: Option<DocumentId>,
        outcome: crate::external::ExternalOutcome,
    ) -> (FileStatus, Option<bool>) {
        let Some(document) = document else {
            return (
                FileStatus::Library(LibraryStatus::InvalidOwner),
                Some(false),
            );
        };
        let p = &mut self.library;

        let snapshot = outcome.snapshot();
        let same_document = self.editor.session.document() == document;
        if same_document && !self.editor.session.record_saved(document, &outcome.recipe) {
            return (
                FileStatus::Library(LibraryStatus::InvalidOwner),
                Some(false),
            );
        }
        if same_document {
            self.editor.location = snapshot
                .external_file()
                .cloned()
                .map_or(DocumentLocation::Unsaved, DocumentLocation::External);
        }
        // Replace only the selected external observation's index, keeping its key.
        let path = outcome.previous.path();
        if let Some(index) = p
            .catalog
            .iter()
            .position(|s| s.external_file().is_some_and(|f| f.path() == path))
        {
            let old_id = p.catalog[index].id();
            if let Some((key, id)) = p.snapshots.iter_mut().find(|(_, id)| *id == old_id) {
                *id = snapshot.id();
                p.entries.retain(|e| &e.key != key);
                p.entries
                    .push(EntrySummary::from_snapshot(key.clone(), &snapshot, None));
            }
            p.catalog[index] = snapshot;
        }
        p.refilter();
        (
            FileStatus::Library(LibraryStatus::ExternalWritten {
                status: outcome.status,
                diagnostics: outcome.diagnostics,
            }),
            Some(true),
        )
    }
    /// Persistent edit location, distinct from selection, ancestry and playback.
    #[must_use]
    pub fn document_location(&self) -> String {
        self.location_projection().1
    }
    /// Short location badge; exact saved identity/version remains in details.
    #[must_use]
    pub fn document_location_label(&self) -> &'static str {
        self.location_projection().0
    }
    fn location_projection(&self) -> (&'static str, String) {
        if let Some(error) = self.library.binding.problem() {
            return (
                "User Library unavailable",
                format!("Unsaved · User Library unavailable: {}", error.message),
            );
        }
        if let Some(key) = self.editor.location.detached() {
            return (
                "Detached draft · save an independent copy or restore/edit saved entry",
                format!(
                    "Detached draft · {key} · restore/edit the saved version or Save independent copy"
                ),
            );
        }
        if let Some(external) = self.editor.location.external() {
            return (
                "External file",
                format!(
                    "External file · {} · observed {}",
                    external.path().display(),
                    external.fingerprint()
                ),
            );
        }
        if let Some((key, token)) = self.editor.location.managed() {
            return ("User Library", format!("User Library · {key} · {token}"));
        }
        (
            "Unsaved · Save to User Library",
            "Unsaved · Save to User Library".into(),
        )
    }
    fn accept_managed_outcome(
        &mut self,
        document: Option<DocumentId>,
        outcome: WriteOutcome,
    ) -> (FileStatus, Option<bool>) {
        let mut feedback = Vec::new();
        let p = &mut self.library;
        let key = outcome.key.clone();

        if let Some(record) = outcome.record {
            if document.is_some()
                && let Err(error) = record.snapshot.at(self.editor.export_rate)
            {
                feedback.push(SaveFeedback::InvalidAtRate {
                    rate: self.editor.export_rate,
                    error,
                });
            }
            if let Some(document) = document {
                let same_document = self.editor.session.document() == document;
                if same_document
                    && !self
                        .editor
                        .session
                        .record_saved(document, record.snapshot.recipe())
                {
                    return (
                        FileStatus::Library(LibraryStatus::InvalidOwner),
                        Some(false),
                    );
                }
                if same_document {
                    self.editor.location = DocumentLocation::Managed {
                        key: key.clone(),
                        token: record.version_token,
                    };
                }
            } else if self
                .editor
                .location
                .managed()
                .is_some_and(|(saved, _)| saved == &key)
            {
                // Membership changes do not alter portable history or the live draft.
                // A dirty rename keeps its old expected token so Save exposes conflict.
                if matches!(outcome.kind, MutationKind::Trash) {
                    self.editor.location = DocumentLocation::Detached(key.clone());
                } else if !self.editor.session.dirty() {
                    match EditorSession::new(Draft::from_recipe(record.snapshot.recipe().clone())) {
                        Ok(editor) => {
                            self.editor.session = editor;
                            self.editor.status = EditStatus::Idle;
                            self.editor.session.record_saved(
                                self.editor.session.document(),
                                record.snapshot.recipe(),
                            );
                            self.editor.location = DocumentLocation::Managed {
                                key: key.clone(),
                                token: record.version_token,
                            };
                        }
                        Err(error) => feedback.push(SaveFeedback::EditorRefresh(error)),
                    }
                }
            }
            p.accept_record(record);
        } else {
            p.entries.retain(|e| e.key != key);
            if p.loaded.as_ref().is_some_and(|r| r.key == key) {
                p.loaded = None;
            }
            p.refilter();
        }
        p.last_saved = Some(key.clone());
        (
            FileStatus::Library(LibraryStatus::Written {
                key,
                status: outcome.status,
                document: document.is_some(),
                diagnostics: outcome.diagnostics,
                feedback,
            }),
            document.map(|_| true),
        )
    }
}

#[cfg(test)]
mod tests;
