// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Strict one-version portable contract; independent shape, byte and numeric oracles.
use super::{Recipe, RecipeError};
use crate::{
    Carrier, Component, ComponentForm, ComponentId, Description, DocumentIdentity, Pair, Sound,
    Voice,
};
use alloc::{format, string::ToString, vec, vec::Vec};
const PAIR: &str = r#"# Two raw tones retain their source order and authored identities.
schema_version = 4
[sound]
duration_seconds = 10.0
fade_seconds = 0.1
[document]
id = "00000000-0000-4000-8000-000000000001"
[[sound.component]]
id = 7
ears = "left"
kind = "tone"
frequency_hz = 250.0
peak_dbfs = -20.0
[[sound.component]]
id = 19
ears = "right"
kind = "tone"
frequency_hz = 256.0
peak_dbfs = -20.0
"#;
fn edited(from: &str, to: &str) -> Result<Recipe, RecipeError> {
    assert!(PAIR.contains(from));
    Recipe::parse(&PAIR.replacen(from, to, 1))
}
#[test]
fn the_format_rejects_missing_unknown_duplicated_and_mistyped_fields() {
    for (from, to) in [
        ("schema_version = 4\n", ""),
        ("duration_seconds = 10.0\n", ""),
        ("fade_seconds = 0.1\n", ""),
        ("fade_seconds = 0.1", "fade_seconds = 0.1\ngain = 0.1"),
        (
            "fade_seconds = 0.1",
            "fade_seconds = 0.1\nsample_rate_hz = 48000",
        ),
        (
            "fade_seconds = 0.1",
            "fade_seconds = 0.1\nfade_seconds = 0.2",
        ),
        ("duration_seconds = 10.0", "duration_seconds = \"ten\""),
        ("ears = \"left\"\n", ""),
        ("ears = \"left\"", "ears = \"centre\""),
        ("ears = \"left\"", "ears = \"left\"\nchannel = 1"),
        ("kind = \"tone\"\n", ""),
        ("kind = \"tone\"", "kind = \"noise\""),
        ("frequency_hz = 250.0\n", ""),
        (
            "frequency_hz = 250.0",
            "frequency_hz = 250.0\nrate_hz = 40.0",
        ),
        ("frequency_hz = 250.0", "frequency_hz = 250.0\nphase = 0.0"),
        ("peak_dbfs = -20.0\n", ""),
        ("peak_dbfs = -20.0", "peak_dbfs = -20.0\npeak_dbfs = -10.0"),
        ("peak_dbfs = -20.0", "gain = 0.1"),
        ("[[sound.component]]", "[[components]]"),
        ("[[sound.component]]", "[component]"),
        (
            "[[sound.component]]",
            "[signal]\nkind = \"tone\"\n\n[[sound.component]]",
        ),
    ] {
        assert!(
            matches!(edited(from, to), Err(RecipeError::Format(_))),
            "accepted {to:?}"
        );
    }
    let modulated = format!(
        "{PAIR}\n[sound.component.modulation]\nkind = \"am\"\nrate_hz = 4.0\ndepth = 0.5\n"
    );
    assert!(Recipe::parse(&modulated).is_ok());
    for (from, to) in [
        ("kind = \"am\"\n", ""),
        ("kind = \"am\"", "kind = \"square\""),
        ("depth = 0.5\n", ""),
        ("depth = 0.5", "depth = 0.5\nphase = 0.0"),
        ("rate_hz = 4.0\n", ""),
    ] {
        assert!(
            matches!(
                Recipe::parse(&modulated.replacen(from, to, 1)),
                Err(RecipeError::Format(_))
            ),
            "accepted {to:?}"
        );
    }
}

