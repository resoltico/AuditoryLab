use super::ContainerFormat as F;

fn with_head(head: &[u8]) -> F {
    F::from_signature(head)
}

#[test]
fn leading_signatures_identify_common_audio_containers() {
    assert_eq!(with_head(b"RIFF\x24\x08\0\0WAVEfmt "), F::Wav);
    assert_eq!(with_head(b"RF64\xff\xff\xff\xffWAVEds64"), F::Wav);
    assert_eq!(with_head(b"BW64\xff\xff\xff\xffWAVE"), F::Wav);
    assert_eq!(with_head(b"FORM\0\0\0\0AIFFCOMM"), F::Aiff);
    assert_eq!(with_head(b"FORM\0\0\0\0AIFC"), F::Aiff);
    assert_eq!(with_head(b"fLaC\0\0\0\x22"), F::Flac);
    assert_eq!(with_head(b"OggS\0\x02\0\0"), F::Ogg);
    assert_eq!(with_head(&[0x1A, 0x45, 0xDF, 0xA3, 0x9F]), F::Matroska);
    assert_eq!(with_head(b"\0\0\0\x20ftypM4A "), F::Mp4);
    assert_eq!(with_head(b"ID3\x04\0\0"), F::Mp3);
    assert_eq!(with_head(&[0xFF, 0xFB, 0x90, 0x64]), F::Mp3);
    assert_eq!(with_head(&[0xFF, 0xF1, 0x50, 0x80]), F::AdtsAac);
    assert_eq!(with_head(&[0xFF, 0xF9, 0x50, 0x80]), F::AdtsAac);
}

#[test]
fn near_misses_and_short_input_are_unrecognized() {
    assert_eq!(with_head(b""), F::Unknown);
    assert_eq!(with_head(b"RIFF"), F::Unknown);
    assert_eq!(with_head(b"RIFF\0\0\0\0AVI LIST"), F::Unknown);
    assert_eq!(with_head(b"FORM\0\0\0\08SVX"), F::Unknown);
    assert_eq!(with_head(b"schema_version = 5"), F::Unknown);
    assert_eq!(with_head(&[0xFF, 0x00]), F::Unknown);
    assert_eq!(with_head(&[0xFF]), F::Unknown);
}

#[test]
fn pure_measurement_cause_survives_the_recording_error_boundary() {
    use crate::core::diagnostic::Code;
    use std::error::Error;
    let error = super::RecordingError::from(
        auditorylab_core::measurement::MeasurementError::UnsupportedChannels { channels: 3 },
    );
    assert_eq!(error.code(), "measurement.unsupported_channels");
    assert_eq!(error.params()["channels"], 3_u64.into());
    assert_eq!(
        error.source().unwrap().to_string(),
        "measurement.unsupported_channels"
    );
}

#[test]
fn completed_source_extent_rejects_one_frame_overrun_and_overflow() {
    let mut measurement = auditorylab_core::measurement::Measurement::new(1000, 1, 2000).unwrap();
    for _ in 0..2000 {
        measurement.push(&[0.0]).unwrap();
    }
    let samples = measurement.finish(&|| false).unwrap();
    let complete = |source_frames, start_frame| {
        super::Analysis::complete(
            "PCM".into(),
            false,
            source_frames,
            start_frame,
            samples.clone(),
        )
    };
    assert!(complete(Some(3000), 1000).is_ok());
    assert!(matches!(
        complete(Some(2999), 1000),
        Err(super::RecordingError::InvalidWindow)
    ));
    assert!(matches!(
        complete(None, u64::MAX),
        Err(super::RecordingError::InvalidWindow)
    ));
}
