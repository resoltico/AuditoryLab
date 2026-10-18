//! Atomic publication of completed files.
use crate::{Cancel, Error, ErrorKind, Replace};
use std::{fs::File, path::Path};
use tempfile::NamedTempFile;
fn cleanup(stage: NamedTempFile, failure: Error) -> Error {
    let path = stage.path().to_owned();
    match stage.close() {
        Ok(()) => failure,
        Err(e) => failure.with_leftover(path, e),
    }
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
            let kind = if e.kind() == std::io::ErrorKind::AlreadyExists {
                ErrorKind::DestinationExists
            } else {
                ErrorKind::Write
            };
            let message = if kind == ErrorKind::DestinationExists {
                format!(
                    "cannot publish to {path:?}: the file already exists and was left untouched"
                )
            } else {
                format!("cannot publish to {path:?}: {e}")
            };
            Err(cleanup(failure.file, Error::caused(kind, message, e)))
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
}
