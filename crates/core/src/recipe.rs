// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::{
    BibliographySource, Component, Description, DocumentIdentity, Evidence, EvidenceScope, Lineage,
    Sound,
};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::{self, Display, Formatter};
use serde::{Deserialize, Serialize};

/// One portable authored document, independent of local ownership and membership.
/// Schema 4 uses a strict bounded document shape; finite acoustic errors remain
/// editable and serializable.
///
/// Recipe content is immutable outside core; edits must pass a checked builder.
/// ```compile_fail
/// use auditorylab_core::{Recipe, catalog};
/// let mut recipe = Recipe::parse(catalog::ENTRIES[0].text).unwrap();
/// recipe.sound.duration_seconds = 1.0;
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    /// Stable identity and optional declared history, never filesystem dates.
    document: DocumentIdentity,
    /// Ordered authored sound; pairs retain their signed nominal offset.
    sound: Sound,
    /// Current title and notes, independent of source evidence.
    description: Option<Description>,
    /// Bounded bibliography referenced by evidence and lineage.
    sources: Vec<BibliographySource>,
    /// Full scoped evidence declarations, never a trust flag.
    evidence: Vec<Evidence>,
    /// Flat ancestor descriptors, not recursively embedded documents.
    lineage: Vec<Lineage>,
}

