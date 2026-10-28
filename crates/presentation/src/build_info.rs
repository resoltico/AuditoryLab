//! Product identity wording derived from application metadata.
use auditorylab_app::identity::BuildIdentity;

/// About-window title derived from the canonical product name.
#[must_use]
pub fn about_title() -> String {
    format!("About {}", auditorylab_app::identity::BUILD_IDENTITY.name)
}

/// Product version with the full source revision when supplied.
#[must_use]
pub fn build_identity_version_text(value: BuildIdentity) -> String {
    value.revision.map_or_else(
        || value.version.into(),
        |revision| format!("{} ({revision})", value.version),
    )
}
/// A binary name followed by its canonical source identity.
#[must_use]
pub fn build_identity_version_line(value: BuildIdentity, binary: &str) -> String {
    format!("{binary} {}", build_identity_version_text(value))
}
/// Version and optional short source revision in the graphical About window.
#[must_use]
pub fn build_identity_about_version(value: BuildIdentity) -> String {
    value.revision.map_or_else(
        || format!("Version {}", value.version),
        |revision| format!("Version {} ({})", value.version, &revision[..12]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_display_retains_full_source_and_about_uses_twelve_characters() {
        let identity = BuildIdentity {
            name: "Auditory Lab",
            copyright: "Copyright",
            version: "0.7.0",
            revision: None,
        };
        assert_eq!(build_identity_version_text(identity), "0.7.0");
        assert_eq!(build_identity_about_version(identity), "Version 0.7.0");
        let identity = BuildIdentity {
            revision: Some("0123456789abcdef0123456789abcdef01234567"),
            ..identity
        };
        assert_eq!(
            build_identity_version_line(identity, "auditorylab"),
            "auditorylab 0.7.0 (0123456789abcdef0123456789abcdef01234567)"
        );
        assert_eq!(
            build_identity_about_version(identity),
            "Version 0.7.0 (0123456789ab)"
        );
    }
}
