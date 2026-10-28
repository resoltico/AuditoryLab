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

use alloc::{boxed::Box, string::String};

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
    /// Standalone schema-4 TOML, with the study citation and evidence limitations.
    #[must_use]
    pub fn text(&self) -> String {
        generated_recipe(self.condition, |key| {
            STUDIES
                .iter()
                .find(|(id, _)| *id == key)
                .map(|(_, text)| String::from(*text))
                .ok_or_else(|| "unknown built-in study".into())
        })
        .unwrap_or_else(|error| panic!("invalid built-in {}: {error}", self.id))
    }
}

fn generated_recipe(
    condition: &str,
    read: impl FnOnce(&str) -> Result<String, Box<dyn core::error::Error>>,
) -> Result<String, Box<dyn core::error::Error>> {
    let mut document: toml::Table = toml::from_str(condition)?;
    let sources = document
        .get_mut("source")
        .and_then(toml::Value::as_array_mut)
        .ok_or("missing condition bibliography")?;
    if sources.len() != 1 {
        return Err("catalog conditions require one primary bibliography record".into());
    }
    let source = sources[0].as_table_mut().ok_or("source must be a table")?;
    let key = source
        .get("key")
        .and_then(toml::Value::as_str)
        .ok_or("missing source key")?;
    if key.is_empty()
        || !key
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err("invalid study source ID".into());
    }
    let study: toml::Table = toml::from_str(&read(key)?)?;
    if study.len() != 2 || !study.contains_key("citation") || !study.contains_key("source_url") {
        return Err("study must contain only citation and source_url".into());
    }
    let citation = study.get("citation").cloned().ok_or("missing citation")?;
    let url = study
        .get("source_url")
        .and_then(toml::Value::as_str)
        .ok_or("missing URL")?;
    let doi = url
        .strip_prefix("https://doi.org/")
        .ok_or("study URL must be a DOI URL")?;
    for (field, value) in [
        ("citation", citation),
        ("url", toml::Value::String(url.into())),
        ("doi", toml::Value::String(doi.to_ascii_lowercase())),
    ] {
        if source.insert(field.into(), value).is_some() {
            return Err("condition duplicates authoritative study citation/DOI".into());
        }
    }
    let evidence = document
        .get_mut("evidence")
        .and_then(toml::Value::as_array_mut)
        .ok_or("missing condition evidence")?;
    for record in evidence {
        record
            .as_table_mut()
            .ok_or("evidence must be a table")?
            .entry("limitations")
            .or_insert_with(|| toml::Value::String(LIMITATIONS.into()));
    }
    Ok(toml::to_string(&document)?)
}

#[cfg(test)]
mod tests;
