// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::Stimulus;
use crate::stimulus::DocumentComponent;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};
use serde::{Deserialize, Serialize};

/// A preset: a stimulus and, optionally, what it is about.
///
/// A preset is strict TOML of at most [`Preset::MAX_BYTES`]. Unknown,
/// missing required, duplicated and mistyped fields are errors. Values are checked
/// when the stimulus is given a sample rate by [`Stimulus::signal`].
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    /// The sound.
    pub stimulus: Stimulus,
    /// What the sound is and where it comes from, when the preset says.
    pub about: Option<About>,
}

/// What a preset's sound is, for listeners and for researchers. Parsing
/// checks the field structure and string types; sources are not fetched or
/// certified. This descriptive text does not affect the samples.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct About {
    /// A short name for the sound.
    pub title: String,
    /// What the source studied the sound for, in a few plain words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub studied_for: Option<String>,
    /// For listeners: what the sound is, what was studied and what was found,
    /// in plain language.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// The source: authors, year, title and publication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation: Option<String>,
    /// A link to the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// For researchers: the stimulus's role in the source and what the source
    /// reports about it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub study: Option<String>,
    /// How the preset differs from, or adds to, the source's description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adaptation: Option<String>,
}

/// Why text is not a preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetError {
    /// The preset is longer than [`Preset::MAX_BYTES`].
    TooLarge,
    /// The preset is not UTF-8 text.
    NotUtf8,
    /// The preset is UTF-16 text, as some editors and shells save it.
    Utf16,
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
            Self::Utf16 => write!(f, "preset is UTF-16 text; save it as UTF-8"),
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
            Self::TooLarge | Self::NotUtf8 | Self::Utf16 | Self::UnsupportedVersion { .. } => None,
        }
    }
}

/// Only the version, read first so that a preset written for another
/// version is reported as such rather than by its unfamiliar fields.
#[derive(Deserialize)]
struct Version {
    schema_version: Option<u32>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u32,
    duration_seconds: f64,
    fade_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<About>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    component: Vec<DocumentComponent>,
}

impl Preset {
    /// The one preset schema this build reads.
    pub const SCHEMA_VERSION: u32 = 2;

    /// The longest preset, in bytes. A reader can stop after one byte more.
    pub const MAX_BYTES: usize = 65_536;

    /// Read a preset from bytes.
    ///
    /// # Errors
    /// Returns [`PresetError::TooLarge`], [`PresetError::Utf16`],
    /// [`PresetError::NotUtf8`], or what
    /// [`Preset::parse`] returns.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PresetError> {
        if bytes.len() > Self::MAX_BYTES {
            return Err(PresetError::TooLarge);
        }
        let text = core::str::from_utf8(bytes).map_err(|_| {
            // A byte-order mark identifies UTF-16; other invalid input is just not UTF-8.
            if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
                PresetError::Utf16
            } else {
                PresetError::NotUtf8
            }
        })?;
        Self::parse(text)
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
                components: document.component.into_iter().map(Into::into).collect(),
            },
            about: document.about,
        })
    }

    /// Serialize the preset in the schema-2 TOML recipe layout.
    ///
    /// # Errors
    /// Returns the TOML writer's error; finite values always serialize.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(&Document {
            schema_version: Self::SCHEMA_VERSION,
            duration_seconds: self.stimulus.duration_seconds,
            fade_seconds: self.stimulus.fade_seconds,
            about: self.about.clone(),
            component: self
                .stimulus
                .components
                .iter()
                .map(|c| (*c).into())
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests;
