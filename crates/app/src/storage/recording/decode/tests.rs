use super::*;
use crate::core::{
    ChannelRouting, Component, ComponentForm, ComponentId, Finite, SampleRate, SoundSource,
    test_support::FiniteFixture,
};
fn finite(x: f64) -> Finite {
    Finite::new(x).unwrap()
}
fn exported_tone(path: &Path, seconds: f64) {
    let signal = FiniteFixture::new(
        finite(seconds),
        finite(0.001),
        vec![Component {
            id: ComponentId::numbered(0).unwrap(),
            form: ComponentForm {
                source: SoundSource::Tone {
                    frequency_hz: finite(40.0),
                    route: ChannelRouting::Both,
                    peak_dbfs: finite(-20.0),
                },
                modulation: None,
            },
        }],
    )
    .unwrap()
    .signal(SampleRate::new(8000).unwrap())
    .unwrap();
    let _published = crate::storage::wav::encode(
        &signal,
        &mut File::create(path).unwrap(),
        Cancel::default().flag(),
        &Progress::default(),
    )
    .unwrap();
}
#[test]
fn exporter_samples_are_measured_and_the_window_excludes_later_audio() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tone.wav");
    exported_tone(&path, 10.0);
    let progress = Progress::default();
    let result = analyze(
        &path,
        AnalysisOptions {
            start_seconds: 0,
            seconds: Some(2),
        },
        &Cancel::default(),
        &progress,
    )
    .unwrap();
    let a = result.analysis.clone();
    assert_eq!(a.window.start_seconds, 0.0);
    assert_eq!(a.window.end_seconds, 2.0);
    assert_eq!(a.duration_seconds, Some(10.0));
    assert!(!a.lossy);
    assert_eq!(a.channels, 2);
    assert!((a.tones[0].hz - 40.0).abs() < a.resolution_hz);
    assert!((a.stereo.rms_dbfs[0].unwrap() + 23.0103).abs() < 0.5);
    assert!(a.low_energy[0].fraction.unwrap() > 0.99);
    assert_eq!(progress.frames(), progress.total());
    let original = std::fs::read(&path).unwrap();
    let mut bytes = original.clone();
    let payload = bytes.len() - 8000 * 10 * 8;
    for sample in bytes[payload + 8000 * 4 * 8..].as_chunks_mut::<4>().0 {
        sample.copy_from_slice(&0.5_f32.to_le_bytes());
    }
    std::fs::write(&path, bytes).unwrap();
    let after = analyze(
        &path,
        AnalysisOptions {
            start_seconds: 0,
            seconds: Some(2),
        },
        &Cancel::default(),
        &Progress::default(),
    )
    .unwrap();
    assert_eq!(after.analysis, a);
    std::fs::write(&path, &original[..original.len() / 2]).unwrap();
    assert!(matches!(
        analyze(
            &path,
            AnalysisOptions::default(),
            &Cancel::default(),
            &Progress::default()
        ),
        Err(RecordingError::Decode { .. })
    ));
}
#[test]
fn too_short_invalid_window_and_cancellation_are_typed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("short.wav");
    exported_tone(&path, 1.0);
    assert!(matches!(
        analyze(
            &path,
            AnalysisOptions::default(),
            &Cancel::default(),
            &Progress::default()
        ),
        Err(RecordingError::TooShort)
    ));
    assert!(matches!(
        analyze(
            &path,
            AnalysisOptions {
                start_seconds: 0,
                seconds: Some(0)
            },
            &Cancel::default(),
            &Progress::default()
        ),
        Err(RecordingError::InvalidWindow)
    ));
    let cancel = Cancel::default();
    cancel.request();
    assert!(matches!(
        analyze(
            &path,
            AnalysisOptions::default(),
            &cancel,
            &Progress::default()
        ),
        Err(RecordingError::Cancelled)
    ));
}
#[test]
fn checked_in_codec_fixtures_preserve_tone_and_modulation_and_opus_is_refused() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recording");
    for (name, lossy) in [
        ("flac", false),
        ("mp3", true),
        ("m4a", true),
        ("aac", true),
        ("ogg", true),
        ("alac.m4a", false),
        ("aiff", false),
        ("caf", false),
    ] {
        let result = analyze(
            &base.join(format!("tone.{name}")),
            AnalysisOptions::default(),
            &Cancel::default(),
            &Progress::default(),
        )
        .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        let a = result.analysis.clone();
        assert_eq!(a.lossy, lossy, "{name}");
        assert!(
            (a.tones[0].dbfs + 28.1157).abs() < 0.5,
            "{name}: {:?}",
            a.tones
        );
        assert!(
            (a.tones[0].hz - 1000.0).abs() < a.resolution_hz,
            "{name}: {:?}",
            a.tones
        );
        assert!(
            a.pulsing.iter().any(|p| p.envelope_band
                == auditorylab_core::measurement::EnvelopeBand::Whole
                && (p.hz - 40.0).abs() < a.envelope_resolution_hz
                && p.cause == auditorylab_core::measurement::PulsingCause::Modulation),
            "{name}: {:?}",
            a.pulsing
        );
        assert!(a.ear_differences.is_empty(), "{name}");
    }
    let error = analyze(
        &base.join("tone.opus"),
        AnalysisOptions::default(),
        &Cancel::default(),
        &Progress::default(),
    )
    .unwrap_err();
    assert!(matches!(error, RecordingError::UnsupportedCodec { codec } if codec == "Opus"));
}

