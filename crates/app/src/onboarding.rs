//! One optional, one-way desktop orientation preference, independent of the library.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::job::Cancel;
use crate::storage::error::{FileError, IoFailure, IoOperation};
use crate::storage::publish::{self, Access, Policy};
use cap_std::fs::Dir;
use std::io::Write;
use std::path::{Path, PathBuf};

const NAME: &str = "onboarding-seen";
const SEEN: &[u8] = b"auditorylab-onboarding-seen 1\n";

/// A failure to remember orientation never prevents ordinary work.
#[derive(Debug)]
pub enum Problem {
    /// The operating system supplied no application configuration location.
    ConfigurationUnavailable,
    /// Existing bytes are unsupported and must remain intact.
    Unsupported { path: PathBuf },
    /// Retained operation context and original typed filesystem failure.
    File {
        path: PathBuf,
        operation: IoOperation,
        error: FileError,
    },
    /// The flag was published, with its original nonfatal storage notices.
    Published {
        path: PathBuf,
        warnings: Vec<crate::services::StorageWarning>,
    },
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigurationUnavailable => f.write_str("onboarding.configuration_unavailable"),
            Self::Unsupported { path } => write!(f, "onboarding.unsupported: {}", path.display()),
            Self::File {
                path,
                operation,
                error,
            } => {
                write!(
                    f,
                    "onboarding.file ({operation:?}): {}: {error}",
                    path.display()
                )
            }
            Self::Published { path, .. } => {
                write!(f, "onboarding.published_with_warning: {}", path.display())
            }
        }
    }
}

impl std::error::Error for Problem {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::File { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// The dedicated configuration directory. Resolving it performs no I/O.
#[derive(Debug)]
pub struct Preference {
    directory: PathBuf,
}

impl Preference {
    /// Bind an already-resolved dedicated configuration directory.
    /// Enables isolated host/service verification without changing process-wide preferences.
    pub fn at_config_directory(directory: PathBuf) -> Self {
        Self { directory }
    }

    /// Use the same platform identity as the library, but its configuration location.
    /// Library directory overrides deliberately do not affect this preference.
    pub fn configured() -> Result<Self, Problem> {
        directories::ProjectDirs::from("org", "resoltico", "auditorylab")
            .map(|project| Self::at_config_directory(project.config_dir().to_owned()))
            .ok_or(Problem::ConfigurationUnavailable)
    }

    /// Missing means fresh. Reading never creates files or repairs unknown bytes.
    pub fn seen(&self) -> Result<bool, Problem> {
        let dir = match Dir::open_ambient_dir(&self.directory, cap_std::ambient_authority()) {
            Ok(dir) => dir,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(self.io(IoOperation::OpenDirectory, error)),
        };
        self.read(&dir)
    }

    fn file_problem(&self, operation: IoOperation, error: FileError) -> Problem {
        Problem::File {
            path: self.directory.join(NAME),
            operation,
            error,
        }
    }

    fn io(&self, operation: IoOperation, error: std::io::Error) -> Problem {
        self.file_problem(
            operation,
            FileError::Io(IoFailure::new(
                operation,
                Some(self.directory.clone()),
                error,
            )),
        )
    }

    fn read(&self, dir: &Dir) -> Result<bool, Problem> {
        let path = self.directory.join(NAME);
        match dir.symlink_metadata(NAME) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(self.file_problem(
                    IoOperation::Inspect,
                    FileError::Io(IoFailure::new(IoOperation::Inspect, Some(path), error)),
                ));
            }
            Ok(metadata) if !metadata.is_file() => {
                return Err(self.file_problem(IoOperation::Read, FileError::NotRegular { path }));
            }
            Ok(_) => {}
        }
        let bytes = publish::read_bounded(dir, Path::new(NAME), 128)
            .map_err(|error| self.file_problem(IoOperation::Read, error))?;
        if bytes == SEEN {
            Ok(true)
        } else {
            Err(Problem::Unsupported { path })
        }
    }

