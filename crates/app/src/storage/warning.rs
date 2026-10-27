//! Storage notices retain confirmed outcomes and original failure context.
use crate::storage::error::IoCause;
use std::path::PathBuf;
/// A nonfatal publication, cleanup or history condition.
#[derive(Debug, Clone, PartialEq)]
pub enum StorageWarning {
    /// Saved After Io.
    SavedAfterIo {
        /// Retained cause.
        cause: IoCause,
    },
    /// Saved Durability.
    SavedDurability {
        /// Retained cause.
        cause: IoCause,
    },
    /// Retained Stage.
    RetainedStage {
        /// Retained path.
        path: PathBuf,
        /// Retained cause.
        cause: IoCause,
    },
    /// Moved After Io.
    MovedAfterIo {
        /// Retained cause.
        cause: IoCause,
    },
    /// Trash Link.
    TrashLink {
        /// Retained path.
        path: PathBuf,
        /// Retained cause.
        cause: IoCause,
    },
    /// Moved Durability.
    MovedDurability {
        /// Retained cause.
        cause: IoCause,
    },
    /// Deleted After Io.
    DeletedAfterIo {
        /// Retained cause.
        cause: IoCause,
    },
    /// Deleted Durability.
    DeletedDurability {
        /// Retained cause.
        cause: IoCause,
    },
    /// Clock Predates Edit.
    ClockPredatesEdit,
    /// Removed Stage.
    RemovedStage {
        /// Retained path.
        path: PathBuf,
    },
    /// Removed Memberships.
    RemovedMemberships {
        /// Retained count.
        count: usize,
    },
    /// Membership Update Failed.
    MembershipUpdateFailed {
        /// Retained cause.
        cause: Box<crate::storage::library::LibraryError>,
    },
    /// Membership Read Failed.
    MembershipReadFailed {
        /// Retained cause.
        cause: Box<crate::storage::library::LibraryError>,
    },
    /// Recovered Move.
    RecoveredMove {
        /// Retained name.
        name: std::ffi::OsString,
    },
    /// Modification Precedes Creation.
    ModificationPrecedesCreation,
    /// Clock Predates History.
    ClockPredatesHistory,
    /// Future History.
    FutureHistory,
    /// Unavailable History Clock.
    UnavailableHistoryClock {
        /// Retained cause.
        cause: crate::clock::ClockError,
    },
}
impl std::fmt::Display for StorageWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SavedAfterIo { .. } => "storage.saved_after_io",
            Self::SavedDurability { .. } => "storage.saved_durability",
            Self::RetainedStage { .. } => "storage.retained_stage",
            Self::MovedAfterIo { .. } => "storage.moved_after_io",
            Self::TrashLink { .. } => "storage.trash_link",
            Self::MovedDurability { .. } => "storage.moved_durability",
            Self::DeletedAfterIo { .. } => "storage.deleted_after_io",
            Self::DeletedDurability { .. } => "storage.deleted_durability",
            Self::ClockPredatesEdit => "storage.clock_predates_edit",
            Self::RemovedStage { .. } => "storage.removed_stage",
            Self::RemovedMemberships { .. } => "storage.removed_memberships",
            Self::MembershipUpdateFailed { .. } => "storage.membership_update_failed",
            Self::MembershipReadFailed { .. } => "storage.membership_read_failed",
            Self::RecoveredMove { .. } => "storage.recovered_move",
            Self::ModificationPrecedesCreation => "storage.modification_precedes_creation",
            Self::ClockPredatesHistory => "storage.clock_predates_history",
            Self::FutureHistory => "storage.future_history",
            Self::UnavailableHistoryClock { .. } => "storage.unavailable_history_clock",
        })
    }
}
