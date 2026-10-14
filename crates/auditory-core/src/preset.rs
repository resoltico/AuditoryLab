// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::{Component, Stimulus};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};
use serde::Deserialize;

/// A preset: a stimulus and, optionally, where it comes from.
///
/// A preset is strict TOML of at most [`Preset::MAX_BYTES`]. Unknown,
/// missing, duplicated and mistyped fields are errors. Values are checked
/// when the stimulus is given a sample rate by [`Stimulus::session`].
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    /// The sound.
    pub stimulus: Stimulus,
    /// Its source, when the preset states one.
    pub provenance: Option<Provenance>,
}

/// Where a preset's stimulus comes from. Parsing checks the field structure
/// and string types; source content is not fetched or certified. This
/// descriptive metadata does not affect the samples.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// A short name for the stimulus.
    pub title: String,
    /// The source: authors, year, title and publication.
    pub citation: String,
    /// A link to the source.
    pub source_url: String,
    /// The stimulus's role in the source and what the source reports about it.
    pub description: String,
    /// How the preset differs from, or adds to, the source's description.
    pub adaptation: String,
}

/// Why text is not a preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetError {
    /// The preset is longer than [`Preset::MAX_BYTES`].
    TooLarge,
    /// The preset is not UTF-8 text.
    NotUtf8,
    /// The text is not TOML or does not match the preset format.
    Format(toml::de::Error),
    /// The preset states a schema version other than [`Preset::SCHEMA_VERSION`].
    UnsupportedVersion {
        /// The version the preset states.
        found: u32,
    },
}

impl Display for PresetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(f, "preset exceeds {} bytes", Preset::MAX_BYTES),
            Self::NotUtf8 => write!(f, "preset is not UTF-8 text"),
            Self::Format(error) => write!(f, "preset does not match the format: {error}"),
            Self::UnsupportedVersion { found } => write!(
                f,
                "preset has schema_version {found}; this build reads {}",
                Preset::SCHEMA_VERSION
            ),
        }
    }
}

impl core::error::Error for PresetError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Format(error) => Some(error),
            Self::TooLarge | Self::NotUtf8 | Self::UnsupportedVersion { .. } => None,
        }
    }
}

/// Only the version, read first so that a preset written for another
/// version is reported as such rather than by its unfamiliar fields.
#[derive(Deserialize)]
struct Version {
    schema_version: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    #[expect(
        dead_code,
        reason = "checked through `Version` before the document is read"
    )]
    schema_version: u32,
    duration_seconds: f64,
    fade_seconds: f64,
    provenance: Option<Provenance>,
    #[serde(default)]
    component: Vec<Component>,
}

impl Preset {
    /// The one preset schema this build reads.
    pub const SCHEMA_VERSION: u32 = 1;

    /// The longest preset, in bytes. A reader can stop after one byte more.
    pub const MAX_BYTES: usize = 65_536;

    /// Read a preset from bytes.
    ///
    /// # Errors
    /// Returns [`PresetError::TooLarge`], [`PresetError::NotUtf8`], or what
    /// [`Preset::parse`] returns.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PresetError> {
        if bytes.len() > Self::MAX_BYTES {
            return Err(PresetError::TooLarge);
        }
        Self::parse(core::str::from_utf8(bytes).map_err(|_| PresetError::NotUtf8)?)
    }

    /// Read a preset from text.
    ///
    /// # Errors
    /// Returns [`PresetError::TooLarge`], [`PresetError::Format`] or
    /// [`PresetError::UnsupportedVersion`].
    pub fn parse(text: &str) -> Result<Self, PresetError> {
        if text.len() > Self::MAX_BYTES {
            return Err(PresetError::TooLarge);
        }
        let version: Version = toml::from_str(text).map_err(PresetError::Format)?;
        if let Some(found) = version.schema_version
            && found != Self::SCHEMA_VERSION
        {
            return Err(PresetError::UnsupportedVersion { found });
        }
        let document: Document = toml::from_str(text).map_err(PresetError::Format)?;
        Ok(Self {
            stimulus: Stimulus {
                duration_seconds: document.duration_seconds,
                fade_seconds: document.fade_seconds,
                components: document.component,
            },
            provenance: document.provenance,
        })
    }
}

#[cfg(test)]
mod tests;
