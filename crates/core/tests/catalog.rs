// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The shipped catalog, checked against recipes written out independently
//! of the files and against the files on disk.

#![cfg(test)]

use auditorylab_core::catalog::{self, Entry};
use auditorylab_core::{Carrier, Ears, Modulation, Recipe, SampleRate};
use std::collections::BTreeMap;
use std::path::Path;

/// How an independently written recipe modulates a component.
#[derive(Clone, Copy)]
enum Mod {
    /// Full-depth sinusoidal modulation at this rate.
    Sam(f64),
    /// Rectangular gating at this rate, open for 1 ms.
    Gate(f64),
}

/// (name, seconds, components as (ears, kind, Hz, peak dBFS, modulation)).
type Written = (
    &'static str,
    f64,
    &'static [(Ears, &'static str, f64, f64, Option<Mod>)],
);

const L: Ears = Ears::Left;
const R: Ears = Ears::Right;
const B: Ears = Ears::Both;
const MONAURAL: f64 = -23.0103;

const RECIPES: &[Written] = &[
    (
        "beauchene-2016-binaural-5-hz",
        300.0,
        &[
            (L, "tone", 245.0, -20.0, None),
            (R, "tone", 240.0, -20.0, None),
        ],
    ),
    (
        "beauchene-2016-binaural-10-hz",
        300.0,
        &[
            (L, "tone", 250.0, -20.0, None),
            (R, "tone", 240.0, -20.0, None),
        ],
    ),
    (
        "beauchene-2016-binaural-15-hz",
        300.0,
        &[
            (L, "tone", 255.0, -20.0, None),
            (R, "tone", 240.0, -20.0, None),
        ],
    ),
    (
        "beauchene-2016-pure-tone-240-hz",
        300.0,
        &[(B, "tone", 240.0, -20.0, None)],
    ),
    (
        "chan-2022-clicks-40-hz",
        180.0,
        &[(B, "click", 40.0, -20.0, None)],
    ),
    (
        "ioannou-2015-binaural-10-hz",
        60.0,
        &[
            (L, "tone", 200.0, -20.0, None),
            (R, "tone", 210.0, -20.0, None),
        ],
    ),
    (
        "ioannou-2015-pure-tone-200-hz",
        60.0,
        &[(B, "tone", 200.0, -20.0, None)],
    ),
    (
        "jirakittayakorn-2017-binaural-6-hz",
        1800.0,
        &[
            (L, "tone", 250.0, -20.0, None),
            (R, "tone", 256.0, -20.0, None),
        ],
    ),
    (
        "lahijanian-2024-tone-pips-40-hz",
        40.0,
        &[(B, "tone", 5000.0, -20.0, Some(Mod::Gate(40.0)))],
    ),
    (
        "martorell-2019-tone-pips-8-hz",
        3600.0,
        &[(B, "tone", 10_000.0, -20.0, Some(Mod::Gate(8.0)))],
    ),
    (
        "martorell-2019-tone-pips-40-hz",
        3600.0,
        &[(B, "tone", 10_000.0, -20.0, Some(Mod::Gate(40.0)))],
    ),
    (
        "martorell-2019-tone-pips-80-hz",
        3600.0,
        &[(B, "tone", 10_000.0, -20.0, Some(Mod::Gate(80.0)))],
    ),
    (
        "orozco-perez-2020-binaural-7-hz",
        480.0,
        &[
            (L, "tone", 396.5, -20.0, None),
            (R, "tone", 403.5, -20.0, None),
        ],
    ),
    (
        "orozco-perez-2020-binaural-40-hz",
        480.0,
        &[
            (L, "tone", 380.0, -20.0, None),
            (R, "tone", 420.0, -20.0, None),
        ],
    ),
    (
        "orozco-perez-2020-monaural-7-hz",
        480.0,
        &[
            (B, "tone", 396.5, MONAURAL, None),
            (B, "tone", 403.5, MONAURAL, None),
        ],
    ),
    (
        "orozco-perez-2020-monaural-40-hz",
        480.0,
        &[
            (B, "tone", 380.0, MONAURAL, None),
            (B, "tone", 420.0, MONAURAL, None),
        ],
    ),
    (
        "van-eeckhoutte-2018-am-500-hz",
        307.2,
        &[(L, "tone", 500.0, -20.0, Some(Mod::Sam(39.0625)))],
    ),
    (
        "van-eeckhoutte-2018-am-1000-hz",
        307.2,
        &[(L, "tone", 1000.0, -20.0, Some(Mod::Sam(39.0625)))],
    ),
    (
        "van-eeckhoutte-2018-am-2000-hz",
        307.2,
        &[(L, "tone", 2000.0, -20.0, Some(Mod::Sam(39.0625)))],
    ),
];

