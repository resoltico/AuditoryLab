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
            Self::Recipe(_) => Outcome::InvalidRecipe,
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
    pub const fn outcome() -> crate::outcome::Outcome {
        crate::outcome::Outcome::InvalidSound
    }
}

impl std::fmt::Display for EditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for EditorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Identity(error) => Some(error),
            Self::Recipe(error) => Some(error),
        }
    }
}

impl std::fmt::Display for RenditionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for crate::document::error::RenditionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    EditorError {
        codes {}
        delegates {
            Self::Identity(error) => error,
            Self::Recipe(error) => error,
        }
    }
    RenditionError {
        codes {}
        delegates {
            Self { error, .. } => error,
        }
        params(this) {
            crate::core::diagnostic_params! {
                "origin"=>this.origin.to_string(),
                "diagnostic_rate_hz"=>this.rate.hz(),
            }
        }
    }
}
