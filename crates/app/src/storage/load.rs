//! Bounded portable recipe loading from built-ins, opened files and readers.
use super::input_admission;
use crate::core::Recipe;
use crate::document::recipe::{Origin, RecipeSnapshot};
use std::{io::Read, path::Path};
use {
    crate::storage::error::FileError, crate::storage::error::IoCause,
    crate::storage::error::IoFailure, crate::storage::error::IoOperation,
    crate::storage::load::LoadError as Error,
};
pub(crate) fn parse(bytes: &[u8], origin: Origin) -> Result<RecipeSnapshot, Error> {
    let recipe = Recipe::from_bytes(bytes).map_err(|error| Error::Recipe {
        origin: origin.clone(),
        error: Box::new(error),
    })?;
    Ok(RecipeSnapshot::new(origin, recipe))
}
fn read(reader: impl Read, origin: Origin) -> Result<RecipeSnapshot, Error> {
    let mut bytes = Vec::new();
    reader
        .take((Recipe::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| Error::Read {
            origin: origin.clone(),
            cause: IoCause::new(error),
        })?;
    parse(&bytes, origin)
}
/// Read a bounded portable stream.
///
/// # Errors
/// Preserves read, encoding, format, contract and version failures.
pub(crate) fn load_reader(reader: impl Read, label: &str) -> Result<RecipeSnapshot, Error> {
    read(reader, Origin::Stream(label.into()))
}
/// Open and read a regular recipe file without blocking on special files.
///
/// # Errors
/// Preserves opened-object admission and portable-contract failures.
pub(crate) fn load_file(path: &Path) -> Result<RecipeSnapshot, Error> {
    let mut file = input_admission::open(path).map_err(|error| match error {
        input_admission::Failure::Io(error) => Error::File(FileError::Io(IoFailure::new(
            IoOperation::Open,
            Some(path.to_owned()),
            error,
        ))),
        input_admission::Failure::NotRegular => Error::File(FileError::NotRegular {
            path: path.to_owned(),
        }),
    })?;
    let mut bytes = Vec::new();
    (&mut file)
        .take((Recipe::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| Error::Read {
            origin: Origin::File(path.to_owned()),
            cause: IoCause::new(error),
        })?;
    let snapshot = parse(&bytes, Origin::File(path.to_owned()))?;
    let observed = crate::document::origin::ObservedFile::capture(path, file, &bytes)?;
    Ok(snapshot.with_external(observed))
}

use crate::core::{RecipeError, SampleRate, SoundError};
use crate::{outcome::Outcome, storage::library::LibraryError};
/// A recipe or rendition could not be loaded.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// Temporary process-local identities are not reusable selectors.
    TemporarySelector {
        /// Requested ephemeral identity.
        key: crate::document::origin::LibraryKey,
    },
    /// A temporary source identity could not be allocated.
    Identity(crate::identity::IdentityError),
    /// The built-in selector is unknown.
    UnknownBuiltin {
        /// Requested catalog ID.
        id: String,
    },
    /// Input failed its portable contract.
    Recipe {
        /// Captured source attribution.
        origin: Origin,
        /// Exact format/contract/version failure.
        error: Box<RecipeError>,
    },
    /// A bounded stream could not be read.
    Read {
        /// Captured stream or file attribution.
        origin: Origin,
        /// Original read failure.
        cause: IoCause,
    },
    /// A file could not be opened or observed.
    File(FileError),
    /// Managed source resolution failed.
    Library(LibraryError),
    /// A snapshot cannot be realized at the selected rate.
    Sound {
        /// Captured source attribution.
        origin: Origin,
        /// Chosen rendering rate.
        rate: SampleRate,
        /// Acoustic failure.
        error: Box<SoundError>,
    },
}
impl LoadError {
    /// Failure category, independent of frontend exit policy.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::UnknownBuiltin { .. } => Outcome::UnknownRecipe,
            Self::TemporarySelector { .. } => Outcome::InvalidArgument,
            Self::Identity(error) => error.outcome(),
            Self::Recipe { .. } => Outcome::InvalidRecipe,
            Self::Read { .. } | Self::File(FileError::Io(_) | FileError::NotRegular { .. }) => {
                Outcome::Unreadable
            }
            Self::File(error) => error.outcome(),
            Self::Library(error) => error.outcome(),
            Self::Sound { .. } => Outcome::InvalidSound,
        }
    }
}
impl From<FileError> for LoadError {
    fn from(error: FileError) -> Self {
        Self::File(error)
    }
}
impl From<LibraryError> for LoadError {
    fn from(error: LibraryError) -> Self {
        Self::Library(error)
    }
}

