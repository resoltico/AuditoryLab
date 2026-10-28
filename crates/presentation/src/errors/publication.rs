//! Warnings emitted only after the storage adapter verifies publication truth.
use std::path::Path;
pub(crate) fn saved_after_io(error: &auditorylab_app::services::IoCause) -> String {
    let error = error.error();
    format!("Saved; publication was verified after an I/O error: {error}")
}
pub(crate) fn saved_durability(error: &auditorylab_app::services::IoCause) -> String {
    let error = error.error();
    format!("Saved; directory durability warning: {error}")
}
pub(crate) fn retained_stage(stage: &Path, error: &auditorylab_app::services::IoCause) -> String {
    let error = error.error();
    format!("Saved; owned stage {} remains: {error}", stage.display())
}
pub(crate) fn moved_after_io(error: &auditorylab_app::services::IoCause) -> String {
    let error = error.error();
    format!("Moved; source removal verified after I/O error: {error}")
}
pub(crate) fn trash_link(source: &Path, error: &auditorylab_app::services::IoCause) -> String {
    let error = error.error();
    format!(
        "Moved to Trash; redundant active link remains at {}: {error}",
        source.display()
    )
}
pub(crate) fn moved_durability(error: &auditorylab_app::services::IoCause) -> String {
    let error = error.error();
    format!("Moved; directory durability warning: {error}")
}
pub(crate) fn deleted_after_io(error: &auditorylab_app::services::IoCause) -> String {
    let error = error.error();
    format!("Deleted; absence verified under the exclusive lock after I/O error: {error}")
}
pub(crate) fn deleted_durability(error: &auditorylab_app::services::IoCause) -> String {
    let error = error.error();
    format!("Deleted; directory durability warning: {error}")
}
pub(crate) const CLOCK_PREDATES_EDIT: &str =
    "Observed clock predates the prior declared edit time; no artificial timestamp was fabricated.";

/// Wording for a confirmed storage notice.
#[must_use]
pub fn warning(warning: &auditorylab_app::services::StorageWarning) -> String {
    use auditorylab_app::services::StorageWarning as W;
    match warning {
        W::SavedAfterIo { cause } => saved_after_io(cause),
        W::SavedDurability { cause } => saved_durability(cause),
        W::RetainedStage { path, cause } => retained_stage(path, cause),
        W::MovedAfterIo { cause } => moved_after_io(cause),
        W::TrashLink { path, cause } => trash_link(path, cause),
        W::MovedDurability { cause } => moved_durability(cause),
        W::DeletedAfterIo { cause } => deleted_after_io(cause),
        W::DeletedDurability { cause } => deleted_durability(cause),
        W::ClockPredatesEdit => CLOCK_PREDATES_EDIT.into(),
        W::RemovedStage { path } => crate::library::removed_stage(path),
        W::RemovedMemberships { count } => crate::library::removed_memberships(*count),
        W::MembershipUpdateFailed { cause } => crate::library::membership_update_failed(cause),
        W::MembershipReadFailed { cause } => crate::library::membership_read_failed(cause),
        W::RecoveredMove { name } => crate::library::recovered_move(name),
        W::ModificationPrecedesCreation => crate::library::MODIFICATION_PRECEDES_CREATION.into(),
        W::ClockPredatesHistory => crate::library::CLOCK_PREDATES_HISTORY.into(),
        W::FutureHistory => crate::library::FUTURE_HISTORY.into(),
        W::UnavailableHistoryClock { cause } => crate::library::unavailable_history_clock(cause),
    }
}