fn recipe(entry: &Entry) -> Recipe {
    Recipe::parse(entry.text).unwrap_or_else(|error| panic!("{}: {error}", entry.id))
}

/// Name order with runs of digits compared as numbers, written independently
/// of the catalog's own list.
fn natural(name: &str) -> Vec<(bool, u64, String)> {
    let mut parts = Vec::new();
    let mut rest = name;
    while let Some(first) = rest.chars().next() {
        let digits = first.is_ascii_digit();
        let end = rest
            .find(|c: char| c.is_ascii_digit() != digits)
            .unwrap_or(rest.len());
        let (part, tail) = rest.split_at(end);
        parts.push(if digits {
            (true, part.parse().unwrap(), String::new())
        } else {
            (false, 0, part.to_owned())
        });
        rest = tail;
    }
    parts
}

#[test]
fn the_catalog_is_exactly_the_files_in_the_catalog_directory_in_natural_name_order() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("catalog");
    let mut files: Vec<(String, String)> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            assert_eq!(
                path.extension().and_then(|e| e.to_str()),
                Some("toml"),
                "{path:?}"
            );
            let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
            (
                name,
                catalog_build::generated_recipe(&std::fs::read_to_string(&path).unwrap(), |id| {
                    Ok(std::fs::read_to_string(
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("studies")
                            .join(format!("{id}.toml")),
                    )?)
                })
                .unwrap(),
            )
        })
        .collect();
    files.sort_by_key(|(name, _)| natural(name));
    let entries: Vec<(String, String)> = catalog::ENTRIES
        .iter()
        .map(|e| (e.id.to_owned(), e.text.to_owned()))
        .collect();
    assert_eq!(entries, files);
    for entry in catalog::ENTRIES {
        assert!(
            entry
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "{}",
            entry.id
        );
        assert_eq!(catalog::find(entry.id), Some(entry));
    }
    assert_eq!(catalog::find("missing"), None);
    let names: Vec<_> = catalog::ENTRIES.iter().map(|e| e.id).collect();
    let position = |name| names.iter().position(|n| *n == name).unwrap();
    assert!(position("beauchene-2016-binaural-5-hz") < position("beauchene-2016-binaural-10-hz"));
    assert!(position("martorell-2019-tone-pips-8-hz") < position("martorell-2019-tone-pips-40-hz"));
}

#[test]
fn every_entry_has_its_independently_written_recipe() {
    assert_eq!(
        catalog::ENTRIES.iter().map(|e| e.id).collect::<Vec<_>>(),
        RECIPES.iter().map(|r| r.0).collect::<Vec<_>>()
    );
    for (entry, (name, seconds, parts)) in catalog::ENTRIES.iter().zip(RECIPES) {
        let stimulus = recipe(entry).stimulus;
        assert_eq!(stimulus.duration_seconds, *seconds, "{name}");
        assert_eq!(stimulus.fade_seconds, 0.1, "{name}");
        assert_eq!(stimulus.components.len(), parts.len(), "{name}");
        for (component, (ears, kind, hz, level, modulation)) in
            stimulus.components.iter().zip(*parts)
        {
            assert_eq!(component.ears, *ears, "{name}");
            assert_eq!(component.peak_dbfs, *level, "{name}");
            assert_eq!(
                component.modulation,
                modulation.map(|modulation| match modulation {
                    Mod::Sam(rate_hz) => Modulation::Sinusoidal {
                        rate_hz,
                        depth: 1.0,
                    },
                    Mod::Gate(rate_hz) => Modulation::Rectangular {
                        rate_hz,
                        width_seconds: 0.001,
                    },
                }),
                "{name}"
            );
            match (&component.carrier, *kind) {
                (Carrier::Tone { frequency_hz, .. }, "tone") => {
                    assert_eq!(frequency_hz, hz, "{name}")
                }
                (
                    Carrier::Click {
                        rate_hz,
                        width_seconds,
                        ..
                    },
                    "click",
                ) => {
                    assert_eq!(rate_hz, hz, "{name}");
                    assert_eq!(*width_seconds, 0.001, "{name}");
                }
                _ => panic!("{name}: expected a {kind}"),
            }
        }
        assert_eq!(
            stimulus.uses_separate_ears(),
            parts.iter().any(|p| p.0 != B),
            "{name}"
        );
    }
}

