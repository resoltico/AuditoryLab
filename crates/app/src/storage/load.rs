//! Bounded portable recipe loading from built-ins, opened files and readers.
use super::input_admission;
use crate::core::Recipe;
use crate::document::recipe::{Origin, RecipeSnapshot};
use crate::error::{Error, ErrorKind};
use std::{io::Read, path::Path};

/// Load every built-in recipe in natural order.
///
/// # Errors
/// Returns an error only if a shipped recipe fails to parse.
pub fn catalog() -> Result<Vec<RecipeSnapshot>, Error> {
    auditorylab_core::catalog::ENTRIES
        .iter()
        .map(|e| builtin(e.id))
        .collect()
}

fn entry(id: &str) -> Result<&'static auditorylab_core::catalog::Entry, Error> {
    auditorylab_core::catalog::find(id).ok_or_else(|| {
        Error::new(
            ErrorKind::UnknownRecipe,
            format!("unknown built-in recipe {id:?}"),
        )
    })
}

/// Load a built-in recipe by exact ID.
///
/// # Errors
/// Returns [`ErrorKind::UnknownRecipe`] for an unknown ID.
pub fn builtin(id: &str) -> Result<RecipeSnapshot, Error> {
    let e = entry(id)?;
    parse(e.text.as_bytes(), Origin::BuiltIn(e.id))
}

/// Return a built-in recipe's exact TOML text, comments included.
///
/// # Errors
/// Returns [`ErrorKind::UnknownRecipe`] for an unknown ID.
pub fn builtin_source(id: &str) -> Result<&'static str, Error> {
    Ok(entry(id)?.text)
}

fn parse(bytes: &[u8], origin: Origin) -> Result<RecipeSnapshot, Error> {
    let recipe = Recipe::from_bytes(bytes)
        .map_err(|e| Error::caused(ErrorKind::InvalidRecipe, format!("{origin}: {e}"), e))?;
    Ok(RecipeSnapshot::new(origin, recipe))
}

fn read(reader: impl Read, origin: Origin) -> Result<RecipeSnapshot, Error> {
    let mut bytes = Vec::new();
    reader
        .take((Recipe::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            Error::caused(
                ErrorKind::Unreadable,
                format!("cannot read {origin}: {e}"),
                e,
            )
        })?;
    parse(&bytes, origin)
}

/// Read a bounded recipe stream.
///
/// # Errors
/// Returns [`ErrorKind::Unreadable`] or [`ErrorKind::InvalidRecipe`].
pub fn load_reader(reader: impl Read, label: &str) -> Result<RecipeSnapshot, Error> {
    read(reader, Origin::Stream(label.into()))
}

/// Open and read a regular recipe file without blocking on special files.
///
/// # Errors
/// Returns [`ErrorKind::Unreadable`] or [`ErrorKind::InvalidRecipe`].
pub fn load_file(path: &Path) -> Result<RecipeSnapshot, Error> {
    let file = input_admission::open(path).map_err(|e| match e {
        input_admission::Failure::Io(e) => Error::caused(
            ErrorKind::Unreadable,
            format!("cannot open {path:?}: {e}"),
            e,
        ),
        input_admission::Failure::NotRegular => Error::new(
            ErrorKind::Unreadable,
            format!("cannot read {path:?}: expected a regular file"),
        ),
    })?;
    let mut bytes = Vec::new();
    let mut file = file;
    (&mut file)
        .take((Recipe::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::caused(ErrorKind::Unreadable, "cannot read external recipe", e))?;
    let snapshot = parse(&bytes, Origin::File(path.to_owned()))?;
    let observed = crate::storage::external::ObservedFile::capture(path, file, &bytes)?;
    Ok(snapshot.with_external(observed))
}
