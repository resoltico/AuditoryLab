// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::wav::{Container, ExportError, header};
use auditorylab_core::{SampleRate, Signal};

use std::io::Write;

use std::sync::atomic::{AtomicBool, Ordering};

/// Samples rendered and written at a time.
const BLOCK_FRAMES: usize = 4_096;

/// What a successful export wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exported {
    /// Number of frames in the file.
    pub frames: u64,
    /// The sample rate the file states.
    pub sample_rate: SampleRate,
    /// Container variant chosen for that length.
    pub container: Container,
}

/// Encode a validated signal into a writer.
pub(crate) fn encode(
    session: &Signal,
    output: &mut impl Write,
    cancel: &AtomicBool,
    progress: &crate::Progress,
) -> Result<Exported, ExportError> {
    let frames = session.frames();
    let sample_rate = session.sample_rate();
    let layout = header::layout(sample_rate, frames);
    stop_if_cancelled(cancel)?;
    write(output, &layout.header, session, cancel, progress)?;
    Ok(Exported {
        frames,
        sample_rate,
        container: layout.container,
    })
}

fn stop_if_cancelled(cancel: &AtomicBool) -> Result<(), ExportError> {
    if cancel.load(Ordering::Relaxed) {
        Err(ExportError::Cancelled)
    } else {
        Ok(())
    }
}

fn write(
    output: &mut impl Write,
    header: &[u8],
    signal: &Signal,
    cancel: &AtomicBool,
    progress: &crate::Progress,
) -> Result<(), ExportError> {
    output.write_all(header)?;
    let mut samples = vec![[0.0; 2]; BLOCK_FRAMES];
    let mut bytes = Vec::with_capacity(samples.len() * 8);
    let mut cursor = 0;
    while cursor < signal.frames() {
        stop_if_cancelled(cancel)?;
        let rendered = signal.render(cursor, &mut samples);
        bytes.clear();
        for sample in samples[..rendered].iter().flatten() {
            bytes.extend_from_slice(&nearest_f32(*sample).to_le_bytes());
        }
        output.write_all(&bytes)?;
        cursor += rendered as u64;
        progress.set(cursor);
    }
    stop_if_cancelled(cancel)
}

/// Round a binary64 sample within full scale to the nearest binary32.
#[expect(
    clippy::cast_possible_truncation,
    reason = "quality:lib-wav-export-nearest-f32"
)]
const fn nearest_f32(sample: f64) -> f32 {
    sample as f32
}

#[cfg(test)]
mod quantization {
    use super::*;
    #[test]
    fn nearest_binary32_handles_even_midpoints_signed_zero_subnormals_and_underflow() {
        let minimum = f64::from(f32::from_bits(1));
        let normal = f64::from(f32::MIN_POSITIVE);
        for (input, bits) in [
            (0.0, 0),
            (-0.0, 0x8000_0000),
            (1.0 + 2.0_f64.powi(-24), 1.0_f32.to_bits()),
            (1.0 + 3.0 * 2.0_f64.powi(-24), 1.0_f32.to_bits() + 2),
            (minimum / 2.0, 0),
            (-minimum / 2.0, 0x8000_0000),
            ((minimum / 2.0).next_up(), 1),
            (-(minimum / 2.0).next_up(), 0x8000_0001),
            (minimum, 1),
            (-minimum, 0x8000_0001),
            (minimum * 1.5, 2),
            (minimum * 2.5, 2),
            (normal - minimum / 2.0, f32::MIN_POSITIVE.to_bits()),
            (f64::MIN_POSITIVE, 0),
            (-f64::MIN_POSITIVE, 0x8000_0000),
        ] {
            assert_eq!(nearest_f32(input).to_bits(), bits, "input {input:e}");
        }
    }
    #[test]
    fn absolute_rounding_bounds_include_half_a_minimum_subnormal_near_zero() {
        let half_minimum = f64::from(f32::from_bits(1)) / 2.0;
        for input in [
            0.0,
            half_minimum,
            half_minimum.next_up(),
            half_minimum * 3.0,
            1e-45,
            1e-40,
            f64::from(f32::MIN_POSITIVE),
            1e-20,
            0.1,
            0.5,
            1.0,
        ] {
            for input in [input, -input] {
                let encoded = f64::from(nearest_f32(input));
                let allowance = (input.abs() * 2.0_f64.powi(-24)).max(half_minimum);
                assert!(
                    (encoded - input).abs() <= allowance,
                    "{input:e} -> {encoded:e}"
                );
            }
        }
        // A normal-only relative epsilon bound is false at underflow.
        assert!(half_minimum > half_minimum * f64::from(f32::EPSILON));
        assert_eq!(nearest_f32(half_minimum), 0.0);
    }
    #[test]
    fn actual_encoded_peaks_can_round_above_the_binary64_generation_bound() {
        use auditorylab_core::{Component, Ears, Stimulus};
        let session = Stimulus {
            duration_seconds: 4.0 / 8_000.0,
            fade_seconds: 0.0,
            components: vec![Component {
                ears: Ears::Left,
                peak_dbfs: -20.0,
                modulation: None,
                carrier: crate::Carrier::Tone {
                    frequency_hz: 2_000.0,
                },
            }],
        }
        .signal(SampleRate::new(8_000).unwrap())
        .unwrap();
        let mut expected = [[0.0; 2]; 4];
        session.render(0, &mut expected);
        assert_eq!(expected[1][0], 0.1);
        let mut bytes = Vec::new();
        encode(
            &session,
            &mut bytes,
            &AtomicBool::new(false),
            &crate::Progress::default(),
        )
        .unwrap();
        let encoded = f32::from_le_bytes(bytes[66..70].try_into().unwrap());
        assert!(f64::from(encoded) > expected[1][0]);
        for frame in bytes[58..].as_chunks::<8>().0 {
            assert_eq!(&frame[4..], &[0; 4]);
        }
        let session = Stimulus {
            duration_seconds: 4.0 / 8_000.0,
            fade_seconds: 0.0,
            components: vec![Component {
                ears: Ears::Both,
                peak_dbfs: -1_000.0,
                modulation: None,
                carrier: crate::Carrier::Tone {
                    frequency_hz: 2_000.0,
                },
            }],
        }
        .signal(SampleRate::new(8_000).unwrap())
        .unwrap();
        let mut bytes = Vec::new();
        encode(
            &session,
            &mut bytes,
            &AtomicBool::new(false),
            &crate::Progress::default(),
        )
        .unwrap();
        for sample in bytes[58..].as_chunks::<4>().0 {
            assert_eq!(f32::from_le_bytes(*sample), 0.0);
        }
        assert_eq!(
            u32::from_le_bytes(bytes[82..86].try_into().unwrap()),
            0x8000_0000
        );
    }
}
