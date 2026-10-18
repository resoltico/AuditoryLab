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
    pub name: &'static str,
    /// The preset's TOML text.
    pub text: &'static str,
}

macro_rules! entries {
    ($($name:literal),* $(,)?) => {
        &[$(Entry { name: $name, text: include_str!(concat!("../catalog/", $name, ".toml")) }),*]
    };
}

/// Every built-in preset in name order, comparing runs of digits as numbers
/// so that `8-hz` precedes `40-hz`.
pub const ENTRIES: &[Entry] = entries![
    "beauchene-2016-binaural-5-hz",
    "beauchene-2016-binaural-10-hz",
    "beauchene-2016-binaural-15-hz",
    "beauchene-2016-pure-tone-240-hz",
    "chan-2022-clicks-40-hz",
    "ioannou-2015-binaural-10-hz",
    "ioannou-2015-pure-tone-200-hz",
    "jirakittayakorn-2017-binaural-6-hz",
    "lahijanian-2024-tone-pips-40-hz",
    "martorell-2019-tone-pips-8-hz",
    "martorell-2019-tone-pips-40-hz",
    "martorell-2019-tone-pips-80-hz",
    "orozco-perez-2020-binaural-7-hz",
    "orozco-perez-2020-binaural-40-hz",
    "orozco-perez-2020-monaural-7-hz",
    "orozco-perez-2020-monaural-40-hz",
    "van-eeckhoutte-2018-am-500-hz",
    "van-eeckhoutte-2018-am-1000-hz",
    "van-eeckhoutte-2018-am-2000-hz",
];

/// Return the built-in preset with this name.
#[must_use]
pub fn find(name: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|entry| entry.name == name)
}
