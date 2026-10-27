//! Source resolution captures complete provenance before rendering or device work.
use super::library::Library;
use crate::document::origin::{ExternalId, LibraryKey};
use std::{io::Read, path::PathBuf};
use {
    crate::discovery::collection::EntrySummary, crate::document::recipe::RecipeSnapshot,
    crate::storage::load, crate::storage::load::LoadError,
};

/// One explicit source; a stream's label belongs to its frontend.
pub enum Source<'a> {
    /// Stable Built-in or User selector.
    Selector(LibraryKey),
    /// Bounded regular portable file.
    File(PathBuf),
    /// Bounded portable stream.
    Stream {
        /// Reader supplied by the frontend.
        reader: &'a mut dyn Read,
        /// Captured attribution, such as standard input.
        label: &'a str,
    },
}
/// An immutable resolved source and its correlated discovery metadata.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub(super) snapshot: RecipeSnapshot,
    pub(super) summary: EntrySummary,
    pub(super) managed: Option<Library>,
}
impl Resolved {
    /// Captured portable content and origin.
    #[must_use]
    pub const fn snapshot(&self) -> &RecipeSnapshot {
        &self.snapshot
    }
    /// Correlated ownership, membership and nominal facts.
    #[must_use]
    pub const fn summary(&self) -> &EntrySummary {
        &self.summary
    }
    /// Consume resolution when only immutable content is needed.
    #[must_use]
    pub fn into_snapshot(self) -> RecipeSnapshot {
        self.snapshot
    }
}
/// Resolve a selector, regular file or bounded stream without initializing storage.
///
/// # Errors
/// Preserves configuration, input, identity and portable-contract failures.
pub fn resolve(
    source: Source<'_>,
    root_override: Option<&std::path::Path>,
) -> Result<Resolved, LoadError> {
    let snapshot = match source {
        Source::Selector(key) => match &key {
            LibraryKey::BuiltIn(id) => {
                let snapshot = builtin(id.as_str())?;
                let summary = EntrySummary::from_snapshot(key, &snapshot, None);
                return Ok(Resolved {
                    snapshot,
                    summary,
                    managed: None,
                });
            }
            LibraryKey::User(_) => {
                let library = Library::resolve(root_override)?;
                let record = library.load(&key)?;
                return Ok(Resolved {
                    summary: record.summary(),
                    snapshot: record.snapshot,
                    managed: Some(library),
                });
            }
            LibraryKey::External(_) => return Err(LoadError::TemporarySelector { key }),
        },
        Source::File(path) => super::open_file(&path)?,
        Source::Stream { reader, label } => super::read_source(reader, label)?,
    };
    let key = LibraryKey::External(ExternalId::allocate().map_err(LoadError::Identity)?);
    let summary = EntrySummary::from_snapshot(key, &snapshot, None);
    Ok(Resolved {
        snapshot,
        summary,
        managed: None,
    })
}
/// Exact portable text; built-ins retain their embedded comments and whitespace.
///
/// # Errors
/// Rejects temporary selectors and preserves managed input failures.
pub fn portable_text(
    key: &LibraryKey,
    root_override: Option<&std::path::Path>,
) -> Result<String, LoadError> {
    match key {
        LibraryKey::BuiltIn(id) => Ok(builtin_source(id.as_str())?.into()),
        LibraryKey::User(_) => Ok(Library::resolve(root_override)?
            .load(key)?
            .recipe_toml
            .to_string()),
        LibraryKey::External(_) => Err(LoadError::TemporarySelector { key: key.clone() }),
    }
}

/// Load every built-in recipe in natural order.
///
/// # Errors
/// Returns the exact portable failure of an invalid shipped recipe.
pub fn catalog() -> Result<Vec<RecipeSnapshot>, LoadError> {
    auditorylab_core::catalog::ENTRIES
        .iter()
        .map(|entry| builtin(entry.id))
        .collect()
}
fn entry(id: &str) -> Result<&'static auditorylab_core::catalog::Entry, LoadError> {
    auditorylab_core::catalog::find(id).ok_or_else(|| LoadError::UnknownBuiltin { id: id.into() })
}
/// Load a built-in recipe by exact ID.
///
/// # Errors
/// Returns an unknown-selector or portable-contract failure.
pub fn builtin(id: &str) -> Result<RecipeSnapshot, LoadError> {
    let entry = entry(id)?;
    load::parse(
        entry.text.as_bytes(),
        crate::document::recipe::Origin::BuiltIn(entry.id),
    )
}
/// Return the exact embedded portable text of a built-in.
///
/// # Errors
/// Returns an unknown-selector failure.
pub fn builtin_source(id: &str) -> Result<&'static str, LoadError> {
    Ok(entry(id)?.text)
}
