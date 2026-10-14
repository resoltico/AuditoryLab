// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::{Preset, PresetError};
use crate::{Component, Ears, Modulation};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;

const PAIR: &str = r#"# Two tones.
schema_version = 1
duration_seconds = 10.0
fade_seconds = 0.1

[[component]]
ears = "left"
kind = "tone"
frequency_hz = 250.0
level_dbfs = -20.0

[[component]]
ears = "right"
kind = "tone"
frequency_hz = 256.0
level_dbfs = -20.0
"#;

const PROVENANCE: &str = "\n[provenance]\ntitle = \"t\"\ncitation = \"c\"\nsource_url = \"u\"\ndescription = \"d\"\nadaptation = \"a\"\n";

fn edited(from: &str, to: &str) -> Result<Preset, PresetError> {
    assert!(PAIR.contains(from), "fixture lacks {from}");
    Preset::parse(&PAIR.replacen(from, to, 1))
}

#[test]
fn a_preset_reads_into_its_stimulus_in_file_order() {
    let preset = Preset::parse(PAIR).unwrap();
    assert_eq!(preset.provenance, None);
    assert_eq!(preset.stimulus.duration_seconds, 10.0);
    assert_eq!(preset.stimulus.fade_seconds, 0.1);
    assert_eq!(
        preset.stimulus.components,
        vec![
            Component::Tone {
                ears: Ears::Left,
                frequency_hz: 250.0,
                level_dbfs: -20.0,
                modulation: None
            },
            Component::Tone {
                ears: Ears::Right,
                frequency_hz: 256.0,
                level_dbfs: -20.0,
                modulation: None
            },
        ]
    );
}

#[test]
fn every_kind_and_modulation_reads_and_whole_numbers_are_accepted_as_reals() {
    let text = "schema_version = 1\nduration_seconds = 3\nfade_seconds = 0\n\n[[component]]\nears = \"both\"\nkind = \"click\"\nrate_hz = 40\nwidth_seconds = 0.001\nlevel_dbfs = -20\n\n[component.modulation]\nkind = \"sinusoidal\"\nrate_hz = 2\ndepth = 1\n";
    let stimulus = Preset::parse(text).unwrap().stimulus;
    assert_eq!(stimulus.duration_seconds, 3.0);
    assert_eq!(
        stimulus.components,
        vec![Component::Click {
            ears: Ears::Both,
            rate_hz: 40.0,
            width_seconds: 0.001,
            level_dbfs: -20.0,
            modulation: Some(Modulation::Sinusoidal {
                rate_hz: 2.0,
                depth: 1.0
            })
        }]
    );
}

#[test]
fn a_preset_without_components_is_silence() {
    let preset =
        Preset::parse("schema_version = 1\nduration_seconds = 1.0\nfade_seconds = 0.0\n").unwrap();
    assert!(preset.stimulus.components.is_empty());
    assert!(!preset.stimulus.uses_separate_ear_routing());
}

#[test]
fn provenance_is_optional_and_strict_and_never_changes_the_stimulus() {
    let plain = Preset::parse(PAIR).unwrap();
    let annotated = Preset::parse(&format!("{PAIR}{PROVENANCE}")).unwrap();
    assert_eq!(annotated.stimulus, plain.stimulus);
    let context = annotated.provenance.unwrap();
    assert_eq!(
        [
            context.title,
            context.citation,
            context.source_url,
            context.description,
            context.adaptation
        ],
        ["t", "c", "u", "d", "a"].map(String::from)
    );
    for (field, value) in [
        ("title", "t"),
        ("citation", "c"),
        ("source_url", "u"),
        ("description", "d"),
        ("adaptation", "a"),
    ] {
        let entry = format!("{field} = \"{value}\"\n");
        assert!(PROVENANCE.contains(&entry), "{entry}");
        for altered in [
            PROVENANCE.replace(&entry, ""),
            PROVENANCE.replace(&entry, &format!("{field} = 1\n")),
            PROVENANCE.replace(&entry, &format!("{entry}{entry}")),
        ] {
            assert!(
                matches!(
                    Preset::parse(&format!("{PAIR}{altered}")),
                    Err(PresetError::Format(_))
                ),
                "{altered}"
            );
        }
    }
    let extra = PROVENANCE.replace("[provenance]", "[provenance]\nreviewed = \"x\"");
    assert!(Preset::parse(&format!("{PAIR}{extra}")).is_err());
}

