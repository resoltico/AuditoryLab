// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Single owner of new-file publication and cleanup.
use crate::{AppError, Cancellation, FilesystemPhase, Operation, Origin};
#[cfg(test)]
use crate::{ErrorClass, Reason};
use std::{fs::File, io, path::Path};
use tempfile::NamedTempFile;
pub(crate) enum Failure {
    Cancelled,
    Io(io::Error),
}
fn error(
    failure: Failure,
    operation: Operation,
    origin: Option<Origin>,
    path: &Path,
    phase: FilesystemPhase,
    stage: Option<&Path>,
) -> AppError {
    match failure {
        Failure::Cancelled => AppError::cancelled(operation, origin, path),
        Failure::Io(error) => AppError::filesystem(
            operation,
            origin,
            Some(path),
            phase,
            Some(path),
            stage,
            error,
        ),
    }
}
fn cleanup(stage: NamedTempFile, failure: AppError) -> AppError {
    let path = stage.path().to_owned();
    match stage.close() {
        Ok(()) => failure,
        Err(error) => failure.with_cleanup(path, error),
    }
}
pub(crate) fn publish<T>(
    path: &Path,
    operation: Operation,
    origin: Option<Origin>,
    cancel: &Cancellation,
    write: impl FnOnce(&mut File) -> Result<T, Failure>,
) -> Result<T, AppError> {
    publish_with(
        path,
        operation,
        origin,
        cancel,
        write,
        |file| file.sync_all(),
        || {},
        |stage| stage.persist_noclobber(path),
    )
}
fn publish_with<T>(
    path: &Path,
    operation: Operation,
    origin: Option<Origin>,
    cancel: &Cancellation,
    write: impl FnOnce(&mut File) -> Result<T, Failure>,
    sync: impl FnOnce(&File) -> io::Result<()>,
    precommit: impl FnOnce(),
    commit: impl FnOnce(NamedTempFile) -> Result<File, tempfile::PersistError>,
) -> Result<T, AppError> {
    let failed = |failure, phase, stage: Option<&Path>| {
        error(failure, operation, origin.clone(), path, phase, stage)
    };
    if cancel.is_requested() {
        return Err(failed(
            Failure::Cancelled,
            FilesystemPhase::CreateStage,
            None,
        ));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut builder = tempfile::Builder::new();
    builder.prefix(".auditorylab-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut stage = builder
        .tempfile_in(parent)
        .map_err(|e| failed(Failure::Io(e), FilesystemPhase::CreateStage, None))?;
    let stage_path = stage.path().to_owned();
    let result = write(stage.as_file_mut())
        .map_err(|failure| failed(failure, FilesystemPhase::WriteStage, Some(&stage_path)))
        .and_then(|value| {
            sync(stage.as_file()).map_err(|error| {
                failed(
                    Failure::Io(error),
                    FilesystemPhase::SyncStage,
                    Some(&stage_path),
                )
            })?;
            precommit();
            if cancel.is_requested() {
                Err(failed(
                    Failure::Cancelled,
                    FilesystemPhase::Commit,
                    Some(&stage_path),
                ))
            } else {
                Ok(value)
            }
        });
    let value = match result {
        Ok(value) => value,
        Err(error) => return Err(cleanup(stage, error)),
    };
    match commit(stage) {
        Ok(_) => Ok(value),
        Err(failure) => Err(cleanup(
            failure.file,
            failed(
                Failure::Io(failure.error),
                FilesystemPhase::Commit,
                Some(&stage_path),
            ),
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        sync::{Arc, Barrier},
    };
    fn run(
        path: &Path,
        cancel: &Cancellation,
        write: impl FnOnce(&mut File) -> Result<(), Failure>,
    ) -> Result<(), AppError> {
        publish(path, Operation::Save, None, cancel, write)
    }
    #[test]
    fn stage_is_in_parent_and_destination_absent_until_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("final");
        run(&path, &Cancellation::default(), |file| {
            assert!(!path.exists());
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
            file.write_all(b"complete").map_err(Failure::Io)
        })
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"complete");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn write_sync_and_precommit_cancel_failures_remove_only_stage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("final");
        for mode in 0..3 {
            let cancel = Cancellation::default();
            let result = publish_with(
                &path,
                Operation::Save,
                None,
                &cancel,
                |f| {
                    f.write_all(b"partial").unwrap();
                    if mode == 0 {
                        Err(Failure::Io(io::Error::other("write failure")))
                    } else {
                        Ok(())
                    }
                },
                |_| {
                    if mode == 1 {
                        Err(io::Error::other("sync failure"))
                    } else {
                        Ok(())
                    }
                },
                || {
                    if mode == 2 {
                        cancel.request();
                    }
                },
                |stage| stage.persist_noclobber(&path),
            );
            let failure = result.unwrap_err();
            if mode < 2 {
                assert!(
                    matches!(failure.diagnostic(),crate::Diagnostic::Filesystem(detail) if detail.phase()==if mode==0{FilesystemPhase::WriteStage}else{FilesystemPhase::SyncStage}&&detail.stage().is_some())
                );
            }
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        }
    }
    #[test]
    fn competitor_during_write_and_existing_directory_remain_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("final");
        assert!(
            run(&path, &Cancellation::default(), |_| {
                fs::write(&path, b"competitor").unwrap();
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"competitor");
        assert!(run(dir.path(), &Cancellation::default(), |_| Ok(())).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    fn simultaneous_contenders_publish_exactly_one_complete_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("final");
        let barrier = Arc::new(Barrier::new(2));
        let workers: Vec<_> = (0..2)
            .map(|value| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    run(&path, &Cancellation::default(), |file| {
                        file.write_all(&[value; 100]).map_err(Failure::Io)?;
                        barrier.wait();
                        Ok(())
                    })
                })
            })
            .collect();
        assert_eq!(
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .filter(Result::is_ok)
                .count(),
            1
        );
        let bytes = fs::read(path).unwrap();
        assert!(bytes == vec![0; 100] || bytes == vec![1; 100]);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[cfg(unix)]
    #[test]
    fn dangling_symlink_and_inherited_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("final");
        symlink("absent", &path).unwrap();
        assert!(run(&path, &Cancellation::default(), |_| Ok(())).is_err());
        assert_eq!(fs::read_link(&path).unwrap(), Path::new("absent"));
        fs::remove_file(&path).unwrap();
        let reference = dir.path().join("reference");
        File::create_new(&reference).unwrap();
        run(&path, &Cancellation::default(), |_| Ok(())).unwrap();
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode(),
            fs::metadata(reference).unwrap().permissions().mode()
        );
    }
    #[test]
    fn cancellation_during_write_removes_stage_and_after_final_check_loses_to_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("final");
        let cancel = Cancellation::default();
        let failure = run(&path, &cancel, |file| {
            file.write_all(b"partial").unwrap();
            cancel.request();
            Err(Failure::Cancelled)
        })
        .unwrap_err();
        assert_eq!(failure.class(), ErrorClass::Cancelled);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        let cancel = Cancellation::default();
        publish_with(
            &path,
            Operation::Save,
            None,
            &cancel,
            |file| file.write_all(b"complete").map_err(Failure::Io),
            |file| file.sync_all(),
            || {},
            |stage| {
                cancel.request();
                stage.persist_noclobber(&path)
            },
        )
        .unwrap();
        assert!(cancel.is_requested());
        assert_eq!(fs::read(path).unwrap(), b"complete");
    }
    #[cfg(unix)]
    #[test]
    fn failed_cleanup_retains_stage_and_primary_cause_and_changes_cancellation_class() {
        for cancelled in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("final");
            let cancel = Cancellation::default();
            let failure = run(&path, &cancel, |_| {
                let stage = fs::read_dir(dir.path())
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                fs::remove_file(&stage).unwrap();
                fs::create_dir(&stage).unwrap();
                if cancelled {
                    Err(Failure::Cancelled)
                } else {
                    Err(Failure::Io(io::Error::other("primary write evidence")))
                }
            })
            .unwrap_err();
            assert_eq!(failure.class(), ErrorClass::Operational);
            assert!(!path.exists());
            let cleanup = failure.cleanup().unwrap();
            assert!(cleanup.stage().is_dir());
            assert_eq!(cleanup.phase(), FilesystemPhase::Cleanup);
            assert!(matches!(
                cleanup.error().kind(),
                io::ErrorKind::IsADirectory | io::ErrorKind::PermissionDenied
            ));
            assert!(failure.to_string().contains(if cancelled {
                "cancelled"
            } else {
                "primary write evidence"
            }));
            assert_eq!(
                failure.reason(),
                if cancelled {
                    Reason::Cancelled
                } else {
                    Reason::Publication
                }
            );
        }
    }
    #[test]
    fn already_exists_is_a_destination_conflict_only_at_final_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("final");
        for during_write in [true, false] {
            let failure = publish_with(
                &path,
                Operation::Export,
                None,
                &Cancellation::default(),
                |_| {
                    if during_write {
                        Err(Failure::Io(io::Error::from_raw_os_error(
                            if cfg!(windows) { 183 } else { 17 },
                        )))
                    } else {
                        Ok(())
                    }
                },
                |_| {
                    Err(io::Error::from_raw_os_error(if cfg!(windows) {
                        183
                    } else {
                        17
                    }))
                },
                || {},
                |stage| stage.persist_noclobber(&path),
            )
            .unwrap_err();
            assert_eq!(failure.reason(), Reason::Publication);
            assert!(!path.exists());
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
            assert_eq!(
                std::error::Error::source(&failure)
                    .unwrap()
                    .downcast_ref::<io::Error>()
                    .unwrap()
                    .raw_os_error(),
                Some(if cfg!(windows) { 183 } else { 17 })
            );
        }
        fs::write(&path, b"untouched").unwrap();
        let failure = run(&path, &Cancellation::default(), |_| Ok(())).unwrap_err();
        assert_eq!(failure.reason(), Reason::DestinationExists);
        assert_eq!(fs::read(path).unwrap(), b"untouched");
    }
}
