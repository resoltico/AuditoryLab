//! Controlled faults around real native operations; excluded from product builds.
use crate::Timestamp;
/// Native store fault selected explicitly by a test, never by portable content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreFault {
    /// Write a partial real stage, then report failure.
    Write,
    /// Fail flush after a complete native write, before publication.
    Flush,
    /// Fail file synchronization before publication.
    FileSync,
    /// Request the real cancellation flag after stage sync.
    CancelBeforePublication,
    /// Publish natively, then report a directory-sync warning.
    DirectorySync,
    /// Leave the successful no-clobber staging hard link and warn.
    StageCleanup,
    /// Native publication succeeded but its reported result is ambiguous.
    PublicationCommittedError,
    /// Publication was not performed and its reported result is ambiguous.
    PublicationUncommittedError,
    /// Native publication succeeded and the immediate outcome reread is unavailable.
    PublicationCommittedUnreadable,
    /// Publication was not performed and the immediate outcome reread is unavailable.
    PublicationUncommittedUnreadable,
    /// Insert a real foreign destination after staging to challenge no-clobber.
    CollisionAfterStage,
    /// Purge natively, then report ambiguity with unavailable attribution.
    PurgeCommittedUnreadable,
    /// Do not purge; report ambiguity with unavailable attribution.
    PurgeUncommittedUnreadable,
    /// Pause with an owned test signal after syncing the stage; parent kills writer.
    PauseBeforePublication,
    /// Pause after actual publication, before completion/directory sync.
    PauseAfterPublication,
    /// Pause after real unlink, before completion/directory sync.
    PauseAfterPurge,
    /// Use a specified observed clock without modifying system settings.
    Clock(Timestamp),
    /// Fail nonce acquisition before stage creation; never fall back.
    RandomUnavailable,
}
