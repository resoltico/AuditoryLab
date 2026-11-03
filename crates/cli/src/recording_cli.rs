//! Recording identification for `auditorylab analyze`.
use super::{CliError, library_cli::ReadFormat, library_cli::print_json, output, prose};
use auditorylab_presentation::report::Report;
use std::path::Path;

pub(super) fn analyze(path: &Path, format: ReadFormat) -> Result<(), CliError> {
    let recording = auditorylab_app::services::open_recording(path)?;
    if format == ReadFormat::Json {
        print_json(&Report::recording(&recording))?;
    } else {
        output(
            format!(
                "{}\n",
                prose(&auditorylab_presentation::recording::recording_description(
                    &recording
                ))
            )
            .as_bytes(),
        )?;
    }
    Ok(())
}
