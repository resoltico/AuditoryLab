//! Fallible random identities for documents and file publication.
/// Allocate a fallible random portable identity; never substitute clocks or counters.
pub(crate) fn fresh_document()
-> Result<auditorylab_core::DocumentIdentity, crate::identity::IdentityError> {
    Ok(auditorylab_core::DocumentIdentity {
        id: random_uuid()?,
        created_at: None,
        modified_at: None,
    })
}

pub(crate) fn random_uuid() -> Result<auditorylab_core::DocumentUuid, crate::identity::IdentityError>
{
    generate_uuid(getrandom::fill)
}
fn generate_uuid(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<auditorylab_core::DocumentUuid, crate::identity::IdentityError> {
    let mut bytes = [0_u8; 16];
    fill(&mut bytes).map_err(crate::identity::IdentityError::Entropy)?;
    Ok(auditorylab_core::DocumentUuid::from_random_bytes(bytes))
}

use crate::outcome::Outcome;
/// Identity allocation context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityScope {
    /// Process-local editor documents.
    Document,
    /// Process-local external entries.
    ExternalEntry,
}
/// Secure or process-local identities could not be allocated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// The native entropy source failed.
    Entropy(getrandom::Error),
    /// A process-local counter is exhausted.
    Exhausted(IdentityScope),
}
impl IdentityError {
    /// Failure category.
    #[must_use]
    pub const fn outcome(&self) -> Outcome {
        match self {
            Self::Entropy(_) | Self::Exhausted(IdentityScope::ExternalEntry) => Outcome::Identity,
            Self::Exhausted(IdentityScope::Document) => Outcome::InvalidRecipe,
        }
    }
}

impl std::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Entropy(..) => "identity.entropy",
            Self::Exhausted(..) => "identity.exhausted",
        })
    }
}
impl std::error::Error for IdentityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Entropy(error) => Some(error),
            Self::Exhausted(_) => None,
        }
    }
}

/// Identity shared by the command-line and desktop products.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildIdentity {
    /// Product name declared in workspace metadata.
    pub name: &'static str,
    /// Copyright line declared in workspace metadata.
    pub copyright: &'static str,
    /// Workspace product version.
    pub version: &'static str,
    /// Native build counter in 1..9999.
    pub build: u32,
    /// Validated full ASCII hexadecimal source revision, when supplied at build time.
    pub revision: Option<&'static str>,
}
mod generated {
    use super::BuildIdentity;
    include!(concat!(env!("OUT_DIR"), "/identity.rs"));
}
/// Canonical repository URL inherited from workspace package metadata.
pub const SOURCE_URL: &str = env!("CARGO_PKG_REPOSITORY");
/// Identity generated from workspace metadata.
pub const BUILD_IDENTITY: BuildIdentity = generated::BUILD;

#[cfg(test)]
#[path = "../identity_input.rs"]
mod input;
#[cfg(test)]
mod tests {
    use crate::presentation::Explain;
    #[test]
    fn random_failure_does_not_fall_back_to_clock_or_counter() {
        let error = super::generate_uuid(|_| Err(getrandom::Error::UNSUPPORTED)).unwrap_err();
        assert_eq!(error.outcome(), crate::outcome::Outcome::Identity);
        assert_eq!(error.to_string(), "identity.entropy");
        assert!(error.explain().contains("random UUID"));
        assert!(std::error::Error::source(&error).is_some());
    }
}
