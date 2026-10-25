//! Local persistent User Library: plain recipe files and cooperative locking.
//!
//! Portable recipe ownership, declared history and nominal classification remain
//! separate from local membership. No library operation belongs on an audio callback.
mod record;
mod root;
pub(crate) mod store;
use crate::core::DocumentUuid;
use crate::error::{Error, ErrorKind};
pub use record::{Fingerprint, VersionToken};
pub use root::{Initialization, LIBRARY_LAYOUT, LibraryRoot, RootInfo, RootSource};
use serde::{Serialize, Serializer};
use std::{
    fmt,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};
pub use store::{
    Inventory, LibraryDiagnostic, LibraryStore, LoadedRecord, MAX_SCAN_DECODED_BYTES,
    MAX_SCAN_RAW_BYTES, MAX_SCAN_RECORDS, Mutation, MutationKind, MutationStatus, WriteOutcome,
    history_warnings,
};

/// Identity of a temporary entry in one application process, never a CLI selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExternalId(u64);
static NEXT_EXTERNAL: AtomicU64 = AtomicU64::new(1);
impl ExternalId {
    /// Allocate without reuse; fails explicitly on exhaustion.
    ///
    /// # Errors
    /// Returns an identity-allocation error at the process counter limit.
    pub fn allocate() -> Result<Self, Error> {
        NEXT_EXTERNAL
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map(Self)
            .map_err(|_| Error::new(ErrorKind::Identity, "temporary entry identities exhausted"))
    }
}
/// Stable collection capability; titles and portable declarations cannot grant it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LibraryKey {
    /// Immutable curated catalog identity.
    BuiltIn(String),
    /// Managed record addressed by its portable UUID.
    User(DocumentUuid),
    /// Temporary opened content, independent of persisted membership.
    External(ExternalId),
}
impl LibraryKey {
    /// Require managed User mutation capability before any path or input-file work.
    ///
    /// # Errors
    /// Refuses Built-in and temporary keys without creating or changing files.
    pub fn require_user(&self) -> Result<DocumentUuid, Error> {
        match self {
            Self::User(id) => Ok(*id),
            Self::BuiltIn(_) => Err(Error::new(
                ErrorKind::ReadOnlyBuiltIn,
                "shipped recipes cannot be renamed, replaced, trashed or purged; edit a copy",
            )),
            Self::External(_) => Err(Error::new(
                ErrorKind::InvalidArgument,
                "temporary files must be explicitly added or copied before managed operations",
            )),
        }
    }
}
impl fmt::Display for LibraryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltIn(id) => write!(f, "builtin:{id}"),
            Self::User(id) => write!(f, "user:{id}"),
            Self::External(id) => write!(f, "external:{}", id.0),
        }
    }
}
impl FromStr for LibraryKey {
    type Err = Error;
    fn from_str(text: &str) -> Result<Self, Error> {
        if let Some(id) = text.strip_prefix("builtin:")
            && !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Ok(Self::BuiltIn(id.into()));
        }
        if let Some(id) = text.strip_prefix("user:") {
            return id
                .parse()
                .map(Self::User)
                .map_err(|e| Error::new(ErrorKind::InvalidArgument, e));
        }
        Err(Error::new(
            ErrorKind::InvalidArgument,
            "expected builtin:<catalog-id> or user:<canonical-uuid>; use --file for temporary input",
        ))
    }
}
impl Serialize for LibraryKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
