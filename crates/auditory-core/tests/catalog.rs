// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The shipped catalog, checked against recipes written out independently
//! of the files and against the files on disk.

#![cfg(test)]

use auditory_core::catalog::{self, Entry};
use auditory_core::{Component, Ears, Modulation, Preset, SampleRate};
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};
use std::path::Path;

/// (name, seconds, components as (ears, kind, Hz, level dBFS, modulation Hz)).
type Recipe = (
    &'static str,
    f64,
    &'static [(Ears, &'static str, f64, f64, Option<f64>)],
);

const L: Ears = Ears::Left;
const R: Ears = Ears::Right;
const B: Ears = Ears::Both;
const MONAURAL: f64 = -23.0103;

const RECIPES: &[Recipe] = &[
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
        "beauchene-2016-binaural-5-hz",
        300.0,
        &[
            (L, "tone", 245.0, -20.0, None),
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
        "orozco-perez-2020-binaural-40-hz",
        480.0,
        &[
            (L, "tone", 380.0, -20.0, None),
            (R, "tone", 420.0, -20.0, None),
        ],
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
        "orozco-perez-2020-monaural-40-hz",
        480.0,
        &[
            (B, "tone", 380.0, MONAURAL, None),
            (B, "tone", 420.0, MONAURAL, None),
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
        "van-eeckhoutte-2018-am-1000-hz",
        307.2,
        &[(L, "tone", 1000.0, -20.0, Some(39.0625))],
    ),
    (
        "van-eeckhoutte-2018-am-2000-hz",
        307.2,
        &[(L, "tone", 2000.0, -20.0, Some(39.0625))],
    ),
    (
        "van-eeckhoutte-2018-am-500-hz",
        307.2,
        &[(L, "tone", 500.0, -20.0, Some(39.0625))],
    ),
];

fn preset(entry: &Entry) -> Preset {
    Preset::parse(entry.text).unwrap_or_else(|error| panic!("{}: {error}", entry.name))
}

#[test]
fn the_catalog_is_exactly_the_files_in_the_catalog_directory_in_name_order() {
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
            (name, std::fs::read_to_string(&path).unwrap())
        })
        .collect();
    files.sort();
    let entries: Vec<(String, String)> = catalog::ENTRIES
        .iter()
        .map(|e| (e.name.to_owned(), e.text.to_owned()))
        .collect();
    assert_eq!(entries, files);
    for entry in catalog::ENTRIES {
        assert!(
            entry
                .name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "{}",
            entry.name
        );
        assert_eq!(catalog::find(entry.name), Some(entry));
    }
    assert_eq!(catalog::find("missing"), None);
}

