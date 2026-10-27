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
        /// Captured title.
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
