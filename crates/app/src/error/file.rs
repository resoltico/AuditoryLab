//! Filesystem adapter failures carry operations, paths and original causes.
use super::{IoCause, Outcome};
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
    Identity(super::IdentityError),
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
    Changed {
        /// Captured path.
        path: PathBuf,
    },
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

impl From<super::IdentityError> for FileError {
    fn from(error: super::IdentityError) -> Self {
        Self::Identity(error)
    }
}