#[test]
fn the_format_rejects_missing_unknown_duplicated_and_mistyped_fields() {
    for (from, to) in [
        ("schema_version = 1\n", ""),
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
        ("level_dbfs = -20.0\n", ""),
        (
            "level_dbfs = -20.0",
            "level_dbfs = -20.0\nlevel_dbfs = -10.0",
        ),
        ("level_dbfs = -20.0", "gain = 0.1"),
        ("[[component]]", "[[components]]"),
        ("[[component]]", "[component]"),
        (
            "[[component]]",
            "[signal]\nkind = \"tone\"\n\n[[component]]",
        ),
    ] {
        assert!(
            matches!(edited(from, to), Err(PresetError::Format(_))),
            "accepted {to:?}"
        );
    }
    let modulated = format!(
        "{PAIR}\n[component.modulation]\nkind = \"sinusoidal\"\nrate_hz = 4.0\ndepth = 0.5\n"
    );
    assert!(Preset::parse(&modulated).is_ok());
    for (from, to) in [
        ("kind = \"sinusoidal\"\n", ""),
        ("kind = \"sinusoidal\"", "kind = \"square\""),
        ("depth = 0.5\n", ""),
        ("depth = 0.5", "depth = 0.5\nphase = 0.0"),
        ("rate_hz = 4.0\n", ""),
    ] {
        assert!(
            matches!(
                Preset::parse(&modulated.replacen(from, to, 1)),
                Err(PresetError::Format(_))
            ),
            "accepted {to:?}"
        );
    }
}

#[test]
fn another_schema_version_is_reported_before_its_fields_are_read() {
    for version in [0, 2, 999, u32::MAX] {
        // A later version may add fields this build does not know.
        let text = PAIR
            .replace("schema_version = 1", &format!("schema_version = {version}"))
            .replace(
                "level_dbfs = -20.0",
                "level_dbfs = -20.0\nphase_degrees = 90.0",
            );
        let error = Preset::parse(&text).unwrap_err();
        assert_eq!(error, PresetError::UnsupportedVersion { found: version });
        assert_eq!(
            error.to_string(),
            format!("preset has schema_version {version}; this build reads 1")
        );
    }
    assert!(matches!(
        edited("schema_version = 1", "schema_version = \"1\""),
        Err(PresetError::Format(_))
    ));
}

#[test]
fn input_is_bounded_and_must_be_utf_8() {
    let padded = format!("{PAIR}{}", " ".repeat(Preset::MAX_BYTES - PAIR.len()));
    assert_eq!(padded.len(), 65_536);
    assert!(Preset::parse(&padded).is_ok());
    assert!(Preset::from_bytes(padded.as_bytes()).is_ok());
    let oversized = padded + " ";
    assert_eq!(Preset::parse(&oversized), Err(PresetError::TooLarge));
    assert_eq!(
        Preset::from_bytes(oversized.as_bytes()),
        Err(PresetError::TooLarge)
    );
    assert_eq!(Preset::from_bytes(&[0xff]), Err(PresetError::NotUtf8));
}

#[test]
fn messages_say_what_is_wrong_with_the_preset() {
    use core::error::Error;
    assert_eq!(
        PresetError::TooLarge.to_string(),
        "preset exceeds 65536 bytes"
    );
    assert_eq!(PresetError::NotUtf8.to_string(), "preset is not UTF-8 text");
    let format = edited("frequency_hz = 250.0\n", "").unwrap_err();
    let text = format.to_string();
    assert!(
        text.starts_with("preset does not match the format: "),
        "{text}"
    );
    assert!(text.contains("frequency_hz"), "{text}");
    assert!(format.source().is_some());
    assert!(PresetError::TooLarge.source().is_none());
}