#[test]
fn valid_32bit_flac_is_refused_before_the_broken_decoder_path() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recording/tone.32bit.flac");
    let error = analyze(
        &path,
        AnalysisOptions::default(),
        &Cancel::default(),
        &Progress::default(),
    )
    .unwrap_err();
    assert!(matches!(error, RecordingError::UnsupportedCodec { codec }
        if codec.starts_with("32-bit FLAC")));
}

#[test]
fn a_video_only_container_is_not_an_audio_recording() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recording/no-audio.mp4");
    assert!(matches!(
        analyze(
            &path,
            AnalysisOptions::default(),
            &Cancel::default(),
            &Progress::default()
        ),
        Err(RecordingError::NoAudioTrack)
    ));
}
#[test]
fn exported_builtin_clicks_have_a_40hz_modulation_pattern() {
    let recipe = crate::services::builtin("chan-2022-clicks-40-hz").unwrap();
    let mut fixture = FiniteFixture::from_recipe(recipe.recipe());
    fixture.set_duration(finite(10.0));
    fixture.set_fade(finite(0.01));
    let signal = fixture.signal(SampleRate::new(48000).unwrap()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clicks.wav");
    let _published = crate::storage::wav::encode(
        &signal,
        &mut File::create(&path).unwrap(),
        Cancel::default().flag(),
        &Progress::default(),
    )
    .unwrap();
    let a = analyze(
        &path,
        AnalysisOptions::default(),
        &Cancel::default(),
        &Progress::default(),
    )
    .unwrap()
    .analysis
    .clone();
    assert!(
        a.pulsing.iter().any(|p| p.envelope_band
            == auditorylab_core::measurement::EnvelopeBand::Whole
            && (p.hz - 40.0).abs() < a.envelope_resolution_hz
            && p.cause == auditorylab_core::measurement::PulsingCause::Modulation),
        "{:?}",
        a.pulsing
    );
}

#[test]
fn exported_pairs_keep_channel_routing_through_the_decoder() {
    let dir = tempfile::tempdir().unwrap();
    for binaural in [true, false] {
        let source = if binaural {
            SoundSource::BinauralPair {
                first_hz: finite(200.0),
                offset_hz: finite(10.0),
                first_peak_dbfs: finite(-20.0),
                second_peak_dbfs: finite(-20.0),
            }
        } else {
            SoundSource::MonauralPair {
                route: ChannelRouting::Both,
                first_hz: finite(200.0),
                offset_hz: finite(10.0),
                first_peak_dbfs: finite(-20.0),
                second_peak_dbfs: finite(-20.0),
            }
        };
        let signal = FiniteFixture::new(
            finite(10.0),
            finite(0.001),
            vec![Component {
                id: ComponentId::numbered(0).unwrap(),
                form: ComponentForm {
                    source,
                    modulation: None,
                },
            }],
        )
        .unwrap()
        .signal(SampleRate::new(8000).unwrap())
        .unwrap();
        let path = dir.path().join(if binaural {
            "binaural.wav"
        } else {
            "monaural.wav"
        });
        let _published = crate::storage::wav::encode(
            &signal,
            &mut File::create(&path).unwrap(),
            Cancel::default().flag(),
            &Progress::default(),
        )
        .unwrap();
        let a = analyze(
            &path,
            AnalysisOptions::default(),
            &Cancel::default(),
            &Progress::default(),
        )
        .unwrap()
        .analysis
        .clone();
        if binaural {
            assert_eq!(a.ear_differences.len(), 1);
            assert!((a.ear_differences[0].left_hz - 200.0).abs() < a.resolution_hz);
            assert!((a.ear_differences[0].right_hz - 210.0).abs() < a.resolution_hz);
            assert_eq!(
                a.ear_differences[0].band,
                Some(crate::core::rhythm::Band::Alpha)
            );
        } else {
            assert_eq!(a.ear_differences, Vec::new());
            assert!(a.coincident_tones);
            assert!(
                a.pulsing
                    .iter()
                    .any(|p| (p.hz - 10.0).abs() < a.envelope_resolution_hz
                        && matches!(
                            p.cause,
                            auditorylab_core::measurement::PulsingCause::Beating {
                                harmonic: 1,
                                ..
                            }
                        ))
            );
        }
    }
}

#[test]
fn unsupported_aac_profiles_name_the_supported_profile() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recording/tone.aac");
    let mut bytes = std::fs::read(base).unwrap();
    // ADTS profile bits 11 denote AAC LTP, outside the enabled AAC-LC decoder.
    bytes[2] |= 0xC0;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profile.aac");
    std::fs::write(&path, bytes).unwrap();
    let error = analyze(
        &path,
        AnalysisOptions::default(),
        &Cancel::default(),
        &Progress::default(),
    )
    .unwrap_err();
    assert!(matches!(error, RecordingError::UnsupportedCodec { codec }
        if codec.contains("AAC-LC only")));
}

