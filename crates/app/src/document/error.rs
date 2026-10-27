//! Document construction and rendition failures.
use crate::{core::RecipeError, identity::IdentityError, outcome::Outcome};
/// An editable document could not be created.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorError {
    /// Identity creation failed.
    Identity(IdentityError),
    /// Portable copy construction failed.
    Recipe(RecipeError),
}
impl EditorError {
    /// Failure category.
    #[must_use]
    pub fn outcome(&self) -> Outcome {
        match self {
            Self::Identity(error) => error.outcome(),
            _ => Outcome::InvalidRecipe,
        }
    }
}
impl From<IdentityError> for EditorError {
    fn from(error: IdentityError) -> Self {
        Self::Identity(error)
    }
}
impl From<RecipeError> for EditorError {
    fn from(error: RecipeError) -> Self {
        Self::Recipe(error)
    }
}

/// A captured recipe cannot be synthesized at the requested rate.
#[derive(Debug, Clone, PartialEq)]
pub struct RenditionError {
    /// Captured source attribution.
    pub origin: crate::document::recipe::Origin,
    /// Requested sample rate.
    pub rate: crate::core::SampleRate,
    /// Exact acoustic failure.
    pub error: crate::core::SoundError,
}
impl RenditionError {
    /// Frontend-independent failure category.
    pub const fn outcome(&self) -> crate::outcome::Outcome {
        crate::outcome::Outcome::InvalidSound
    }
}
