//! Filesystem adapter failures carry operations, paths and original causes.
use crate::outcome::Outcome;
use std::path::PathBuf;
/// The filesystem operation that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoOperation {
    /// Open an existing object.
    Open,
    /// Read object contents.
    Read,
    /// Seek within an opened object.
    Seek,
    /// Write object contents.
    Write,
    /// Inspect object metadata or identity.
    Inspect,
    /// Create a directory.
    CreateDirectory,
    /// Open a directory capability.
    OpenDirectory,
    /// Create a staged file.
    CreateStage,
    /// Flush staged file bytes.
    SyncFile,
    /// Flush directory entries.
    SyncDirectory,
    /// Remove an owned file or stage.
    Remove,
    /// Publish a destination entry.
    Publish,
    /// Relocate an entry.
    Relocate,
    /// Resolve an opened external path.
    ResolvePath,
    /// Retain an opened object identity.
    RetainIdentity,
    /// Compare an opened object with its pathname.
    CheckIdentity,
    /// Set staged file permissions.
    SetPermissions,
}
/// A native filesystem failure with its operation and optional known path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoFailure {
    /// Attempted operation.
    pub action: IoOperation,
    /// Affected path when available at this boundary.
    pub path: Option<PathBuf>,
    /// Original external cause.
    pub cause: IoCause,
}
impl IoFailure {
    pub(crate) fn new(action: IoOperation, path: Option<PathBuf>, error: std::io::Error) -> Self {
        Self {
            action,
            path,
            cause: IoCause::new(error),
        }
    }
}
/// The mutation phase whose result needed independent confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationPhase {
    /// Publication reported an I/O failure.
    Publish,
    /// Publication directory synchronization failed.
    SyncPublication,
    /// A relocation could not remove its original name.
    Relocate {
        /// Original source path.
        source: PathBuf,
    },
    /// Relocation directory synchronization failed.
    SyncRelocation,
    /// Deletion reported an I/O failure.
    Remove,
    /// Deletion directory synchronization failed.
    SyncRemoval,
}
/// Observed state after a mutation's I/O failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirmation {
    /// The previous state remains.
    Unchanged,
    /// The expected state was not found.
    Different,
    /// State inspection itself failed.
    Unavailable(Box<FileError>),
}
/// A checked filesystem publication or observation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    /// Native filesystem operation failed.
    Io(IoFailure),
    /// An exclusive stage identity could not be allocated.
    Identity(crate::identity::IdentityError),
    /// Opened object is not a regular file.
    NotRegular {
        /// Affected path.
        path: PathBuf,
    },
    /// Bounded reading exceeded its byte limit.
    ReadLimit {
        /// Affected path.
        path: PathBuf,
        /// Whether growth happened during the read.
        grew: bool,
    },
    /// Cancellation was observed before publication.
    Cancelled {
        /// Captured destination when applicable.
        path: Option<PathBuf>,
    },
    /// An existing destination was preserved.
    DestinationExists {
        /// Destination path.
        path: PathBuf,
        /// Native creation race, absent for a preflight refusal.
        cause: Option<IoCause>,
    },
    /// Opened identity or bytes changed before the action.
    Changed { path: PathBuf },
    /// The destination has no usable filename.
    MissingFilename,
    /// The external destination has no parent.
    MissingParent,
    /// A relocation destination already exists.
    RelocationExists {
        /// Destination path.
        path: PathBuf,
    },
    /// A failed mutation was checked against its authoritative observed state.
    AfterMutation {
        /// Failed mutation phase.
        phase: MutationPhase,
        /// Affected destination.
        path: PathBuf,
        /// Independently observed postcondition.
        confirmation: Confirmation,
        /// Original mutation failure.
        cause: IoCause,
    },
    /// Failed cleanup left an owned partial file after the primary failure.
    Cleanup {
        /// Original failure.
        primary: Box<FileError>,
        /// Owned partial path left behind.
        path: PathBuf,
        /// Cleanup cause.
        cause: IoCause,
    },
}
impl FileError {
    /// Category of the failed operation.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::Identity(error) => error.outcome(),
            Self::NotRegular { .. } => Outcome::Unreadable,
            Self::ReadLimit { .. } => Outcome::IncompleteInventory,
            Self::Cancelled { .. } => Outcome::Cancelled,
            Self::DestinationExists { .. } => Outcome::DestinationExists,
            Self::Changed { .. } | Self::RelocationExists { .. } => Outcome::Conflict,
            Self::Io(_)
            | Self::MissingFilename
            | Self::MissingParent
            | Self::Cleanup { .. }
            | Self::AfterMutation { .. } => Outcome::Write,
        }
    }
    /// Native I/O classification, when this failure has a direct I/O cause.
    #[must_use]
    pub fn io_kind(&self) -> Option<std::io::ErrorKind> {
        match self {
            Self::Io(error) => Some(error.cause.error().kind()),
            _ => None,
        }
    }
    /// An owned partial file that cleanup could not remove.
    #[must_use]
    pub fn leftover(&self) -> Option<&std::path::Path> {
        match self {
            Self::Cleanup { path, .. } => Some(path),
            _ => None,
        }
    }
    pub(crate) fn with_leftover(self, path: PathBuf, error: std::io::Error) -> Self {
        Self::Cleanup {
            primary: Box::new(self),
            path,
            cause: IoCause::new(error),
        }
    }
}

