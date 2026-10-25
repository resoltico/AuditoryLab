//! Single-record lifecycle operations over rooted native filesystem handles.
use super::{
    LibraryKey, LibraryRoot, RecordRevision, VersionToken,
    record::{self, Decoded, Envelope, Nullable},
    root,
};
use crate::{
    Cancel, DocumentUuid, Error, ErrorKind, Origin, Recipe, RecipeSnapshot, Timestamp,
    collection::EntrySummary,
};
use cap_std::fs::Dir;
use serde::Serialize;
use std::{collections::BTreeSet, io::Write, path::PathBuf, sync::Arc};

/// Maximum record candidates examined in one scan.
pub const MAX_SCAN_RECORDS: usize = 10_000;
/// Maximum raw managed bytes consumed in one scan.
pub const MAX_SCAN_RAW_BYTES: usize = 256 * 1024 * 1024;
/// Maximum decoded recipe bytes admitted in one scan.
pub const MAX_SCAN_DECODED_BYTES: usize = 128 * 1024 * 1024;
/// One diagnostic with a storage identifier, never an invented recipe title.
#[derive(Debug, Clone, Serialize)]
pub struct LibraryDiagnostic {
    /// Root or relative storage identifier.
    pub storage_id: String,
    /// Stable diagnostic category.
    pub code: String,
    /// Context without raw corrupt-record snippets.
    pub message: String,
}
/// A bounded disposable inventory; incomplete never means an empty library.
#[derive(Debug, Clone)]
pub struct Inventory {
    /// Healthy structurally readable summaries, including finite repairable sounds.
    pub entries: Vec<EntrySummary>,
    /// Individual/root/budget errors retained alongside healthy entries.
    pub diagnostics: Vec<LibraryDiagnostic>,
    /// Staging artifacts, reported but never promoted or deleted by scanning.
    pub stages: Vec<String>,
    /// Every eligible entry was discovered and read within the scan bounds.
    pub complete: bool,
    /// Missing default root is an uninitialized empty library, not a read failure.
    pub not_initialized: bool,
    /// Dominant failure category for CLI exit policy.
    pub failure_kind: Option<ErrorKind>,
}
impl Default for Inventory {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            diagnostics: Vec::new(),
            stages: Vec::new(),
            complete: true,
            not_initialized: false,
            failure_kind: None,
        }
    }
}
impl Inventory {
    fn error(
        &mut self,
        id: impl Into<String>,
        code: &str,
        error: impl std::fmt::Display,
        kind: ErrorKind,
    ) {
        self.complete = false;
        self.failure_kind = Some(kind);
        self.diagnostics.push(LibraryDiagnostic {
            storage_id: id.into(),
            code: code.into(),
            message: error.to_string(),
        });
    }
}
/// Full immutable selected record; scanning does not retain one for every row.
#[derive(Debug, Clone)]
pub struct LoadedRecord {
    /// User capability/key determined by managed storage.
    pub key: LibraryKey,
    /// Revision and exact observed bytes for optimistic mutation.
    pub version_token: VersionToken,
    /// Local membership timestamp, separate from portable history.
    pub added_at: Timestamp,
    /// Local Trash flag/time.
    pub trashed_at: Option<Timestamp>,
    /// Captured full portable content.
    pub snapshot: RecipeSnapshot,
    /// Exact stored plain portable recipe bytes, without local envelope fields.
    pub recipe_toml: Arc<str>,
}
impl LoadedRecord {
    fn from_decoded(id: DocumentUuid, decoded: Decoded) -> Self {
        let key = LibraryKey::User(id);
        let token = decoded.token;
        let snapshot = RecipeSnapshot::new(
            Origin::User {
                id,
                version: token.clone(),
            },
            decoded.recipe,
        );
        Self {
            key,
            version_token: token,
            added_at: decoded.envelope.added_at,
            trashed_at: decoded.envelope.trashed_at.0,
            snapshot,
            recipe_toml: Arc::from(decoded.envelope.recipe_toml),
        }
    }
    /// Compact index projection; no target compilation or audio rendering.
    #[must_use]
    pub fn summary(&self) -> EntrySummary {
        EntrySummary::from_snapshot(
            self.key.clone(),
            &self.snapshot,
            Some((self.added_at, self.trashed_at, self.version_token.clone())),
        )
    }
}
/// Captured managed intent. Existing mutations always carry a fresh expected token.
#[derive(Debug, Clone)]
pub enum Mutation {
    /// New authored/copy identity; dates assigned on its first publication.
    Create(Arc<Recipe>),
    /// Add portable external content, preserving declared/unknown history.
    Add(Arc<Recipe>),
    /// Replace editable content of the same identity, never creation history.
    Update {
        /// Existing User key.
        key: LibraryKey,
        /// Exact observed record token.
        expect: VersionToken,
        /// Immutable proposed content.
        recipe: Arc<Recipe>,
    },
    /// Title-only content update; no audio or category change.
    Rename {
        /// Existing User key.
        key: LibraryKey,
        /// Exact observed token.
        expect: VersionToken,
        /// Nonempty new title.
        title: String,
    },
    /// Soft deletion in the same atomic record.
    Trash {
        /// Existing User key.
        key: LibraryKey,
        /// Exact observed token.
        expect: VersionToken,
    },
    /// Explicit restoration, preserving creation/edit/Added here dates.
    Restore {
        /// Existing User key.
        key: LibraryKey,
        /// Exact observed token.
        expect: VersionToken,
    },
    /// Explicit permanent removal of one verified trashed User record.
    Purge {
        /// Existing User key.
        key: LibraryKey,
        /// Exact observed token.
        expect: VersionToken,
        /// Explicit permanent-deletion confirmation.
        confirmed: bool,
    },
}
/// Operation class retained in a receipt, independent of its reporting status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    /// First authored publication.
    Create,
    /// External membership import.
    Add,
    /// Content edit.
    Update,
    /// Title edit.
    Rename,
    /// Soft delete.
    Trash,
    /// Restore active membership.
    Restore,
    /// Permanent deletion.
    Purge,
}
/// Publication truth, separate from whether terminal/UI reporting succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationStatus {
    /// Fresh token checked; no dates/revision/content changed.
    Unchanged,
    /// Atomic publication/unlink confirmed.
    Published,
    /// Commit confirmed, with durability/cleanup/chronology diagnostics.
    PublishedWithWarning,
    /// OS result and reread could not establish the candidate's outcome.
    PublicationUnconfirmed,
}
/// A retained operation receipt. Candidate bytes are private recovery state.
#[derive(Debug, Clone)]
pub struct OperationReceipt {
    /// Stable affected key.
    pub key: LibraryKey,
    /// Unique operation revision, not a timestamp.
    pub operation_revision: RecordRevision,
    /// Requested lifecycle operation.
    pub kind: MutationKind,
    /// Token observed before the operation, when updating/purging.
    pub previous: Option<VersionToken>,
    /// Exact candidate token, absent for purge.
    pub candidate_token: Option<VersionToken>,
    /// Exact confirmed/proposed candidate; never the later editor draft.
    pub record: Option<LoadedRecord>,
    /// Relevant owned recovery stage, never automatically promoted.
    pub owned_stage: Option<String>,
    root: PathBuf,
    candidate_bytes: Option<Arc<[u8]>>,
}
/// One completed, unchanged or explicitly unconfirmed operation.
#[derive(Debug, Clone)]
pub struct WriteOutcome {
    /// Commit truth; unconfirmed is neither saved nor proof of no change.
    pub status: MutationStatus,
    /// Retained candidate/identity/version receipt for display and Recheck.
    pub receipt: OperationReceipt,
    /// Actual chronology/durability/reconciliation diagnostics.
    pub diagnostics: Vec<String>,
}
impl WriteOutcome {
    /// Saved baseline/index may advance only for these confirmed outcomes.
    #[must_use]
    pub const fn confirmed(&self) -> bool {
        !matches!(self.status, MutationStatus::PublicationUnconfirmed)
    }
}
/// An explicit reread cannot attribute a missing target after a lost/crashed purge.
#[derive(Debug, Clone)]
pub enum RecheckOutcome {
    /// Candidate bytes/revision observed exactly; commit confirmed.
    Published(Box<WriteOutcome>),
    /// Original expected bytes still observed; no candidate currently published.
    Original(LoadedRecord),
    /// Absence observed, without attributing a durable purge/publication receipt.
    Absent,
    /// Different saved content observed; retain draft and reconcile explicitly.
    Changed(LoadedRecord),
}
/// Persistent local store shared by native and command-line frontends.
#[derive(Debug, Clone)]
pub struct LibraryStore {
    root: LibraryRoot,
    reserved: Arc<BTreeSet<DocumentUuid>>,
    #[cfg(any(test, feature = "test-support"))]
    fault: Option<super::StoreFault>,
}
#[derive(Clone, Copy)]
struct ScanLimits {
    entries: usize,
    raw: usize,
    decoded: usize,
}
impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            entries: MAX_SCAN_RECORDS,
            raw: MAX_SCAN_RAW_BYTES,
            decoded: MAX_SCAN_DECODED_BYTES,
        }
    }
}
impl LibraryStore {
    /// Bind a resolved root; verify curated reserved identities without filesystem writes.
    ///
    /// # Errors
    /// Refuses an invalid shipped catalog rather than losing identity reservations.
    pub fn new(root: LibraryRoot) -> Result<Self, Error> {
        let mut reserved = BTreeSet::new();
        for entry in auditorylab_core::catalog::ENTRIES {
            let recipe = Recipe::parse(entry.text).map_err(|e| {
                Error::caused(
                    ErrorKind::InvalidRecipe,
                    "invalid shipped catalog identity",
                    e,
                )
            })?;
            if !reserved.insert(recipe.document.id) {
                return Err(Error::new(
                    ErrorKind::InvalidRecipe,
                    "duplicate shipped document UUID",
                ));
            }
        }
        Ok(Self {
            root,
            reserved: Arc::new(reserved),
            #[cfg(any(test, feature = "test-support"))]
            fault: None,
        })
    }
    /// Select a controlled fault around real filesystem operations for native tests.
    /// Product builds do not expose or inspect this test configuration.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn with_fault(mut self, fault: Option<super::StoreFault>) -> Self {
        self.fault = fault;
        self
    }

    /// Configured root, including override provenance.
    #[must_use]
    pub const fn root(&self) -> &LibraryRoot {
        &self.root
    }
    fn reserved(&self, id: DocumentUuid) -> Result<(), Error> {
        if self.reserved.contains(&id) {
            Err(Error::new(
                ErrorKind::Conflict,
                "ReservedIdentity: this document UUID belongs to a built-in; explicitly create a copy",
            ))
        } else {
            Ok(())
        }
    }
    /// Read a full managed snapshot under a shared root lock, without initialization.
    ///
    /// # Errors
    /// Reports missing/corrupt/unsupported targets, Busy or unavailable locking.
    pub fn load(&self, key: &LibraryKey) -> Result<LoadedRecord, Error> {
        let id = key.require_user()?;
        self.load_user(id).map_err(|error| {
            if error.kind() == ErrorKind::Unreadable {
                Error::caused(ErrorKind::IncompleteInventory, error.to_string(), error)
            } else {
                error
            }
        })
    }
    fn load_user(&self, id: DocumentUuid) -> Result<LoadedRecord, Error> {
        self.reserved(id)?;
        let lease = self.root.lock(false, false)?.ok_or_else(|| {
            Error::new(
                ErrorKind::UnknownRecipe,
                "managed recipe not found; library is not initialized",
            )
        })?;
        root::validate_marker(&lease.dir)?;
        let records = lease.records()?;
        let decoded = read_record(&records, id)?.ok_or_else(|| {
            Error::new(
                ErrorKind::UnknownRecipe,
                format!("unknown managed recipe user:{id}"),
            )
        })?;
        Ok(LoadedRecord::from_decoded(id, decoded))
    }
    /// Stream a bounded shared-lock inventory, retaining healthy summaries and errors.
    #[must_use]
    pub fn scan(&self, cancel: &Cancel) -> Inventory {
        self.scan_limited(cancel, ScanLimits::default())
    }
    fn scan_limited(&self, cancel: &Cancel, limits: ScanLimits) -> Inventory {
        let mut inventory = Inventory::default();
        if let Err(error) = self.scan_into(cancel, &mut inventory, limits) {
            let kind = if matches!(
                error.kind(),
                ErrorKind::Busy | ErrorKind::LockUnavailable | ErrorKind::Cancelled
            ) {
                error.kind()
            } else {
                ErrorKind::IncompleteInventory
            };
            inventory.error(self.root.path().display().to_string(), "root", error, kind);
        }
        inventory
    }
    fn scan_into(
        &self,
        cancel: &Cancel,
        inventory: &mut Inventory,
        limits: ScanLimits,
    ) -> Result<(), Error> {
        if cancel.is_requested() {
            return Err(Error::new(
                ErrorKind::Cancelled,
                "library discovery cancelled; inventory incomplete",
            ));
        }
        let Some(lease) = self.root.lock(false, false)? else {
            inventory.not_initialized = true;
            if self.root.missing_is_empty() {
                return Ok(());
            }
            return Err(Error::new(
                ErrorKind::IncompleteInventory,
                "configured library root is missing; use library init, no fallback was used",
            ));
        };
        root::validate_marker(&lease.dir)?;
        let records = lease.records()?;
        let mut count = 0;
        let mut raw = 0;
        let mut decoded_bytes = 0;
        for entry in lease
            .dir
            .entries()
            .map_err(|e| root::io("cannot enumerate library root artifacts", e))?
        {
            let entry = entry.map_err(|e| root::io("cannot inspect library root artifact", e))?;
            let name = entry.file_name();
            if name == "library.json" || name == ".library.lock" || name == "records" {
                continue;
            }
            count += 1;
            if count > limits.entries {
                return Err(Error::new(
                    ErrorKind::IncompleteInventory,
                    "directory-entry scan budget exhausted",
                ));
            }
            if cancel.is_requested() {
                return Err(Error::new(
                    ErrorKind::Cancelled,
                    "library discovery cancelled; inventory incomplete",
                ));
            }
            let name = name.to_string_lossy();
            if name.ends_with(".partial") {
                inventory.stages.push(format!("root/{name}"));
            } else {
                inventory.diagnostics.push(LibraryDiagnostic {
                    storage_id: format!("root/{name}"),
                    code: "unrecognized_artifact".into(),
                    message: "Unrecognized root artifact retained; not a managed record.".into(),
                });
            }
        }
        for entry in records
            .entries()
            .map_err(|e| root::io("cannot enumerate managed records", e))?
        {
            if cancel.is_requested() {
                return Err(Error::new(
                    ErrorKind::Cancelled,
                    "library discovery cancelled; inventory incomplete",
                ));
            }
            let entry = entry.map_err(|e| root::io("managed directory enumeration failed", e))?;
            count += 1;
            if count > limits.entries {
                return Err(Error::new(
                    ErrorKind::IncompleteInventory,
                    "directory-entry scan budget exhausted",
                ));
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                inventory.error(
                    "non-UTF8 storage entry",
                    "filename",
                    "invalid managed storage filename",
                    ErrorKind::IncompleteInventory,
                );
                continue;
            };
            if name.ends_with(".partial") {
                inventory.stages.push(name.into());
                continue;
            }
            let Some(stem) = name.strip_suffix(".json") else {
                inventory.diagnostics.push(LibraryDiagnostic {
                    storage_id: name.into(),
                    code: "unrecognized_artifact".into(),
                    message: "Unrecognized artifact retained; not a managed record.".into(),
                });
                continue;
            };
            let id = match stem.parse::<DocumentUuid>() {
                Ok(id) => id,
                Err(_) => {
                    inventory.error(
                        name,
                        "filename",
                        "expected canonical UUID.json; entry retained",
                        ErrorKind::IncompleteInventory,
                    );
                    continue;
                }
            };
            if self.reserved.contains(&id) {
                inventory.error(
                    name,
                    "reserved_identity",
                    "managed record uses a reserved builtin UUID; entry retained",
                    ErrorKind::IncompleteInventory,
                );
                continue;
            }
            if raw >= limits.raw {
                return Err(Error::new(
                    ErrorKind::IncompleteInventory,
                    "library byte scan budget exhausted",
                ));
            }
            if decoded_bytes >= limits.decoded {
                return Err(Error::new(
                    ErrorKind::IncompleteInventory,
                    "decoded recipe scan budget exhausted",
                ));
            }
            let bytes = match root::read_bounded(
                &records,
                name,
                record::MAX_RECORD_BYTES.min(limits.raw - raw),
            ) {
                Ok(bytes) => bytes,
                Err(e) => {
                    inventory.error(name, "record_read", e, ErrorKind::IncompleteInventory);
                    continue;
                }
            };
            raw += bytes.len();
            let envelope = match record::decode_envelope(id, &bytes) {
                Ok(value) => value,
                Err(e) => {
                    inventory.error(name, "record_contract", e, ErrorKind::IncompleteInventory);
                    continue;
                }
            };
            if envelope.recipe_toml.len() > limits.decoded - decoded_bytes {
                return Err(Error::new(
                    ErrorKind::IncompleteInventory,
                    "decoded recipe scan budget exhausted",
                ));
            }
            decoded_bytes += envelope.recipe_toml.len();
            let decoded = match record::decode_recipe(id, envelope, &bytes) {
                Ok(decoded) => decoded,
                Err(error) => {
                    inventory.error(
                        name,
                        "record_contract",
                        error,
                        ErrorKind::IncompleteInventory,
                    );
                    continue;
                }
            };
            inventory
                .entries
                .push(LoadedRecord::from_decoded(id, decoded).summary());
        }
        inventory.entries.sort_by(|a, b| {
            a.normalized_title
                .cmp(&b.normalized_title)
                .then_with(|| a.key.cmp(&b.key))
        });
        inventory.stages.sort();
        Ok(())
    }
    /// Apply one immutable intent; all existing mutations recheck revision and digest
    /// even for no-op. No multi-record transaction or external-editor exclusion claimed.
    ///
    /// # Errors
    /// Before-publication rejection/failure leaves saved bytes untouched. An ambiguous
    /// publication returns an explicit unconfirmed receipt instead of this error path.
    pub fn apply(&self, mutation: Mutation, cancel: &Cancel) -> Result<WriteOutcome, Error> {
        let (key, kind, expected) = match &mutation {
            Mutation::Create(recipe) => (
                LibraryKey::User(recipe.document.id),
                MutationKind::Create,
                None,
            ),
            Mutation::Add(recipe) => (
                LibraryKey::User(recipe.document.id),
                MutationKind::Add,
                None,
            ),
            Mutation::Update { key, expect, .. } => {
                (key.clone(), MutationKind::Update, Some(expect.clone()))
            }
            Mutation::Rename { key, expect, .. } => {
                (key.clone(), MutationKind::Rename, Some(expect.clone()))
            }
            Mutation::Trash { key, expect } => {
                (key.clone(), MutationKind::Trash, Some(expect.clone()))
            }
            Mutation::Restore { key, expect } => {
                (key.clone(), MutationKind::Restore, Some(expect.clone()))
            }
            Mutation::Purge { key, expect, .. } => {
                (key.clone(), MutationKind::Purge, Some(expect.clone()))
            }
        };
        let id = key.require_user()?;
        self.reserved(id)?;
        match &mutation {
            Mutation::Create(recipe) | Mutation::Add(recipe) | Mutation::Update { recipe, .. } => {
                recipe
                    .validate()
                    .map_err(|e| Error::caused(ErrorKind::InvalidRecipe, e.to_string(), e))?;
                if recipe.document.id != id {
                    return Err(Error::new(
                        ErrorKind::Conflict,
                        "proposed document identity differs from the selected User key",
                    ));
                }
            }
            _ => {}
        }
        if matches!(
            mutation,
            Mutation::Purge {
                confirmed: false,
                ..
            }
        ) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "permanent deletion requires explicit confirmation",
            ));
        }
        if cancel.is_requested() {
            return Err(Error::new(
                ErrorKind::Cancelled,
                "library operation cancelled before publication",
            ));
        }
        let mut bootstrap = Vec::new();
        let lease = match self.root.lock(true, false)? {
            Some(lease) => lease,
            None => {
                if expected.is_some() {
                    return Err(Error::new(
                        ErrorKind::Conflict,
                        "library/record disappeared; existing identity was not recreated",
                    ));
                }
                if !self.root.missing_is_empty() {
                    return Err(Error::new(
                        ErrorKind::IncompleteInventory,
                        "explicit library root needs library init before mutation",
                    ));
                }
                bootstrap = self.root.initialize()?.warnings;
                self.root.lock(true, false)?.ok_or_else(|| {
                    Error::new(ErrorKind::Write, "initialized library root disappeared")
                })?
            }
        };
        root::validate_marker(&lease.dir)?;
        let records = lease.records()?;
        let old = read_record(&records, id)?;
        if let Some(expect) = &expected
            && old.as_ref().is_none_or(|old| old.token != *expect)
        {
            return Err(Error::new(
                ErrorKind::Conflict,
                "Conflict: saved record changed or disappeared; retain the draft and reload/reconcile or save a new copy",
            ));
        }
        if let Some(old) = old.as_ref()
            && matches!(kind, MutationKind::Create | MutationKind::Add)
        {
            let incoming = match &mutation {
                Mutation::Create(recipe) | Mutation::Add(recipe) => recipe,
                _ => unreachable!("creation kinds above"),
            };
            if kind == MutationKind::Add
                && old.envelope.trashed_at.0.is_none()
                && old.recipe.same_document(incoming)
            {
                return Ok(unchanged(
                    key,
                    kind,
                    LoadedRecord::from_decoded(id, old.clone()),
                    &lease.canonical,
                ));
            }
            return Err(Error::new(
                ErrorKind::Conflict,
                "Identity collision (including Trash); keep existing, open separately, restore explicitly or add a new copy",
            ));
        }
        if matches!(kind, MutationKind::Purge) {
            let old =
                old.ok_or_else(|| Error::new(ErrorKind::Conflict, "selected record disappeared"))?;
            return self.purge(key, old, expected, bootstrap, &lease, &records);
        }
        let mut recipe = match mutation {
            Mutation::Create(recipe) | Mutation::Add(recipe) | Mutation::Update { recipe, .. } => {
                (*recipe).clone()
            }
            Mutation::Rename { title, .. } => {
                let mut recipe = old
                    .as_ref()
                    .ok_or_else(|| Error::new(ErrorKind::Conflict, "selected record disappeared"))?
                    .recipe
                    .clone();
                let notes = recipe.description.as_ref().and_then(|d| d.notes.clone());
                recipe.description = Some(crate::Description { title, notes });
                recipe
            }
            Mutation::Trash { .. } | Mutation::Restore { .. } => old
                .as_ref()
                .ok_or_else(|| Error::new(ErrorKind::Conflict, "selected record disappeared"))?
                .recipe
                .clone(),
            Mutation::Purge { .. } => unreachable!("purge handled above"),
        };
        recipe
            .validate()
            .map_err(|e| Error::caused(ErrorKind::InvalidRecipe, e.to_string(), e))?;
        if let Some(old) = &old {
            if recipe.document.created_at != old.recipe.document.created_at {
                return Err(Error::new(
                    ErrorKind::HistoryConflict,
                    "HistoryConflict: update cannot replace this identity's creation history",
                ));
            }
            if matches!(kind, MutationKind::Update | MutationKind::Rename)
                && old.envelope.trashed_at.0.is_some()
            {
                return Err(Error::new(
                    ErrorKind::Conflict,
                    "selected record is in Trash; restore it or create a copy before editing",
                ));
            }
            let noop = match kind {
                MutationKind::Trash => old.envelope.trashed_at.0.is_some(),
                MutationKind::Restore => old.envelope.trashed_at.0.is_none(),
                _ => old.recipe.same_editable_content(&recipe),
            };
            if noop {
                return Ok(unchanged(
                    key,
                    kind,
                    LoadedRecord::from_decoded(id, old.clone()),
                    &lease.canonical,
                ));
            }
            if !old.recipe.stimulus.same_content(&recipe.stimulus) {
                for e in &mut recipe.evidence {
                    e.scope = crate::EvidenceScope::Ancestor;
                }
            }
        }
        let now = self.now()?;
        if kind == MutationKind::Create {
            recipe.document.created_at = Some(now);
            recipe.document.modified_at = Some(now);
        } else if matches!(kind, MutationKind::Update | MutationKind::Rename) {
            recipe.document.modified_at = Some(now);
        }
        let added_at = old.as_ref().map_or(now, |old| old.envelope.added_at);
        let trashed_at = match kind {
            MutationKind::Trash => Some(now),
            MutationKind::Restore => None,
            _ => old.as_ref().and_then(|old| old.envelope.trashed_at.0),
        };
        let revision = self.revision()?;
        let recipe_toml = recipe
            .to_toml()
            .map_err(|e| Error::caused(ErrorKind::InvalidRecipe, e.to_string(), e))?;
        let envelope = Envelope {
            record_format: record::RECORD_FORMAT,
            revision,
            added_at,
            trashed_at: Nullable(trashed_at),
            recipe_toml,
        };
        let bytes = record::encode(&envelope)?;
        let decoded = record::decode(id, &bytes)?;
        let record = LoadedRecord::from_decoded(id, decoded);
        bootstrap.extend(chronology(
            &recipe,
            old.as_ref().map(|old| &old.recipe),
            now,
        ));
        let candidate = Candidate {
            name: record::filename(id),
            stage: format!(".{id}.{revision}.partial"),
            bytes: Arc::from(bytes),
            record,
            kind,
            previous: expected,
            root: lease.canonical.clone(),
            diagnostics: bootstrap,
        };
        self.publish(&lease, &records, candidate, cancel)
    }
    fn publish(
        &self,
        lease: &root::Lease,
        records: &Dir,
        candidate: Candidate,
        cancel: &Cancel,
    ) -> Result<WriteOutcome, Error> {
        let mut file = root::create_stage(records, &candidate.stage)?;
        let staged_write = self.write_stage(&mut file, &candidate.bytes);
        if let Err(e) = staged_write {
            drop(file);
            return Err(cleanup_error(
                records,
                &candidate,
                "failed before publication",
                e,
            ));
        }
        match root::read_bounded(records, &candidate.stage, record::MAX_RECORD_BYTES) {
            Ok(bytes) if bytes.as_slice() == candidate.bytes.as_ref() => {}
            result => {
                drop(file);
                let failure = match result {
                    Err(error) => error,
                    Ok(_) => Error::new(
                        ErrorKind::Write,
                        "staged candidate bytes differ from the immutable candidate",
                    ),
                };
                let cleanup = records.remove_file(&candidate.stage);
                return Err(match cleanup {
                    Ok(()) => failure,
                    Err(e) => failure
                        .with_leftover(candidate.root.join("records").join(&candidate.stage), e),
                });
            }
        }
        #[cfg(any(test, feature = "test-support"))]
        if self.fault == Some(super::StoreFault::CancelBeforePublication) {
            cancel.request();
        }
        if cancel.is_requested() {
            drop(file);
            let cleanup = records.remove_file(&candidate.stage);
            return match cleanup {
                Ok(()) => Err(Error::new(
                    ErrorKind::Cancelled,
                    "library operation cancelled before publication",
                )),
                Err(e) => Err(Error::caused(
                    ErrorKind::Write,
                    format!(
                        "cancelled before publication; owned stage {} remains",
                        candidate.stage
                    ),
                    e,
                )),
            };
        }
        #[cfg(any(test, feature = "test-support"))]
        self.pause_for_kill_test(super::StoreFault::PauseBeforePublication);
        let publication = self.publish_native(records, &candidate);
        #[cfg(any(test, feature = "test-support"))]
        if publication.is_ok() {
            self.pause_for_kill_test(super::StoreFault::PauseAfterPublication);
        }
        drop(file);
        if let Err(error) = publication {
            let observed = self.reread_after_publication(records, &candidate.name);
            match observed {
                Ok(bytes) if bytes.as_slice() == candidate.bytes.as_ref() => {
                    let mut candidate = candidate;
                    candidate.diagnostics.push(format!(
                        "Publication confirmed by exact reread after OS error: {error}"
                    ));
                    return Ok(self.finish_publication(records, candidate));
                }
                Ok(bytes)
                    if candidate.previous.as_ref().is_some_and(|token| {
                        token.fingerprint() == record::Fingerprint::of(&bytes)
                    }) =>
                {
                    return Err(cleanup_error(
                        records,
                        &candidate,
                        "OS publication failed; original exact bytes confirmed",
                        error,
                    ));
                }
                Err(ref read_error) if is_missing(read_error) && candidate.previous.is_none() => {
                    return Err(cleanup_error(
                        records,
                        &candidate,
                        "new-record publication failed; absence confirmed under the same lock",
                        error,
                    ));
                }
                _ => {}
            }
            if error.kind() == std::io::ErrorKind::AlreadyExists && candidate.previous.is_none() {
                let cleanup = records.remove_file(&candidate.stage);
                let failure = Error::caused(
                    ErrorKind::Conflict,
                    "destination collision; existing record was not replaced",
                    error,
                );
                return Err(match cleanup {
                    Ok(()) => failure,
                    Err(e) => failure
                        .with_leftover(candidate.root.join("records").join(&candidate.stage), e),
                });
            }
            let receipt = candidate.receipt(Some(candidate.stage.clone()));
            return Ok(WriteOutcome {
                status: MutationStatus::PublicationUnconfirmed,
                receipt,
                diagnostics: vec![format!(
                    "PublicationUnconfirmed: {error}; destination reread could not establish the candidate. Retain draft/receipt and Recheck; do not retry blindly."
                )],
            });
        }
        let _ = lease;
        Ok(self.finish_publication(records, candidate))
    }
    fn purge(
        &self,
        key: LibraryKey,
        old: Decoded,
        expected: Option<VersionToken>,
        mut diagnostics: Vec<String>,
        lease: &root::Lease,
        records: &Dir,
    ) -> Result<WriteOutcome, Error> {
        if old.envelope.trashed_at.0.is_none() {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "permanent deletion is allowed only for a trashed User record",
            ));
        }
        let id = key.require_user()?;
        let operation_revision = self.revision()?;
        let name = record::filename(id);
        let record = LoadedRecord::from_decoded(id, old);
        let receipt = OperationReceipt {
            key,
            operation_revision,
            kind: MutationKind::Purge,
            previous: expected,
            candidate_token: None,
            record: Some(record),
            owned_stage: None,
            root: lease.canonical.clone(),
            candidate_bytes: None,
        };
        if let Err(error) = self.unlink_native(records, &name) {
            if self.reread_after_purge(records, id).is_ok_and(|current| {
                current.is_some_and(|value| receipt.previous.as_ref() == Some(&value.token))
            }) {
                return Err(Error::caused(
                    ErrorKind::Write,
                    "purge failed before confirmed unlink; original exact record observed",
                    error,
                ));
            }
            return Ok(WriteOutcome {
                status: MutationStatus::PublicationUnconfirmed,
                receipt,
                diagnostics: vec![format!(
                    "Purge outcome not confirmed: {error}; observe current state with Recheck, never recreate the record to retry."
                )],
            });
        }
        #[cfg(any(test, feature = "test-support"))]
        self.pause_for_kill_test(super::StoreFault::PauseAfterPurge);
        if let Err(error) = self.sync_records(records) {
            diagnostics.push(format!(
                "Permanently removed; directory durability warning: {error}"
            ));
        }
        Ok(WriteOutcome {
            status: if diagnostics.is_empty() {
                MutationStatus::Published
            } else {
                MutationStatus::PublishedWithWarning
            },
            receipt,
            diagnostics,
        })
    }
    /// Recheck a retained uncertain receipt, without assigning dates or writing bytes.
    ///
    /// # Errors
    /// Reports root mismatch, unavailable locking or unreadable current record.
    pub fn recheck(&self, receipt: &OperationReceipt) -> Result<RecheckOutcome, Error> {
        let lease = self.root.lock(false, false)?.ok_or_else(|| {
            Error::new(
                ErrorKind::IncompleteInventory,
                "library unavailable; outcome remains unconfirmed",
            )
        })?;
        if lease.canonical != receipt.root {
            return Err(Error::new(
                ErrorKind::Conflict,
                "receipt belongs to another library root",
            ));
        }
        root::validate_marker(&lease.dir)?;
        let records = lease.records()?;
        let id = receipt.key.require_user()?;
        let Some(bytes) = read_optional(&records, &record::filename(id))? else {
            return Ok(RecheckOutcome::Absent);
        };
        let decoded = record::decode(id, &bytes)?;
        let current = LoadedRecord::from_decoded(id, decoded);
        if receipt
            .candidate_bytes
            .as_ref()
            .is_some_and(|candidate| candidate.as_ref() == bytes)
        {
            return Ok(RecheckOutcome::Published(Box::new(WriteOutcome {status:MutationStatus::PublishedWithWarning,receipt:OperationReceipt {record:Some(current),..receipt.clone()},diagnostics:vec!["Exact candidate observed by Recheck; earlier directory durability/reporting is not inferred.".into()]})));
        }
        if receipt.previous.as_ref() == Some(&current.version_token) {
            Ok(RecheckOutcome::Original(current))
        } else {
            Ok(RecheckOutcome::Changed(current))
        }
    }
}
struct Candidate {
    name: String,
    stage: String,
    bytes: Arc<[u8]>,
    record: LoadedRecord,
    kind: MutationKind,
    previous: Option<VersionToken>,
    root: PathBuf,
    diagnostics: Vec<String>,
}
impl Candidate {
    fn receipt(&self, owned_stage: Option<String>) -> OperationReceipt {
        OperationReceipt {
            key: self.record.key.clone(),
            operation_revision: self.record.version_token.revision(),
            kind: self.kind,
            previous: self.previous.clone(),
            candidate_token: Some(self.record.version_token.clone()),
            record: Some(self.record.clone()),
            owned_stage,
            root: self.root.clone(),
            candidate_bytes: Some(self.bytes.clone()),
        }
    }
}
impl LibraryStore {
    fn finish_publication(&self, records: &Dir, mut candidate: Candidate) -> WriteOutcome {
        let stage = match self.cleanup_stage(records, &candidate.stage) {
            Ok(()) => None,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                candidate.diagnostics.push(format!(
                    "Saved; owned stage {} cleanup warning: {e}",
                    candidate.stage
                ));
                Some(candidate.stage.clone())
            }
        };
        if let Err(e) = self.sync_records(records) {
            candidate
                .diagnostics
                .push(format!("Saved; directory durability warning: {e}"));
        }
        let receipt = candidate.receipt(stage);
        WriteOutcome {
            status: if candidate.diagnostics.is_empty() {
                MutationStatus::Published
            } else {
                MutationStatus::PublishedWithWarning
            },
            receipt,
            diagnostics: candidate.diagnostics,
        }
    }
}
fn sync_dir(dir: &Dir) -> std::io::Result<()> {
    dir.try_clone()?.into_std_file().sync_all()
}
fn unchanged(
    key: LibraryKey,
    kind: MutationKind,
    record: LoadedRecord,
    root: &std::path::Path,
) -> WriteOutcome {
    let token = record.version_token.clone();
    WriteOutcome {
        status: MutationStatus::Unchanged,
        receipt: OperationReceipt {
            key,
            operation_revision: token.revision(),
            kind,
            previous: Some(token.clone()),
            candidate_token: Some(token),
            record: Some(record),
            owned_stage: None,
            root: root.into(),
            candidate_bytes: None,
        },
        diagnostics: Vec::new(),
    }
}
fn read_optional(dir: &Dir, name: &str) -> Result<Option<Vec<u8>>, Error> {
    match root::read_bounded(dir, name, record::MAX_RECORD_BYTES) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if is_missing(&e) => Ok(None),
        Err(e) => Err(e),
    }
}
fn read_record(dir: &Dir, id: DocumentUuid) -> Result<Option<Decoded>, Error> {
    read_optional(dir, &record::filename(id))?
        .map(|bytes| record::decode(id, &bytes))
        .transpose()
}
fn is_missing(error: &Error) -> bool {
    std::error::Error::source(error)
        .and_then(|e| e.downcast_ref::<std::io::Error>())
        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
}
fn cleanup_error(
    records: &Dir,
    candidate: &Candidate,
    context: &str,
    error: std::io::Error,
) -> Error {
    let failure = Error::caused(ErrorKind::Write, format!("{context}: {error}"), error);
    match records.remove_file(&candidate.stage) {
        Ok(()) => failure,
        Err(e) => failure.with_leftover(candidate.root.join("records").join(&candidate.stage), e),
    }
}
fn chronology(recipe: &Recipe, previous: Option<&Recipe>, now: Timestamp) -> Vec<String> {
    let mut warnings = Vec::new();
    if recipe
        .document
        .created_at
        .zip(recipe.document.modified_at)
        .is_some_and(|(created, modified)| modified < created)
    {
        warnings.push(
            "ChronologyWarning: declared modification precedes creation; history was not clamped."
                .into(),
        );
    }
    if previous
        .and_then(|p| p.document.modified_at.or(p.document.created_at))
        .is_some_and(|prior| now < prior)
    {
        warnings.push("ClockRegression: observed clock is earlier than prior history; revision remains the conflict authority.".into());
    }
    if recipe.document.created_at.is_some_and(|date| date > now)
        || recipe.document.modified_at.is_some_and(|date| date > now)
    {
        warnings.push(
            "ChronologyWarning: declared history lies in the future; it was preserved.".into(),
        );
    }
    warnings
}
/// Inspect declared chronology without changing imported dates or fabricating history.
/// The observed clock is presentation context, never a version token.
#[must_use]
pub fn history_warnings(recipe: &Recipe) -> Vec<String> {
    observed_now().map_or_else(
        |error| {
            vec![format!(
                "Cannot compare declared history with the observed clock: {error}"
            )]
        },
        |now| chronology(recipe, None, now),
    )
}
fn observed_now() -> Result<Timestamp, Error> {
    let invalid = || {
        Error::new(
            ErrorKind::Write,
            "observed system time is outside the supported UTC timestamp range",
        )
    };
    let (seconds, nanos) = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
    {
        Ok(d) => (
            i64::try_from(d.as_secs()).map_err(|_| invalid())?,
            d.subsec_nanos(),
        ),
        Err(e) => {
            let d = e.duration();
            let seconds = i64::try_from(d.as_secs()).map_err(|_| invalid())?;
            if d.subsec_nanos() == 0 {
                (-seconds, 0)
            } else {
                (-seconds - 1, 1_000_000_000 - d.subsec_nanos())
            }
        }
    };
    Timestamp::from_unix(seconds, nanos).ok_or_else(invalid)
}

