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
#[cfg(test)]
#[path = "../../identity_input.rs"]
mod input;

impl BuildIdentity {
    /// Version and native build counter shown in the graphical About window.
    #[must_use]
    pub fn about_version(self) -> String {
        format!("Version {} (Build {})", self.version, self.build)
    }
}
