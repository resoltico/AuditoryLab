//! Managed-library state and immutable worker requests. Only selected User content
//! is retained in full; index rows are disposable summaries.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::core::query::Scope;
use crate::discovery::collection::{CatalogQuery, EntrySummary};
use crate::document::origin::{LibraryKey, VersionToken};
use crate::services::library::Library;
use crate::storage::library::{Inventory, LoadedRecord, Mutation, MutationKind, WriteOutcome};

/// An app-issued row capability bound to one process-local listing epoch.
#[derive(Debug, Clone)]
pub struct EntryHandle {
    listing: Arc<()>,
    key: LibraryKey,
    version: Option<VersionToken>,
    trashed: bool,
}
impl PartialEq for EntryHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.listing, &other.listing)
            && self.key == other.key
            && self.version == other.version
            && self.trashed == other.trashed
    }
}
impl Eq for EntryHandle {}

/// A captured library context and its immutable frontend diagnostic projection.
#[derive(Debug)]
pub struct LibraryBinding {
    context: crate::services::library::LibraryContext,
    problem: Option<crate::services::library::LibraryDiagnostic>,
}
impl LibraryBinding {
    /// Resolve native configuration outside the sans-IO coordinator.
    ///
    /// # Errors
    /// Refuses invalid shipped metadata while retaining configuration failures.
    pub fn configured() -> Result<Self, crate::services::library::LibraryError> {
        Self::from_context(crate::services::library::LibraryContext::resolve(None))
    }
    /// Bind one resolved context, deriving its frontend failure view once.
    ///
    /// # Errors
    /// Invalid shipped catalog identities remain fatal startup failures.
    pub fn from_context(
        context: crate::services::library::LibraryContext,
    ) -> Result<Self, crate::services::library::LibraryError> {
        use crate::services::library::{DiagnosticDetail, LibraryDiagnostic, LibraryError};
        if let Some(
            error @ (LibraryError::InvalidBuiltin { .. } | LibraryError::DuplicateBuiltin { .. }),
        ) = context.problem()
        {
            return Err(error.clone());
        }
        let problem = context.problem().map(|error| LibraryDiagnostic {
            storage_id: context.location().map_or_else(
                || "configured root".into(),
                |location| location.path().display().to_string(),
            ),
            detail: DiagnosticDetail::Failure(Box::new(error.clone())),
        });
        Ok(Self { context, problem })
    }
    fn from_library(library: Library) -> Self {
        Self {
            context: crate::services::library::LibraryContext::from_library(library),
            problem: None,
        }
    }
    pub(super) fn store(&self) -> Option<&Library> {
        self.context.store()
    }
    pub(super) fn problem(&self) -> Option<&crate::services::library::LibraryDiagnostic> {
        self.problem.as_ref()
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
    snapshots: Vec<(LibraryKey, crate::document::recipe::SnapshotId)>,
    pub(super) query: CatalogQuery,
    visible: Vec<LibraryKey>,
    selection: Option<LibraryKey>,
    loaded: Option<LoadedRecord>,
    refresh: bool,
    load_attempted: Option<LibraryKey>,
    inventory: Arc<InventoryStatus>,
    read_failure: Option<crate::storage::library::LibraryDiagnostic<super::TaskFailure>>,
    filter_error: Option<crate::discovery::query::QueryError>,
    unreadable: std::collections::BTreeSet<LibraryKey>,
    stale: std::collections::BTreeSet<LibraryKey>,
    root_unavailable: bool,
    pub(super) revision: u64,
    listing: Arc<()>,
    last_saved: Option<LibraryKey>,
}
impl LibraryState {
    pub(super) fn new(
        binding: LibraryBinding,
        catalog: Vec<RecipeSnapshot>,
    ) -> Result<Self, Error> {
        let available = binding.store().is_some();
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
            listing: Arc::new(()),
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
            | FileStatus::Failed(error) => {
                Some(crate::services::library::DiagnosticDetail::Failure(
                    Box::new(super::TaskFailure::Operation(error.clone())),
                ))
            }
            FileStatus::WorkerFailed { error, .. } => {
                Some(crate::services::library::DiagnosticDetail::Failure(
                    Box::new(super::TaskFailure::Worker(error.clone())),
                ))
            }
            _ => None,
        };
        if let Some(message) = cause {
            let storage_id = match &job.operation {
                LibraryRequest::Load(key) => {
                    self.unreadable.insert(key.clone());
                    key.to_string()
                }
                _ => job.store.path().display().to_string(),
            };
            self.read_failure = Some(crate::storage::library::LibraryDiagnostic {
                storage_id,

                detail: message,
            });
            Arc::make_mut(&mut self.inventory).complete = false;
            self.revision += 1;
        } else if self.read_failure.take().is_some() {
            self.revision += 1;
        }
    }
    fn entry_key(snapshot: &RecipeSnapshot) -> Result<Option<LibraryKey>, Error> {
        Ok(match snapshot.origin() {
            crate::document::recipe::Origin::BuiltIn(id) => Some(LibraryKey::BuiltIn(
                crate::core::catalog::CatalogId::new(id)
                    .unwrap_or_else(|| unreachable!("catalog names are validated at build")),
            )),
            crate::document::recipe::Origin::User { .. } => None,
            _ => Some(crate::document::origin::ExternalId::allocate().map(LibraryKey::External)?),
        })
    }
    fn register_as(&mut self, snapshot: &RecipeSnapshot, key: LibraryKey) {
        self.snapshots.push((key.clone(), snapshot.id()));
        self.entries
            .push(EntrySummary::from_snapshot(key, snapshot, None));
    }
    fn register(&mut self, snapshot: &RecipeSnapshot) -> Result<(), Error> {
        if let Some(key) = Self::entry_key(snapshot)? {
            self.register_as(snapshot, key);
        }
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
    pub(super) fn store_clone(&self) -> Option<Library> {
        self.binding.store().cloned()
    }
    pub(super) fn selected_text(&self) -> Option<Arc<str>> {
        self.loaded
            .as_ref()
            .filter(|r| Some(&r.key) == self.selection.as_ref())
            .map(|r| r.recipe_toml.clone())
    }
    fn entry_handle(&self, entry: &EntrySummary) -> EntryHandle {
        EntryHandle {
            listing: Arc::clone(&self.listing),
            key: entry.key.clone(),
            version: entry.version_token,
            trashed: entry.trashed,
        }
    }
    fn select(&mut self, selection: Option<LibraryKey>) {
        if self.selection != selection {
            self.selection = selection;
            self.loaded = None;
            self.load_attempted = None;
        }
    }
    pub(super) fn refilter(&mut self) {
        self.listing = Arc::new(());
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
        let entry = |entry: &EntrySummary| LibraryRowView::Entry {
            handle: self.entry_handle(entry),
            entry: Box::new(entry.clone()),
            requires_read: self.requires_read(&entry.key),
        };
        let mut rows = Vec::new();
        if let Ok(result) = self.query.execute(&self.entries) {
            if result.groups.is_empty() {
                rows.extend(result.entries.into_iter().map(entry));
            } else {
                for group in result.groups {
                    let entries = group
                        .keys
                        .iter()
                        .filter_map(|key| self.entries.iter().find(|e| &e.key == key))
                        .map(entry)
                        .collect::<Vec<_>>();
                    rows.push(LibraryRowView::Group(group));
                    rows.extend(entries);
                }
            }
        }
        let diagnostics = self
            .inventory
            .diagnostics
            .iter()
            .filter(|problem| problem.storage_id.ends_with(".toml"))
            .collect::<Vec<_>>();
        if !matches!(
            self.query.scope,
            Scope::BuiltIn | Scope::OpenFiles | Scope::Trash
        ) && !diagnostics.is_empty()
        {
            rows.push(LibraryRowView::Unreadable);
            rows.extend(
                diagnostics
                    .into_iter()
                    .cloned()
                    .map(LibraryRowView::Diagnostic),
            );
        }
        let selected = self.selection.as_ref().and_then(|key| {
            rows.iter()
                .position(|row| matches!(row,LibraryRowView::Entry {entry,..} if &entry.key==key))
        });
        (rows, selected)
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
    pub(super) store: Library,
    operation: LibraryRequest,
}
#[derive(Debug, Clone)]
enum LibraryRequest {
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
        expected: crate::document::origin::ObservedFile,
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
    ExternalWritten(Box<crate::storage::external::ExternalOutcome>),
    /// Captured selected record.
    Loaded(Box<LoadedRecord>),
    /// Verified managed mutation.
    Written(Box<WriteOutcome>),
}
impl LibraryJob {
    pub(super) fn operation_term(&self) -> crate::core::vocabulary::Operation {
        use crate::core::vocabulary::Operation as Term;
        match &self.operation {
            LibraryRequest::Scan => Term::LibraryRefresh,
            LibraryRequest::Initialize => Term::LibraryInit,
            LibraryRequest::Load(_) => Term::LibraryRead,
            LibraryRequest::DiscardTrash { .. } => Term::LibraryTrash,
            LibraryRequest::ExternalSave { .. } => Term::Save,
            LibraryRequest::Write {
                document: Some(_), ..
            } => Term::Save,
            LibraryRequest::Write { mutation, .. } => match mutation {
                Mutation::Create(_) => Term::SaveCopy,
                Mutation::Add(_) => Term::LibraryAdd,
                Mutation::Update { .. } => Term::LibraryUpdate,
                Mutation::Rename { .. } => Term::LibraryRename,
                Mutation::Trash { .. } => Term::LibraryTrash,
                Mutation::Restore { .. } => Term::LibraryRestore,
                Mutation::Purge { .. } => Term::LibraryPurge,
            },
        }
    }

    pub(super) fn context(&self) -> status::LibraryContext {
        let selector = match &self.operation {
            LibraryRequest::Load(key) | LibraryRequest::DiscardTrash { key, .. } => {
                Some(key.clone())
            }
            LibraryRequest::Write { mutation, .. } => Some(match mutation {
                Mutation::Create(recipe) | Mutation::Add(recipe) => {
                    LibraryKey::User(recipe.document().id)
                }
                Mutation::Update { key, .. }
                | Mutation::Rename { key, .. }
                | Mutation::Trash { key, .. }
                | Mutation::Restore { key, .. }
                | Mutation::Purge { key, .. } => key.clone(),
            }),
            LibraryRequest::Scan
            | LibraryRequest::Initialize
            | LibraryRequest::ExternalSave { .. } => None,
        };
        let path = match &self.operation {
            LibraryRequest::ExternalSave { expected, .. } => expected.path().to_owned(),
            _ => self.store.path().to_owned(),
        };
        status::LibraryContext { path, selector }
    }
    pub(super) fn background_read(&self) -> bool {
        matches!(
            self.operation,
            LibraryRequest::Scan | LibraryRequest::Load(_)
        )
    }
    pub(super) fn may_publish(&self) -> bool {
        matches!(
            self.operation,
            LibraryRequest::Write { .. }
                | LibraryRequest::ExternalSave { .. }
                | LibraryRequest::DiscardTrash { .. }
        )
    }

    pub(super) fn accepts(&self, outcome: &LibraryOutcome) -> bool {
        let record_matches = |record: &LoadedRecord, key: &LibraryKey| {
            record.key == *key
                && matches!(key,LibraryKey::User(id) if record.snapshot.recipe().document().id==*id)
                && matches!(record.snapshot.origin(),crate::document::recipe::Origin::User {id,version} if key==&LibraryKey::User(*id) && version==&record.version_token)
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
            (LibraryRequest::Scan | LibraryRequest::Initialize, LibraryOutcome::Inventory(_)) => {
                true
            }
            (LibraryRequest::Load(key), LibraryOutcome::Loaded(record)) => {
                record_matches(record, key)
            }
            (LibraryRequest::DiscardTrash { key, .. }, LibraryOutcome::Written(outcome)) => {
                written_matches(outcome, key, MutationKind::Trash)
            }
            (LibraryRequest::Write { mutation, .. }, LibraryOutcome::Written(outcome)) => {
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
                LibraryRequest::ExternalSave {
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
            LibraryRequest::DiscardTrash { key, expect, .. } => self
                .store
                .apply(
                    Mutation::Trash {
                        key,
                        expect: Some(expect),
                    },
                    &Cancel::default(),
                )
                .map(|r| LibraryOutcome::Written(Box::new(r)))
                .map_err(Error::from),
            LibraryRequest::ExternalSave {
                expected, recipe, ..
            } => crate::services::save_external(&expected, recipe, &Cancel::default())
                .map(|r| LibraryOutcome::ExternalWritten(Box::new(r)))
                .map_err(Error::from),
            LibraryRequest::Scan => Ok(LibraryOutcome::Inventory(
                self.store.scan(&Cancel::default()),
            )),
            LibraryRequest::Initialize => {
                let initialized = self.store.initialize()?;
                let mut inventory = self.store.scan(&Cancel::default());
                inventory
                    .diagnostics
                    .extend(initialized.warnings.into_iter().map(|message| {
                        crate::storage::library::LibraryDiagnostic {
                            storage_id: "library.toml".into(),

                            detail: crate::storage::library::DiagnosticDetail::Warning(message),
                        }
                    }));
                Ok(LibraryOutcome::Inventory(inventory))
            }
            LibraryRequest::Load(key) => self
                .store
                .load(&key)
                .map(|r| LibraryOutcome::Loaded(Box::new(r)))
                .map_err(Error::from),
            LibraryRequest::Write { mutation, .. } => self
                .store
                .apply(mutation, &Cancel::default())
                .map(|r| LibraryOutcome::Written(Box::new(r)))
                .map_err(Error::from),
        }
    }
}
impl Workspace {
    /// Issue the current visible row's opaque handle without parsing display data.
    #[must_use]
    pub fn entry_handle(&self, key: &LibraryKey) -> Option<EntryHandle> {
        self.library
            .visible
            .contains(key)
            .then(|| self.library.entries.iter().find(|entry| &entry.key == key))
            .flatten()
            .map(|entry| self.library.entry_handle(entry))
    }
    /// Handle of the current indexed selection, if it belongs to this listing.
    #[must_use]
    pub fn selected_entry_handle(&self) -> Option<EntryHandle> {
        self.library
            .selection
            .as_ref()
            .and_then(|key| self.entry_handle(key))
    }
    fn resolve_entry_handle(
        &self,
        handle: &EntryHandle,
        purge: bool,
    ) -> Result<(LibraryKey, Option<VersionToken>, bool), Refusal> {
        let refusal = || {
            if purge {
                Refusal::PurgeSelectionChanged
            } else {
                Refusal::SelectionChanged
            }
        };
        if !Arc::ptr_eq(&self.library.listing, &handle.listing)
            || !self.library.visible.contains(&handle.key)
        {
            return Err(refusal());
        }
        let current = self
            .library
            .entries
            .iter()
            .find(|entry| entry.key == handle.key)
            .ok_or_else(refusal)?;
        if current.version_token != handle.version || current.trashed != handle.trashed {
            return Err(refusal());
        }
        Ok((handle.key.clone(), handle.version, handle.trashed))
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
        store: Library,
    ) -> Result<Self, Error> {
        Self::with_binding(catalog, capabilities, LibraryBinding::from_library(store))
    }
    pub(super) fn reveal_retained_source(&mut self, snapshot: RecipeSnapshot) {
        let p = &mut self.library;
        let key = if let Some((key, _)) = p.snapshots.iter().find(|(_, id)| *id == snapshot.id()) {
            key.clone()
        } else {
            let key = match crate::document::origin::ExternalId::allocate() {
                Ok(id) => LibraryKey::External(id),
                Err(error) => {
                    self.files.status = FileStatus::Failed(error.into());
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
        self.library.query.rhythm = crate::discovery::query::RhythmQuery::default();
        self.view = View::Library;
        self.effects.push(Effect::ShowView(View::Library));
        self.files.status = FileStatus::Notice(FileNotice::RetainedSource);
        self.changed();
    }
    /// Surface a native filter error while retaining the prior valid query.
    pub(super) fn library_filter_error(&mut self, error: crate::discovery::query::QueryError) {
        self.library.filter_error = Some(error);
        self.library.revision += 1;
        self.changed();
    }
    /// Disjunctive unique counts from the shared query, with reference intervals.
    #[must_use]
    pub fn library_facets(&self) -> FacetCounts {
        use crate::core::{
            query::RhythmState,
            rhythm::{Band, Method},
        };
        let complete = self.library.inventory.complete;
        let facet = |query: &CatalogQuery, kind| LibraryFacetView {
            kind,
            count: query.count(&self.library.entries).unwrap_or(0),
            complete,
        };
        let mut query = self.library.query.clone();
        query.rhythm.bands.clear();
        query.rhythm.state = None;
        let mut bands = vec![facet(&query, FacetKind::AllSounds)];
        for band in Band::ALL {
            let mut q = query.clone();
            q.rhythm.bands = vec![band];
            bands.push(facet(&q, FacetKind::Band(band)));
        }
        for state in RhythmState::ALL {
            let mut q = query.clone();
            q.rhythm.state = Some(state);
            bands.push(facet(&q, FacetKind::State(state)));
        }
        let mut query = self.library.query.clone();
        query.rhythm.methods.clear();
        let mut methods = vec![facet(&query, FacetKind::AllMethods)];
        for method in Method::ALL {
            let mut q = query.clone();
            q.rhythm.methods = vec![method];
            methods.push(facet(&q, FacetKind::Method(method)));
        }
        FacetCounts { bands, methods }
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
    pub(super) fn set_catalog_query(
        &mut self,
        query: CatalogQuery,
    ) -> Result<(), crate::discovery::query::QueryError> {
        self.library.set_query(query)?;
        self.load_library_selection();
        self.changed();
        Ok(())
    }
    /// Select only a key that still belongs to the rendered query. Old row
    /// positions and stale callbacks cannot address replacement entries.
    pub(super) fn select_entry(&mut self, entry: EntryHandle) -> Result<(), Refusal> {
        let result = self
            .resolve_entry_handle(&entry, false)
            .map(|(key, _, _)| key);
        match result {
            Ok(key) => {
                self.library.select(Some(key));
                self.library.revision += 1;
                self.load_library_selection();
                self.changed();
                Ok(())
            }
            Err(refusal) => {
                self.files.status = FileStatus::Refused(refusal.clone());
                self.changed();
                Err(refusal)
            }
        }
    }
    /// Keyboard movement through unique visible sounds, skipping virtual headings.
    pub(super) fn navigate_library(&mut self, direction: i32) {
        self.library.navigate(direction);
        self.load_library_selection();
        self.changed();
    }
    /// Request a rescan without dropping stale rows or the open draft on failure.
    pub(super) fn refresh_library(&mut self) {
        self.library.request_refresh();
        self.continue_library();
        self.changed();
    }
    /// Explicitly initialize an override root. No scan initializes storage.
    pub(super) fn initialize_library(&mut self) -> Result<(), Refusal> {
        self.managed_job(LibraryRequest::Initialize)?;
        Ok(())
    }
    /// Open the captured User version in the editor, retaining its identity.
    /// Unsaved changes use the existing Save/Discard/Cancel decision.
    pub(super) fn edit_selected(&mut self) -> Result<(), Refusal> {
        if let Some(snapshot) = self
            .selected()
            .filter(|s| s.external_file().is_some())
            .cloned()
        {
            return self.request(DocumentAction::EditExternal(snapshot));
        }
        let record = {
            self.library
                .loaded
                .as_ref()
                .filter(|r| Some(&r.key) == self.library.selection.as_ref())
        }
        .cloned();
        if let Some(record) = record.filter(|r| !r.trashed) {
            self.request(DocumentAction::EditManaged(record))
        } else {
            self.copy_selected()
        }
    }
    /// Edit only the managed version bound to the activated control.
    pub(super) fn edit_entry(&mut self, entry: EntryHandle) -> Result<(), Refusal> {
        let result = (|| {
            let (key, version, trashed) = self.resolve_entry_handle(&entry, false)?;
            if self.library.selection.as_ref() != Some(&key) {
                return Err(Refusal::SelectionChanged);
            }
            if matches!(key, LibraryKey::External(_))
                && self
                    .selected()
                    .is_some_and(|snapshot| snapshot.external_file().is_some())
            {
                return self.edit_selected();
            }
            let record = self
                .library
                .loaded
                .as_ref()
                .filter(|record| {
                    record.key == key
                        && Some(record.version_token) == version
                        && !trashed
                        && !self.library.requires_read(&key)
                })
                .cloned()
                .ok_or(Refusal::SelectionChanged)?;
            self.request(DocumentAction::EditManaged(record))
        })();
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused(refusal.clone());
            self.changed();
        }
        result
    }
    /// Save the current complete draft under a fresh independent User identity.
    ///
    /// # Errors
    /// Refuses incomplete drafts or occupied slots without replacing the document.
    pub(super) fn save_independent_copy(&mut self) -> Result<(), Refusal> {
        let result = self.save_independent_copy_inner();
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused((refusal).clone());
        }
        self.changed();
        result
    }
    fn save_independent_copy_inner(&mut self) -> Result<(), Refusal> {
        let content = self.prepare_save_copy()?;
        let snapshot = RecipeSnapshot::new(
            crate::document::recipe::Origin::Draft(self.editor.session.draft().title().into()),
            (*content).clone(),
        );
        let draft =
            Draft::from_snapshot(&snapshot).map_err(|e| Refusal::Document(Box::new(e.into())))?;

        let editor =
            EditorSession::new(draft).map_err(|error| Refusal::Document(Box::new(error.into())))?;
        self.install_document(editor);
        self.save()
    }
    /// Add only the temporary snapshot named by the activated control.
    ///
    /// # Errors
    /// Refuses a changed/hidden selection rather than adding a replacement row.
    pub(super) fn add_entry(&mut self, entry: EntryHandle) -> Result<(), Refusal> {
        let result = (|| {
            let (key, _, _) = self.resolve_entry_handle(&entry, false)?;
            if self.library.selection.as_ref() != Some(&key) {
                return Err(Refusal::SelectionChanged);
            }
            self.add_selected_to_library()
        })();
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused(refusal.clone());
        }
        self.changed();
        result
    }
    /// Add a temporary opened document to User without modifying its source file.
    ///
    /// # Errors
    /// Refuses non-temporary selections or occupied task slots.
    pub(super) fn add_selected_to_library(&mut self) -> Result<(), Refusal> {
        let recipe = self.prepare_add_to_library()?;
        self.managed_job(LibraryRequest::Write {
            document: None,
            mutation: Mutation::Add(recipe),
        })?;
        Ok(())
    }
    pub(super) fn discard_then_trash(&mut self, key: LibraryKey, expect: VersionToken) {
        let operation = LibraryRequest::DiscardTrash {
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
    pub(super) fn mutate_entry(
        &mut self,
        entry: EntryHandle,
        mutation: EntryMutation,
    ) -> Result<(), Refusal> {
        let result = self.mutate_entry_inner(entry, mutation);
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused(refusal.clone());
        }
        self.changed();
        result
    }
    fn mutate_entry_inner(
        &mut self,
        entry: EntryHandle,
        mutation: EntryMutation,
    ) -> Result<(), Refusal> {
        let purge = matches!(mutation, EntryMutation::RequestPurge);
        let (key, version, trashed) = self.resolve_entry_handle(&entry, purge)?;
        crate::storage::library::require_user(&key)
            .map_err(|error| Refusal::Document(Box::new(error.into())))?;
        let expect = version.ok_or(if purge {
            Refusal::PurgeSelectionChanged
        } else {
            Refusal::SelectionChanged
        })?;
        if matches!(mutation, EntryMutation::Trash) {
            self.trash_request_admission(&key)?;
            if self.editor.dirty_managed(&key) {
                return self.request(DocumentAction::TrashSaved { key, expect });
            }
        }
        let mutation = match mutation {
            EntryMutation::RequestPurge => {
                self.library_job_admission()?;
                if !trashed {
                    return Err(Refusal::PurgeSelectionChanged);
                }
                let title = self
                    .library
                    .entries
                    .iter()
                    .find(|entry| entry.key == key)
                    .ok_or(Refusal::PurgeSelectionChanged)?
                    .title
                    .clone();
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
            EntryMutation::Rename { title } => Mutation::Rename {
                key,
                expect: Some(expect),
                title,
            },
            EntryMutation::Trash => Mutation::Trash {
                key,
                expect: Some(expect),
            },
            EntryMutation::Restore => Mutation::Restore {
                key,
                expect: Some(expect),
            },
        };
        self.managed_job(LibraryRequest::Write {
            document: None,
            mutation,
        })?;
        Ok(())
    }
    pub(super) fn purge_confirmed(
        &mut self,
        key: LibraryKey,
        expect: VersionToken,
    ) -> Result<(), Refusal> {
        let result = self
            .managed_job(LibraryRequest::Write {
                document: None,
                mutation: Mutation::Purge {
                    key,
                    expect: Some(expect),
                    confirmed: true,
                },
            })
            .map(|_| ());
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused(refusal.clone());
        }
        self.changed();
        result
    }
    /// Close only a still-selected temporary entry captured at activation.
    pub(super) fn close_entry(&mut self, entry: EntryHandle) -> Result<(), Refusal> {
        let result = self
            .resolve_entry_handle(&entry, false)
            .and_then(|(key, _, _)| {
                if self.library.selection.as_ref() != Some(&key) {
                    return Err(Refusal::SelectionChanged);
                }
                self.close_open_file();
                Ok(())
            });
        if let Err(refusal) = &result {
            self.files.status = FileStatus::Refused(refusal.clone());
            self.changed();
        }
        result
    }
    /// Close the selected temporary entry without changing its external file.
    pub(super) fn close_open_file(&mut self) {
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
    pub(super) fn show_saved(&mut self) -> bool {
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
        self.library.query.rhythm = crate::discovery::query::RhythmQuery::default();
        self.load_library_selection();
        self.changed();
        true
    }

    /// Selected indexed identity and observed token; absence is represented by None.
    #[must_use]
    pub fn library_binding(&self) -> Option<SelectionBinding<'_>> {
        let p = &self.library;
        p.selection
            .as_ref()
            .and_then(|key| p.entries.iter().find(|entry| &entry.key == key))
            .map(|entry| SelectionBinding {
                key: &entry.key,
                version_token: if p.selected_unreadable() {
                    None
                } else {
                    entry.version_token
                },
                trashed: entry.trashed,
            })
    }
    fn managed_job(&mut self, operation: LibraryRequest) -> Result<TaskId, Refusal> {
        self.library_job_admission()?;
        let job = self.library.job(operation)?;
        let id = self.start_file(FileKind::Managed(job));
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
            return Err(Refusal::DetachedDraft(key.clone()));
        }
        if let Some(expected) = self.editor.location.external().cloned() {
            return self.managed_job(LibraryRequest::ExternalSave {
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
            return Err(Refusal::ExistingSavedIdentity(recipe.document().id));
        }
        let mutation = self.editor.location.managed().map_or_else(
            || Mutation::Create(recipe.clone()),
            |(key, expect)| Mutation::Update {
                key: key.clone(),
                expect: Some(expect),
                recipe: recipe.clone(),
            },
        );
        self.managed_job(LibraryRequest::Write {
            document: Some(document),
            mutation,
        })
    }
    pub(super) fn continue_library(&mut self) {
        if self.exiting {
            return;
        }
        if self.files.slot.is_some() || self.prompts.current.is_some() || self.pending.is_some() {
            return;
        }
        if self.library.refresh {
            self.library.refresh = false;
            let _ = self.managed_job(LibraryRequest::Scan);
            return;
        }
        self.load_library_selection();
    }
    pub(super) fn load_library_selection(&mut self) {
        if self.files.slot.is_some() || self.prompts.current.is_some() || self.pending.is_some() {
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
        let _ = self.managed_job(LibraryRequest::Load(key));
    }

    pub(super) fn finish_managed(
        &mut self,
        job: &LibraryJob,
        result: Result<LibraryOutcome, Error>,
    ) -> (FileStatus, Option<bool>) {
        let document = match job.operation {
            LibraryRequest::Write { document, .. } => document,
            LibraryRequest::ExternalSave { document, .. } => Some(document),
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

            LibraryOutcome::Inventory(inventory) => (
                FileStatus::Library(LibraryStatus::Inventory(
                    self.library.accept_inventory(inventory),
                )),
                None,
            ),
            LibraryOutcome::Loaded(record) => {
                let p = &mut self.library;
                p.accept_record(*record);
                (FileStatus::Library(LibraryStatus::Loaded), None)
            }
            LibraryOutcome::Written(outcome) => {
                let mut result = self.accept_managed_outcome(document, *outcome);
                if let LibraryRequest::DiscardTrash {
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
        editor: Result<EditorSession, crate::document::error::EditorError>,
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
        outcome: crate::storage::external::ExternalOutcome,
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

    fn accept_managed_outcome(
        &mut self,
        document: Option<DocumentId>,
        outcome: WriteOutcome,
    ) -> (FileStatus, Option<bool>) {
        let mut feedback = Vec::new();
        let key = outcome.key.clone();

        if let Some(record) = outcome.record {
            if !self
                .editor
                .record_managed(document, &key, outcome.kind, &record, &mut feedback)
            {
                return (
                    FileStatus::Library(LibraryStatus::InvalidOwner),
                    Some(false),
                );
            }
            self.library.accept_record(record);
        } else {
            self.library.remove_entry(&key);
        }
        self.library.last_saved = Some(key.clone());
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

impl LibraryState {
    pub(super) fn save_destination_admission(&self) -> Result<(), Refusal> {
        self.binding.problem().map_or(Ok(()), |problem| {
            Err(Refusal::LibraryUnavailable(problem.clone()))
        })
    }
    pub(super) fn copy_recipe_admission(recipe: &Recipe) -> Result<(), Refusal> {
        recipe.copy_readiness().map_err(|error| {
            Refusal::Document(Box::new(crate::core::RecipeError::Contract(error).into()))
        })
    }
    pub(super) fn copy_admission(&self) -> Result<(), Refusal> {
        Self::copy_recipe_admission(self.selected().ok_or(Refusal::NoSelection)?.recipe())
    }
    pub(super) fn prepare_audio(&self) -> Result<AudioRequest, Refusal> {
        if self.selected_unreadable() {
            return Err(Refusal::SavedEntryUnreadable);
        }
        if self.selected_trashed() {
            return Err(Refusal::RestoreBeforePlayback);
        }
        let snapshot = self.selected().ok_or(Refusal::NoSelection)?;
        Ok(AudioRequest::Finite {
            snapshot: Box::new(snapshot.clone()),
            store: self.selected_user().then(|| self.store_clone()).flatten(),
        })
    }
    pub(super) fn prepare_export(&self, rate: SampleRate) -> Result<file::PreparedExport, Refusal> {
        let snapshot = self.selected().ok_or(Refusal::NoSelection)?;
        let sound = snapshot.realize(rate).map_err(|error| Refusal::Invalid {
            title: snapshot.title(),
            rate,
            error: Some(error),
        })?;
        Ok(file::PreparedExport::Library(
            sound,
            snapshot.suggested_output_name("wav"),
        ))
    }
    pub(super) fn prepare_recipe_text(&self) -> Result<(Arc<str>, String), Refusal> {
        let snapshot = self.selected().ok_or(Refusal::NoSelection)?;
        // Selection authority, rather than retained provenance, chooses exact managed text.
        let text = match self.selection.as_ref().ok_or(Refusal::NoSelection)? {
            LibraryKey::BuiltIn(id) => Arc::from(
                crate::services::builtin_source(id.as_str())
                    .map_err(|error| Refusal::Document(Box::new(error.into())))?,
            ),
            LibraryKey::User(_) => self.selected_text().ok_or(Refusal::NoSelection)?,
            LibraryKey::External(_) => snapshot
                .canonical_text()
                .map_err(|error| Refusal::Document(Box::new(error.into())))?,
        };
        Ok((text, snapshot.title()))
    }
}

impl LibraryState {
    pub(super) fn selected_trashed(&self) -> bool {
        {
            self.loaded
                .as_ref()
                .filter(|r| Some(&r.key) == self.selection.as_ref())
        }
        .is_some_and(|r| r.trashed)
    }
}

impl LibraryState {
    pub(super) fn set_query(
        &mut self,
        query: CatalogQuery,
    ) -> Result<(), crate::discovery::query::QueryError> {
        query.execute(&self.entries)?;
        self.filter_error = None;
        self.query = query;
        self.refilter();
        Ok(())
    }

    pub(super) fn navigate(&mut self, direction: i32) {
        let current = self
            .selection
            .as_ref()
            .and_then(|key| self.visible.iter().position(|k| k == key))
            .unwrap_or(0);
        let index = match direction {
            -2 => 0,
            2 => self.visible.len().saturating_sub(1),
            -1 => current.saturating_sub(1),
            _ => current
                .saturating_add(1)
                .min(self.visible.len().saturating_sub(1)),
        };
        if let Some(key) = self.visible.get(index) {
            self.select(Some(key.clone()));
            self.revision += 1;
        }
    }
    pub(super) fn request_refresh(&mut self) {
        self.refresh = true;
        self.load_attempted = None;
    }
    pub(super) fn accept_inventory(&mut self, inventory: Inventory) -> Arc<InventoryStatus> {
        self.read_failure = None;
        self.inventory = Arc::new(InventoryStatus {
            complete: inventory.complete,
            not_initialized: inventory.not_initialized,
            count: inventory.entries.len(),
            diagnostics: inventory.diagnostics.clone(),
            stages: inventory.stages.clone(),
        });
        self.root_unavailable = inventory.aborted;
        self.unreadable = inventory
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
        self.stale = if inventory.complete {
            std::collections::BTreeSet::new()
        } else {
            self.entries
                .iter()
                .filter(|e| matches!(e.key, LibraryKey::User(_)) && !observed.contains(&e.key))
                .map(|e| e.key.clone())
                .collect()
        };
        if inventory.complete {
            self.entries
                .retain(|e| !matches!(e.key, LibraryKey::User(_)));
            self.entries.extend(inventory.entries);
        } else {
            let mut entries = std::mem::take(&mut self.entries)
                .into_iter()
                .map(|e| (e.key.clone(), e))
                .collect::<std::collections::BTreeMap<_, _>>();
            for entry in inventory.entries {
                entries.insert(entry.key.clone(), entry);
            }
            self.entries = entries.into_values().collect();
        }

        self.load_attempted = None;
        self.refilter();
        self.inventory.clone()
    }
    pub(super) fn register_external(&mut self, snapshot: &RecipeSnapshot) -> Result<(), Error> {
        // Reserve any needed identity before changing the previous observation.
        let key = Self::entry_key(snapshot)?;
        // Drop superseded temporary entries for the same opened path.
        let old_ids: Vec<_> = self
            .catalog
            .iter()
            .filter(|s| s.origin() == snapshot.origin())
            .map(RecipeSnapshot::id)
            .collect();
        let old_keys: Vec<_> = self
            .snapshots
            .iter()
            .filter(|(_, id)| old_ids.contains(id))
            .map(|(k, _)| k.clone())
            .collect();
        self.entries.retain(|e| !old_keys.contains(&e.key));
        self.snapshots.retain(|(k, _)| !old_keys.contains(k));
        if let Some(key) = key {
            self.register_as(snapshot, key);
        }
        self.query.scope = Scope::OpenFiles;
        self.refilter();
        self.select(self.snapshots.last().map(|(key, _)| key.clone()));
        Ok(())
    }
    fn job(&self, operation: LibraryRequest) -> Result<LibraryJob, Refusal> {
        self.save_destination_admission()?;
        let store = self
            .store_clone()
            .unwrap_or_else(|| unreachable!("admitted library binding has a service"));
        Ok(LibraryJob { store, operation })
    }
    pub(super) fn remove_entry(&mut self, key: &LibraryKey) {
        self.entries.retain(|entry| &entry.key != key);
        if self
            .loaded
            .as_ref()
            .is_some_and(|record| &record.key == key)
        {
            self.loaded = None;
        }
        self.refilter();
    }
}

impl LibraryState {
    pub(super) fn set_search(&mut self, search: &str) {
        self.query.search = search.into();
        self.refilter();
    }
    pub(super) fn set_rhythm(&mut self, rhythm: crate::discovery::query::RhythmQuery) {
        self.query.rhythm = rhythm;
        self.refilter();
    }
}

impl Workspace {
    /// Loaded in-memory Built-in and temporary snapshots. Managed User summaries
    /// appear in `library_view`; only selected User content is loaded in full.
    #[must_use]
    pub fn catalog(&self) -> &[RecipeSnapshot] {
        &self.library.catalog
    }
    /// The selected recipe, when the search shows it. A search that hides
    /// every recipe leaves nothing selected.
    #[must_use]
    pub fn selected(&self) -> Option<&RecipeSnapshot> {
        self.library.selected()
    }
    #[must_use]
    pub fn query(&self) -> &str {
        &self.library.query.search
    }
    /// Search titles, IDs, research text and component meaning. When the
    /// selection stops matching, the first match is selected.
    pub(super) fn set_query(&mut self, query: &str) {
        self.library.set_search(query);
        self.load_library_selection();
        self.changed();
    }
    /// Change nominal facets without rebinding any playback or editor snapshot.
    pub(super) fn set_rhythm_query(&mut self, query: crate::discovery::query::RhythmQuery) {
        self.library.set_rhythm(query);
        self.load_library_selection();
        self.changed();
    }
    /// Current shared nominal restrictions, retained when text search changes.
    #[must_use]
    pub const fn rhythm_query(&self) -> &crate::discovery::query::RhythmQuery {
        &self.library.query.rhythm
    }
}

impl LibraryState {
    pub(super) fn prepare_add(&self) -> Result<Arc<Recipe>, Refusal> {
        let snapshot = self.selected().ok_or(Refusal::NoSelection)?;
        if !matches!(
            snapshot.origin(),
            crate::document::recipe::Origin::File(_) | crate::document::recipe::Origin::Stream(_)
        ) {
            return Err(Refusal::TemporaryRequired);
        }
        Ok(Arc::new(snapshot.recipe().clone()))
    }
    pub(super) fn selected_key(&self) -> Option<&LibraryKey> {
        self.selection.as_ref()
    }
}

impl LibraryState {
    pub(super) fn import(&mut self, snapshot: RecipeSnapshot) -> Result<(), Error> {
        self.register_external(&snapshot)?;
        if let Some(index) = self
            .catalog
            .iter()
            .position(|existing| existing.origin() == snapshot.origin())
        {
            self.catalog[index] = snapshot;
        } else {
            self.catalog.push(snapshot);
        }
        self.query.search.clear();
        self.refilter();
        Ok(())
    }
}

/// A managed row mutation, independent of display wording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryMutation {
    /// Rename the captured saved entry.
    Rename { title: String },
    /// Move the captured active entry to Trash.
    Trash,
    /// Restore the captured trashed entry.
    Restore,
    /// Ask for confirmation before permanently deleting the captured Trash entry.
    RequestPurge,
}

/// Captured library and document-location context for frontend projections.
pub struct LibraryData<'a> {
    /// Committed query whose restrictions produced the visible selection.
    pub query: &'a CatalogQuery,
    /// Last observed unique index rows, independent of full source loading.
    pub entries: &'a [EntrySummary],
    /// Unique matching keys after the committed query.
    pub visible: usize,
    /// Completeness, scan bounds and retained storage causes.
    pub inventory: &'a InventoryStatus,
    /// Configured managed root when a usable capability exists.
    pub path: Option<&'a std::path::Path>,
    /// Configuration failure retained instead of silently using a fallback root.
    pub problem: Option<&'a crate::services::library::LibraryDiagnostic>,
    /// Refused query change; the prior valid query remains committed.
    pub filter_error: Option<&'a crate::discovery::query::QueryError>,
    /// Failure of the most recent admitted managed read.
    pub read_failure: Option<&'a crate::services::library::LibraryDiagnostic<super::TaskFailure>>,
    /// Readable immutable source correlated with the current selection.
    pub selected: Option<&'a RecipeSnapshot>,
    /// Indexed membership and dates of the selected key.
    pub selected_entry: Option<&'a EntrySummary>,
    /// Managed identity from which the open draft was detached.
    pub detached: Option<&'a LibraryKey>,
    /// Retained external file identity and exact-byte observation.
    pub external: Option<&'a crate::document::origin::ObservedFile>,
    /// Managed identity and exact content token bound to the open document.
    pub managed: Option<(&'a LibraryKey, VersionToken)>,
}
impl Workspace {
    /// Borrow the exact currently indexed library and captured edit location.
    #[must_use]
    pub fn library_data(&self) -> LibraryData<'_> {
        LibraryData {
            query: &self.library.query,
            entries: &self.library.entries,
            visible: self.library.visible.len(),
            inventory: &self.library.inventory,
            path: self.library.binding.store().map(Library::path),
            problem: self.library.binding.problem(),
            filter_error: self.library.filter_error.as_ref(),
            read_failure: self.library.read_failure.as_ref(),
            selected: self.selected(),
            selected_entry: self
                .library
                .selection
                .as_ref()
                .and_then(|key| self.library.entries.iter().find(|e| &e.key == key)),
            detached: self.editor.location.detached(),
            external: self.editor.location.external(),
            managed: self.editor.location.managed(),
        }
    }
}

/// Selected indexed identity for a captured frontend action; workers still revalidate it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionBinding<'a> {
    /// Current runtime ownership key; temporary identities remain process-local.
    pub key: &'a LibraryKey,
    /// Exact observed content token, absent for temporary or unreadable entries.
    pub version_token: Option<VersionToken>,
    /// Directory-owned Trash state from the current index.
    pub trashed: bool,
}
