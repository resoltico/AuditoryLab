//! Human sentences for typed operation failures.
use super::DisplayText;
use super::core_errors::recipe_error;
use crate::error::{
    EditorError, ExportError, FileError, IdentityError, IdentityScope, IoFailure, IoOperation,
    LibraryError, LoadError, PlaybackError, QueryError, SaveError,
};
use std::{error::Error as StdError, fmt};
fn path_text(path: &std::path::Path) -> String {
    path.to_string_lossy().display_scalar()
}
fn io_text(error: &IoFailure) -> String {
    let action = match error.action {
        IoOperation::Open => "cannot open file",
        IoOperation::Read => "cannot read file",
        IoOperation::Write => "cannot write file",
        IoOperation::Seek => "cannot seek file",
        IoOperation::Inspect => "cannot inspect file",
        IoOperation::CreateDirectory => "cannot create directory",
        IoOperation::OpenDirectory => "cannot open directory",
        IoOperation::CreateStage => "cannot create stage",
        IoOperation::SyncFile => "cannot sync file",
        IoOperation::SyncDirectory => "cannot sync directory",
        IoOperation::Remove => "cannot remove file",
        IoOperation::Publish => "cannot publish destination",
        IoOperation::Relocate => "cannot relocate entry",
        IoOperation::ResolvePath => "cannot resolve opened external file",
        IoOperation::RetainIdentity => "cannot retain external file identity",
        IoOperation::CheckIdentity => "cannot check opened path identity",
        IoOperation::SetPermissions => "cannot set file permissions",
    };
    error.path.as_ref().map_or_else(
        || format!("{action}: {}", error.cause),
        |path| format!("{}: {action}: {}", path_text(path), error.cause),
    )
}
fn file_text(error: &FileError) -> String {
    match error {
        FileError::Io(error) => io_text(error),
        FileError::Identity(error) => identity_text(error),
        FileError::NotRegular { path } => format!("{}: expected a regular file", path_text(path)),
        FileError::ReadLimit { path, grew } => format!(
            "{}: file {} the bounded read limit",
            path_text(path),
            if *grew { "grew beyond" } else { "exceeds" }
        ),
        FileError::Cancelled { path } => path.as_ref().map_or_else(
            || "file operation cancelled before publication".into(),
            |path| format!("publication to {} cancelled before commit", path_text(path)),
        ),
        FileError::DestinationExists { path, cause } => format!(
            "{}: destination {} and was left untouched",
            path_text(path),
            if cause.is_some() {
                "was created by another writer"
            } else {
                "already exists"
            }
        ),
        FileError::Changed { path } => format!(
            "{}: file changed identity or bytes since it was opened",
            path_text(path)
        ),
        FileError::MissingFilename => "destination needs a filename".into(),
        FileError::MissingParent => "external destination needs a parent".into(),
        FileError::RelocationExists { path } => format!(
            "{}: relocation destination already exists and was left untouched",
            path_text(path)
        ),
        FileError::AfterMutation {
            phase,
            path,
            confirmation,
            cause,
        } => {
            use crate::error::{Confirmation as C, MutationPhase as P};
            let explanation = match (phase, confirmation) {
                (P::Publish, C::Unchanged) => "publication failed; previous content remains".into(),
                (P::Publish, C::Different) => "publication failed and destination differs from both previous and candidate bytes; inspect this path before retrying".into(),
                (P::Publish, C::Unavailable(error)) => format!("publication outcome is unconfirmed; destination could not be verified ({error}); inspect this path before retrying"),
                (P::SyncPublication, C::Unchanged) => "directory sync failed and previous bytes remain".into(),
                (P::SyncPublication, C::Different) => "directory sync failed and candidate bytes could not be verified; inspect this path".into(),
                (P::SyncPublication, C::Unavailable(error)) => format!("directory sync outcome is unconfirmed; destination could not be verified ({error}); inspect this path"),
                (P::Relocate { .. }, C::Unchanged) => "relocation did not remove its source; Trash remains authoritative for same-file links".into(),
                (P::Relocate { source }, C::Unavailable(error)) => format!("relocation outcome is unconfirmed; source {} could not be inspected ({error}); recheck before retrying", path_text(source)),
                (P::SyncRelocation, C::Different) => "relocation durability failed and destination changed".into(),
                (P::SyncRelocation, C::Unavailable(error)) => format!("relocation durability is unconfirmed; destination could not be verified ({error}); recheck before retrying"),
                (P::Remove, C::Unchanged) => "deletion was not applied; the destination remains".into(),
                (P::Remove, C::Unavailable(error)) => format!("deletion outcome is unconfirmed; destination could not be inspected ({error}); recheck before retrying"),
                (P::SyncRemoval, C::Different) => "deletion durability failed and the destination is present".into(),
                (P::SyncRemoval, C::Unavailable(error)) => format!("deletion durability is unconfirmed; destination could not be inspected ({error}); recheck before retrying"),
                // These combinations are not emitted by the adapter. Retain conservative
                // wording for public error values without claiming a verified outcome.
                (_, C::Different) => "expected state was not found; inspect this path before retrying".into(),
                (_, C::Unchanged) => "previous state remains".into(),
            };
            format!("{}: {explanation}: {cause}", path_text(path))
        }

        FileError::Cleanup {
            primary,
            path,
            cause,
        } => format!(
            "{primary}; a partial file remains at {}: {cause}",
            path_text(path)
        ),
    }
}
fn load_text(error: &LoadError) -> String {
    match error {
        LoadError::TemporarySelector { .. } => "temporary entries have no reusable selector".into(),
        LoadError::Identity(error) => identity_text(error),
        LoadError::UnknownBuiltin { id } => format!("unknown built-in recipe \"{id}\""),
        LoadError::Recipe { origin, error } => format!("{origin}: {}", recipe_error(error)),
        LoadError::Read { origin, cause } => format!("cannot read {origin}: {cause}"),
        LoadError::File(error) => file_text(error),
        LoadError::Library(error) => library_text(error),
        LoadError::Sound {
            origin,
            rate,
            error,
        } => format!(
            "{origin} at {} Hz: {}",
            rate.hz(),
            super::acoustic_errors::sound_error(error)
        ),
    }
}
fn library_text(error: &LibraryError) -> String {
    use LibraryError as E;
    match error {
        E::File { error } => file_text(error), E::Read { error } => io_text(error),
        E::IncompleteRead { error } => library_text(error),
        E::RecipeUtf8 { path, .. } => format!("{}: recipe is not UTF-8", path_text(path)),
        E::RecordRead { path, error } => format!("{}: {error}", path_text(path)),
        E::RecipeFile { path, error } => format!("{}: {}; file was left intact", path_text(path), recipe_error(error)),
        E::RecordIdentity { path, expected, found } => format!("{}: filename UUID {expected} does not match document.id {found}", path_text(path)),
        E::Recipe { error } => recipe_error(error),
        E::InvalidBuiltin { error } => format!("invalid shipped identity: {}", recipe_error(error)),
        E::DuplicateBuiltin { id } => format!("duplicate shipped UUID {id}"),
        E::ReservedBuiltin { id } => format!("document UUID {id} belongs to a built-in; explicitly create a copy"),
        E::NotInitialized => "library is not initialized".into(), E::UnknownManaged { id } => format!("unknown managed recipe user:{id}"),
        E::RootMissing => "configured root is missing; initialize it explicitly".into(), E::RootDisappeared => "initialization did not create a root".into(),
        E::MissingMarkerOnCreate | E::MissingMarkerOnMutation => "library is not initialized; existing content was not recreated".into(),
        E::StaleContent { id } => format!("saved recipe changed or disappeared (user:{id}); retain the draft and reload or save a copy"),
        E::EntryDisappeared { id } => format!("selected entry user:{id} disappeared; it was not recreated"),
        E::IdentityExists { id } => format!("identity already exists (user:{id}), including Trash; restore or create an independent copy"),
        E::ProposedIdentity { expected, found } => format!("proposed document identity {found} differs from selected user:{expected}"),
        E::MustRestore { id } => format!("restore trashed entry user:{id} before editing it"),
        E::PurgeRequiresTrash { id } => format!("only entries in Trash can be permanently deleted: user:{id}"),
        E::PurgeUnconfirmed => "permanent deletion requires explicit confirmation".into(),
        E::HistoryReplacement { id } => format!("an update cannot replace user:{id}'s creation history"),
        E::MembershipUtf8 { .. } => "membership.toml is not UTF-8".into(), E::MembershipFormat { .. } => "invalid membership.toml timestamps".into(),
        E::MembershipEncode { error } => format!("cannot encode membership timestamps: {error}"),
        E::MembershipSize { bytes, limit } => format!("membership timestamps contain {bytes} bytes, exceeding their {limit}-byte limit"),
        E::AmbiguousRecord { id } => format!("user:{id}: different recipe files exist in recipes and trash; both were retained"),
        E::Cancelled => "library operation cancelled before publication".into(), E::ScanBudget => "library scan budget exhausted; inventory is incomplete".into(),
        E::Clock { .. } => "observed system time is outside the supported UTC timestamp range".into(),
        E::ReadOnlyBuiltin { id } => format!("shipped recipe builtin:{id} cannot be renamed, replaced, trashed or purged; edit a copy"),
        E::TemporarySelection => "temporary files must be explicitly added or copied before managed operations".into(),
        E::InvalidSelector { value } => format!("invalid selector {value}; expected builtin:<catalog-id> or user:<canonical-uuid>"),
        E::InvalidFingerprint { .. } => "expected a canonical 64-digit lowercase SHA-256 fingerprint".into(),
        E::NativeRootUnavailable => "cannot resolve native per-user application data; configure an absolute library directory".into(),
        E::RelativeRoot { path } => format!("library directory {} must be absolute; no fallback was used", path_text(path)),
        E::LockNotRegular => "library lock must be a regular persistent file".into(), E::Busy => "Library busy; retry after the other operation finishes".into(),
        E::LockUnavailable { error } => format!("required library locking assurance is unavailable; no unlocked fallback: {error}"),
        E::MarkerUtf8 { .. } => "layout marker is not UTF-8".into(), E::MarkerFormat { .. } => "invalid library.toml layout marker".into(),
        E::UnsupportedLayout { found } => format!("unsupported library layout {found}; this build reads {}", crate::storage::library::LIBRARY_LAYOUT),
        E::MembershipWithoutMarker => "nonempty membership exists without a layout marker; initialization left it intact".into(),
        E::RecipesWithoutMarker => "recipes exist without a supported marker; initialization left them unchanged".into(),
        E::UnknownRootContents => "unknown existing root contents; initialization refused without changing them".into(),
        E::ExpectedUser => "expected a captured User selection".into(),
        E::StalePlayback => "Saved selection changed or moved to Trash. Refresh before starting playback; no audio was started.".into(),
    }
}
fn identity_text(error: &IdentityError) -> String {
    match error {
        IdentityError::Entropy(error) => format!("cannot allocate a random UUID: {error}"),
        IdentityError::Exhausted(IdentityScope::Document) => "document identities exhausted".into(),
        IdentityError::Exhausted(IdentityScope::ExternalEntry) => {
            "temporary entry identities exhausted".into()
        }
    }
}
fn editor_text(error: &EditorError) -> String {
    match error {
        EditorError::Identity(error) => identity_text(error),
        EditorError::Recipe(error) => recipe_error(error),
    }
}
fn save_text(error: &SaveError) -> String {
    match error { SaveError::Clock { error } => error.to_string(), SaveError::Load { error } => load_text(error), SaveError::File { error } => file_text(error), SaveError::Recipe { error } => recipe_error(error), SaveError::HistoryReplacement => "external Save must retain identity and creation history; save an independent copy instead".into() }
}
fn export_text(error: &ExportError) -> String {
    match error {
        ExportError::File { error } => file_text(error),
        ExportError::Sound {
            origin,
            rate,
            error,
        } => {
            format!(
                "{origin} at {} Hz: {}",
                rate.hz(),
                super::acoustic_errors::sound_error(error)
            )
        }
    }
}
fn playback_text(error: &PlaybackError) -> String {
    match error {
        PlaybackError::Trashed { .. } => {
            "trashed recipes can be inspected/exported; restore before playback".into()
        }
        PlaybackError::Device(error) => error.to_string(),
        PlaybackError::Sound { title, rate, error } => {
            format!(
                "{title} at {} Hz: {}",
                rate.hz(),
                super::acoustic_errors::sound_error(error)
            )
        }
        PlaybackError::Library(error) => library_text(error),
        PlaybackError::Load(error) => load_text(error),
        PlaybackError::MissingTerminalOutcome => {
            "stopped preparation has no terminal outcome".into()
        }
        PlaybackError::MonitorInUse => "monitor reader is already in use".into(),
        PlaybackError::MissingDocument => "a live session needs a committed document".into(),
    }
}
fn query_text(error: &QueryError) -> String {
    match error {
        QueryError::TemporaryFilesScope { .. } => {
            "temporary inputs require the All collection".into()
        }
        QueryError::RangeOrder { .. } => "date range requires before strictly after from".into(),
        QueryError::Study { .. } => {
            "study must be a nonempty source key or valid DOI identity".into()
        }
        QueryError::LocalDate { .. } => {
            "Use YYYY-MM-DD dates with a unique local midnight and an ordered range.".into()
        }
        QueryError::LocalOrder {
            field,
            from,
            through,
        } => format!(
            "{} through ({through}) must be on or after {} from ({from}).",
            field.label(),
            field.label()
        ),
    }
}
macro_rules! displays {
    ($($ty:ty => $formatter:ident),* $(,)?) => { $(impl fmt::Display for $ty { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&$formatter(self)) } })* };
}
displays! { FileError => file_text, LoadError => load_text, LibraryError => library_text, IdentityError => identity_text, EditorError => editor_text, SaveError => save_text, ExportError => export_text, PlaybackError => playback_text, QueryError => query_text }
impl StdError for FileError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) => Some(error.cause.error()),
            Self::Identity(error) => Some(error),
            Self::DestinationExists {
                cause: Some(error), ..
            }
            | Self::AfterMutation { cause: error, .. } => Some(error.error()),
            Self::Cleanup { primary, .. } => Some(primary.as_ref()),
            _ => None,
        }
    }
}
impl StdError for LoadError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Recipe { error, .. } => Some(error.as_ref()),
            Self::Read { cause, .. } => Some(cause.error()),
            Self::File(error) => Some(error),
            Self::Library(error) => Some(error),
            Self::Sound { error, .. } => Some(error),
            Self::UnknownBuiltin { .. } | Self::TemporarySelector { .. } => None,
            Self::Identity(error) => Some(error),
        }
    }
}
impl StdError for IdentityError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Entropy(error) => Some(error),
            Self::Exhausted(_) => None,
        }
    }
}
impl StdError for EditorError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Identity(error) => Some(error),
            Self::Recipe(error) => Some(error),
        }
    }
}
impl StdError for LibraryError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
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
impl StdError for SaveError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::File { error } => Some(error),
            Self::Recipe { error } => Some(error),
            Self::Load { error } => Some(error),
            Self::Clock { error } => Some(error),
            Self::HistoryReplacement => None,
        }
    }
}
impl StdError for ExportError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::File { error } => Some(error),
            Self::Sound { error, .. } => Some(error),
        }
    }
}
impl StdError for PlaybackError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Device(error) => Some(error),
            Self::Sound { error, .. } => Some(error),
            Self::Library(error) => Some(error),
            Self::Load(error) => Some(error),
            _ => None,
        }
    }
}
impl StdError for QueryError {}

