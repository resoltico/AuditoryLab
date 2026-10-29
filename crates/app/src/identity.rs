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

pub(crate) fn random_uuid() -> Result<crate::core::DocumentUuid, IdentityError> {
    generate_uuid(getrandom::fill)
}
fn generate_uuid(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<crate::core::DocumentUuid, IdentityError> {
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
        f.write_str(crate::core::diagnostic::Code::code(self))
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
    /// Validated full ASCII hexadecimal source revision, when supplied at build time.
    pub revision: Option<&'static str>,
}
impl BuildIdentity {
    /// Link to a repository document as it was at this build: the exact source
    /// revision when embedded, otherwise the release tag of this version.
    #[must_use]
    pub fn document_url(self, path: &str) -> String {
        let reference = self
            .revision
            .map_or_else(|| format!("v{}", self.version), str::to_owned);
        format!("{SOURCE_URL}/blob/{reference}/{path}")
    }
}
mod generated {
    use super::BuildIdentity;
    include!(concat!(env!("OUT_DIR"), "/identity.rs"));
}
/// Canonical repository URL inherited from workspace package metadata.
pub const SOURCE_URL: &str = env!("CARGO_PKG_REPOSITORY");
/// Identity generated from workspace metadata.
pub const BUILD_IDENTITY: BuildIdentity = generated::BUILD;

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    IdentityError {
        codes {
            Self::Entropy(..) => "identity.entropy",
            Self::Exhausted(..) => "identity.exhausted",
        }
        delegates {}
        params(this) {
            match this {
                Self::Entropy(error) => crate::core::diagnostic_params! {"cause"=>error.to_string()}
                    ,
                Self::Exhausted(scope) => {
                    crate::core::diagnostic_params! {"scope"=>match scope {
                        IdentityScope::Document=>"document",
                        IdentityScope::ExternalEntry=>"external_e\
                        ntry"}}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn random_failure_does_not_fall_back_to_clock_or_counter() {
        let error = super::generate_uuid(|_| Err(getrandom::Error::UNSUPPORTED)).unwrap_err();
        assert_eq!(error.outcome(), crate::outcome::Outcome::Identity);
        assert_eq!(
            crate::core::diagnostic::Code::code(&error),
            "identity.entropy"
        );
        assert!(std::error::Error::source(&error).is_some());
    }
}