#[test]
fn in_band_sbr_is_refused_instead_of_returning_only_the_lc_core() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recording/tone.aac");
    let original = std::fs::read(base).unwrap();
    let frame_len = (usize::from(original[3] & 3) << 11)
        | (usize::from(original[4]) << 3)
        | usize::from(original[5] >> 5);
    // ID_FIL=6, count=1, EXT_SBR_DATA=13, four payload bits. Preserve the
    // actual LC frame after the declaration: a decoder ignoring SBR would accept it.
    let mut bits = vec![
        true, true, false, false, false, false, true, true, true, false, true, false, false, false,
        false,
    ];
    for byte in &original[7..frame_len] {
        bits.extend((0..8).rev().map(|shift| byte & (1 << shift) != 0));
    }
    let payload: Vec<u8> = bits
        .chunks(8)
        .map(|bits| {
            bits.iter()
                .enumerate()
                .fold(0, |value, (i, bit)| value | (u8::from(*bit) << (7 - i)))
        })
        .collect();
    let length = u16::try_from(7 + payload.len()).unwrap();
    let mut bytes = original[..7].to_vec();
    bytes[3] = (bytes[3] & 0xFC) | u8::try_from((length >> 11) & 3).unwrap();
    bytes[4] = u8::try_from((length >> 3) & 0xFF).unwrap();
    bytes[5] = (bytes[5] & 0x1F) | (u8::try_from(length & 7).unwrap() << 5);
    bytes.extend(payload);
    bytes.extend_from_slice(&original[frame_len..]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sbr.aac");
    std::fs::write(&path, bytes).unwrap();
    match analyze(
        &path,
        AnalysisOptions::default(),
        &Cancel::default(),
        &Progress::default(),
    ) {
        Err(RecordingError::UnsupportedCodec { codec }) => assert!(codec.contains("AAC-LC only")),
        Err(error) => panic!("SBR must have a codec refusal, got {error:?}"),
        Ok(_) => panic!("SBR must not be accepted as a complete LC decode"),
    }
}

