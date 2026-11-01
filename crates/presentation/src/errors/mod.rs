//! Human sentences for typed operation failures.

use crate::errors::core::recipe_error;
use {
    auditorylab_app::discovery::query::QueryError, auditorylab_app::document::error::EditorError,
    auditorylab_app::identity::IdentityError, auditorylab_app::identity::IdentityScope,
    auditorylab_app::services::ExportError, auditorylab_app::services::FileError,
    auditorylab_app::services::IoFailure, auditorylab_app::services::IoOperation,
    auditorylab_app::services::LoadError, auditorylab_app::services::SaveError,
    auditorylab_app::services::library::LibraryError,
    auditorylab_app::services::playback::PlaybackError,
};
fn path_text(path: &std::path::Path) -> String {
    crate::display_scalar(&(path.to_string_lossy()))
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
    let cause = error.cause.error();
    error.path.as_ref().map_or_else(
        || format!("{action}: {cause}"),
        |path| format!("{}: {action}: {}", path_text(path), cause),
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
        FileError::InsufficientSpace {
            path,
            needed,
            available,
        } => format!(
            "{}: the file needs {} but only {} is free on that volume; nothing was written",
            path_text(path),
            crate::numbers::file_size(*needed),
            crate::numbers::file_size(*available)
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
            let explanation = mutation_explanation(phase, confirmation);
            format!(
                "{}: {explanation}: {cause}",
                path_text(path),
                cause = cause.explain()
            )
        }

        FileError::Cleanup {
            primary,
            path,
            cause,
        } => format!(
            "{}; a partial file remains at {}: {cause}",
            primary.explain(),
            path_text(path),
            cause = cause.explain()
        ),
    }
}
fn load_text(error: &LoadError) -> String {
    match error {
        LoadError::TemporarySelector { .. } => "temporary entries have no reusable selector".into(),
        LoadError::Identity(error) => identity_text(error),
        LoadError::UnknownBuiltin { id } => format!("unknown built-in recipe \"{id}\""),
        LoadError::Recipe { origin, error } => format!(
            "{origin}: {}",
            recipe_error(error),
            origin = origin.explain()
        ),
        LoadError::Read { origin, cause } => {
            format!(
                "cannot read {origin}: {cause}",
                cause = cause.explain(),
                origin = origin.explain()
            )
        }
        LoadError::File(error) => file_text(error),
        LoadError::Library(error) => library_text(error),
        LoadError::Sound {
            origin,
            rate,
            error,
        } => format!(
            "{origin} at {} Hz: {}",
            rate.hz(),
            crate::errors::acoustic::sound_error(error),
            origin = origin.explain()
        ),
    }
}
fn library_text(error: &LibraryError) -> String {
    use LibraryError as E;
    match error {
        E::Origin { error } => origin_error(error),
        E::File { error } => file_text(error),
        E::Read { error } => io_text(error),
        E::IncompleteRead { error } => library_text(error),
        E::RecipeUtf8 { path, .. } => format!("{}: recipe is not UTF-8", path_text(path)),
        E::RecordRead { path, error } => format!("{}: {}", path_text(path), error.explain()),
        E::RecipeFile { path, error } => format!(
            "{}: {}; file was left intact",
            path_text(path),
            recipe_error(error)
        ),
        E::RecordIdentity {
            path,
            expected,
            found,
        } => format!(
            "{}: filename UUID {expected} does \
            not match document.id {found}",
            path_text(path)
        ),
        E::Recipe { error } => recipe_error(error),
        E::InvalidBuiltin { error } => format!("invalid shipped identity: {}", recipe_error(error)),
        E::DuplicateBuiltin { id } => format!("duplicate shipped UUID {id}"),
        E::ReservedBuiltin { id } => format!(
            "document UUID {id} belongs to a built-in; \
            explicitly create a copy"
        ),
        E::NotInitialized => "library is not initialized".into(),
        E::UnknownManaged { id } => format!(
            "unknown \
            managed recipe user:{id}"
        ),
        E::RootMissing => "configured root is missing; initialize it explicitly".into(),
        E::RootDisappeared => "initializa\
            tion did not create a root"
            .into(),
        E::MissingMarkerOnCreate | E::MissingMarkerOnMutation => "library is not initialized; \
            existing content was not recreated"
            .into(),
        E::StaleContent { id } => format!(
            "saved recipe changed or disappeared (user:{id}); \
            retain the draft and reload or save a copy"
        ),
        E::EntryDisappeared { id } => format!(
            "selected entry user:{id} disappeared; it was not \
            recreated"
        ),
        E::IdentityExists { id } => format!(
            "identity already exists (user:{id}), including \
            Trash; restore or create an independent copy"
        ),
        E::ProposedIdentity { expected, found } => format!(
            "proposed document identity {found} \
            differs from selected user:{expected}"
        ),
        E::MustRestore { id } => format!("restore trashed entry user:{id} before editing it"),
        E::PurgeRequiresTrash { id } => format!(
            "only entries in Trash can be permanently \
            deleted: user:{id}"
        ),
        E::PurgeUnconfirmed => "permanent deletion requires explicit confirmation".into(),
        E::HistoryReplacement { id } => format!(
            "an update cannot replace user:{id}'s creation \
            history"
        ),
        E::ExpectedUser => "expected a captured User selection".into(),
        E::StalePlayback => "Saved selection changed or moved to Trash. Refresh before starting \
            playback; no audio was started."
            .into(),
        other => library_layout_text(other),
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
    match error {
        SaveError::Clock { error } => error.explain(),
        SaveError::Load { error } => load_text(error),
        SaveError::File { error } => file_text(error),
        SaveError::Recipe { error } => recipe_error(error),
        SaveError::HistoryReplacement => "external \
        Save must retain identity and creation history; save an independent \
        copy instead"
            .into(),
    }
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
                crate::errors::acoustic::sound_error(error),
                origin = origin.explain()
            )
        }
    }
}
fn playback_text(error: &PlaybackError) -> String {
    match error {
        PlaybackError::Trashed { .. } => {
            "trashed recipes can be inspected/exported; restore before playback".into()
        }
        PlaybackError::Device(error) => error.explain(),
        PlaybackError::Sound { title, rate, error } => {
            format!(
                "{title} at {} Hz: {}",
                rate.hz(),
                crate::errors::acoustic::sound_error(error)
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

/// Human explanation of a retained inventory diagnostic.
/// Render the retained typed cause or inventory notice.
pub fn library_diagnostic_message<E: Explain>(
    value: &auditorylab_app::services::library::LibraryDiagnostic<E>,
) -> String {
    match &value.detail {
        auditorylab_app::services::library::DiagnosticDetail::Failure(error) => error.explain(),
        auditorylab_app::services::library::DiagnosticDetail::Warning(warning) => {
            crate::errors::publication::warning(warning)
        }
        auditorylab_app::services::library::DiagnosticDetail::Notice(notice) => {
            crate::library::inventory_notice(notice)
        }
    }
}

/// Explain a canonical source-selector or exact-byte-token failure.
#[must_use]
pub(crate) fn origin_error(error: &auditorylab_app::document::origin::OriginError) -> String {
    match error {
        auditorylab_app::document::origin::OriginError::InvalidSelector { value } => format!(
            "invalid selector {value}; expected builtin:<catalog-id> or user:<canonical-uuid>"
        ),
        auditorylab_app::document::origin::OriginError::InvalidFingerprint { .. } => {
            "expected a canonical 64-digit lowercase SHA-256 fingerprint".into()
        }
    }
}

/// Human explanation, separate from a stable machine code.
pub trait Explain {
    /// Render the retained context and original cause for a person.
    fn explain(&self) -> String;
}

mod onboarding;
struct Human<'a, T>(&'a T);
impl Explain for FileError {
    fn explain(&self) -> String {
        file_text(self)
    }
}
impl Explain for LoadError {
    fn explain(&self) -> String {
        load_text(self)
    }
}
impl Explain for LibraryError {
    fn explain(&self) -> String {
        library_text(self)
    }
}
impl Explain for IdentityError {
    fn explain(&self) -> String {
        identity_text(self)
    }
}
impl Explain for EditorError {
    fn explain(&self) -> String {
        editor_text(self)
    }
}
impl Explain for SaveError {
    fn explain(&self) -> String {
        save_text(self)
    }
}
impl Explain for ExportError {
    fn explain(&self) -> String {
        export_text(self)
    }
}
impl Explain for PlaybackError {
    fn explain(&self) -> String {
        playback_text(self)
    }
}
impl Explain for QueryError {
    fn explain(&self) -> String {
        query_text(self)
    }
}
impl Explain for auditorylab_app::clock::ClockError {
    fn explain(&self) -> String {
        "observed system time is outside the supported UTC timestamp range".into()
    }
}
impl Explain for auditorylab_app::document::origin::OriginError {
    fn explain(&self) -> String {
        origin_error(self)
    }
}
impl Explain for auditorylab_app::services::OperationError {
    fn explain(&self) -> String {
        use auditorylab_app::services::OperationError as E;
        match self {
            E::SampleRate(error) => crate::errors::core::sample_rate_error(*error),
            E::Load(error) => error.explain(),
            E::Library(error) => error.explain(),
            E::Export(error) => error.explain(),
            E::Playback(error) => error.explain(),
            E::Save(error) => error.explain(),
            E::Editor(error) => error.explain(),
            E::Identity(error) => error.explain(),
            E::Query(error) => error.explain(),
            E::File(error) => error.explain(),
            E::Recipe(error) => recipe_error(error),
        }
    }
}
impl Explain for auditorylab_app::document::error::RenditionError {
    fn explain(&self) -> String {
        load_text(&LoadError::Sound {
            origin: self.origin.clone(),
            rate: self.rate,
            error: Box::new(self.error.clone()),
        })
    }
}
impl std::fmt::Display for Human<'_, auditorylab_app::services::playback::DeviceError> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            auditorylab_app::services::playback::DeviceError::NoDefaultOutput => {
                write!(f, "no default audio output device found")
            }
            auditorylab_app::services::playback::DeviceError::RouteChanged(_) => {
                write!(
                    f,
                    "the audio output route changed and playback stopped; select the \
                        intended output and play again"
                )
            }
            auditorylab_app::services::playback::DeviceError::Backend(error) => {
                write!(f, "audio backend: {error}")
            }
            auditorylab_app::services::playback::DeviceError::UnsupportedSampleFormat(format) => {
                write!(
                    f,
                    "the output device's {format} sample format is not supported"
                )
            }
            auditorylab_app::services::playback::DeviceError::UnsupportedSampleRate(error) => {
                let hz = error.hz;
                write!(
                    f,
                    "the output device's {hz} Hz sample rate is not supported"
                )
            }
            auditorylab_app::services::playback::DeviceError::TooFewChannels { channels } => {
                write!(
                    f,
                    "the default output device has {channels} channel{}; stereo output needs two \
                        digital channels",
                    if *channels == 1 { "" } else { "s" }
                )
            }
            auditorylab_app::services::playback::DeviceError::InvalidTiming => {
                write!(f, "the audio backend supplied invalid playback timing")
            }
            auditorylab_app::services::playback::DeviceError::MisalignedBuffer => write!(
                f,
                "the audio backend supplied a buffer holding a partial frame"
            ),
            auditorylab_app::services::playback::DeviceError::TimedOut => write!(
                f,
                "the audio stream stopped making progress or outlasted its time limit"
            ),
            auditorylab_app::services::playback::DeviceError::Abandoned => {
                write!(f, "the playback session ended before its stream started")
            }
        }
    }
}
impl Explain for auditorylab_app::services::playback::DeviceError {
    fn explain(&self) -> String {
        Human(self).to_string()
    }
}
impl std::fmt::Display for Human<'_, auditorylab_app::services::playback::LiveUpdateError> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            auditorylab_app::services::playback::LiveUpdateError::SessionClosed => {
                f.write_str("the live session no longer accepts changes")
            }
            auditorylab_app::services::playback::LiveUpdateError::RateMismatch {
                target_hz,
                session_hz,
            } => write!(
                f,
                "the change was checked at {target_hz} Hz but the live session runs at \
                    {session_hz} Hz"
            ),
            auditorylab_app::services::playback::LiveUpdateError::RevisionLimit => {
                f.write_str("the live session's revision limit is reached")
            }
            auditorylab_app::services::playback::LiveUpdateError::Unavailable => {
                f.write_str("live control is unavailable")
            }
        }
    }
}
impl Explain for auditorylab_app::services::playback::LiveUpdateError {
    fn explain(&self) -> String {
        Human(self).to_string()
    }
}
impl std::fmt::Display for Human<'_, auditorylab_app::document::recipe::Origin> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            auditorylab_app::document::recipe::Origin::BuiltIn(id) => write!(f, "{id}"),
            auditorylab_app::document::recipe::Origin::User { id, .. } => write!(f, "user:{id}"),
            auditorylab_app::document::recipe::Origin::File(path) => f.write_str(&path_text(path)),
            auditorylab_app::document::recipe::Origin::Stream(label)
            | auditorylab_app::document::recipe::Origin::Draft(label) => {
                f.write_str(&crate::display_scalar(label))
            }
        }
    }
}
impl Explain for auditorylab_app::document::recipe::Origin {
    fn explain(&self) -> String {
        Human(self).to_string()
    }
}

