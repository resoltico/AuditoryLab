// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::{Recipe, RecipeError};
use crate::{Component, Ears, Modulation};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

const PAIR: &str = r#"# Two tones.
schema_version = 2
duration_seconds = 10.0
fade_seconds = 0.1

[[component]]
ears = "left"
kind = "tone"
frequency_hz = 250.0
peak_dbfs = -20.0

[[component]]
ears = "right"
kind = "tone"
frequency_hz = 256.0
peak_dbfs = -20.0
"#;

const ABOUT: &str = "\n[about]\ntitle = \"t\"\nstudied_for = \"f\"\nsummary = \"s\"\ncitation = \"c\"\nsource_url = \"u\"\nstudy = \"d\"\nadaptation = \"a\"\n";

fn edited(from: &str, to: &str) -> Result<Recipe, RecipeError> {
    assert!(PAIR.contains(from), "fixture lacks {from}");
    Recipe::parse(&PAIR.replacen(from, to, 1))
}

#[test]
fn a_preset_reads_into_its_stimulus_in_file_order() {
    let recipe = Recipe::parse(PAIR).unwrap();
    assert_eq!(recipe.about, None);
    assert_eq!(recipe.stimulus.duration_seconds, 10.0);
    assert_eq!(recipe.stimulus.fade_seconds, 0.1);
    assert_eq!(
        recipe.stimulus.components,
        vec![
            Component {
                ears: Ears::Left,
                peak_dbfs: -20.0,
                modulation: None,
                carrier: crate::Carrier::Tone {
                    frequency_hz: 250.0
                }
            },
            Component {
                ears: Ears::Right,
                peak_dbfs: -20.0,
                modulation: None,
                carrier: crate::Carrier::Tone {
                    frequency_hz: 256.0
                }
            },
        ]
    );
}

#[test]
fn every_kind_and_modulation_reads_and_whole_numbers_are_accepted_as_reals() {
    let text = "schema_version = 2\nduration_seconds = 3\nfade_seconds = 0\n\n[[component]]\nears = \"both\"\nkind = \"click\"\nrate_hz = 40\nwidth_seconds = 0.001\npeak_dbfs = -20\n\n[component.modulation]\nkind = \"sinusoidal\"\nrate_hz = 2\ndepth = 1\n";
    let stimulus = Recipe::parse(text).unwrap().stimulus;
    assert_eq!(stimulus.duration_seconds, 3.0);
    assert_eq!(
        stimulus.components,
        vec![Component {
            ears: Ears::Both,
            peak_dbfs: -20.0,
            modulation: Some(Modulation::Sinusoidal {
                rate_hz: 2.0,
                depth: 1.0
            }),
            carrier: crate::Carrier::Click {
                rate_hz: 40.0,
                width_seconds: 0.001
            }
        }]
    );
}

#[test]
fn a_preset_without_components_is_silence() {
    let recipe =
        Recipe::parse("schema_version = 2\nduration_seconds = 1.0\nfade_seconds = 0.0\n").unwrap();
    assert!(recipe.stimulus.components.is_empty());
    assert!(!recipe.stimulus.uses_separate_ears());
}

#[test]
fn about_text_is_optional_strict_and_never_changes_the_stimulus() {
    let plain = Recipe::parse(PAIR).unwrap();
    let annotated = Recipe::parse(&format!("{PAIR}{ABOUT}")).unwrap();
    assert_eq!(annotated.stimulus, plain.stimulus);
    let about = annotated.about.unwrap();
    assert_eq!(about.title, "t");
    assert_eq!(
        [
            about.studied_for,
            about.summary,
            about.citation,
            about.source_url,
            about.study,
            about.adaptation
        ],
        ["f", "s", "c", "u", "d", "a"].map(|value| Some(String::from(value)))
    );
    // Only the title is required; every other field may be omitted alone.
    let titled = Recipe::parse(&format!("{PAIR}\n[about]\ntitle = \"t\"\n")).unwrap();
    assert_eq!(titled.about.unwrap().summary, None);
    for (field, value) in [
        ("title", "t"),
        ("studied_for", "f"),
        ("summary", "s"),
        ("citation", "c"),
        ("source_url", "u"),
        ("study", "d"),
        ("adaptation", "a"),
    ] {
        let entry = format!("{field} = \"{value}\"\n");
        assert!(ABOUT.contains(&entry), "{entry}");
        let omitted = Recipe::parse(&format!("{PAIR}{}", ABOUT.replace(&entry, "")));
        assert_eq!(omitted.is_ok(), field != "title", "{field}");
        for altered in [
            ABOUT.replace(&entry, &format!("{field} = 1\n")),
            ABOUT.replace(&entry, &format!("{entry}{entry}")),
        ] {
            assert!(
                matches!(
                    Recipe::parse(&format!("{PAIR}{altered}")),
                    Err(RecipeError::Format(_))
                ),
                "{altered}"
            );
        }
    }
    for unknown in ["reviewed = \"x\"", "description = \"d\""] {
        let extra = ABOUT.replace("[about]", &format!("[about]\n{unknown}"));
        assert!(
            Recipe::parse(&format!("{PAIR}{extra}")).is_err(),
            "{unknown}"
        );
    }
    let old_table = ABOUT.replace("[about]", "[provenance]");
    assert!(Recipe::parse(&format!("{PAIR}{old_table}")).is_err());
}

