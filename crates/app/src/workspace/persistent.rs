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

#[derive(Debug)]
pub(super) struct Persistent {
    store: LibraryStore,
    entries: Vec<EntrySummary>,
    snapshots: Vec<(LibraryKey, crate::recipe::SnapshotId)>,
    pub(super) query: CatalogQuery,
    visible: Vec<LibraryKey>,
    selection: Option<LibraryKey>,
    loaded: Option<LoadedRecord>,
    pub(super) location: Option<(LibraryKey, VersionToken)>,
    pub(super) detached: Option<LibraryKey>,
    pub(super) external: Option<crate::external::ObservedFile>,
    refresh: bool,
    load_attempted: Option<LibraryKey>,
    diagnostics: String,
    complete: bool,
    unreadable: std::collections::BTreeSet<LibraryKey>,
    stale: std::collections::BTreeSet<LibraryKey>,
    root_unavailable: bool,
    record_diagnostics: Vec<String>,
    pub(super) revision: u64,
    last_saved: Option<LibraryKey>,
}
impl Persistent {
    fn new(store: LibraryStore, catalog: &[RecipeSnapshot]) -> Result<Self, Error> {
        let mut result = Self {
            store,
            entries: Vec::new(),
            snapshots: Vec::new(),
            query: CatalogQuery::default(),
            visible: Vec::new(),
            selection: None,
            loaded: None,
            location: None,
            detached: None,
            external: None,
            refresh: true,
            load_attempted: None,
            diagnostics: String::new(),
            complete: true,
            unreadable: std::collections::BTreeSet::new(),
            stale: std::collections::BTreeSet::new(),
            root_unavailable: false,
            record_diagnostics: Vec::new(),
            revision: 0,
            last_saved: None,
        };
        for snapshot in catalog {
            result.register(snapshot)?;
        }
        result.refilter();
        Ok(result)
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

    pub(super) fn saved_token(&self, key: &LibraryKey) -> Option<VersionToken> {
        self.location
            .as_ref()
            .filter(|(k, _)| k == key)
            .map(|(_, token)| *token)
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
    pub(super) fn store_clone(&self) -> LibraryStore {
        self.store.clone()
    }
    pub(super) fn selected_text(&self) -> Option<Arc<str>> {
        self.loaded
            .as_ref()
            .filter(|r| Some(&r.key) == self.selection.as_ref())
            .map(|r| r.recipe_toml.clone())
    }
    pub(super) fn visible_key(&self, position: usize) -> Option<String> {
        self.visible.get(position).map(ToString::to_string)
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
            self.selection = self.visible.first().cloned();
        }
        self.revision += 1;
    }
    pub(super) fn selected<'a>(
        &'a self,
        catalog: &'a [RecipeSnapshot],
    ) -> Option<&'a RecipeSnapshot> {
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
        catalog.iter().find(|s| s.id() == *id)
    }
    pub(super) fn rows(&self) -> (Vec<String>, Vec<String>, Option<usize>) {
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
            format!(
                "{} · {}{}\n{}\nDuration {}\n{}",
                e.title.display_scalar(),
                match e.key {
                    LibraryKey::BuiltIn(_) => "Built-in · Read only",
                    LibraryKey::User(_) if e.trashed => "Trash",
                    LibraryKey::User(_) => "User",
                    LibraryKey::External(_) => "Open file",
                },
                if self.requires_read(&e.key) {
                    " · Last observed · Needs review"
                } else {
                    ""
                },
                e.nominal.summary(),
                NumberStyle::Duration.format(e.duration_seconds),
                dates,
            )
        };
        let mut keys = Vec::new();
        let mut rows = Vec::new();
        if let Ok(result) = self.query.execute(&self.entries) {
            if result.groups.is_empty() {
                for e in result.entries {
                    keys.push(e.key.to_string());
                    rows.push(label(e));
                }
            } else {
                for group in result.groups {
                    keys.push(String::new());
                    rows.push(format!(
                        "{} · {} unique sounds",
                        group.label,
                        group.keys.len()
                    ));
                    for key in group.keys {
                        if let Some(e) = self.entries.iter().find(|e| e.key == key) {
                            keys.push(key.to_string());
                            rows.push(label(e));
                        }
                    }
                }
            }
        }
        if !matches!(
            self.query.scope,
            Scope::BuiltIn | Scope::OpenFiles | Scope::Trash
        ) && !self.record_diagnostics.is_empty()
        {
            keys.push(String::new());
            rows.push("Unreadable storage entries · not included in sound counts".into());
            for diagnostic in &self.record_diagnostics {
                keys.push(String::new());
                rows.push(diagnostic.clone());
            }
        }
        let selected = self
            .selection
            .as_ref()
            .and_then(|key| keys.iter().position(|k| k == &key.to_string()));
        (keys, rows, selected)
    }
    fn accept_record(&mut self, record: LoadedRecord) {
        self.unreadable.remove(&record.key);
        self.stale.remove(&record.key);
        self.root_unavailable = false;
        self.entries.retain(|e| e.key != record.key);
        self.entries.push(record.summary());
        self.loaded = Some(record);
        self.refilter();
    }
}

#[derive(Debug, Clone)]
pub(super) struct Job {
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
pub(super) enum ResultValue {
    Inventory(Inventory),
    ExternalWritten(Box<crate::external::ExternalOutcome>),
    Loaded(Box<LoadedRecord>),
    Written(Box<WriteOutcome>),
}
impl Job {
    pub(super) fn may_publish(&self) -> bool {
        matches!(
            self.operation,
            Operation::Write { .. }
                | Operation::ExternalSave { .. }
                | Operation::DiscardTrash { .. }
        )
    }

