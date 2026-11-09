//! Failures specific to decoding and measuring recordings.
use crate::{outcome::Outcome, services::FileError};

/// A recording could not be measured; failed work never replaces a previous report.
#[derive(Debug, Clone)]
pub enum RecordingError {
    /// Opening or reading the input failed.
    File(FileError),
    /// Pure sample measurement refused or did not complete.
    Measurement(auditorylab_core::measurement::MeasurementError),
    /// Cooperative input work budget exhausted; no partial result is returned.
    BudgetExceeded { resource: &'static str },
    /// A known codec has no enabled decoder.
    UnsupportedCodec { codec: String },
    /// Invalid media or samples, located at the packet being read.
    Decode { packet: u64, cause: String },
    /// The container has no audio track.
    NoAudioTrack,
    /// Fewer than two seconds of samples are available in the requested window.
    TooShort,
    /// The requested analysis length is shorter than two seconds.
    InvalidWindow,
    /// Analysis supports mono and stereo recordings only.
    UnsupportedChannels { channels: usize },
    /// Work was cancelled before a complete report was available.
    Cancelled,
}
impl RecordingError {
    /// Frontend-independent failure category.
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::File(error) => error.input_outcome(),
            Self::Cancelled
            | Self::Measurement(auditorylab_core::measurement::MeasurementError::Cancelled) => {
                Outcome::Cancelled
            }
            Self::InvalidWindow => Outcome::InvalidArgument,
            _ => Outcome::Unreadable,
        }
    }
}
impl From<FileError> for RecordingError {
    fn from(error: FileError) -> Self {
        Self::File(error)
    }
}
impl std::fmt::Display for RecordingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for RecordingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::File(error) => Some(error),
            _ => None,
        }
    }
}
crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    RecordingError {
        codes {
            Self::Measurement { .. } => "recording.measurement",
            Self::BudgetExceeded { .. } => "recording.budget_exceeded",
            Self::UnsupportedCodec { .. } => "recording.unsupported_codec",
            Self::Decode { .. } => "recording.decode",
            Self::NoAudioTrack => "recording.no_audio_track",
            Self::TooShort => "recording.too_short",
            Self::InvalidWindow => "recording.invalid_window",
            Self::UnsupportedChannels { .. } => "recording.unsupported_channels",
            Self::Cancelled => "recording.cancelled",
        }
        delegates { Self::File(error) => error, }
        params(this) {
            match this {
                Self::BudgetExceeded { resource } => {
                    crate::core::diagnostic_params! { "resource" => *resource }
                },
                Self::UnsupportedCodec { codec } => {
                    crate::core::diagnostic_params! { "codec" => codec.as_str() }
                },
                Self::Decode { packet, cause } => {
                    crate::core::diagnostic_params! {
                        "packet" => *packet,
                        "cause" => cause.as_str(),
                    }
                },
                Self::UnsupportedChannels { channels } => {
                    crate::core::diagnostic_params! { "channels" => *channels }
                },
                _ => crate::core::diagnostic::Params::new(),
            }
        }
    }
}

impl From<auditorylab_core::measurement::MeasurementError> for RecordingError {
    fn from(error: auditorylab_core::measurement::MeasurementError) -> Self {
        Self::Measurement(error)
    }
}
