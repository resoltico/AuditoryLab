// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The shipped catalog, checked against recipes written out independently
//! of the files and against the files on disk.

#![cfg(test)]

use crate as auditorylab_core;
use auditorylab_core::catalog::{self, Entry};
use auditorylab_core::{ChannelRouting, Modulation, Recipe, SampleRate};
use std::path::Path;
use std::{borrow::ToOwned, format, string::String, vec::Vec};

/// How an independently written recipe modulates a component.
#[derive(Debug, Clone, Copy)]
enum Mod {
    /// Full-depth sinusoidal modulation at this rate.
    Sam(f64),
    /// Pulsed gating at this rate, open for 1 ms.
    Pulsed(f64),
}

/// (name, seconds, components as (route, kind, Hz, peak dBFS, modulation)).
type Written = (
    &'static str,
    f64,
    &'static [(ChannelRouting, &'static str, f64, f64, Option<Mod>)],
);

const L: ChannelRouting = ChannelRouting::Left;
const R: ChannelRouting = ChannelRouting::Right;
const B: ChannelRouting = ChannelRouting::Both;
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
        &[(B, "clicks", 40.0, -20.0, None)],
    ),
    (
        "ioannou-2015-binaural-2-hz",
        60.0,
        &[
            (L, "tone", 200.0, -20.0, None),
            (R, "tone", 202.0, -20.0, None),
        ],
    ),
    (
        "ioannou-2015-binaural-6-hz",
        60.0,
        &[
            (L, "tone", 200.0, -20.0, None),
            (R, "tone", 206.0, -20.0, None),
        ],
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
        "ioannou-2015-binaural-20-hz",
        60.0,
        &[
            (L, "tone", 200.0, -20.0, None),
            (R, "tone", 220.0, -20.0, None),
        ],
    ),
    (
        "ioannou-2015-binaural-40-hz",
        60.0,
        &[
            (L, "tone", 200.0, -20.0, None),
            (R, "tone", 240.0, -20.0, None),
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
        &[(B, "tone", 5000.0, -20.0, Some(Mod::Pulsed(40.0)))],
    ),
    (
        "martorell-2019-tone-pips-8-hz",
        3600.0,
        &[(B, "tone", 10_000.0, -20.0, Some(Mod::Pulsed(8.0)))],
    ),
    (
        "martorell-2019-tone-pips-40-hz",
        3600.0,
        &[(B, "tone", 10_000.0, -20.0, Some(Mod::Pulsed(40.0)))],
    ),
    (
        "martorell-2019-tone-pips-80-hz",
        3600.0,
        &[(B, "tone", 10_000.0, -20.0, Some(Mod::Pulsed(80.0)))],
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
    Recipe::parse(&entry.text()).unwrap_or_else(|error| panic!("{}: {error}", entry.id))
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
                super::generated_recipe(&std::fs::read_to_string(&path).unwrap(), |id| {
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
        .map(|e| (e.id.to_owned(), e.text()))
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
        let authored = crate::FiniteFixture::from_recipe(&recipe(entry));
        let sound = authored;
        let target = sound.target(SampleRate::DEFAULT).unwrap();
        let lanes = target.voices().collect::<Vec<_>>();
        assert_eq!(sound.duration_seconds().get(), *seconds, "{name}");
        assert_eq!(sound.fade_seconds().get(), 0.1, "{name}");
        assert_eq!(lanes.len(), parts.len(), "{name}");
        for ((_, component), (route, kind, hz, level, modulation)) in lanes.iter().zip(*parts) {
            assert_eq!(component.route, *route, "{name}");
            assert_eq!(component.peak_dbfs, *level, "{name}");
            match (component.modulation, modulation) {
                (None, None) => {}
                (
                    Some(crate::ModulationFacts::Sinusoidal { rate, depth }),
                    Some(Mod::Sam(expected)),
                ) => {
                    assert_eq!(rate.requested_hz, *expected, "{name}");
                    assert_eq!(depth, 1.0);
                }
                (Some(crate::ModulationFacts::Pulsed(p)), Some(Mod::Pulsed(expected))) => {
                    assert_eq!(p.rate_hz, *expected, "{name}");
                    assert_eq!(
                        p.width,
                        crate::PulseWidth::Duration {
                            seconds: crate::test_support::finite(0.001)
                        }
                    );
                    assert!(matches!(p.shape, crate::PulseShape::Rectangular {}));
                }
                actual => panic!("{name}: {actual:?}"),
            }
            match (&component.carrier, *kind) {
                (crate::CarrierFacts::Tone(rate), "tone") => {
                    assert_eq!(rate.requested_hz, *hz, "{name}");
                }
                (crate::CarrierFacts::PulseTrain(pulse), "clicks") => {
                    assert_eq!(pulse.rate_hz, *hz, "{name}");
                    assert_eq!(pulse.requested_width_seconds, 0.001, "{name}");
                    assert!(matches!(pulse.shape, crate::PulseShape::Rectangular {}));
                }
                _ => panic!("{name}: expected a {kind}"),
            }
        }
        assert_eq!(
            sound.uses_separate_channels(),
            parts.iter().any(|p| p.0 != B),
            "{name}"
        );
    }
}

#[test]
fn every_entry_validates_and_pulses_follow_integer_onsets() {
    for entry in catalog::ENTRIES {
        for hz in [44_100, 48_000, 96_000] {
            let authored = crate::FiniteFixture::from_recipe(&recipe(entry));
            let mut sound = authored;
            let signal = sound.signal(SampleRate::new(hz).unwrap()).unwrap();
            assert_eq!(signal.duration_seconds(), sound.duration_seconds().get());
            sound.set_fade(crate::test_support::finite(0.0));
            let signal = sound.signal(SampleRate::new(hz).unwrap()).unwrap();
            if let [part] = sound.components() {
                let form = part.form;
                let pulse_rate = match form.source {
                    crate::SoundSource::PulseTrain { pulse, .. } => Some(pulse.rate_hz.get()),
                    crate::SoundSource::Tone { .. } => match form.modulation {
                        Some(Modulation::Pulsed { pulse }) => Some(pulse.rate_hz.get()),
                        _ => None,
                    },
                    crate::SoundSource::BinauralPair { .. }
                    | crate::SoundSource::MonauralPair { .. } => None,
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
                            } else if matches!(form.source, crate::SoundSource::PulseTrain { .. }) {
                                assert!(value > 0.0);
                            }
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn builtins_have_unique_curated_identities_and_do_not_invent_creation_dates() {
    let mut identities = std::collections::BTreeSet::new();
    for entry in catalog::ENTRIES {
        let recipe = recipe(entry);
        assert!(
            identities.insert(recipe.document().id),
            "{} duplicates a builtin UUID",
            entry.id
        );
        assert_eq!(recipe.document().created_at, None);
        assert_eq!(recipe.document().modified_at, None);
    }
    assert_eq!(identities.len(), catalog::ENTRIES.len());
}

#[test]
fn catalog_identity_validates_grammar_without_claiming_resolution() {
    for invalid in ["", "UPPER", "a_b", "a/b", "builtin:a", "é", " a"] {
        assert!(super::CatalogId::new(invalid).is_none(), "{invalid:?}");
    }
    for valid in ["a", "0", "a-9", "unknown-recipe"] {
        let id = super::CatalogId::new(valid).unwrap();
        assert_eq!(id.as_str(), valid);
    }
    assert!(super::find("unknown-recipe").is_none());
}

#[test]
fn studies_match_directory_contents_and_curated_ids_are_unique() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let names = |directory: &str| {
        std::fs::read_dir(root.join(directory))
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                assert_eq!(path.extension().and_then(|x| x.to_str()), Some("toml"));
                path.file_stem().unwrap().to_str().unwrap().to_owned()
            })
            .collect::<std::collections::BTreeSet<_>>()
    };
    let ids = catalog::ENTRIES
        .iter()
        .map(|entry| entry.id.to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), catalog::ENTRIES.len());
    assert_eq!(ids, names("catalog"));
    let studies = super::STUDIES
        .iter()
        .map(|(id, text)| {
            assert!(super::CatalogId::new(id).is_some());
            assert_eq!(
                *text,
                std::fs::read_to_string(root.join("studies").join(format!("{id}.toml"))).unwrap()
            );
            (*id).to_owned()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(studies.len(), super::STUDIES.len());
    assert_eq!(studies, names("studies"));
}

#[test]
fn merging_refuses_duplicate_citations_and_preserves_specific_limitations() {
    let entry = &catalog::ENTRIES[0];
    let read = |key: &str| -> Result<String, std::boxed::Box<dyn core::error::Error>> {
        Ok(super::STUDIES
            .iter()
            .find(|(id, _)| *id == key)
            .unwrap()
            .1
            .into())
    };
    let mut condition: toml::Table = toml::from_str(entry.condition).unwrap();
    condition
        .get_mut("evidence")
        .unwrap()
        .as_array_mut()
        .unwrap()[0]
        .as_table_mut()
        .unwrap()
        .insert(
            "limitations".into(),
            toml::Value::String("Specific limit".into()),
        );
    let merged = super::generated_recipe(&toml::to_string(&condition).unwrap(), read).unwrap();
    assert_eq!(
        Recipe::parse(&merged).unwrap().evidence()[0].limitations,
        "Specific limit"
    );
    condition.get_mut("source").unwrap().as_array_mut().unwrap()[0]
        .as_table_mut()
        .unwrap()
        .insert(
            "citation".into(),
            toml::Value::String("Duplicate citation".into()),
        );
    assert!(super::generated_recipe(&toml::to_string(&condition).unwrap(), read).is_err());
}