    pub(super) fn execute(self) -> Result<ResultValue, Error> {
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
                .map(|r| ResultValue::Written(Box::new(r))),
            Operation::ExternalSave {
                expected, recipe, ..
            } => crate::external::ExternalWriter
                .save(&expected, recipe, &Cancel::default())
                .map(|r| ResultValue::ExternalWritten(Box::new(r))),
            Operation::Scan => Ok(ResultValue::Inventory(self.store.scan(&Cancel::default()))),
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
                Ok(ResultValue::Inventory(inventory))
            }
            Operation::Load(key) => self
                .store
                .load(&key)
                .map(|r| ResultValue::Loaded(Box::new(r))),
            Operation::Write { mutation, .. } => self
                .store
                .apply(mutation, &Cancel::default())
                .map(|r| ResultValue::Written(Box::new(r))),
        }
    }
}
impl Workspace {
    pub(super) fn missing_file_result(&self, _job: &Job) -> String {
        "File worker exited without a result. Draft retained; refresh and inspect the affected path before retrying. A missing completion does not establish that storage was unchanged.".into()
    }

    /// Resolve the production root without falling back to external saves when
    /// configuration fails. Built-ins remain visible with an actionable banner.
    ///
    /// # Errors
    /// Returns document/catalog identity allocation failures.
    pub fn configured_library(
        catalog: Vec<RecipeSnapshot>,
        capabilities: Capabilities,
    ) -> Result<Self, Error> {
        match crate::library::LibraryRoot::resolve(None).and_then(LibraryStore::new) {
            Ok(store) => Self::with_library(catalog, capabilities, store),
            Err(error) => {
                let mut workspace = Self::new(catalog, capabilities)?;
                workspace.persistent_config_error = Some(error.to_string());
                workspace.file_status = format!(
                    "User Library unavailable: {error}. Built-ins remain available; configure an absolute AUDITORYLAB_LIBRARY_DIR and restart."
                );
                Ok(workspace)
            }
        }
    }
    /// Incomplete storage state remains visible outside the storage disclosure.
    #[must_use]
    pub fn library_alert(&self) -> String {
        if let Some(error) = &self.persistent_config_error {
            return format!(
                "User Library unavailable: {error}. Configure an absolute AUDITORYLAB_LIBRARY_DIR and restart; drafts are retained."
            );
        }
        self.persistent.as_ref().map_or(String::new(),|p| {
            if !p.complete { return "Library inventory incomplete · previous unseen rows are last observed snapshots. Refresh before acting; no empty-library claim is implied.".into(); }
            String::new()
        })
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
        let persistent = Persistent::new(store, &catalog)?;
        let mut workspace = Self::new(catalog, capabilities)?;
        workspace.persistent = Some(persistent);
        workspace.continue_persistent();
        Ok(workspace)
    }
    pub(super) fn reveal_retained_source(&mut self, snapshot: RecipeSnapshot) {
        let Some(p) = &mut self.persistent else {
            return;
        };
        let key = if let Some((key, _)) = p.snapshots.iter().find(|(_, id)| *id == snapshot.id()) {
            key.clone()
        } else {
            let key = match crate::library::ExternalId::allocate() {
                Ok(id) => LibraryKey::External(id),
                Err(error) => {
                    self.file_status = error.to_string();
                    self.changed();
                    return;
                }
            };
            p.snapshots.push((key.clone(), snapshot.id()));
            p.entries
                .push(EntrySummary::from_snapshot(key.clone(), &snapshot, None));
            self.catalog.push(snapshot);
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
        p.selection = Some(key);
        self.query.clear();
        self.rhythm_query = crate::query::RhythmQuery::default();
        self.view = View::Library;
        self.effects.push(Effect::ShowView(View::Library));
        self.file_status="Showing the retained source snapshot; filters explicitly cleared. This does not reload or reconstruct an ancestor.".into();
        self.changed();
    }
    /// Surface a native filter error while retaining the prior valid query.
    pub fn library_filter_error(&mut self, message: &str) {
        self.file_status = message.into();
        self.changed();
    }
    /// Disjunctive unique counts from the shared query, with reference intervals.
    #[must_use]
    pub fn library_facet_labels(&self) -> (Vec<String>, Vec<String>) {
        use crate::core::nominal::{Band, Method};
        use crate::query::RhythmState;
        let count = |query: &CatalogQuery| {
            self.persistent
                .as_ref()
                .map(|p| query.count(&p.entries).unwrap_or(0))
        };
        let base = self.catalog_query().cloned().unwrap_or_default();
        let suffix = |count: Option<usize>| count.map_or(String::new(), |n| format!(" · {n}"));
        let mut unrestricted = base.clone();
        unrestricted.rhythm.bands.clear();
        unrestricted.rhythm.state = None;
        let mut bands = vec![format!("All sounds{}", suffix(count(&unrestricted)))];
        for band in Band::ALL {
            let mut query = unrestricted.clone();
            query.rhythm.bands = vec![band];
            bands.push(format!(
                "{band} · {}{}",
                band.interval(),
                suffix(count(&query))
            ));
        }
        for state in [
            RhythmState::OutsideBands,
            RhythmState::None,
            RhythmState::Multiple,
            RhythmState::NeedsReview,
        ] {
            let label = state.label();
            let mut query = unrestricted.clone();
            query.rhythm.state = Some(state);
            bands.push(format!("{label}{}", suffix(count(&query))));
        }
        let mut unrestricted = base.clone();
        unrestricted.rhythm.methods.clear();
        let mut methods = vec![format!("All methods{}", suffix(count(&unrestricted)))];
        for method in Method::ALL {
            let mut query = unrestricted.clone();
            query.rhythm.methods = vec![method];
            methods.push(format!("{}{}", method.key(), suffix(count(&query))));
        }
        (bands, methods)
    }
    /// Unique counts and active restrictions; group repeats are never counted as sounds.
    #[must_use]
    pub fn library_count(&self) -> String {
        self.persistent.as_ref().map_or(String::new(), |p| {
            let q = &p.query;
            let (sort, direction) = q.effective_sort();
            let count = if p.complete {
                format!("{} unique sounds", p.visible.len())
            } else {
                format!(
                    "{} observed unique sounds · Inventory incomplete",
                    p.visible.len()
                )
            };
            let mut lines = vec![format!(
                "{} · {} · Sort {} {} · Group {}",
                count,
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
        })
    }
    /// Current persistent query; native filters and the CLI use the same contract.
    #[must_use]
    pub fn catalog_query(&self) -> Option<&CatalogQuery> {
        self.persistent.as_ref().map(|p| &p.query)
    }
    /// Replace valid collection, date, evidence and sorting restrictions together.
    ///
    /// # Errors
    /// An invalid range leaves the previous query and selection intact.
    pub fn set_catalog_query(&mut self, query: CatalogQuery) -> Result<(), Error> {
        if let Some(p) = &mut self.persistent {
            query.execute(&p.entries)?;
            self.query.clone_from(&query.search);
            self.rhythm_query.clone_from(&query.rhythm);
            p.query = query;
            p.refilter();
            self.load_persistent_selection();
            self.changed();
        }
        Ok(())
    }
    /// Select only a key that still belongs to the rendered query. Old row
    /// positions and stale callbacks cannot address replacement entries.
    pub fn select_library_key(&mut self, key: &str) {
        if let Some(p) = &mut self.persistent {
            if let Some(found) = p.visible.iter().find(|k| k.to_string() == key) {
                p.selection = Some(found.clone());
                p.revision += 1;
            }
            self.load_persistent_selection();
            self.changed();
        } else if let Some(id) = key
            .strip_prefix("snapshot:")
            .and_then(|s| s.parse::<u64>().ok())
        {
            let snapshot = self
                .catalog
                .iter()
                .find(|s| s.id().get() == id)
                .map(RecipeSnapshot::id);
            if let Some(id) = snapshot {
                self.select_snapshot(id);
            }
        }
    }
    /// Keyboard movement through unique visible sounds, skipping virtual headings.
    pub fn navigate_library(&mut self, direction: i32) {
        if let Some(p) = &mut self.persistent {
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
                p.selection = Some(key.clone());
                p.revision += 1;
            }
            self.load_persistent_selection();
            self.changed();
        } else {
            let current = self
                .filtered
                .iter()
                .position(|i| *i == self.selected)
                .unwrap_or(0);
            let index = match direction {
                -2 => 0,
                2 => self.filtered.len().saturating_sub(1),
                -1 => current.saturating_sub(1),
                _ => current
                    .saturating_add(1)
                    .min(self.filtered.len().saturating_sub(1)),
            };
            self.select(index);
        }
    }
    /// Request a rescan without dropping stale rows or the open draft on failure.
    pub fn refresh_library(&mut self) {
        if let Some(p) = &mut self.persistent {
            p.refresh = true;
            p.load_attempted = None;
        }
        self.continue_persistent();
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
        let record = self
            .persistent
            .as_ref()
            .and_then(|p| {
                p.loaded
                    .as_ref()
                    .filter(|r| Some(&r.key) == p.selection.as_ref())
            })
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
                .persistent
                .as_ref()
                .is_some_and(|p| p.selection.as_ref().is_some_and(|k| k.to_string() == key))
            && self.selected().is_some_and(|s| s.external_file().is_some())
        {
            self.edit_selected();
            return;
        }
        let record = self
            .persistent
            .as_ref()
            .and_then(|p| p.loaded.as_ref().filter(|r| !p.requires_read(&r.key)))
            .filter(|r| {
                r.key.to_string() == key && r.version_token.to_string() == token && !r.trashed
            })
            .cloned();
        if let Some(record) = record {
            self.request(DocumentAction::EditManaged(record));
        } else {
            self.file_status =
                "Saved selection changed. Refresh before editing this version.".into();
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
            self.file_status = views::refusal_text(refusal);
        }
        self.changed();
        result
    }
    fn save_independent_copy_inner(&mut self) -> Result<(), Refusal> {
        self.import_admission()?;
        let content = self.prepare_recipe()?;
        let snapshot = RecipeSnapshot::new(
            crate::recipe::Origin::Draft(self.editor.draft().title().into()),
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
            self.file_status = views::refusal_text(refusal);
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
            document: self.editor.document(),
            draft: self.editor.draft_revision(),
            input: self.editor.input_revision(),
        };
        if let Err(refusal) = self.managed_job(operation) {
            self.file_status = views::refusal_text(&refusal);
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
            self.file_status = views::refusal_text(refusal);
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
            && self.editor.dirty()
            && self
                .persistent
                .as_ref()
                .and_then(|p| p.location.as_ref())
                .is_some_and(|(k, _)| k == &key)
        {
            self.request(DocumentAction::TrashSaved { key, expect });
            return Ok(());
        }
        if action == "purge" && !confirmed {
            self.start_allowed()?;
            if self.file.is_some() {
                return Err(Refusal::FileBusy);
            }
            let entry = self
                .persistent
                .as_ref()
                .and_then(|p| {
                    p.entries.iter().find(|e| {
                        e.key == key && e.version_token.as_ref() == Some(&expect) && e.trashed
                    })
                })
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
        let Some(p) = &mut self.persistent else {
            return;
        };
        let Some(key @ LibraryKey::External(_)) = p.selection.clone() else {
            return;
        };
        let ids: Vec<_> = p
            .snapshots
            .iter()
            .filter(|(k, _)| k == &key)
            .map(|(_, id)| *id)
            .collect();
        self.catalog.retain(|s| !ids.contains(&s.id()));
        p.snapshots.retain(|(k, _)| k != &key);
        p.entries.retain(|e| e.key != key);
        p.refilter();
        self.changed();
    }
    /// Reveal the last confirmed saved key explicitly, clearing filters only on
    /// this command; saving itself preserves the current query.
    pub fn show_saved(&mut self) -> bool {
        let Some(p) = &mut self.persistent else {
            return false;
        };
        let Some(key) = p.last_saved.clone() else {
            return false;
        };
        let Some(entry) = p.entries.iter().find(|e| e.key == key) else {
            self.file_status="The saved entry is no longer in the observed inventory; refresh before locating it.".into();
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
        p.selection = Some(key);
        self.query.clear();
        self.rhythm_query = crate::query::RhythmQuery::default();
        self.load_persistent_selection();
        self.changed();
        true
    }
    pub(super) fn selected_unreadable(&self) -> bool {
        self.persistent
            .as_ref()
            .is_some_and(Persistent::selected_unreadable)
    }
    pub(super) fn selected_trashed(&self) -> bool {
        self.persistent
            .as_ref()
            .and_then(|p| {
                p.loaded
                    .as_ref()
                    .filter(|r| Some(&r.key) == p.selection.as_ref())
            })
            .is_some_and(|r| r.trashed)
    }
    /// Root location and current inventory diagnostics.
    #[must_use]
    pub fn library_status(&self) -> String {
        self.persistent.as_ref().map_or_else(String::new, |p| {
            format!("{}\n{}", p.store.root().path().display(), p.diagnostics)
        })
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
        self.persistent
            .as_ref()
            .and_then(|p| {
                p.selection
                    .as_ref()
                    .and_then(|key| p.entries.iter().find(|e| &e.key == key))
            })
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
        if self.persistent.is_none() {
            return self
                .selected()
                .map_or((String::new(), String::new(), false), |s| {
                    (format!("snapshot:{}", s.id().get()), String::new(), false)
                });
        }
        self.persistent
            .as_ref()
            .and_then(|p| {
                p.selection
                    .as_ref()
                    .and_then(|key| p.entries.iter().find(|e| &e.key == key))
            })
            .map_or((String::new(), String::new(), false), |e| {
                (
                    e.key.to_string(),
                    if self
                        .persistent
                        .as_ref()
                        .is_some_and(|p| p.selected_unreadable())
                    {
                        String::new()
                    } else {
                        e.version_token
                            .as_ref()
                            .map_or(String::new(), ToString::to_string)
                    },
                    e.trashed,
                )
            })
    }
    fn managed_job(&mut self, operation: Operation) -> Result<TaskId, Refusal> {
        self.start_allowed()?;
        if self.file.is_some() {
            return Err(Refusal::FileBusy);
        }
        let p = self.persistent.as_ref().ok_or(Refusal::Document(
            "Persistent library is not configured.".into(),
        ))?;

        let store = p.store.clone();
        let id = self.start_file(FileKind::Managed(Job { store, operation }));
        self.changed();
        Ok(id)
    }
    pub(super) fn start_managed_save(
        &mut self,
        document: DocumentId,
        recipe: Arc<Recipe>,
    ) -> Result<TaskId, Refusal> {
        let p = self.persistent.as_ref().ok_or(Refusal::NoSelection)?;
        if let Some(key) = &p.detached {
            return Err(Refusal::Document(format!(
                "Detached draft from {key}. Edit the restored saved entry or Save independent copy; the old identity is not recreated."
            )));
        }
        if let Some(expected) = p.external.clone() {
            return self.managed_job(Operation::ExternalSave {
                document,
                expected,
                recipe,
            });
        }
        if p.location.is_none()
            && p.entries
                .iter()
                .any(|e| e.key == LibraryKey::User(recipe.document.id))
        {
            return Err(Refusal::Document("Detached draft belongs to an existing saved identity. Restore/edit that version or Save independent copy.".into()));
        }
        let mutation = p.location.as_ref().map_or_else(
            || Mutation::Create(recipe.clone()),
            |(key, expect)| Mutation::Update {
                key: key.clone(),
                expect: Some(*expect),
                recipe: recipe.clone(),
            },
        );
        self.managed_job(Operation::Write {
            document: Some(document),
            mutation,
        })
    }
    pub(super) fn continue_persistent(&mut self) {
        if self.file.is_some() || self.prompt.is_some() || self.pending.is_some() {
            return;
        }
        if let Some(p) = &mut self.persistent
            && p.refresh
        {
            p.refresh = false;
            let _ = self.managed_job(Operation::Scan);
            return;
        }
        self.load_persistent_selection();
    }
    pub(super) fn load_persistent_selection(&mut self) {
        if self.file.is_some() || self.prompt.is_some() || self.pending.is_some() {
            return;
        }
        let Some(p) = &mut self.persistent else {
            return;
        };
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
        if let Some(p) = &mut self.persistent {
            // Drop superseded temporary entries for the same opened path.
            let old_ids: Vec<_> = self
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
                    p.selection = p.snapshots.last().map(|(k, _)| k.clone());
                }
                Err(e) => self.file_status = e.to_string(),
            }
        }
    }
    pub(super) fn finish_managed(
        &mut self,
        job: &Job,
        result: Result<ResultValue, Error>,
    ) -> (String, Option<bool>) {
        let document = match job.operation {
            Operation::Write { document, .. } => document,
            Operation::ExternalSave { document, .. } => Some(document),
            _ => None,
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                return (
                    format!(
                        "{}: {error}\nDraft retained. Refresh and reload the saved version, keep the draft, or save an independent copy.",
                        if job.may_publish() {
                            "Operation failed; inspect the reported path"
                        } else {
                            "Library read failed"
                        }
                    ),
                    document.map(|_| false),
                );
            }
        };
        match result {
            ResultValue::ExternalWritten(outcome) => {
                self.accept_external_outcome(document, *outcome)
            }

            ResultValue::Inventory(inventory) => {
                let Some(p) = self.persistent.as_mut() else {
                    return ("Library operation completed without an attached workspace; inspect saved storage before another write.".into(),Some(false));
                };
                p.complete = inventory.complete;
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
                p.record_diagnostics = inventory
                    .diagnostics
                    .iter()
                    .filter(|d| d.storage_id.ends_with(".toml"))
                    .map(|d| format!("Needs review · {}\n{}", d.storage_id, d.message))
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
                let count = inventory.entries.len();
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

                p.diagnostics = format!(
                    "{} · {count} readable User entries{}{}",
                    if inventory.complete {
                        "Inventory complete"
                    } else {
                        "Inventory incomplete · unseen previous rows retained; refresh before acting"
                    },
                    if inventory.not_initialized {
                        " · Not initialized"
                    } else {
                        ""
                    },
                    inventory
                        .diagnostics
                        .iter()
                        .map(|d| format!("\n{}: {}", d.code, d.message))
                        .collect::<String>()
                );
                if !inventory.stages.is_empty() {
                    p.diagnostics.push_str(&format!(
                        "\n{} staging artifacts retained for recovery.",
                        inventory.stages.len()
                    ));
                }
                p.load_attempted = None;
                p.refilter();
                (p.diagnostics.clone(), None)
            }
            ResultValue::Loaded(record) => {
                let Some(p) = self.persistent.as_mut() else {
                    return ("Library operation completed without an attached workspace; inspect saved storage before another write.".into(),Some(false));
                };
                p.accept_record(*record);
                (
                    "Saved version loaded; open draft and playing snapshot retained.".into(),
                    None,
                )
            }
            ResultValue::Written(outcome) => {
                let result = self.accept_managed_outcome(document, *outcome);
                if let Operation::DiscardTrash {
                    document,
                    draft,
                    input,
                    ..
                } = job.operation
                    && self.editor.document() == document
                    && self.editor.draft_revision() == draft
                    && self.editor.input_revision() == input
                {
                    match Draft::new() {
                        Ok(draft) => self.replace_document(draft),
                        Err(error) => self.file_status = error.to_string(),
                    }
                }
                result
            }
        }
    }
    fn accept_external_outcome(
        &mut self,
        document: Option<DocumentId>,
        outcome: crate::external::ExternalOutcome,
    ) -> (String, Option<bool>) {
        let Some(document) = document else {
            return (
                "External save result has no owning document; saved baseline was not changed."
                    .into(),
                Some(false),
            );
        };
        let Some(p) = self.persistent.as_mut() else {
            return ("Library operation completed without an attached workspace; inspect saved storage before another write.".into(),Some(false));
        };

        let snapshot = outcome.snapshot();
        self.editor.record_saved(document, &outcome.recipe);
        if self.editor.document() == document {
            p.external = snapshot.external_file().cloned();
        }
        // Replace only the selected external observation's index, keeping its key.
        let path = outcome.previous.path();
        if let Some(index) = self
            .catalog
            .iter()
            .position(|s| s.external_file().is_some_and(|f| f.path() == path))
        {
            let old_id = self.catalog[index].id();
            if let Some((key, id)) = p.snapshots.iter_mut().find(|(_, id)| *id == old_id) {
                *id = snapshot.id();
                p.entries.retain(|e| &e.key != key);
                p.entries
                    .push(EntrySummary::from_snapshot(key.clone(), &snapshot, None));
            }
            self.catalog[index] = snapshot;
        }
        p.refilter();
        (
            format!(
                "{} · External file saved from captured draft.\n{}",
                outcome.status.label(),
                outcome.diagnostics.join("\n")
            ),
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
        if let Some(error) = &self.persistent_config_error {
            return (
                "User Library unavailable",
                format!("Unsaved · User Library unavailable: {error}"),
            );
        }
        let Some(p) = &self.persistent else {
            return (
                "Recipe file",
                "External save destination chosen on Save".into(),
            );
        };
        if let Some(key) = &p.detached {
            return (
                "Detached draft · save an independent copy or restore/edit saved entry",
                format!(
                    "Detached draft · {key} · restore/edit the saved version or Save independent copy"
                ),
            );
        }
        if let Some(external) = &p.external {
            return (
                "External file",
                format!(
                    "External file · {} · observed {}",
                    external.path().display(),
                    external.fingerprint()
                ),
            );
        }
        if let Some((key, token)) = &p.location {
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
        mut outcome: WriteOutcome,
    ) -> (String, Option<bool>) {
        let Some(p) = self.persistent.as_mut() else {
            return ("Library operation completed without an attached workspace; inspect saved storage before another write.".into(),Some(false));
        };
        let key = outcome.key.clone();

        if let Some(record) = outcome.record {
            if document.is_some()
                && let Err(error) = record.snapshot.at(self.export_rate)
            {
                outcome.diagnostics.push(format!("Saved with validation errors at {} Hz: {error}. This finite document can be repaired later.",self.export_rate.hz()));
            }
            if let Some(document) = document {
                self.editor.record_saved(document, record.snapshot.recipe());
                if self.editor.document() == document {
                    p.location = Some((key.clone(), record.version_token));
                    p.detached = None;
                }
            } else if p.location.as_ref().is_some_and(|(k, _)| k == &key) {
                // Membership changes do not alter portable history or the live draft.
                // A dirty rename keeps its old expected token so Save exposes conflict.
                if matches!(outcome.kind, MutationKind::Trash) {
                    p.detached = Some(key.clone());
                    p.location = None;
                } else if !self.editor.dirty() {
                    match EditorSession::new(Draft::from_recipe(record.snapshot.recipe().clone())) {
                        Ok(editor) => {
                            self.editor = editor;
                            self.editor
                                .record_saved(self.editor.document(), record.snapshot.recipe());
                            p.location = Some((key.clone(), record.version_token));
                        }
                        Err(error) => outcome.diagnostics.push(format!(
                            "Saved, but the editor could not refresh: {error}. Open draft retained."
                        )),
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
            format!(
                "{} · {key}\n{}{}",
                outcome.status.label(),
                if document.is_some() {
                    "Saved captured version; later edits remain independent. Use Show saved if filters hide it."
                } else {
                    "Library operation confirmed."
                },
                outcome
                    .diagnostics
                    .iter()
                    .map(|d| format!("\n{d}"))
                    .collect::<String>()
            ),
            document.map(|_| true),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::LibraryRoot;
    use std::time::{Duration, Instant};
    fn fixture() -> (tempfile::TempDir, LibraryStore, Workspace) {
        let temp = tempfile::tempdir().unwrap();
        let store =
            LibraryStore::new(LibraryRoot::explicit(temp.path().join("library")).unwrap()).unwrap();
        store.root().initialize().unwrap();
        let mut workspace = Workspace::with_library(
            crate::recipe::catalog().unwrap(),
            Capabilities::portable(),
            store.clone(),
        )
        .unwrap();
        idle(&mut workspace);
        (temp, store, workspace)
    }
    fn idle(w: &mut Workspace) {
        let limit = Instant::now() + Duration::from_secs(10);
        loop {
            w.poll();
            if !w.file_busy() && !w.action_pending() {
                break;
            }
            assert!(Instant::now() < limit, "file worker did not finish");
            std::thread::yield_now();
        }
    }
    fn title(w: &mut Workspace, text: &str) {
        let handle = w
            .editor
            .handle(EditingField::Metadata(crate::editing::MetadataField::Title));
        w.editor.stage(handle, text).unwrap();
        w.editor.commit().unwrap();
    }
    fn user(w: &mut Workspace) {
        w.set_catalog_query(CatalogQuery {
            scope: Scope::User,
            ..CatalogQuery::default()
        })
        .unwrap();
        idle(w);
    }
    #[test]
    fn applied_summary_lists_values_from_committed_query_and_refuses_invalid_replacement() {
        use crate::collection::{DateRange, GroupBy, SortBy};
        let (_temp, _store, mut w) = fixture();
        let from = "2026-03-28T22:00:00Z".parse().unwrap();
        let before = "2026-03-29T21:00:00Z".parse().unwrap();
        let query = CatalogQuery {
            search: "exact search".into(),
            studies: vec!["10.1234/exact".into()],
            populations: vec![crate::core::Population::Human],
            roles: vec![crate::core::ConditionRole::Comparison],
            analysis: Some(crate::core::nominal::Completeness::Partial),
            rhythm: crate::query::RhythmQuery {
                bands: vec![crate::core::nominal::Band::Delta],
                methods: vec![crate::core::nominal::Method::Binaural],
                state: Some(crate::query::RhythmState::Multiple),
            },
            created: DateRange {
                from: Some(from),
                before: Some(before),
            },
            modified: DateRange {
                from: Some(from),
                before: None,
            },
            added: DateRange {
                from: None,
                before: Some(before),
            },
            sort: SortBy::Title,
            group_by: GroupBy::Study,
            ..CatalogQuery::default()
        };
        w.set_catalog_query(query.clone()).unwrap();
        let summary = w.library_count();
        for expected in [
            "exact search",
            "10.1234/exact",
            "Population: Human",
            "Condition role: Comparison",
            "Analysis: Partial",
            "Band: Delta",
            "Method: binaural",
            "Rhythm state: Multiple",
            "Created from inclusive:",
            "Created before exclusive:",
            "Edited from inclusive:",
            "Added here before exclusive:",
            "2026-03-28T22:00:00.000000000Z",
            "2026-03-29T21:00:00.000000000Z",
        ] {
            assert!(summary.contains(expected), "{expected}: {summary}");
        }
        let mut invalid = query;
        invalid.created.before = Some(from);
        assert!(w.set_catalog_query(invalid).is_err());
        assert_eq!(w.library_count(), summary);
    }

    #[test]
    fn summary_rows_show_real_sort_date_unknown_edit_and_added_without_loading_documents() {
        use crate::collection::{Direction, SortBy};
        let (_temp, _store, mut w) = fixture();
        let p = w.persistent.as_mut().unwrap();
        let mut unknown = p.entries[0].clone();
        unknown.duration_seconds = 60.0;
        let mut known = unknown.clone();
        unknown.document_id = crate::core::DocumentUuid::from_random_bytes([1; 16]);
        unknown.key = LibraryKey::User(unknown.document_id);
        unknown.title = "A unknown edit".into();
        unknown.normalized_title = "a unknown edit".into();
        unknown.created_at = Some("2026-01-01T12:00:00Z".parse().unwrap());
        unknown.modified_at = None;
        unknown.added_at = Some("2026-10-24T12:00:00Z".parse().unwrap());
        known.document_id = crate::core::DocumentUuid::from_random_bytes([2; 16]);
        known.key = LibraryKey::User(known.document_id);
        known.title = "Z known edit".into();
        known.normalized_title = "z known edit".into();
        known.created_at = Some("2026-01-02T12:00:00Z".parse().unwrap());
        known.modified_at = Some("2026-06-01T12:00:00Z".parse().unwrap());
        known.added_at = Some("2026-10-23T12:00:00Z".parse().unwrap());
        p.entries = vec![unknown.clone(), known.clone()];
        p.query = CatalogQuery {
            scope: Scope::User,
            ..CatalogQuery::default()
        };
        p.refilter();
        let (keys, rows, _) = p.rows();
        assert_eq!(
            keys[0],
            known.key.to_string(),
            "Unknown edit must not sort by newer Added here"
        );
        assert!(rows[1].contains("Edited: Unknown") && rows[1].contains("Added here:"));
        assert!(rows[0].contains("Duration 1 min"));
        p.query.sort = SortBy::Created;
        p.query.direction = Some(Direction::Asc);
        p.refilter();
        let (keys, rows, _) = p.rows();
        assert_eq!(keys[0], unknown.key.to_string());
        assert!(rows[0].contains(&format!(
            "Created: {}",
            crate::calendar::local_timestamp(unknown.created_at.unwrap())
        )));
        p.query.sort = SortBy::Added;
        p.query.direction = Some(Direction::Desc);
        p.refilter();
        let (keys, rows, _) = p.rows();
        assert_eq!(keys[0], unknown.key.to_string());
        assert!(rows[0].contains("Edited: Unknown") && rows[0].contains("Added here:"));
        assert!(
            p.loaded.is_none(),
            "Rows consume summaries, not full selected User documents"
        );
    }

    #[test]
    fn save_publishes_captured_history_and_keeps_later_draft_and_filters() {
        let (_temp, store, mut w) = fixture();
        w.new_document();
        title(&mut w, "captured");
        // Capture before a later edit; the real worker publishes only this Arc.
        let content = Arc::new(w.editor.draft().recipe());
        w.start_managed_save(w.editor.document(), content).unwrap();
        title(&mut w, "later");
        idle(&mut w);
        assert!(w.editor.dirty());
        assert_eq!(w.editor.draft().title(), "later");
        assert_eq!(w.catalog_query().unwrap().scope, Scope::All);
        let inventory = store.scan(&Cancel::default());
        assert_eq!(inventory.entries.len(), 1);
        assert_eq!(inventory.entries[0].title, "captured");
        assert!(inventory.entries[0].created_at.is_some());
        user(&mut w);
        assert_eq!(w.selected().unwrap().title(), "captured");
        assert!(w.editor.dirty());
        w.save().unwrap();
        idle(&mut w);
        assert!(!w.editor.dirty());
        assert_eq!(store.scan(&Cancel::default()).entries[0].title, "later");
    }
    #[test]
    fn two_editors_conflict_even_for_noop_and_refresh_preserves_draft() {
        let (_temp, store, mut w) = fixture();
        w.save().unwrap();
        idle(&mut w);
        user(&mut w);
        w.edit_selected();
        let mut other = Workspace::with_library(
            crate::recipe::catalog().unwrap(),
            Capabilities::portable(),
            store.clone(),
        )
        .unwrap();
        idle(&mut other);
        user(&mut other);
        other.edit_selected();
        let (key, token, _) = w.library_binding();
        w.mutate_library(&key, &token, "rename", "renamed elsewhere", false)
            .unwrap();
        idle(&mut w);
        other.save().unwrap();
        idle(&mut other);
        assert!(other.file_status.contains("saved recipe changed"));
        assert_eq!(other.editor.draft().title(), "Untitled sound");
        other.refresh_library();
        idle(&mut other);
        assert_eq!(other.selected().unwrap().title(), "renamed elsewhere");
        assert_eq!(other.editor.draft().title(), "Untitled sound");
        assert!(other.editor.draft().recipe().document.created_at.is_some());
    }
    #[test]
    fn trash_restore_and_purge_use_exact_gesture_and_explicit_prompt() {
        let (_temp, store, mut w) = fixture();
        w.save().unwrap();
        idle(&mut w);
        user(&mut w);
        let (key, token, _) = w.library_binding();
        w.mutate_library(&key, &token, "trash", "", false).unwrap();
        idle(&mut w);
        assert!(w.library_view().rows.is_empty());
        w.set_catalog_query(CatalogQuery {
            scope: Scope::Trash,
            ..CatalogQuery::default()
        })
        .unwrap();
        idle(&mut w);
        let (_, trash_token, trashed) = w.library_binding();
        assert!(trashed);
        assert!(w.prepare_audio().is_err());
        w.mutate_library(&key, &trash_token, "restore", "", false)
            .unwrap();
        idle(&mut w);
        user(&mut w);
        let (_, restored, _) = w.library_binding();
        w.mutate_library(&key, &restored, "trash", "", false)
            .unwrap();
        idle(&mut w);
        w.set_catalog_query(CatalogQuery {
            scope: Scope::Trash,
            ..CatalogQuery::default()
        })
        .unwrap();
        idle(&mut w);
        let (_, current, _) = w.library_binding();
        w.mutate_library(&key, &current, "purge", "", false)
            .unwrap();
        let prompt = w
            .take_effects()
            .into_iter()
            .find_map(|e| match e {
                Effect::AskPurge {
                    prompt,
                    key: prompted,
                    ..
                } => {
                    assert_eq!(prompted, key);
                    Some(prompt)
                }
                _ => None,
            })
            .unwrap();
        assert!(
            store.load(&key.parse().unwrap()).is_ok(),
            "confirmation occurs before any deletion"
        );
        w.respond(prompt, Response::Confirm);
        idle(&mut w);
        assert!(store.scan(&Cancel::default()).entries.is_empty());
        assert!(w.document_location().starts_with("Detached draft"));
        assert!(
            w.save().is_err(),
            "purging a saved entry cannot turn the retained draft into an implicit Create with the old UUID"
        );
        assert!(store.scan(&Cancel::default()).entries.is_empty());
        w.save_independent_copy().unwrap();
        idle(&mut w);
        let inventory = store.scan(&Cancel::default());
        assert_eq!(inventory.entries.len(), 1);
        assert_ne!(inventory.entries[0].key.to_string(), key);
    }
    #[test]
    fn incomplete_scan_retains_old_rows_and_never_overwrites_open_editor() {
        let (_temp, store, mut w) = fixture();
        w.save().unwrap();
        idle(&mut w);
        user(&mut w);
        let (key, _, _) = w.library_binding();
        let record = store.load(&key.parse().unwrap()).unwrap();
        std::fs::write(
            store
                .root()
                .path()
                .join("recipes")
                .join(format!("{}.toml", record.snapshot.recipe().document.id)),
            b"broken",
        )
        .unwrap();
        w.refresh_library();
        idle(&mut w);
        assert_eq!(
            w.library_view()
                .keys
                .iter()
                .filter(|key| !key.is_empty())
                .count(),
            1
        );
        assert!(
            w.library_view()
                .rows
                .iter()
                .any(|row| row.contains("Needs review"))
        );
        assert!(w.selected_unreadable());
        assert!(w.library_status().contains("incomplete"));
        assert_eq!(w.editor.draft().title(), "Untitled sound");
    }

    #[test]
    fn save_before_trash_uses_confirmed_new_token_and_preserves_published_edit() {
        let (_temp, store, mut w) = fixture();
        w.save().unwrap();
        idle(&mut w);
        user(&mut w);
        w.edit_selected();
        title(&mut w, "save before trash");
        let (key, token, _) = w.library_binding();
        w.mutate_library(&key, &token, "trash", "", false).unwrap();
        let prompt = w
            .take_effects()
            .into_iter()
            .find_map(|e| {
                if let Effect::AskUnsaved(prompt) = e {
                    Some(prompt)
                } else {
                    None
                }
            })
            .unwrap();
        w.respond(prompt, Response::Save);
        idle(&mut w);
        let record = store.load(&key.parse().unwrap()).unwrap();
        assert!(record.trashed_at.is_some());
        assert_eq!(record.snapshot.title(), "save before trash");
        assert_eq!(w.editor.draft().title(), "Untitled sound");
    }
    #[test]
    fn discard_before_trash_does_not_discard_edits_made_during_publication() {
        let (_temp, store, mut w) = fixture();
        w.save().unwrap();
        idle(&mut w);
        user(&mut w);
        w.edit_selected();
        title(&mut w, "before request");
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let worker_barrier = barrier.clone();
        w.file_spawner = Some(Box::new(move |kind| {
            let barrier = worker_barrier.clone();
            tasks::Worker::spawn(move |tx| {
                barrier.wait();
                let _ = tx.send(kind.execute());
            })
        }));
        let (key, token, _) = w.library_binding();
        let document = w.editor.document();
        w.mutate_library(&key, &token, "trash", "", false).unwrap();
        let prompt = w
            .take_effects()
            .into_iter()
            .find_map(|e| {
                if let Effect::AskUnsaved(prompt) = e {
                    Some(prompt)
                } else {
                    None
                }
            })
            .unwrap();
        w.respond(prompt, Response::Discard);
        title(&mut w, "later edit retained");
        barrier.wait();
        idle(&mut w);
        w.file_spawner = None;
        assert_eq!(w.editor.document(), document);
        assert_eq!(w.editor.draft().title(), "later edit retained");
        assert!(w.editor.dirty());
        assert!(
            store
                .load(&key.parse().unwrap())
                .unwrap()
                .trashed_at
                .is_some()
        );
        w.save_independent_copy().unwrap();
        idle(&mut w);
        let inventory = store.scan(&Cancel::default());
        assert_eq!(inventory.entries.len(), 2);
        assert_eq!(inventory.entries.iter().filter(|e| !e.trashed).count(), 1);
    }
    #[test]
    fn managed_playback_worker_refuses_changed_or_trashed_observation() {
        let (_temp, store, mut w) = fixture();
        w.save().unwrap();
        idle(&mut w);
        user(&mut w);
        let captured = w.selected().unwrap().clone();
        assert!(tasks::revalidate_managed(&captured, &store).is_ok());
        let record = store.load(&w.library_binding().0.parse().unwrap()).unwrap();
        store
            .apply(
                Mutation::Trash {
                    key: record.key,
                    expect: Some(record.version_token),
                },
                &Cancel::default(),
            )
            .unwrap();
        std::fs::remove_file(store.root().path().join("membership.toml")).unwrap();
        assert_eq!(
            tasks::revalidate_managed(&captured, &store)
                .unwrap_err()
                .kind(),
            ErrorKind::Conflict
        );
        assert!(!w.audio_busy());
        assert_eq!(captured.title(), "Untitled sound");
    }

    #[test]
    fn retained_user_source_is_captured_temporary_content_not_a_fresh_user_selection() {
        let (_temp, store, mut w) = fixture();
        w.save().unwrap();
        idle(&mut w);
        user(&mut w);
        let original = w.selected().unwrap().clone();
        w.copy_selected();
        let key: LibraryKey = format!("user:{}", original.recipe().document.id)
            .parse()
            .unwrap();
        let record = store.load(&key).unwrap();
        store
            .apply(
                Mutation::Rename {
                    key,
                    expect: Some(record.version_token),
                    title: "Changed current User".into(),
                },
                &Cancel::default(),
            )
            .unwrap();
        w.show_based_on_source();
        assert_eq!(w.selected().unwrap().id(), original.id());
        assert_eq!(w.selected().unwrap().title(), "Untitled sound");
        assert_eq!(w.catalog_query().unwrap().scope, Scope::OpenFiles);
        assert!(!w.persistent.as_ref().unwrap().selected_user());
        assert!(!w.file_busy());
        assert!(!w.audio_busy());
    }

    #[test]
    fn missing_worker_result_keeps_draft_and_retry_observes_real_storage() {
        for applied in [false, true] {
            let (_temp, store, mut workspace) = fixture();
            title(&mut workspace, "Unsaved draft must survive");
            workspace.file_spawner = Some(Box::new(move |kind| {
                tasks::Worker::spawn(move |_| {
                    if applied {
                        let _ = kind.execute();
                    }
                })
            }));
            workspace.save().unwrap();
            idle(&mut workspace);
            assert!(workspace.editor.dirty());
            assert!(workspace.file_status.contains("without a result"));
            assert_eq!(
                store.scan(&Cancel::default()).entries.len(),
                usize::from(applied)
            );
            workspace.file_spawner = None;
            workspace.save().unwrap();
            idle(&mut workspace);
            if applied {
                assert!(workspace.editor.dirty());
                assert!(workspace.file_status.contains("identity already exists"));
                workspace.save_independent_copy().unwrap();
                idle(&mut workspace);
                assert_eq!(store.scan(&Cancel::default()).entries.len(), 2);
            } else {
                assert_eq!(store.scan(&Cancel::default()).entries.len(), 1);
            }
            assert!(!workspace.editor.dirty());
        }
    }

    #[test]
    fn refresh_reloads_membership_state_even_when_recipe_sha_is_unchanged() {
        let (_temp, store, mut workspace) = fixture();
        workspace.save().unwrap();
        idle(&mut workspace);
        user(&mut workspace);
        let (binding, token, _) = workspace.library_binding();
        let key: LibraryKey = binding.parse().unwrap();
        store
            .apply(
                Mutation::Trash {
                    key: key.clone(),
                    expect: Some(token.parse().unwrap()),
                },
                &Cancel::default(),
            )
            .unwrap();
        std::fs::remove_file(store.root().path().join("membership.toml")).unwrap();
        workspace
            .set_catalog_query(CatalogQuery {
                scope: Scope::Trash,
                ..CatalogQuery::default()
            })
            .unwrap();
        workspace.refresh_library();
        idle(&mut workspace);
        assert!(workspace.selected_trashed());
        let loaded = workspace
            .persistent
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap();
        assert!(loaded.trashed);
        assert_eq!(loaded.trashed_at, None);
        assert_eq!(loaded.version_token.to_string(), token);
    }
}
