// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

// Independently decode caller-owned encoded streams and test writer boundaries.

use crate::wav::{Container, Exported, encode};
use auditorylab_core::{Ears, Recipe, SampleRate, Signal, Sound, Voice, catalog};
fn export(
    session: Signal,
    path: &Path,
    cancel: &AtomicBool,
) -> Result<Exported, crate::wav::ExportError> {
    encode(
        &session,
        &mut fs::File::create(path).unwrap(),
        cancel,
        &crate::workflow::Progress::default(),
    )
}
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

fn rate() -> SampleRate {
    SampleRate::new(8_000).unwrap()
}

fn tone_session(frames: u32, sample_rate: SampleRate) -> Signal {
    Sound {
        duration_seconds: f64::from(frames) / f64::from(sample_rate.hz()),
        fade_seconds: 0.001,
        components: (vec![Voice {
            ears: Ears::Both,
            peak_dbfs: -20.0,
            modulation: None,
            carrier: crate::core::Carrier::Tone {
                frequency_hz: 500.0,
            },
        }])
        .into_iter()
        .enumerate()
        .map(|(index, voice)| auditorylab_core::Component {
            id: auditorylab_core::ComponentId::numbered(index).unwrap(),
            node: auditorylab_core::Node::Voice(voice),
        })
        .collect(),
    }
    .signal(sample_rate)
    .unwrap()
}

fn never() -> AtomicBool {
    AtomicBool::new(false)
}

fn decoded_frames(path: &Path, frames: u32, sample_rate: SampleRate) -> Vec<[f32; 2]> {
    let mut reader = waveadapter::WavReader::new(fs::File::open(path).unwrap()).unwrap();
    assert_eq!(
        reader.sample_rate(),
        usize::try_from(sample_rate.hz()).unwrap()
    );
    assert_eq!(reader.channels(), 2);
    assert_eq!(reader.frames(), usize::try_from(frames).unwrap());
    assert_eq!(reader.sample_format(), Some(waveadapter::SampleFormat::F32));
    let mut payload = Vec::new();
    assert_eq!(
        reader
            .read_raw_interleaved(usize::try_from(frames).unwrap() + 1, &mut payload)
            .unwrap(),
        usize::try_from(frames).unwrap()
    );
    assert_eq!(payload.len(), usize::try_from(frames).unwrap() * 8);
    payload
        .as_chunks::<8>()
        .0
        .iter()
        .map(|bytes| {
            [
                f32::from_le_bytes(bytes[..4].try_into().unwrap()),
                f32::from_le_bytes(bytes[4..].try_into().unwrap()),
            ]
        })
        .collect()
}

#[test]
fn an_independent_decoder_reads_the_format_length_and_nearest_f32_samples() {
    let directory = tempfile::tempdir().unwrap();
    for hz in [8_000, 44_100, 48_000, 96_000, 768_000] {
        let sample_rate = SampleRate::new(hz).unwrap();
        let frames = 20_000;
        let path = directory.path().join(format!("{hz}.wav"));
        assert_eq!(
            export(tone_session(frames, sample_rate), &path, &never()).unwrap(),
            Exported {
                frames: u64::from(frames),
                sample_rate,
                container: Container::Riff
            }
        );
        let decoded = decoded_frames(&path, frames, sample_rate);
        let mut expected = vec![[0.0; 2]; usize::try_from(frames).unwrap()];
        assert_eq!(
            tone_session(frames, sample_rate).render(0, &mut expected),
            usize::try_from(frames).unwrap()
        );
        for (file, rendered) in decoded.iter().zip(&expected) {
            for (actual, original) in file.iter().zip(rendered) {
                assert!(
                    (f64::from(*actual) - original).abs()
                        <= f64::from(f32::EPSILON) / 2.0
                            * original.abs().max(f64::from(f32::MIN_POSITIVE))
                );
            }
        }
        assert!(decoded.iter().any(|frame| frame[0].abs() > 0.09));
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 5);
}

#[test]
fn a_stereo_file_holds_exactly_the_58_byte_header_and_payload() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("silence.wav");
    let silence = Sound {
        duration_seconds: 5.0 / 8_000.0,
        fade_seconds: 0.0,
        components: (vec![])
            .into_iter()
            .enumerate()
            .map(|(index, voice)| auditorylab_core::Component {
                id: auditorylab_core::ComponentId::numbered(index).unwrap(),
                node: auditorylab_core::Node::Voice(voice),
            })
            .collect(),
    }
    .signal(rate())
    .unwrap();
    export(silence, &path, &never()).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 58 + 5 * 8);
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 90);
    assert_eq!(&bytes[8..20], b"WAVEfmt \x12\0\0\0");
    assert_eq!(&bytes[20..24], &[3, 0, 2, 0]);
    assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 8_000);
    assert_eq!(
        u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
        64_000
    );
    assert_eq!(&bytes[32..38], &[8, 0, 32, 0, 0, 0]);
    assert_eq!(&bytes[38..46], b"fact\x04\0\0\0");
    assert_eq!(u32::from_le_bytes(bytes[46..50].try_into().unwrap()), 5);
    assert_eq!(&bytes[50..54], b"data");
    assert_eq!(u32::from_le_bytes(bytes[54..58].try_into().unwrap()), 40);
    assert_eq!(&bytes[58..], &[0; 40]);
}

