//! File export and recipe publication workflows.
use crate::core::Recipe;
use crate::error::{Error, ErrorKind};
use crate::recipe::{Rendition, builtin_source};
use crate::wav::Exported;
use crate::{publish, wav};
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

/// Export stereo float WAV, sync it and publish the completed file.
///
/// Confirmed replacement is atomic. No-overwrite publication preserves an
/// existing destination but is not guaranteed atomic on every platform.
///
/// # Errors
/// Returns [`ErrorKind::DestinationExists`], [`ErrorKind::Cancelled`] or
/// [`ErrorKind::Write`]. A cancellation requested after the file was
/// published does not unpublish it.
pub fn export(
    sound: &Rendition,
    path: &Path,
    replace: Replace,
    cancel: &Cancel,
    progress: &Progress,
) -> Result<Exported, Error> {
    publish::publish(path, replace, cancel, |file| {
        wav::encode(sound.signal(), file, &cancel.0, progress).map_err(|e| match e {
            wav::ExportError::Cancelled => Error::new(ErrorKind::Cancelled, "export cancelled"),
            wav::ExportError::Io(e) => Error::caused(
                ErrorKind::Write,
                format!("cannot export to {path:?}: {e}"),
                e,
            ),
        })
    })
    .map(|published| {
        let mut exported = published.value;
        exported.warnings.extend(published.warnings);
        exported
    })
}

/// Save a built-in recipe's exact text, comments included.
///
/// # Errors
/// Returns [`ErrorKind::UnknownRecipe`], [`ErrorKind::DestinationExists`]
/// under [`Replace::Never`], or [`ErrorKind::Write`].
pub fn save_builtin(id: &str, path: &Path, replace: Replace) -> Result<SaveOutcome, Error> {
    use std::io::Write;
    let text = builtin_source(id)?;
    publish::publish(path, replace, &Cancel::default(), |file| {
        file.write_all(text.as_bytes()).map_err(|e| {
            Error::caused(ErrorKind::Write, format!("cannot save to {path:?}: {e}"), e)
        })
    })
    .map(|published| SaveOutcome {
        warnings: published.warnings,
    })
}

/// Export exact captured portable UTF-8 recipe bytes without rewriting history.
///
/// # Errors
/// Rejects unsupported/invalid content before staging and preserves explicit
/// no-clobber/replacement publication semantics.
pub fn save_recipe_text(text: &str, path: &Path, replace: Replace) -> Result<SaveOutcome, Error> {
    use std::io::Write;
    Recipe::parse(text).map_err(|e| Error::caused(ErrorKind::InvalidRecipe, e.to_string(), e))?;
    publish::publish(path, replace, &Cancel::default(), |file| {
        file.write_all(text.as_bytes()).map_err(|e| {
            Error::caused(
                ErrorKind::Write,
                format!("cannot export recipe to {path:?}: {e}"),
                e,
            )
        })
    })
    .map(|published| SaveOutcome {
        warnings: published.warnings,
    })
}
