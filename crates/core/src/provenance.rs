// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Portable bibliography and scoped evidence; declarations never grant ownership.
use crate::DocumentUuid;
use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

/// Maximum portable bibliography records.
pub const MAX_SOURCES: usize = 16;
/// Maximum scoped evidence records.
pub const MAX_EVIDENCE: usize = 32;
/// Maximum flat ancestry descriptors; no entry is silently discarded.
pub const MAX_LINEAGE: usize = 16;

/// User-visible description of the current authored sound.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Description {
    /// Nonempty title when a description is supplied.
    pub title: String,
    /// Notes about this version, independent of ancestor evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}
/// One bibliography record, referenced by a local key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BibliographySource {
    /// Unique local reference key.
    pub key: String,
    /// Human-readable label.
    pub label: String,
    /// Publication citation.
    pub citation: String,
    /// DOI identity, normalized independently of the citation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    /// Explicit http(s) URL, never fetched automatically.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}
/// Which authored content an evidence declaration concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceScope {
    /// Published reference condition with disclosed adaptations, not validation.
    ReferenceCondition,
    /// Retained ancestor context, not findings for the changed sound.
    Ancestor,
}
/// Population declared in one evidence record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Population {
    /// Human participants.
    Human,
    /// Mice.
    Mouse,
    /// Nonhuman animals other than mice.
    OtherAnimal,
    /// Multiple populations.
    Mixed,
    /// No population applies.
    NotApplicable,
    /// Not declared; never inferred from a title.
    Unspecified,
}
/// Experimental role declared for a single condition/evidence record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionRole {
    /// Test condition.
    Test,
    /// Comparison/control condition.
    Comparison,
    /// Multiple roles.
    Mixed,
    /// Unknown role.
    Unspecified,
}
/// A measure assessed by a source, not a promised outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    /// EEG recordings.
    Eeg,
    /// Behavioral measure.
    Behavior,
    /// Subjective report.
    Subjective,
    /// Sleep measurement.
    Sleep,
    /// Hearing measurement.
    Hearing,
    /// Another described measure.
    Other,
}
/// One scoped condition declaration; facets must correlate within this record.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    /// Bibliography key; dangling keys are rejected.
    pub source: String,
    /// Current reference condition versus retained ancestor.
    pub scope: EvidenceScope,
    /// Population of this declaration only.
    pub population: Population,
    /// Optional participant/sample detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub population_detail: Option<String>,
    /// Role of the stated condition.
    pub condition_role: ConditionRole,
    /// Measured quantities; empty means not assessed.
    pub measures: Vec<Measure>,
    /// Primary methods location.
    pub methods_locator: String,
    /// Findings and context, without guaranteed efficacy.
    pub summary: String,
    /// Evidence limitations, including animal/combined/null-result scope.
    pub limitations: String,
    /// Synthesis and presentation deviations from the source.
    pub adaptation: String,
}
/// One flat ancestor descriptor, never a recursive document or private path.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Lineage {
    /// Ancestor's title at the time of copying.
    pub title: String,
    /// Explicit relationship and changes, without inherited validation.
    pub note: String,
    /// Ancestor's declared identity when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<DocumentUuid>,
    /// Catalog identifier, never ownership authority or exact reconstruction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
    /// Bibliography keys retained for this ancestor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_keys: Vec<String>,
}
/// Check bounded metadata, explicit URLs and source references without I/O.
///
/// # Errors
/// Rejects capacity overflow, duplicate/dangling keys and malformed source shape.
pub fn validate(
    sources: &[BibliographySource],
    evidence: &[Evidence],
    lineage: &[Lineage],
) -> Result<(), &'static str> {
    if sources.len() > MAX_SOURCES || evidence.len() > MAX_EVIDENCE || lineage.len() > MAX_LINEAGE {
        return Err(
            "metadata capacity: at most 16 sources, 32 evidence records and 16 lineage descriptors; edit metadata explicitly rather than dropping history",
        );
    }
    for (i, source) in sources.iter().enumerate() {
        if source.key.trim().is_empty()
            || source.label.trim().is_empty()
            || source.citation.trim().is_empty()
            || sources[..i].iter().any(|s| s.key == source.key)
        {
            return Err("source keys must be unique and nonempty; label and citation are required");
        }
        if source.doi.is_none() && source.url.is_none() {
            return Err("source requires a DOI or explicit http(s) URL");
        }
        if let Some(url) = &source.url {
            let host = url
                .strip_prefix("https://")
                .or_else(|| url.strip_prefix("http://"))
                .ok_or("source URL must be explicit http(s)")?;
            if host.is_empty()
                || host.starts_with(['/', '?', '#'])
                || url.chars().any(char::is_whitespace)
                || url.chars().any(char::is_control)
            {
                return Err("source URL requires a host and no whitespace/control characters");
            }
        }
        if let Some(doi) = &source.doi
            && normalize_doi(doi).is_none()
        {
            return Err("DOI must begin with 10. and include a nonempty suffix");
        }
    }
    let exists = |key: &str| sources.iter().any(|s| s.key == key);
    if evidence.iter().any(|e| !exists(&e.source))
        || lineage
            .iter()
            .any(|l| l.source_keys.iter().any(|key| !exists(key)))
    {
        return Err("dangling bibliography reference");
    }
    if lineage
        .iter()
        .any(|l| l.title.trim().is_empty() || l.note.trim().is_empty())
    {
        return Err("lineage requires a title and an explicit relationship note");
    }
    Ok(())
}
/// Normalize DOI identity without rewriting the displayed citation.
#[must_use]
pub fn normalize_doi(doi: &str) -> Option<String> {
    let normalized = doi.trim().to_ascii_lowercase();
    let value = normalized
        .strip_prefix("https://doi.org/")
        .or_else(|| normalized.strip_prefix("http://doi.org/"))
        .or_else(|| normalized.strip_prefix("doi:"))
        .unwrap_or(&normalized)
        .trim();
    let (prefix, suffix) = value.split_once('/')?;
    (prefix.starts_with("10.")
        && prefix[3..].bytes().all(|b| b.is_ascii_digit())
        && prefix.len() > 3
        && !suffix.is_empty()
        && !value.chars().any(char::is_whitespace))
    .then(|| value.into())
}