/// Derived presentation of current title/notes and a primary current condition.
/// This is a view, not a second persisted metadata authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct About {
    /// Current title.
    pub title: String,
    /// Current own notes.
    pub summary: Option<String>,
    /// Primary current-condition context; complete evidence remains on Recipe.
    pub research: Option<Research>,
}
/// A concise derived research view. Full records retain their explicit scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Research {
    /// Declared population/role context.
    pub studied_for: Option<String>,
    /// Findings with limitations.
    pub summary: Option<String>,
    /// Primary source citation.
    pub citation: Option<String>,
    /// Explicit source URL, never fetched automatically.
    pub source_url: Option<String>,
    /// Primary methods locator.
    pub study: Option<String>,
    /// Disclosed adaptations.
    pub adaptation: Option<String>,
}
/// Why a byte sequence does not satisfy the one supported portable contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecipeError {
    /// Portable byte bound exceeded.
    TooLarge,
    /// Input is not UTF-8.
    NotUtf8,
    /// UTF-16 byte order mark detected.
    Utf16,
    /// Strict TOML shape/type error.
    Format(toml::de::Error),
    /// Identity, reference, budget or finite-number contract failed.
    Contract(String),
    /// TOML serialization failed.
    Write(String),
    /// A different schema is not read or converted.
    UnsupportedVersion {
        /// The declared unsupported version.
        found: u32,
    },
}
impl Display for RecipeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(f, "recipe exceeds {} bytes", Recipe::MAX_BYTES),
            Self::NotUtf8 => f.write_str("recipe is not UTF-8 text"),
            Self::Utf16 => f.write_str("recipe is UTF-16 text; save it as UTF-8"),
            Self::Format(e) => write!(f, "recipe does not match the format: {e}"),
            Self::Contract(e) => write!(f, "recipe contract: {e}"),
            Self::Write(e) => write!(f, "cannot serialize recipe: {e}"),
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
            Self::Format(e) => Some(e),
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
struct Document {
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
    duration_seconds: f64,
    fade_seconds: f64,
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
            sound: self.sound,
            description: self.description,
            sources: self.sources,
            evidence: self.evidence,
            lineage: self.lineage,
        }
    }
    /// The document's document.
    #[must_use]
    pub const fn document(&self) -> &DocumentIdentity {
        &self.document
    }
    /// Replace document while enforcing the portable contract.
    ///
    /// # Errors
    /// Rejects invalid structure, references or nonfinite authored numbers.
    pub fn with_document(self, value: DocumentIdentity) -> Result<Self, RecipeError> {
        let mut builder = self.into_builder();
        builder.document = value;
        builder.build()
    }
    /// The document's sound.
    #[must_use]
    pub const fn sound(&self) -> &Sound {
        &self.sound
    }
    /// Replace sound while enforcing the portable contract.
    ///
    /// # Errors
    /// Rejects invalid structure, references or nonfinite authored numbers.
    pub fn with_sound(self, value: Sound) -> Result<Self, RecipeError> {
        let mut builder = self.into_builder();
        builder.sound = value;
        builder.build()
    }
    /// The document's description.
    #[must_use]
    pub const fn description(&self) -> &Option<Description> {
        &self.description
    }
    /// Replace description while enforcing the portable contract.
    ///
    /// # Errors
    /// Rejects invalid structure, references or nonfinite authored numbers.
    pub fn with_description(self, value: Option<Description>) -> Result<Self, RecipeError> {
        let mut builder = self.into_builder();
        builder.description = value;
        builder.build()
    }
    /// The document's sources.
    #[must_use]
    pub const fn sources(&self) -> &Vec<BibliographySource> {
        &self.sources
    }
    /// Replace sources while enforcing the portable contract.
    ///
    /// # Errors
    /// Rejects invalid structure, references or nonfinite authored numbers.
    pub fn with_sources(self, value: Vec<BibliographySource>) -> Result<Self, RecipeError> {
        let mut builder = self.into_builder();
        builder.sources = value;
        builder.build()
    }
    /// The document's evidence.
    #[must_use]
    pub const fn evidence(&self) -> &Vec<Evidence> {
        &self.evidence
    }
    /// Replace evidence while enforcing the portable contract.
    ///
    /// # Errors
    /// Rejects invalid structure, references or nonfinite authored numbers.
    pub fn with_evidence(self, value: Vec<Evidence>) -> Result<Self, RecipeError> {
        let mut builder = self.into_builder();
        builder.evidence = value;
        builder.build()
    }
    /// The document's lineage.
    #[must_use]
    pub const fn lineage(&self) -> &Vec<Lineage> {
        &self.lineage
    }
    /// Replace lineage while enforcing the portable contract.
    ///
    /// # Errors
    /// Rejects invalid structure, references or nonfinite authored numbers.
    pub fn with_lineage(self, value: Vec<Lineage>) -> Result<Self, RecipeError> {
        let mut builder = self.into_builder();
        builder.lineage = value;
        builder.build()
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
            sound: Sound {
                duration_seconds: 60.0,
                fade_seconds: 0.1,
                components: Vec::new(),
            },
            description: None,
            sources: Vec::new(),
            evidence: Vec::new(),
            lineage: Vec::new(),
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
        let d: Document = toml::from_str(text).map_err(RecipeError::Format)?;
        let mut value = Self {
            document: d.document,
            sound: Sound {
                duration_seconds: d.sound.duration_seconds,
                fade_seconds: d.sound.fade_seconds,
                components: d.sound.component,
            },
            description: d.description,
            sources: d.source,
            evidence: d.evidence,
            lineage: d.lineage,
        };
        value.normalize_sources();
        value.validate()?;
        Ok(value)
    }
    fn normalize_sources(&mut self) {
        for source in &mut self.sources {
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
    /// Returns the first nonfinite, identity, capacity or metadata reference error.
    fn validate(&self) -> Result<(), RecipeError> {
        let invalid = |message: &str| RecipeError::Contract(message.into());
        if let Some((component, parameter)) = self.sound.nonfinite().next() {
            return Err(RecipeError::Contract(alloc::format!(
                "{parameter} must be finite (component {component:?})"
            )));
        }
        Sound::check_structure(&self.sound.components)
            .map_err(|error| RecipeError::Contract(error.to_string()))?;
        if self
            .description
            .as_ref()
            .is_some_and(|d| d.title.trim().is_empty())
        {
            return Err(invalid("description title must not be empty"));
        }
        crate::provenance::validate(&self.sources, &self.evidence, &self.lineage).map_err(invalid)
    }
    /// Canonical bounded TOML, preserving every authored field and source scope.
    ///
    /// # Errors
    /// Returns a TOML serialization error or refuses an over-limit serialized result.
    pub fn to_toml(&self) -> Result<String, RecipeError> {
        let text = toml::to_string(&Document {
            schema_version: Self::SCHEMA_VERSION,
            document: self.document.clone(),
            description: self.description.clone(),
            sound: DocumentSound {
                duration_seconds: self.sound.duration_seconds,
                fade_seconds: self.sound.fade_seconds,
                component: self.sound.components.clone(),
            },
            source: self.sources.clone(),
            evidence: self.evidence.clone(),
            lineage: self.lineage.clone(),
        })
        .map_err(|e| RecipeError::Write(e.to_string()))?;
        if text.len() > Self::MAX_BYTES {
            return Err(RecipeError::TooLarge);
        }
        Ok(text)
    }
    /// Editable semantic content equality, excluding declared dates and identity.
    /// Exact sound bits/order/owners are preserved; normalized DOI identity is used.
    #[must_use]
    pub fn same_editable_content(&self, other: &Self) -> bool {
        self.clone()
            .into_builder()
            .same_editable_content(&other.clone().into_builder())
    }
    /// Identity, normalized declared history and full editable content equality.
    #[must_use]
    pub fn same_document(&self, other: &Self) -> bool {
        self.document == other.document && self.same_editable_content(other)
    }

    /// Fork under a new supplied identity; retain bibliography and scoped ancestry.
    ///
    /// # Errors
    /// Refuses ancestry overflow or reuse of the original UUID; drops no history.
    pub fn fork(
        &self,
        document: DocumentIdentity,
        catalog_id: Option<String>,
    ) -> Result<Self, RecipeError> {
        if document.id == self.document.id {
            return Err(RecipeError::Contract(
                "copy requires a new document UUID".into(),
            ));
        }
        if self.lineage.len() >= crate::provenance::MAX_LINEAGE {
            return Err(RecipeError::Contract(
                "lineage capacity reached; review metadata before making a copy".into(),
            ));
        }
        let mut copy = self.clone();
        copy.document = document;
        copy.lineage.push(Lineage {
            title: self
                .description
                .as_ref()
                .map_or_else(|| self.document.id.to_string(), |d| d.title.clone()),
            note: self.description.as_ref().and_then(|d|d.notes.as_deref()).filter(|s|!s.is_empty()).map_or_else(||"Attributed copy; source findings concern the ancestor, not this sound.".into(),|notes|alloc::format!("Attributed copy; source findings concern the ancestor, not this sound. Ancestor notes: {notes}")),
            document_id: Some(self.document.id),
            catalog_id,
            source_keys: self.sources.iter().map(|s| s.key.clone()).collect(),
        });
        for e in &mut copy.evidence {
            e.scope = EvidenceScope::Ancestor;
        }
        Ok(copy)
    }
    /// Derive concise presentation; all full records remain separately available.
    #[must_use]
    pub fn display_about(&self) -> Option<About> {
        let description = self.description.as_ref()?;
        let research = self
            .evidence
            .iter()
            .find(|e| e.scope == EvidenceScope::ReferenceCondition)
            .and_then(|e| {
                self.sources
                    .iter()
                    .find(|s| s.key == e.source)
                    .map(|s| Research {
                        studied_for: e.population_detail.clone().or_else(|| {
                            Some(alloc::format!(
                                "{:?} study · {:?}",
                                e.population,
                                e.condition_role
                            ))
                        }),
                        summary: Some(alloc::format!("{} {}", e.summary, e.limitations)),
                        citation: Some(s.citation.clone()),
                        source_url: s.url.clone().or_else(|| {
                            s.doi
                                .as_ref()
                                .map(|d| alloc::format!("https://doi.org/{d}"))
                        }),
                        study: Some(e.methods_locator.clone()),
                        adaptation: Some(e.adaptation.clone()),
                    })
            });
        Some(About {
            title: description.title.clone(),
            summary: description.notes.clone(),
            research,
        })
    }
}
/// Editable portable content; only `build` produces a validated recipe.
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeBuilder {
    /// Stable identity and optional declared history, never filesystem dates.
    pub document: DocumentIdentity,
    /// Ordered authored sound; pairs retain their signed nominal offset.
    pub sound: Sound,
    /// Current title and notes, independent of source evidence.
    pub description: Option<Description>,
    /// Bounded bibliography referenced by evidence and lineage.
    pub sources: Vec<BibliographySource>,
    /// Full scoped evidence declarations, never a trust flag.
    pub evidence: Vec<Evidence>,
    /// Flat ancestor descriptors, not recursively embedded documents.
    pub lineage: Vec<Lineage>,
}
impl RecipeBuilder {
    /// Validate portable structure without acoustic validation at a sample rate.
    ///
    /// # Errors
    /// Rejects nonfinite numbers, duplicate identities, capacity and metadata errors.
    pub fn build(self) -> Result<Recipe, RecipeError> {
        let mut recipe = Recipe {
            document: self.document,
            sound: self.sound,
            description: self.description,
            sources: self.sources,
            evidence: self.evidence,
            lineage: self.lineage,
        };
        recipe.normalize_sources();
        recipe.validate()?;
        Ok(recipe)
    }
    /// Exact editable content comparison, excluding identity and declared history.
    #[must_use]
    pub fn same_editable_content(&self, other: &Self) -> bool {
        self.sound.same_content(&other.sound)
            && self.description == other.description
            && self.sources.len() == other.sources.len()
            && self.sources.iter().zip(&other.sources).all(|(a, b)| {
                a.key == b.key
                    && a.label == b.label
                    && a.citation == b.citation
                    && a.url == b.url
                    && a.doi.as_deref().and_then(crate::provenance::normalize_doi)
                        == b.doi.as_deref().and_then(crate::provenance::normalize_doi)
            })
            && self.evidence == other.evidence
            && self.lineage == other.lineage
    }
}
#[cfg(test)]
mod tests;
