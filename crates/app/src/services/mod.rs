//! File export and recipe publication workflows.
use crate::core::Recipe;
use crate::document::recipe::Rendition;
use crate::error::{ExportError, FileError, IoFailure, IoOperation, SaveError};
use crate::storage::load::builtin_source;
use crate::storage::wav::Exported;
use crate::{storage::publish, storage::wav};
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
