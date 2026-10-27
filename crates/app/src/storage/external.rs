//! External Save with observed identity and exact-byte checks immediately before publication.
use crate::core::Recipe;
use crate::document::recipe::RecipeSnapshot;
use crate::error::{FileError, IoFailure, IoOperation, SaveError as Error};
use crate::services::Cancel;
use crate::storage::library::{Fingerprint, MutationStatus};
use crate::storage::publish::{self, Policy};
use cap_std::fs::Dir;
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};
/// Exact opened-file observation retained by an editable external snapshot.
#[derive(Debug, Clone)]
pub struct ObservedFile {
    path: PathBuf,
    fingerprint: Fingerprint,
    handle: Arc<same_file::Handle>,
}
impl ObservedFile {
    pub(crate) fn capture(path: &Path, file: File, bytes: &[u8]) -> Result<Self, FileError> {
        let path =
            std::fs::canonicalize(path).map_err(|e| io(path, IoOperation::ResolvePath, e))?;
        let handle = same_file::Handle::from_file(file)
            .map_err(|e| io(&path, IoOperation::RetainIdentity, e))?;
        let current = same_file::Handle::from_path(&path)
            .map_err(|e| io(&path, IoOperation::CheckIdentity, e))?;
        if handle != current {
            return Err(FileError::Changed { path });
        }
        Ok(Self {
            path,
            fingerprint: Fingerprint::of(bytes),
            handle: Arc::new(handle),
        })
    }
    /// Canonical file path displayed before Save.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Hash of exact portable bytes observed while opening.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }
    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.path == other.path
            && self.fingerprint == other.fingerprint
            && self.handle == other.handle
    }
}
/// Confirmed external Save result; no retained recovery state is needed.
#[derive(Debug, Clone)]
pub struct ExternalOutcome {
    /// Unchanged, saved, or saved with warnings.
    pub status: MutationStatus,
    /// Observation on which the operation was based.
    pub previous: ObservedFile,
    /// Exact confirmed recipe, including its declared edit time.
    pub recipe: Arc<Recipe>,
    observed: ObservedFile,
    /// Durability and chronology context.
    pub diagnostics: Vec<String>,
}
impl ExternalOutcome {
    /// Capture the confirmed saved baseline without rereading a later file version.
    #[must_use]
    pub fn snapshot(&self) -> RecipeSnapshot {
        RecipeSnapshot::new(
            crate::document::recipe::Origin::File(self.observed.path.clone()),
            (*self.recipe).clone(),
        )
        .with_external(self.observed.clone())
    }
}
/// Stateless external writer using the shared publication primitive.
#[derive(Debug, Clone, Default)]
pub(crate) struct ExternalWriter;
impl ExternalWriter {
    /// Save a captured recipe only while the opened file's identity and bytes still match.
    ///
    /// # Errors
    /// Rejects stale observations, history changes and invalid content; ambiguous I/O is verified before returning.
    pub(crate) fn save(
        &self,
        expected: &ObservedFile,
        recipe: Arc<Recipe>,
        cancel: &Cancel,
    ) -> Result<ExternalOutcome, Error> {
        if cancel.is_requested() {
            return Err(Error::File {
                error: FileError::Cancelled {
                    path: Some(expected.path.clone()),
                },
            });
        }
        let parent = expected.path.parent().ok_or(Error::File {
            error: FileError::MissingParent,
        })?;
        let name = expected.path.file_name().ok_or(Error::File {
            error: FileError::MissingFilename,
        })?;
        let dir = Dir::open_ambient_dir(parent, cap_std::ambient_authority())
            .map_err(|e| io(&expected.path, IoOperation::OpenDirectory, e))?;
        let mut file = publish::open_regular(&dir, Path::new(name))?;
        let mut bytes = Vec::new();
        use std::io::Read;
        (&mut file)
            .take((Recipe::MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| io(&expected.path, IoOperation::Read, e))?;
        if bytes.len() > Recipe::MAX_BYTES {
            return Err(Error::Recipe {
                error: crate::core::RecipeError::TooLarge,
            });
        }
        let current = ObservedFile::capture(&expected.path, file, &bytes)?;
        if !expected.matches(&current) {
            return Err(FileError::Changed {
                path: expected.path.clone(),
            }
            .into());
        }
        let original = Recipe::from_bytes(&bytes).map_err(|e| Error::Recipe { error: e })?;
        if original.document().id != recipe.document().id
            || original.document().created_at != recipe.document().created_at
        {
            return Err(Error::HistoryReplacement);
        }
        if original.content() == recipe.content() {
            return Ok(ExternalOutcome {
                status: MutationStatus::Unchanged,
                previous: expected.clone(),
                recipe: Arc::new(original),
                observed: current,
                diagnostics: Vec::new(),
            });
        }
        let mut document = recipe.document().clone();
        let now = crate::clock::observed_now()?;
        let mut diagnostics = Vec::new();
        if original
            .document()
            .modified_at
            .is_some_and(|prior| now < prior)
        {
            diagnostics.push(crate::presentation::publication_text::CLOCK_PREDATES_EDIT.into());
        }
        document.modified_at = Some(now);
        let recipe = (*recipe)
            .clone()
            .with_document(document)
            .map_err(|e| Error::Recipe { error: e })?;
        let bytes = recipe
            .to_toml()
            .map_err(|e| Error::Recipe { error: e })?
            .into_bytes();
        let publication = publish::publish_in(
            &dir,
            Path::new(name),
            &expected.path,
            Policy::ReplaceIfUnchanged {
                fingerprint: expected.fingerprint,
                identity: &expected.handle,
            },
            cancel,
            |file| {
                file.write_all(&bytes)
                    .map_err(|e| io(&expected.path, IoOperation::Write, e))?;
                same_file::Handle::from_file(
                    file.try_clone()
                        .map_err(|e| io(&expected.path, IoOperation::RetainIdentity, e))?,
                )
                .map_err(|e| io(&expected.path, IoOperation::RetainIdentity, e))
            },
        )?;
        diagnostics.extend(publication.warnings);
        let observed = ObservedFile {
            path: expected.path.clone(),
            fingerprint: Fingerprint::of(&bytes),
            handle: Arc::new(publication.value),
        };
        Ok(ExternalOutcome {
            status: if diagnostics.is_empty() {
                MutationStatus::Published
            } else {
                MutationStatus::PublishedWithWarning
            },
            previous: expected.clone(),
            recipe: Arc::new(recipe),
            observed,
            diagnostics,
        })
    }
}
fn io(path: &Path, action: IoOperation, error: std::io::Error) -> FileError {
    FileError::Io(IoFailure::new(action, Some(path.to_owned()), error))
}
#[cfg(test)]
mod tests;
