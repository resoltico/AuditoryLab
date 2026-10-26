// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Admission of document handles, distinct from intentionally streaming readers.
use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};

pub(super) enum Failure {
    Io(io::Error),
    NotRegular,
}

pub(super) fn open(path: &Path) -> Result<File, Failure> {
    admit(open_handle(path).map_err(Failure::Io)?)
}

fn open_handle(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    // Unlike a path-only type check, nonblocking open also handles a regular
    // path being replaced by a FIFO before open. Regular-file reads are unchanged.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    options.open(path)
}

fn admit(file: File) -> Result<File, Failure> {
    // Reject non-disk Windows handles before querying filesystem metadata or
    // reading any bytes; document admission does not consume streams.
    #[cfg(windows)]
    if !winapi_util::file::typ(&file)
        .map_err(Failure::Io)?
        .is_disk()
    {
        return Err(Failure::NotRegular);
    }
    // Inspect the opened object, never a second lookup of its pathname. A disk
    // handle still needs this check to distinguish regular files from directories.
    if !file.metadata().map_err(Failure::Io)?.is_file() {
        return Err(Failure::NotRegular);
    }
    Ok(file)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::symlink, process::Command};

    #[test]
    fn growth_after_admission_still_obeys_the_shared_byte_limit() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recipe");
        fs::write(
            &path,
            crate::test_recipes::silent(0.01, 0.0)
                .to_toml()
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        let file = open(&path).unwrap_or_else(|_| panic!("regular fixture not admitted"));
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&vec![b' '; 65_537])
            .unwrap();
        let error = crate::recipe::load_reader(file, "grown recipe").unwrap_err();
        assert_eq!(error.kind(), crate::error::ErrorKind::InvalidRecipe);
    }

    #[test]
    fn admission_uses_opened_handle_when_path_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recipe");
        let saved = dir.path().join("original");
        fs::write(&path, b"schema_version=4").unwrap();
        let file = open_handle(&path).unwrap();
        fs::rename(&path, &saved).unwrap();
        assert!(
            Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        assert!(admit(file).is_ok());
        // The same path now opens nonblocking, but its opened FIFO is rejected
        // even if the pathname is replaced with a regular file before admission.
        // Fixture opening is independently nonblocking even if production's
        // flag regresses; the bounded integration child detects that regression.
        use std::os::unix::fs::OpenOptionsExt;
        let fifo = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .unwrap();
        fs::remove_file(&path).unwrap();
        fs::rename(&saved, &path).unwrap();
        assert!(matches!(admit(fifo), Err(Failure::NotRegular)));
        let link = dir.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(open(&link).is_ok());
    }
}