#[test]
fn the_format_rejects_missing_unknown_duplicated_and_mistyped_fields() {
    for (from, to) in [
        ("schema_version = 2\n", ""),
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
        ("[[component]]", "[[components]]"),
        ("[[component]]", "[component]"),
        (
            "[[component]]",
            "[signal]\nkind = \"tone\"\n\n[[component]]",
        ),
    ] {
        assert!(
            matches!(edited(from, to), Err(RecipeError::Format(_))),
            "accepted {to:?}"
        );
    }
    let modulated = format!(
        "{PAIR}\n[component.modulation]\nkind = \"sinusoidal\"\nrate_hz = 4.0\ndepth = 0.5\n"
    );
    assert!(Recipe::parse(&modulated).is_ok());
    for (from, to) in [
        ("kind = \"sinusoidal\"\n", ""),
        ("kind = \"sinusoidal\"", "kind = \"square\""),
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
    for version in [0, 1, 3, 999, u32::MAX] {
        // A later version may add fields this build does not know.
        let text = PAIR
            .replace("schema_version = 2", &format!("schema_version = {version}"))
            .replace(
                "peak_dbfs = -20.0",
                "peak_dbfs = -20.0\nphase_degrees = 90.0",
            );
        let error = Recipe::parse(&text).unwrap_err();
        assert_eq!(error, RecipeError::UnsupportedVersion { found: version });
        assert_eq!(
            error.to_string(),
            format!("recipe has schema_version {version}; this build reads 2")
        );
    }
    assert!(matches!(
        edited("schema_version = 2", "schema_version = \"1\""),
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
fn messages_say_what_is_wrong_with_the_preset() {
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

mod writing {
    use crate::{About, Carrier, Component, Ears, Modulation, Recipe, Stimulus};
    use alloc::string::String;
    use alloc::vec::Vec;
    use proptest::{prelude::*, test_runner::Config, test_runner::TestRunner};

    #[test]
    fn every_catalog_recipe_reads_back_equal_after_writing() {
        for entry in crate::catalog::ENTRIES {
            let recipe = Recipe::parse(entry.text).unwrap();
            let text = recipe.to_toml().unwrap();
            assert!(text.starts_with("schema_version = 2\n"), "{}", entry.id);
            assert_eq!(Recipe::parse(&text).unwrap(), recipe, "{}", entry.id);
        }
    }

    #[test]
    fn written_presets_read_back_equal() {
        let number = prop_oneof![
            any::<f64>().prop_filter("finite", |x| x.is_finite()),
            Just(5e-324),
            Just(-0.0),
            Just(0.1),
        ];
        let text = prop_oneof![
            Just(String::new()),
            "[ -~]{0,40}",
            Just(String::from(
                "quote \" backslash \\ newline\n tab\t é 🎧 \u{7}"
            )),
        ];
        let modulation = prop_oneof![
            Just(None),
            (number.clone(), number.clone())
                .prop_map(|(rate_hz, depth)| Some(Modulation::Sinusoidal { rate_hz, depth })),
            (number.clone(), number.clone()).prop_map(|(rate_hz, width_seconds)| Some(
                Modulation::Rectangular {
                    rate_hz,
                    width_seconds
                }
            )),
        ];
        let component = (
            0_u8..3,
            any::<bool>(),
            number.clone(),
            number.clone(),
            number.clone(),
            modulation,
        )
            .prop_map(|(ears, click, a, b, peak_dbfs, modulation)| Component {
                ears: [Ears::Left, Ears::Right, Ears::Both][usize::from(ears)],
                peak_dbfs,
                carrier: if click {
                    Carrier::Click {
                        rate_hz: a,
                        width_seconds: b,
                    }
                } else {
                    Carrier::Tone { frequency_hz: a }
                },
                modulation,
            });
        let about = prop::option::of((
            text.clone(),
            prop::option::of(text.clone()),
            prop::option::of(text),
        ))
        .prop_map(|about| {
            about.map(|(title, summary, citation)| About {
                title,
                studied_for: None,
                summary,
                citation,
                source_url: None,
                study: None,
                adaptation: None,
            })
        });
        let recipe = (
            number.clone(),
            number,
            prop::collection::vec(component, 0..5),
            about,
        )
            .prop_map(
                |(duration_seconds, fade_seconds, components, about)| Recipe {
                    stimulus: Stimulus {
                        duration_seconds,
                        fade_seconds,
                        components,
                    },
                    about,
                },
            );
        let config = Config {
            cases: 256,
            failure_persistence: None,
            max_shrink_iters: 256,
            ..Config::default()
        };
        TestRunner::new(config)
            .run(&recipe, |recipe| {
                let text = recipe.to_toml().unwrap();
                let back = Recipe::parse(&text).unwrap();
                // Bitwise equality keeps signed zeros and subnormals honest.
                let bits = |p: &Recipe| -> Vec<u64> {
                    let s = &p.stimulus;
                    let mut out =
                        alloc::vec![s.duration_seconds.to_bits(), s.fade_seconds.to_bits()];
                    for c in &s.components {
                        out.push(c.peak_dbfs.to_bits());
                        match c.carrier {
                            Carrier::Tone { frequency_hz } => out.push(frequency_hz.to_bits()),
                            Carrier::Click {
                                rate_hz,
                                width_seconds,
                            } => {
                                out.extend([rate_hz.to_bits(), width_seconds.to_bits()]);
                            }
                        }
                        match c.modulation {
                            None => {}
                            Some(Modulation::Sinusoidal { rate_hz, depth }) => {
                                out.extend([rate_hz.to_bits(), depth.to_bits()])
                            }
                            Some(Modulation::Rectangular {
                                rate_hz,
                                width_seconds,
                            }) => out.extend([rate_hz.to_bits(), width_seconds.to_bits()]),
                        }
                    }
                    out
                };
                prop_assert_eq!(bits(&back), bits(&recipe));
                prop_assert_eq!(back, recipe);
                Ok(())
            })
            .unwrap();
    }
}
