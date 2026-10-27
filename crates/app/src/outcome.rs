//! Frontend-independent operation categories.
/// A failure category for frontend exit codes and presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Unknown built-in recipe ID.
    UnknownRecipe,
    /// Input could not be read.
    Unreadable,
    /// Invalid recipe document: format, schema, non-finite numbers or size.
    InvalidRecipe,
    /// Invalid signal parameters.
    InvalidSound,
    /// Secure portable identity generation failed; no fallback identity is invented.
    Identity,
    /// Device failure.
    Device,
    /// Destination was left untouched.
    DestinationExists,
    /// Publication failure.
    Write,
    /// Invalid command/configuration argument.
    InvalidArgument,
    /// A selected built-in cannot be mutated.
    ReadOnlyBuiltIn,
    /// Expected record content/revision no longer matches.
    Conflict,
    /// Incoming creation history differs from the stored identity's history.
    HistoryConflict,
    /// A cooperating reader/writer currently owns the root lock.
    Busy,
    /// Required filesystem locking assurance is unavailable.
    LockUnavailable,
    /// Discovery or a requested managed record cannot be read completely.
    IncompleteInventory,
    /// A file operation was cancelled before publication.
    Cancelled,
}
