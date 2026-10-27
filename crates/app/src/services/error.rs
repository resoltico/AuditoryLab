//! Aggregate operation and export failures.
use {
    crate::audio::PlaybackError, crate::core::SampleRate, crate::core::SoundError,
    crate::discovery::query::QueryError, crate::document::error::EditorError,
    crate::identity::IdentityError, crate::outcome::Outcome, crate::storage::error::FileError,
    crate::storage::error::SaveError, crate::storage::library::LibraryError,
    crate::storage::load::LoadError,
};
/// A WAV export could not complete.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportError {
    /// Filesystem publication failed.
    File {
        /// Captured error.
        error: FileError,
    },
    /// The selected rate cannot realize the captured sound.
    Sound {
        /// Captured origin.
        origin: Box<crate::document::recipe::Origin>,
        /// Captured rate.
        rate: SampleRate,
        /// Captured error.
        error: SoundError,
    },
}
impl ExportError {
    /// Failure category.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::File { error } => error.outcome(),
            Self::Sound { .. } => Outcome::InvalidSound,
        }
    }
}
impl From<FileError> for ExportError {
    fn from(error: FileError) -> Self {
        Self::File { error }
    }
}
/// A typed completion failure for code coordinating several operation families.
#[derive(Debug, Clone)]
pub enum OperationError {
    /// Portable input or rendition loading failed.
    Load(LoadError),
    /// Managed library operation failed.
    Library(LibraryError),
    /// WAV export failed.
    Export(ExportError),
    /// Device playback failed.
    Playback(PlaybackError),
    /// Portable save failed.
    Save(SaveError),
    /// Editable document creation failed.
    Editor(EditorError),
    /// Identity allocation failed.
    Identity(IdentityError),
    /// Discovery query failed.
    Query(QueryError),
    /// Filesystem adapter failed.
    File(FileError),
    /// A pure portable candidate failed validation.
    Recipe(crate::core::RecipeError),
}
impl OperationError {
    /// Frontend-independent failure category.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::Load(error) => error.outcome(),
            Self::Library(error) => error.outcome(),
            Self::Export(error) => error.outcome(),
            Self::Playback(error) => error.outcome(),
            Self::Save(error) => error.outcome(),
            Self::Editor(error) => error.outcome(),
            Self::Identity(error) => error.outcome(),
            Self::Query(error) => error.outcome(),
            Self::File(error) => error.outcome(),
            Self::Recipe(_) => Outcome::InvalidRecipe,
        }
    }
    /// Owned partial file that cleanup could not remove.
    #[must_use]
    pub fn leftover(&self) -> Option<&std::path::Path> {
        match self {
            Self::File(error) => error.leftover(),
            Self::Save(SaveError::File { error })
            | Self::Export(ExportError::File { error })
            | Self::Library(LibraryError::File { error }) => error.leftover(),
            _ => None,
        }
    }
}
macro_rules! operation_from {
    ($($ty:ty => $variant:ident),* $(,)?) => { $(impl From<$ty> for OperationError { fn from(error: $ty) -> Self { Self::$variant(error) } })* };
}
operation_from! { LoadError => Load, LibraryError => Library, ExportError => Export, PlaybackError => Playback, SaveError => Save, EditorError => Editor, IdentityError => Identity, QueryError => Query, FileError => File, crate::core::RecipeError => Recipe }

impl From<crate::document::origin::OriginError> for OperationError {
    fn from(error: crate::document::origin::OriginError) -> Self {
        Self::Library(LibraryError::from(error))
    }
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::File { .. } => "export.file",
            Self::Sound { .. } => "export.sound",
        })
    }
}
impl std::error::Error for ExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::File { error } => Some(error),
            Self::Sound { error, .. } => Some(error),
        }
    }
}

impl std::fmt::Display for crate::services::OperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::services::OperationError as E;
        match self {
            E::Load(error) => error.fmt(f),
            E::Library(error) => error.fmt(f),
            E::Export(error) => error.fmt(f),
            E::Playback(error) => error.fmt(f),
            E::Save(error) => error.fmt(f),
            E::Editor(error) => error.fmt(f),
            E::Identity(error) => error.fmt(f),
            E::Query(error) => error.fmt(f),
            E::File(error) => error.fmt(f),
            E::Recipe(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for crate::services::OperationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        use crate::services::OperationError as E;
        Some(match self {
            E::Load(error) => error,
            E::Library(error) => error,
            E::Export(error) => error,
            E::Playback(error) => error,
            E::Save(error) => error,
            E::Editor(error) => error,
            E::Identity(error) => error,
            E::Query(error) => error,
            E::File(error) => error,
            E::Recipe(error) => error,
        })
    }
}
