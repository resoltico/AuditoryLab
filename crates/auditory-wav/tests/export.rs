// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Exports validated stereo sessions through the public filesystem boundary.

#![cfg(test)]

use auditory_core::{Component, Ears, Preset, SampleRate, Session, Stimulus, catalog};
use auditory_wav::{Container, ExportError, Exported, export};
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

fn rate() -> SampleRate {
    SampleRate::new(8_000).unwrap()
}

fn tone_session(frames: u32) -> Session {
    Stimulus {
        duration_seconds: f64::from(frames) / 8_000.0,
        fade_seconds: 0.001,
        components: vec![Component::Tone {
            ears: Ears::Both,
            frequency_hz: 500.0,
            level_dbfs: -20.0,
            modulation: None,
        }],
    }
    .session(rate())
    .unwrap()
}

fn never() -> AtomicBool {
    AtomicBool::new(false)
}

fn decoded_frames(path: &Path, frames: u32) -> Vec<[f32; 2]> {
    let mut reader = waveadapter::WavReader::new(fs::File::open(path).unwrap()).unwrap();
    assert_eq!(reader.sample_rate(), 8_000);
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
    let path = directory.path().join("stimulus.wav");
    assert_eq!(
        export(tone_session(1_000), &path, &never()).unwrap(),
        Exported {
            frames: 1_000,
            sample_rate: rate(),
            container: Container::Riff,
        }
    );
    let decoded = decoded_frames(&path, 1_000);
    let mut expected = vec![[0.0; 2]; 1_000];
    assert_eq!(
        tone_session(1_000).into_renderer().render(&mut expected),
        1_000
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
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn a_stereo_file_holds_exactly_the_58_byte_header_and_payload() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("silence.wav");
    let silence = Stimulus {
        duration_seconds: 5.0 / 8_000.0,
        fade_seconds: 0.0,
        components: vec![],
    }
    .session(rate())
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

#[test]
fn existing_files_and_directories_are_never_replaced() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("stimulus.wav");
    fs::write(&path, b"existing document").unwrap();
    assert!(matches!(
        export(tone_session(100), &path, &never()),
        Err(ExportError::DestinationExists)
    ));
    assert_eq!(fs::read(&path).unwrap(), b"existing document");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    assert!(export(tone_session(100), directory.path(), &never()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"existing document");
}

#[cfg(unix)]
#[test]
fn a_dangling_destination_symbolic_link_is_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("stimulus.wav");
    std::os::unix::fs::symlink("missing", &path).unwrap();
    assert!(matches!(
        export(tone_session(100), &path, &never()),
        Err(ExportError::DestinationExists)
    ));
    assert_eq!(fs::read_link(&path).unwrap(), Path::new("missing"));
    assert!(!directory.path().join("missing").exists());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn missing_and_nondirectory_parents_fail_without_creating_output() {
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("parent");
    assert!(matches!(
        export(tone_session(100), &parent.join("stimulus.wav"), &never()),
        Err(ExportError::Io(_))
    ));
    assert!(fs::read_dir(directory.path()).unwrap().next().is_none());
    fs::write(&parent, b"not a directory").unwrap();
    assert!(matches!(
        export(tone_session(100), &parent.join("stimulus.wav"), &never()),
        Err(ExportError::Io(_))
    ));
    assert_eq!(fs::read(&parent).unwrap(), b"not a directory");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn cancellation_before_start_leaves_no_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("stimulus.wav");
    assert!(matches!(
        export(tone_session(100), &path, &AtomicBool::new(true)),
        Err(ExportError::Cancelled)
    ));
    assert!(fs::read_dir(directory.path()).unwrap().next().is_none());
}

#[test]
fn a_relative_destination_exports_in_its_owned_directory() {
    // Cargo executes integration tests from their package root. Keep the
    // owned directory under the workspace target without changing process cwd.
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let directory = tempfile::tempdir_in(target).unwrap();
    let path = Path::new("../../target")
        .join(directory.path().file_name().unwrap())
        .join("relative.wav");
    assert!(path.is_relative());
    export(tone_session(100), &path, &never()).unwrap();
    assert_eq!(
        decoded_frames(&directory.path().join("relative.wav"), 100).len(),
        100
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

fn short_catalog_session(name: &str) -> Session {
    let mut preset = Preset::parse(catalog::find(name).unwrap().text).unwrap();
    preset.stimulus.duration_seconds = 0.125;
    preset.stimulus.fade_seconds = 0.01;
    preset.stimulus.session(rate()).unwrap()
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
    let decoded = decoded_frames(&path, 1_000);
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
    let decoded = decoded_frames(&path, 1_000);
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
