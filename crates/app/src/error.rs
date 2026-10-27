//! Typed operation failures and frontend-independent outcome categories.
mod clock;
mod families;
mod file;
mod load;
mod native;
pub use clock::ClockError;
pub use families::{
    EditorError, ExportError, IdentityError, IdentityScope, LibraryError, PlaybackError,
    QueryError, SaveError,
};
pub use file::{Confirmation, FileError, IoFailure, IoOperation, MutationPhase};
pub use load::LoadError;
pub use native::IoCause;
/// A failure category for frontend exit codes and presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Unknown built-in recipe ID.
    UnknownRecipe,
    /// Input could not be read.
    Unreadable,
    /// Invalid recipe document: format, schema, non-finite numbers or size.
    InvalidRecipe,
    /// Invalid signal parameters.
    InvalidSound,
    /// Secure portable identity generation failed; no fallback identity is invented.
    Identity,
    /// Device failure.
    Device,
    /// Destination was left untouched.
    DestinationExists,
    /// Publication failure.
    Write,
    /// Invalid command/configuration argument.
    InvalidArgument,
    /// A selected built-in cannot be mutated.
    ReadOnlyBuiltIn,
    /// Expected record content/revision no longer matches.
    Conflict,
    /// Incoming creation history differs from the stored identity's history.
    HistoryConflict,
    /// A cooperating reader/writer currently owns the root lock.
    Busy,
    /// Required filesystem locking assurance is unavailable.
    LockUnavailable,
    /// Discovery or a requested managed record cannot be read completely.
    IncompleteInventory,
    /// A file operation was cancelled before publication.
    Cancelled,
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
