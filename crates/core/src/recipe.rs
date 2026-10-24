// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::Stimulus;
use crate::stimulus::DocumentComponent;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Display, Formatter};
use serde::{Deserialize, Serialize};

/// A recipe: a stimulus and, optionally, what it is about.
///
/// A recipe is strict TOML of at most [`Recipe::MAX_BYTES`]. Unknown,
/// missing required, duplicated and mistyped fields are errors. Values are checked
/// when the stimulus is given a sample rate by [`Stimulus::signal`]; a parsed
/// recipe can still hold values no rate accepts, including `nan` and `inf`.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    /// The sound.
    pub stimulus: Stimulus,
    /// What the sound is and where it comes from, when the recipe says.
    pub about: Option<About>,
    /// The retained original source, with no ancestry or runtime identity.
    pub source: Option<Source>,
}

/// What a recipe's sound is, for listeners and for researchers. Parsing
/// checks the field structure and string types; sources are not fetched or
/// certified. This descriptive text does not affect the samples.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct About {
    /// A short name for the sound.
    pub title: String,
    /// Plain notes about this sound, independent of research claims.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Research claims about this exact stimulus.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub research: Option<Research>,
}

/// Research context about one stimulus; never inherited by changed audio.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Research {
    /// The study's subject.
    pub studied_for: Option<String>,
    /// Plain-language findings and limitations.
    pub summary: Option<String>,
    /// Publication citation.
    pub citation: Option<String>,
    /// Publication URL.
    pub source_url: Option<String>,
    /// Condition-specific methods and role.
    pub study: Option<String>,
    /// Condition-specific adaptations.
    pub adaptation: Option<String>,
}

/// One nonrecursive persisted source content anchor.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// Original sound parameters.
    pub stimulus: Stimulus,
    /// Original description and research context.
    pub about: Option<About>,
}

/// Why text is not a recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeError {
    /// The recipe is longer than [`Recipe::MAX_BYTES`].
    TooLarge,
    /// The recipe is not UTF-8 text.
    NotUtf8,
    /// The recipe is UTF-16 text, as some editors and shells save it.
    Utf16,
    /// The text is not TOML or does not match the recipe format.
    Format(toml::de::Error),
    /// The recipe states a schema version other than [`Recipe::SCHEMA_VERSION`].
    UnsupportedVersion {
        /// The version the recipe states.
        found: u32,
    },
}

impl Display for RecipeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(f, "recipe exceeds {} bytes", Recipe::MAX_BYTES),
            Self::NotUtf8 => write!(f, "recipe is not UTF-8 text"),
            Self::Utf16 => write!(f, "recipe is UTF-16 text; save it as UTF-8"),
            Self::Format(error) => write!(f, "recipe does not match the format: {error}"),
            Self::UnsupportedVersion { found } => write!(
                f,
                "recipe has schema_version {found}; this build reads {}",
                Recipe::SCHEMA_VERSION
            ),
        }
    }
}

impl core::error::Error for RecipeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Format(error) => Some(error),
            Self::TooLarge | Self::NotUtf8 | Self::Utf16 | Self::UnsupportedVersion { .. } => None,
        }
    }
}

/// Only the version, read first so that a recipe written for another
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
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<SourceDocument>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourceDocument {
    duration_seconds: f64,
    fade_seconds: f64,
    about: Option<About>,
    #[serde(default)]
    component: Vec<DocumentComponent>,
}

impl Recipe {
    /// The one recipe schema this build reads.
    pub const SCHEMA_VERSION: u32 = 3;

    /// The longest recipe, in bytes. A reader can stop after one byte more.
    pub const MAX_BYTES: usize = 65_536;

    /// Read a recipe from bytes.
    ///
    /// # Errors
    /// Returns [`RecipeError::TooLarge`], [`RecipeError::Utf16`],
    /// [`RecipeError::NotUtf8`], or what
    /// [`Recipe::parse`] returns.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RecipeError> {
        if bytes.len() > Self::MAX_BYTES {
            return Err(RecipeError::TooLarge);
        }
        let text = core::str::from_utf8(bytes).map_err(|_| {
            // A byte-order mark identifies UTF-16; other invalid input is just not UTF-8.
            if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
                RecipeError::Utf16
            } else {
                RecipeError::NotUtf8
            }
        })?;
        Self::parse(text)
    }

    /// Read a recipe from text.
    ///
    /// # Errors
    /// Returns [`RecipeError::TooLarge`], [`RecipeError::Format`] or
    /// [`RecipeError::UnsupportedVersion`].
    pub fn parse(text: &str) -> Result<Self, RecipeError> {
        if text.len() > Self::MAX_BYTES {
            return Err(RecipeError::TooLarge);
        }
        let version: Version = toml::from_str(text).map_err(RecipeError::Format)?;
        if let Some(found) = version.schema_version
            && found != Self::SCHEMA_VERSION
        {
            return Err(RecipeError::UnsupportedVersion { found });
        }
        let document: Document = toml::from_str(text).map_err(RecipeError::Format)?;
        Ok(Self {
            stimulus: Stimulus {
                duration_seconds: document.duration_seconds,
                fade_seconds: document.fade_seconds,
                components: document.component.into_iter().map(Into::into).collect(),
            },
            about: document.about,
            source: document.source.map(|source| Source {
                stimulus: Stimulus {
                    duration_seconds: source.duration_seconds,
                    fade_seconds: source.fade_seconds,
                    components: source.component.into_iter().map(Into::into).collect(),
                },
                about: source.about,
            }),
        })
    }

    /// Serialize the recipe in the schema-3 TOML recipe layout.
    ///
    /// # Errors
    /// Returns the TOML writer's error; finite values always serialize.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(&Document {
            schema_version: Self::SCHEMA_VERSION,
            duration_seconds: self.stimulus.duration_seconds,
            fade_seconds: self.stimulus.fade_seconds,
            about: self.about.clone(),
            source: self.source.as_ref().map(|source| SourceDocument {
                duration_seconds: source.stimulus.duration_seconds,
                fade_seconds: source.stimulus.fade_seconds,
                about: source.about.clone(),
                component: source
                    .stimulus
                    .components
                    .iter()
                    .map(|c| (*c).into())
                    .collect(),
            }),
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
