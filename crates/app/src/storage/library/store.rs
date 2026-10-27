//! Bounded plain-recipe inventory and locked single-entry mutations.
use super::{LibraryRoot, root};
use crate::clock::observed_now;
use crate::core::{DocumentUuid, Recipe, Timestamp};
use crate::discovery::collection::EntrySummary;
use crate::document::origin::{LibraryKey, VersionToken};
use crate::document::recipe::{Origin, RecipeSnapshot};
use crate::job::Cancel;
use crate::storage::publish::{self, Policy};
use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
    sync::Arc,
};
use {
    crate::outcome::Outcome, crate::storage::error::FileError, crate::storage::error::IoFailure,
    crate::storage::error::IoOperation, crate::storage::library::LibraryError as Error,
};

/// Maximum recipe candidates examined in one scan.
pub(crate) const MAX_SCAN_RECORDS: usize = 10_000;
/// Maximum exact recipe bytes consumed in one scan.
pub(crate) const MAX_SCAN_RAW_BYTES: usize = 256 * 1024 * 1024;
/// Maximum decoded recipe bytes admitted in one scan.
pub(crate) const MAX_SCAN_DECODED_BYTES: usize = 128 * 1024 * 1024;
const MEMBERSHIP: &str = "membership.toml";
const MAX_MEMBERSHIP_BYTES: usize = 4 * 1024 * 1024;
/// A bounded inventory observation.
#[derive(Debug, Clone)]
pub enum InventoryNotice {
    UnknownRootEntry,
    UnknownEntry,
    UuidFilenameRequired,
    ReservedBuiltinIdentity,
    /// Both directories link to the same file; Trash owns membership.
    InterruptedMove,
    /// Active and Trash contain different files with the same UUID.
    DuplicateIdentity,
    /// Membership has no corresponding recipe.
    OrphanMembership(crate::core::DocumentUuid),
}
/// A nonfatal inventory notice or a typed operation failure.
#[derive(Debug, Clone)]
pub enum DiagnosticDetail<E = Error> {
    /// A descriptive inventory notice.
    Notice(InventoryNotice),
    /// Confirmed storage outcome.
    Warning(crate::storage::warning::StorageWarning),
    /// A failure with its exact typed cause.
    Failure(Box<E>),
}
impl From<InventoryNotice> for DiagnosticDetail {
    fn from(value: InventoryNotice) -> Self {
        Self::Notice(value)
    }
}
impl From<Error> for DiagnosticDetail {
    fn from(value: Error) -> Self {
        Self::Failure(Box::new(value))
    }
}
impl From<FileError> for DiagnosticDetail {
    fn from(value: FileError) -> Self {
        Self::Failure(Box::new(value.into()))
    }
}
/// One storage diagnostic retains its cause separately from human rendering.
#[derive(Debug, Clone)]
pub struct LibraryDiagnostic<E = Error> {
    /// Root or relative storage identifier.
    pub storage_id: String,
    /// Inventory diagnostic category.
    pub code: String,
    /// Exact notice or operation failure.
    pub detail: DiagnosticDetail<E>,
}
/// Disposable bounded index; incomplete never means an empty library.
#[derive(Debug, Clone)]
pub struct Inventory {
    /// Healthy entries from both recipe directories.
    pub entries: Vec<EntrySummary>,
    /// Storage, chronology and discovery problems.
    pub diagnostics: Vec<LibraryDiagnostic>,
    /// Owned stages reported without promoting them.
    pub stages: Vec<std::path::PathBuf>,
    /// All eligible entries were inspected within the limits.
    pub complete: bool,
    /// Missing root has not been initialized.
    pub not_initialized: bool,
    /// Failure class for CLI exit policy.
    pub failure_kind: Option<Outcome>,
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
        path: impl Into<String>,
        code: &str,
        error: impl Into<DiagnosticDetail>,
        kind: Outcome,
    ) {
        self.complete = false;
        self.failure_kind = Some(kind);
        self.note(path, code, error);
    }
    fn note(&mut self, path: impl Into<String>, code: &str, message: impl Into<DiagnosticDetail>) {
        self.diagnostics.push(LibraryDiagnostic {
            storage_id: path.into(),
            code: code.into(),
            detail: message.into(),
        });
    }
}
/// One immutable selected portable recipe and its observed local membership.
#[derive(Debug, Clone)]
pub struct LoadedRecord {
    /// Storage-owned managed key.
    pub key: LibraryKey,
    /// SHA-256 of exact file bytes.
    pub version_token: VersionToken,
    /// Local addition time; missing metadata means Unknown.
    pub added_at: Option<Timestamp>,
    /// Local Trash time; missing metadata means Unknown.
    pub trashed_at: Option<Timestamp>,
    /// Authoritative directory state, independent of optional timestamps.
    pub trashed: bool,
    /// Captured full content.
    pub snapshot: RecipeSnapshot,
    /// Exact portable UTF-8 bytes, including comments and whitespace.
    pub recipe_toml: Arc<str>,
}
impl LoadedRecord {
    fn decode(
        id: DocumentUuid,
        path: &Path,
        bytes: Vec<u8>,
        trashed: bool,
        membership: Option<&Membership>,
    ) -> Result<Self, Error> {
        let recipe = Recipe::from_bytes(&bytes).map_err(|error| Error::RecipeFile {
            path: path.to_owned(),
            error,
        })?;
        if recipe.document().id != id {
            return Err(Error::RecordIdentity {
                path: path.to_owned(),
                expected: id,
                found: recipe.document().id,
            });
        }
        let version_token = VersionToken::observed(&bytes);
        let recipe_toml = String::from_utf8(bytes).map_err(|e| Error::RecipeUtf8 {
            path: path.to_owned(),
            error: e,
        })?;
        let snapshot = RecipeSnapshot::new(
            Origin::User {
                id,
                version: version_token,
            },
            recipe,
        );
        Ok(Self {
            key: LibraryKey::User(id),
            version_token,
            added_at: membership.and_then(|m| m.added_at),
            trashed_at: if trashed {
                membership.and_then(|m| m.trashed_at)
            } else {
                None
            },
            trashed,
            snapshot,
            recipe_toml: Arc::from(recipe_toml),
        })
    }
    /// Compact typed index projection without realizing audio.
    #[must_use]
    pub fn summary(&self) -> EntrySummary {
        let mut summary = EntrySummary::from_snapshot(self.key.clone(), &self.snapshot, None);
        summary.added_at = self.added_at;
        summary.trashed_at = self.trashed_at;
        summary.trashed = self.trashed;
        summary.version_token = Some(self.version_token);
        summary
    }
}
/// Captured mutation intent. Expected bytes are optional for CLI optimistic checks.
#[derive(Debug, Clone)]
pub enum Mutation {
    /// Create a new authored identity and its initial dates.
    Create(Arc<Recipe>),
    /// Add portable content without changing declared history.
    Add(Arc<Recipe>),
    /// Replace the editable content of an existing active entry.
    Update {
        /// Existing managed identity.
        key: LibraryKey,
        /// Optional observed-byte token.
        expect: Option<VersionToken>,
        /// Proposed complete content.
        recipe: Arc<Recipe>,
    },
    /// Change only the title.
    Rename {
        /// Existing managed identity.
        key: LibraryKey,
        /// Optional observed-byte token.
        expect: Option<VersionToken>,
        /// Nonempty title.
        title: String,
    },
    /// Move an active entry into Trash.
    Trash {
        /// Existing managed identity.
        key: LibraryKey,
        /// Optional observed-byte token.
        expect: Option<VersionToken>,
    },
    /// Move a trashed entry into the active directory.
    Restore {
        /// Existing managed identity.
        key: LibraryKey,
        /// Optional observed-byte token.
        expect: Option<VersionToken>,
    },
    /// Permanently remove a trashed entry.
    Purge {
        /// Existing managed identity.
        key: LibraryKey,
        /// Optional observed-byte token.
        expect: Option<VersionToken>,
        /// Explicit deletion confirmation.
        confirmed: bool,
    },
}
/// Requested library operation class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    /// New authored identity.
    Create,
    /// Imported identity.
    Add,
    /// Edited content.
    Update,
    /// Title change.
    Rename,
    /// Soft deletion.
    Trash,
    /// Restoration.
    Restore,
    /// Permanent deletion.
    Purge,
}
/// Confirmed publication truth, independent of terminal or GUI reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationStatus {
    /// The selected recipe bytes and its membership dates are unchanged.
    Unchanged,
    /// Publication or removal succeeded.
    Published,
    /// Saved with durability, cleanup or chronology warnings.
    PublishedWithWarning,
}
impl MutationStatus {
    /// Listener-facing publication status.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unchanged => crate::core::vocabulary::Status::Unchanged.label(),
            Self::Published => crate::core::vocabulary::Status::Saved.label(),
            Self::PublishedWithWarning => crate::core::vocabulary::Status::SavedWithWarning.label(),
        }
    }
}
/// A completed mutation; failures are ordinary path-specific errors.
#[derive(Debug, Clone)]
pub struct WriteOutcome {
    /// Confirmed publication state.
    pub status: MutationStatus,
    /// Managed key.
    pub key: LibraryKey,
    /// Requested operation.
    pub kind: MutationKind,
    /// Confirmed current content, absent after purge.
    pub record: Option<LoadedRecord>,
    /// Durability, cleanup, chronology and metadata warnings.
    pub diagnostics: Vec<crate::storage::warning::StorageWarning>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Membership {
    #[serde(skip_serializing_if = "Option::is_none")]
    added_at: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trashed_at: Option<Timestamp>,
}
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Memberships {
    #[serde(default)]
    entries: BTreeMap<DocumentUuid, Membership>,
}
/// Rooted native store shared by both frontends.
#[derive(Debug, Clone)]
pub(crate) struct LibraryStore {
    root: LibraryRoot,
    reserved: Arc<BTreeSet<DocumentUuid>>,
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
    /// Bind a root and validate the immutable reserved catalog identities.
    ///
    /// # Errors
    /// Refuses corrupt or duplicate built-in identities without filesystem mutation.
    pub(crate) fn new(root: LibraryRoot) -> Result<Self, Error> {
        let mut reserved = BTreeSet::new();
        for entry in crate::core::catalog::ENTRIES {
            let recipe =
                Recipe::parse(entry.text).map_err(|e| Error::InvalidBuiltin { error: e })?;
            if !reserved.insert(recipe.document().id) {
                return Err(Error::DuplicateBuiltin {
                    id: recipe.document().id,
                });
            }
        }
        Ok(Self {
            root,
            reserved: Arc::new(reserved),
        })
    }
    /// Configured root and its provenance.
    #[must_use]
    pub(crate) const fn root(&self) -> &LibraryRoot {
        &self.root
    }
    fn reserved(&self, id: DocumentUuid) -> Result<(), Error> {
        if self.reserved.contains(&id) {
            Err(Error::ReservedBuiltin { id })
        } else {
            Ok(())
        }
    }
    /// Load exact portable bytes under a shared root lock.
    ///
    /// # Errors
    /// Reports missing, malformed, conflicting or unsafe entries and lock failures.
    pub(crate) fn load(&self, key: &LibraryKey) -> Result<LoadedRecord, Error> {
        let id = crate::storage::library::require_user(key)?;
        self.reserved(id)?;
        (|| {
            let lease = self.root.lock(false, false)?.ok_or(Error::NotInitialized)?;
            root::validate_marker(&lease.dir)?;
            let membership = read_memberships(&lease.dir)?;
            read_record(&lease, id, &membership)?.ok_or(Error::UnknownManaged { id })
        })()
        .map_err(|error| {
            if matches!(error.outcome(), Outcome::Write | Outcome::Unreadable) {
                Error::IncompleteRead {
                    error: Box::new(error),
                }
            } else {
                error
            }
        })
    }
    /// Stream bounded summaries without changing any files.
    #[must_use]
    pub(crate) fn scan(&self, cancel: &Cancel) -> Inventory {
        self.scan_limited(cancel, ScanLimits::default())
    }
    fn scan_limited(&self, cancel: &Cancel, limits: ScanLimits) -> Inventory {
        let mut result = Inventory::default();
        if let Err(error) = self.scan_into(cancel, &mut result, limits) {
            let kind = match error.outcome() {
                Outcome::Busy | Outcome::LockUnavailable | Outcome::Cancelled => error.outcome(),
                _ => Outcome::IncompleteInventory,
            };
            result.error(self.root.path().display().to_string(), "root", error, kind);
        }
        result
    }
    fn scan_into(
        &self,
        cancel: &Cancel,
        result: &mut Inventory,
        limits: ScanLimits,
    ) -> Result<(), Error> {
        if cancel.is_requested() {
            return Err(cancelled());
        }
        let Some(lease) = self.root.lock(false, false)? else {
            result.not_initialized = true;
            return if self.root.missing_is_empty() {
                Ok(())
            } else {
                Err(Error::RootMissing)
            };
        };
        root::validate_marker(&lease.dir)?;
        let memberships = match read_memberships(&lease.dir) {
            Ok(value) => value,
            Err(error) => {
                result.error(
                    MEMBERSHIP,
                    "membership",
                    error,
                    Outcome::IncompleteInventory,
                );
                Memberships::default()
            }
        };
        let mut count = 0;
        let mut total = 0;
        let mut seen = BTreeSet::new();
        for entry in lease
            .dir
            .entries()
            .map_err(|e| root::io(IoOperation::Read, e))?
        {
            if cancel.is_requested() {
                return Err(cancelled());
            }
            let entry = entry.map_err(|e| root::io(IoOperation::Inspect, e))?;
            let name = entry.file_name();
            if matches!(
                name.to_str(),
                Some(
                    "library.toml"
                        | ".library.lock"
                        | "membership.toml"
                        | "recipes"
                        | "trash"
                        | ".stage"
                )
            ) {
                continue;
            }
            count += 1;
            if count > limits.entries {
                return Err(budget());
            }
            if root::owned_stage(&name) {
                result.stages.push(name.into());
            } else {
                result.note(
                    name.to_string_lossy(),
                    "unrecognized_artifact",
                    InventoryNotice::UnknownRootEntry,
                );
            }
        }
        let mut duplicates = BTreeSet::new();
        for (directory, trashed) in [("recipes", false), ("trash", true)] {
            let dir = lease.directory(directory)?;
            for entry in dir.entries().map_err(|e| root::io(IoOperation::Read, e))? {
                if cancel.is_requested() {
                    return Err(cancelled());
                }
                let entry = entry.map_err(|e| root::io(IoOperation::Inspect, e))?;
                count += 1;
                if count > limits.entries {
                    return Err(budget());
                }
                let name = entry.file_name();
                let relative = format!("{directory}/{}", name.to_string_lossy());
                if root::owned_stage(&name) {
                    result.stages.push(relative.into());
                    continue;
                }
                let Some(stem) = name.to_str().and_then(|n| n.strip_suffix(".toml")) else {
                    result.note(
                        relative,
                        "unrecognized_artifact",
                        InventoryNotice::UnknownEntry,
                    );
                    continue;
                };
                let id = match stem.parse::<DocumentUuid>() {
                    Ok(id) => id,
                    Err(_) => {
                        result.error(
                            relative,
                            "filename",
                            InventoryNotice::UuidFilenameRequired,
                            Outcome::IncompleteInventory,
                        );
                        continue;
                    }
                };
                if !seen.insert(id) {
                    duplicates.insert(id);
                }
                if self.reserved(id).is_err() {
                    result.error(
                        relative,
                        "reserved_identity",
                        InventoryNotice::ReservedBuiltinIdentity,
                        Outcome::IncompleteInventory,
                    );
                    continue;
                }
                if total >= limits.raw || total >= limits.decoded {
                    return Err(budget());
                }
                let allowance = Recipe::MAX_BYTES
                    .min(limits.raw - total)
                    .min(limits.decoded - total);
                let bytes = match publish::read_bounded_counted(
                    &dir,
                    Path::new(&name),
                    allowance,
                    &mut total,
                ) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        result.error(relative, "recipe_read", error, Outcome::IncompleteInventory);
                        continue;
                    }
                };
                match LoadedRecord::decode(
                    id,
                    &self.root.path().join(&relative),
                    bytes,
                    trashed,
                    memberships.entries.get(&id),
                ) {
                    Ok(record) => result.entries.push(record.summary()),
                    Err(error) => result.error(
                        relative,
                        "recipe_contract",
                        error,
                        Outcome::IncompleteInventory,
                    ),
                }
            }
        }
        for id in duplicates {
            let recipes = lease.directory("recipes")?;
            let trash = lease.directory("trash")?;
            let name = format!("{id}.toml");
            let same = publish::same_file(&recipes, Path::new(&name), &trash, Path::new(&name))
                .unwrap_or(false);
            if same {
                result
                    .entries
                    .retain(|entry| entry.key != LibraryKey::User(id) || entry.trashed);
                result.note(name, "interrupted_move", InventoryNotice::InterruptedMove);
            } else {
                result
                    .entries
                    .retain(|entry| entry.key != LibraryKey::User(id));
                result.error(
                    name,
                    "duplicate_identity",
                    InventoryNotice::DuplicateIdentity,
                    Outcome::IncompleteInventory,
                );
            }
        }
        for id in memberships.entries.keys().filter(|id| !seen.contains(id)) {
            result.note(
                MEMBERSHIP,
                "orphan_membership",
                InventoryNotice::OrphanMembership(*id),
            );
        }
        let stages = lease.directory(".stage")?;
        for entry in stages
            .entries()
            .map_err(|e| root::io(IoOperation::Read, e))?
        {
            if cancel.is_requested() {
                return Err(cancelled());
            }
            count += 1;
            if count > limits.entries {
                return Err(budget());
            }
            let entry = entry.map_err(|e| root::io(IoOperation::Inspect, e))?;
            result
                .stages
                .push(std::path::Path::new(".stage").join(entry.file_name()));
        }
        result.entries.sort_by(|a, b| {
            a.normalized_title
                .cmp(&b.normalized_title)
                .then(a.key.cmp(&b.key))
        });
        result.stages.sort();
        Ok(())
    }
    /// Execute a single locked read/check/publish/verify operation.
    ///
    /// # Errors
    /// Rejects invalid/stale intent before publication, and names the path if a failed I/O result cannot be verified.
    pub(crate) fn apply(&self, mutation: Mutation, cancel: &Cancel) -> Result<WriteOutcome, Error> {
        let (key, kind, expected) = match &mutation {
            Mutation::Create(recipe) => (
                LibraryKey::User(recipe.document().id),
                MutationKind::Create,
                None,
            ),
            Mutation::Add(recipe) => (
                LibraryKey::User(recipe.document().id),
                MutationKind::Add,
                None,
            ),
            Mutation::Update { key, expect, .. } => (key.clone(), MutationKind::Update, *expect),
            Mutation::Rename { key, expect, .. } => (key.clone(), MutationKind::Rename, *expect),
            Mutation::Trash { key, expect } => (key.clone(), MutationKind::Trash, *expect),
            Mutation::Restore { key, expect } => (key.clone(), MutationKind::Restore, *expect),
            Mutation::Purge { key, expect, .. } => (key.clone(), MutationKind::Purge, *expect),
        };
        let id = crate::storage::library::require_user(&key)?;
        self.reserved(id)?;
        if let Mutation::Create(recipe) | Mutation::Add(recipe) | Mutation::Update { recipe, .. } =
            &mutation
            && recipe.document().id != id
        {
            return Err(Error::ProposedIdentity {
                expected: id,
                found: recipe.document().id,
            });
        }
        if let Mutation::Purge {
            confirmed: false, ..
        } = mutation
        {
            return Err(Error::PurgeUnconfirmed);
        }
        if cancel.is_requested() {
            return Err(cancelled());
        }
        let creating = matches!(kind, MutationKind::Create | MutationKind::Add);
        let mut diagnostics = Vec::new();
        let lease = match self.root.lock(true, false)? {
            Some(lease) => lease,
            None if creating && self.root.missing_is_empty() => {
                diagnostics.extend(self.root.initialize()?.warnings);
                self.root.lock(true, false)?.ok_or(Error::RootDisappeared)?
            }
            None => {
                return Err(if creating {
                    Error::MissingMarkerOnCreate
                } else {
                    Error::MissingMarkerOnMutation
                });
            }
        };
        root::validate_marker(&lease.dir)?;
        let mut memberships = read_memberships(&lease.dir)?;
        for directory in ["recipes", "trash", ".stage"] {
            let dir = lease.directory(directory)?;
            root::cleanup_stages(&dir, &lease.canonical.join(directory), &mut diagnostics)?;
        }
        root::cleanup_stages(&lease.dir, &lease.canonical, &mut diagnostics)?;
        reconcile_moves(&lease, &mut diagnostics)?;
        let old = read_record(&lease, id, &memberships)?;
        if let Some(expect) = expected
            && old.as_ref().is_none_or(|old| old.version_token != expect)
        {
            return Err(Error::StaleContent { id });
        }
        if !creating && old.is_none() {
            return Err(Error::EntryDisappeared { id });
        }
        let removed = prune_memberships(&lease, &mut memberships)?;
        if removed > 0 {
            diagnostics.push(
                crate::storage::warning::StorageWarning::RemovedMemberships { count: removed },
            );
            persist_memberships(&lease, &memberships, cancel, &mut diagnostics);
        }
        if let Some(old) = old.as_ref()
            && creating
        {
            if let Mutation::Add(recipe) = &mutation
                && !old.trashed
                && old.snapshot.recipe() == recipe.as_ref()
            {
                return Ok(unchanged(key, kind, old.clone(), diagnostics));
            }
            return Err(Error::IdentityExists { id });
        }
        let now = observed_now()?;
        let name = format!("{id}.toml");
        if matches!(kind, MutationKind::Purge) {
            let old = old.ok_or(Error::EntryDisappeared { id })?;
            if !old.trashed {
                return Err(Error::PurgeRequiresTrash { id });
            }
            let trash = lease.directory("trash")?;
            diagnostics.extend(publish::remove(
                &trash,
                Path::new(&name),
                &lease.canonical.join("trash").join(&name),
            )?);
            memberships.entries.remove(&id);
            persist_memberships(&lease, &memberships, cancel, &mut diagnostics);
            return Ok(outcome(key, kind, None, diagnostics));
        }
        if matches!(kind, MutationKind::Trash | MutationKind::Restore) {
            let mut record = old.ok_or(Error::EntryDisappeared { id })?;
            let trashed = kind == MutationKind::Trash;
            if record.trashed == trashed {
                return Ok(unchanged(key, kind, record, diagnostics));
            }
            let (source, destination) = if trashed {
                ("recipes", "trash")
            } else {
                ("trash", "recipes")
            };
            let from = lease.directory(source)?;
            let to = lease.directory(destination)?;
            diagnostics.extend(publish::relocate(
                &from,
                &to,
                Path::new(&name),
                &lease.canonical.join(source).join(&name),
                &lease.canonical.join(destination).join(&name),
                cancel,
            )?);
            let membership = memberships.entries.entry(id).or_default();
            membership.trashed_at = if trashed { Some(now) } else { None };
            record.trashed = trashed;
            record.trashed_at = membership.trashed_at;
            if !persist_memberships(&lease, &memberships, cancel, &mut diagnostics) {
                refresh_membership(&lease, &mut record, &mut diagnostics);
            }
            return Ok(outcome(key, kind, Some(record), diagnostics));
        }
        let recipe = match mutation {
            Mutation::Create(recipe) | Mutation::Add(recipe) | Mutation::Update { recipe, .. } => {
                (*recipe).clone()
            }
            Mutation::Rename { title, .. } => {
                let recipe = old
                    .as_ref()
                    .ok_or(Error::EntryDisappeared { id })?
                    .snapshot
                    .recipe()
                    .clone();
                let notes = recipe.description().and_then(|d| d.notes.clone());
                recipe
                    .with_description(Some(crate::core::Description { title, notes }))
                    .map_err(|e| Error::Recipe { error: e })?
            }
            _ => unreachable!("lifecycle operations handled above"),
        };
        if recipe.document().id != id {
            return Err(Error::ProposedIdentity {
                expected: id,
                found: recipe.document().id,
            });
        }
        if let Some(old) = &old {
            if old.trashed {
                return Err(Error::MustRestore { id });
            }
            let previous = old.snapshot.recipe();
            if previous.document().created_at != recipe.document().created_at {
                return Err(Error::HistoryReplacement { id });
            }
            if previous.content() == recipe.content() {
                return Ok(unchanged(key, kind, old.clone(), diagnostics));
            }
        }
        let mut proposal = recipe.into_builder();
        if let Some(old) = &old
            && old.snapshot.recipe().sound() != &proposal.content.sound
        {
            for evidence in &mut proposal.content.evidence {
                evidence.scope = crate::core::EvidenceScope::Ancestor;
            }
        }
        if kind == MutationKind::Create {
            proposal.document.created_at = Some(now);
            proposal.document.modified_at = Some(now);
        } else if !creating {
            proposal.document.modified_at = Some(now);
        }
        let recipe = proposal.build().map_err(|e| Error::Recipe { error: e })?;
        diagnostics.extend(chronology(
            &recipe,
            old.as_ref().map(|r| r.snapshot.recipe()),
            now,
        ));
        let bytes = recipe
            .to_toml()
            .map_err(|e| Error::Recipe { error: e })?
            .into_bytes();
        let recipes = lease.directory("recipes")?;
        let publication = publish::publish_in(
            &recipes,
            Path::new(&name),
            &lease.canonical.join("recipes").join(&name),
            if creating {
                Policy::CreateNew
            } else {
                Policy::Replace
            },
            cancel,
            |file| {
                file.write_all(&bytes).map_err(|e| {
                    FileError::Io(IoFailure::new(
                        IoOperation::Write,
                        Some(lease.canonical.join("recipes").join(&name)),
                        e,
                    ))
                })
            },
        )?;
        diagnostics.extend(publication.warnings);
        let membership = memberships.entries.entry(id).or_default();
        if creating {
            membership.added_at = Some(now);
            membership.trashed_at = None;
        }
        let mut record = LoadedRecord::decode(
            id,
            &lease.canonical.join("recipes").join(&name),
            bytes,
            false,
            Some(membership),
        )?;
        if !persist_memberships(&lease, &memberships, cancel, &mut diagnostics) {
            refresh_membership(&lease, &mut record, &mut diagnostics);
        }
        Ok(outcome(key, kind, Some(record), diagnostics))
    }
}
fn read_memberships(dir: &Dir) -> Result<Memberships, Error> {
    if let Err(error) = dir.symlink_metadata(MEMBERSHIP)
        && error.kind() == std::io::ErrorKind::NotFound
    {
        return Ok(Memberships::default());
    }
    let bytes = root::read_bounded(dir, MEMBERSHIP, MAX_MEMBERSHIP_BYTES)?;
    let text = std::str::from_utf8(&bytes).map_err(|e| Error::MembershipUtf8 { error: e })?;
    toml::from_str(text).map_err(|e| Error::MembershipFormat { error: e })
}
fn persist_memberships(
    lease: &root::Lease,
    memberships: &Memberships,
    _cancel: &Cancel,
    warnings: &mut Vec<crate::storage::warning::StorageWarning>,
) -> bool {
    // The primary recipe already committed; cancellation cannot undo it or skip best-effort timestamps.
    let result = (|| {
        let text =
            toml::to_string(memberships).map_err(|e| Error::MembershipEncode { error: e })?;
        if text.len() > MAX_MEMBERSHIP_BYTES {
            return Err(Error::MembershipSize {
                bytes: text.len(),
                limit: MAX_MEMBERSHIP_BYTES,
            });
        }
        publish::publish_in(
            &lease.dir,
            Path::new(MEMBERSHIP),
            &lease.canonical.join(MEMBERSHIP),
            Policy::Replace,
            &Cancel::default(),
            |file| {
                file.write_all(text.as_bytes()).map_err(|e| {
                    FileError::Io(IoFailure::new(
                        IoOperation::Write,
                        Some(lease.canonical.join(MEMBERSHIP)),
                        e,
                    ))
                })
            },
        )
        .map_err(Error::from)
    })();
    match result {
        Ok(publication) => {
            warnings.extend(publication.warnings);
            true
        }
        Err(error) => {
            warnings.push(
                crate::storage::warning::StorageWarning::MembershipUpdateFailed {
                    cause: Box::new(error),
                },
            );
            false
        }
    }
}
fn refresh_membership(
    lease: &root::Lease,
    record: &mut LoadedRecord,
    warnings: &mut Vec<crate::storage::warning::StorageWarning>,
) {
    match read_memberships(&lease.dir) {
        Ok(memberships) => {
            let membership = crate::storage::library::require_user(&record.key)
                .ok()
                .and_then(|id| memberships.entries.get(&id));
            record.added_at = membership.and_then(|m| m.added_at);
            record.trashed_at = if record.trashed {
                membership.and_then(|m| m.trashed_at)
            } else {
                None
            };
        }
        Err(error) => {
            record.added_at = None;
            record.trashed_at = None;
            warnings.push(
                crate::storage::warning::StorageWarning::MembershipReadFailed {
                    cause: Box::new(error),
                },
            );
        }
    }
}
fn read_record(
    lease: &root::Lease,
    id: DocumentUuid,
    memberships: &Memberships,
) -> Result<Option<LoadedRecord>, Error> {
    let name = format!("{id}.toml");
    let recipes = lease.directory("recipes")?;
    let trash = lease.directory("trash")?;
    let active = exists(&recipes, &name)?;
    let trashed = exists(&trash, &name)?;
    let directory = match (active, trashed) {
        (false, false) => return Ok(None),
        (true, false) => (&recipes, false),
        (false, true) => (&trash, true),
        (true, true)
            if publish::same_file(&recipes, Path::new(&name), &trash, Path::new(&name))? =>
        {
            (&trash, true)
        }
        (true, true) => {
            return Err(Error::AmbiguousRecord { id });
        }
    };
    let bytes = root::read_bounded(directory.0, &name, Recipe::MAX_BYTES)?;
    LoadedRecord::decode(
        id,
        &lease
            .canonical
            .join(if directory.1 { "trash" } else { "recipes" })
            .join(&name),
        bytes,
        directory.1,
        memberships.entries.get(&id),
    )
    .map(Some)
}
fn exists(dir: &Dir, name: &str) -> Result<bool, Error> {
    match dir.symlink_metadata(name) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(root::io(IoOperation::Inspect, e)),
    }
}
fn prune_memberships(lease: &root::Lease, memberships: &mut Memberships) -> Result<usize, Error> {
    let recipes = lease.directory("recipes")?;
    let trash = lease.directory("trash")?;
    let mut keep = BTreeSet::new();
    for id in memberships.entries.keys() {
        let name = format!("{id}.toml");
        if exists(&recipes, &name)? || exists(&trash, &name)? {
            keep.insert(*id);
        }
    }
    let before = memberships.entries.len();
    memberships.entries.retain(|id, _| keep.contains(id));
    Ok(before - memberships.entries.len())
}
fn reconcile_moves(
    lease: &root::Lease,
    warnings: &mut Vec<crate::storage::warning::StorageWarning>,
) -> Result<(), Error> {
    let recipes = lease.directory("recipes")?;
    let trash = lease.directory("trash")?;
    for entry in trash
        .entries()
        .map_err(|e| root::io(IoOperation::Read, e))?
    {
        let entry = entry.map_err(|e| root::io(IoOperation::Inspect, e))?;
        let name = entry.file_name();
        if name
            .to_str()
            .and_then(|n| n.strip_suffix(".toml"))
            .is_none_or(|n| n.parse::<DocumentUuid>().is_err())
        {
            continue;
        }
        if recipes.symlink_metadata(&name).is_ok()
            && publish::same_file(&recipes, Path::new(&name), &trash, Path::new(&name))?
        {
            recipes
                .remove_file(&name)
                .map_err(|e| root::io(IoOperation::Remove, e))?;
            warnings.push(crate::storage::warning::StorageWarning::RecoveredMove {
                name: (name).to_owned(),
            });
        }
    }
    Ok(())
}
fn outcome(
    key: LibraryKey,
    kind: MutationKind,
    record: Option<LoadedRecord>,
    diagnostics: Vec<crate::storage::warning::StorageWarning>,
) -> WriteOutcome {
    WriteOutcome {
        status: if diagnostics.is_empty() {
            MutationStatus::Published
        } else {
            MutationStatus::PublishedWithWarning
        },
        key,
        kind,
        record,
        diagnostics,
    }
}
fn unchanged(
    key: LibraryKey,
    kind: MutationKind,
    record: LoadedRecord,
    diagnostics: Vec<crate::storage::warning::StorageWarning>,
) -> WriteOutcome {
    WriteOutcome {
        status: MutationStatus::Unchanged,
        key,
        kind,
        record: Some(record),
        diagnostics,
    }
}
fn cancelled() -> Error {
    Error::Cancelled
}
fn budget() -> Error {
    Error::ScanBudget
}

