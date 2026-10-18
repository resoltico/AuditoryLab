// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::wav::{Container, ExportError, header};
use auditorylab_core::{Renderer, SampleRate, Session};

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

/// Encode a validated session into a caller-owned writer. No destination I/O occurs.
/// Cancellation is checked before writing and between rendered blocks.
///
/// ```no_run
/// fn encode(session: auditorylab_core::Session) -> Result<(), crate::wav::ExportError> {
///     let mut bytes = Vec::new();
///     let cancel = std::sync::atomic::AtomicBool::new(false);
///     crate::wav::encode(session, &mut bytes, &cancel)?;
///     Ok(())
/// }
/// ```
/// ```compile_fail
/// fn encode(renderer: auditorylab_core::Renderer) {
///     let _ = crate::wav::encode(renderer, &mut Vec::new(), &std::sync::atomic::AtomicBool::new(false));
/// }
/// ```
///
/// # Errors
/// Returns cancellation or the writer's original I/O error.
pub fn encode(
    session: Session,
    output: &mut impl Write,
    cancel: &AtomicBool,
) -> Result<Exported, ExportError> {
    let frames = session.frames();
    let sample_rate = session.sample_rate();
    let layout = header::layout(sample_rate, frames);
    stop_if_cancelled(cancel)?;
    write(output, &layout.header, session.into_renderer(), cancel)?;
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
    mut renderer: Renderer,
    cancel: &AtomicBool,
) -> Result<(), ExportError> {
    output.write_all(header)?;
    let mut samples = vec![[0.0; 2]; BLOCK_FRAMES];
    let mut bytes = Vec::with_capacity(samples.len() * 8);
    while !renderer.is_complete() {
        stop_if_cancelled(cancel)?;
        let rendered = renderer.render(&mut samples).frames();
        bytes.clear();
        for sample in samples[..rendered].iter().flatten() {
            bytes.extend_from_slice(&nearest_f32(*sample).to_le_bytes());
        }
        output.write_all(&bytes)?;
    }
    stop_if_cancelled(cancel)
}

/// Round a binary64 sample within full scale to the nearest binary32.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the file holds binary32 samples; the cast rounds to the nearest one"
)]
const fn nearest_f32(sample: f64) -> f32 {
    sample as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use auditorylab_core::{Component, Ears, Stimulus};
    use std::io;

    fn session(frames: u64) -> Session {
        Stimulus {
            duration_seconds: f64::from(u32::try_from(frames).unwrap()) / 8_000.0,
            fade_seconds: 0.0,
            components: vec![Component::Tone {
                ears: Ears::Both,
                frequency_hz: 500.0,
                peak_dbfs: -20.0,
                modulation: None,
            }],
        }
        .session(SampleRate::new(8_000).unwrap())
        .unwrap()
    }

    /// Accepts a number of bytes, then fails.
    struct Limited {
        accepted: Vec<u8>,
        limit: usize,
    }

    impl Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.accepted.len() + bytes.len() > self.limit {
                return Err(io::Error::other("injected write failure"));
            }
            self.accepted.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn the_header_then_every_sample_is_written_as_the_nearest_binary32() {
        let layout = header::layout(session(3).sample_rate(), 3);
        let mut output = Limited {
            accepted: Vec::new(),
            limit: usize::MAX,
        };
        write(
            &mut output,
            &layout.header,
            session(3).into_renderer(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let mut expected = [[0.0; 2]; 3];
        session(3).into_renderer().render(&mut expected);
        let mut bytes = layout.header.clone();
        for sample in expected.into_iter().flatten() {
            bytes.extend_from_slice(&nearest_f32(sample).to_le_bytes());
        }
        assert_eq!(output.accepted, bytes);
        assert_eq!(nearest_f32(0.1).to_bits(), 0.1_f32.to_bits());
        assert_eq!(nearest_f32(-1.0), -1.0);
    }

    #[test]
    fn a_write_failure_in_the_header_or_in_any_block_is_reported() {
        let layout = header::layout(session(20_000).sample_rate(), 20_000);
        // Before the header, within the first block, and within the last block.
        for limit in [0, layout.header.len(), layout.header.len() + 70_000] {
            let mut output = Limited {
                accepted: Vec::new(),
                limit,
            };
            let result = write(
                &mut output,
                &layout.header,
                session(20_000).into_renderer(),
                &AtomicBool::new(false),
            );
            assert!(
                matches!(result, Err(ExportError::Io(_))),
                "limit {limit}: {result:?}"
            );
        }
    }

    #[test]
    fn cancellation_before_encoding_writes_nothing() {
        let mut bytes = Vec::new();
        assert!(matches!(
            encode(session(5), &mut bytes, &AtomicBool::new(true)),
            Err(ExportError::Cancelled)
        ));
        assert!(bytes.is_empty());
    }
    #[test]
    fn cancellation_in_payload_stops_before_another_block_or_final_success() {
        struct Cancelling<'a> {
            bytes: Vec<u8>,
            cancel: &'a AtomicBool,
            calls: usize,
        }
        impl Write for Cancelling<'_> {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                self.bytes.extend_from_slice(bytes);
                self.calls += 1;
                if self.calls == 2 {
                    self.cancel.store(true, Ordering::Relaxed);
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        for frames in [10, 5000] {
            let cancel = AtomicBool::new(false);
            let mut writer = Cancelling {
                bytes: vec![],
                cancel: &cancel,
                calls: 0,
            };
            assert!(matches!(
                encode(session(frames), &mut writer, &cancel),
                Err(ExportError::Cancelled)
            ));
            assert_eq!(writer.calls, 2);
            assert_eq!(writer.bytes.len() as u64, 58 + frames.min(4096) * 8);
        }
    }
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
            components: vec![Component::Tone {
                ears: Ears::Left,
                frequency_hz: 2_000.0,
                peak_dbfs: -20.0,
                modulation: None,
            }],
        }
        .session(SampleRate::new(8_000).unwrap())
        .unwrap();
        let mut expected = [[0.0; 2]; 4];
        session.clone().into_renderer().render(&mut expected);
        assert_eq!(expected[1][0], 0.1);
        let mut bytes = Vec::new();
        encode(session, &mut bytes, &AtomicBool::new(false)).unwrap();
        let encoded = f32::from_le_bytes(bytes[66..70].try_into().unwrap());
        assert!(f64::from(encoded) > expected[1][0]);
        for frame in bytes[58..].as_chunks::<8>().0 {
            assert_eq!(&frame[4..], &[0; 4]);
        }
        let session = Stimulus {
            duration_seconds: 4.0 / 8_000.0,
            fade_seconds: 0.0,
            components: vec![Component::Tone {
                ears: Ears::Both,
                frequency_hz: 2_000.0,
                peak_dbfs: -1_000.0,
                modulation: None,
            }],
        }
        .session(SampleRate::new(8_000).unwrap())
        .unwrap();
        let mut bytes = Vec::new();
        encode(session, &mut bytes, &AtomicBool::new(false)).unwrap();
        for sample in bytes[58..].as_chunks::<4>().0 {
            assert_eq!(f32::from_le_bytes(*sample), 0.0);
        }
        assert_eq!(
            u32::from_le_bytes(bytes[82..86].try_into().unwrap()),
            0x8000_0000
        );
    }
}