#[test]
fn known_multichannel_input_is_refused_by_the_analysis_policy_before_codec_setup() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/recording/tone.aac");
    let mut bytes = std::fs::read(base).unwrap();
    // ADTS channel configuration 110 means 5.1; the original fixture is 010 (stereo).
    bytes[2] |= 1;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("surround.aac");
    std::fs::write(&path, bytes).unwrap();
    assert!(matches!(
        analyze(
            &path,
            AnalysisOptions::default(),
            &Cancel::default(),
            &Progress::default()
        ),
        Err(RecordingError::UnsupportedChannels { channels: 6 })
    ));
}

// Independently wrap the actual exporter's RIFF chunks in a ds64 container.
fn extended_wave(bytes: &[u8]) -> Vec<u8> {
    let mut chunks = bytes[12..].to_vec();
    let mut offset = 0;
    let data = loop {
        let size = u32::from_le_bytes(chunks[offset + 4..offset + 8].try_into().unwrap());
        if &chunks[offset..offset + 4] == b"data" {
            chunks[offset + 4..offset + 8].copy_from_slice(&u32::MAX.to_le_bytes());
            break u64::from(size);
        }
        offset += 8 + usize::try_from(size + (size & 1)).unwrap();
    };
    let mut out = b"RF64\xff\xff\xff\xffWAVEds64".to_vec();
    out.extend_from_slice(&28_u32.to_le_bytes());
    out.extend_from_slice(&(u64::try_from(bytes.len()).unwrap() + 28).to_le_bytes());
    out.extend_from_slice(&data.to_le_bytes());
    out.extend_from_slice(&(data / 8).to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend(chunks);
    out
}
#[test]
fn rf64_and_bw64_decode_exporter_samples_and_reject_invalid_sizes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("extended.wav");
    exported_tone(&path, 3.0);
    let ordinary = std::fs::read(&path).unwrap();
    let expected = analyze(
        &path,
        AnalysisOptions::default(),
        &Cancel::default(),
        &Progress::default(),
    )
    .unwrap();
    let rf64 = extended_wave(&ordinary);
    // Repeated ds64 table IDs resolve in occurrence order; odd chunks keep padding.
    let mut tabled = rf64[..48].to_vec();
    tabled[16..20].copy_from_slice(&52_u32.to_le_bytes());
    tabled[20..28].copy_from_slice(&(u64::try_from(rf64.len()).unwrap() - 8 + 52).to_le_bytes());
    tabled[44..48].copy_from_slice(&2_u32.to_le_bytes());
    for size in [9_u64, 2] {
        tabled.extend_from_slice(b"JUNK");
        tabled.extend_from_slice(&size.to_le_bytes());
    }
    for size in [9_usize, 2] {
        tabled.extend_from_slice(b"JUNK\xff\xff\xff\xff");
        tabled.extend(vec![0; size + (size & 1)]);
    }
    tabled.extend_from_slice(&rf64[48..]);
    std::fs::write(&path, &tabled).unwrap();
    assert_eq!(
        analyze(
            &path,
            AnalysisOptions::default(),
            &Cancel::default(),
            &Progress::default()
        )
        .unwrap()
        .analysis,
        expected.analysis
    );
    tabled[48..52].copy_from_slice(b"MISS");
    std::fs::write(&path, &tabled).unwrap();
    assert!(
        analyze(
            &path,
            AnalysisOptions::default(),
            &Cancel::default(),
            &Progress::default()
        )
        .is_err()
    );
    for marker in [b"RF64", b"BW64"] {
        let mut bytes = rf64.clone();
        bytes[..4].copy_from_slice(marker);
        std::fs::write(&path, bytes).unwrap();
        let measured = analyze(
            &path,
            AnalysisOptions::default(),
            &Cancel::default(),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(measured.analysis, expected.analysis);
    }
    for (offset, replacement) in [
        (16, 27_u32.to_le_bytes().to_vec()),
        (20, u64::MAX.to_le_bytes().to_vec()),
        (28, u64::MAX.to_le_bytes().to_vec()),
        (36, 1_u64.to_le_bytes().to_vec()),
        (44, 1_u32.to_le_bytes().to_vec()),
    ] {
        let mut bytes = rf64.clone();
        bytes[offset..offset + replacement.len()].copy_from_slice(&replacement);
        std::fs::write(&path, bytes).unwrap();
        assert!(matches!(
            analyze(
                &path,
                AnalysisOptions::default(),
                &Cancel::default(),
                &Progress::default()
            ),
            Err(RecordingError::Decode { .. })
        ));
    }
    std::fs::write(&path, &rf64[..rf64.len() - 8]).unwrap();
    assert!(
        analyze(
            &path,
            AnalysisOptions::default(),
            &Cancel::default(),
            &Progress::default()
        )
        .is_err()
    );
}
#[test]
fn rf64_seeks_and_reads_past_four_gibibytes_without_copying_the_payload() {
    use symphonia::core::formats::{SeekMode, SeekTo};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.wav");
    exported_tone(&path, 3.0);
    let mut bytes = extended_wave(&std::fs::read(&path).unwrap());
    let data = (1_u64 << 32) + 8000;
    let payload = bytes.len() - 3 * 8000 * 8;
    let size = u64::try_from(payload).unwrap() + data;
    bytes[20..28].copy_from_slice(&(size - 8).to_le_bytes());
    bytes[28..36].copy_from_slice(&data.to_le_bytes());
    bytes[36..44].copy_from_slice(&(data / 8).to_le_bytes());
    std::fs::write(&path, &bytes[..payload]).unwrap();
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(size)
        .unwrap();
    let mut decoder = Decoder::new(File::open(&path).unwrap()).unwrap();
    assert_eq!(decoder.frames, Some(data / 8));
    decoder
        .format
        .seek(
            SeekMode::Accurate,
            SeekTo::Timestamp {
                track_id: 0,
                ts: i64::try_from(data / 8 - 1).unwrap().into(),
            },
        )
        .unwrap();
    assert!(decoder.next(&Cancel::default()).unwrap());
    assert!(decoder.samples.iter().all(|sample| *sample == 0.0));
    assert!(!decoder.next(&Cancel::default()).unwrap());
}

fn silent_decoder(rate: u32, frames: u32) -> Decoder {
    use std::io::Write;
    let mut file = tempfile::tempfile().unwrap();
    let bytes = frames.checked_mul(2).unwrap();
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36 + bytes).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt \x10\0\0\0\x01\0\x01\0").unwrap();
    file.write_all(&rate.to_le_bytes()).unwrap();
    file.write_all(&(rate * 2).to_le_bytes()).unwrap();
    file.write_all(b"\x02\0\x10\0data").unwrap();
    file.write_all(&bytes.to_le_bytes()).unwrap();
    file.set_len(u64::from(bytes) + 44).unwrap();
    Decoder::new(file).unwrap()
}
#[test]
fn actual_two_second_minimum_applies_after_prefix_at_all_rates() {
    for rate in [1000, 48_000, 768_000] {
        for delta in [-1_i64, 0, 1] {
            let frames = u32::try_from(i64::from(rate) * 2 + delta).unwrap();
            let mut decoder = silent_decoder(rate, frames);
            decoder.frames = None;
            let result = measure_decoder(
                decoder,
                AnalysisOptions::default(),
                &Cancel::default(),
                &Progress::default(),
            );
            if delta < 0 {
                assert!(
                    matches!(result, Err(RecordingError::TooShort)),
                    "rate={rate}"
                );
            } else {
                let a = result.unwrap();
                assert_eq!(a.duration_seconds, None);
                assert_eq!(a.window.end_seconds, f64::from(frames) / f64::from(rate));
                assert_eq!(a.stereo.rms_dbfs, vec![None]);
                let n = 1_u32 << frames.min(1 << 20).ilog2();
                assert_eq!(a.resolution_hz, f64::from(rate) / f64::from(n));
            }
        }
    }
}
#[test]
fn trustworthy_selected_endpoint_clips_normally_and_truncation_fails() {
    for rate in [1000, 48_000, 768_000] {
        let options = AnalysisOptions {
            start_seconds: 1,
            seconds: Some(4),
        };
        let a = measure_decoder(
            silent_decoder(rate, rate * 3),
            options,
            &Cancel::default(),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(a.duration_seconds, Some(3.0));
        assert_eq!(a.window.start_seconds, 1.0);
        assert_eq!(a.window.end_seconds, 3.0);
        let mut truncated = silent_decoder(rate, rate * 3);
        truncated.frames = Some(u64::from(rate) * 4);
        assert!(matches!(
            measure_decoder(truncated, options, &Cancel::default(), &Progress::default()),
            Err(RecordingError::Decode { .. })
        ));
    }
}
#[test]
fn unknown_clean_eof_selects_fitting_fft_and_exact_limit_ignores_later_packets() {
    for frames in [2047_u32, 2048, 2049, 4095, 4096, 4097] {
        let mut decoder = silent_decoder(1000, frames);
        decoder.frames = None;
        let a = measure_decoder(
            decoder,
            AnalysisOptions::default(),
            &Cancel::default(),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(a.resolution_hz, 1000.0 / f64::from(1_u32 << frames.ilog2()));
        assert_eq!(a.window.end_seconds, f64::from(frames) / 1000.0);
    }
    let mut decoder = silent_decoder(1000, 6000);
    decoder.frames = None;
    let a = measure_decoder(
        decoder,
        AnalysisOptions {
            start_seconds: 2,
            seconds: Some(2),
        },
        &Cancel::default(),
        &Progress::default(),
    )
    .unwrap();
    assert_eq!(
        a.window,
        auditorylab_core::measurement::AnalysisWindow {
            start_seconds: 2.0,
            end_seconds: 4.0
        }
    );
    assert_eq!(a.samples().window.end_seconds, 2.0);
}
#[test]
fn request_grammar_and_frame_budgets_refuse_before_owned_allocation() {
    for (start, duration) in [
        ("-1", "2"),
        (" 1", "2"),
        ("1.0", "2"),
        ("0", "1"),
        ("0", "3601"),
        ("86401", "2"),
        ("0", "4294967296"),
        ("０", "2"),
    ] {
        assert!(AnalysisOptions::from_text(start, duration).is_err());
    }
    assert_eq!(
        AnalysisOptions::from_text("", "").unwrap(),
        AnalysisOptions::default()
    );
    let max = AnalysisOptions {
        start_seconds: 0,
        seconds: Some(2),
    };
    assert!(Selection::new(1 << 31, 1, None, max).is_ok());
    assert!(matches!(
        Selection::new((1 << 31) + 1, 1, None, max),
        Err(RecordingError::BudgetExceeded {
            resource: "measurement frames"
        })
    ));
    assert!(matches!(
        Selection::new(
            u32::MAX,
            1,
            None,
            AnalysisOptions {
                start_seconds: 2,
                seconds: Some(2)
            }
        ),
        Err(RecordingError::BudgetExceeded {
            resource: "skip frames"
        })
    ));
    assert!(
        Selection::new(
            768_000,
            2,
            None,
            AnalysisOptions {
                start_seconds: 0,
                seconds: Some(3600)
            }
        )
        .is_ok()
    );
}

#[test]
fn fractional_trustworthy_endpoints_are_checked_as_integer_frames() {
    for (rate, start_seconds, frames) in [
        (1000, 1, 4012),
        (48_000, 2, 192_006),
        (768_000, 2, 3_072_002),
    ] {
        let a = measure_decoder(
            silent_decoder(rate, frames),
            AnalysisOptions {
                start_seconds,
                seconds: Some(5),
            },
            &Cancel::default(),
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(a.start_frame, u64::from(start_seconds) * u64::from(rate));
        assert_eq!(a.end_frame, u64::from(frames));
        assert_eq!(a.frames, a.end_frame - a.start_frame);
        assert_eq!(a.window.end_seconds, f64::from(frames) / f64::from(rate));
        assert_eq!(a.duration_seconds, Some(a.window.end_seconds));
        assert_eq!(a.source_frames, Some(u64::from(frames)));
    }
}
#[test]
fn packet_and_prefix_budgets_accept_limits_and_refuse_one_past() {
    assert!(admit_decoded_frames(1 << 22).is_ok());
    assert!(matches!(
        admit_decoded_frames((1 << 22) + 1),
        Err(RecordingError::BudgetExceeded {
            resource: "decoded packet"
        })
    ));
    assert!(admit_decoded_frames(usize::MAX).is_err());
    let mut decoder = silent_decoder(1000, 10_000);
    decoder.packet = (1 << 24) - 1;
    assert!(decoder.next(&Cancel::default()).unwrap());
    assert_eq!(decoder.packet, 1 << 24);
    assert!(matches!(
        decoder.next(&Cancel::default()),
        Err(RecordingError::BudgetExceeded { resource: "packet" })
    ));
    let options = AnalysisOptions {
        start_seconds: 0,
        seconds: Some(3600),
    };
    assert_eq!(
        Selection::new(768_000, 2, None, options)
            .unwrap()
            .prefix_capacity,
        2 * (1 << 20)
    );
    assert!(
        Selection::new(
            1 << 31,
            1,
            None,
            AnalysisOptions {
                start_seconds: 2,
                seconds: Some(2)
            }
        )
        .is_ok()
    );
    assert!(matches!(
        Selection::new(
            6_700_417,
            1,
            None,
            AnalysisOptions {
                start_seconds: 641,
                seconds: Some(2)
            }
        ),
        Err(RecordingError::BudgetExceeded {
            resource: "skip frames"
        })
    ));
    assert!(matches!(
        Selection::new(
            6_700_417,
            1,
            None,
            AnalysisOptions {
                start_seconds: 0,
                seconds: Some(641)
            }
        ),
        Err(RecordingError::BudgetExceeded {
            resource: "measurement frames"
        })
    ));
}

#[test]
fn selected_start_discards_actual_samples_inside_a_decoder_packet() {
    use std::io::Write;
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(b"RIFF").unwrap();
    file.write_all(&8036_u32.to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt \x10\0\0\0\x01\0\x01\0").unwrap();
    file.write_all(&1000_u32.to_le_bytes()).unwrap();
    file.write_all(&2000_u32.to_le_bytes()).unwrap();
    file.write_all(b"\x02\0\x10\0data").unwrap();
    file.write_all(&8000_u32.to_le_bytes()).unwrap();
    file.set_len(8044).unwrap();
    file.seek(SeekFrom::Start(44 + 2000 * 2)).unwrap();
    for frame in 0..2000 {
        let sample = (std::f64::consts::TAU * 300.0 * f64::from(frame) / 1000.0).sin() * 3000.0;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "independent bounded PCM16 tone fixture"
        )]
        file.write_all(&(sample as i16).to_le_bytes()).unwrap();
    }
    let first = measure_decoder(
        Decoder::new(file.try_clone().unwrap()).unwrap(),
        AnalysisOptions {
            start_seconds: 0,
            seconds: Some(2),
        },
        &Cancel::default(),
        &Progress::default(),
    )
    .unwrap();
    assert_eq!(first.tones, Vec::new());
    assert_eq!(first.stereo.rms_dbfs, vec![None]);
    let selected = measure_decoder(
        Decoder::new(file).unwrap(),
        AnalysisOptions {
            start_seconds: 2,
            seconds: Some(2),
        },
        &Cancel::default(),
        &Progress::default(),
    )
    .unwrap();
    assert!((selected.tones[0].hz - 300.0).abs() < selected.resolution_hz);
    assert_eq!(selected.start_frame, 2000);
    assert_eq!(selected.end_frame, 4000);
}
