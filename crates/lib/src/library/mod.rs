//! Local persistent User Library: bounded records, cooperative locking and receipts.
//!
//! Portable recipe ownership, declared history and nominal classification remain
//! separate from local membership. No library operation belongs on an audio callback.
#[cfg(any(test, feature = "test-support"))]
mod faults;
#[cfg(any(test, feature = "test-support"))]
pub use faults::StoreFault;
mod record;
mod root;
mod store;
use crate::{DocumentUuid, Error, ErrorKind};
pub use record::{Fingerprint, MAX_RECORD_BYTES, RECORD_FORMAT, VersionToken};
pub use root::{Initialization, LibraryRoot, RootInfo, RootSource};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use std::{
    fmt,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};
pub use store::{
    Inventory, LibraryDiagnostic, LibraryStore, LoadedRecord, MAX_SCAN_DECODED_BYTES,
    MAX_SCAN_RAW_BYTES, MAX_SCAN_RECORDS, Mutation, MutationKind, MutationStatus, OperationReceipt,
    RecheckOutcome, WriteOutcome, history_warnings,
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
                "ReadOnlyBuiltIn: shipped recipes cannot be renamed, replaced, trashed or purged; edit a copy",
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
/// Opaque random per-operation record revision, separate from document identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordRevision(DocumentUuid);
impl RecordRevision {
    pub(super) fn fresh() -> Result<Self, Error> {
        crate::identity::random_uuid().map(Self)
    }
}
impl fmt::Display for RecordRevision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for RecordRevision {
    type Err = Error;
    fn from_str(text: &str) -> Result<Self, Error> {
        text.parse()
            .map(Self)
            .map_err(|e| Error::new(ErrorKind::InvalidArgument, e))
    }
}
impl Serialize for RecordRevision {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for RecordRevision {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(D::Error::custom)
    }
}
