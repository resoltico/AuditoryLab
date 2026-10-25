//! Exact-byte content tokens for managed recipe files.
use crate::error::{Error, ErrorKind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fmt, str::FromStr};

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
/// SHA-256 of the exact portable recipe bytes observed on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VersionToken(Fingerprint);
impl VersionToken {
    pub(super) fn observed(bytes: &[u8]) -> Self {
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
    type Err = Error;
    fn from_str(text: &str) -> Result<Self, Error> {
        text.parse().map(Self)
    }
}
impl Serialize for VersionToken {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
