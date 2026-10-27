//! Stream encoder failures.
use std::{error::Error, io};
/// Why stream encoding stopped.
#[derive(Debug)]
pub(crate) enum ExportError {
    /// Writer failure.
    Io(io::Error),
    /// Cooperative cancellation.
    Cancelled,
}
impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::Code::code(self))
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

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    ExportError {
        codes {
            Self::Io(_) => "wav.io",
            Self::Cancelled => "wav.cancelled",
        }
        delegates {}
        params(this) {
            match this {
                Self::Io(error) => crate::core::diagnostic_params! {"cause"=>error.to_string()},
                Self::Cancelled => crate::core::Params::new(),
            }
        }
    }
}
