// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::{
    BibliographySource, Component, ContractError, Description, DocumentIdentity, Evidence,
    EvidenceScope, Finite, Lineage, Sound, StructureError,
};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use serde::{Deserialize, Serialize};

/// One portable authored document, independent of local ownership and membership.
/// Schema 4 uses a strict bounded document shape; finite acoustic errors remain
/// editable and serializable.
///
/// Recipe content is immutable outside core; edits must pass a checked builder.
/// ```compile_fail
/// use auditorylab_core::{Recipe, catalog};
/// let recipe = Recipe::parse(&catalog::ENTRIES[0].text()).unwrap();
/// let _ = recipe.content;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    document: DocumentIdentity,
    content: RecipeContent,
}
/// Authored content independent of document identity and declared dates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeContent {
    /// Ordered authored sound.
    pub sound: Sound,
    /// Current title and notes.
    pub description: Option<Description>,
    /// Bounded bibliography.
    pub sources: Vec<BibliographySource>,
    /// Scoped evidence declarations.
    pub evidence: Vec<Evidence>,
    /// Flat ancestor descriptors.
    pub lineage: Vec<Lineage>,
}

/// Why a byte sequence does not satisfy the one supported portable contract.
#[derive(Debug, Clone, PartialEq)]
pub enum RecipeError {
    /// Portable byte bound exceeded.
    TooLarge,
    /// Input is not UTF-8.
    NotUtf8,
    /// UTF-16 byte order mark detected.
    Utf16,
    /// Strict TOML shape/type error.
    Format(toml::de::Error),
    /// Metadata, reference or attribution-budget contract failed.
    Contract(ContractError),
    /// Authored identities or capacity violate the sound structure.
    Structure(StructureError),
    /// TOML serialization failed.
    Write(toml::ser::Error),
    /// A different schema is not read or converted.
    UnsupportedVersion {
        /// The declared unsupported version.
        found: u32,
    },
}
impl core::fmt::Display for RecipeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}
impl core::error::Error for RecipeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Format(e) => Some(e),
            Self::Contract(e) => Some(e),
            Self::Write(e) => Some(e),
            Self::Structure(e) => Some(e),
            _ => None,
        }
    }
}
#[derive(Deserialize)]
struct Version {
    schema_version: Option<u32>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecipeDocument {
    schema_version: u32,
    document: DocumentIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<Description>,
    sound: DocumentSound,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    source: Vec<BibliographySource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    evidence: Vec<Evidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lineage: Vec<Lineage>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DocumentSound {
    duration_seconds: Finite,
    fade_seconds: Finite,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    component: Vec<Component>,
}
impl Recipe {
    /// Start an editable, unchecked candidate from a silent document.
    #[must_use]
    pub fn builder(document: DocumentIdentity) -> RecipeBuilder {
        Self::silent(document).into_builder()
    }
    /// Move validated content into an editable candidate.
    #[must_use]
    pub fn into_builder(self) -> RecipeBuilder {
        RecipeBuilder {
            document: self.document,
            content: self.content,
        }
    }
    /// The document's document.
    #[must_use]
    pub const fn document(&self) -> &DocumentIdentity {
        &self.document
    }

    /// The document's sound.
    #[must_use]
    pub const fn sound(&self) -> &Sound {
        &self.content.sound
    }

    /// The document's description.
    #[must_use]
    pub const fn description(&self) -> Option<&Description> {
        self.content.description.as_ref()
    }

    /// The document's sources.
    #[must_use]
    pub fn sources(&self) -> &[BibliographySource] {
        &self.content.sources
    }

    /// The document's evidence.
    #[must_use]
    pub fn evidence(&self) -> &[Evidence] {
        &self.content.evidence
    }

    /// The document's lineage.
    #[must_use]
    pub fn lineage(&self) -> &[Lineage] {
        &self.content.lineage
    }

    /// Version of the portable UTF-8 TOML recipe format.
    pub const SCHEMA_VERSION: u32 = 4;
    /// Maximum portable UTF-8 bytes, including metadata.
    pub const MAX_BYTES: usize = 65_536;
    /// An empty authored document with the supplied provisional identity/history.
    #[must_use]
    pub fn silent(document: DocumentIdentity) -> Self {
        Self {
            document,
            content: RecipeContent {
                sound: Sound::new(
                    Finite::new(60.0).unwrap_or_else(|| unreachable!("finite default duration")),
                    Finite::new(0.1).unwrap_or_else(|| unreachable!("finite default fade")),
                    Vec::new(),
                )
                .unwrap_or_else(|_| unreachable!("empty sound")),
                description: None,
                sources: Vec::new(),
                evidence: Vec::new(),
                lineage: Vec::new(),
            },
        }
    }
    /// Read bounded portable UTF-8 bytes; no migration or identity invention.
    ///
    /// # Errors
    /// Returns a precise size/encoding/shape/contract/version diagnostic.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RecipeError> {
        if bytes.len() > Self::MAX_BYTES {
            return Err(RecipeError::TooLarge);
        }
        let text = core::str::from_utf8(bytes).map_err(|_| {
            if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
                RecipeError::Utf16
            } else {
                RecipeError::NotUtf8
            }
        })?;
        Self::parse(text)
    }
    /// Read the one strict schema; finite acoustic invalidity is checked at a rate.
    ///
    /// # Errors
    /// Rejects missing identity, unknown/duplicate fields, malformed history,
    /// nonfinite values, duplicate owners, budgets and dangling references.
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
        let d: RecipeDocument = toml::from_str(text).map_err(RecipeError::Format)?;
        let mut value = Self {
            document: d.document,
            content: RecipeContent {
                sound: Sound::new(
                    d.sound.duration_seconds,
                    d.sound.fade_seconds,
                    d.sound.component,
                )
                .map_err(RecipeError::Structure)?,
                description: d.description,
                sources: d.source,
                evidence: d.evidence,
                lineage: d.lineage,
            },
        };
        value.normalize_sources();
        value.validate()?;
        Ok(value)
    }
    fn normalize_sources(&mut self) {
        for source in &mut self.content.sources {
            if let Some(doi) = source
                .doi
                .as_deref()
                .and_then(crate::provenance::normalize_doi)
            {
                source.doi = Some(doi);
            }
        }
    }
    /// Check portable structure without target compilation, rendering or I/O.
    ///
    /// # Errors
    /// Returns the first metadata, bibliography-reference or attribution-budget error.
    fn validate(&self) -> Result<(), RecipeError> {
        if self
            .content
            .description
            .as_ref()
            .is_some_and(|d| d.title.trim().is_empty())
        {
            return Err(RecipeError::Contract(ContractError::EmptyTitle));
        }
        crate::provenance::validate(
            &self.content.sources,
            &self.content.evidence,
            &self.content.lineage,
        )
        .map_err(RecipeError::Contract)
    }
    /// Canonical bounded TOML, preserving every authored field and source scope.
    ///
    /// # Errors
    /// Returns a TOML serialization error or refuses an over-limit serialized result.
    pub fn to_toml(&self) -> Result<String, RecipeError> {
        let text = toml::to_string(&RecipeDocument {
            schema_version: Self::SCHEMA_VERSION,
            document: self.document.clone(),
            description: self.content.description.clone(),
            sound: DocumentSound {
                duration_seconds: self.content.sound.duration_seconds(),
                fade_seconds: self.content.sound.fade_seconds(),
                component: self.content.sound.components().to_vec(),
            },
            source: self.content.sources.clone(),
            evidence: self.content.evidence.clone(),
            lineage: self.content.lineage.clone(),
        })
        .map_err(RecipeError::Write)?;
        if text.len() > Self::MAX_BYTES {
            return Err(RecipeError::TooLarge);
        }
        Ok(text)
    }
    /// Content independent of identity and declared dates.
    #[must_use]
    pub const fn content(&self) -> &RecipeContent {
        &self.content
    }

    /// Whether the flat ancestry has room for an independent copy.
    ///
    /// # Errors
    /// Returns the exact capacity failure before a caller allocates a new identity.
    pub fn copy_readiness(&self) -> Result<(), ContractError> {
        if self.content.lineage.len() >= crate::provenance::MAX_LINEAGE {
            return Err(ContractError::CopyLineageFull {
                count: self.content.lineage.len(),
            });
        }
        Ok(())
    }
    /// Fork under a new supplied identity and explicit attribution note.
    /// Retain bibliography and demote evidence to ancestor scope.
    ///
    /// # Errors
    /// Refuses ancestry overflow, an empty note or reuse of the original UUID;
    /// drops no history.
    pub fn fork(
        &self,
        document: DocumentIdentity,
        catalog_id: Option<String>,
        note: String,
    ) -> Result<Self, RecipeError> {
        if document.id == self.document.id {
            return Err(RecipeError::Contract(ContractError::ReusedCopyIdentity {
                id: document.id,
            }));
        }
        self.copy_readiness().map_err(RecipeError::Contract)?;
        if note.trim().is_empty() {
            return Err(RecipeError::Contract(ContractError::EmptyLineageNote {
                index: self.content.lineage.len(),
            }));
        }
        let mut copy = self.clone();
        copy.document = document;
        copy.content.lineage.push(Lineage {
            title: self
                .content
                .description
                .as_ref()
                .map_or_else(|| self.document.id.to_string(), |d| d.title.clone()),
            note,
            document_id: Some(self.document.id),
            catalog_id,
            source_keys: self.content.sources.iter().map(|s| s.key.clone()).collect(),
        });
        for e in &mut copy.content.evidence {
            e.scope = EvidenceScope::Ancestor;
        }
        Ok(copy)
    }
}
/// Editable portable content; only `build` produces a validated recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeBuilder {
    /// Provisional document identity and history.
    pub document: DocumentIdentity,
    /// Editable content, validated when built.
    pub content: RecipeContent,
}
impl RecipeBuilder {
    /// Validate portable structure without acoustic validation at a sample rate.
    ///
    /// # Errors
    /// Rejects metadata, bibliography-reference or attribution-budget errors.
    pub fn build(self) -> Result<Recipe, RecipeError> {
        let mut recipe = Recipe {
            document: self.document,
            content: self.content,
        };
        recipe.normalize_sources();
        recipe.validate()?;
        Ok(recipe)
    }
    /// Content independent of provisional identity and dates.
    #[must_use]
    pub const fn content(&self) -> &RecipeContent {
        &self.content
    }
}

crate::diagnostic_codes! {
    pub(crate) const CODES;
    RecipeError {
        codes {
            Self::TooLarge => "recipe.size",
            Self::NotUtf8 => "recipe.utf8",
            Self::Utf16 => "recipe.utf16",
            Self::Format(_) => "recipe.format",
            Self::Write(_) => "recipe.write",
            Self::UnsupportedVersion { .. } => "recipe.schema_version",
        }
        delegates {
            Self::Contract(error) => error,
            Self::Structure(error) => error,
        }
        params(this) {
            match this {
                Self::UnsupportedVersion { found } => {
                    crate::diagnostic_params! {
                        "found"=>*found,
                        "supported"=>Recipe::SCHEMA_VERSION,
                    }
                }
                _ => crate::Params::new(),
            }
        }
    }
}

#[cfg(test)]
mod tests;
