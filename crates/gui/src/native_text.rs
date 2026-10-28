//! Runtime wording and product identity for native desktop surfaces.
use crate::terms::Caption;

impl Caption {
    /// Title of a recipe details window.
    #[must_use]
    pub(crate) fn sound_details(title: &str) -> String {
        format!("Sound details · {title}")
    }
    /// Caption of a typography diagnostic.
    #[must_use]
    pub(crate) fn typography(family: &str, size: f32, source: &str) -> String {
        format!(
            "Typography: {}",
            Self::typography_values(family, size, source)
        )
    }
}
impl Caption {
    /// Prefix used for a graphical application error dialog.
    #[must_use]
    #[cfg(target_os = "windows")]
    pub(crate) fn application_error_title() -> String {
        format!("{} error", auditorylab_app::identity::BUILD_IDENTITY.name)
    }
    /// A monitor's native typography values, without duplicating the diagnostic heading.
    #[must_use]
    pub(crate) fn typography_values(family: &str, size: f32, source: &str) -> String {
        format!("{family} · {size} logical px · {source}")
    }
    /// Explain an incomplete desktop package without depending on the process cwd.
    #[must_use]
    pub(crate) fn missing_notices(path: &std::path::Path) -> String {
        format!(
            "Bundled notices are missing or empty: {}. Obtain a complete desktop download from {}",
            path.display(),
            auditorylab_app::identity::SOURCE_URL
        )
    }
}
impl Caption {
    /// Default portable filename for a file panel whose destination is not yet chosen.
    #[must_use]
    pub(crate) fn suggested_filename(extension: &str) -> String {
        format!("sound.{extension}")
    }
}
/// The interface operation that failed; backend error detail remains attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InterfaceOperation {
    /// Show the About window.
    About,
    /// Show bundled licenses.
    Licenses,
    /// Complete a native Quit request.
    Quit,
    /// Shut down application jobs.
    Shutdown,
    /// Show a details window.
    Details,
    /// Open a web or installed notices link.
    OpenLink,
    /// Show the output monitor.
    ShowMonitor,
    /// Hide the output monitor.
    HideMonitor,
    /// Present audio controls beside a native dialog.
    AudioControls,
    /// Continue the graphical application after an error.
    #[cfg(target_os = "windows")]
    Application,
}
impl InterfaceOperation {
    /// Error explanation shared by native windows and terminal renderers.
    #[must_use]
    pub(crate) fn failure(self, error: impl std::fmt::Display) -> String {
        match self {
            Self::About => format!("About: {error}"),
            Self::Licenses => format!("Licenses: {error}"),
            Self::Quit => format!("Quit: {error}"),
            Self::Shutdown => format!("Shutdown: {error}"),
            Self::Details => format!("Details: {error}"),
            Self::OpenLink => format!("Cannot open link: {error}"),
            Self::ShowMonitor => format!("Cannot show Output monitor: {error}"),
            Self::HideMonitor => format!("Cannot hide Output monitor: {error}"),
            Self::AudioControls => format!("Cannot present Audio controls: {error}"),
            #[cfg(target_os = "windows")]
            Self::Application => format!(
                "{} could not continue.\n\n{error}",
                auditorylab_app::identity::BUILD_IDENTITY.name
            ),
        }
    }
}

pub(crate) enum CommandProblem {
    DesktopArguments,
}
impl CommandProblem {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::DesktopArguments => "Use auditorylab-gui without arguments or with --version.",
        }
    }
}

/// Populate the product metadata before a native window is shown.
pub(crate) fn initialize_identity(terms: &crate::generated::Terms<'_>) {
    let identity = &auditorylab_app::identity::BUILD_IDENTITY;
    terms.set_caption_product_name(identity.name.into());
    terms.set_caption_copyright(identity.copyright.into());
    terms.set_caption_about_product(auditorylab_presentation::build_info::about_title().into());
}
