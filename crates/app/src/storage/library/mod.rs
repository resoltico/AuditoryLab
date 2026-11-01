//! Local persistent User Library: plain recipe files and cooperative locking.
//!
//! Portable recipe ownership, declared history and nominal classification remain
//! separate from local membership. No library operation belongs on an audio callback.
mod error;
pub use self::error::LibraryError;
pub(crate) use error::CODES as ERROR_CODES;
mod root;
pub(crate) mod store;
use crate::core::DocumentUuid;

use crate::storage::library::LibraryError as Error;
pub(crate) use root::LibraryRoot;
pub use root::{BUSY_WAIT, Initialization, LIBRARY_LAYOUT, RootInfo, RootSource};
pub(crate) use store::LibraryStore;
pub use store::{
    DiagnosticDetail, Inventory, LibraryDiagnostic, LoadedRecord, Mutation, MutationKind,
    MutationStatus, WriteOutcome, history_warnings,
};

pub(crate) fn require_user(
    key: &crate::document::origin::LibraryKey,
) -> Result<DocumentUuid, Error> {
    use crate::document::origin::LibraryKey as Key;
    match key {
        Key::User(id) => Ok(*id),
        Key::BuiltIn(id) => Err(Error::ReadOnlyBuiltin { id: id.to_string() }),
        Key::External(_) => Err(Error::TemporarySelection),
    }
}

/// Typed bounded inventory notices.
pub use store::InventoryNotice;