impl From<crate::identity::IdentityError> for FileError {
    fn from(error: crate::identity::IdentityError) -> Self {
        Self::Identity(error)
    }
}

use std::{error::Error, io, sync::Arc};
/// A captured I/O cause. Clones retain the same cause identity.
#[derive(Debug, Clone)]
pub struct IoCause(Arc<io::Error>);
impl IoCause {
    pub(crate) fn new(error: io::Error) -> Self {
        Self(Arc::new(error))
    }
    /// The original I/O cause.
    pub fn error(&self) -> &io::Error {
        self.0.as_ref()
    }
}
impl PartialEq for IoCause {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for IoCause {}
impl std::fmt::Display for IoCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl Error for IoCause {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

use crate::core::RecipeError;
/// Portable content could not be saved or published.
#[derive(Debug, Clone, PartialEq)]
pub enum SaveError {
    /// Static portable source resolution failed.
    Load {
        error: crate::storage::load::LoadError,
    },
    /// The observed clock is outside the portable timestamp profile.
    Clock { error: crate::clock::ClockError },
    /// Filesystem publication or observation failed.
    File { error: FileError },
    /// Portable content failed validation or serialization.
    Recipe { error: RecipeError },
    /// Saving would replace identity or creation history.
    HistoryReplacement,
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
impl From<crate::clock::ClockError> for SaveError {
    fn from(error: crate::clock::ClockError) -> Self {
        Self::Clock { error }
    }
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for FileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
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

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::File { error } => Some(error),
            Self::Recipe { error } => Some(error),
            Self::Load { error } => Some(error),
            Self::Clock { error } => Some(error),
            Self::HistoryReplacement => None,
        }
    }
}

impl std::fmt::Display for IoFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for IoFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.error())
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    IoCause {
        codes {
            Self(..) => "io.cause",
        }
        delegates {}
        params(this) {
            crate::core::diagnostic_params! {"cause"=>this.error().to_string()}
        }
    }
    IoFailure {
        codes {
            Self { .. } => "io.failure",
        }
        delegates {}
        params(this) {
            let action = match this.action {
                IoOperation::Open => "open",
                IoOperation::Read => "read",
                IoOperation::Seek => "seek",
                IoOperation::Write => "write",
                IoOperation::Inspect => "inspect",
                IoOperation::CreateDirectory => "create_directory",
                IoOperation::OpenDirectory => "open_directory",
                IoOperation::CreateStage => "create_stage",
                IoOperation::SyncFile => "sync_file",
                IoOperation::SyncDirectory => "sync_directory",
                IoOperation::Remove => "remove",
                IoOperation::Publish => "publish",
                IoOperation::Relocate => "relocate",
                IoOperation::ResolvePath => "resolve_path",
                IoOperation::RetainIdentity => "retain_identity",
                IoOperation::CheckIdentity => "check_identity",
                IoOperation::SetPermissions => "set_permissions",
            };
            let mut params = crate::core::diagnostic_params! {"action"=>action,"cause"=>this.cause.error().to_string()};
            if let Some(path) = &this.path {
                params.insert("path", path.to_string_lossy().into_owned().into());
            }
            params
        }
    }
    FileError {
        codes {
            Self::NotRegular { .. } => "file.not_regular",
            Self::ReadLimit { .. } => "file.read_limit",
            Self::Cancelled { .. } => "file.cancelled",
            Self::DestinationExists { .. } => "file.destination_exists",
            Self::Changed { .. } => "file.changed",
            Self::MissingFilename => "file.missing_filename",
            Self::MissingParent => "file.missing_parent",
            Self::RelocationExists { .. } => "file.relocation_exists",
            Self::AfterMutation { .. } => "file.after_mutation",
        }
        delegates {
            Self::Io(error) => error,
            Self::Identity(error) => error,
            Self::Cleanup { primary, .. } => primary,
        }
        params(this) {
            match this {
                Self::NotRegular { path }
                | Self::DestinationExists { path, .. }
                | Self::Changed { path }
                | Self::RelocationExists { path } => {
                    crate::core::diagnostic_params! {"path"=>path.to_string_lossy().into_owned()}
                }
                Self::ReadLimit { path, grew } => {
                    crate::core::diagnostic_params! {"path"=>path.to_string_lossy().into_owned(),"grew"=>*grew}
                }
                Self::Cancelled { path: Some(path) } => {
                    crate::core::diagnostic_params! {"path"=>path.to_string_lossy().into_owned()}
                }
                Self::AfterMutation {
                    phase,
                    path,
                    confirmation,
                    cause,
                } => {
                    let phase_key = match phase {
                        MutationPhase::Publish => "publish",
                        MutationPhase::SyncPublication => "sync_publication",
                        MutationPhase::Relocate { .. } => "relocate",
                        MutationPhase::SyncRelocation => "sync_relocation",
                        MutationPhase::Remove => "remove",
                        MutationPhase::SyncRemoval => "sync_removal",
                    };
                    let confirmation_key = match confirmation {
                        Confirmation::Unchanged => "unchanged",
                        Confirmation::Different => "different",
                        Confirmation::Unavailable(_) => "unavailable",
                    };
                    let mut params = crate::core::diagnostic_params! {"phase"=>phase_key,"path"=>path.to_string_lossy().into_owned(),"confirmation"=>confirmation_key,"cause"=>cause.error().to_string()};
                    if let MutationPhase::Relocate { source } = phase {
                        params.insert("source_path", source.to_string_lossy().into_owned().into());
                    }
                    if let Confirmation::Unavailable(error) = confirmation {
                        params.insert("verification_code", crate::core::diagnostic::Code::code(error).into());
                    }
                    params
                }
                Self::Cleanup { path, cause, .. } => {
                    crate::core::diagnostic_params! {"leftover_path"=>path.to_string_lossy().into_owned(),"cleanup_cause"=>cause.error().to_string()}
                }
                _ => crate::core::diagnostic::Params::new(),
            }
        }
    }
    SaveError {
        codes {
            Self::HistoryReplacement => "save.history_replacement",
        }
        delegates {
            Self::Load { error } => error,
            Self::Clock { error } => error,
            Self::File { error } => error,
            Self::Recipe { error } => error,
        }
    }
}
