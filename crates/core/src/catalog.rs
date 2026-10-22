// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The built-in recipes: stimuli adapted from published auditory studies.
//!
//! Each entry is the text of one file in the crate's `catalog` directory,
//! named by its file stem. Parse it with [`Recipe::parse`](crate::Recipe::parse);
//! the text includes the comments that explain the recipe.

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
