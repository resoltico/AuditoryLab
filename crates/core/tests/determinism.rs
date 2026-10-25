// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The exact samples of every kind of component, pinned so that any
//! platform, compiler or code change that alters one bit is noticed.

#![cfg(test)]

use auditorylab_core::{Recipe, SampleRate};

/// FNV-1a over the little-endian bytes of every sample.
fn digest(frames: &[[f64; 2]]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in frames
        .iter()
        .flatten()
        .flat_map(|sample| sample.to_le_bytes())
    {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Two faded seconds at 48,000 Hz, rendered in buffers of uneven length.
fn rendered(components: &str) -> Vec<[f64; 2]> {
    let text = format!(
        "schema_version = 3\nduration_seconds = 2.0\nfade_seconds = 0.1\n{components}\n[document]\nid=\"00000000-0000-4000-8000-000000000001\"\n"
    );
    let session = Recipe::parse(&text)
        .unwrap()
        .stimulus
        .signal(SampleRate::new(48_000).unwrap())
        .unwrap();
    let mut frames = vec![[0.0; 2]; 96_000];
    let mut offset = 0;
    for count in [1, 4_095, 37, 60_000].into_iter().cycle() {
        if offset == frames.len() {
            break;
        }
        let end = frames.len().min(offset + count);
        offset += session.render(offset as u64, &mut frames[offset..end]);
    }
    assert_eq!(offset, frames.len());
    frames
}

const CASES: [(&str, &str); 5] = [
    (
        "separate-ear tones",
        "[[component]]\nid=1\nears = \"left\"\nkind = \"tone\"\nfrequency_hz = 250.0\npeak_dbfs = -20.0\n[[component]]\nid=2\nears = \"right\"\nkind = \"tone\"\nfrequency_hz = 256.0\npeak_dbfs = -20.0\n",
    ),
    (
        "summed tones",
        "[[component]]\nid=1\nears = \"both\"\nkind = \"tone\"\nfrequency_hz = 396.5\npeak_dbfs = -23.0103\n[[component]]\nid=2\nears = \"both\"\nkind = \"tone\"\nfrequency_hz = 403.5\npeak_dbfs = -23.0103\n",
    ),
    (
        "modulated tone",
        "[[component]]\nid=1\nears = \"left\"\nkind = \"tone\"\nfrequency_hz = 1000.0\npeak_dbfs = -20.0\n[component.modulation]\nkind = \"sinusoidal\"\nrate_hz = 39.0625\ndepth = 1.0\n",
    ),
    (
        "gated tone",
        "[[component]]\nid=1\nears = \"both\"\nkind = \"tone\"\nfrequency_hz = 5000.0\npeak_dbfs = -20.0\n[component.modulation]\nkind = \"rectangular\"\nrate_hz = 40.0\nwidth_seconds = 0.001\n",
    ),
    (
        "clicks",
        "[[component]]\nid=1\nears = \"both\"\nkind = \"click\"\nrate_hz = 40.0\nwidth_seconds = 0.001\npeak_dbfs = -20.0\n",
    ),
];

#[test]
fn every_kind_of_component_renders_the_same_bits_on_every_platform() {
    let digests: Vec<String> = CASES
        .iter()
        .map(|(name, components)| format!("{name} {:#018x}", digest(&rendered(components))))
        .collect();
    assert_eq!(digests, PINNED);
}

const PINNED: [&str; 5] = [
    "separate-ear tones 0x923b526697b0dfc0",
    "summed tones 0x125945695a8a70c5",
    "modulated tone 0x7163997cb756ff9c",
    "gated tone 0x524d4e9fc6e3c2e5",
    "clicks 0xff6d3c57217ac3dd",
];

#[test]
fn the_digest_tells_apart_samples_that_differ_in_their_last_bit() {
    let frames: [[f64; 2]; 2] = [[0.0, 0.25], [-0.5, 1.0]];
    let mut altered = frames;
    altered[0][1] = f64::from_bits(altered[0][1].to_bits() + 1);
    assert_ne!(digest(&frames), digest(&altered));
    assert_eq!(digest(&[]), 0xcbf2_9ce4_8422_2325);
}
