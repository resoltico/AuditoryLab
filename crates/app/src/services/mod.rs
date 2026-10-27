//! Shared source, discovery, playback and publication operations.
mod error;
pub use self::error::{ExportError, OperationError};
use crate::document::origin::ObservedFile;
pub(crate) use error::CODES as DIAGNOSTIC_CODES;
pub use library::LibraryContext;
pub use {
    crate::storage::error::Confirmation, crate::storage::error::FileError,
    crate::storage::error::IoCause, crate::storage::error::IoFailure,
    crate::storage::error::IoOperation, crate::storage::error::MutationPhase,
    crate::storage::error::SaveError, crate::storage::load::LoadError,
};
mod discovery;
mod inspection;
pub mod library;
pub mod playback;
mod sources;
use crate::core::Recipe;
use crate::core::SampleRate;
use crate::document::recipe::{RecipeSnapshot, Rendition};

use crate::job::{Cancel, Progress, Replace};
pub use crate::storage::external::ExternalOutcome;
pub use crate::storage::library::history_warnings;

pub use crate::storage::wav::{Container, Exported};
use crate::{storage::publish, storage::wav};
pub use discovery::{ListRequest, Listing, list};
pub use inspection::{Description, Inspection, describe, inspect};
pub use sources::{Resolved, Source, builtin, builtin_source, catalog, portable_text, resolve};
use std::{path::Path, sync::Arc};

/// Confirmed recipe-file publication and any durability or cleanup warnings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SaveOutcome {
    /// Warnings do not undo successful publication or prevent advancing a saved baseline.
    pub warnings: Vec<crate::storage::warning::StorageWarning>,
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
/// Export this already inspected rendition without realizing its snapshot again.
///
/// # Errors
/// Preserves staged publication, cancellation and encoding failures.
pub fn export_rendition(
    sound: &Rendition,
    path: &Path,
    replace: Replace,
    cancel: &Cancel,
    progress: &Progress,
) -> Result<Exported, ExportError> {
    publish::publish(path, replace, cancel, |file| {
        wav::encode(sound.signal(), file, cancel.flag(), progress).map_err(|error| match error {
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
) -> Result<crate::document::recipe::RecipeSnapshot, crate::storage::load::LoadError> {
    crate::storage::load::load_file(path)
}
/// Read one bounded portable stream under an explicit captured attribution.
///
/// # Errors
/// Preserves stream, encoding, schema and portable-contract failures.
pub fn read_source(
    reader: impl std::io::Read,
    label: &str,
) -> Result<crate::document::recipe::RecipeSnapshot, crate::storage::load::LoadError> {
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

/// Typed nonfatal storage outcomes.
pub use crate::storage::warning::StorageWarning;
