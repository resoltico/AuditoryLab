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
    crate::storage::wav::encode(
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
        AnalysisOptions { seconds: Some(2) },
        &Cancel::default(),
        &progress,
    )
    .unwrap();
    let a = result.analysis;
    assert_eq!(a.window.start_seconds, 1.0);
    assert_eq!(a.window.end_seconds, 3.0);
    assert_eq!(a.duration_seconds, 10.0);
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
        AnalysisOptions { seconds: Some(2) },
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
            AnalysisOptions { seconds: Some(0) },
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
        let a = result.analysis;
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
            a.pulsing
                .iter()
                .any(|p| p.envelope_band == crate::recording::EnvelopeBand::Whole
                    && (p.hz - 40.0).abs() < a.envelope_resolution_hz
                    && p.cause == crate::recording::PulsingCause::Modulation),
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
    crate::storage::wav::encode(
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
    .analysis;
    assert!(
        a.pulsing
            .iter()
            .any(|p| p.envelope_band == crate::recording::EnvelopeBand::Whole
                && (p.hz - 40.0).abs() < a.envelope_resolution_hz
                && p.cause == crate::recording::PulsingCause::Modulation),
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
        crate::storage::wav::encode(
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
        .analysis;
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
                            crate::recording::PulsingCause::Beating { harmonic: 1, .. }
                        ))
            );
        }
    }
}
