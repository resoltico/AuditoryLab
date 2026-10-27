//! Application audio responsibilities.
pub(crate) mod device;
pub(crate) mod session;

use crate::{
    core::{SampleRate, SoundError},
    outcome::Outcome,
    storage::library::LibraryError,
};
/// Device playback could not prepare or complete.
#[derive(Debug, Clone)]
pub enum PlaybackError {
    /// A captured Trash record must be restored before playback.
    Trashed {
        /// Requested managed identity.
        key: crate::document::origin::LibraryKey,
    },
    /// The native device adapter failed.
    Device(crate::audio::device::DeviceError),
    /// The sound is invalid at the actual device rate.
    Sound {
        title: String,
        /// Actual device rate.
        rate: SampleRate,
        /// Acoustic failure.
        error: SoundError,
    },
    /// Captured rendition could not be loaded at the device rate.
    Load(crate::storage::load::LoadError),
    /// Managed source revalidation failed.
    Library(LibraryError),
    /// Stopped preparation produced no terminal outcome.
    MissingTerminalOutcome,
    /// The monitoring reader has already been taken.
    MonitorInUse,
    /// Live preparation has no committed document.
    MissingDocument,
}
impl PlaybackError {
    /// Failure category.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::Sound { .. } | Self::MissingDocument => Outcome::InvalidSound,
            Self::Trashed { .. } => Outcome::InvalidArgument,
            Self::Library(error) => error.outcome(),
            Self::Load(error) => error.outcome(),
            Self::Device(_) | Self::MissingTerminalOutcome | Self::MonitorInUse => Outcome::Device,
        }
    }
}
impl From<LibraryError> for PlaybackError {
    fn from(error: LibraryError) -> Self {
        Self::Library(error)
    }
}
impl From<crate::storage::load::LoadError> for PlaybackError {
    fn from(error: crate::storage::load::LoadError) -> Self {
        Self::Load(error)
    }
}

impl From<crate::document::error::RenditionError> for PlaybackError {
    fn from(error: crate::document::error::RenditionError) -> Self {
        Self::Load(error.into())
    }
}

impl std::fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::Code::code(self))
    }
}
impl std::error::Error for PlaybackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Device(error) => Some(error),
            Self::Sound { error, .. } => Some(error),
            Self::Library(error) => Some(error),
            Self::Load(error) => Some(error),
            _ => None,
        }
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    PlaybackError {
        codes {
            Self::Trashed { .. } => "playback.trashed",
            Self::MissingTerminalOutcome => "playback.missing_terminal_outcome",
            Self::MonitorInUse => "playback.monitor_in_use",
            Self::MissingDocument => "playback.missing_document",
        }
        delegates {
            Self::Device(error) => error,
            Self::Load(error) => error,
            Self::Library(error) => error,
            Self::Sound { error, .. } => error,
        }
        params(this) {
            match this {
                Self::Trashed { key } => crate::core::diagnostic_params! {"selector"=>key.to_string()},
                Self::Sound { title, rate, .. } => {
                    crate::core::diagnostic_params! {"title"=>title.clone(),"diagnostic_rate_hz"=>rate.hz()}
                }
                _ => crate::core::Params::new(),
            }
        }
    }
}