impl LibraryStore {
    #[cfg(any(test, feature = "test-support"))]
    fn pause_for_kill_test(&self, point: super::StoreFault) {
        if self.fault == Some(point) {
            let path = std::env::var_os("AUDITORYLAB_STORE_TEST_READY_FILE")
                .unwrap_or_else(|| panic!("controlled writer pause requires a parent test signal"));
            std::fs::write(path, b"ready")
                .unwrap_or_else(|e| panic!("cannot signal controlled parent: {e}"));
            loop {
                std::thread::park();
            }
        }
    }
    fn now(&self) -> Result<Timestamp, Error> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(super::StoreFault::Clock(now)) = self.fault {
            return Ok(now);
        }
        observed_now()
    }
    fn revision(&self) -> Result<RecordRevision, Error> {
        #[cfg(any(test, feature = "test-support"))]
        if self.fault == Some(super::StoreFault::RandomUnavailable) {
            return Err(Error::new(
                ErrorKind::Identity,
                "random revision acquisition failed; no fallback",
            ));
        }
        RecordRevision::fresh()
    }
    fn write_stage(&self, file: &mut cap_std::fs::File, bytes: &[u8]) -> std::io::Result<()> {
        #[cfg(any(test, feature = "test-support"))]
        if self.fault == Some(super::StoreFault::Write) {
            file.write_all(&bytes[..bytes.len() / 2])?;
            return Err(std::io::Error::other("injected partial native stage write"));
        }
        file.write_all(bytes)?;
        #[cfg(any(test, feature = "test-support"))]
        if self.fault == Some(super::StoreFault::Flush) {
            return Err(std::io::Error::other("injected stage flush failure"));
        }
        file.flush()?;
        #[cfg(any(test, feature = "test-support"))]
        if self.fault == Some(super::StoreFault::FileSync) {
            return Err(std::io::Error::other("injected stage file-sync failure"));
        }
        file.sync_all()
    }
    fn publish_native(&self, records: &Dir, candidate: &Candidate) -> std::io::Result<()> {
        #[cfg(any(test, feature = "test-support"))]
        {
            use super::StoreFault as F;
            if self.fault == Some(F::CollisionAfterStage) && candidate.previous.is_none() {
                let mut foreign = root::create_stage(records, &candidate.name)
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                foreign.write_all(b"foreign bytes must remain intact")?;
                foreign.sync_all()?;
            }
            if matches!(
                self.fault,
                Some(F::PublicationUncommittedError | F::PublicationUncommittedUnreadable)
            ) {
                return Err(std::io::Error::other(
                    "injected ambiguous result before native publication",
                ));
            }
        }
        if candidate.previous.is_some() {
            records.rename(&candidate.stage, records, &candidate.name)?;
        } else {
            records.hard_link(&candidate.stage, records, &candidate.name)?;
        }
        #[cfg(any(test, feature = "test-support"))]
        if matches!(
            self.fault,
            Some(
                super::StoreFault::PublicationCommittedError
                    | super::StoreFault::PublicationCommittedUnreadable
            )
        ) {
            return Err(std::io::Error::other(
                "injected ambiguous result after real native publication",
            ));
        }
        Ok(())
    }
    fn reread_after_publication(&self, records: &Dir, name: &str) -> Result<Vec<u8>, Error> {
        #[cfg(any(test, feature = "test-support"))]
        if matches!(
            self.fault,
            Some(
                super::StoreFault::PublicationCommittedUnreadable
                    | super::StoreFault::PublicationUncommittedUnreadable
            )
        ) {
            return Err(Error::new(
                ErrorKind::Unreadable,
                "injected unavailable immediate reread",
            ));
        }
        root::read_bounded(records, name, record::MAX_RECORD_BYTES)
    }
    fn reread_after_purge(
        &self,
        records: &Dir,
        id: DocumentUuid,
    ) -> Result<Option<Decoded>, Error> {
        #[cfg(any(test, feature = "test-support"))]
        if matches!(
            self.fault,
            Some(
                super::StoreFault::PurgeCommittedUnreadable
                    | super::StoreFault::PurgeUncommittedUnreadable
            )
        ) {
            return Err(Error::new(
                ErrorKind::Unreadable,
                "injected unavailable purge reread",
            ));
        }
        read_record(records, id)
    }
    fn sync_records(&self, records: &Dir) -> std::io::Result<()> {
        #[cfg(any(test, feature = "test-support"))]
        if self.fault == Some(super::StoreFault::DirectorySync) {
            return Err(std::io::Error::other(
                "injected post-publication directory-sync failure",
            ));
        }
        sync_dir(records)
    }
    fn cleanup_stage(&self, records: &Dir, stage: &str) -> std::io::Result<()> {
        #[cfg(any(test, feature = "test-support"))]
        if self.fault == Some(super::StoreFault::StageCleanup) {
            return Err(std::io::Error::other(
                "injected successful-publication stage-cleanup failure",
            ));
        }
        records.remove_file(stage)
    }
    fn unlink_native(&self, records: &Dir, name: &str) -> std::io::Result<()> {
        #[cfg(any(test, feature = "test-support"))]
        if self.fault == Some(super::StoreFault::PurgeUncommittedUnreadable) {
            return Err(std::io::Error::other(
                "injected ambiguous purge without unlink",
            ));
        }
        records.remove_file(name)?;
        #[cfg(any(test, feature = "test-support"))]
        if self.fault == Some(super::StoreFault::PurgeCommittedUnreadable) {
            return Err(std::io::Error::other(
                "injected ambiguous purge after real unlink",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod scan_budget_tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, LibraryStore) {
        let temp = tempfile::tempdir().unwrap();
        let store = LibraryStore::new(
            crate::library::LibraryRoot::explicit(temp.path().join("library")).unwrap(),
        )
        .unwrap();
        store.root().initialize().unwrap();
        (temp, store)
    }
    fn seed(store: &LibraryStore) -> LoadedRecord {
        store
            .apply(
                Mutation::Create(Arc::new(crate::Draft::new().unwrap().recipe())),
                &Cancel::default(),
            )
            .unwrap()
            .receipt
            .record
            .unwrap()
    }
    #[test]
    fn actual_record_count_raw_and_decoded_budgets_mark_incomplete() {
        let (_temp, store) = fixture();
        let first = seed(&store);
        seed(&store);
        let raw = usize::try_from(
            std::fs::metadata(
                store
                    .root()
                    .path()
                    .join("records")
                    .join(record::filename(first.snapshot.recipe().document.id)),
            )
            .unwrap()
            .len(),
        )
        .unwrap();
        let limits = [
            ScanLimits {
                entries: 1,
                ..ScanLimits::default()
            },
            ScanLimits {
                raw,
                ..ScanLimits::default()
            },
            ScanLimits {
                decoded: first.recipe_toml.len(),
                ..ScanLimits::default()
            },
        ];
        for limit in limits {
            let inventory = store.scan_limited(&Cancel::default(), limit);
            assert!(!inventory.complete);
            assert_eq!(inventory.entries.len(), 1);
            assert!(!inventory.diagnostics.is_empty());
        }
    }
    #[test]
    fn invalid_embedded_documents_still_consume_decode_budget() {
        let (_temp, store) = fixture();
        let records = [seed(&store), seed(&store)];
        for record in records {
            let path = store
                .root()
                .path()
                .join("records")
                .join(record::filename(record.snapshot.recipe().document.id));
            let mut value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            value["recipe_toml"] = serde_json::Value::String("invalid toml".into());
            std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        }
        let inventory = store.scan_limited(
            &Cancel::default(),
            ScanLimits {
                decoded: 12,
                ..ScanLimits::default()
            },
        );
        assert!(!inventory.complete);
        assert!(inventory.entries.is_empty());
        assert_eq!(inventory.diagnostics.len(), 2);
        assert!(
            inventory
                .diagnostics
                .iter()
                .any(|d| d.message.contains("decoded recipe scan budget"))
        );
    }
    #[test]
    fn unknown_artifacts_and_stages_cannot_bypass_work_and_memory_budget() {
        let (_temp, store) = fixture();
        for i in 0..3 {
            std::fs::write(
                store
                    .root()
                    .path()
                    .join("records")
                    .join(format!("stage-{i}.partial")),
                b"owned by nobody",
            )
            .unwrap();
        }
        let inventory = store.scan_limited(
            &Cancel::default(),
            ScanLimits {
                entries: 2,
                ..ScanLimits::default()
            },
        );
        assert!(!inventory.complete);
        assert_eq!(inventory.stages.len(), 2);
        let cancel = Cancel::default();
        cancel.request();
        let inventory = store.scan(&cancel);
        assert!(!inventory.complete);
        assert_eq!(inventory.failure_kind, Some(ErrorKind::Cancelled));
    }
    #[test]
    fn raw_readers_observe_complete_old_or_new_record_during_replace() {
        let (_temp, store) = fixture();
        let first = seed(&store);
        let path = store
            .root()
            .path()
            .join("records")
            .join(record::filename(first.snapshot.recipe().document.id));
        let (started, ready) = std::sync::mpsc::channel();
        let (finished, done) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut observed_new = false;
            let mut announced = false;
            while !observed_new {
                let bytes = std::fs::read(&path).unwrap();
                let outer: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let recipe = Recipe::parse(outer["recipe_toml"].as_str().unwrap()).unwrap();
                let title = &recipe.description.as_ref().unwrap().title;
                assert!(title == "Untitled sound" || title == "atomic replacement");
                if !announced {
                    assert_eq!(title, "Untitled sound");
                    started.send(()).unwrap();
                    announced = true;
                }
                observed_new = title == "atomic replacement";
            }
            finished.send(()).unwrap();
        });
        ready
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        store
            .apply(
                Mutation::Rename {
                    key: first.key,
                    expect: first.version_token,
                    title: "atomic replacement".into(),
                },
                &Cancel::default(),
            )
            .unwrap();
        done.recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        reader.join().unwrap();
    }
}
