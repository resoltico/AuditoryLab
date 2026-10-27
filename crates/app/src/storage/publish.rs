//! Exclusive adjacent staging, atomic publication and verification of ambiguous I/O results.
use crate::document::origin::Fingerprint;
use crate::job::{Cancel, Replace};
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use {
    crate::storage::error::Confirmation, crate::storage::error::FileError as Error,
    crate::storage::error::IoCause, crate::storage::error::IoFailure,
    crate::storage::error::IoOperation, crate::storage::error::MutationPhase,
};

#[derive(Clone, Copy)]
pub(crate) enum Policy<'a> {
    CreateNew,
    Replace,
    ReplaceIfUnchanged {
        fingerprint: Fingerprint,
        identity: &'a same_file::Handle,
    },
}
#[derive(Debug)]
pub(crate) struct Published<T> {
    pub(crate) value: T,
    pub(crate) warnings: Vec<String>,
}
/// Open without following the last symlink or waiting for a FIFO.
pub(crate) fn open_regular(dir: &Dir, name: &Path) -> Result<File, Error> {
    open_regular_with(dir, name, OpenOptions::new())
}
fn open_regular_with(dir: &Dir, name: &Path, mut options: OpenOptions) -> Result<File, Error> {
    options.read(true).follow(FollowSymlinks::No);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = dir
        .open_with(name, &options)
        .map_err(|e| io(name, IoOperation::Open, e))?;
    if !file
        .metadata()
        .map_err(|e| io(name, IoOperation::Inspect, e))?
        .is_file()
    {
        return Err(Error::NotRegular {
            path: name.to_owned(),
        });
    }
    Ok(file.into_std())
}
pub(crate) fn read_bounded(dir: &Dir, name: &Path, limit: usize) -> Result<Vec<u8>, Error> {
    read_bounded_counted(dir, name, limit, &mut 0)
}
pub(crate) fn read_bounded_counted(
    dir: &Dir,
    name: &Path,
    limit: usize,
    consumed: &mut usize,
) -> Result<Vec<u8>, Error> {
    let file = open_regular(dir, name)?;
    if file
        .metadata()
        .map_err(|e| io(name, IoOperation::Inspect, e))?
        .len()
        > u64::try_from(limit).unwrap_or(u64::MAX)
    {
        return Err(Error::ReadLimit {
            path: name.to_owned(),
            grew: false,
        });
    }
    let mut bytes = Vec::new();
    let result = file
        .take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes);
    *consumed = consumed.saturating_add(bytes.len());
    result.map_err(|e| io(name, IoOperation::Read, e))?;
    if bytes.len() > limit {
        return Err(Error::ReadLimit {
            path: name.to_owned(),
            grew: true,
        });
    }
    Ok(bytes)
}

/// Pre-commit reads honor cancellation; post-commit verification passes None to preserve publication truth.
fn hash(file: &mut File, path: &Path, cancel: Option<&Cancel>) -> Result<Fingerprint, Error> {
    if cancel.is_some_and(Cancel::is_requested) {
        return Err(cancelled(path));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|e| io(path, IoOperation::Seek, e))?;
    let mut digest = Sha256::new();
    let mut block = [0; 64 * 1024];
    loop {
        if cancel.is_some_and(Cancel::is_requested) {
            return Err(cancelled(path));
        }
        let count = match file.read(&mut block) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io(path, IoOperation::Read, error)),
        };
        if count == 0 {
            break;
        }
        digest.update(&block[..count]);
        #[cfg(test)]
        HASH_BLOCK.with(|slot| {
            if let Some(hook) = slot.borrow_mut().as_mut() {
                hook();
            }
        });
    }
    Ok(Fingerprint::from_digest(digest.finalize().into()))
}
fn observed(dir: &Dir, name: &Path, cancel: Option<&Cancel>) -> Result<Option<Fingerprint>, Error> {
    match dir.symlink_metadata(name) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io(name, IoOperation::Inspect, error)),
        Ok(_) => hash(&mut open_regular(dir, name)?, name, cancel).map(Some),
    }
}

fn io(path: &Path, action: IoOperation, error: std::io::Error) -> Error {
    Error::Io(IoFailure::new(action, Some(path.to_owned()), error))
}
fn cancelled(path: &Path) -> Error {
    Error::Cancelled {
        path: Some(path.to_owned()),
    }
}
fn failed(
    path: &Path,
    phase: MutationPhase,
    confirmation: Confirmation,
    error: std::io::Error,
) -> Error {
    Error::AfterMutation {
        path: path.to_owned(),
        phase,
        confirmation,
        cause: IoCause::new(error),
    }
}
fn cleanup(dir: &Dir, stage: &Path, path: &Path, error: Error) -> Error {
    match fault(FaultPoint::Cleanup).and_then(|()| dir.remove_file(stage)) {
        Ok(()) => error,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => error,
        Err(e) => error.with_leftover(path.parent().unwrap_or(Path::new(".")).join(stage), e),
    }
}
pub(crate) fn sync_directory(dir: &Dir) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        // A capability directory can use O_PATH on Linux, which cannot be fsynced.
        // Opening its own "." yields a read descriptor without resolving an ambient path.
        dir.open(".")?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}
