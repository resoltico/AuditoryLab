//! Shared source, discovery, playback and publication operations.
mod discovery;
mod inspection;
pub mod library;
pub mod playback;
mod sources;
use crate::core::Recipe;
use crate::core::SampleRate;
use crate::document::recipe::{RecipeSnapshot, Rendition};
use crate::error::{ExportError, FileError, IoFailure, IoOperation, SaveError};
pub use crate::storage::external::{ExternalOutcome, ObservedFile};
pub use crate::storage::library::history_warnings;

pub use crate::storage::wav::{Container, Exported};
use crate::{storage::publish, storage::wav};
pub use discovery::{ListRequest, Listing, list};
pub use inspection::{Description, Inspection, describe, inspect};
pub use sources::{Resolved, Source, builtin, builtin_source, catalog, portable_text, resolve};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// Confirmed recipe-file publication and any durability or cleanup warnings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveOutcome {
    /// Warnings do not undo successful publication or prevent advancing a saved baseline.
    pub warnings: Vec<String>,
}
/// Whether publication may replace an existing destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replace {
    /// Keep existing files untouched.
    Never,
    /// Atomically replace, after the person confirmed it.
    Allowed,
}

/// A file export's cancellation request; audio sessions have their own Stop.
#[derive(Clone, Default, Debug)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// Request cancellation.
    pub fn request(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether cancellation was requested.
    #[must_use]
    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A file export's written frame count.
#[derive(Clone, Default, Debug)]
pub struct Progress(Arc<AtomicU64>);

impl Progress {
    /// Frames written so far.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    pub(crate) fn set(&self, n: u64) {
        self.0.store(n, Ordering::Relaxed);
    }
}

/// Export a captured rendition with staged no-clobber or confirmed replacement.
///
/// # Errors
/// Preserves typed filesystem, cancellation and verification failures.
pub fn export(
    snapshot: &RecipeSnapshot,
    rate: SampleRate,
    path: &Path,
    replace: Replace,
    cancel: &Cancel,
    progress: &Progress,
) -> Result<Exported, ExportError> {
    let sound = snapshot.realize(rate).map_err(|error| ExportError::Sound {
        origin: Box::new(snapshot.origin().clone()),
        rate,
        error,
    })?;
    export_rendition(&sound, path, replace, cancel, progress)
}
fn export_rendition(
    sound: &Rendition,
    path: &Path,
    replace: Replace,
    cancel: &Cancel,
    progress: &Progress,
) -> Result<Exported, ExportError> {
    publish::publish(path, replace, cancel, |file| {
        wav::encode(sound.signal(), file, &cancel.0, progress).map_err(|error| match error {
            wav::ExportError::Cancelled => FileError::Cancelled {
                path: Some(path.to_owned()),
            },
            wav::ExportError::Io(error) => FileError::Io(IoFailure::new(
                IoOperation::Write,
                Some(path.to_owned()),
                error,
            )),
        })
    })
    .map(|published| {
        let mut exported = published.value;
        exported.warnings.extend(published.warnings);
        exported
    })
    .map_err(ExportError::from)
}
/// Save a built-in's exact portable text.
///
/// # Errors
/// Preserves source resolution and staged publication failures.
pub fn save_builtin(id: &str, path: &Path, replace: Replace) -> Result<SaveOutcome, SaveError> {
    let text = builtin_source(id).map_err(|error| SaveError::Load { error })?;
    publish_text(text, path, replace)
}
/// Export captured portable text without rewriting history.
///
/// # Errors
/// Rejects an invalid portable contract before staging.
pub fn save_recipe_text(
    text: &str,
    path: &Path,
    replace: Replace,
) -> Result<SaveOutcome, SaveError> {
    Recipe::parse(text)?;
    publish_text(text, path, replace)
}
/// Save validated portable content with staged publication.
///
/// # Errors
/// Preserves size, serialization and publication failures.
pub fn save_content(
    recipe: &Recipe,
    path: &Path,
    replace: Replace,
) -> Result<SaveOutcome, SaveError> {
    let text = crate::document::editing::serialize(recipe)?;
    publish_text(&text, path, replace)
}
fn publish_text(text: &str, path: &Path, replace: Replace) -> Result<SaveOutcome, SaveError> {
    use std::io::Write;
    publish::publish(path, replace, &Cancel::default(), |file| {
        file.write_all(text.as_bytes()).map_err(|error| {
            FileError::Io(IoFailure::new(
                IoOperation::Write,
                Some(path.to_owned()),
                error,
            ))
        })
    })
    .map(|published| SaveOutcome {
        warnings: published.warnings,
    })
    .map_err(SaveError::from)
}

/// Open one bounded regular portable input without adding it to the User Library.
///
/// # Errors
/// Preserves input admission, encoding, schema and contract failures.
pub fn open_file(
    path: &Path,
) -> Result<crate::document::recipe::RecipeSnapshot, crate::error::LoadError> {
    crate::storage::load::load_file(path)
}
/// Read one bounded portable stream under an explicit captured attribution.
///
/// # Errors
/// Preserves stream, encoding, schema and portable-contract failures.
pub fn read_source(
    reader: impl std::io::Read,
    label: &str,
) -> Result<crate::document::recipe::RecipeSnapshot, crate::error::LoadError> {
    crate::storage::load::load_reader(reader, label)
}
/// Save a captured external version with identity, bytes and history checked at publication.
///
/// # Errors
/// Preserves captured-version conflicts and truthful publication/cleanup failures.
pub fn save_external(
    expected: &ObservedFile,
    recipe: Arc<Recipe>,
    cancel: &Cancel,
) -> Result<crate::storage::external::ExternalOutcome, SaveError> {
    crate::storage::external::ExternalWriter.save(expected, recipe, cancel)
}

#[cfg(test)]
mod tests;
