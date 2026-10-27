// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Portable identity and declared history, independent of local collection state.
use alloc::string::{String, ToString};
use chrono::{DateTime, Datelike, SecondsFormat, Utc};
use core::{fmt, str::FromStr};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

/// A canonical lowercase RFC 9562 random-version UUID. It conveys no ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentUuid([u8; 16]);
impl DocumentUuid {
    /// Set the UUIDv4 version/variant in cryptographically random caller bytes.
    /// Randomness acquisition is fallible application work, never synthesis work.
    #[must_use]
    pub const fn from_random_bytes(mut bytes: [u8; 16]) -> Self {
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Self(bytes)
    }
}
impl FromStr for DocumentUuid {
    type Err = &'static str;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let parsed =
            uuid::Uuid::parse_str(text).map_err(|_| "expected a canonical lowercase UUIDv4")?;
        let bytes = *parsed.as_bytes();
        if parsed.hyphenated().to_string() != text || bytes[6] >> 4 != 4 || bytes[8] >> 6 != 2 {
            return Err("expected a canonical lowercase UUIDv4");
        }
        Ok(Self(bytes))
    }
}
impl fmt::Display for DocumentUuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        uuid::Uuid::from_bytes(self.0).hyphenated().fmt(f)
    }
}
impl Serialize for DocumentUuid {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for DocumentUuid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

/// A nanosecond-precise UTC instant in the application's restricted RFC3339 profile.
/// Years 0001–9999, explicit offset, uppercase T/Z, seconds 00–59; no -00:00.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp {
    seconds: i64,
    nanos: u32,
}
impl Timestamp {
    /// Construct a UTC instant, rejecting invalid nanoseconds or out-of-range years.
    #[must_use]
    pub fn from_unix(seconds: i64, nanos: u32) -> Option<Self> {
        let date = DateTime::<Utc>::from_timestamp(seconds, nanos)?;
        ((1..=9999).contains(&date.year()) && nanos < 1_000_000_000)
            .then_some(Self { seconds, nanos })
    }
    /// Whole seconds since the Unix epoch; this is not a concurrency revision.
    #[must_use]
    pub const fn unix_seconds(self) -> i64 {
        self.seconds
    }
    /// Fractional nanoseconds within the second.
    #[must_use]
    pub const fn nanoseconds(self) -> u32 {
        self.nanos
    }
}
impl FromStr for Timestamp {
    type Err = &'static str;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let error = "expected YYYY-MM-DDTHH:MM:SS[.1–9 digits]Z or an explicit numeric offset; no leap seconds or -00:00";
        let b = text.as_bytes();
        if b.len() < 20
            || !b[..4].iter().all(u8::is_ascii_digit)
            || &b[..4] == b"0000"
            || b.get(10) != Some(&b'T')
            || b.get(17..19)
                .and_then(|s| core::str::from_utf8(s).ok())
                .and_then(|s| s.parse::<u8>().ok())
                .is_none_or(|s| s > 59)
            || text.ends_with("-00:00")
        {
            return Err(error);
        }
        let zone = if text.ends_with('Z') {
            text.len() - 1
        } else {
            let start = text.len().checked_sub(6).ok_or(error)?;
            if !matches!(b[start], b'+' | b'-') || b.get(start + 3) != Some(&b':') {
                return Err(error);
            }
            start
        };
        if zone != 19
            && (b.get(19) != Some(&b'.')
                || !(1..=9).contains(&(zone.saturating_sub(20)))
                || !b[20..zone].iter().all(u8::is_ascii_digit))
        {
            return Err(error);
        }
        let date = DateTime::parse_from_rfc3339(text)
            .map_err(|_| error)?
            .with_timezone(&Utc);
        Self::from_unix(date.timestamp(), date.timestamp_subsec_nanos()).ok_or(error)
    }
}
impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match DateTime::<Utc>::from_timestamp(self.seconds, self.nanos) {
            Some(date) => f.write_str(&date.to_rfc3339_opts(SecondsFormat::Nanos, true)),
            None => Err(fmt::Error),
        }
    }
}
impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}
/// Declared recipe history. Missing timestamps remain unknown on import/edit.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentIdentity {
    /// Identity retained on export and ordinary edits; copies require a new UUID.
    pub id: DocumentUuid,
    /// Declared creation of this recipe identity, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<Timestamp>,
    /// Declared meaningful edit, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<Timestamp>,
}