/// Publish within an already-open parent, retaining its capability across pathname changes.
pub(crate) fn publish_in<T>(
    dir: &Dir,
    name: &Path,
    path: &Path,
    policy: Policy<'_>,
    cancel: &Cancel,
    write: impl FnOnce(&mut File) -> Result<T, Error>,
) -> Result<Published<T>, Error> {
    if cancel.is_requested() {
        return Err(cancelled(path));
    }
    if matches!(policy, Policy::CreateNew) && dir.symlink_metadata(name).is_ok() {
        return Err(Error::DestinationExists {
            path: path.to_owned(),
            cause: None,
        });
    }
    let previous = if matches!(policy, Policy::CreateNew) {
        None
    } else {
        observed(dir, name, Some(cancel))?
    };
    let stage = PathBuf::from(format!(
        ".auditorylab-{}.partial",
        crate::identity::random_uuid()?
    ));
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    fault(FaultPoint::StageCreate).map_err(|e| io(path, IoOperation::CreateStage, e))?;
    let mut file = dir
        .open_with(&stage, &options)
        .map_err(|e| io(path, IoOperation::CreateStage, e))?
        .into_std();
    let prepared = (|| {
        fault(FaultPoint::Write).map_err(|e| io(path, IoOperation::Write, e))?;
        let value = write(&mut file)?;
        file.flush()
            .and_then(|()| fault(FaultPoint::FileSync))
            .and_then(|()| file.sync_all())
            .map_err(|e| io(path, IoOperation::SyncFile, e))?;
        let fingerprint = hash(&mut file, path, Some(cancel))?;
        if let Policy::ReplaceIfUnchanged {
            fingerprint: expected,
            identity,
        } = policy
        {
            let mut latest = open_regular(dir, name)?;
            let found = hash(&mut latest, path, Some(cancel))?;
            let handle = same_file::Handle::from_file(latest)
                .map_err(|e| io(path, IoOperation::CheckIdentity, e))?;
            if found != expected || handle != *identity {
                return Err(Error::Changed {
                    path: path.to_owned(),
                });
            }
        }
        if cancel.is_requested() {
            return Err(cancelled(path));
        }
        Ok((value, fingerprint))
    })();
    let (value, candidate) = match prepared {
        Ok(v) => v,
        Err(e) => return Err(cleanup(dir, &stage, path, e)),
    };
    pause(FaultPoint::BeforePublish);
    let publication = fault(FaultPoint::BeforePublish)
        .and_then(|()| match policy {
            Policy::CreateNew => dir.hard_link(&stage, dir, name),
            Policy::Replace | Policy::ReplaceIfUnchanged { .. } => dir.rename(&stage, dir, name),
        })
        .and_then(|()| fault(FaultPoint::AfterPublish));
    pause(FaultPoint::AfterPublish);
    let mut warnings = Vec::new();
    if let Err(error) = publication {
        if matches!(policy, Policy::CreateNew) && error.kind() == std::io::ErrorKind::AlreadyExists
        {
            return Err(cleanup(
                dir,
                &stage,
                path,
                Error::DestinationExists {
                    path: path.to_owned(),
                    cause: Some(IoCause::new(error)),
                },
            ));
        }
        let found = fault(FaultPoint::Verify)
            .map_err(|e| io(path, IoOperation::Inspect, e))
            .and_then(|()| observed(dir, name, None));
        match found {
            Ok(Some(fingerprint)) if fingerprint == candidate => warnings.push(
                crate::presentation::publication_text::saved_after_io(&error),
            ),
            Ok(found) if found == previous => {
                return Err(cleanup(
                    dir,
                    &stage,
                    path,
                    failed(path, MutationPhase::Publish, Confirmation::Unchanged, error),
                ));
            }
            Err(verification_error) => {
                return Err(cleanup(
                    dir,
                    &stage,
                    path,
                    failed(
                        path,
                        MutationPhase::Publish,
                        Confirmation::Unavailable(Box::new(verification_error)),
                        error,
                    ),
                ));
            }
            Ok(_) => {
                return Err(cleanup(
                    dir,
                    &stage,
                    path,
                    failed(path, MutationPhase::Publish, Confirmation::Different, error),
                ));
            }
        }
    }
    drop(file);
    if let Err(error) = fault(FaultPoint::DirectorySync).and_then(|()| sync_directory(dir)) {
        match observed(dir, name, None) {
            Ok(Some(found)) if found == candidate => warnings.push(
                crate::presentation::publication_text::saved_durability(&error),
            ),
            Ok(found) if found == previous => {
                return Err(cleanup(
                    dir,
                    &stage,
                    path,
                    failed(
                        path,
                        MutationPhase::SyncPublication,
                        Confirmation::Unchanged,
                        error,
                    ),
                ));
            }
            found => {
                let confirmation = match found {
                    Ok(_) => Confirmation::Different,
                    Err(error) => Confirmation::Unavailable(Box::new(error)),
                };
                return Err(cleanup(
                    dir,
                    &stage,
                    path,
                    failed(path, MutationPhase::SyncPublication, confirmation, error),
                ));
            }
        }
    }
    if matches!(policy, Policy::CreateNew)
        && let Err(error) = fault(FaultPoint::Cleanup).and_then(|()| dir.remove_file(&stage))
    {
        warnings.push(crate::presentation::publication_text::retained_stage(
            &stage, &error,
        ));
    }
    Ok(Published { value, warnings })
}
pub(crate) fn publish<T>(
    path: &Path,
    replace: Replace,
    cancel: &Cancel,
    write: impl FnOnce(&mut File) -> Result<T, Error>,
) -> Result<Published<T>, Error> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().ok_or(Error::MissingFilename)?;
    let dir = Dir::open_ambient_dir(parent, cap_std::ambient_authority())
        .map_err(|e| io(path, IoOperation::OpenDirectory, e))?;
    let result = publish_in(
        &dir,
        Path::new(name),
        path,
        match replace {
            Replace::Never => Policy::CreateNew,
            Replace::Allowed => Policy::Replace,
        },
        cancel,
        write,
    )?;
    Ok(result)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaultPoint {
    StageCreate,
    Write,
    FileSync,
    BeforePublish,
    AfterPublish,
    DirectorySync,
    Cleanup,
    Verify,
    BeforeLink,
    AfterLink,
    BeforeRemove,
    AfterRemove,
}
#[cfg(test)]
thread_local! {static HASH_BLOCK:std::cell::RefCell<Option<Box<dyn FnMut()>>>=const {std::cell::RefCell::new(None)};}
#[cfg(test)]
thread_local! {static AFTER_PUBLISH: std::cell::RefCell<Option<Box<dyn FnOnce()>>>=const {std::cell::RefCell::new(None)};static INJECT: std::cell::RefCell<Vec<FaultPoint>>=const {std::cell::RefCell::new(Vec::new())};}
fn fault(point: FaultPoint) -> std::io::Result<()> {
    #[cfg(test)]
    {
        if point == FaultPoint::AfterPublish {
            AFTER_PUBLISH.with(|slot| {
                if let Some(hook) = slot.borrow_mut().take() {
                    hook();
                }
            });
        }
        if INJECT.with(|slot| {
            let mut faults = slot.borrow_mut();
            if let Some(i) = faults.iter().position(|p| *p == point) {
                faults.remove(i);
                true
            } else {
                false
            }
        }) {
            return Err(std::io::Error::other(format!("injected {point:?} failure")));
        }
    }
    let _ = point;
    Ok(())
}
fn pause(point: FaultPoint) {
    #[cfg(test)]
    if std::env::var("AUDITORYLAB_PUBLISH_PAUSE").ok().as_deref()
        == Some(match point {
            FaultPoint::BeforePublish => "before",
            FaultPoint::AfterPublish => "after",
            FaultPoint::BeforeLink => "before-link",
            FaultPoint::AfterLink => "after-link",
            FaultPoint::AfterRemove => "after-remove",
            _ => "",
        })
        && let Some(path) = std::env::var_os("AUDITORYLAB_PUBLISH_READY")
    {
        std::fs::write(path, b"ready")
            .unwrap_or_else(|e| panic!("cannot signal publication fixture: {e}"));
        loop {
            std::thread::park();
        }
    }
    let _ = point;
}
#[cfg(test)]
mod tests;