fn chronology(
    recipe: &Recipe,
    previous: Option<&Recipe>,
    now: Timestamp,
) -> Vec<crate::storage::warning::StorageWarning> {
    let mut warnings = Vec::new();
    if recipe
        .document()
        .created_at
        .zip(recipe.document().modified_at)
        .is_some_and(|(created, modified)| modified < created)
    {
        warnings.push(crate::storage::warning::StorageWarning::ModificationPrecedesCreation);
    }
    if previous
        .and_then(|p| p.document().modified_at.or(p.document().created_at))
        .is_some_and(|prior| now < prior)
    {
        warnings.push(crate::storage::warning::StorageWarning::ClockPredatesHistory);
    }
    if recipe.document().created_at.is_some_and(|date| date > now)
        || recipe.document().modified_at.is_some_and(|date| date > now)
    {
        warnings.push(crate::storage::warning::StorageWarning::FutureHistory);
    }
    warnings
}
/// Inspect declared chronology without changing imported dates or fabricating history.
/// The observed clock is presentation context, never a version token.
#[must_use]
pub fn history_warnings(recipe: &Recipe) -> Vec<crate::storage::warning::StorageWarning> {
    observed_now().map_or_else(
        |error| {
            vec![crate::storage::warning::StorageWarning::UnavailableHistoryClock { cause: error }]
        },
        |now| chronology(recipe, None, now),
    )
}

#[cfg(test)]
mod tests;
