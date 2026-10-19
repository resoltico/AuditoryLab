// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The built-in presets: stimuli adapted from published auditory studies.
//!
//! Each entry is the text of one file in the crate's `catalog` directory,
//! named by its file stem. Parse it with [`Preset::parse`](crate::Preset::parse);
//! the text includes the comments that explain the recipe.

/// One built-in preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The preset's name: lowercase letters, digits and hyphens.
    pub id: &'static str,
    /// The preset's TOML text.
    pub text: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/catalog.rs"));

/// Return the built-in preset with this name.
#[must_use]
pub fn find(name: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|entry| entry.id == name)
}
