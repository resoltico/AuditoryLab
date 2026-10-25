//! Root resolution, private initialization and stable cooperative filesystem locks.
use crate::error::{Error, ErrorKind};
use crate::publish::{self, Policy};
use crate::workflow::Cancel;
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::File,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

const LOCK: &str = ".library.lock";
const MARKER: &str = "library.toml";
/// Version of the plain-file library directory layout.
pub const LIBRARY_LAYOUT: u32 = 1;
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
    /// Inspect the marker and required directories without creating a root.
    ///
    /// # Errors
    /// Reports busy locks, unsupported markers and unsafe/incomplete directories.
    pub fn inspect(&self) -> Result<RootInfo, Error> {
        let Some(lease) = self.lock(false, false)? else {
            return Ok(RootInfo {
                path: self.path.clone(),
                source: self.source,
                exists: false,
                layout: None,
                initialized: false,
            });
        };
        validate_marker(&lease.dir)?;
        for name in ["recipes", "trash", ".stage"] {
            lease.directory(name)?;
        }
        Ok(RootInfo {
            path: lease.canonical.clone(),
            source: self.source,
            exists: true,
            layout: Some(LIBRARY_LAYOUT),
            initialized: true,
        })
    }
    /// Initialize an empty or interrupted owned layout; publish its marker last.
    ///
    /// # Errors
    /// Refuses unknown existing content, symlinks, unsupported layouts and lock failures.
    pub fn initialize(&self) -> Result<Initialization, Error> {
        let lease = self
            .lock(true, true)?
            .ok_or_else(|| Error::new(ErrorKind::Write, "initialization did not create a root"))?;
        if lease.dir.symlink_metadata(MARKER).is_ok() {
            validate_marker(&lease.dir)?;
            for name in ["recipes", "trash", ".stage"] {
                lease.directory(name)?;
            }
            return Ok(Initialization {
                created: false,
                path: lease.canonical.clone(),
                warnings: Vec::new(),
            });
        }
        validate_unmarked(&lease.dir)?;
        for name in ["recipes", "trash", ".stage"] {
            match lease.dir.symlink_metadata(name) {
                Ok(_) => {
                    lease.directory(name)?;
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
                        .create_dir_with(name, &builder)
                        .map_err(|e| io("cannot create private library directory", e))?;
                }
                Err(e) => return Err(io("cannot inspect library directory", e)),
            }
        }
        let bytes = format!("layout = {LIBRARY_LAYOUT}\n");
        let mut warnings = Vec::new();
        if lease.dir.symlink_metadata("membership.toml").is_err() {
            let initialized = publish::publish_in(
                &lease.dir,
                Path::new("membership.toml"),
                &lease.canonical.join("membership.toml"),
                Policy::CreateNew,
                &Cancel::default(),
                |file| {
                    use std::io::Write;
                    file.write_all(b"[entries]\n")
                        .map_err(|e| io("cannot initialize membership timestamps", e))
                },
            )?;
            warnings.extend(initialized.warnings);
        }
        cleanup_stages(&lease.dir, &lease.canonical, &mut warnings)?;
        let result = publish::publish_in(
            &lease.dir,
            Path::new(MARKER),
            &lease.canonical.join(MARKER),
            Policy::CreateNew,
            &Cancel::default(),
            |file| {
                use std::io::Write;
                file.write_all(bytes.as_bytes())
                    .map_err(|e| io("cannot write library marker", e))
            },
        )?;
        warnings.extend(result.warnings);
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
            match create_lock(&dir, LOCK) {
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
    pub layout: Option<u32>,
    /// Marker/lock/plain-file layout was verified.
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    layout: u32,
}
pub(super) fn validate_marker(dir: &Dir) -> Result<(), Error> {
    let bytes = read_bounded(dir, MARKER, 1024)?;
    let marker: Marker = toml::from_str(std::str::from_utf8(&bytes).map_err(|e| {
        Error::caused(
            ErrorKind::IncompleteInventory,
            "layout marker is not UTF-8",
            e,
        )
    })?)
    .map_err(|e| {
        Error::caused(
            ErrorKind::IncompleteInventory,
            "invalid library.toml layout marker",
            e,
        )
    })?;
    if marker.layout != LIBRARY_LAYOUT {
        return Err(Error::new(
            ErrorKind::IncompleteInventory,
            format!(
                "unsupported library layout {}; this build reads {LIBRARY_LAYOUT}",
                marker.layout
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
        let entry = entry.map_err(|e| io("cannot inspect root entry", e))?;
        let name = entry.file_name();
        if name == LOCK {
            continue;
        }
        if name == MARKER {
            validate_marker(dir)?;
            continue;
        }
        if name == "membership.toml" {
            let bytes = read_bounded(dir, &name, 1024)?;
            if bytes == b"[entries]\n" {
                continue;
            }
            return Err(Error::new(
                ErrorKind::IncompleteInventory,
                "nonempty membership exists without a layout marker; initialization left it intact",
            ));
        }
        if matches!(name.to_str(), Some("recipes" | "trash" | ".stage")) {
            let child = dir
                .open_dir_nofollow(&name)
                .map_err(|e| io("library directory is unsafe", e))?;
            for entry in child
                .entries()
                .map_err(|e| io("cannot enumerate unmarked directory", e))?
            {
                let entry = entry.map_err(|e| io("cannot inspect unmarked entry", e))?;
                if !owned_stage(&entry.file_name()) {
                    return Err(Error::new(
                        ErrorKind::IncompleteInventory,
                        "recipes exist without a supported marker; initialization left them unchanged",
                    ));
                }
            }
            continue;
        }
        if owned_stage(&name) {
            continue;
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
fn create_lock(dir: &Dir, name: &str) -> Result<cap_std::fs::File, Error> {
    let mut options = read_options();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    dir.open_with(name, &options)
        .map_err(|e| io("cannot exclusively create library lock", e))
}
pub(super) fn read_bounded(
    dir: &Dir,
    name: impl AsRef<Path>,
    limit: usize,
) -> Result<Vec<u8>, Error> {
    publish::read_bounded(dir, name.as_ref(), limit)
}
pub(super) fn owned_stage(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .and_then(|n| n.strip_prefix(".auditorylab-"))
        .and_then(|n| n.strip_suffix(".partial"))
        .is_some_and(|n| n.parse::<crate::core::DocumentUuid>().is_ok())
}
pub(super) fn cleanup_stages(
    dir: &Dir,
    path: &Path,
    warnings: &mut Vec<String>,
) -> Result<(), Error> {
    for entry in dir
        .entries()
        .map_err(|e| io("cannot enumerate owned stages", e))?
    {
        let entry = entry.map_err(|e| io("cannot inspect owned stage", e))?;
        let name = entry.file_name();
        if owned_stage(&name)
            && entry
                .file_type()
                .map_err(|e| io("cannot inspect stage type", e))?
                .is_file()
        {
            dir.remove_file(&name)
                .map_err(|e| io("cannot clean abandoned owned stage", e))?;
            warnings.push(format!(
                "Removed abandoned stage {}",
                path.join(name).display()
            ));
        }
    }
    Ok(())
}
pub(super) struct Lease {
    pub(super) dir: Dir,
    pub(super) canonical: PathBuf,
    file: File,
    admission: Admission,
}
impl Lease {
    pub(super) fn directory(&self, name: &str) -> Result<Dir, Error> {
        self.dir
            .open_dir_nofollow(name)
            .map_err(|e| io("library directory is missing, unreadable or a symlink", e))
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