impl From<crate::document::error::RenditionError> for LoadError {
    fn from(error: crate::document::error::RenditionError) -> Self {
        Self::Sound {
            origin: error.origin,
            rate: error.rate,
            error: Box::new(error.error),
        }
    }
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Recipe { error, .. } => Some(error.as_ref()),
            Self::Read { cause, .. } => Some(cause.error()),
            Self::File(error) => Some(error),
            Self::Library(error) => Some(error),
            Self::Sound { error, .. } => Some(error.as_ref()),
            Self::UnknownBuiltin { .. } | Self::TemporarySelector { .. } => None,
            Self::Identity(error) => Some(error),
        }
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    LoadError {
        codes {
            Self::TemporarySelector { .. } => "load.temporary_selector",
            Self::UnknownBuiltin { .. } => "load.unknown_builtin",
            Self::Read { .. } => "load.read",
        }
        delegates {
            Self::Identity(error) => error,
            Self::Recipe { error, .. } => error,
            Self::File(error) => error,
            Self::Library(error) => error,
            Self::Sound { error, .. } => error,
        }
        params(this) {
            match this {
                Self::TemporarySelector { key } => {
                    crate::core::diagnostic_params! {"selector"=>key.to_string()}
                }
                Self::UnknownBuiltin { id } => {
                    crate::core::diagnostic_params! {"catalog_id"=>id.clone()}
                }
                Self::Recipe { origin, .. }
                | Self::Read { origin, .. }
                | Self::Sound { origin, .. } => {
                    let mut params = crate::core::diagnostic_params! {"origin"=>origin.to_string()};
                    if let Self::Sound { rate, .. } = this {
                        params.insert("diagnostic_rate_hz", rate.hz().into());
                    }
                    params
                }
                _ => crate::core::diagnostic::Params::new(),
            }
        }
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;
    use crate::core::{ParameterId, VoiceError, diagnostic::Code};
    use std::error::Error as _;

    #[test]
    fn boxed_causes_preserve_types_codes_parameters_and_outcomes() {
        let sound = SoundError::Voice {
            id: crate::core::ComponentId::new(7).unwrap().into(),
            error: VoiceError::Frequency {
                parameter: ParameterId::ModulationPulseRate,
                hz: 24_000.0,
                sample_rate_hz: 48_000,
            },
        };
        let load: LoadError = crate::document::error::RenditionError {
            origin: Origin::Stream("captured".into()),
            rate: SampleRate::DEFAULT,
            error: sound.clone(),
        }
        .into();
        assert_eq!(
            load.source().unwrap().downcast_ref::<SoundError>(),
            Some(&sound)
        );
        assert_eq!(load.code(), "voice.frequency");
        assert_eq!(
            load.params()["hz"],
            crate::core::diagnostic::ParamValue::from(24_000.0)
        );
        assert_eq!(load.outcome(), Outcome::InvalidSound);
        let recipe = LibraryError::RecipeFile {
            path: "captured.toml".into(),
            error: Box::new(RecipeError::UnsupportedVersion { found: 3 }),
        };
        assert!(matches!(
            recipe.source().unwrap().downcast_ref::<RecipeError>(),
            Some(RecipeError::UnsupportedVersion { found: 3 })
        ));
        assert_eq!(recipe.code(), "recipe.schema_version");
        assert_eq!(recipe.outcome(), Outcome::IncompleteInventory);
        let file = FileError::Changed {
            path: "captured.toml".into(),
        };
        let record = LibraryError::RecordRead {
            path: "captured.toml".into(),
            error: Box::new(file.clone()),
        };
        assert_eq!(
            record.source().unwrap().downcast_ref::<FileError>(),
            Some(&file)
        );
        assert_eq!(record.code(), file.code());
        assert_eq!(record.outcome(), Outcome::IncompleteInventory);
    }
}
