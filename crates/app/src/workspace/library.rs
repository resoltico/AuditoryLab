//! Managed-library state and immutable worker requests. Only selected User content
//! is retained in full; index rows are disposable summaries.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::{
    Arc, AudioRequest, Cancel, DocumentId, DocumentLocation, Error, FacetCounts, FacetKind,
    FileStatus, InventoryStatus, LibraryFacetView, LibraryRowView, LibraryStatus, LibraryView,
    Recipe, RecipeSnapshot, Refusal, SampleRate, file, status,
};
use crate::discovery::collection::{CatalogQuery, EntrySummary};
use crate::discovery::query::Scope;
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
    pub(super) fn from_library(library: Library) -> Self {
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
    binding: LibraryBinding,
    catalog: Vec<RecipeSnapshot>,
    cache: std::cell::RefCell<Option<(LibraryViewKey, Arc<LibraryView>)>>,
    #[cfg(test)]
    evaluations: std::cell::Cell<u64>,
    entries: Vec<EntrySummary>,
    snapshots: Vec<(LibraryKey, crate::document::recipe::SnapshotId)>,
    query: CatalogQuery,
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
    revision: u64,
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

    pub(super) fn requires_read(&self, key: &LibraryKey) -> bool {
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
            .filter(|problem| {
                problem
                    .storage_id
                    .rsplit_once('.')
                    .is_some_and(|(_, suffix)| suffix == "toml")
            })
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

    pub(super) fn accept_record(&mut self, record: LoadedRecord) {
        self.unreadable.remove(&record.key);
        self.stale.remove(&record.key);
        self.root_unavailable = false;
        self.entries.retain(|e| e.key != record.key);
        self.entries.push(record.summary());
        self.recount();
        self.refilter();
        if self.selection.as_ref() == Some(&record.key) {
            self.loaded = Some(record);
        }
    }

    pub(super) fn save_destination_admission(&self) -> Result<(), Refusal> {
        self.binding.problem().map_or(Ok(()), |problem| {
            Err(Refusal::LibraryUnavailable(problem.clone()))
        })
    }
    /// Writing a recipe also needs a set-up folder: a configured folder that
    /// exists without being initialized needs Initialize first, which this
    /// check leaves available. A missing default root is created by its first save.
    pub(super) fn record_destination_admission(&self) -> Result<(), Refusal> {
        use crate::services::library::{DiagnosticDetail, LibraryError};
        self.save_destination_admission()?;
        self.inventory
            .diagnostics
            .iter()
            .find(|problem| {
                matches!(&problem.detail, DiagnosticDetail::Failure(error)
                    if matches!(**error, LibraryError::NotInitialized))
            })
            .map_or(Ok(()), |problem| {
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

    pub(super) fn selected_trashed(&self) -> bool {
        {
            self.loaded
                .as_ref()
                .filter(|r| Some(&r.key) == self.selection.as_ref())
        }
        .is_some_and(|r| r.trashed)
    }

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
    pub(super) fn job(&self, operation: LibraryRequest) -> Result<LibraryJob, Refusal> {
        self.save_destination_admission()?;
        let store = self
            .store_clone()
            .unwrap_or_else(|| unreachable!("admitted library binding has a service"));
        Ok(LibraryJob { store, operation })
    }
    /// Keep the storage count in step with records this workspace wrote,
    /// so it never contradicts the listed entries between scans.
    fn recount(&mut self) {
        let count = self
            .entries
            .iter()
            .filter(|entry| matches!(entry.key, LibraryKey::User(_)))
            .count();
        if self.inventory.count != count || self.inventory.not_initialized {
            let mut inventory = (*self.inventory).clone();
            inventory.count = count;
            inventory.not_initialized = false;
            self.inventory = Arc::new(inventory);
        }
    }
    pub(super) fn remove_entry(&mut self, key: &LibraryKey) {
        self.entries.retain(|entry| &entry.key != key);
        self.recount();
        if self
            .loaded
            .as_ref()
            .is_some_and(|record| &record.key == key)
        {
            self.loaded = None;
        }
        self.refilter();
    }

    pub(super) fn set_search(&mut self, search: &str) {
        self.query.search = search.into();
        self.refilter();
    }
    pub(super) fn set_rhythm(&mut self, rhythm: crate::discovery::query::RhythmQuery) {
        self.query.rhythm = rhythm;
        self.refilter();
    }

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

    pub(super) fn handle_for_key(&self, key: &LibraryKey) -> Option<EntryHandle> {
        self.visible
            .contains(key)
            .then(|| self.entries.iter().find(|entry| &entry.key == key))
            .flatten()
            .map(|entry| self.entry_handle(entry))
    }

    pub(super) fn selected_entry_handle(&self) -> Option<EntryHandle> {
        self.selection
            .as_ref()
            .and_then(|key| self.handle_for_key(key))
    }

    pub(super) fn resolve_entry_handle(
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
        if !Arc::ptr_eq(&self.listing, &handle.listing) || !self.visible.contains(&handle.key) {
            return Err(refusal());
        }
        let current = self
            .entries
            .iter()
            .find(|entry| entry.key == handle.key)
            .ok_or_else(refusal)?;
        if current.version_token != handle.version || current.trashed != handle.trashed {
            return Err(refusal());
        }
        Ok((handle.key.clone(), handle.version, handle.trashed))
    }

    pub(super) fn library_facets(&self) -> FacetCounts {
        use crate::core::rhythm::{Band, Method};
        use crate::discovery::query::RhythmState;
        let complete = self.inventory.complete;
        let facet = |query: &CatalogQuery, kind| LibraryFacetView {
            kind,
            count: query.count(&self.entries).unwrap_or(0),
            complete,
        };
        let mut query = self.query.clone();
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
        let mut query = self.query.clone();
        query.rhythm.methods.clear();
        let mut methods = vec![facet(&query, FacetKind::AllMethods)];
        for method in Method::ALL {
            let mut q = query.clone();
            q.rhythm.methods = vec![method];
            methods.push(facet(&q, FacetKind::Method(method)));
        }
        FacetCounts { bands, methods }
    }

    pub(super) fn library_binding(&self) -> Option<SelectionBinding<'_>> {
        let p = &self;
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

    pub(super) fn view(&self, rate: SampleRate) -> Arc<LibraryView> {
        let key = LibraryViewKey {
            revision: self.revision,
            selected: self.selected().map(RecipeSnapshot::id),
            rate,
        };
        if let Some((cached, view)) = &*self.cache.borrow()
            && *cached == key
        {
            return view.clone();
        }
        #[cfg(test)]
        self.evaluations.set(self.evaluations.get() + 1);
        let view = Arc::new(self.build_library_view(rate));
        *self.cache.borrow_mut() = Some((key, view.clone()));
        view
    }

    fn build_library_view(&self, rate: SampleRate) -> LibraryView {
        let (rows, selected) = self.rows();
        LibraryView {
            query: self.query.clone(),
            rows,
            selected,
            snapshot: self.selected().cloned(),
            rate,
            sort: self.query.effective_sort().0,
        }
    }

    pub(super) fn reveal_retained_source(&mut self, snapshot: RecipeSnapshot) -> Result<(), Error> {
        let p = &mut *self;
        let key = if let Some((key, _)) = p.snapshots.iter().find(|(_, id)| *id == snapshot.id()) {
            key.clone()
        } else {
            let key = {
                let id = crate::document::origin::ExternalId::allocate()?;
                LibraryKey::External(id)
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
        self.query.search.clear();
        self.query.rhythm = crate::discovery::query::RhythmQuery::default();
        Ok(())
    }

    pub(super) fn library_filter_error(&mut self, error: crate::discovery::query::QueryError) {
        self.filter_error = Some(error);
        self.revision += 1;
    }

    pub(super) fn close_open_file(&mut self) -> bool {
        let p = &mut *self;
        let Some(key @ LibraryKey::External(_)) = p.selection.clone() else {
            return false;
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
        true
    }

    pub(super) fn show_saved(&mut self) -> Result<bool, ()> {
        let p = &mut *self;
        let Some(key) = p.last_saved.clone() else {
            return Ok(false);
        };
        let Some(entry) = p.entries.iter().find(|e| e.key == key) else {
            return Err(());
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
        self.query.search.clear();
        self.query.rhythm = crate::discovery::query::RhythmQuery::default();
        Ok(true)
    }

    pub(super) fn load_request(&mut self) -> Option<LibraryRequest> {
        let p = &mut *self;
        let Some(key @ LibraryKey::User(_)) = p.selection.clone() else {
            return None;
        };
        if p.load_attempted.as_ref() == Some(&key) {
            return None;
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
            return None;
        }
        p.load_attempted = Some(key.clone());
        Some(LibraryRequest::Load(key))
    }

    pub(super) fn replace_external(
        &mut self,
        previous: &std::path::Path,
        snapshot: RecipeSnapshot,
    ) {
        let p = &mut *self;
        // Replace only the selected external observation's index, keeping its key.
        let path = previous;
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
    }

    pub(super) fn library_data<'a>(&'a self, location: &'a DocumentLocation) -> LibraryData<'a> {
        LibraryData {
            query: &self.query,
            entries: &self.entries,
            visible: self.visible.len(),
            inventory: &self.inventory,
            path: self.binding.store().map(Library::path),
            problem: self.binding.problem(),
            filter_error: self.filter_error.as_ref(),
            read_failure: self.read_failure.as_ref(),
            selected: self.selected(),
            selected_entry: self
                .selection
                .as_ref()
                .and_then(|key| self.entries.iter().find(|e| &e.key == key)),
            detached: location.detached(),
            external: location.external(),
            managed: location.managed(),
        }
    }

    pub(super) fn catalog(&self) -> &[RecipeSnapshot] {
        &self.catalog
    }
    pub(super) const fn query(&self) -> &CatalogQuery {
        &self.query
    }
    pub(super) fn loaded(&self) -> Option<&LoadedRecord> {
        self.loaded.as_ref()
    }
    pub(super) fn entries(&self) -> &[EntrySummary] {
        &self.entries
    }
    pub(super) fn take_refresh(&mut self) -> bool {
        std::mem::take(&mut self.refresh)
    }
    pub(super) fn mark_saved(&mut self, key: LibraryKey) {
        self.last_saved = Some(key);
    }
    pub(super) fn select_key(&mut self, key: LibraryKey) {
        self.select(Some(key));
        self.revision += 1;
    }

    pub(super) fn apply(
        &mut self,
        command: LibraryInput,
    ) -> Result<(), crate::discovery::query::QueryError> {
        match command {
            LibraryInput::Query(query) => self.set_query(*query)?,
            LibraryInput::Search(text) => self.set_search(&text),
            LibraryInput::Rhythm(query) => self.set_rhythm(query),
            LibraryInput::Navigate(direction) => self.navigate(direction),
            LibraryInput::Refresh => self.request_refresh(),
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LibraryJob {
    pub(super) store: Library,
    operation: LibraryRequest,
}
#[derive(Debug, Clone)]
pub(super) enum LibraryRequest {
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
    pub(super) fn operation_term(&self) -> crate::vocabulary::Operation {
        use crate::vocabulary::Operation as Term;
        match &self.operation {
            LibraryRequest::Scan => Term::LibraryRefresh,
            LibraryRequest::Initialize => Term::LibraryInit,
            LibraryRequest::Load(_) => Term::LibraryRead,
            LibraryRequest::DiscardTrash { .. } => Term::LibraryTrash,
            LibraryRequest::ExternalSave { .. }
            | LibraryRequest::Write {
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
                && matches!(record.snapshot.origin(),crate::document::recipe::Origin::User {id,
                    version} if key==&LibraryKey::User(*id) && version==&record.version_token)
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
            } => crate::services::save_external(&expected, &(recipe), &Cancel::default())
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

#[derive(Debug, PartialEq, Eq)]
struct LibraryViewKey {
    revision: u64,
    selected: Option<crate::document::recipe::SnapshotId>,
    rate: SampleRate,
}

impl LibraryJob {
    pub(super) fn document(&self) -> Option<DocumentId> {
        match self.operation {
            LibraryRequest::Write { document, .. } => document,
            LibraryRequest::ExternalSave { document, .. } => Some(document),
            _ => None,
        }
    }
    pub(super) fn discard_revision(&self) -> Option<(DocumentId, u64, u64)> {
        match self.operation {
            LibraryRequest::DiscardTrash {
                document,
                draft,
                input,
                ..
            } => Some((document, draft, input)),
            _ => None,
        }
    }
}

#[cfg(test)]
impl LibraryState {
    pub(super) fn evaluations(&self) -> u64 {
        self.evaluations.get()
    }
    pub(super) fn replace_binding(&mut self, binding: LibraryBinding) {
        self.binding = binding;
    }
}

pub(super) enum LibraryInput {
    Query(Box<CatalogQuery>),
    Search(String),
    Rhythm(crate::discovery::query::RhythmQuery),
    Navigate(i32),
    Refresh,
}

#[cfg(test)]
mod tests;