impl fmt::Display for crate::error::OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use crate::error::OperationError as E;
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
            E::Recipe(error) => f.write_str(&recipe_error(error)),
        }
    }
}
impl StdError for crate::error::OperationError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        use crate::error::OperationError as E;
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

impl fmt::Display for crate::error::ClockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("observed system time is outside the supported UTC timestamp range")
    }
}
impl StdError for crate::error::ClockError {}

impl crate::storage::library::LibraryDiagnostic {
    /// Render the retained typed cause or inventory notice.
    #[must_use]
    pub fn message(&self) -> String {
        match &self.detail {
            crate::storage::library::DiagnosticDetail::Failure(error) => error.to_string(),
            crate::storage::library::DiagnosticDetail::Notice(message) => message.clone(),
        }
    }
}
impl serde::Serialize for crate::storage::library::LibraryDiagnostic {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut output = serializer.serialize_struct("LibraryDiagnostic", 3)?;
        output.serialize_field("storage_id", &self.storage_id)?;
        output.serialize_field("code", &self.code)?;
        output.serialize_field("message", &self.message())?;
        output.end()
    }
}

impl fmt::Display for crate::audio::device::DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDefaultOutput => write!(f, "no default audio output device found"),
            Self::Backend(error) if error.kind() == cpal::ErrorKind::DeviceChanged => write!(
                f,
                "the audio output route changed and playback stopped; select the intended output and play again"
            ),
            Self::Backend(error) => write!(f, "audio backend: {error}"),
            Self::UnsupportedSampleFormat(format) => write!(
                f,
                "the output device's {format} sample format is not supported"
            ),
            Self::UnsupportedSampleRate { hz } => write!(
                f,
                "the output device's {hz} Hz sample rate is not supported"
            ),
            Self::TooFewChannels { channels } => write!(
                f,
                "the default output device has {channels} channel{}; stimuli need two, one for each ear",
                if *channels == 1 { "" } else { "s" }
            ),
            Self::InvalidTiming => write!(f, "the audio backend supplied invalid playback timing"),
            Self::MisalignedBuffer => write!(
                f,
                "the audio backend supplied a buffer holding a partial frame"
            ),
            Self::TimedOut => write!(
                f,
                "the audio stream stopped making progress or outlasted its time limit"
            ),
            Self::Abandoned => write!(f, "the playback session ended before its stream started"),
        }
    }
}

impl std::fmt::Display for crate::audio::session::LiveUpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionClosed => f.write_str("the live session no longer accepts changes"),
            Self::RateMismatch {
                target_hz,
                session_hz,
            } => write!(
                f,
                "the change was checked at {target_hz} Hz but the live session runs at {session_hz} Hz"
            ),
            Self::RevisionLimit => f.write_str("the live session's revision limit is reached"),
            Self::Unavailable => f.write_str("live control is unavailable"),
        }
    }
}
impl std::fmt::Display for crate::document::recipe::Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuiltIn(id) => write!(f, "{id}"),
            Self::User { id, .. } => write!(f, "user:{id}"),
            Self::File(path) => f.write_str(&path_text(path)),
            Self::Stream(label) | Self::Draft(label) => f.write_str(&label.display_scalar()),
        }
    }
}
