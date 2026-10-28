//! Completed workspace/publication descriptions.
use auditorylab_app::services::Exported;
use std::path::Path;
/// Confirm completed export of the exact captured recipe text.
#[must_use]
pub fn recipe_exported(path: &Path) -> String {
    format!(
        "Exported recipe to {}.",
        crate::display_scalar(&(path.display().to_string()))
    )
}

/// A storage stage retained for diagnosis.
#[must_use]
pub fn retained_stage(stage: &str) -> String {
    format!("Retained stage: {}\n", crate::display_scalar(stage))
}

/// A selectable snapshot of the information shown by the desktop diagnostics reader.
#[derive(Debug)]
pub struct DesktopDiagnostics<'a> {
    /// Captured transport identity and phase.
    pub player: &'a str,
    /// Captured generated voice details.
    pub generated: &'a str,
    /// Captured monitor provenance.
    pub monitor: &'a str,
    /// Actual output device information.
    pub device: &'a str,
    /// Current task/outcome description.
    pub status: &'a str,
    /// Failure to present an auxiliary window, if any.
    pub auxiliary_error: &'a str,
    /// Native motion preference explanation.
    pub motion: &'a str,
    /// Native typography origin and size.
    pub typography: &'a str,
    /// Static model provenance.
    pub model: &'a str,
}

pub fn desktop_diagnostics_text(value: &DesktopDiagnostics<'_>) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\nTypography: {}\n{}",
        value.player,
        value.generated,
        value.monitor,
        value.device,
        value.status,
        value.auxiliary_error,
        value.motion,
        value.typography,
        value.model
    )
}

/// Explain the actual completed WAV container and frame count.
pub fn exported_description(value: &Exported, path: &Path) -> String {
    format!(
        "Exported {} frames to {} ({}).",
        value.frames,
        crate::display_scalar(&(path.display().to_string())),
        value.container
    )
}
