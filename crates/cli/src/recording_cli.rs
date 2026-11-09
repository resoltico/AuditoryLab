//! Recording measurements for `auditorylab analyze`.
use super::{CliError, library_cli::ReadFormat, library_cli::print_json, output, prose};
use auditorylab_presentation::report::Report;
use std::path::Path;

pub(super) fn analyze(
    path: &Path,
    format: ReadFormat,
    seconds: Option<u32>,
    start: u32,
) -> Result<(), CliError> {
    let cancel = auditorylab_app::job::Cancel::default();
    let request = cancel.clone();
    super::on_interrupt(move || request.request())?;
    let progress = auditorylab_app::job::Progress::default();
    let recording = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            auditorylab_app::services::analyze_recording(
                path,
                auditorylab_app::recording::AnalysisOptions {
                    start_seconds: start,
                    seconds,
                },
                &cancel,
                &progress,
            )
        });
        let started = std::time::Instant::now();
        let mut last = None;
        let mut last_phase = None;
        while !worker.is_finished() {
            let phase = progress.phase();
            if last_phase != Some(phase) {
                use std::io::Write;
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "{}",
                    auditorylab_presentation::recording::progress_phase(phase)
                );
                last_phase = Some(phase);
            }
            let percent = progress
                .frames()
                .saturating_mul(100)
                .checked_div(progress.total())
                .map(|p| p.min(100));
            if percent != last && started.elapsed().as_millis() >= 250 {
                if let Some(percent) = percent {
                    eprintln!("Analyzing: {percent}%");
                }
                last = percent;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        worker
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })?;
    if format == ReadFormat::Json {
        print_json(&Report::analyzed_recording(&recording))?;
    } else {
        output(
            format!(
                "{}\n",
                prose(&auditorylab_presentation::recording::analysis_description(
                    &recording
                ))
            )
            .as_bytes(),
        )?;
    }
    Ok(())
}

/// Command grammar delegates to the same strict field interpretation as the desktop.
pub(super) fn parse_duration(text: &str) -> Result<u32, String> {
    use auditorylab_presentation::Explain;
    auditorylab_app::recording::AnalysisOptions::from_text("", text)
        .map(|options| options.seconds.unwrap_or(600))
        .map_err(|error| error.explain())
}
pub(super) fn parse_start(text: &str) -> Result<u32, String> {
    use auditorylab_presentation::Explain;
    auditorylab_app::recording::AnalysisOptions::from_text(text, "")
        .map(|options| options.start_seconds)
        .map_err(|error| error.explain())
}
