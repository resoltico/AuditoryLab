//! Free-space admission before a large staged write.
use super::error::FileError;
use std::path::Path;

/// Refuse a write of `needed` bytes that the staged file's volume cannot hold,
/// before anything is written. Platforms without a safe volume query admit it.
///
/// # Errors
/// [`FileError::InsufficientSpace`] when fewer bytes are available than needed.
pub(crate) fn admit(file: &std::fs::File, path: &Path, needed: u64) -> Result<(), FileError> {
    match available(file) {
        Some(available) if available < needed => Err(FileError::InsufficientSpace {
            path: path.to_owned(),
            needed,
            available,
        }),
        _ => Ok(()),
    }
}

#[cfg(unix)]
fn available(file: &std::fs::File) -> Option<u64> {
    let stats = rustix::fs::fstatvfs(file).ok()?;
    Some(stats.f_bavail.saturating_mul(stats.f_frsize))
}

#[cfg(not(unix))]
fn available(_: &std::fs::File) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_larger_than_the_volume_is_refused_and_a_small_one_admitted() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
        std::fs::create_dir_all(&base).unwrap();
        let directory = tempfile::tempdir_in(base).unwrap();
        let path = directory.path().join("probe.wav");
        let file = std::fs::File::create(&path).unwrap();
        admit(&file, &path, 1).unwrap();
        if cfg!(unix) {
            let error = admit(&file, &path, u64::MAX).unwrap_err();
            assert!(
                matches!(error, FileError::InsufficientSpace { needed: u64::MAX, available, .. }
                    if available < u64::MAX),
                "{error:?}"
            );
        }
    }
}
