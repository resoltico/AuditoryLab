// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The built-in recipes: stimuli adapted from published auditory studies.
//!
//! Each entry merges a condition and its study bibliography into generated TOML
//! without comments. Parse it with [`Recipe::parse`](crate::Recipe::parse).
//! Explanations and adaptations are documented in `docs/catalog.md`.

/// A canonical catalog name. Existence is checked separately during resolution.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogId(alloc::string::String);
impl CatalogId {
    /// Validate lowercase ASCII letters, digits and hyphens; names cannot be empty.
    #[must_use]
    pub fn new(value: &str) -> Option<Self> {
        crate::catalog_id::valid(value).then(|| Self(value.into()))
    }
    /// Canonical name without a selector prefix.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl core::fmt::Display for CatalogId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}
impl serde::Serialize for CatalogId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}
/// One built-in recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The recipe's name: lowercase letters, digits and hyphens.
    pub id: &'static str,
    /// The curated condition, before bibliography and shared limitations are merged.
    condition: &'static str,
}

/// Return the built-in recipe with this name.
#[must_use]
pub fn find(name: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|entry| entry.id == name)
}

use alloc::{string::String, vec::Vec};
use serde::Deserialize;

/// Recipes adapted from published conditions share this evidence boundary.
const LIMITATIONS: &str = "Source findings concern the stated population and condition; \
    adaptations do not establish efficacy for generated audio. See the \
    summary and adaptation for study-specific limitations.";

/// Built-in recipes in curated natural ID order.
pub const ENTRIES: &[Entry] = &[
    Entry {
        id: "beauchene-2016-binaural-5-hz",
        condition: include_str!("../../catalog/beauchene-2016-binaural-5-hz.toml"),
    },
    Entry {
        id: "beauchene-2016-binaural-10-hz",
        condition: include_str!("../../catalog/beauchene-2016-binaural-10-hz.toml"),
    },
    Entry {
        id: "beauchene-2016-binaural-15-hz",
        condition: include_str!("../../catalog/beauchene-2016-binaural-15-hz.toml"),
    },
    Entry {
        id: "beauchene-2016-pure-tone-240-hz",
        condition: include_str!("../../catalog/beauchene-2016-pure-tone-240-hz.toml"),
    },
    Entry {
        id: "chan-2022-clicks-40-hz",
        condition: include_str!("../../catalog/chan-2022-clicks-40-hz.toml"),
    },
    Entry {
        id: "ioannou-2015-binaural-2-hz",
        condition: include_str!("../../catalog/ioannou-2015-binaural-2-hz.toml"),
    },
    Entry {
        id: "ioannou-2015-binaural-6-hz",
        condition: include_str!("../../catalog/ioannou-2015-binaural-6-hz.toml"),
    },
    Entry {
        id: "ioannou-2015-binaural-10-hz",
        condition: include_str!("../../catalog/ioannou-2015-binaural-10-hz.toml"),
    },
    Entry {
        id: "ioannou-2015-binaural-20-hz",
        condition: include_str!("../../catalog/ioannou-2015-binaural-20-hz.toml"),
    },
    Entry {
        id: "ioannou-2015-binaural-40-hz",
        condition: include_str!("../../catalog/ioannou-2015-binaural-40-hz.toml"),
    },
    Entry {
        id: "ioannou-2015-pure-tone-200-hz",
        condition: include_str!("../../catalog/ioannou-2015-pure-tone-200-hz.toml"),
    },
    Entry {
        id: "jirakittayakorn-2017-binaural-6-hz",
        condition: include_str!("../../catalog/jirakittayakorn-2017-binaural-6-hz.toml"),
    },
    Entry {
        id: "lahijanian-2024-tone-pips-40-hz",
        condition: include_str!("../../catalog/lahijanian-2024-tone-pips-40-hz.toml"),
    },
    Entry {
        id: "martorell-2019-tone-pips-8-hz",
        condition: include_str!("../../catalog/martorell-2019-tone-pips-8-hz.toml"),
    },
    Entry {
        id: "martorell-2019-tone-pips-40-hz",
        condition: include_str!("../../catalog/martorell-2019-tone-pips-40-hz.toml"),
    },
    Entry {
        id: "martorell-2019-tone-pips-80-hz",
        condition: include_str!("../../catalog/martorell-2019-tone-pips-80-hz.toml"),
    },
    Entry {
        id: "orozco-perez-2020-binaural-7-hz",
        condition: include_str!("../../catalog/orozco-perez-2020-binaural-7-hz.toml"),
    },
    Entry {
        id: "orozco-perez-2020-binaural-40-hz",
        condition: include_str!("../../catalog/orozco-perez-2020-binaural-40-hz.toml"),
    },
    Entry {
        id: "orozco-perez-2020-monaural-7-hz",
        condition: include_str!("../../catalog/orozco-perez-2020-monaural-7-hz.toml"),
    },
    Entry {
        id: "orozco-perez-2020-monaural-40-hz",
        condition: include_str!("../../catalog/orozco-perez-2020-monaural-40-hz.toml"),
    },
    Entry {
        id: "van-eeckhoutte-2018-am-500-hz",
        condition: include_str!("../../catalog/van-eeckhoutte-2018-am-500-hz.toml"),
    },
    Entry {
        id: "van-eeckhoutte-2018-am-1000-hz",
        condition: include_str!("../../catalog/van-eeckhoutte-2018-am-1000-hz.toml"),
    },
    Entry {
        id: "van-eeckhoutte-2018-am-2000-hz",
        condition: include_str!("../../catalog/van-eeckhoutte-2018-am-2000-hz.toml"),
    },
];

