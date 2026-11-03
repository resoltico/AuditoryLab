//! An opened recording's identity, as text.
use auditorylab_app::recording::RecordingFile;

/// What recording analysis reports about the audio itself.
pub const MEASUREMENTS: &str = "Auditory Lab identifies the format from the file's first bytes. \
    It does not decode or measure the audio.";

/// The file name, or the whole path when it has none.
#[must_use]
pub fn recording_name(recording: &RecordingFile) -> String {
    recording.path.file_name().map_or_else(
        || crate::display_scalar(&recording.path.display().to_string()),
        |name| crate::display_scalar(&name.to_string_lossy()),
    )
}

/// Size in decimal units with the exact byte count.
#[must_use]
pub fn recording_size(recording: &RecordingFile) -> String {
    if recording.bytes < 1_000 {
        crate::numbers::file_size(recording.bytes)
    } else {
        format!(
            "{} ({} bytes)",
            crate::numbers::file_size(recording.bytes),
            recording.bytes
        )
    }
}

/// Labeled lines describing one opened recording.
#[must_use]
pub fn recording_description(recording: &RecordingFile) -> String {
    format!(
        "Recording: {}\nFile: {}\nSize: {}\nFormat: {}\nMeasurements: {MEASUREMENTS}",
        recording_name(recording),
        crate::display_scalar(&recording.path.display().to_string()),
        recording_size(recording),
        recording.format.label(),
    )
}