impl Explain for auditorylab_app::services::IoCause {
    fn explain(&self) -> String {
        self.error().to_string()
    }
}
impl Explain for auditorylab_core::SoundError {
    fn explain(&self) -> String {
        crate::errors::acoustic::sound_error(self)
    }
}
impl Explain for auditorylab_core::RecipeError {
    fn explain(&self) -> String {
        recipe_error(self)
    }
}

pub mod acoustic;
mod context;
pub mod core;
pub mod publication;
pub use context::*;

mod workspace;

fn mutation_explanation(
    phase: &auditorylab_app::services::MutationPhase,
    confirmation: &auditorylab_app::services::Confirmation,
) -> String {
    use {
        auditorylab_app::services::Confirmation as C, auditorylab_app::services::MutationPhase as P,
    };
    match (phase, confirmation) {
        (P::Publish, C::Unchanged) => "publication failed; previous content remains".into(),
        (P::Publish, C::Different) => "publication failed and destination differs from \
                    both previous and candidate bytes; inspect this path before retrying"
            .into(),
        (P::Publish, C::Unavailable(error)) => format!(
            "publication outcome is \
                    unconfirmed; destination could not be verified ({error}); inspect \
                    this path before retrying",
            error = error.explain()
        ),
        (P::SyncPublication, C::Unchanged) => "directory sync failed and previous bytes \
                    remain"
            .into(),
        (P::SyncPublication, C::Different) => "directory sync failed and candidate bytes \
                    could not be verified; inspect this path"
            .into(),
        (P::SyncPublication, C::Unavailable(error)) => format!(
            "directory sync outcome is \
                    unconfirmed; destination could not be verified ({error}); inspect \
                    this path",
            error = error.explain()
        ),
        (P::Relocate { .. }, C::Unchanged) => "relocation did not remove its source; \
                    Trash remains authoritative for same-file links"
            .into(),
        (P::Relocate { source }, C::Unavailable(error)) => format!(
            "relocation outcome is \
                    unconfirmed; source {} could not be inspected ({error}); recheck \
                    before retrying",
            path_text(source),
            error = error.explain()
        ),
        (P::SyncRelocation, C::Different) => "relocation durability failed and \
                    destination changed"
            .into(),
        (P::SyncRelocation, C::Unavailable(error)) => format!(
            "relocation durability is \
                    unconfirmed; destination could not be verified ({error}); recheck \
                    before retrying",
            error = error.explain()
        ),
        (P::Remove, C::Unchanged) => "deletion was not applied; the destination remains".into(),
        (P::Remove, C::Unavailable(error)) => format!(
            "deletion outcome is unconfirmed; \
                    destination could not be inspected ({error}); recheck before retrying",
            error = error.explain()
        ),
        (P::SyncRemoval, C::Different) => "deletion durability failed and the destination \
                    is present"
            .into(),
        (P::SyncRemoval, C::Unavailable(error)) => format!(
            "deletion durability is \
                    unconfirmed; destination could not be inspected ({error}); recheck \
                    before retrying",
            error = error.explain()
        ),
        // These combinations are not emitted by the adapter. Retain conservative
        // wording for public error values without claiming a verified outcome.
        (_, C::Different) => "expected state was not found; inspect this path before \
                    retrying"
            .into(),
        (_, C::Unchanged) => "previous state remains".into(),
    }
}

