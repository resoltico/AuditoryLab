// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io;

/// Why an export failed, including an incomplete file left behind.
#[derive(Debug)]
pub enum ExportError {
    /// Something already has the destination's name; it was left untouched.
    DestinationExists,
    /// Creating, writing or syncing the file failed.
    Io(io::Error),
    /// Cancellation was observed before export completed.
    Cancelled,
    /// The export failed and its incomplete file could not be removed.
    IncompleteFileLeft {
        /// Why the export failed.
        failure: Box<Self>,
        /// Why the incomplete file could not be removed.
        removal: io::Error,
    },
}

impl Display for ExportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DestinationExists => {
                write!(f, "the destination already exists and was left untouched")
            }
            Self::Io(error) => write!(f, "{error}"),
            Self::Cancelled => write!(f, "export cancelled before completion"),
            Self::IncompleteFileLeft { failure, removal } => write!(
                f,
                "{failure}; the incomplete file could not be removed ({removal}) and remains at the destination"
            ),
        }
    }
}

impl Error for ExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::IncompleteFileLeft { failure, .. } => Some(failure),
            Self::DestinationExists | Self::Cancelled => None,
        }
    }
}

impl From<io::Error> for ExportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::ExportError;
    use std::error::Error;
    use std::io;

    #[test]
    fn each_failure_says_what_happened_and_names_its_cause() {
        let io = ExportError::from(io::Error::other("disk full"));
        assert_eq!(io.to_string(), "disk full");
        assert_eq!(io.source().unwrap().to_string(), "disk full");

        let left = ExportError::IncompleteFileLeft {
            failure: Box::new(ExportError::Cancelled),
            removal: io::Error::other("read-only"),
        };
        assert_eq!(
            left.to_string(),
            "export cancelled before completion; the incomplete file could not be removed (read-only) and remains at the destination"
        );
        assert_eq!(
            left.source().unwrap().to_string(),
            "export cancelled before completion"
        );

        assert_eq!(
            ExportError::DestinationExists.to_string(),
            "the destination already exists and was left untouched"
        );
        for without_cause in [ExportError::DestinationExists, ExportError::Cancelled] {
            assert!(without_cause.source().is_none());
        }
    }
}