#[test]
fn every_entry_has_its_independently_written_recipe() {
    assert_eq!(
        catalog::ENTRIES.iter().map(|e| e.name).collect::<Vec<_>>(),
        RECIPES.iter().map(|r| r.0).collect::<Vec<_>>()
    );
    for (entry, (name, seconds, parts)) in catalog::ENTRIES.iter().zip(RECIPES) {
        let stimulus = preset(entry).stimulus;
        assert_eq!(stimulus.duration_seconds, *seconds, "{name}");
        assert_eq!(stimulus.fade_seconds, 0.1, "{name}");
        assert_eq!(stimulus.components.len(), parts.len(), "{name}");
        for (component, (ears, kind, hz, level, modulation)) in
            stimulus.components.iter().zip(*parts)
        {
            assert_eq!(component.ears(), *ears, "{name}");
            assert_eq!(component.level_dbfs(), *level, "{name}");
            assert_eq!(
                component.modulation(),
                modulation.map(|rate_hz| Modulation::Sinusoidal {
                    rate_hz,
                    depth: 1.0
                }),
                "{name}"
            );
            match (component, *kind) {
                (Component::Tone { frequency_hz, .. }, "tone") => {
                    assert_eq!(frequency_hz, hz, "{name}")
                }
                (
                    Component::Click {
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
            stimulus.uses_separate_ear_routing(),
            parts.iter().any(|p| p.0 != B),
            "{name}"
        );
    }
}

#[test]
fn every_entry_states_its_provenance_and_one_source_has_one_citation() {
    let mut citations = BTreeMap::new();
    for entry in catalog::ENTRIES {
        let provenance = preset(entry)
            .provenance
            .unwrap_or_else(|| panic!("{} has no provenance", entry.name));
        for (field, value) in [
            ("title", &provenance.title),
            ("citation", &provenance.citation),
            ("source_url", &provenance.source_url),
            ("description", &provenance.description),
            ("adaptation", &provenance.adaptation),
        ] {
            assert!(!value.trim().is_empty(), "{}: empty {field}", entry.name);
            assert!(
                !value.contains('"') && !value.contains('\n'),
                "{}: {field}",
                entry.name
            );
        }
        assert!(
            provenance.source_url.starts_with("https://doi.org/10."),
            "{}",
            entry.name
        );
        let year = entry
            .name
            .split('-')
            .find(|part| part.len() == 4 && part.bytes().all(|b| b.is_ascii_digit()))
            .unwrap();
        assert!(
            provenance.citation.contains(&format!("({year})")),
            "{}",
            entry.name
        );
        let first = citations
            .entry(provenance.source_url.clone())
            .or_insert_with(|| provenance.citation.clone());
        assert_eq!(*first, provenance.citation, "{}", entry.name);
    }
    assert_eq!(citations.len(), 6);
}

#[test]
fn every_entry_renders_at_common_rates_within_its_stated_peak() {
    for entry in catalog::ENTRIES {
        let stimulus = preset(entry).stimulus;
        for hz in [44_100, 48_000, 96_000] {
            let session = stimulus.session(SampleRate::new(hz).unwrap()).unwrap();
            assert_eq!(
                session.duration_seconds(),
                stimulus.duration_seconds,
                "{}",
                entry.name
            );
            assert_eq!(session.fade_frames(), u64::from(hz) / 10, "{}", entry.name);
            let peak = session.peak_dbfs();
            let mut renderer = session.into_renderer();
            let mut frames = vec![[9.0; 2]; usize::try_from(hz).unwrap()];
            renderer.render(&mut frames);
            assert_eq!(frames[0], [0.0, 0.0], "{}", entry.name);
            for (ear, limit) in peak.iter().enumerate() {
                let limit = 10_f64.powf(limit / 20.0);
                assert!(
                    frames.iter().all(|f| f[ear].abs() <= limit),
                    "{}",
                    entry.name
                );
            }
        }
    }
}

#[test]
fn separate_ear_and_summed_conditions_of_one_study_have_equal_plateau_rms() {
    let rms = |name: &str| {
        let session = preset(catalog::find(name).unwrap())
            .stimulus
            .session(SampleRate::new(48_000).unwrap())
            .unwrap();
        let mut renderer = session.into_renderer();
        let mut skip = vec![[0.0; 2]; 48_000];
        renderer.render(&mut skip);
        let mut window = vec![[0.0; 2]; 96_000];
        renderer.render(&mut window);
        [0, 1].map(|ear| (window.iter().map(|f| f[ear] * f[ear]).sum::<f64>() / 96_000.0).sqrt())
    };
    for beat in ["7", "40"] {
        let separate = rms(&format!("orozco-perez-2020-binaural-{beat}-hz"));
        let summed = rms(&format!("orozco-perez-2020-monaural-{beat}-hz"));
        for ear in 0..2 {
            let difference_db = 20.0 * (summed[ear] / separate[ear]).log10();
            assert!(
                difference_db.abs() < 1e-3,
                "{beat} Hz, ear {ear}: {difference_db} dB"
            );
        }
    }
}

#[test]
fn the_amplitude_modulated_entries_match_the_textbook_formula() {
    for carrier in [500.0, 1000.0, 2000.0] {
        let name = format!("van-eeckhoutte-2018-am-{carrier}-hz");
        let session = preset(catalog::find(&name).unwrap())
            .stimulus
            .session(SampleRate::new(48_000).unwrap())
            .unwrap();
        let mut frames = vec![[0.0; 2]; 48_000];
        session.into_renderer().render(&mut frames);
        for (n, [left, right]) in (0_u32..).zip(&frames) {
            let t = f64::from(n) / 48_000.0;
            let fade = if n >= 4_800 {
                1.0
            } else {
                0.5 - 0.5 * (PI * f64::from(n) / 4_800.0).cos()
            };
            let expected =
                0.1 * (TAU * carrier * t).sin() * (1.0 + (TAU * 39.0625 * t).sin()) / 2.0 * fade;
            assert!((left - expected).abs() < 1e-10, "{name} frame {n}");
            assert_eq!(right.to_bits(), 0);
        }
    }
}

#[test]
fn the_click_entry_is_one_millisecond_pulses_forty_times_a_second() {
    let session = preset(catalog::find("chan-2022-clicks-40-hz").unwrap())
        .stimulus
        .session(SampleRate::new(48_000).unwrap())
        .unwrap();
    let mut renderer = session.into_renderer();
    let mut faded = vec![[0.0; 2]; 4_800];
    renderer.render(&mut faded);
    let mut frames = vec![[0.0; 2]; 48_000];
    renderer.render(&mut frames);
    let on: Vec<bool> = frames.iter().map(|f| f[0] != 0.0).collect();
    assert!(
        frames
            .iter()
            .all(|f| f[0] == f[1] && (f[0] == 0.0 || (f[0] - 0.1).abs() < 1e-15))
    );
    let onsets = (1..on.len()).filter(|&i| on[i] && !on[i - 1]).count() + usize::from(on[0]);
    assert_eq!(onsets, 40);
    // 40 clicks of 48 frames each, give or take a frame at each edge.
    assert!((1_840..=2_000).contains(&on.iter().filter(|o| **o).count()));
}
