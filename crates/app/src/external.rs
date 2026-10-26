//! External Save with observed identity and exact-byte checks immediately before publication.
use crate::core::Recipe;
use crate::error::{Error, ErrorKind};
use crate::library::{Fingerprint, MutationStatus};
use crate::publish::{self, Policy};
use crate::recipe::RecipeSnapshot;
use crate::workflow::Cancel;
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
    pub(crate) fn capture(path: &Path, file: File, bytes: &[u8]) -> Result<Self, Error> {
        let path = std::fs::canonicalize(path)
            .map_err(|e| io("cannot resolve opened external file", e))?;
        let handle = same_file::Handle::from_file(file)
            .map_err(|e| io("cannot retain external file identity", e))?;
        let current = same_file::Handle::from_path(&path)
            .map_err(|e| io("cannot check opened path identity", e))?;
        if handle != current {
            return Err(conflict());
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
            crate::recipe::Origin::File(self.observed.path.clone()),
            (*self.recipe).clone(),
        )
        .with_external(self.observed.clone())
    }
}
/// Stateless external writer using the shared publication primitive.
#[derive(Debug, Clone, Default)]
pub struct ExternalWriter;
impl ExternalWriter {
    /// Save a captured recipe only while the opened file's identity and bytes still match.
    ///
    /// # Errors
    /// Rejects stale observations, history changes and invalid content; ambiguous I/O is verified before returning.
    pub fn save(
        &self,
        expected: &ObservedFile,
        recipe: Arc<Recipe>,
        cancel: &Cancel,
    ) -> Result<ExternalOutcome, Error> {
        if cancel.is_requested() {
            return Err(Error::new(
                ErrorKind::Cancelled,
                "external Save cancelled before publication",
            ));
        }
        let parent = expected
            .path
            .parent()
            .ok_or_else(|| Error::new(ErrorKind::Write, "external destination needs a parent"))?;
        let name = expected
            .path
            .file_name()
            .ok_or_else(|| Error::new(ErrorKind::Write, "external destination needs a filename"))?;
        let dir = Dir::open_ambient_dir(parent, cap_std::ambient_authority())
            .map_err(|e| io("cannot open external parent", e))?;
        let mut file = publish::open_regular(&dir, Path::new(name))?;
        let mut bytes = Vec::new();
        use std::io::Read;
        (&mut file)
            .take((Recipe::MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| io("cannot read external recipe", e))?;
        if bytes.len() > Recipe::MAX_BYTES {
            return Err(Error::new(
                ErrorKind::InvalidRecipe,
                "external recipe exceeds its byte limit",
            ));
        }
        let current = ObservedFile::capture(&expected.path, file, &bytes)?;
        if !expected.matches(&current) {
            return Err(conflict());
        }
        let original = Recipe::from_bytes(&bytes)
            .map_err(|e| Error::new(ErrorKind::InvalidRecipe, e.to_string()))?;
        if original.document().id != recipe.document().id
            || original.document().created_at != recipe.document().created_at
        {
            return Err(Error::new(
                ErrorKind::HistoryConflict,
                "external Save must retain identity and creation history; save an independent copy instead",
            ));
        }
        if original.same_editable_content(&recipe) {
            return Ok(ExternalOutcome {
                status: MutationStatus::Unchanged,
                previous: expected.clone(),
                recipe: Arc::new(original),
                observed: current,
                diagnostics: Vec::new(),
            });
        }
        let mut document = recipe.document().clone();
        let now = crate::library::store::observed_now()?;
        let mut diagnostics = Vec::new();
        if original
            .document()
            .modified_at
            .is_some_and(|prior| now < prior)
        {
            diagnostics.push("Observed clock predates the prior declared edit time; no artificial timestamp was fabricated.".into());
        }
        document.modified_at = Some(now);
        let recipe = (*recipe)
            .clone()
            .with_document(document)
            .map_err(|e| Error::new(ErrorKind::InvalidRecipe, e.to_string()))?;
        let bytes = recipe
            .to_toml()
            .map_err(|e| Error::new(ErrorKind::InvalidRecipe, e.to_string()))?
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
                    .map_err(|e| io("cannot write external recipe stage", e))?;
                same_file::Handle::from_file(
                    file.try_clone()
                        .map_err(|e| io("cannot retain saved file handle", e))?,
                )
                .map_err(|e| io("cannot retain saved file identity", e))
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
fn io(context: &str, error: std::io::Error) -> Error {
    Error::caused(ErrorKind::Write, format!("{context}: {error}"), error)
}
fn conflict() -> Error {
    Error::new(
        ErrorKind::Conflict,
        "external file changed identity or bytes since opening; reload or save an independent copy",
    )
}
#[cfg(test)]
mod tests;
