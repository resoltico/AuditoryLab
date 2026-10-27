//! Warnings emitted only after the storage adapter verifies publication truth.
use std::{io, path::Path};
pub(crate) fn saved_after_io(error: &io::Error) -> String {
    format!("Saved; publication was verified after an I/O error: {error}")
}
pub(crate) fn saved_durability(error: &io::Error) -> String {
    format!("Saved; directory durability warning: {error}")
}
pub(crate) fn retained_stage(stage: &Path, error: &io::Error) -> String {
    format!("Saved; owned stage {} remains: {error}", stage.display())
}
pub(crate) fn moved_after_io(error: &io::Error) -> String {
    format!("Moved; source removal verified after I/O error: {error}")
}
pub(crate) fn trash_link(source: &Path, error: &io::Error) -> String {
    format!(
        "Moved to Trash; redundant active link remains at {}: {error}",
        source.display()
    )
}
pub(crate) fn moved_durability(error: &io::Error) -> String {
    format!("Moved; directory durability warning: {error}")
}
pub(crate) fn deleted_after_io(error: &io::Error) -> String {
    format!("Deleted; absence verified under the exclusive lock after I/O error: {error}")
}
pub(crate) fn deleted_durability(error: &io::Error) -> String {
    format!("Deleted; directory durability warning: {error}")
}
pub(crate) const CLOCK_PREDATES_EDIT: &str =
    "Observed clock predates the prior declared edit time; no artificial timestamp was fabricated.";