fn printable_about_value(value: &str) -> bool {
    value
        .chars()
        .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
}

#[test]
fn descriptive_text_supports_unicode_quotes_and_multiline() {
    assert!(printable_about_value("Rīga \"quoted\"\nline\ttab"));
    assert!(!printable_about_value("escape\u{1b}"));
}

#[test]
fn every_entry_states_all_about_text_and_one_source_has_one_citation() {
    let mut citations = BTreeMap::new();
    for entry in catalog::ENTRIES {
        let about = recipe(entry)
            .about
            .unwrap_or_else(|| panic!("{} has no about table", entry.id));
        let field = |name: &str, value: &Option<String>| {
            value
                .clone()
                .unwrap_or_else(|| panic!("{}: no {name}", entry.id))
        };
        let research = about.research.as_ref().unwrap();
        let citation = field("citation", &research.citation);
        let source_url = field("source_url", &research.source_url);
        for (name, value) in [
            ("title", about.title.clone()),
            ("studied_for", field("studied_for", &research.studied_for)),
            ("summary", field("summary", &research.summary)),
            ("citation", citation.clone()),
            ("source_url", source_url.clone()),
            ("study", field("study", &research.study)),
            ("adaptation", field("adaptation", &research.adaptation)),
        ] {
            assert!(!value.trim().is_empty(), "{}: empty {name}", entry.id);
            // Human text permits Unicode and TOML escaping; control bytes remain invalid.
            assert!(printable_about_value(&value), "{}: {name}", entry.id);
        }
        assert!(
            source_url.starts_with("https://doi.org/10."),
            "{}",
            entry.id
        );
        let year = entry
            .id
            .split('-')
            .find(|part| part.len() == 4 && part.bytes().all(|b| b.is_ascii_digit()))
            .unwrap();
        assert!(citation.contains(&format!("({year})")), "{}", entry.id);
        let first = citations
            .entry(source_url.clone())
            .or_insert_with(|| citation.clone());
        assert_eq!(*first, citation, "{}", entry.id);
    }
}

#[test]
fn every_entry_validates_and_pulses_follow_integer_onsets() {
    for entry in catalog::ENTRIES {
        for hz in [44_100, 48_000, 96_000] {
            let mut stimulus = recipe(entry).stimulus;
            let signal = stimulus.signal(SampleRate::new(hz).unwrap()).unwrap();
            assert_eq!(signal.duration_seconds(), stimulus.duration_seconds);
            stimulus.fade_seconds = 0.0;
            let signal = stimulus.signal(SampleRate::new(hz).unwrap()).unwrap();
            if let [part] = &stimulus.components[..] {
                let pulse_rate = match part.carrier {
                    Carrier::Click { rate_hz, .. } => Some(rate_hz),
                    _ => match part.modulation {
                        Some(Modulation::Rectangular { rate_hz, .. }) => Some(rate_hz),
                        _ => None,
                    },
                };
                if let Some(rate) = pulse_rate {
                    let rate = u64::from(format!("{rate:.0}").parse::<u32>().unwrap());
                    let width = u64::from((hz + 500) / 1000);
                    for k in 0..80 {
                        let onset = (k * u64::from(hz)).div_ceil(rate);
                        for offset in [0, width - 1, width] {
                            let value = signal.frame(onset + offset)[0];
                            if offset == width {
                                assert_eq!(value, 0.0);
                            } else if matches!(part.carrier, Carrier::Click { .. }) {
                                assert!(value > 0.0);
                            }
                        }
                    }
                }
            }
        }
    }
}
// Compile the build script's pure ordering test as part of this integration target.
#[path = "../build.rs"]
#[expect(
    dead_code,
    reason = "The integration test imports the real catalog build module to check catalog inputs; its unused Cargo build-script main is not executed by that test"
)]
mod catalog_build;
