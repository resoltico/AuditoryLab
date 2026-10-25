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
/// Only unreleased schema 3 is supported. The writer and reader share the strict
/// bounded shape; finite acoustic errors remain editable and serializable.
#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    /// Stable identity and optional declared history, never filesystem dates.
    pub document: DocumentIdentity,
    /// Ordered authored sound; pairs retain their signed nominal offset.
    pub stimulus: Sound,
    /// Current title and notes, independent of source evidence.
    pub description: Option<Description>,
    /// Bounded bibliography referenced by evidence and lineage.
    pub sources: Vec<BibliographySource>,
    /// Full scoped evidence declarations, never a trust flag.
    pub evidence: Vec<Evidence>,
    /// Flat ancestor descriptors, not recursively embedded documents.
    pub lineage: Vec<Lineage>,
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
    duration_seconds: f64,
    fade_seconds: f64,
    document: DocumentIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<Description>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    component: Vec<Component>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    source: Vec<BibliographySource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    evidence: Vec<Evidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lineage: Vec<Lineage>,
}
impl Recipe {
    /// The only supported recipe schema; extended in place while Unreleased.
    pub const SCHEMA_VERSION: u32 = 3;
    /// Maximum portable UTF-8 bytes, including metadata.
    pub const MAX_BYTES: usize = 65_536;
    /// An empty authored document with the supplied provisional identity/history.
    #[must_use]
    pub fn silent(document: DocumentIdentity) -> Self {
        Self {
            document,
            stimulus: Sound {
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
            stimulus: Sound {
                duration_seconds: d.duration_seconds,
                fade_seconds: d.fade_seconds,
                components: d.component,
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
    pub fn validate(&self) -> Result<(), RecipeError> {
        let invalid = |message: &str| RecipeError::Contract(message.into());
        if let Some((component, parameter)) = self.stimulus.nonfinite().next() {
            return Err(RecipeError::Contract(alloc::format!(
                "{parameter} must be finite (component {component:?})"
            )));
        }
        let nodes = &self.stimulus.components;
        if nodes.len() > crate::Sound::MAX_COMPONENTS
            || nodes.iter().map(|c| c.node.voices()).sum::<usize>() > crate::Sound::MAX_VOICES
        {
            return Err(invalid(
                "at most 16 authored nodes and 16 expanded voices; a pair costs two",
            ));
        }
        for (i, c) in nodes.iter().enumerate() {
            if c.id.get() == 0
                || c.id.lane() != crate::Lane::Primary
                || nodes[..i].iter().any(|v| v.id == c.id)
            {
                return Err(invalid(
                    "component IDs must be unique positive primary-lane u32 values",
                ));
            }
        }
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
    /// Refuses invalid portable structure or an over-limit serialized result.
    pub fn to_toml(&self) -> Result<String, RecipeError> {
        self.validate()?;
        let mut normalized = self.clone();
        normalized.normalize_sources();
        let text = toml::to_string(&Document {
            schema_version: Self::SCHEMA_VERSION,
            duration_seconds: self.stimulus.duration_seconds,
            fade_seconds: self.stimulus.fade_seconds,
            document: self.document.clone(),
            description: self.description.clone(),
            component: self.stimulus.components.clone(),
            source: normalized.sources,
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
        self.stimulus.same_content(&other.stimulus)
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
        self.validate()?;
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
#[cfg(test)]
mod tests;
