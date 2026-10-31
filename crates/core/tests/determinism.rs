// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The exact samples of every kind of component, pinned so that any
//! platform, compiler or code change that alters one bit is noticed.

#![cfg(test)]

use auditorylab_core::test_support as common;
use auditorylab_core::{ChannelRouting, ComponentForm, Modulation, SampleRate};

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
fn rendered(components: &[ComponentForm]) -> Vec<[f64; 2]> {
    let session = common::recipe(2.0, 0.1, components)
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

const CASES: [(&str, &[ComponentForm]); 9] = [
    (
        "independent channel tones",
        &[
            ComponentForm {
                source: auditorylab_core::SoundSource::Tone {
                    frequency_hz: finite(250.0),
                    route: ChannelRouting::Left,
                    peak_dbfs: finite(-20.0),
                },
                modulation: None,
            },
            ComponentForm {
                source: auditorylab_core::SoundSource::Tone {
                    frequency_hz: finite(256.0),
                    route: ChannelRouting::Right,
                    peak_dbfs: finite(-20.0),
                },
                modulation: None,
            },
        ],
    ),
    (
        "summed tones",
        &[
            ComponentForm {
                source: auditorylab_core::SoundSource::Tone {
                    frequency_hz: finite(396.5),
                    route: ChannelRouting::Both,
                    peak_dbfs: finite(-23.0103),
                },
                modulation: None,
            },
            ComponentForm {
                source: auditorylab_core::SoundSource::Tone {
                    frequency_hz: finite(403.5),
                    route: ChannelRouting::Both,
                    peak_dbfs: finite(-23.0103),
                },
                modulation: None,
            },
        ],
    ),
    (
        "modulated tone",
        &[ComponentForm {
            source: auditorylab_core::SoundSource::Tone {
                frequency_hz: finite(1000.0),
                route: ChannelRouting::Left,
                peak_dbfs: finite(-20.0),
            },
            modulation: Some(Modulation::Sinusoidal {
                rate_hz: finite(39.0625),
                depth: finite(1.0),
            }),
        }],
    ),
    (
        "gated tone",
        &[ComponentForm {
            source: auditorylab_core::SoundSource::Tone {
                frequency_hz: finite(5000.0),
                route: ChannelRouting::Both,
                peak_dbfs: finite(-20.0),
            },
            modulation: Some(Modulation::Pulsed {
                pulse: auditorylab_core::PulseSpec {
                    rate_hz: finite(40.0),
                    width: auditorylab_core::PulseWidth::Duration {
                        seconds: finite(0.001),
                    },
                    shape: auditorylab_core::PulseShape::Rectangular {},
                },
            }),
        }],
    ),
    (
        "rectangular pulse train",
        &[ComponentForm {
            source: auditorylab_core::SoundSource::PulseTrain {
                route: ChannelRouting::Both,
                peak_dbfs: finite(-20.0),
                pulse: auditorylab_core::PulseSpec {
                    rate_hz: finite(40.0),
                    width: auditorylab_core::PulseWidth::Duration {
                        seconds: finite(0.001),
                    },
                    shape: auditorylab_core::PulseShape::Rectangular {},
                },
            },
            modulation: None,
        }],
    ),
    (
        "shaped pulsed tone",
        &[ComponentForm {
            source: auditorylab_core::SoundSource::Tone {
                frequency_hz: finite(440.0),
                route: ChannelRouting::Both,
                peak_dbfs: finite(-30.0),
            },
            modulation: Some(Modulation::Pulsed {
                pulse: auditorylab_core::PulseSpec {
                    rate_hz: finite(10.0),
                    width: auditorylab_core::PulseWidth::DutyCycle {
                        fraction: finite(0.5),
                    },
                    shape: auditorylab_core::PulseShape::RaisedCosine {
                        rise_seconds: finite(0.002),
                        fall_seconds: finite(0.002),
                    },
                },
            }),
        }],
    ),
    (
        "shared monaural pulse",
        &[ComponentForm {
            source: auditorylab_core::SoundSource::MonauralPair {
                first_hz: finite(400.0),
                offset_hz: finite(6.0),
                first_peak_dbfs: finite(-30.0),
                second_peak_dbfs: finite(-36.0),
                route: ChannelRouting::Both,
            },
            modulation: Some(Modulation::Pulsed {
                pulse: auditorylab_core::PulseSpec {
                    rate_hz: finite(10.0),
                    width: auditorylab_core::PulseWidth::Duration {
                        seconds: finite(0.05),
                    },
                    shape: auditorylab_core::PulseShape::RaisedCosine {
                        rise_seconds: finite(0.002),
                        fall_seconds: finite(0.001),
                    },
                },
            }),
        }],
    ),
    (
        "shaped pulse source AM",
        &[ComponentForm {
            source: auditorylab_core::SoundSource::PulseTrain {
                route: ChannelRouting::Both,
                peak_dbfs: finite(-30.0),
                pulse: auditorylab_core::PulseSpec {
                    rate_hz: finite(40.0),
                    width: auditorylab_core::PulseWidth::DutyCycle {
                        fraction: finite(0.2),
                    },
                    shape: auditorylab_core::PulseShape::RaisedCosine {
                        rise_seconds: finite(0.001),
                        fall_seconds: finite(0.002),
                    },
                },
            },
            modulation: Some(Modulation::Sinusoidal {
                rate_hz: finite(8.0),
                depth: finite(0.75),
            }),
        }],
    ),
    (
        "shared binaural AM",
        &[ComponentForm {
            source: auditorylab_core::SoundSource::BinauralPair {
                first_hz: finite(250.0),
                offset_hz: finite(-10.0),
                first_peak_dbfs: finite(-20.0),
                second_peak_dbfs: finite(-20.0),
            },
            modulation: Some(Modulation::Sinusoidal {
                rate_hz: finite(8.0),
                depth: finite(0.5),
            }),
        }],
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

// Levels/routes sum before shared modulation. The changed AM association and new
// shaped/pair cases are checked by independent order/Fourier controls in component_oracles.
const PINNED: [&str; 9] = [
    "independent channel tones 0x5b853fb0775f8d78",
    "summed tones 0x84203345c16839f9",
    "modulated tone 0x00b6979df739e6fc",
    "gated tone 0x7de0fa6eae5ddf99",
    "rectangular pulse train 0xff6d3c57217ac3dd",
    "shaped pulsed tone 0xfeb9ff613918e8c5",
    "shared monaural pulse 0x4392cbf9da3bfb29",
    "shaped pulse source AM 0x338508cbc5bd1a01",
    "shared binaural AM 0xa5de5506f7462b43",
];

#[test]
fn the_digest_tells_apart_samples_that_differ_in_their_last_bit() {
    let frames: [[f64; 2]; 2] = [[0.0, 0.25], [-0.5, 1.0]];
    let mut altered = frames;
    altered[0][1] = f64::from_bits(altered[0][1].to_bits() + 1);
    assert_ne!(digest(&frames), digest(&altered));
    assert_eq!(digest(&[]), 0xcbf2_9ce4_8422_2325);
}

const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}
