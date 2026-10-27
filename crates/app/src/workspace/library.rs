// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Managed-library state and immutable worker requests. Only selected User content
//! is retained in full; index rows are disposable summaries.
use super::*;
use crate::core::query::Scope;
use crate::discovery::collection::{CatalogQuery, EntrySummary};
use crate::services::library::Library;
use crate::storage::library::{
    Inventory, LibraryKey, LoadedRecord, Mutation, MutationKind, VersionToken, WriteOutcome,
};

/// A real configured library capability or a typed configuration failure.
#[derive(Debug)]
pub enum LibraryBinding {
    /// A persistent store whose filesystem work is performed by jobs.
    Store(Library),
    /// Invalid/unavailable configuration; Built-ins remain usable.
    Unavailable(crate::storage::library::LibraryDiagnostic),
}
impl LibraryBinding {
    /// Resolve native configuration outside the sans-IO coordinator.
    ///
    /// # Errors
    /// Refuses invalid shipped identity metadata while retaining configuration failures as data.
    pub fn configured() -> Result<Self, crate::error::LibraryError> {
        match crate::services::library::Library::resolve(None) {
            Ok(library) => Ok(Self::Store(library)),
            Err(
                error @ (crate::error::LibraryError::InvalidBuiltin { .. }
                | crate::error::LibraryError::DuplicateBuiltin { .. }),
            ) => Err(error),
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
    pub(super) fn store(&self) -> Option<&Library> {
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
            | FileStatus::Failed(error) => Some(
                crate::services::library::DiagnosticDetail::Failure(Box::new(error.clone())),
            ),
            FileStatus::WorkerFailed { error, .. } => Some(
                crate::services::library::DiagnosticDetail::Notice(error.description()),
            ),
            _ => None,
        };
        if let Some(message) = cause {
            let storage_id = match &job.operation {
                Operation::Load(key) => {
                    self.unreadable.insert(key.clone());
                    key.to_string()
                }
                _ => job.store.path().display().to_string(),
            };
            self.read_failure = Some(crate::storage::library::LibraryDiagnostic {
                storage_id,
                code: "read_failure".into(),
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
            crate::document::recipe::Origin::BuiltIn(id) => Some(LibraryKey::BuiltIn((*id).into())),
            crate::document::recipe::Origin::User { .. } => None,
            _ => Some(crate::services::library::ExternalId::allocate().map(LibraryKey::External)?),
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
    pub(super) store: Library,
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
            _ => self.store.path().to_owned(),
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
            } => crate::services::save_external(&expected, recipe, &Cancel::default())
                .map(|r| LibraryOutcome::ExternalWritten(Box::new(r)))
                .map_err(Error::from),
            Operation::Scan => Ok(LibraryOutcome::Inventory(
                self.store.scan(&Cancel::default()),
            )),
            Operation::Initialize => {
                let initialized = self.store.initialize()?;
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
        store: Library,
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
        self.library.set_query(query)?;
        self.load_library_selection();
        self.changed();
        Ok(())
    }
    /// Select only a key that still belongs to the rendered query. Old row
    /// positions and stale callbacks cannot address replacement entries.
    pub fn select_library_key(&mut self, key: &str) {
        self.library.select_key(key);
        self.load_library_selection();
        self.changed();
    }
    /// Keyboard movement through unique visible sounds, skipping virtual headings.
    pub fn navigate_library(&mut self, direction: i32) {
        self.library.navigate(direction);
        self.load_library_selection();
        self.changed();
    }
    /// Request a rescan without dropping stale rows or the open draft on failure.
    pub fn refresh_library(&mut self) {
        self.library.request_refresh();
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
        let recipe = self.prepare_add_to_library()?;
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
        if action == "trash" {
            self.trash_request_admission(&key)?;
        }
        if action == "trash" && self.editor.dirty_managed(&key) {
            self.request(DocumentAction::TrashSaved { key, expect });
            return Ok(());
        }
        if action == "purge" && !confirmed {
            self.library_job_admission()?;
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

    /// Root location and current inventory diagnostics.
    #[must_use]
    pub fn library_status(&self) -> String {
        match &self.library.binding {
            LibraryBinding::Store(store) => format!(
                "{}\n{}{}",
                store.path().display(),
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
        if self.files.slot.is_some() || self.prompts.current.is_some() || self.pending.is_some() {
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
        let _ = self.managed_job(Operation::Load(key));
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
                crate::services::builtin_source(id)
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
    ) -> Result<(), crate::error::QueryError> {
        query.execute(&self.entries)?;
        self.filter_error = None;
        self.query = query;
        self.refilter();
        Ok(())
    }
    pub(super) fn select_key(&mut self, key: &str) {
        if let Some(found) = self.visible.iter().find(|entry| entry.to_string() == key) {
            self.select(Some(found.clone()));
            self.revision += 1;
        }
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
        self.root_unavailable = inventory.diagnostics.iter().any(|d| d.code == "root");
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
    fn job(&self, operation: Operation) -> Result<LibraryJob, Refusal> {
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
    /// The search text.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.library.query.search
    }
    /// Search titles, IDs, research text and component meaning. When the
    /// selection stops matching, the first match is selected.
    pub fn set_query(&mut self, query: &str) {
        self.library.set_search(query);
        self.load_library_selection();
        self.changed();
    }
    /// Change nominal facets without rebinding any playback or editor snapshot.
    pub fn set_rhythm_query(&mut self, query: crate::discovery::query::RhythmQuery) {
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
