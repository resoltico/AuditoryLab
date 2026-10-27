//! Operation-specific failure data. Presentation supplies human sentences.
use super::{FileError, IoCause, IoFailure, Outcome};
use crate::core::{DocumentUuid, RecipeError, SampleRate, SoundError, StructureError};
use std::path::PathBuf;
macro_rules! error_data {
    ($(#[$meta:meta])* $name:ident { $($(#[$variant_meta:meta])* $variant:ident $( { $($field:ident: $ty:ty),* $(,)? } )?),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        pub enum $name {
            $($(#[$variant_meta])* $variant $( { $(#[doc = concat!("Captured ", stringify!($field), ".")] $field: $ty),* } )?),*
        }
    }
}
/// Identity allocation context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityScope {
    /// Process-local editor documents.
    Document,
    /// Process-local external entries.
    ExternalEntry,
}
/// Secure or process-local identities could not be allocated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// The native entropy source failed.
    Entropy(getrandom::Error),
    /// A process-local counter is exhausted.
    Exhausted(IdentityScope),
}
impl IdentityError {
    /// Failure category.
    #[must_use]
    pub const fn outcome(&self) -> Outcome {
        match self {
            Self::Entropy(_) | Self::Exhausted(IdentityScope::ExternalEntry) => Outcome::Identity,
            Self::Exhausted(IdentityScope::Document) => Outcome::InvalidRecipe,
        }
    }
}
/// An editable document could not be created.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorError {
    /// Identity creation failed.
    Identity(IdentityError),
    /// Portable copy construction failed.
    Recipe(RecipeError),
    /// An editable candidate has invalid owners or capacity.
    Structure(StructureError),
}
impl EditorError {
    /// Failure category.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::Identity(error) => error.outcome(),
            _ => Outcome::InvalidRecipe,
        }
    }
}
impl From<IdentityError> for EditorError {
    fn from(error: IdentityError) -> Self {
        Self::Identity(error)
    }
}
impl From<RecipeError> for EditorError {
    fn from(error: RecipeError) -> Self {
        Self::Recipe(error)
    }
}
impl From<StructureError> for EditorError {
    fn from(error: StructureError) -> Self {
        Self::Structure(error)
    }
}
error_data! {
    /// A persistent library operation failed.
    LibraryError {
        /// Filesystem publication or observation failed.
        File { error: FileError },
        /// A read-only filesystem operation failed.
        Read { error: IoFailure },
        /// A managed record could not be read completely.
        RecordRead { path: PathBuf, error: FileError },
        /// A requested managed load could not establish complete readable state.
        IncompleteRead { error: Box<LibraryError> },
        /// Portable recipe bytes failed their required UTF-8 conversion.
        RecipeUtf8 { path: PathBuf, error: std::string::FromUtf8Error },
        /// A managed recipe failed its exact portable contract.
        RecipeFile { path: PathBuf, error: RecipeError },
        /// A managed filename names a different document identity.
        RecordIdentity { path: PathBuf, expected: DocumentUuid, found: DocumentUuid },
        /// Portable content failed before publication.
        Recipe { error: RecipeError },
        /// The shipped catalog contains invalid portable content.
        InvalidBuiltin { error: RecipeError },
        /// Two shipped recipes declare the same UUID.
        DuplicateBuiltin { id: DocumentUuid },
        /// The selected UUID is reserved by a built-in.
        ReservedBuiltin { id: DocumentUuid },
        /// The library has not been initialized.
        NotInitialized,
        /// The requested managed UUID does not exist.
        UnknownManaged { id: DocumentUuid },
        /// A previously configured root disappeared.
        RootMissing,
        /// Initialization did not produce the expected root.
        RootDisappeared,
        /// Existing content has no supported marker during creation.
        MissingMarkerOnCreate,
        /// Existing content has no supported marker during mutation.
        MissingMarkerOnMutation,
        /// Captured content no longer matches the managed record.
        StaleContent { id: DocumentUuid },
        /// The selected entry disappeared.
        EntryDisappeared { id: DocumentUuid },
        /// This UUID already exists in the active library or Trash.
        IdentityExists { id: DocumentUuid },
        /// The proposed document does not match its captured key.
        ProposedIdentity { expected: DocumentUuid, found: DocumentUuid },
        /// A trashed entry cannot be edited.
        MustRestore { id: DocumentUuid },
        /// Permanent deletion requires a Trash entry.
        PurgeRequiresTrash { id: DocumentUuid },
        /// Permanent deletion lacks explicit confirmation.
        PurgeUnconfirmed,
        /// Updating would replace declared creation history.
        HistoryReplacement { id: DocumentUuid },
        /// Membership text is not UTF-8.
        MembershipUtf8 { error: std::str::Utf8Error },
        /// Membership timestamps do not match the strict shape.
        MembershipFormat { error: toml::de::Error },
        /// Membership timestamps could not be encoded.
        MembershipEncode { error: toml::ser::Error },
        /// Encoded membership exceeds its byte limit.
        MembershipSize { bytes: usize, limit: usize },
        /// Active and Trash paths contain different files for one UUID.
        AmbiguousRecord { id: DocumentUuid },
        /// A library operation was cancelled before publication.
        Cancelled,
        /// A bounded scan exhausted one of its resource limits.
        ScanBudget,
        /// Observed clock cannot be represented by the portable timestamp profile.
        Clock { error: super::ClockError },
        /// A built-in is read-only.
        ReadOnlyBuiltin { id: String },
        /// Temporary entries are not reusable managed selections.
        TemporarySelection,
        /// The selector is not canonical.
        InvalidSelector { value: String },
        /// A fingerprint is not canonical lowercase SHA-256.
        InvalidFingerprint { value: String },
        /// Native per-user application data could not be resolved.
        NativeRootUnavailable,
        /// A configured root is relative.
        RelativeRoot { path: PathBuf },
        /// A lock path is not a regular persistent file.
        LockNotRegular,
        /// A cooperating operation holds the root lock.
        Busy,
        /// Required locking assurance is unavailable.
        LockUnavailable { error: IoCause },
        /// The layout marker is not UTF-8.
        MarkerUtf8 { error: std::str::Utf8Error },
        /// The layout marker has invalid shape.
        MarkerFormat { error: toml::de::Error },
        /// A different persistent layout was found.
        UnsupportedLayout { found: u32 },
        /// Nonempty membership exists without a supported marker.
        MembershipWithoutMarker,
        /// Recipe files exist without a supported marker.
        RecipesWithoutMarker,
        /// Unknown existing root contents prevent initialization.
        UnknownRootContents,
        /// Managed playback requires a captured User source.
        ExpectedUser,
        /// The captured saved selection changed or moved to Trash before playback.
        StalePlayback,
    }
}
impl LibraryError {
    /// Failure category.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::File { error } => error.outcome(),
            Self::Read { .. } => Outcome::Unreadable,
            Self::Recipe { .. } | Self::InvalidBuiltin { .. } | Self::DuplicateBuiltin { .. } => {
                Outcome::InvalidRecipe
            }
            Self::NotInitialized | Self::UnknownManaged { .. } => Outcome::UnknownRecipe,
            Self::NativeRootUnavailable
            | Self::RelativeRoot { .. }
            | Self::TemporarySelection
            | Self::InvalidSelector { .. }
            | Self::InvalidFingerprint { .. }
            | Self::PurgeUnconfirmed
            | Self::ExpectedUser => Outcome::InvalidArgument,
            Self::ReadOnlyBuiltin { .. } => Outcome::ReadOnlyBuiltIn,
            Self::ReservedBuiltin { .. }
            | Self::MissingMarkerOnMutation
            | Self::StaleContent { .. }
            | Self::EntryDisappeared { .. }
            | Self::IdentityExists { .. }
            | Self::ProposedIdentity { .. }
            | Self::MustRestore { .. }
            | Self::PurgeRequiresTrash { .. }
            | Self::StalePlayback => Outcome::Conflict,
            Self::HistoryReplacement { .. } => Outcome::HistoryConflict,
            Self::Busy => Outcome::Busy,
            Self::LockUnavailable { .. } => Outcome::LockUnavailable,
            Self::Cancelled => Outcome::Cancelled,
            Self::RootDisappeared
            | Self::MembershipEncode { .. }
            | Self::MembershipSize { .. }
            | Self::Clock { .. } => Outcome::Write,
            Self::IncompleteRead { .. }
            | Self::RecipeUtf8 { .. }
            | Self::RecordRead { .. }
            | Self::RecipeFile { .. }
            | Self::RecordIdentity { .. }
            | Self::RootMissing
            | Self::MissingMarkerOnCreate
            | Self::MembershipUtf8 { .. }
            | Self::MembershipFormat { .. }
            | Self::AmbiguousRecord { .. }
            | Self::ScanBudget
            | Self::LockNotRegular
            | Self::MarkerUtf8 { .. }
            | Self::MarkerFormat { .. }
            | Self::UnsupportedLayout { .. }
            | Self::MembershipWithoutMarker
            | Self::RecipesWithoutMarker
            | Self::UnknownRootContents => Outcome::IncompleteInventory,
        }
    }
}
impl From<FileError> for LibraryError {
    fn from(error: FileError) -> Self {
        Self::File { error }
    }
}
error_data! {
    /// Portable content could not be saved or published.
    SaveError {
        /// Static portable source resolution failed.
        Load { error: super::LoadError },
        /// The observed clock is outside the portable timestamp profile.
        Clock { error: super::ClockError },
        /// Filesystem publication or observation failed.
        File { error: FileError },
        /// Portable content failed validation or serialization.
        Recipe { error: RecipeError },
        /// Saving would replace identity or creation history.
        HistoryReplacement,
    }
}
impl SaveError {
    /// Failure category.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::File { error } => error.outcome(),
            Self::Recipe { .. } => Outcome::InvalidRecipe,
            Self::HistoryReplacement => Outcome::HistoryConflict,
            Self::Load { error } => error.outcome(),
            Self::Clock { .. } => Outcome::Write,
        }
    }
}
impl From<FileError> for SaveError {
    fn from(error: FileError) -> Self {
        Self::File { error }
    }
}
impl From<RecipeError> for SaveError {
    fn from(error: RecipeError) -> Self {
        Self::Recipe { error }
    }
}
error_data! {
    /// A WAV export could not complete.
    ExportError {
        /// Filesystem publication failed.
        File { error: FileError },
        /// The selected rate cannot realize the captured sound.
        Sound { title: String, rate: SampleRate, error: SoundError },
    }
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
/// Device playback could not prepare or complete.
#[derive(Debug, Clone)]
pub enum PlaybackError {
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
    Load(super::LoadError),
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
error_data! {
    /// A discovery filter or local-date range is invalid.
    QueryError {
        /// The exclusive upper bound is not after the lower bound.
        RangeOrder { from: crate::core::Timestamp, before: crate::core::Timestamp },
        /// A study identity is empty or malformed.
        Study { value: String },
        /// A local date or midnight cannot be resolved uniquely.
        LocalDate { value: String },
        /// A local through date predates its from date.
        LocalOrder { field: crate::discovery::calendar::CalendarField, from: String, through: String },
    }
}
impl QueryError {
    /// Failure category.
    #[must_use]
    pub const fn outcome(&self) -> Outcome {
        Outcome::InvalidArgument
    }
}
impl From<super::ClockError> for LibraryError {
    fn from(error: super::ClockError) -> Self {
        Self::Clock { error }
    }
}
impl From<super::ClockError> for SaveError {
    fn from(error: super::ClockError) -> Self {
        Self::Clock { error }
    }
}

impl From<super::LoadError> for PlaybackError {
    fn from(error: super::LoadError) -> Self {
        Self::Load(error)
    }
}
