//! Recordings opened for analysis: the file's identity and container format.
//!
//! The format comes from the leading signature bytes alone; nothing is decoded.
use auditorylab_core::vocabulary;
use std::path::PathBuf;

/// A container format recognized from a file's leading bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContainerFormat {
    /// RIFF, RF64 or BW64 WAVE.
    Wav,
    /// AIFF or AIFF-C.
    Aiff,
    /// Native FLAC.
    Flac,
    /// MPEG audio frames or an ID3-tagged stream.
    Mp3,
    /// AAC in ADTS frames.
    AdtsAac,
    /// An ISO base media file, such as M4A or MP4.
    Mp4,
    /// An Ogg stream, such as Vorbis or Opus.
    Ogg,
    /// A Matroska file, including `WebM`.
    Matroska,
    /// No recognized signature.
    Unknown,
}
vocabulary! {ContainerFormat,"container_format" {
    Wav => ("wav","WAV","WAV"),
    Aiff => ("aiff","AIFF","AIFF"),
    Flac => ("flac","FLAC","FLAC"),
    Mp3 => ("mp3","MP3","MP3"),
    AdtsAac => ("adts_aac","AAC (ADTS)","AAC (ADTS)"),
    Mp4 => ("mp4","MPEG-4 (M4A/MP4)","MPEG-4 (M4A/MP4)"),
    Ogg => ("ogg","Ogg","Ogg"),
    Matroska => ("matroska","Matroska/WebM","Matroska/WebM"),
    Unknown => ("unknown","Unrecognized format","unrecognized format"),
}}

impl ContainerFormat {
    /// File-name extensions usually given to this format, without the dot.
    #[must_use]
    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Wav => &["wav", "wave", "bwf"],
            Self::Aiff => &["aif", "aiff", "aifc"],
            Self::Flac => &["flac"],
            Self::Mp3 => &["mp3"],
            Self::AdtsAac => &["aac"],
            Self::Mp4 => &["m4a", "mp4", "m4b"],
            Self::Ogg => &["ogg", "oga", "opus"],
            Self::Matroska => &["webm", "mka", "mkv"],
            Self::Unknown => &[],
        }
    }
    /// Leading bytes read to recognize a format.
    pub const SIGNATURE_BYTES: usize = 12;

    /// Recognize a format from a file's leading bytes; shorter input is allowed.
    #[must_use]
    pub fn from_signature(head: &[u8]) -> Self {
        let at = |start: usize, tag: &[u8]| head.get(start..start + tag.len()) == Some(tag);
        if (at(0, b"RIFF") || at(0, b"RF64") || at(0, b"BW64")) && at(8, b"WAVE") {
            Self::Wav
        } else if at(0, b"FORM") && (at(8, b"AIFF") || at(8, b"AIFC")) {
            Self::Aiff
        } else if at(0, b"fLaC") {
            Self::Flac
        } else if at(0, b"OggS") {
            Self::Ogg
        } else if at(0, &[0x1A, 0x45, 0xDF, 0xA3]) {
            Self::Matroska
        } else if at(4, b"ftyp") {
            Self::Mp4
        } else if at(0, b"ID3") {
            Self::Mp3
        } else if let [0xFF, second, ..] = head
            && second & 0xE0 == 0xE0
        {
            // MPEG frame sync; ADTS sets the two layer bits to zero.
            if second & 0x06 == 0 {
                Self::AdtsAac
            } else {
                Self::Mp3
            }
        } else {
            Self::Unknown
        }
    }
}

/// An opened recording: where it is, how large it is and what it contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingFile {
    /// The path the recording was opened from.
    pub path: PathBuf,
    /// File size in bytes when it was opened.
    pub bytes: u64,
    /// Container format recognized from the leading bytes.
    pub format: ContainerFormat,
}

#[cfg(test)]
mod tests;