fn short_catalog_session(name: &str) -> Signal {
    let mut recipe = Recipe::parse(catalog::find(name).unwrap().text).unwrap();
    recipe.stimulus.duration_seconds = 0.125;
    recipe.stimulus.fade_seconds = 0.01;
    recipe.stimulus.signal(rate()).unwrap()
}

fn edge_gain(frame: u32) -> f64 {
    let depth = frame.min(999 - frame);
    (std::f64::consts::FRAC_PI_2 * f64::from(depth.min(80)) / 80.0)
        .sin()
        .powi(2)
}

#[test]
fn a_shortened_monaural_catalog_entry_has_identical_channels_and_independent_samples() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("monaural.wav");
    export(
        short_catalog_session("orozco-perez-2020-monaural-7-hz"),
        &path,
        &never(),
    )
    .unwrap();
    let decoded = decoded_frames(&path, 1_000, rate());
    for (frame, actual) in (0_u32..).zip(&decoded) {
        assert_eq!(actual[0].to_bits(), actual[1].to_bits());
        let t = f64::from(frame) / 8_000.0;
        let expected = ((std::f64::consts::TAU * 396.5 * t).sin()
            + (std::f64::consts::TAU * 403.5 * t).sin())
            * 10.0_f64.powf(-23.0103 / 20.0)
            * edge_gain(frame);
        // Nearest binary32 conversion plus binary64 oscillator phase rounding.
        assert!((f64::from(actual[0]) - expected).abs() < 1e-8);
    }
    assert_eq!(decoded[0], [0.0; 2]);
    assert_eq!(decoded[999], [0.0; 2]);
    assert!(decoded.iter().any(|frame| frame[0].abs() > 0.05));
}

#[test]
fn a_shortened_left_ear_sam_catalog_entry_has_a_silent_right_channel() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sam.wav");
    export(
        short_catalog_session("van-eeckhoutte-2018-am-500-hz"),
        &path,
        &never(),
    )
    .unwrap();
    let decoded = decoded_frames(&path, 1_000, rate());
    for (frame, actual) in (0_u32..).zip(&decoded) {
        assert_eq!(actual[1].to_bits(), 0);
        let t = f64::from(frame) / 8_000.0;
        let expected = 0.1
            * (std::f64::consts::TAU * 500.0 * t).sin()
            * (1.0 + (std::f64::consts::TAU * 39.0625 * t).sin())
            / 2.0
            * edge_gain(frame);
        assert!((f64::from(actual[0]) - expected).abs() < 1e-8);
    }
    assert_eq!(decoded[0], [0.0; 2]);
    assert_eq!(decoded[999], [0.0; 2]);
    assert!(decoded.iter().any(|frame| frame[0].abs() > 0.05));
}

#[test]
fn cancellation_requested_by_the_writer_stops_before_the_next_sample_block() {
    use std::io::{self, Write};
    use std::sync::atomic::Ordering;
    struct CancelAfterWrite<'a> {
        cancel: &'a AtomicBool,
        trigger: usize,
        writes: usize,
        bytes: Vec<u8>,
    }
    impl Write for CancelAfterWrite<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            self.bytes.extend_from_slice(bytes);
            if self.writes == self.trigger {
                self.cancel.store(true, Ordering::Relaxed);
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    // First stop immediately after the header, then after the first sample
    // block. A missing between-block cancellation check would write the whole signal.
    for trigger in [1, 2] {
        let cancel = never();
        let mut writer = CancelAfterWrite {
            cancel: &cancel,
            trigger,
            writes: 0,
            bytes: vec![],
        };
        let result = encode(
            &tone_session(20_000, rate()),
            &mut writer,
            &cancel,
            &crate::workflow::Progress::default(),
        );
        assert!(matches!(result, Err(crate::wav::ExportError::Cancelled)));
        assert_eq!(writer.writes, trigger);
        assert!(writer.bytes.starts_with(b"RIFF"));
        assert!(writer.bytes.len() < 20_000 * 8);
        if trigger == 1 {
            assert_eq!(
                &writer.bytes[writer.bytes.len() - 8..writer.bytes.len() - 4],
                b"data"
            );
        } else {
            assert!(writer.bytes.len() > 58);
            assert!(writer.bytes[58..].iter().any(|byte| *byte != 0));
        }
    }
}
#[test]
fn writer_failures_in_the_header_and_payload_keep_the_original_error() {
    struct Failing {
        calls: usize,
        fail_on: usize,
    }
    impl std::io::Write for Failing {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            if self.calls == self.fail_on {
                Err(std::io::Error::other("injected writer failure"))
            } else {
                Ok(bytes.len())
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    for fail_on in [1, 2, 3] {
        let mut writer = Failing { calls: 0, fail_on };
        let error = encode(
            &tone_session(20000, rate()),
            &mut writer,
            &never(),
            &crate::workflow::Progress::default(),
        )
        .unwrap_err();
        assert!(
            matches!(error,crate::wav::ExportError::Io(ref e) if e.to_string()=="injected writer failure")
        );
        assert_eq!(writer.calls, fail_on);
    }
}
