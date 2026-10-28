//! Product identity wording derived from application metadata.
use auditorylab_app::identity::BuildIdentity;

/// About-window title derived from the canonical product name.
#[must_use]
pub fn about_title() -> String {
    format!("About {}", auditorylab_app::identity::BUILD_IDENTITY.name)
}

/// Product version, native build counter and optional short source revision.

pub fn build_identity_version_text(value: BuildIdentity) -> String {
    let source = value
        .revision
        .map_or(String::new(), |revision| format!(" · {}", &revision[..12]));
    format!("{} (build {}){source}", value.version, value.build)
}
/// A binary name followed by its canonical build identity.

pub fn build_identity_version_line(value: BuildIdentity, binary: &str) -> String {
    format!(
        "{binary} {}",
        crate::build_info::build_identity_version_text(value)
    )
}
/// Version and native build counter shown in the graphical About window.

pub fn build_identity_about_version(value: BuildIdentity) -> String {
    format!("Version {} (Build {})", value.version, value.build)
}
