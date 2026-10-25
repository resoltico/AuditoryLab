//! Root resolution, private initialization and stable cooperative filesystem locks.
use super::{RecordRevision, record};
use crate::{Error, ErrorKind};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const LOCK: &str = ".library.lock";
const MARKER: &str = "library.json";
/// Which configuration selected this root, in precedence order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RootSource {
    /// Explicit absolute command-line path.
    CommandLine,
    /// AUDITORYLAB_LIBRARY_DIR environment override.
    Environment,
    /// Native per-user persistent application data.
    Default,
}
/// Resolved configuration; construction performs no directory/file creation.
#[derive(Debug, Clone)]
pub struct LibraryRoot {
    path: PathBuf,
    source: RootSource,
}
impl LibraryRoot {
    /// Resolve flag, then environment, then native persistent data location.
    ///
    /// # Errors
    /// Rejects relative/empty explicit roots or unavailable native data resolution.
    pub fn resolve(explicit: Option<&Path>) -> Result<Self, Error> {
        let (path, source) = if let Some(path) = explicit {
            (path.to_owned(), RootSource::CommandLine)
        } else if let Some(path) = std::env::var_os("AUDITORYLAB_LIBRARY_DIR") {
            (PathBuf::from(path), RootSource::Environment)
        } else {
            let project=directories::ProjectDirs::from("org","resoltico","auditorylab").ok_or_else(||Error::new(ErrorKind::InvalidArgument,"cannot resolve native per-user application data; configure an absolute --library-dir"))?;
            (
                project.data_local_dir().join("library"),
                RootSource::Default,
            )
        };
        Self::configured(path, source)
    }
    /// Select a caller-provided absolute root with explicit initialization policy.
    ///
    /// # Errors
    /// Rejects paths that are not absolute; never falls back to another directory.
    pub fn explicit(path: PathBuf) -> Result<Self, Error> {
        Self::configured(path, RootSource::CommandLine)
    }
    fn configured(path: PathBuf, source: RootSource) -> Result<Self, Error> {
        if !path.is_absolute() {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "library directory must be an absolute path; no fallback was used",
            ));
        }
        let path = match std::fs::canonicalize(&path) {
            Ok(canonical) => canonical,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => path,
            Err(e) => return Err(io("cannot resolve the configured library root", e)),
        };
        Ok(Self { path, source })
    }
    /// Resolved path, never a cache, bundle or checkout-relative location.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Selected override/default source.
    #[must_use]
    pub const fn source(&self) -> RootSource {
        self.source
    }
    pub(super) fn missing_is_empty(&self) -> bool {
        self.source == RootSource::Default
    }
    /// Inspect existence/marker without creating a lock or initializing a root.
    ///
    /// # Errors
    /// Reports busy, unsafe/incomplete layout or unreadable/unsupported marker.
    pub fn inspect(&self) -> Result<RootInfo, Error> {
        match self.lock(false, false)? {
            None => Ok(RootInfo {
                path: self.path.clone(),
                source: self.source,
                exists: false,
                library_format: None,
                initialized: false,
            }),
            Some(lease) => {
                validate_marker(&lease.dir)?;
                lease.records()?;
                Ok(RootInfo {
                    path: lease.canonical.clone(),
                    source: self.source,
                    exists: true,
                    library_format: Some(record::RECORD_FORMAT),
                    initialized: true,
                })
            }
        }
    }
    /// Initialize an empty/owned interrupted layout, under the persistent root lock.
    ///
    /// # Errors
    /// Refuses unknown existing contents, unsupported markers, nonempty unmarked
    /// records, symlinks, locking failure and pre-publication I/O errors.
    pub fn initialize(&self) -> Result<Initialization, Error> {
        self.initialize_inner(|| {})
    }
    /// Pause after the real marker stage is synchronized, before publication.
    /// Native parent tests terminate this process and inspect/retry the owned layout.
    ///
    /// # Errors
    /// Reports real initialization errors before reaching the controlled pause.
    #[cfg(any(test, feature = "test-support"))]
    pub fn initialize_paused(&self, ready: &Path) -> Result<Initialization, Error> {
        self.initialize_inner(|| {
            std::fs::write(ready, b"ready")
                .unwrap_or_else(|error| panic!("cannot signal initialization fixture: {error}"));
            loop {
                std::thread::park();
            }
        })
    }
    fn initialize_inner(&self, before_marker: impl FnOnce()) -> Result<Initialization, Error> {
        let lease = self.lock(true, true)?.ok_or_else(|| {
            Error::new(
                ErrorKind::Write,
                "library initialization did not create a root",
            )
        })?;
        if lease.dir.symlink_metadata(MARKER).is_ok() {
            validate_marker(&lease.dir)?;
            lease.records()?;
            return Ok(Initialization {
                created: false,
                path: lease.canonical.clone(),
                warnings: Vec::new(),
            });
        }
        validate_unmarked(&lease.dir)?;
        match lease.dir.symlink_metadata("records") {
            Ok(metadata) => {
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(Error::new(
                        ErrorKind::IncompleteInventory,
                        "records must be a real directory",
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut builder = cap_std::fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use cap_std::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                lease
                    .dir
                    .create_dir_with("records", &builder)
                    .map_err(|e| io("cannot create records directory", e))?;
            }
            Err(e) => return Err(io("cannot inspect records directory", e)),
        }
        let stage = format!(".library.{}.partial", RecordRevision::fresh()?);
        let mut file = create_stage(&lease.dir, &stage)?;
        let bytes = serde_json::to_vec(&Marker {
            library_format: record::RECORD_FORMAT,
        })
        .map_err(|e| Error::caused(ErrorKind::Write, "cannot encode library marker", e))?;
        if let Err(e) = file
            .write_all(&bytes)
            .and_then(|()| file.flush())
            .and_then(|()| file.sync_all())
        {
            let cleanup = lease.dir.remove_file(&stage);
            return Err(stage_failure(
                "cannot prepare library marker",
                e,
                &stage,
                cleanup,
            ));
        }
        before_marker();
        let published = lease.dir.hard_link(&stage, &lease.dir, MARKER);
        if let Err(e) = published
            && validate_marker(&lease.dir).is_err()
        {
            let cleanup = lease.dir.remove_file(&stage);
            return Err(stage_failure(
                "library marker publication was not confirmed; inspect the root before retry",
                e,
                &stage,
                cleanup,
            ));
        }
        let mut warnings = Vec::new();
        if let Err(e) = lease.dir.remove_file(&stage) {
            warnings.push(format!(
                "Initialized; marker stage {stage} could not be removed: {e}"
            ));
        }
        if let Err(e) = lease.sync() {
            warnings.push(format!(
                "Initialized; root directory durability warning: {e}"
            ));
        }
        Ok(Initialization {
            created: true,
            path: lease.canonical.clone(),
            warnings,
        })
    }
    pub(super) fn lock(&self, exclusive: bool, initialize: bool) -> Result<Option<Lease>, Error> {
        let canonical = match std::fs::canonicalize(&self.path) {
            Ok(path) => path,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !initialize => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut builder = std::fs::DirBuilder::new();
                builder.recursive(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                builder
                    .create(&self.path)
                    .map_err(|e| io("cannot create configured library root", e))?;
                std::fs::canonicalize(&self.path)
                    .map_err(|e| io("cannot resolve initialized root", e))?
            }
            Err(e) => return Err(io("cannot inspect configured library root", e)),
        };
        let admission = Admission::acquire(canonical.clone())?;
        let dir = Dir::open_ambient_dir(&canonical, cap_std::ambient_authority())
            .map_err(|e| io("cannot open configured library root", e))?;
        if initialize && dir.symlink_metadata(LOCK).is_err() {
            validate_unmarked(&dir)?;
            match create_stage(&dir, LOCK) {
                Ok(_) => {}
                Err(e) if dir.symlink_metadata(LOCK).is_ok() => {
                    drop(e);
                }
                Err(e) => return Err(e),
            }
        }
        let mut options = read_options();
        options.write(exclusive);
        let file=dir.open_with(LOCK,&options).map_err(|e|io("library lock is missing/unreadable; use library init only after inspecting this layout",e))?.into_std();
        if !file
            .metadata()
            .map_err(|e| io("cannot inspect library lock", e))?
            .is_file()
        {
            return Err(Error::new(
                ErrorKind::IncompleteInventory,
                "library lock must be a regular persistent file",
            ));
        }
        let result = if exclusive {
            file.try_lock()
        } else {
            file.try_lock_shared()
        };
        result.map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => Error::new(
                ErrorKind::Busy,
                "Library busy; retry after the other operation finishes",
            ),
            std::fs::TryLockError::Error(e) => Error::caused(
                ErrorKind::LockUnavailable,
                "required library locking assurance is unavailable; no unlocked fallback",
                e,
            ),
        })?;
        Ok(Some(Lease {
            dir,
            canonical,
            file,
            admission,
        }))
    }
}
/// Read-only path/format information.
#[derive(Debug, Clone, Serialize)]
pub struct RootInfo {
    /// Canonical configured path when it exists.
    pub path: PathBuf,
    /// Configuration precedence source.
    pub source: RootSource,
    /// Root exists; false does not trigger initialization.
    pub exists: bool,
    /// Supported observed marker format, otherwise absent.
    pub library_format: Option<u32>,
    /// Marker/lock/records layout was verified.
    pub initialized: bool,
}
/// Explicit initialization result, distinct from creating any recipe.
#[derive(Debug, Clone, Serialize)]
pub struct Initialization {
    /// False when a supported initialized root already existed.
    pub created: bool,
    /// Root actually initialized/observed.
    pub path: PathBuf,
    /// Post-publication durability/cleanup warnings.
    pub warnings: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    library_format: u32,
}
pub(super) fn validate_marker(dir: &Dir) -> Result<(), Error> {
    let bytes = read_bounded(dir, MARKER, 1024)?;
    let marker: Marker = serde_json::from_slice(&bytes).map_err(|_| {
        Error::new(
            ErrorKind::IncompleteInventory,
            "missing/invalid library format marker; existing records were left intact",
        )
    })?;
    if marker.library_format != record::RECORD_FORMAT {
        return Err(Error::new(
            ErrorKind::IncompleteInventory,
            format!(
                "unsupported library format {}; this build reads {}",
                marker.library_format,
                record::RECORD_FORMAT
            ),
        ));
    }
    Ok(())
}
fn validate_unmarked(dir: &Dir) -> Result<(), Error> {
    for entry in dir
        .entries()
        .map_err(|e| io("cannot enumerate uninitialized root", e))?
    {
        let entry = entry.map_err(|e| io("cannot inspect uninitialized root entry", e))?;
        let name = entry.file_name();
        if name == LOCK {
            continue;
        }
        if name == "records" {
            let records = dir
                .open_dir_nofollow("records")
                .map_err(|e| io("unsafe records directory", e))?;
            if records
                .entries()
                .map_err(|e| io("cannot inspect unmarked records", e))?
                .next()
                .transpose()
                .map_err(|e| io("cannot inspect unmarked record", e))?
                .is_some()
            {
                return Err(Error::new(
                    ErrorKind::IncompleteInventory,
                    "records exist without a supported marker; do not initialize or convert them automatically",
                ));
            }
            continue;
        }
        if name == MARKER {
            validate_marker(dir)?;
            continue;
        }
        if name
            .to_str()
            .is_some_and(|s| s.starts_with(".library.") && s.ends_with(".partial"))
        {
            let bytes = read_bounded(dir, &name, 1024)?;
            let marker: Marker = serde_json::from_slice(&bytes).map_err(|_| {
                Error::new(
                    ErrorKind::IncompleteInventory,
                    "unknown contents in an uninitialized root",
                )
            })?;
            if marker.library_format == record::RECORD_FORMAT {
                continue;
            }
        }
        return Err(Error::new(
            ErrorKind::IncompleteInventory,
            "unknown existing root contents; initialization refused without changing them",
        ));
    }
    Ok(())
}
fn read_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    options
}
pub(super) fn create_stage(dir: &Dir, name: &str) -> Result<cap_std::fs::File, Error> {
    let mut options = read_options();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    dir.open_with(name, &options)
        .map_err(|e| io("cannot create an owned exclusive stage", e))
}
pub(super) fn read_bounded(
    dir: &Dir,
    name: impl AsRef<Path>,
    limit: usize,
) -> Result<Vec<u8>, Error> {
    let file = dir.open_with(name, &read_options()).map_err(|e| {
        io(
            "cannot open regular managed file without following symlinks",
            e,
        )
    })?;
    let metadata = file
        .metadata()
        .map_err(|e| io("cannot inspect opened managed file", e))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Error::new(
            ErrorKind::IncompleteInventory,
            "managed entry must be a regular non-symlink file",
        ));
    }
    if metadata.len() > u64::try_from(limit).unwrap_or(u64::MAX) {
        return Err(Error::new(
            ErrorKind::IncompleteInventory,
            "managed file exceeds its byte limit",
        ));
    }
    let mut bytes = Vec::new();
    file.take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| io("cannot read managed file", e))?;
    if bytes.len() > limit {
        return Err(Error::new(
            ErrorKind::IncompleteInventory,
            "managed file grew beyond its bounded read limit",
        ));
    }
    Ok(bytes)
}
pub(super) struct Lease {
    pub(super) dir: Dir,
    pub(super) canonical: PathBuf,
    file: File,
    admission: Admission,
}
impl Lease {
    pub(super) fn records(&self) -> Result<Dir, Error> {
        self.dir
            .open_dir_nofollow("records")
            .map_err(|e| io("records directory is missing, unreadable or a symlink", e))
    }
    pub(super) fn sync(&self) -> std::io::Result<()> {
        self.dir.try_clone()?.into_std_file().sync_all()
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        self.admission.release();
    }
}
static ACTIVE: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
struct Admission {
    key: Option<PathBuf>,
}
impl Admission {
    fn acquire(key: PathBuf) -> Result<Self, Error> {
        // This mutex protects only a short in-memory set operation, never I/O.
        // try_lock here would spuriously report Busy for a different root.
        // Same-root admission below and the OS locks remain nonblocking.
        let mut active = ACTIVE
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !active.insert(key.clone()) {
            return Err(Error::new(
                ErrorKind::Busy,
                "Library root already has an operation in this process; retry",
            ));
        }
        Ok(Self { key: Some(key) })
    }
    fn release(&mut self) {
        if let Some(key) = self.key.take() {
            ACTIVE
                .get_or_init(|| Mutex::new(HashSet::new()))
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&key);
        }
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        self.release();
    }
}
pub(super) fn io(context: &str, error: std::io::Error) -> Error {
    Error::caused(ErrorKind::Unreadable, format!("{context}: {error}"), error)
}
fn stage_failure(
    context: &str,
    error: std::io::Error,
    stage: &str,
    cleanup: std::io::Result<()>,
) -> Error {
    let mut message = format!("{context}: {error}");
    if let Err(e) = cleanup {
        message.push_str(&format!("; owned stage {stage} retained: {e}"));
    }
    Error::caused(ErrorKind::Write, message, error)
}
