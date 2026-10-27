//! Captured portable descriptions and rate-dependent inspections, independent of output format.
use super::Resolved;
use crate::{
    core::SampleRate,
    document::recipe::Rendition,
    error::{LoadError, OperationError, Outcome},
};
/// Complete portable content and chronology diagnostics for one captured source.
#[derive(Debug)]
pub struct Description<'a> {
    pub(super) source: &'a Resolved,
    pub(super) text: std::sync::Arc<str>,
    pub(super) history: Vec<String>,
}
impl<'a> Description<'a> {
    pub(crate) fn into_parts(self) -> (&'a Resolved, std::sync::Arc<str>, Vec<String>) {
        (self.source, self.text, self.history)
    }
    /// Captured source and correlated ownership.
    #[must_use]
    pub const fn source(&self) -> &'a Resolved {
        self.source
    }
    /// Encoded portable content.
    #[must_use]
    pub fn portable_text(&self) -> &str {
        &self.text
    }
    /// Declared chronology warnings against one observed clock.
    #[must_use]
    pub fn history_warnings(&self) -> &[String] {
        &self.history
    }
}
/// Describe captured content without changing declared dates or initializing a root.
///
/// # Errors
/// Preserves exact portable serialization failures.
pub fn describe(source: &Resolved) -> Result<Description<'_>, OperationError> {
    Ok(Description {
        source,
        text: source.snapshot.canonical_text()?,
        history: crate::storage::library::history_warnings(source.snapshot.recipe()),
    })
}
/// One rate's actual validation outcome; presentation never recompiles it.
#[derive(Debug)]
pub struct Inspection<'a> {
    source: &'a Resolved,
    rate: SampleRate,
    result: Result<Rendition, LoadError>,
}
impl<'a> Inspection<'a> {
    /// Captured source and ownership.
    #[must_use]
    pub const fn source(&self) -> &'a Resolved {
        self.source
    }
    /// Requested rendering rate.
    #[must_use]
    pub const fn rate(&self) -> SampleRate {
        self.rate
    }
    /// Validated rendition or exact captured failure.
    ///
    /// # Errors
    /// Returns the validation failure produced by this inspection.
    pub fn sound(&self) -> Result<&Rendition, &LoadError> {
        self.result.as_ref()
    }
    /// Frontend-independent failure category; a valid inspection has none.
    #[must_use]
    pub fn outcome(&self) -> Option<Outcome> {
        self.result.as_ref().err().map(LoadError::outcome)
    }
}
/// Validate one captured source at a rate, retaining an invalid result for structured reporting.
#[must_use]
pub fn inspect(source: &Resolved, rate: SampleRate) -> Inspection<'_> {
    Inspection {
        source,
        rate,
        result: source.snapshot.at(rate),
    }
}