fn library_layout_text(error: &LibraryError) -> String {
    use LibraryError as E;
    match error {
        E::MembershipUtf8 { .. } => "membership.toml is not UTF-8".into(),
        E::MembershipFormat { .. } => "invalid \
            membership.toml timestamps"
            .into(),
        E::MembershipEncode { error } => format!("cannot encode membership timestamps: {error}"),
        E::MembershipSize { bytes, limit } => format!(
            "membership timestamps contain {bytes} \
            bytes, exceeding their {limit}-byte limit"
        ),
        E::AmbiguousRecord { id } => format!(
            "user:{id}: different recipe files exist in recipes \
            and trash; both were retained"
        ),
        E::Cancelled => "library operation cancelled before publication".into(),
        E::ScanBudget => "library \
            scan budget exhausted; inventory is incomplete"
            .into(),
        E::Clock { .. } => {
            "observed system time is outside the supported UTC timestamp range".into()
        }
        E::ReadOnlyBuiltin { id } => format!(
            "shipped recipe builtin:{id} cannot be renamed, \
            replaced, trashed or purged; edit a copy"
        ),
        E::TemporarySelection => "temporary files must be explicitly added or copied before \
            managed operations"
            .into(),
        E::NativeRootUnavailable => "cannot resolve native per-user application data; configure \
            an absolute library directory"
            .into(),
        E::RelativeRoot { path } if path.as_os_str().is_empty() => "library directory is set \
            but empty; give an absolute folder or unset AUDITORYLAB_LIBRARY_DIR; no fallback was \
            used"
            .into(),
        E::RelativeRoot { path } => format!(
            "library directory {} must be absolute; no fallback \
            was used",
            path_text(path)
        ),
        E::LockNotRegular => "library lock must be a regular persistent file".into(),
        E::Busy => "Library \
            busy; retry after the other operation finishes"
            .into(),
        E::LockUnavailable { error } => format!(
            "required library locking assurance is \
            unavailable; no unlocked fallback: {}",
            error.explain()
        ),
        E::MarkerUtf8 { .. } => "layout marker is not UTF-8".into(),
        E::MarkerFormat { .. } => "invalid \
            library.toml layout marker"
            .into(),
        E::UnsupportedLayout { found } => format!(
            "unsupported library layout {found}; this build \
            reads {}",
            auditorylab_app::services::library::LIBRARY_LAYOUT
        ),
        E::MembershipWithoutMarker => "nonempty membership exists without a layout marker; \
            initialization left it intact"
            .into(),
        E::RecipesWithoutMarker => "recipes exist without a supported marker; initialization left \
            them unchanged"
            .into(),
        E::UnknownRootContents => "unknown existing root contents; initialization refused without \
            changing them"
            .into(),

        _ => unreachable!("handled by library_text"),
    }
}
