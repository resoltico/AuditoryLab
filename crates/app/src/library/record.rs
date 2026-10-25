//! Strict managed-record codec and optimistic exact-byte version tokens.
use super::RecordRevision;
use crate::core::{DocumentUuid, Recipe, Timestamp};
use crate::error::{Error, ErrorKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt, str::FromStr};

/// Maximum managed envelope bytes, including JSON escaping and local metadata.
pub const MAX_RECORD_BYTES: usize = 524_288;
/// Supported managed record/marker version; independent of recipe/output versions.
pub const RECORD_FORMAT: u32 = 1;

/// SHA-256 of exact observed bytes; a conflict token, never scientific trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint([u8; 32]);
impl Fingerprint {
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
    type Err = Error;
    fn from_str(text: &str) -> Result<Self, Error> {
        if text.len() != 64
            || !text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "expected a canonical 64-digit lowercase SHA-256 fingerprint",
            ));
        }
        let mut bytes = [0; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
                .map_err(|e| Error::caused(ErrorKind::InvalidArgument, "invalid fingerprint", e))?;
        }
        Ok(Self(bytes))
    }
}
impl Serialize for Fingerprint {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
/// A random revision plus exact-byte hash; wall-clock dates never authorize writes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VersionToken {
    revision: RecordRevision,
    fingerprint: Fingerprint,
}
impl VersionToken {
    pub(super) fn observed(revision: RecordRevision, bytes: &[u8]) -> Self {
        Self {
            revision,
            fingerprint: Fingerprint::of(bytes),
        }
    }
    /// Opaque operation revision stored in the observed envelope.
    #[must_use]
    pub const fn revision(&self) -> RecordRevision {
        self.revision
    }
    /// Fingerprint of the exact observed outer record bytes.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
}
impl fmt::Display for VersionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.revision, self.fingerprint)
    }
}
impl FromStr for VersionToken {
    type Err = Error;
    fn from_str(text: &str) -> Result<Self, Error> {
        let (revision, fingerprint) = text.split_once(':').ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidArgument,
                "expected a version token returned by describe/list JSON",
            )
        })?;
        Ok(Self {
            revision: revision.parse()?,
            fingerprint: fingerprint.parse()?,
        })
    }
}
impl Serialize for VersionToken {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
// A real newtype (not serde-transparent) makes a missing field an error while
// permitting an explicit JSON null. Missing Option<T> fields otherwise default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Nullable<T>(pub(super) Option<T>);
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Envelope {
    pub(super) record_format: u32,
    pub(super) revision: RecordRevision,
    pub(super) added_at: Timestamp,
    pub(super) trashed_at: Nullable<Timestamp>,
    pub(super) recipe_toml: String,
}
#[derive(Clone)]
pub(super) struct Decoded {
    pub(super) envelope: Envelope,
    pub(super) recipe: Recipe,
    pub(super) token: VersionToken,
}
pub(super) fn decode(id: DocumentUuid, bytes: &[u8]) -> Result<Decoded, Error> {
    let envelope = decode_envelope(id, bytes)?;
    decode_recipe(id, envelope, bytes)
}
pub(super) fn decode_envelope(id: DocumentUuid, bytes: &[u8]) -> Result<Envelope, Error> {
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(Error::new(
            ErrorKind::IncompleteInventory,
            "managed record exceeds its byte limit",
        ));
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| {
        Error::new(
            ErrorKind::IncompleteInventory,
            format!("user:{id}: malformed managed JSON shape, fields or dates; record left intact"),
        )
    })?;
    if envelope.record_format != RECORD_FORMAT || envelope.recipe_toml.len() > Recipe::MAX_BYTES {
        return Err(Error::new(
            ErrorKind::IncompleteInventory,
            format!(
                "user:{id}: unsupported record format or embedded recipe byte limit; record left intact"
            ),
        ));
    }
    Ok(envelope)
}
pub(super) fn decode_recipe(
    id: DocumentUuid,
    envelope: Envelope,
    bytes: &[u8],
) -> Result<Decoded, Error> {
    let invalid = |message: &str| {
        Error::new(
            ErrorKind::IncompleteInventory,
            format!("user:{id}: {message}; record left intact"),
        )
    };
    // Inventory diagnostics deliberately do not expose raw TOML snippets.
    let recipe = Recipe::parse(&envelope.recipe_toml).map_err(|_| {
        invalid("embedded recipe violates the supported schema or structural contract")
    })?;
    if recipe.document.id != id {
        return Err(invalid("filename UUID does not match document.id"));
    }
    let token = VersionToken::observed(envelope.revision, bytes);
    Ok(Decoded {
        envelope,
        recipe,
        token,
    })
}
pub(super) fn encode(envelope: &Envelope) -> Result<Vec<u8>, Error> {
    let bytes = serde_json::to_vec(envelope)
        .map_err(|e| Error::caused(ErrorKind::Write, "cannot encode the managed candidate", e))?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(Error::new(
            ErrorKind::InvalidRecipe,
            "managed candidate exceeds its outer byte limit",
        ));
    }
    Ok(bytes)
}
pub(super) fn filename(id: DocumentUuid) -> String {
    format!("{id}.json")
}
