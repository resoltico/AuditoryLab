//! Captured portable descriptions and rate-dependent inspections, independent of output format.
use super::Resolved;
use {
    crate::core::SampleRate, crate::document::recipe::Rendition, crate::outcome::Outcome,
    crate::services::OperationError, crate::storage::load::LoadError,
};
/// Complete portable content and chronology diagnostics for one captured source.
#[derive(Debug)]
pub struct Description<'a> {
    pub(super) source: &'a Resolved,
    pub(super) text: std::sync::Arc<str>,
    pub(super) history: Vec<crate::storage::warning::StorageWarning>,
}
impl<'a> Description<'a> {
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
    pub fn history_warnings(&self) -> &[crate::storage::warning::StorageWarning] {
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
    component_check: Option<crate::core::TargetCheck>,
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
    /// Complete component diagnostics and surviving lane facts for invalid sound.
    /// Valid inspections expose their compiled facts through `sound` instead.
    #[must_use]
    pub const fn component_check(&self) -> Option<&crate::core::TargetCheck> {
        self.component_check.as_ref()
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
    let result = source.snapshot.at(rate).map_err(Into::into);
    let component_check = result
        .as_ref()
        .err()
        .map(|_| source.snapshot.sound().check(rate));
    Inspection {
        source,
        rate,
        result,
        component_check,
    }
}
