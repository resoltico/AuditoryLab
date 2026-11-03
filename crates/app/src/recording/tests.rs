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
