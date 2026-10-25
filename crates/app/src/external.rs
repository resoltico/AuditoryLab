//! Explicit external-file editing with observed identity/content checks.
//!
//! Cooperating writers use a stable adjacent lock. Other applications need not
//! honor that lock: their race after our last check cannot be excluded. Managed
//! library operations never invoke this writer or alter an external source.
use crate::core::{Recipe, Timestamp};
use crate::error::{Error, ErrorKind};
use crate::library::{Fingerprint, MutationStatus};
use crate::recipe::RecipeSnapshot;
use crate::workflow::Cancel;
use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

/// Process-local observation of an opened regular file and its exact bounded bytes.
#[derive(Debug, Clone)]
pub struct ObservedFile {
    path: PathBuf,
    fingerprint: Fingerprint,
    handle: Arc<same_file::Handle>,
}
impl ObservedFile {
    pub(crate) fn capture(path: &Path, file: File, bytes: &[u8]) -> Result<Self, Error> {
        let path = std::fs::canonicalize(path)
            .map_err(|e| io("cannot resolve opened external file", e))?;
        let handle = same_file::Handle::from_file(file)
            .map_err(|e| io("cannot retain external file identity", e))?;
        // Detect a pathname replacement during initial reading; never attach an
        // old handle's bytes to the newly resolved target's edit capability.
        let resolved = same_file::Handle::from_path(&path)
            .map_err(|e| io("cannot check opened external path", e))?;
        if handle != resolved {
            return Err(conflict());
        }
        Ok(Self {
            path,
            fingerprint: Fingerprint::of(bytes),
            handle: Arc::new(handle),
        })
    }
    /// Canonical external destination shown before Save, not a managed record.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Hash of exact observed plain recipe bytes, independent of portable UUID.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
    fn matches(&self, other: &Self) -> bool {
        self.path == other.path
            && self.fingerprint == other.fingerprint
            && self.handle == other.handle
    }
}
/// Exact candidate retained when publication is confirmed or uncertain.
#[derive(Debug, Clone)]
pub struct ExternalReceipt {
    /// Observed source before editing; needed even for no-op saves.
    pub previous: ObservedFile,
    /// Captured proposed portable content, including its proposed edit time.
    pub recipe: Arc<Recipe>,
    candidate: ObservedFile,
    bytes: Arc<[u8]>,
    /// Owned stage, never automatically promoted during recovery.
    pub owned_stage: Option<PathBuf>,
}
/// Publication truth and warnings, independent of reporting/UI success.
#[derive(Debug, Clone)]
pub struct ExternalOutcome {
    /// Confirmed, unchanged, warning, or explicitly unconfirmed.
    pub status: MutationStatus,
    /// Exact captured recipe/versions for save baseline or explicit recheck.
    pub receipt: ExternalReceipt,
    /// Durability/chronology/cleanup context.
    pub diagnostics: Vec<String>,
}
impl ExternalOutcome {
    /// Only confirmed outcomes may advance a saved baseline.
    #[must_use]
    pub const fn confirmed(&self) -> bool {
        !matches!(self.status, MutationStatus::PublicationUnconfirmed)
    }
    /// Capture this confirmed publication as a new external snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Option<RecipeSnapshot> {
        self.confirmed().then(|| {
            RecipeSnapshot::new(
                crate::recipe::Origin::File(self.receipt.candidate.path.clone()),
                (*self.receipt.recipe).clone(),
            )
            .with_external(self.receipt.candidate.clone())
        })
    }
}
/// Explicit reread of a retained external-file receipt.
#[derive(Debug, Clone)]
pub enum ExternalRecheck {
    /// Exact candidate file identity and bytes observed.
    Published(Box<ExternalOutcome>),
    /// Original identity and bytes still present; explicit retry is possible.
    Original,
    /// Target absent; absence does not establish attribution or authorize recreation.
    Absent,
    /// Another identity/content exists; draft and receipt must be reconciled.
    Changed,
}
/// Stateless writer configuration; fault controls are excluded from product builds.
#[derive(Debug, Clone, Default)]
pub struct ExternalWriter {
    #[cfg(any(test, feature = "test-support"))]
    fault: Option<crate::library::StoreFault>,
}
impl ExternalWriter {
    /// Select a controlled fault around real filesystem operations in native tests.
    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub const fn with_fault(mut self, fault: crate::library::StoreFault) -> Self {
        self.fault = Some(fault);
        self
    }
    /// Save only the captured file version, including for a semantic no-op.
    ///
    /// # Errors
    /// Returns pre-publication failures/conflicts without advancing any baseline.
    /// An uncertain publication is a retained outcome, never an ordinary failure.
    pub fn save(
        &self,
        expected: &ObservedFile,
        recipe: Arc<Recipe>,
        cancel: &Cancel,
    ) -> Result<ExternalOutcome, Error> {
        let (dir, name, _lock) = lease(expected)?;
        let (current, bytes) = observe(&dir, &name, &expected.path)?;
        if !expected.matches(&current) {
            return Err(conflict());
        }
        let original = Recipe::from_bytes(&bytes)
            .map_err(|e| Error::new(ErrorKind::InvalidRecipe, e.to_string()))?;
        if original.document.id != recipe.document.id
            || original.document.created_at != recipe.document.created_at
        {
            return Err(Error::new(
                ErrorKind::HistoryConflict,
                "external Save must retain observed identity and creation history; save an independent copy instead",
            ));
        }
        if original.same_editable_content(&recipe) {
            return Ok(ExternalOutcome {
                status: MutationStatus::Unchanged,
                receipt: ExternalReceipt {
                    previous: expected.clone(),
                    recipe: Arc::new(original),
                    candidate: current,
                    bytes: Arc::from(bytes),
                    owned_stage: None,
                },
                diagnostics: Vec::new(),
            });
        }
        let mut recipe = (*recipe).clone();
        let elapsed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| Error::new(ErrorKind::Write, e.to_string()))?;
        let seconds = i64::try_from(elapsed.as_secs())
            .map_err(|e| Error::new(ErrorKind::Write, e.to_string()))?;
        let now = Timestamp::from_unix(seconds, elapsed.subsec_nanos()).ok_or_else(|| {
            Error::new(
                ErrorKind::Write,
                "observed clock is outside supported history",
            )
        })?;
        recipe.document.modified_at = Some(now);
        let mut diagnostics = Vec::new();
        if original.document.modified_at.is_some_and(|old| now < old) {
            diagnostics.push("Observed clock predates the prior declared edit time; no artificial timestamp was fabricated.".into());
        }
        let bytes = recipe
            .to_toml()
            .map_err(|e| Error::new(ErrorKind::InvalidRecipe, e.to_string()))?
            .into_bytes();
        if cancel.is_requested() {
            return Err(cancelled());
        }
        let stage = format!(".auditorylab-{}.partial", crate::identity::random_uuid()?);
        let mut file = dir
            .open_with(&stage, &create_options(true))
            .map_err(|e| io("cannot create external save stage", e))?;
        let prepared = (|| {
            #[cfg(any(test, feature = "test-support"))]
            if self.fault == Some(crate::library::StoreFault::Write) {
                file.write_all(&bytes[..bytes.len() / 2])
                    .map_err(|e| io("partial stage write", e))?;
                return Err(Error::new(
                    ErrorKind::Write,
                    "injected partial external stage write",
                ));
            }
            file.write_all(&bytes)
                .map_err(|e| io("cannot write external stage", e))?;
            file.flush()
                .map_err(|e| io("cannot flush external stage", e))?;
            file.sync_all()
                .map_err(|e| io("cannot sync external stage", e))?;
            let (candidate, observed) = observe(&dir, &stage, &expected.path)?;
            if observed != bytes {
                return Err(Error::new(
                    ErrorKind::Write,
                    "external stage verification failed",
                ));
            }
            let (latest, _) = observe(&dir, &name, &expected.path)?;
            if !expected.matches(&latest) {
                return Err(conflict());
            }
            #[cfg(any(test, feature = "test-support"))]
            if self.fault == Some(crate::library::StoreFault::CancelBeforePublication) {
                cancel.request();
            }
            if cancel.is_requested() {
                return Err(cancelled());
            }
            Ok(candidate)
        })();
        let candidate = match prepared {
            Ok(candidate) => candidate,
            Err(error) => return Err(cleanup(&dir, &stage, &expected.path, error)),
        };
        let mut receipt = ExternalReceipt {
            previous: expected.clone(),
            recipe: Arc::new(recipe),
            candidate,
            bytes: Arc::from(bytes),
            owned_stage: Some(
                expected
                    .path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(&stage),
            ),
        };
        #[cfg(any(test, feature = "test-support"))]
        let omit = matches!(
            self.fault,
            Some(crate::library::StoreFault::PublicationUncommittedUnreadable)
        );
        #[cfg(not(any(test, feature = "test-support")))]
        let omit = false;
        let publication = if omit {
            Err(std::io::Error::other(
                "injected unavailable external publication result",
            ))
        } else {
            dir.rename(&stage, &dir, &name)
        };
        #[cfg(any(test, feature = "test-support"))]
        let uncertain = matches!(
            self.fault,
            Some(
                crate::library::StoreFault::PublicationCommittedUnreadable
                    | crate::library::StoreFault::PublicationUncommittedUnreadable
            )
        );
        #[cfg(not(any(test, feature = "test-support")))]
        let uncertain = false;
        if uncertain {
            return Ok(ExternalOutcome { status:MutationStatus::PublicationUnconfirmed,receipt,diagnostics:vec!["Publication result and immediate reread unavailable; retain receipt and Recheck before retrying.".into()] });
        }
        if let Err(error) = publication {
            match observe(&dir, &name, &expected.path) {
                Ok((found, bytes))
                    if receipt.candidate.matches(&found)
                        && bytes.as_slice() == receipt.bytes.as_ref() =>
                {
                    diagnostics.push(format!(
                        "Publication confirmed by exact identity and bytes after OS error: {error}"
                    ))
                }
                Ok((found, _)) if expected.matches(&found) => {
                    return Err(cleanup(
                        &dir,
                        &stage,
                        &expected.path,
                        io("external publication failed; original observed", error),
                    ));
                }
                _ => {
                    return Ok(ExternalOutcome {
                        status: MutationStatus::PublicationUnconfirmed,
                        receipt,
                        diagnostics: vec![format!("Publication could not be confirmed: {error}")],
                    });
                }
            }
        }
        receipt.owned_stage = None;
        #[cfg(any(test, feature = "test-support"))]
        let sync = if self.fault == Some(crate::library::StoreFault::DirectorySync) {
            Err(std::io::Error::other(
                "injected external directory synchronization failure",
            ))
        } else {
            dir.try_clone().and_then(|d| d.into_std_file().sync_all())
        };
        #[cfg(not(any(test, feature = "test-support")))]
        let sync = dir.try_clone().and_then(|d| d.into_std_file().sync_all());
        if let Err(error) = sync {
            diagnostics.push(format!(
                "Saved; external directory durability warning: {error}"
            ));
        }
        Ok(ExternalOutcome {
            status: if diagnostics.is_empty() {
                MutationStatus::Published
            } else {
                MutationStatus::PublishedWithWarning
            },
            receipt,
            diagnostics,
        })
    }
    /// Recheck under the same stable cooperative lock without retrying publication.
    ///
    /// # Errors
    /// Reports Busy, unsafe paths and read failures without discarding the receipt.
    pub fn recheck(&self, receipt: &ExternalReceipt) -> Result<ExternalRecheck, Error> {
        let (dir, name, _lock) = lease(&receipt.previous)?;
        if let Err(error) = dir.symlink_metadata(&name)
            && error.kind() == std::io::ErrorKind::NotFound
        {
            return Ok(ExternalRecheck::Absent);
        }
        let (observed, bytes) = observe(&dir, &name, &receipt.previous.path)?;
        if receipt.candidate.matches(&observed) && bytes.as_slice() == receipt.bytes.as_ref() {
            Ok(ExternalRecheck::Published(Box::new(ExternalOutcome { status:MutationStatus::PublishedWithWarning,receipt:receipt.clone(),diagnostics:vec!["Exact candidate observed on Recheck; prior directory durability is not established.".into()] })))
        } else if receipt.previous.matches(&observed) {
            Ok(ExternalRecheck::Original)
        } else {
            Ok(ExternalRecheck::Changed)
        }
    }
}
fn io(context: &str, error: std::io::Error) -> Error {
    Error::caused(ErrorKind::Write, format!("{context}: {error}"), error)
}
fn conflict() -> Error {
    Error::new(
        ErrorKind::Conflict,
        "Conflict: external file changed identity or bytes since opening; draft retained. Reload or save an independent copy.",
    )
}
fn cancelled() -> Error {
    Error::new(
        ErrorKind::Cancelled,
        "external save cancelled before publication",
    )
}
fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    options
}
fn create_options(exclusive: bool) -> OpenOptions {
    let mut options = options();
    options.write(true);
    if exclusive {
        options.create_new(true);
    } else {
        options.create(true);
    }
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}
#[derive(Debug)]
struct ExternalLock(File);
impl Drop for ExternalLock {
    fn drop(&mut self) {
        // Close alone can leave a flock alive in a concurrently spawned child
        // sharing this open description. Release explicitly before closing.
        let _ = self.0.unlock();
    }
}
fn lease(expected: &ObservedFile) -> Result<(Dir, std::ffi::OsString, ExternalLock), Error> {
    let parent = expected
        .path
        .parent()
        .ok_or_else(|| Error::new(ErrorKind::Write, "external destination has no parent"))?;
    let name = expected
        .path
        .file_name()
        .ok_or_else(|| Error::new(ErrorKind::Write, "external destination has no filename"))?
        .to_owned();
    let dir = Dir::open_ambient_dir(parent, cap_std::ambient_authority())
        .map_err(|e| io("cannot open external parent", e))?;
    let lockname = format!(
        ".auditorylab-{}.lock",
        Fingerprint::of(name.as_encoded_bytes())
    );
    let file = dir
        .open_with(lockname, &create_options(false))
        .map_err(|e| io("cannot open external cooperative lock", e))?;
    if !file
        .metadata()
        .map_err(|e| io("cannot inspect external lock", e))?
        .is_file()
    {
        return Err(Error::new(
            ErrorKind::LockUnavailable,
            "external lock is not a regular file",
        ));
    }
    let file = file.into_std();
    file.try_lock().map_err(|e| {
        Error::new(
            if matches!(e, std::fs::TryLockError::WouldBlock) {
                ErrorKind::Busy
            } else {
                ErrorKind::LockUnavailable
            },
            format!("cannot acquire external cooperative lock: {e}"),
        )
    })?;
    Ok((dir, name, ExternalLock(file)))
}
fn observe(
    dir: &Dir,
    name: impl AsRef<Path>,
    path: &Path,
) -> Result<(ObservedFile, Vec<u8>), Error> {
    let mut file = dir
        .open_with(name, &options())
        .map_err(|e| io("cannot reread external file", e))?;
    if !file
        .metadata()
        .map_err(|e| io("cannot inspect external file", e))?
        .is_file()
    {
        return Err(Error::new(
            ErrorKind::Unreadable,
            "external target must remain a regular file",
        ));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take((Recipe::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| io("cannot read external bytes", e))?;
    if bytes.len() > Recipe::MAX_BYTES {
        return Err(Error::new(
            ErrorKind::InvalidRecipe,
            "external file exceeds the bounded recipe size",
        ));
    }
    let handle = same_file::Handle::from_file(file.into_std())
        .map_err(|e| io("cannot compare external file identity", e))?;
    Ok((
        ObservedFile {
            path: path.to_owned(),
            fingerprint: Fingerprint::of(&bytes),
            handle: Arc::new(handle),
        },
        bytes,
    ))
}
fn cleanup(dir: &Dir, stage: &str, path: &Path, error: Error) -> Error {
    match dir.remove_file(stage) {
        Ok(()) => error,
        Err(cleanup) => {
            error.with_leftover(path.parent().unwrap_or(Path::new(".")).join(stage), cleanup)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::Draft;
    use crate::library::StoreFault;
    fn fixture() -> (tempfile::TempDir, ObservedFile, Recipe) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("external.toml");
        let recipe = Draft::new().unwrap().recipe();
        std::fs::write(&path, recipe.to_toml().unwrap()).unwrap();
        let snapshot = crate::recipe::load_file(&path).unwrap();
        (temp, snapshot.external_file().unwrap().clone(), recipe)
    }
    fn changed(recipe: &Recipe) -> Arc<Recipe> {
        let mut recipe = recipe.clone();
        recipe.description.as_mut().unwrap().title = "edited external".into();
        Arc::new(recipe)
    }
    #[test]
    fn external_noop_retains_exact_bytes_and_unknown_dates() {
        let (_temp, expected, recipe) = fixture();
        let before = std::fs::read(expected.path()).unwrap();
        let outcome = ExternalWriter::default()
            .save(&expected, Arc::new(recipe), &Cancel::default())
            .unwrap();
        assert_eq!(outcome.status, MutationStatus::Unchanged);
        assert_eq!(std::fs::read(expected.path()).unwrap(), before);
        assert!(outcome.receipt.recipe.document.created_at.is_none());
        assert!(outcome.receipt.recipe.document.modified_at.is_none());
    }
    #[test]
    fn external_save_advances_only_modified_and_rejects_stale_even_when_noop() {
        let (_temp, expected, recipe) = fixture();
        let result = ExternalWriter::default()
            .save(&expected, changed(&recipe), &Cancel::default())
            .unwrap();
        assert!(result.confirmed());
        assert!(result.receipt.recipe.document.created_at.is_none());
        assert!(result.receipt.recipe.document.modified_at.is_some());
        let error = ExternalWriter::default()
            .save(&expected, Arc::new(recipe), &Cancel::default())
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Conflict);
        assert_eq!(
            crate::recipe::load_file(expected.path()).unwrap().title(),
            "edited external"
        );
    }
    #[test]
    fn identical_bytes_in_a_replacement_inode_are_a_conflict() {
        let (_temp, expected, recipe) = fixture();
        let bytes = std::fs::read(expected.path()).unwrap();
        let replacement = expected.path().with_extension("replacement");
        std::fs::write(&replacement, &bytes).unwrap();
        std::fs::rename(replacement, expected.path()).unwrap();
        let error = ExternalWriter::default()
            .save(&expected, Arc::new(recipe), &Cancel::default())
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Conflict);
        assert_eq!(std::fs::read(expected.path()).unwrap(), bytes);
    }
    #[test]
    fn actual_prepublication_faults_keep_source_and_remove_owned_stage() {
        for fault in [StoreFault::Write, StoreFault::CancelBeforePublication] {
            let (temp, expected, recipe) = fixture();
            let before = std::fs::read(expected.path()).unwrap();
            assert!(
                ExternalWriter::default()
                    .with_fault(fault)
                    .save(&expected, changed(&recipe), &Cancel::default())
                    .is_err()
            );
            assert_eq!(std::fs::read(expected.path()).unwrap(), before);
            assert!(std::fs::read_dir(temp.path()).unwrap().all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".partial")
            }));
        }
    }
    #[test]
    fn actual_publication_with_sync_warning_is_saved() {
        let (_temp, expected, recipe) = fixture();
        let result = ExternalWriter::default()
            .with_fault(StoreFault::DirectorySync)
            .save(&expected, changed(&recipe), &Cancel::default())
            .unwrap();
        assert_eq!(result.status, MutationStatus::PublishedWithWarning);
        assert_eq!(
            crate::recipe::load_file(expected.path()).unwrap().recipe(),
            result.receipt.recipe.as_ref()
        );
        assert!(result.snapshot().is_some());
    }
    #[test]
    fn uncertain_states_recheck_actual_identity_and_bytes_without_retry() {
        for committed in [false, true] {
            let (_temp, expected, recipe) = fixture();
            let fault = if committed {
                StoreFault::PublicationCommittedUnreadable
            } else {
                StoreFault::PublicationUncommittedUnreadable
            };
            let outcome = ExternalWriter::default()
                .with_fault(fault)
                .save(&expected, changed(&recipe), &Cancel::default())
                .unwrap();
            assert!(!outcome.confirmed());
            assert!(outcome.snapshot().is_none());
            let rechecked = ExternalWriter::default().recheck(&outcome.receipt).unwrap();
            assert!(matches!(rechecked, ExternalRecheck::Published(_)) == committed);
            assert!(matches!(rechecked, ExternalRecheck::Original) != committed);
        }
    }
    #[cfg(unix)]
    #[test]
    fn replaced_symlink_and_fifo_are_refused_without_following_or_blocking() {
        use std::os::unix::fs::symlink;
        for fifo in [false, true] {
            let (temp, expected, recipe) = fixture();
            let original = std::fs::read(expected.path()).unwrap();
            let other = temp.path().join("other");
            std::fs::write(&other, &original).unwrap();
            std::fs::remove_file(expected.path()).unwrap();
            if fifo {
                assert!(
                    std::process::Command::new("mkfifo")
                        .arg(expected.path())
                        .status()
                        .unwrap()
                        .success()
                );
            } else {
                symlink(&other, expected.path()).unwrap();
            }
            assert!(
                ExternalWriter::default()
                    .save(&expected, changed(&recipe), &Cancel::default())
                    .is_err()
            );
            assert_eq!(std::fs::read(other).unwrap(), original);
        }
    }
    #[cfg(unix)]
    #[test]
    fn releasing_external_lease_unlocks_even_while_a_child_inherits_its_description() {
        use std::process::{Command, Stdio};
        let (_temp, expected, _) = fixture();
        let (_dir, _name, lock) = lease(&expected).unwrap();
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::from(lock.0.try_clone().unwrap()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let held = lease(&expected).err().map(|error| error.kind());
        drop(lock);
        let after = lease(&expected);
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(held, Some(ErrorKind::Busy));
        assert!(
            after.is_ok(),
            "the parent's completed operation must release its lock even if a child inherited the open description: {after:?}"
        );
    }
}
