// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The built-in recipes: stimuli adapted from published auditory studies.
//!
//! Each entry is the text of one file in the crate's `catalog` directory,
//! named by its file stem. Parse it with [`Recipe::parse`](crate::Recipe::parse);
//! the text includes the comments that explain the recipe.

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
    /// The recipe's TOML text.
    pub text: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/catalog.rs"));

/// Return the built-in recipe with this name.
#[must_use]
pub fn find(name: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|entry| entry.id == name)
}

#[cfg(test)]
mod tests;