#[test]
fn another_schema_version_is_reported_before_its_fields_are_read() {
    for version in [0, 1, 2, 3, 999, u32::MAX] {
        // A later version may add fields this build does not know.
        let text = PAIR
            .replace("schema_version = 4", &format!("schema_version = {version}"))
            .replace(
                "peak_dbfs = -20.0",
                "peak_dbfs = -20.0\nphase_degrees = 90.0",
            );
        let error = Recipe::parse(&text).unwrap_err();
        assert_eq!(error, RecipeError::UnsupportedVersion { found: version });
        assert_eq!(
            error.to_string(),
            format!("recipe has schema_version {version}; this build reads 4")
        );
    }
    assert!(matches!(
        edited("schema_version = 4", "schema_version = \"1\""),
        Err(RecipeError::Format(_))
    ));
}

#[test]
fn input_is_bounded_and_must_be_utf_8() {
    let padded = format!("{PAIR}{}", " ".repeat(Recipe::MAX_BYTES - PAIR.len()));
    assert_eq!(padded.len(), 65_536);
    assert!(Recipe::parse(&padded).is_ok());
    assert!(Recipe::from_bytes(padded.as_bytes()).is_ok());
    let oversized = padded + " ";
    assert_eq!(Recipe::parse(&oversized), Err(RecipeError::TooLarge));
    assert_eq!(
        Recipe::from_bytes(oversized.as_bytes()),
        Err(RecipeError::TooLarge)
    );
    assert_eq!(Recipe::from_bytes(&[0xff]), Err(RecipeError::NotUtf8));
    // Windows tools commonly save UTF-16 with a byte-order mark; say so.
    let utf16: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain(PAIR.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    assert_eq!(Recipe::from_bytes(&utf16), Err(RecipeError::Utf16));
    let big_endian: Vec<u8> = [0xfe, 0xff]
        .into_iter()
        .chain(PAIR.encode_utf16().flat_map(u16::to_be_bytes))
        .collect();
    assert_eq!(Recipe::from_bytes(&big_endian), Err(RecipeError::Utf16));
    // A UTF-8 byte-order mark is accepted.
    let marked = [&[0xef, 0xbb, 0xbf][..], PAIR.as_bytes()].concat();
    assert!(Recipe::from_bytes(&marked).is_ok());
}

#[test]
fn messages_say_what_is_wrong_with_the_recipe() {
    use core::error::Error;
    assert_eq!(
        RecipeError::TooLarge.to_string(),
        "recipe exceeds 65536 bytes"
    );
    assert_eq!(RecipeError::NotUtf8.to_string(), "recipe is not UTF-8 text");
    assert_eq!(
        RecipeError::Utf16.to_string(),
        "recipe is UTF-16 text; save it as UTF-8"
    );
    let format = edited("frequency_hz = 250.0\n", "").unwrap_err();
    let text = format.to_string();
    assert!(
        text.starts_with("recipe does not match the format: "),
        "{text}"
    );
    assert!(text.contains("frequency_hz"), "{text}");
    assert!(format.source().is_some());
    assert!(RecipeError::TooLarge.source().is_none());
}

#[test]
fn raw_nodes_keep_ids_source_order_and_optional_description_is_not_audio() {
    let plain = Recipe::parse(PAIR).unwrap();
    assert_eq!(
        plain
            .sound
            .components
            .iter()
            .map(|c| c.id.get())
            .collect::<Vec<_>>(),
        [7, 19]
    );
    assert_eq!(plain.description, None);
    let with = Recipe::parse(&format!(
        "{PAIR}\n[description]\ntitle='Rīga'\nnotes='own notes'\n"
    ))
    .unwrap();
    assert_eq!(plain.sound, with.sound);
    for extra in [
        "[description]\nnotes='missing title'",
        "[description]\ntitle=''",
        "[description]\ntitle='t'\nverified=true",
        "[about]\ntitle='old shape'",
    ] {
        assert!(Recipe::parse(&format!("{PAIR}\n{extra}\n")).is_err());
    }
    for (from, to) in [
        ("id = 19", "id = 7"),
        ("id = 7", "id = 0"),
        ("id = 7\n", ""),
        ("frequency_hz = 250.0", "frequency_hz = nan"),
    ] {
        assert!(edited(from, to).is_err());
    }
    let finite = edited("frequency_hz = 250.0", "frequency_hz = -250.0").unwrap();
    assert!(
        finite.to_toml().is_ok(),
        "finite acoustically invalid content remains serializable"
    );
    assert!(finite.sound.signal(crate::SampleRate::DEFAULT).is_err());
}
#[test]
fn all_owned_proposed_examples_roundtrip_and_all_rejection_examples_fail() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut counts = [0, 0];
    for (i, directory) in ["recipes", "invalid"].into_iter().enumerate() {
        for entry in std::fs::read_dir(root.join(directory)).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            counts[i] += 1;
            if i == 0 {
                let recipe = Recipe::parse(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"));
                let reopened = Recipe::parse(&recipe.to_toml().unwrap()).unwrap();
                assert_eq!(recipe, reopened);
            } else {
                assert!(Recipe::parse(&text).is_err(), "{path:?}");
            }
        }
    }
    assert_eq!(
        counts,
        [16, 10],
        "test fixture discovery must not be empty or silently omit files"
    );
}
#[test]
fn copies_keep_scoped_sources_and_refuse_capacity_without_dropping_ancestry() {
    let original = Recipe::parse(include_str!(
        "../../tests/fixtures/recipes/custom-derived-pair.toml"
    ))
    .unwrap();
    let document = DocumentIdentity {
        id: "00000000-0000-4000-8000-000000000099".parse().unwrap(),
        created_at: None,
        modified_at: None,
    };
    let copy = original.fork(document.clone(), None).unwrap();
    assert_eq!(copy.sources, original.sources);
    assert!(
        copy.evidence
            .iter()
            .all(|e| e.scope == crate::EvidenceScope::Ancestor)
    );
    assert_eq!(copy.lineage.len(), original.lineage.len() + 1);
    assert_ne!(copy.document.id, original.document.id);
    let mut full = original;
    full.lineage.resize(16, full.lineage[0].clone());
    assert!(full.fork(document, None).is_err());
    assert_eq!(full.lineage.len(), 16);
}
#[test]
fn bibliography_graph_has_strict_scopes_unique_keys_and_no_recursive_sources() {
    let valid = include_str!("../../tests/fixtures/recipes/custom-derived-pair.toml");
    for invalid in [
        valid.replace("scope = \"ancestor\"", "scope = \"verified\""),
        valid.replace("source = \"ioannou-2015\"", "source = \"missing\""),
        format!(
            "{valid}\n[[source]]\nkey='ioannou-2015'\nlabel='duplicate'\ncitation='c'\ndoi='10.1/x'\n"
        ),
        format!("{valid}\n[source.source]\npath='/private/path'\n"),
    ] {
        assert!(Recipe::parse(&invalid).is_err());
    }
}
#[test]
fn every_catalog_recipe_reads_back_equal_after_writing() {
    for entry in crate::catalog::ENTRIES {
        let recipe = Recipe::parse(entry.text).unwrap();
        let text = recipe.to_toml().unwrap();
        assert!(text.starts_with("schema_version = 4\n"));
        assert_eq!(Recipe::parse(&text).unwrap(), recipe, "{}", entry.id);
    }
}
#[test]
fn bounded_property_roundtrips_preserve_all_finite_bits_and_authored_shapes() {
    use proptest::{
        prelude::*,
        test_runner::{Config, TestRunner},
    };
    let finite = prop_oneof![
        any::<f64>().prop_filter("finite", |v| v.is_finite()),
        Just(f64::from_bits(1)),
        Just(-0.0),
        Just(0.1)
    ];
    let generated = (
        finite.clone(),
        finite.clone(),
        finite.clone(),
        finite.clone(),
        finite.clone(),
        finite,
        any::<bool>(),
        "[a-zA-Z0-9 é🎧]{1,40}",
    );
    let config = Config {
        cases: 256,
        max_shrink_iters: 256,
        failure_persistence: None,
        ..Config::default()
    };
    TestRunner::new(config)
        .run(
            &generated,
            |(duration, fade, base, offset, left, right, compound, title)| {
                let document = DocumentIdentity {
                    id: "00000000-0000-4000-8000-000000000001".parse().unwrap(),
                    created_at: None,
                    modified_at: None,
                };
                let mut recipe = Recipe::builder(document);
                recipe.description = Some(Description {
                    title: format!("Title {title}"),
                    notes: Some("quote \" slash \\ newline\n é 🎧".into()),
                });
                recipe.sound = Sound {
                    duration_seconds: duration,
                    fade_seconds: fade,
                    components: vec![Component {
                        id: ComponentId::new(91).unwrap(),
                        form: if compound {
                            ComponentForm::BinauralPair(Pair {
                                first_hz: base,
                                offset_hz: offset,
                                first_peak_dbfs: left,
                                second_peak_dbfs: right,
                            })
                        } else {
                            ComponentForm::Voice(Voice {
                                ears: crate::Ears::Both,
                                peak_dbfs: left,
                                carrier: Carrier::Clicks {
                                    rate_hz: base,
                                    width_seconds: offset,
                                },
                                modulation: Some(crate::Modulation::Am {
                                    rate_hz: right,
                                    depth: 0.0,
                                }),
                            })
                        },
                    }],
                };
                let recipe = recipe.build().unwrap();
                let back = Recipe::parse(&recipe.to_toml().unwrap()).unwrap();
                let bits = |recipe: &Recipe| {
                    let s = &recipe.sound;
                    let mut values = vec![s.duration_seconds.to_bits(), s.fade_seconds.to_bits()];
                    match s.components[0].form {
                        ComponentForm::BinauralPair(p) => values.extend(
                            [
                                p.first_hz,
                                p.offset_hz,
                                p.first_peak_dbfs,
                                p.second_peak_dbfs,
                            ]
                            .map(f64::to_bits),
                        ),
                        ComponentForm::Voice(c) => {
                            values.push(c.peak_dbfs.to_bits());
                            if let Carrier::Clicks {
                                rate_hz,
                                width_seconds,
                            } = c.carrier
                            {
                                values.extend([rate_hz.to_bits(), width_seconds.to_bits()]);
                            }
                            if let Some(crate::Modulation::Am { rate_hz, depth }) = c.modulation {
                                values.extend([rate_hz.to_bits(), depth.to_bits()]);
                            }
                        }
                        _ => panic!("fixture has exactly two alternatives"),
                    };
                    values
                };
                prop_assert_eq!(bits(&recipe), bits(&back));
                prop_assert_eq!(recipe, back);
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn documented_toml_examples_are_actual_supported_inputs() {
    let guide = include_str!("../../../../docs/recipes.md");
    let mut count = 0;
    for block in guide.split("```toml").skip(1) {
        let text = block.split("```").next().unwrap();
        let recipe = Recipe::parse(text).unwrap();
        recipe.sound.signal(crate::SampleRate::DEFAULT).unwrap();
        count += 1;
    }
    assert_eq!(
        count, 3,
        "owned example discovery is not empty or incomplete"
    );
}

#[test]
fn metadata_capacity_checks_are_independent_and_never_drop_records() {
    let original = Recipe::parse(include_str!(
        "../../tests/fixtures/recipes/custom-derived-pair.toml"
    ))
    .unwrap();
    let mut sources = original.clone().into_builder();
    sources.sources = (0..16)
        .map(|i| {
            let mut source = original.sources[0].clone();
            source.key = format!("source-{i}");
            source
        })
        .collect();
    sources.evidence[0].source = "source-0".into();
    sources.lineage[0].source_keys = vec!["source-0".into()];
    assert!(
        sources
            .clone()
            .build()
            .and_then(|recipe| recipe.to_toml())
            .is_ok()
    );
    let mut extra = original.sources[0].clone();
    extra.key = "source-16".into();
    sources.sources.push(extra);
    assert!(
        sources
            .clone()
            .build()
            .and_then(|recipe| recipe.to_toml())
            .is_err()
    );
    assert_eq!(sources.sources.len(), 17);
    let mut evidence = original.clone().into_builder();
    evidence.evidence = vec![original.evidence[0].clone(); 32];
    assert!(
        evidence
            .clone()
            .build()
            .and_then(|recipe| recipe.to_toml())
            .is_ok()
    );
    evidence.evidence.push(original.evidence[0].clone());
    assert!(
        evidence
            .clone()
            .build()
            .and_then(|recipe| recipe.to_toml())
            .is_err()
    );
    assert_eq!(evidence.evidence.len(), 33);
    let mut lineage = original.clone().into_builder();
    lineage.lineage = vec![original.lineage[0].clone(); 16];
    assert!(
        lineage
            .clone()
            .build()
            .and_then(|recipe| recipe.to_toml())
            .is_ok()
    );
    lineage.lineage.push(original.lineage[0].clone());
    assert!(
        lineage
            .clone()
            .build()
            .and_then(|recipe| recipe.to_toml())
            .is_err()
    );
    assert_eq!(lineage.lineage.len(), 17);
}

#[test]
fn sound_is_required_for_silence_and_root_envelopes_are_rejected() {
    let silent = "schema_version = 4\n[document]\nid='00000000-0000-4000-8000-000000000001'\n";
    assert!(matches!(Recipe::parse(silent), Err(RecipeError::Format(_))));
    let valid = format!("{silent}[sound]\nduration_seconds=1\nfade_seconds=0\n");
    assert!(Recipe::parse(&valid).unwrap().sound.components.is_empty());
    let root_envelope = "schema_version = 4\nduration_seconds=1\nfade_seconds=0\n[document]\nid='00000000-0000-4000-8000-000000000001'\n[sound]\nduration_seconds=1\nfade_seconds=0\n";
    assert!(matches!(
        Recipe::parse(root_envelope),
        Err(RecipeError::Format(_))
    ));
}

#[test]
fn canonical_tables_follow_document_description_sound_and_attribution_order() {
    let recipe = Recipe::parse(include_str!(
        "../../tests/fixtures/recipes/custom-derived-pair.toml"
    ))
    .unwrap();
    let text = recipe.to_toml().unwrap();
    let tables: Vec<_> = text.lines().filter(|line| line.starts_with('[')).collect();
    assert_eq!(
        tables,
        [
            "[document]",
            "[description]",
            "[sound]",
            "[[sound.component]]",
            "[[source]]",
            "[[evidence]]",
            "[[lineage]]"
        ]
    );
}

#[test]
fn the_builder_separates_portable_contracts_from_acoustic_validity() {
    let original = Recipe::parse(PAIR).unwrap();
    let mut builder = original.clone().into_builder();
    builder.sound.duration_seconds = -1.0;
    let finite = builder.clone().build().unwrap();
    assert!(finite.to_toml().is_ok());
    assert!(finite.sound().signal(crate::SampleRate::DEFAULT).is_err());
    builder.sound.duration_seconds = f64::NAN;
    assert!(matches!(builder.build(), Err(RecipeError::Contract(_))));
    let mut builder = original.clone().into_builder();
    builder.description = Some(Description {
        title: " ".into(),
        notes: None,
    });
    assert!(matches!(builder.build(), Err(RecipeError::Contract(_))));
    let mut builder = original.into_builder();
    builder.sound.components.push(builder.sound.components[0]);
    assert!(matches!(builder.build(), Err(RecipeError::Contract(_))));
}
