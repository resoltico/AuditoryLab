// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The exact samples of every kind of component, pinned so that any
//! platform, compiler or code change that alters one bit is noticed.

#![cfg(test)]

use auditory_core::{Preset, SampleRate};

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
    let text =
        format!("schema_version = 2\nduration_seconds = 2.0\nfade_seconds = 0.1\n{components}");
    let session = Preset::parse(&text)
        .unwrap()
        .stimulus
        .session(SampleRate::new(48_000).unwrap())
        .unwrap();
    let mut renderer = session.into_renderer();
    let mut frames = vec![[0.0; 2]; 96_000];
    let mut offset = 0;
    for count in [1, 4_095, 37, 60_000].into_iter().cycle() {
        if renderer.is_complete() {
            break;
        }
        let end = frames.len().min(offset + count);
        offset += renderer.render(&mut frames[offset..end]).frames();
    }
    assert_eq!(offset, frames.len());
    frames
}

const CASES: [(&str, &str); 5] = [
    (
        "separate-ear tones",
        "[[component]]\nears = \"left\"\nkind = \"tone\"\nfrequency_hz = 250.0\npeak_dbfs = -20.0\n[[component]]\nears = \"right\"\nkind = \"tone\"\nfrequency_hz = 256.0\npeak_dbfs = -20.0\n",
    ),
    (
        "summed tones",
        "[[component]]\nears = \"both\"\nkind = \"tone\"\nfrequency_hz = 396.5\npeak_dbfs = -23.0103\n[[component]]\nears = \"both\"\nkind = \"tone\"\nfrequency_hz = 403.5\npeak_dbfs = -23.0103\n",
    ),
    (
        "modulated tone",
        "[[component]]\nears = \"left\"\nkind = \"tone\"\nfrequency_hz = 1000.0\npeak_dbfs = -20.0\n[component.modulation]\nkind = \"sinusoidal\"\nrate_hz = 39.0625\ndepth = 1.0\n",
    ),
    (
        "gated tone",
        "[[component]]\nears = \"both\"\nkind = \"tone\"\nfrequency_hz = 5000.0\npeak_dbfs = -20.0\n[component.modulation]\nkind = \"rectangular\"\nrate_hz = 40.0\nwidth_seconds = 0.001\n",
    ),
    (
        "clicks",
        "[[component]]\nears = \"both\"\nkind = \"click\"\nrate_hz = 40.0\nwidth_seconds = 0.001\npeak_dbfs = -20.0\n",
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
    "separate-ear tones 0x7c2aaab8a485d20e",
    "summed tones 0x37d1ab3d41ad37cd",
    "modulated tone 0xe28a35e32705b6c0",
    "gated tone 0xe6101643f88ab879",
    "clicks 0x6f538dee5ad69699",
];

#[test]
fn the_digest_tells_apart_samples_that_differ_in_their_last_bit() {
    let frames: [[f64; 2]; 2] = [[0.0, 0.25], [-0.5, 1.0]];
    let mut altered = frames;
    altered[0][1] = f64::from_bits(altered[0][1].to_bits() + 1);
    assert_ne!(digest(&frames), digest(&altered));
    assert_eq!(digest(&[]), 0xcbf2_9ce4_8422_2325);
}
