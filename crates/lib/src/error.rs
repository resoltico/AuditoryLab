//! Errors shared by library workflows.
use std::{
    fmt,
    path::{Path, PathBuf},
};
/// A failure category for frontend exit codes and presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Unknown built-in ID.
    UnknownPreset,
    /// Input could not be read.
    Unreadable,
    /// Invalid recipe format.
    InvalidPreset,
    /// Invalid signal parameters.
    InvalidSound,
    /// Device failure.
    Device,
    /// Destination was left untouched.
    DestinationExists,
    /// Publication failure.
    Write,
    /// Operation cancelled.
    Cancelled,
}
/// A complete contextual error, optionally naming a leftover partial file.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    leftover: Option<PathBuf>,
}
impl Error {
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
            leftover: None,
        }
    }
    pub(crate) fn caused(
        kind: ErrorKind,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            source: Some(Box::new(source)),
            ..Self::new(kind, message)
        }
    }
    pub(crate) fn with_leftover(mut self, path: PathBuf, error: std::io::Error) -> Self {
        self.message
            .push_str(&format!("; a partial file remains at {path:?}: {error}"));
        self.leftover = Some(path);
        self.kind = ErrorKind::Write;
        self
    }
    /// Failure category.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
    /// Partial file that cleanup could not remove.
    pub fn leftover(&self) -> Option<&Path> {
        self.leftover.as_deref()
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|e| e as _)
    }
}
