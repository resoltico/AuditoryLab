//! Product, native build counter and optional source identity.
/// Identity shared by the command-line and desktop products.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildIdentity {
    /// Product name from the root license metadata.
    pub name: &'static str,
    /// Copyright line from the root license metadata.
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
/// About title derived from the canonical product name.
pub const ABOUT_TITLE: &str = generated::ABOUT_TITLE;
/// Canonical repository URL inherited from workspace package metadata.
pub const SOURCE_URL: &str = env!("CARGO_PKG_REPOSITORY");
/// Identity generated from workspace metadata.
pub const BUILD_IDENTITY: BuildIdentity = generated::BUILD;
impl BuildIdentity {
    /// Product version, native build counter and optional short source revision.
    #[must_use]
    pub fn version_text(self) -> String {
        let source = self
            .revision
            .map_or(String::new(), |revision| format!(" · {}", &revision[..12]));
        format!("{} (build {}){source}", self.version, self.build)
    }
    /// A binary name followed by its canonical build identity.
    #[must_use]
    pub fn version_line(self, binary: &str) -> String {
        format!("{binary} {}", self.version_text())
    }
}
/// Allocate a fallible random portable identity; never substitute clocks or counters.
pub(crate) fn fresh_document() -> Result<auditorylab_core::DocumentIdentity, crate::error::Error> {
    Ok(auditorylab_core::DocumentIdentity {
        id: random_uuid()?,
        created_at: None,
        modified_at: None,
    })
}

pub(crate) fn random_uuid() -> Result<auditorylab_core::DocumentUuid, crate::error::Error> {
    generate_uuid(getrandom::fill)
}
fn generate_uuid(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<auditorylab_core::DocumentUuid, crate::error::Error> {
    let mut bytes = [0_u8; 16];
    fill(&mut bytes).map_err(|e| {
        crate::error::Error::caused(
            crate::error::ErrorKind::Identity,
            format!("cannot allocate a random UUID: {e}"),
            e,
        )
    })?;
    Ok(auditorylab_core::DocumentUuid::from_random_bytes(bytes))
}
#[cfg(test)]
#[path = "../identity_input.rs"]
mod input;

impl BuildIdentity {
    /// Version and native build counter shown in the graphical About window.
    #[must_use]
    pub fn about_version(self) -> String {
        format!("Version {} (Build {})", self.version, self.build)
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn random_failure_does_not_fall_back_to_clock_or_counter() {
        let error = super::generate_uuid(|_| Err(getrandom::Error::UNSUPPORTED)).unwrap_err();
        assert_eq!(error.kind(), crate::error::ErrorKind::Identity);
        assert!(error.to_string().contains("random UUID"));
        assert!(std::error::Error::source(&error).is_some());
    }
}