const STUDIES: &[(&str, &str)] = &[
    (
        "beauchene-2016",
        include_str!("../../studies/beauchene-2016.toml"),
    ),
    ("chan-2022", include_str!("../../studies/chan-2022.toml")),
    (
        "ingendoh-2023",
        include_str!("../../studies/ingendoh-2023.toml"),
    ),
    (
        "ioannou-2015",
        include_str!("../../studies/ioannou-2015.toml"),
    ),
    (
        "jirakittayakorn-2017",
        include_str!("../../studies/jirakittayakorn-2017.toml"),
    ),
    (
        "lahijanian-2024",
        include_str!("../../studies/lahijanian-2024.toml"),
    ),
    (
        "martorell-2019",
        include_str!("../../studies/martorell-2019.toml"),
    ),
    (
        "orozco-perez-2020",
        include_str!("../../studies/orozco-perez-2020.toml"),
    ),
    (
        "van-eeckhoutte-2018",
        include_str!("../../studies/van-eeckhoutte-2018.toml"),
    ),
];

impl Entry {
    /// Build the strict typed portable recipe, merging authoritative study facts.
    /// No serialization or portable-text reparsing occurs on this path.
    pub fn recipe(&self) -> Result<crate::Recipe, crate::RecipeError> {
        generated_recipe(self.condition, |key| {
            STUDIES
                .iter()
                .find(|(id, _)| *id == key)
                .map(|(_, text)| String::from(*text))
                .ok_or_else(|| author_error("unknown built-in study"))
        })
    }
    /// Standalone portable TOML, with study citations and evidence limitations.
    #[must_use]
    pub fn text(&self) -> String {
        self.recipe()
            .and_then(|recipe| recipe.to_toml())
            .unwrap_or_else(|error| panic!("invalid built-in {}: {error}", self.id))
    }
}

fn author_error(message: &str) -> crate::RecipeError {
    crate::RecipeError::Format(<toml::de::Error as serde::de::Error>::custom(message))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Condition {
    schema_version: u32,
    document: crate::DocumentIdentity,
    description: Option<crate::Description>,
    timing: crate::FiniteTiming,
    sound: ConditionSound,
    source: Vec<ConditionSource>,
    evidence: Vec<ConditionEvidence>,
    #[serde(default)]
    lineage: Vec<crate::Lineage>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConditionSound {
    component: Vec<crate::Component>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConditionSource {
    key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Study {
    label: String,
    citation: String,
    source_url: String,
}
fn default_limitations() -> String {
    LIMITATIONS.into()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConditionEvidence {
    source: String,
    scope: crate::EvidenceScope,
    population: crate::Population,
    population_detail: Option<String>,
    condition_role: crate::ConditionRole,
    measures: Vec<crate::Measure>,
    methods_locator: String,
    summary: String,
    #[serde(default = "default_limitations")]
    limitations: String,
    adaptation: String,
}
impl From<ConditionEvidence> for crate::Evidence {
    fn from(value: ConditionEvidence) -> Self {
        Self {
            source: value.source,
            scope: value.scope,
            population: value.population,
            population_detail: value.population_detail,
            condition_role: value.condition_role,
            measures: value.measures,
            methods_locator: value.methods_locator,
            summary: value.summary,
            limitations: value.limitations,
            adaptation: value.adaptation,
        }
    }
}
fn generated_recipe(
    condition: &str,
    mut read: impl FnMut(&str) -> Result<String, crate::RecipeError>,
) -> Result<crate::Recipe, crate::RecipeError> {
    let condition: Condition = toml::from_str(condition).map_err(crate::RecipeError::Format)?;
    if condition.schema_version != crate::Recipe::SCHEMA_VERSION {
        return Err(crate::RecipeError::UnsupportedVersion {
            found: condition.schema_version,
        });
    }
    if condition.source.is_empty() {
        return Err(author_error(
            "catalog conditions require a bibliography record",
        ));
    }
    let mut sources = Vec::with_capacity(condition.source.len());
    for source in condition.source {
        if !crate::catalog_id::valid(&source.key) {
            return Err(author_error("invalid study source ID"));
        }
        let study: Study =
            toml::from_str(&read(&source.key)?).map_err(crate::RecipeError::Format)?;
        let doi = study
            .source_url
            .strip_prefix("https://doi.org/")
            .and_then(crate::provenance::normalize_doi)
            .ok_or_else(|| author_error("study URL must be a DOI URL"))?;
        sources.push(crate::BibliographySource {
            key: source.key,
            label: study.label,
            citation: study.citation,
            doi: Some(doi),
            url: Some(study.source_url),
        });
    }
    let mut builder = crate::Recipe::builder(condition.document);
    builder.content = crate::RecipeContent {
        sound: crate::Sound::new(condition.sound.component)
            .map_err(crate::RecipeError::Structure)?,
        timing: condition.timing,
        description: condition.description,
        sources,
        evidence: condition.evidence.into_iter().map(Into::into).collect(),
        lineage: condition.lineage,
    };
    builder.build()
}

#[cfg(test)]
mod tests;
