//! Bounded portable recipe loading from built-ins, opened files and readers.
use super::input_admission;
use crate::core::Recipe;
use crate::document::recipe::{Origin, RecipeSnapshot};
use crate::error::{FileError, IoCause, IoFailure, IoOperation, LoadError as Error};
use std::{io::Read, path::Path};
/// Load every built-in recipe in natural order.
///
/// # Errors
/// Returns the exact portable failure of an invalid shipped recipe.
pub fn catalog() -> Result<Vec<RecipeSnapshot>, Error> {
    auditorylab_core::catalog::ENTRIES
        .iter()
        .map(|entry| builtin(entry.id))
        .collect()
}
fn entry(id: &str) -> Result<&'static auditorylab_core::catalog::Entry, Error> {
    auditorylab_core::catalog::find(id).ok_or_else(|| Error::UnknownBuiltin { id: id.into() })
}
/// Load a built-in recipe by exact ID.
///
/// # Errors
/// Returns an unknown-selector or portable-contract failure.
pub fn builtin(id: &str) -> Result<RecipeSnapshot, Error> {
    let entry = entry(id)?;
    parse(entry.text.as_bytes(), Origin::BuiltIn(entry.id))
}
/// Return the exact embedded portable text of a built-in.
///
/// # Errors
/// Returns an unknown-selector failure.
pub fn builtin_source(id: &str) -> Result<&'static str, Error> {
    Ok(entry(id)?.text)
}
fn parse(bytes: &[u8], origin: Origin) -> Result<RecipeSnapshot, Error> {
    let recipe = Recipe::from_bytes(bytes).map_err(|error| Error::Recipe {
        origin: origin.clone(),
        error: Box::new(error),
    })?;
    Ok(RecipeSnapshot::new(origin, recipe))
}
fn read(reader: impl Read, origin: Origin) -> Result<RecipeSnapshot, Error> {
    let mut bytes = Vec::new();
    reader
        .take((Recipe::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| Error::Read {
            origin: origin.clone(),
            cause: IoCause::new(error),
        })?;
    parse(&bytes, origin)
}
/// Read a bounded portable stream.
///
/// # Errors
/// Preserves read, encoding, format, contract and version failures.
pub fn load_reader(reader: impl Read, label: &str) -> Result<RecipeSnapshot, Error> {
    read(reader, Origin::Stream(label.into()))
}
/// Open and read a regular recipe file without blocking on special files.
///
/// # Errors
/// Preserves opened-object admission and portable-contract failures.
pub fn load_file(path: &Path) -> Result<RecipeSnapshot, Error> {
    let mut file = input_admission::open(path).map_err(|error| match error {
        input_admission::Failure::Io(error) => Error::File(FileError::Io(IoFailure::new(
            IoOperation::Open,
            Some(path.to_owned()),
            error,
        ))),
        input_admission::Failure::NotRegular => Error::File(FileError::NotRegular {
            path: path.to_owned(),
        }),
    })?;
    let mut bytes = Vec::new();
    (&mut file)
        .take((Recipe::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| Error::Read {
            origin: Origin::File(path.to_owned()),
            cause: IoCause::new(error),
        })?;
    let snapshot = parse(&bytes, Origin::File(path.to_owned()))?;
    let observed = crate::storage::external::ObservedFile::capture(path, file, &bytes)?;
    Ok(snapshot.with_external(observed))
}
