//! Source identities and exact-byte observations, independent of storage operations.
use crate::core::{DocumentUuid, catalog::CatalogId};
use serde::{Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use {crate::identity::IdentityError, crate::identity::IdentityScope};
/// An origin identifier does not follow its canonical input grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginError {
    /// Noncanonical reusable selector.
    InvalidSelector {
        /// Supplied selector.
        value: String,
    },
    /// Noncanonical lowercase SHA-256.
    InvalidFingerprint {
        /// Supplied token.
        value: String,
    },
}
impl OriginError {
    /// Frontend-independent failure category.
    pub const fn outcome(&self) -> crate::outcome::Outcome {
        match self {
            Self::InvalidSelector { .. } | Self::InvalidFingerprint { .. } => {
                crate::outcome::Outcome::InvalidArgument
            }
        }
    }
}
impl std::fmt::Display for OriginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for OriginError {}
/// Identity of a temporary entry in one application process, never a CLI selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExternalId(u64);
static NEXT_EXTERNAL: AtomicU64 = AtomicU64::new(1);
impl ExternalId {
    /// Allocate without reuse; fails explicitly on exhaustion.
    ///
    /// # Errors
    /// Returns an identity-allocation error at the process counter limit.
    pub fn allocate() -> Result<Self, IdentityError> {
        NEXT_EXTERNAL
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map(Self)
            .map_err(|_| IdentityError::Exhausted(IdentityScope::ExternalEntry))
    }
}
/// Stable collection capability; titles and portable declarations cannot grant it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LibraryKey {
    /// Immutable curated catalog identity.
    BuiltIn(CatalogId),
    /// Managed record addressed by its portable UUID.
    User(DocumentUuid),
    /// Temporary opened content, independent of persisted membership.
    External(ExternalId),
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

impl Serialize for LibraryKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
impl FromStr for LibraryKey {
    type Err = OriginError;
    fn from_str(text: &str) -> Result<Self, OriginError> {
        if let Some(id) = text.strip_prefix("builtin:").and_then(CatalogId::new) {
            return Ok(Self::BuiltIn(id));
        }
        if let Some(id) = text.strip_prefix("user:") {
            return id
                .parse()
                .map(Self::User)
                .map_err(|_| OriginError::InvalidSelector { value: text.into() });
        }
        Err(OriginError::InvalidSelector { value: text.into() })
    }
}
/// SHA-256 of exact observed bytes; a conflict token, never scientific trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 32]);
impl Fingerprint {
    pub(crate) const fn from_digest(value: [u8; 32]) -> Self {
        Self(value)
    }
    /// Hash already-bounded bytes without filesystem or audio work.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
}
impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
impl FromStr for Fingerprint {
    type Err = OriginError;
    fn from_str(text: &str) -> Result<Self, OriginError> {
        if text.len() != 64
            || !text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(OriginError::InvalidFingerprint { value: text.into() });
        }
        let mut bytes = [0; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
                .map_err(|_| OriginError::InvalidFingerprint { value: text.into() })?;
        }
        Ok(Self(bytes))
    }
}
impl Serialize for Fingerprint {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
/// SHA-256 of the exact portable recipe bytes observed on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VersionToken(Fingerprint);
impl VersionToken {
    pub(crate) fn observed(bytes: &[u8]) -> Self {
        Self(Fingerprint::of(bytes))
    }
    /// Exact-byte fingerprint; timestamps never authorize mutations.
    #[must_use]
    pub const fn fingerprint(self) -> Fingerprint {
        self.0
    }
}
impl fmt::Display for VersionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for VersionToken {
    type Err = OriginError;
    fn from_str(text: &str) -> Result<Self, OriginError> {
        text.parse().map(Self)
    }
}
impl Serialize for VersionToken {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
/// Exact opened-file observation retained by an editable external snapshot.
#[derive(Debug, Clone)]
pub struct ObservedFile {
    pub(crate) path: PathBuf,
    pub(crate) fingerprint: Fingerprint,
    pub(crate) handle: Arc<same_file::Handle>,
}
impl ObservedFile {
    /// Canonical file path displayed before Save.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Hash of exact portable bytes observed while opening.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.path == other.path
            && self.fingerprint == other.fingerprint
            && self.handle == other.handle
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    OriginError {
        codes {
            Self::InvalidSelector { .. } => "origin.selector",
            Self::InvalidFingerprint { .. } => "origin.fingerprint",
        }
        delegates {}
        params(this) {
            match this {
                Self::InvalidSelector { value } | Self::InvalidFingerprint { value } => {
                    crate::core::diagnostic_params! {"value"=>value.clone()}
                }
            }
        }
    }
}
