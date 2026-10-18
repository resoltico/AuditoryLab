//! Stream encoder failures.
use std::{
    error::Error,
    fmt::{Display, Formatter},
    io,
};
/// Why stream encoding stopped.
#[derive(Debug)]
pub(crate) enum ExportError {
    /// Writer failure.
    Io(io::Error),
    /// Cooperative cancellation.
    Cancelled,
}
impl Display for ExportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => Display::fmt(e, f),
            Self::Cancelled => f.write_str("export cancelled before completion"),
        }
    }
}
impl Error for ExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Cancelled => None,
        }
    }
}
impl From<io::Error> for ExportError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