    /// Atomically publish the flag after deliberate departure from learning.
    /// Existing files are never replaced, including malformed or concurrently created files.
    pub fn mark_seen(&self) -> Result<(), Problem> {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&self.directory)
            .map_err(|error| self.io(IoOperation::CreateDirectory, error))?;
        let dir = Dir::open_ambient_dir(&self.directory, cap_std::ambient_authority())
            .map_err(|error| self.io(IoOperation::OpenDirectory, error))?;
        if self.read(&dir)? {
            return Ok(());
        }
        let path = self.directory.join(NAME);
        let published = publish::publish_in(
            &dir,
            Path::new(NAME),
            &path,
            Policy::CreateNew,
            Access::Private,
            &Cancel::default(),
            |file| {
                file.write_all(SEEN).map_err(|error| {
                    FileError::Io(IoFailure::new(
                        IoOperation::Write,
                        Some(path.clone()),
                        error,
                    ))
                })
            },
        );
        match published {
            Ok(result) if result.warnings.is_empty() => Ok(()),
            Ok(result) => Err(Problem::Published {
                path,
                warnings: result.warnings,
            }),
            Err(error) => {
                // Another instance may have published the exact same one-way flag.
                if matches!(self.read(&dir), Ok(true)) {
                    Ok(())
                } else {
                    Err(self.file_problem(IoOperation::Publish, error))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> std::io::Result<tempfile::TempDir> {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
        std::fs::create_dir_all(&base)?;
        tempfile::tempdir_in(base)
    }

    #[test]
    fn fresh_read_and_crash_do_not_create_a_flag() {
        let root = scratch().unwrap();
        let preference = Preference {
            directory: root.path().join("config"),
        };
        assert!(!preference.seen().unwrap());
        assert!(!preference.directory.exists());
        drop(preference);
        let preference = Preference {
            directory: root.path().join("config"),
        };
        assert!(!preference.seen().unwrap());
        preference.mark_seen().unwrap();
        assert!(preference.seen().unwrap());
        preference.mark_seen().unwrap();
        assert_eq!(
            std::fs::read(preference.directory.join(NAME)).unwrap(),
            SEEN
        );
        assert_eq!(std::fs::read_dir(&preference.directory).unwrap().count(), 1);
    }

    #[test]
    fn malformed_bytes_and_unavailable_directory_are_preserved() {
        let root = scratch().unwrap();
        let preference = Preference {
            directory: root.path().to_owned(),
        };
        let path = root.path().join(NAME);
        std::fs::write(&path, b"future or user data").unwrap();
        assert!(matches!(
            preference.seen(),
            Err(Problem::Unsupported { .. })
        ));
        assert!(matches!(
            preference.mark_seen(),
            Err(Problem::Unsupported { .. })
        ));
        assert_eq!(std::fs::read(&path).unwrap(), b"future or user data");
        let blocked = Preference {
            directory: path.clone(),
        };
        assert!(blocked.seen().is_err());
        assert!(blocked.mark_seen().is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"future or user data");
    }

    #[cfg(unix)]
    #[test]
    fn read_only_directory_and_symlink_cannot_clobber_user_content() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let root = scratch().unwrap();
        let config = root.path().join("config");
        std::fs::create_dir(&config).unwrap();
        let preference = Preference {
            directory: config.clone(),
        };
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = preference.mark_seen();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        let external = root.path().join("external");
        std::fs::write(&external, b"keep me").unwrap();
        symlink(&external, config.join(NAME)).unwrap();
        assert!(preference.seen().is_err());
        assert!(preference.mark_seen().is_err());
        assert_eq!(std::fs::read(external).unwrap(), b"keep me");
    }
    #[test]
    fn oversized_preference_failure_retains_path_operation_and_typed_cause() {
        let root = scratch().unwrap();
        let preference = Preference::at_config_directory(root.path().to_owned());
        let path = root.path().join(NAME);
        std::fs::write(&path, [0u8; 129]).unwrap();
        let problem = preference.seen().unwrap_err();
        assert!(problem.to_string().contains(path.to_str().unwrap()));
        assert!(matches!(problem, Problem::File {
            path: ref actual, operation: IoOperation::Read,
            error: FileError::ReadLimit { grew: false, .. },
        } if actual == &path));
        assert!(std::error::Error::source(&problem).is_some());
        assert_eq!(std::fs::read(path).unwrap(), [0u8; 129]);
    }
}
