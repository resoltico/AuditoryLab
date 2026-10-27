// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Managed-library state and immutable worker requests. Only selected User content
//! is retained in full; index rows are disposable summaries.
use super::*;
use crate::discovery::collection::{CatalogQuery, EntrySummary, Scope};
use crate::storage::library::{
    Inventory, LibraryKey, LibraryStore, LoadedRecord, Mutation, MutationKind, VersionToken,
    WriteOutcome,
};

/// A real configured library capability or a typed configuration failure.
#[derive(Debug)]
pub enum LibraryBinding {
    /// A persistent store whose filesystem work is performed by jobs.
    Store(LibraryStore),
    /// Invalid/unavailable configuration; Built-ins remain usable.
    Unavailable(crate::storage::library::LibraryDiagnostic),
}
impl LibraryBinding {
    /// Resolve native configuration outside the sans-IO coordinator.
    ///
    /// # Errors
    /// Refuses invalid shipped identity metadata while retaining configuration failures as data.
    pub fn configured() -> Result<Self, crate::error::LibraryError> {
        match crate::storage::library::LibraryRoot::resolve(None) {
            Ok(root) => LibraryStore::new(root).map(Self::Store),
            Err(error) => Ok(Self::Unavailable(
                crate::storage::library::LibraryDiagnostic {
                    storage_id: "configured root".into(),
                    code: "configuration".into(),
                    detail: crate::storage::library::DiagnosticDetail::Failure(Box::new(
                        error.into(),
                    )),
                },
            )),
        }
    }
    pub(super) fn store(&self) -> Option<&LibraryStore> {
        match self {
            Self::Store(store) => Some(store),
            Self::Unavailable(_) => None,
        }
    }
    pub(super) fn problem(&self) -> Option<&crate::storage::library::LibraryDiagnostic> {
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
    snapshots: Vec<(LibraryKey, crate::document::recipe::SnapshotId)>,
    pub(super) query: CatalogQuery,
    visible: Vec<LibraryKey>,
    selection: Option<LibraryKey>,
    loaded: Option<LoadedRecord>,
    refresh: bool,
    load_attempted: Option<LibraryKey>,
    inventory: Arc<InventoryStatus>,
    read_failure: Option<crate::storage::library::LibraryDiagnostic>,
    filter_error: Option<crate::error::QueryError>,
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
            self.read_failure = Some(crate::storage::library::LibraryDiagnostic {
                storage_id,
                code: "read_failure".into(),
                detail: crate::storage::library::DiagnosticDetail::Notice(message),
            });
            Arc::make_mut(&mut self.inventory).complete = false;
            self.revision += 1;
        } else if self.read_failure.take().is_some() {
            self.revision += 1;
        }
    }
    fn register(&mut self, snapshot: &RecipeSnapshot) -> Result<(), Error> {
        let key = match snapshot.origin() {
            crate::document::recipe::Origin::BuiltIn(id) => LibraryKey::BuiltIn((*id).into()),
            crate::document::recipe::Origin::User { .. } => return Ok(()),
            _ => crate::storage::library::ExternalId::allocate().map(LibraryKey::External)?,
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
        crate::presentation::library_text::rows(
            &self.query,
            &self.entries,
            &self.inventory.diagnostics,
            self.selection.as_ref(),
            |key| self.requires_read(key),
        )
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
        expected: crate::storage::external::ObservedFile,
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
                .map(|r| LibraryOutcome::Written(Box::new(r)))
                .map_err(Error::from),
            Operation::ExternalSave {
                expected, recipe, ..
            } => crate::storage::external::ExternalWriter
                .save(&expected, recipe, &Cancel::default())
                .map(|r| LibraryOutcome::ExternalWritten(Box::new(r)))
                .map_err(Error::from),
            Operation::Scan => Ok(LibraryOutcome::Inventory(
                self.store.scan(&Cancel::default()),
            )),
            Operation::Initialize => {
                let initialized = self.store.root().initialize()?;
                let mut inventory = self.store.scan(&Cancel::default());
                inventory
                    .diagnostics
                    .extend(initialized.warnings.into_iter().map(|message| {
                        crate::storage::library::LibraryDiagnostic {
                            storage_id: "library.toml".into(),
                            code: "initialization_warning".into(),
                            detail: crate::storage::library::DiagnosticDetail::Notice(message),
                        }
                    }));
                Ok(LibraryOutcome::Inventory(inventory))
            }
            Operation::Load(key) => self
                .store
                .load(&key)
                .map(|r| LibraryOutcome::Loaded(Box::new(r)))
                .map_err(Error::from),
            Operation::Write { mutation, .. } => self
                .store
                .apply(mutation, &Cancel::default())
                .map(|r| LibraryOutcome::Written(Box::new(r)))
                .map_err(Error::from),
        }
    }
}
impl Workspace {
    /// Incomplete storage state remains visible outside the storage disclosure.
    #[must_use]
    pub fn library_alert(&self) -> String {
        crate::presentation::library_text::alert(
            self.library.binding.problem(),
            self.library.filter_error.as_ref(),
            self.library.read_failure.as_ref(),
            self.library.inventory.complete,
        )
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
            let key = match crate::storage::library::ExternalId::allocate() {
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
    pub fn library_filter_error(&mut self, error: crate::error::QueryError) {
        self.library.filter_error = Some(error);
        self.library.revision += 1;
        self.changed();
    }
    /// Disjunctive unique counts from the shared query, with reference intervals.
    #[must_use]
    pub fn library_facets(&self) -> (Vec<LibraryFacetView>, Vec<String>) {
        crate::presentation::library_text::facets(
            &self.library.query,
            &self.library.entries,
            self.library.inventory.complete,
        )
    }
    /// Concise authoritative query summary for the results header.
    #[must_use]
    pub fn library_overview(&self) -> String {
        crate::presentation::library_text::overview(
            &self.library.query,
            self.library.visible.len(),
            self.library.inventory.complete,
        )
    }
    /// Unique counts and active restrictions; group repeats are never counted as sounds.
    #[must_use]
    pub fn library_count(&self) -> String {
        crate::presentation::library_text::count(
            &self.library.query,
            self.library.visible.len(),
            self.library.inventory.complete,
        )
    }
    /// Explain an empty result without treating an incomplete scan as an empty collection.
    #[must_use]
    pub fn library_empty_message(&self) -> &'static str {
        crate::presentation::library_text::empty_message(
            &self.library.query,
            &self.library.entries,
            self.library.visible.len(),
            self.library.inventory.complete,
        )
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
    pub fn set_catalog_query(
        &mut self,
        query: CatalogQuery,
    ) -> Result<(), crate::error::QueryError> {
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
            crate::document::recipe::Origin::Draft(self.editor.session.draft().title().into()),
            (*content).clone(),
        );
        let draft =
            Draft::from_snapshot(&snapshot).map_err(|e| Refusal::Document(Box::new(e.into())))?;

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
            return Err(Refusal::SelectionChanged);
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
            crate::document::recipe::Origin::File(_) | crate::document::recipe::Origin::Stream(_)
        ) {
            return Err(Refusal::TemporaryRequired);
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
            .map_err(|e: crate::error::LibraryError| Refusal::Document(Box::new(e.into())))?;
        key.require_user()
            .map_err(|e| Refusal::Document(Box::new(e.into())))?;
        let expect: VersionToken = token
            .parse()
            .map_err(|e: crate::error::LibraryError| Refusal::Document(Box::new(e.into())))?;
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
            .ok_or(Refusal::PurgeSelectionChanged)?;
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
            _ => return Err(Refusal::UnknownLibraryAction(action.into())),
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
        self.library.query.rhythm = crate::discovery::query::RhythmQuery::default();
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
                        problem.storage_id,
                        problem.message()
                    ))
            ),
            LibraryBinding::Unavailable(problem) => problem.message(),
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
        let entry = self
            .library
            .selection
            .as_ref()
            .and_then(|key| self.library.entries.iter().find(|entry| &entry.key == key));
        crate::presentation::library_text::dates(entry, self.selected(), utc)
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
            return Err(Refusal::DetachedDraft(key.clone()));
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
        editor: Result<EditorSession, crate::error::EditorError>,
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
        crate::presentation::library_text::document_location(
            self.library.binding.problem(),
            self.editor.location.detached(),
            self.editor.location.external(),
            self.editor.location.managed(),
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
