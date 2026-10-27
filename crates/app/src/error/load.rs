//! Loading failures preserve source identity and portable-format errors.
use super::{FileError, IoCause, LibraryError, Outcome};
use crate::{
    core::{RecipeError, SampleRate, SoundError},
    document::recipe::Origin,
};
/// A recipe or rendition could not be loaded.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// The built-in selector is unknown.
    UnknownBuiltin {
        /// Requested catalog ID.
        id: String,
    },
    /// Input failed its portable contract.
    Recipe {
        /// Captured source attribution.
        origin: Origin,
        /// Exact format/contract/version failure.
        error: Box<RecipeError>,
    },
    /// A bounded stream could not be read.
    Read {
        /// Captured stream or file attribution.
        origin: Origin,
        /// Original read failure.
        cause: IoCause,
    },
    /// A file could not be opened or observed.
    File(FileError),
    /// Managed source resolution failed.
    Library(LibraryError),
    /// A snapshot cannot be realized at the selected rate.
    Sound {
        /// Captured source attribution.
        origin: Origin,
        /// Chosen rendering rate.
        rate: SampleRate,
        /// Acoustic failure.
        error: SoundError,
    },
}
impl LoadError {
    /// Failure category, independent of frontend exit policy.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::UnknownBuiltin { .. } => Outcome::UnknownRecipe,
            Self::Recipe { .. } => Outcome::InvalidRecipe,
            Self::Read { .. } => Outcome::Unreadable,
            Self::File(FileError::Io(_) | FileError::NotRegular { .. }) => Outcome::Unreadable,
            Self::File(error) => error.outcome(),
            Self::Library(error) => error.outcome(),
            Self::Sound { .. } => Outcome::InvalidSound,
        }
    }
}
impl From<FileError> for LoadError {
    fn from(error: FileError) -> Self {
        Self::File(error)
    }
}
impl From<LibraryError> for LoadError {
    fn from(error: LibraryError) -> Self {
        Self::Library(error)
    }
}
