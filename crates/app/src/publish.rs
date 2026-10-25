//! Atomic publication of completed files.
//!
//! The sequence is: check cancellation; for `Replace::Never`, reject an
//! existing destination before creating anything; stage in the destination
//! directory; write; sync; check cancellation once more; publish the completed file.
//! Confirmed replacement is atomic; no-overwrite persistence is not guaranteed
//! atomic on every platform and may retain the staging link.
//! Successful persistence is the publication's linearization point: a
//! cancellation requested afterwards cannot unpublish it. Failure cleanup
//! removes only the operation's own stage. Syncing the stage is not a claim
//! of durability through a system crash or power loss.
use crate::error::{Error, ErrorKind};
use crate::workflow::{Cancel, Replace};
use std::{fs::File, path::Path};
use tempfile::NamedTempFile;
fn cleanup(stage: NamedTempFile, failure: Error) -> Error {
    let path = stage.path().to_owned();
    match stage.close() {
        Ok(()) => failure,
        Err(e) => failure.with_leftover(path, e),
    }
}
/// Reject any existing entry, including directories and dangling symbolic
/// links, before staging or writing. This avoids needless work; the final
/// no-clobber persistence still decides a race with another creator.
fn preflight(path: &Path) -> Result<(), Error> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(exists(path)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::caused(
            ErrorKind::Write,
            format!("cannot check whether {path:?} exists: {e}"),
            e,
        )),
    }
}
fn exists(path: &Path) -> Error {
    Error::new(
        ErrorKind::DestinationExists,
        format!("cannot publish to {path:?}: the file already exists and was left untouched"),
    )
}
pub(crate) fn publish<T>(
    path: &Path,
    replace: Replace,
    cancel: &Cancel,
    write: impl FnOnce(&mut File) -> Result<T, Error>,
) -> Result<T, Error> {
    let cancelled = || {
        Error::new(
            ErrorKind::Cancelled,
            format!("publication to {path:?} cancelled"),
        )
    };
    if cancel.is_requested() {
        return Err(cancelled());
    }
    if replace == Replace::Never {
        preflight(path)?;
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mut end = name.len().min(64);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = format!(".{}.", &name[..end]);
    let mut builder = tempfile::Builder::new();
    builder.prefix(&prefix).suffix(".partial");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut stage = builder.tempfile_in(parent).map_err(|e| {
        Error::caused(
            ErrorKind::Write,
            format!("cannot create a partial file for {path:?}: {e}"),
            e,
        )
    })?;
    let result = write(stage.as_file_mut()).and_then(|value| {
        stage.as_file().sync_all().map_err(|e| {
            Error::caused(ErrorKind::Write, format!("cannot sync {path:?}: {e}"), e)
        })?;
        if cancel.is_requested() {
            Err(cancelled())
        } else {
            Ok(value)
        }
    });
    let value = match result {
        Ok(value) => value,
        Err(e) => return Err(cleanup(stage, e)),
    };
    let committed = match replace {
        Replace::Never => stage.persist_noclobber(path),
        Replace::Allowed => stage.persist(path),
    };
    match committed {
        Ok(_) => Ok(value),
        Err(failure) => {
            let e = failure.error;
            let error = if e.kind() == std::io::ErrorKind::AlreadyExists {
                let message = exists(path).to_string();
                Error::caused(ErrorKind::DestinationExists, message, e)
            } else {
                Error::caused(
                    ErrorKind::Write,
                    format!("cannot publish to {path:?}: {e}"),
                    e,
                )
            };
            Err(cleanup(failure.file, error))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write};
    fn dir() -> tempfile::TempDir {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
        fs::create_dir_all(&base).unwrap();
        tempfile::tempdir_in(base).unwrap()
    }
    #[test]
    fn publication_replaces_only_when_allowed_and_cleans_failed_writes() {
        let dir = dir();
        let path = dir.path().join("file");
        fs::write(&path, b"old").unwrap();
        let write = |f: &mut File| {
            f.write_all(b"complete")
                .map_err(|e| Error::caused(ErrorKind::Write, "write failed", e))
        };
        assert_eq!(
            publish(&path, Replace::Never, &Cancel::default(), write)
                .unwrap_err()
                .kind(),
            ErrorKind::DestinationExists
        );
        assert_eq!(fs::read(&path).unwrap(), b"old");
        publish(&path, Replace::Allowed, &Cancel::default(), write).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"complete");
        for cancelled in [false, true] {
            let cancel = Cancel::default();
            let e = publish(&path, Replace::Allowed, &cancel, |_| {
                if cancelled {
                    cancel.request();
                    Ok(())
                } else {
                    Err(Error::new(ErrorKind::Write, "injected failure"))
                }
            })
            .unwrap_err();
            assert_eq!(
                e.kind(),
                if cancelled {
                    ErrorKind::Cancelled
                } else {
                    ErrorKind::Write
                }
            );
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
            assert_eq!(fs::read(&path).unwrap(), b"complete");
        }
    }
    #[cfg(unix)]
    #[test]
    fn cleanup_failure_names_the_leftover_and_preserves_primary_error() {
        let dir = dir();
        let path = dir.path().join("file");
        let e = publish(
            &path,
            Replace::Never,
            &Cancel::default(),
            |_| -> Result<(), Error> {
                let stage = fs::read_dir(dir.path())
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                fs::remove_file(&stage).unwrap();
                fs::create_dir(&stage).unwrap();
                Err(Error::new(ErrorKind::Write, "injected failure"))
            },
        )
        .unwrap_err();
        assert!(e.leftover().unwrap().is_dir());
        assert!(e.to_string().contains("injected failure"));
        assert!(!path.exists());
    }
    #[test]
    fn long_unicode_stage_names_are_bounded_and_pre_cancel_creates_nothing() {
        let dir = dir();
        let path = dir.path().join("é".repeat(100));
        let cancel = Cancel::default();
        cancel.request();
        assert!(publish(&path, Replace::Never, &cancel, |_| Ok(())).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        publish(&path, Replace::Never, &Cancel::default(), |_| {
            let name = fs::read_dir(dir.path())
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .file_name();
            assert!(name.len() < 100);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn every_existing_entry_is_rejected_before_staging_or_writing() {
        let dir = dir();
        let file = dir.path().join("file");
        fs::write(&file, b"old").unwrap();
        let directory = dir.path().join("directory");
        fs::create_dir(&directory).unwrap();
        let existing = vec![file.clone(), directory];
        #[cfg(unix)]
        let existing = {
            let mut existing = existing;
            let link = dir.path().join("link");
            std::os::unix::fs::symlink(&file, &link).unwrap();
            let dangling = dir.path().join("dangling");
            std::os::unix::fs::symlink(dir.path().join("absent"), &dangling).unwrap();
            existing.extend([link, dangling]);
            existing
        };
        let entries = fs::read_dir(dir.path()).unwrap().count();
        for path in existing {
            let written = std::cell::Cell::new(false);
            let error = publish(&path, Replace::Never, &Cancel::default(), |_| {
                written.set(true);
                Ok(())
            })
            .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::DestinationExists, "{path:?}");
            assert!(!written.get(), "the writer must not run for {path:?}");
            assert_eq!(
                fs::read_dir(dir.path()).unwrap().count(),
                entries,
                "no stage"
            );
        }
        assert_eq!(fs::read(&file).unwrap(), b"old");
        // A missing destination proceeds.
        publish(
            &dir.path().join("new"),
            Replace::Never,
            &Cancel::default(),
            |f| {
                f.write_all(b"new")
                    .map_err(|e| Error::caused(ErrorKind::Write, "write", e))
            },
        )
        .unwrap();
        // Another inspection failure stays actionable rather than meaning absence.
        #[cfg(unix)]
        {
            let error = publish(
                &file.join("below-a-file"),
                Replace::Never,
                &Cancel::default(),
                |_| Ok(()),
            )
            .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Write);
            assert!(error.to_string().contains("cannot check"), "{error}");
        }
    }

    #[test]
    fn a_destination_created_after_preflight_is_preserved_by_atomic_persistence() {
        let dir = dir();
        let path = dir.path().join("raced");
        let error = publish(&path, Replace::Never, &Cancel::default(), |f| {
            // The competitor appears after preflight and before persistence.
            fs::write(&path, b"competitor").unwrap();
            f.write_all(b"ours")
                .map_err(|e| Error::caused(ErrorKind::Write, "write", e))
        })
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::DestinationExists);
        assert_eq!(fs::read(&path).unwrap(), b"competitor");
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "stage removed"
        );
    }

    #[test]
    fn cancellation_after_persistence_cannot_unpublish() {
        let dir = dir();
        let path = dir.path().join("published");
        let cancel = Cancel::default();
        let result = publish(&path, Replace::Never, &cancel, |f| {
            f.write_all(b"done")
                .map_err(|e| Error::caused(ErrorKind::Write, "write", e))
        });
        // Cancellation racing or following successful persistence.
        cancel.request();
        assert!(result.is_ok());
        assert_eq!(fs::read(&path).unwrap(), b"done");
        // Cancellation observed at the final check cleans only the stage.
        let late = Cancel::default();
        let other = dir.path().join("late");
        let error = publish(&other, Replace::Never, &late, |f| {
            late.request();
            f.write_all(b"late")
                .map_err(|e| Error::caused(ErrorKind::Write, "write", e))
        })
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Cancelled);
        assert!(!other.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