pub(crate) fn same_file(
    first: &Dir,
    first_name: &Path,
    second: &Dir,
    second_name: &Path,
) -> Result<bool, Error> {
    let a = same_file::Handle::from_file(open_regular(first, first_name)?)
        .map_err(|e| io(first_name, IoOperation::RetainIdentity, e))?;
    let b = same_file::Handle::from_file(open_regular(second, second_name)?)
        .map_err(|e| io(second_name, IoOperation::RetainIdentity, e))?;
    Ok(a == b)
}
pub(crate) fn relocate(
    from: &Dir,
    to: &Dir,
    name: &Path,
    source: &Path,
    destination: &Path,
    cancel: &Cancel,
) -> Result<Vec<String>, Error> {
    if cancel.is_requested() {
        return Err(cancelled(destination));
    }
    if to.symlink_metadata(name).is_ok() {
        return Err(Error::RelocationExists {
            path: destination.to_owned(),
        });
    }
    #[cfg(windows)]
    let mut source_file = {
        // Windows FlushFileBuffers requires GENERIC_WRITE. Retain the same
        // safely opened regular file for flushing and hashing; never truncate it.
        let mut options = OpenOptions::new();
        options.write(true);
        open_regular_with(from, name, options)?
    };
    #[cfg(not(windows))]
    let mut source_file = open_regular(from, name)?;
    source_file
        .sync_all()
        .map_err(|e| io(source, IoOperation::SyncFile, e))?;
    let expected = hash(&mut source_file, source, Some(cancel))?;
    if cancel.is_requested() {
        return Err(cancelled(destination));
    }
    pause(FaultPoint::BeforeLink);
    fault(FaultPoint::BeforeLink)
        .and_then(|()| from.hard_link(name, to, name))
        .map_err(|e| io(destination, IoOperation::Relocate, e))?;
    pause(FaultPoint::AfterLink);
    let removal = fault(FaultPoint::AfterLink)
        .and_then(|()| fault(FaultPoint::BeforeRemove))
        .and_then(|()| from.remove_file(name))
        .and_then(|()| fault(FaultPoint::AfterRemove));
    pause(FaultPoint::AfterRemove);
    let mut warnings = Vec::new();
    if let Err(error) = removal {
        match fault(FaultPoint::Verify).and_then(|()| from.symlink_metadata(name)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => warnings.push(
                crate::presentation::publication_text::moved_after_io(&error),
            ),
            Ok(_)
                if destination.parent().and_then(Path::file_name)
                    == Some(std::ffi::OsStr::new("trash"))
                    && same_file(from, name, to, name)? =>
            {
                warnings.push(crate::presentation::publication_text::trash_link(
                    source, &error,
                ))
            }
            Ok(_) => {
                return Err(failed(
                    destination,
                    MutationPhase::Relocate {
                        source: source.to_owned(),
                    },
                    Confirmation::Unchanged,
                    error,
                ));
            }
            Err(verification_error) => {
                let verification = io(source, IoOperation::Inspect, verification_error);
                return Err(failed(
                    destination,
                    MutationPhase::Relocate {
                        source: source.to_owned(),
                    },
                    Confirmation::Unavailable(Box::new(verification)),
                    error,
                ));
            }
        }
    }
    for dir in [from, to] {
        if let Err(error) = fault(FaultPoint::DirectorySync).and_then(|()| sync_directory(dir)) {
            match observed(to, name, None) {
                Ok(Some(found)) if found == expected => {}
                found => {
                    let confirmation = match found {
                        Ok(_) => Confirmation::Different,
                        Err(error) => Confirmation::Unavailable(Box::new(error)),
                    };
                    return Err(failed(
                        destination,
                        MutationPhase::SyncRelocation,
                        confirmation,
                        error,
                    ));
                }
            }
            warnings.push(crate::presentation::publication_text::moved_durability(
                &error,
            ));
        }
    }
    Ok(warnings)
}
pub(crate) fn remove(dir: &Dir, name: &Path, path: &Path) -> Result<Vec<String>, Error> {
    let result = fault(FaultPoint::BeforeRemove)
        .and_then(|()| dir.remove_file(name))
        .and_then(|()| fault(FaultPoint::AfterRemove));
    pause(FaultPoint::AfterRemove);
    let mut warnings = Vec::new();
    if let Err(error) = result {
        match fault(FaultPoint::Verify).and_then(|()| dir.symlink_metadata(name)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => warnings.push(
                crate::presentation::publication_text::deleted_after_io(&error),
            ),
            Ok(_) => {
                return Err(failed(
                    path,
                    MutationPhase::Remove,
                    Confirmation::Unchanged,
                    error,
                ));
            }
            Err(verification_error) => {
                let verification = io(path, IoOperation::Inspect, verification_error);
                return Err(failed(
                    path,
                    MutationPhase::Remove,
                    Confirmation::Unavailable(Box::new(verification)),
                    error,
                ));
            }
        }
    }
    if let Err(error) = fault(FaultPoint::DirectorySync).and_then(|()| sync_directory(dir)) {
        match dir.symlink_metadata(name) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => warnings.push(
                crate::presentation::publication_text::deleted_durability(&error),
            ),
            found => {
                let confirmation = match found {
                    Ok(_) => Confirmation::Different,
                    Err(error) => {
                        Confirmation::Unavailable(Box::new(io(path, IoOperation::Inspect, error)))
                    }
                };
                return Err(failed(
                    path,
                    MutationPhase::SyncRemoval,
                    confirmation,
                    error,
                ));
            }
        }
    }
    Ok(warnings)
}
