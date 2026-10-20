//! Product, native build counter and optional source identity.
/// Identity shared by the command-line and desktop products.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildIdentity {
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
/// Product version, native build counter and optional short source revision.
pub fn version_line(binary: &str) -> String {
    let id = BUILD_IDENTITY;
    let source = id
        .revision
        .map_or(String::new(), |revision| format!(" · {}", &revision[..12]));
    format!("{binary} {} (build {}){source}", id.version, id.build)
}
#[cfg(test)]
#[path = "../identity_input.rs"]
mod input;
