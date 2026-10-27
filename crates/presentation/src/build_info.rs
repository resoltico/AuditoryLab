//! Product identity wording derived from application metadata.
use auditorylab_app::identity::BuildIdentity;

/// About-window title derived from the canonical product name.
#[must_use]
pub fn about_title() -> String {
    format!("About {}", auditorylab_app::identity::BUILD_IDENTITY.name)
}

/// Human wording for BuildIdentity values.
pub trait BuildIdentityText {
    /// Product version, native build counter and optional short source revision.
    #[must_use]
    fn version_text(self) -> String;

    /// A binary name followed by its canonical build identity.
    #[must_use]
    fn version_line(self, binary: &str) -> String;

    /// Version and native build counter shown in the graphical About window.
    #[must_use]
    fn about_version(self) -> String;
}
impl BuildIdentityText for BuildIdentity {
    /// Product version, native build counter and optional short source revision.
    fn version_text(self) -> String {
        let source = self
            .revision
            .map_or(String::new(), |revision| format!(" · {}", &revision[..12]));
        format!("{} (build {}){source}", self.version, self.build)
    }

    /// A binary name followed by its canonical build identity.
    fn version_line(self, binary: &str) -> String {
        format!("{binary} {}", self.version_text())
    }

    /// Version and native build counter shown in the graphical About window.
    fn about_version(self) -> String {
        format!("Version {} (Build {})", self.version, self.build)
    }
}
