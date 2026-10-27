//! Storage notices retain confirmed outcomes and original failure context.
use crate::storage::error::IoCause;
use std::path::PathBuf;
/// A nonfatal publication, cleanup or history condition.
#[derive(Debug, Clone, PartialEq)]
pub enum StorageWarning {
    /// Candidate bytes were verified after publication reported an I/O error.
    SavedAfterIo {
        cause: IoCause,
    },
    /// Candidate bytes are present, but synchronizing the directory failed.
    SavedDurability {
        cause: IoCause,
    },
    /// Publication succeeded, but removing its owned stage failed.
    RetainedStage {
        path: PathBuf,
        cause: IoCause,
    },
    /// Source removal was verified after relocation reported an I/O error.
    MovedAfterIo {
        cause: IoCause,
    },
    /// Trash owns membership; a redundant active link remains at the captured path.
    TrashLink {
        path: PathBuf,
        cause: IoCause,
    },
    /// Relocation was verified, but synchronizing its directory failed.
    MovedDurability {
        cause: IoCause,
    },
    /// Absence was verified under the exclusive lock after deletion reported an I/O error.
    DeletedAfterIo {
        cause: IoCause,
    },
    /// Absence is confirmed, but synchronizing its directory failed.
    DeletedDurability {
        cause: IoCause,
    },
    ClockPredatesEdit,
    RemovedStage {
        path: PathBuf,
    },
    RemovedMemberships {
        count: usize,
    },
    /// Recipe publication succeeded, but updating membership timestamps failed.
    MembershipUpdateFailed {
        cause: Box<crate::storage::library::LibraryError>,
    },
    /// Recipe bytes are confirmed; local membership dates are unknown.
    MembershipReadFailed {
        cause: Box<crate::storage::library::LibraryError>,
    },
    /// Interrupted same-file links were reconciled with Trash authoritative.
    RecoveredMove {
        name: std::ffi::OsString,
    },
    ModificationPrecedesCreation,
    ClockPredatesHistory,
    FutureHistory,
    UnavailableHistoryClock {
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
