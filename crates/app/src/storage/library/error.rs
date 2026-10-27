//! Managed-library failures.
use std::path::PathBuf;
use {
    crate::core::DocumentUuid, crate::core::RecipeError, crate::outcome::Outcome,
    crate::storage::error::FileError, crate::storage::error::IoCause,
    crate::storage::error::IoFailure,
};
/// A persistent library operation failed.
#[derive(Debug, Clone, PartialEq)]
pub enum LibraryError {
    /// Origin parsing or selection failed.
    Origin {
        error: crate::document::origin::OriginError,
    },
    /// Filesystem publication or observation failed.
    File { error: FileError },
    /// A read-only filesystem operation failed.
    Read { error: IoFailure },
    /// A managed record could not be read completely.
    RecordRead { path: PathBuf, error: FileError },
    /// A requested managed load could not establish complete readable state.
    IncompleteRead { error: Box<LibraryError> },
    /// Portable recipe bytes failed their required UTF-8 conversion.
    RecipeUtf8 {
        path: PathBuf,
        error: std::string::FromUtf8Error,
    },
    /// A managed recipe failed its exact portable contract.
    RecipeFile { path: PathBuf, error: RecipeError },
    /// A managed filename names a different document identity.
    RecordIdentity {
        path: PathBuf,
        expected: DocumentUuid,
        found: DocumentUuid,
    },
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
    ProposedIdentity {
        expected: DocumentUuid,
        found: DocumentUuid,
    },
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
    Clock { error: crate::clock::ClockError },
    /// A built-in is read-only.
    ReadOnlyBuiltin { id: String },
    /// Temporary entries are not reusable managed selections.
    TemporarySelection,

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
impl LibraryError {
    /// Failure category.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::File { error } => error.outcome(),
            Self::Read { .. } => Outcome::Unreadable,
            Self::Origin { error } => error.outcome(),
            Self::Recipe { .. } | Self::InvalidBuiltin { .. } | Self::DuplicateBuiltin { .. } => {
                Outcome::InvalidRecipe
            }
            Self::NotInitialized | Self::UnknownManaged { .. } => Outcome::UnknownRecipe,
            Self::NativeRootUnavailable
            | Self::RelativeRoot { .. }
            | Self::TemporarySelection
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
impl From<crate::clock::ClockError> for LibraryError {
    fn from(error: crate::clock::ClockError) -> Self {
        Self::Clock { error }
    }
}
impl From<crate::document::origin::OriginError> for LibraryError {
    fn from(error: crate::document::origin::OriginError) -> Self {
        Self::Origin { error }
    }
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Origin { .. } => "library.origin",
            Self::File { .. } => "library.file",
            Self::Read { .. } => "library.read",
            Self::RecordRead { .. } => "library.record_read",
            Self::IncompleteRead { .. } => "library.incomplete_read",
            Self::RecipeUtf8 { .. } => "library.recipe_utf8",
            Self::RecipeFile { .. } => "library.recipe_file",
            Self::RecordIdentity { .. } => "library.record_identity",
            Self::Recipe { .. } => "library.recipe",
            Self::InvalidBuiltin { .. } => "library.invalid_builtin",
            Self::DuplicateBuiltin { .. } => "library.duplicate_builtin",
            Self::ReservedBuiltin { .. } => "library.reserved_builtin",
            Self::NotInitialized => "library.not_initialized",
            Self::UnknownManaged { .. } => "library.unknown_managed",
            Self::RootMissing => "library.root_missing",
            Self::RootDisappeared => "library.root_disappeared",
            Self::MissingMarkerOnCreate => "library.missing_marker_on_create",
            Self::MissingMarkerOnMutation => "library.missing_marker_on_mutation",
            Self::StaleContent { .. } => "library.stale_content",
            Self::EntryDisappeared { .. } => "library.entry_disappeared",
            Self::IdentityExists { .. } => "library.identity_exists",
            Self::ProposedIdentity { .. } => "library.proposed_identity",
            Self::MustRestore { .. } => "library.must_restore",
            Self::PurgeRequiresTrash { .. } => "library.purge_requires_trash",
            Self::PurgeUnconfirmed => "library.purge_unconfirmed",
            Self::HistoryReplacement { .. } => "library.history_replacement",
            Self::MembershipUtf8 { .. } => "library.membership_utf8",
            Self::MembershipFormat { .. } => "library.membership_format",
            Self::MembershipEncode { .. } => "library.membership_encode",
            Self::MembershipSize { .. } => "library.membership_size",
            Self::AmbiguousRecord { .. } => "library.ambiguous_record",
            Self::Cancelled => "library.cancelled",
            Self::ScanBudget => "library.scan_budget",
            Self::Clock { .. } => "library.clock",
            Self::ReadOnlyBuiltin { .. } => "library.read_only_builtin",
            Self::TemporarySelection => "library.temporary_selection",
            Self::NativeRootUnavailable => "library.native_root_unavailable",
            Self::RelativeRoot { .. } => "library.relative_root",
            Self::LockNotRegular => "library.lock_not_regular",
            Self::Busy => "library.busy",
            Self::LockUnavailable { .. } => "library.lock_unavailable",
            Self::MarkerUtf8 { .. } => "library.marker_utf8",
            Self::MarkerFormat { .. } => "library.marker_format",
            Self::UnsupportedLayout { .. } => "library.unsupported_layout",
            Self::MembershipWithoutMarker => "library.membership_without_marker",
            Self::RecipesWithoutMarker => "library.recipes_without_marker",
            Self::UnknownRootContents => "library.unknown_root_contents",
            Self::ExpectedUser => "library.expected_user",
            Self::StalePlayback => "library.stale_playback",
        })
    }
}
impl std::error::Error for LibraryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Origin { error } => Some(error),
            Self::File { error } | Self::RecordRead { error, .. } => Some(error),
            Self::Read { error } => Some(error.cause.error()),
            Self::IncompleteRead { error } => Some(error.as_ref()),
            Self::RecipeUtf8 { error, .. } => Some(error),
            Self::RecipeFile { error, .. }
            | Self::Recipe { error }
            | Self::InvalidBuiltin { error } => Some(error),
            Self::MembershipUtf8 { error } | Self::MarkerUtf8 { error } => Some(error),
            Self::MembershipFormat { error } | Self::MarkerFormat { error } => Some(error),
            Self::MembershipEncode { error } => Some(error),
            Self::Clock { error } => Some(error),
            Self::LockUnavailable { error } => Some(error.error()),
            _ => None,
        }
    }
}
