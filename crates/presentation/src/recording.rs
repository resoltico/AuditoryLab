//! An opened recording's identity, as text.
use auditorylab_app::recording::RecordingFile;
use auditorylab_core::measurement::EnvelopeBand;

const PULSING_ATTRIBUTION_LIMIT: &str =
    "Beating/modulation labels describe spectral patterns, not creator intent.";

/// What recording analysis reports about the audio itself.
pub const MEASUREMENTS: &str =
    "Choose Analyze recording to measure this file. No samples have been measured yet.";

/// Current recording guidance from captured source/report/activity and actual actions.
/// This renders existing state; it neither admits an action nor stores another state machine.
pub fn measurement_guidance(
    source: Option<&RecordingFile>,
    report: Option<&auditorylab_app::recording::AnalyzedRecording>,
    phase: Option<auditorylab_app::job::ProgressPhase>,
    actions: &auditorylab_app::workspace::WorkspaceActions,
) -> String {
    use auditorylab_app::services::OperationError;
    use auditorylab_app::vocabulary::Operation;
    use auditorylab_app::workspace::status::{FileAction, FileStatus};
    let enabled = |operation| {
        actions
            .operations
            .iter()
            .any(|action| action.operation == operation && action.enabled)
    };
    if phase.is_some() {
        let retained = if report.is_some() {
            "The previous completed results below are retained while this run is in progress."
        } else {
            "No completed analysis is available yet."
        };
        return format!("Analysis in progress. {retained} Cancel analysis requests cancellation.");
    }
    if matches!(&actions.file_status, FileStatus::Running(activity) if matches!(activity.kind, FileAction::OpenRecording))
    {
        return if report.is_some() {
            "Opening a recording. The previous completed results remain until another source opens."
                .into()
        } else {
            "Opening a recording. No samples have been measured yet.".into()
        };
    }
    if source.is_none() {
        return if enabled(Operation::OpenRecording) {
            "No recording is open. Choose Open recording to select an audio file.".into()
        } else {
            "No recording is open. Opening is unavailable while the current dialog or operation is active.".into()
        };
    }
    let failed = matches!(
        &actions.file_status,
        FileStatus::Failed(OperationError::Recording(_))
            | FileStatus::WorkerFailed {
                activity: auditorylab_app::workspace::status::FileActivity {
                    kind: FileAction::AnalyzeRecording | FileAction::OpenRecording,
                    ..
                },
                ..
            }
    );
    if report.is_some() {
        return if matches!(
            &actions.file_status,
            FileStatus::Failed(OperationError::Recording(
                auditorylab_app::recording::RecordingError::Cancelled
            ))
        ) {
            "Analysis cancelled. The last completed results below are retained.".into()
        } else if failed {
            "The last recording operation failed. The last completed results below are retained."
                .into()
        } else {
            String::new()
        };
    }
    if enabled(Operation::AnalyzeRecording) {
        MEASUREMENTS.into()
    } else {
        "Recording identified. Analysis is unavailable while the current dialog or operation is active; no samples have been measured yet.".into()
    }
}

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

/// Shared human-readable recording findings; every section remains visible in the desktop.
#[derive(Debug, Clone, PartialEq)]
pub struct Findings {
    pub facts: String,
    pub summary: String,
    pub tones: String,
    pub ears: String,
    pub pulsing: String,
    pub low_tones: String,
    pub stereo: String,
    pub limits: String,
}
/// Render findings from samples alone. Device-check answers add context, never change measurements.
pub fn findings(
    a: &auditorylab_core::measurement::SampleAnalysis,
    lowest: Option<f64>,
) -> Findings {
    numerical_findings(a, lowest, measurement_facts(a, None))
}

fn measurement_facts(
    samples: &auditorylab_core::measurement::SampleAnalysis,
    recording: Option<&auditorylab_app::recording::Analysis>,
) -> String {
    let (window, envelope, source) = match recording {
        Some(recording) => (
            &recording.window,
            &recording.envelope_window,
            format!(
                "{} · {}",
                recording.codec,
                recording.duration_seconds.map_or_else(
                    || "source duration unknown".into(),
                    |seconds| format!("source duration {seconds:.1} s"),
                )
            ),
        ),
        None => (
            &samples.window,
            &samples.envelope_window,
            "Sample-relative coordinates".into(),
        ),
    };
    format!(
        "{source}\n{} Hz · {} channel(s)\nAnalyzed: {:.1}–{:.1} s\n\
        Envelope window: {:.1}–{:.1} s\nTone resolution: {:.3} Hz · envelope resolution: {:.3} Hz",
        samples.sample_rate,
        samples.channels,
        window.start_seconds,
        window.end_seconds,
        envelope.start_seconds,
        envelope.end_seconds,
        samples.resolution_hz,
        samples.envelope_resolution_hz,
    )
}

fn numerical_findings(
    a: &auditorylab_core::measurement::SampleAnalysis,
    lowest: Option<f64>,
    facts: String,
) -> Findings {
    let ears = ear_findings(a);
    let mut summary = Vec::new();
    if let Some(tone) = a.tones.first() {
        summary.push(format!(
            "The strongest detected tone in {} is {:.2} Hz.",
            channel_label(tone.channel, a.channels),
            tone.hz
        ));
    } else {
        summary.push("No tones met the detection thresholds.".into());
    }
    summary.push(ear_overview(a));
    if a.minimum_resolved_envelope_hz > 0.5 {
        summary.push(format!(
            "Envelope rates below {:.2} Hz are unresolved in this analysis; \
            use a longer window when possible. No rates in that region are classified.",
            a.minimum_resolved_envelope_hz
        ));
    }
    summary.extend(pulsing_summary(a));
    if let Some(share) = a
        .low_energy
        .iter()
        .find(|share| share.channel == 0 && share.below_hz == 50.0)
    {
        summary.push(format!(
            "{} of the energy in {} is below 50 Hz.",
            percentage(share.fraction),
            channel_label(0, a.channels)
        ));
    }
    let tones = if a.tones.is_empty() {
        "No detected tones.".into()
    } else {
        a.tones
            .iter()
            .map(|t| {
                format!(
                    "{} · {:.2} Hz · {:.1} dBFS RMS · {:+.1} dB relative",
                    channel_label(t.channel, a.channels),
                    t.hz,
                    t.dbfs,
                    t.relative_db,
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let stereo = (0..a.channels)
        .map(|channel| {
            format!(
                "{} · peak {} dBFS · RMS {} dBFS",
                channel_label(channel, a.channels),
                optional_number(a.stereo.peak_dbfs[channel]),
                optional_number(a.stereo.rms_dbfs[channel]),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        + &format!(
            "\nChannel correlation: {} · side/mid: {} dB",
            optional_number(a.stereo.correlation),
            optional_number(a.stereo.side_to_mid_db),
        );
    Findings {
        facts,
        summary: summary.join("\n"),
        tones,
        ears,
        pulsing: pulsing_findings(a),
        low_tones: low_tone_findings(a, lowest),
        stereo,
        limits: analysis_limits(a),
    }
}
/// Render a completed recording with source facts and absolute selected placement.
pub fn recording_findings(
    a: &auditorylab_app::recording::Analysis,
    lowest: Option<f64>,
) -> Findings {
    let mut rendered =
        numerical_findings(a.samples(), lowest, measurement_facts(a.samples(), Some(a)));
    rendered.limits.push_str(if a.lossy {
        "\nLossy codec: compression can change measured details."
    } else {
        "\nLossless decoded samples."
    });
    rendered
}
/// Render retained samples under current app-owned device interpretation.
pub fn contextual_findings(
    a: &auditorylab_app::recording::Analysis,
    context: auditorylab_app::recording::InterpretationContext,
) -> Findings {
    let mut rendered = recording_findings(a, context.lowest_heard_hz());
    let assessment = context.assessment();
    rendered.ears.push('\n');
    rendered
        .ears
        .push_str(&crate::device_check::channel_result(assessment.channels));
    rendered.low_tones.push('\n');
    rendered
        .low_tones
        .push_str(&crate::device_check::low_tone_result(assessment.low_tones));
    rendered
}
fn ear_overview(a: &auditorylab_core::measurement::SampleFacts) -> String {
    if a.channels == 1 || a.coincident_tones || a.ear_differences.is_empty() {
        ear_findings(a)
    } else {
        let count = a.ear_differences.len();
        let noun = if count == 1 {
            "comparison candidate"
        } else {
            "comparison candidates"
        };
        format!(
            "{count} left-versus-right tone {noun} met the detection thresholds. \
            See Left versus right for the complete list."
        )
    }
}

fn ear_findings(a: &auditorylab_core::measurement::SampleFacts) -> String {
    if a.channels == 1 {
        "Mono recording; no left/right comparison is available.".into()
    } else if a.coincident_tones {
        format!(
            "No left/right tone difference found among detected peaks within \
            {:.3} Hz (two bins).",
            2.0 * a.resolution_hz
        )
    } else if a.ear_differences.is_empty() {
        "No channel-exclusive tone pair met the binaural-difference thresholds. \
            This does not establish identical channels."
            .into()
    } else {
        a.ear_differences
            .iter()
            .map(|p| {
                format!(
                    "Left {:.2} Hz · right {:.2} Hz · difference {:.2} Hz ({})",
                    p.left_hz,
                    p.right_hz,
                    p.difference_hz,
                    band_label(p.band),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
fn pulsing_summary(a: &auditorylab_core::measurement::SampleAnalysis) -> Vec<String> {
    use auditorylab_core::measurement::PulsingCause;
    let mut sentences = Vec::new();
    for channel in 0..a.channels {
        let whole: Vec<_> = a
            .pulsing
            .iter()
            .filter(|p| p.channel == channel && p.envelope_band == EnvelopeBand::Whole)
            .collect();
        for band in EnvelopeBand::ALL {
            let mut beating = Vec::new();
            let mut harmonics = Vec::new();
            let mut modulation = Vec::new();
            for p in a
                .pulsing
                .iter()
                .filter(|p| p.channel == channel && p.envelope_band == band)
            {
                // Keep distinct attributions at the same rate. Filtered bands add
                // information when the whole-band finding does not already cover it.
                if band != EnvelopeBand::Whole
                    && whole.iter().any(|w| {
                        w.cause == p.cause && (w.hz - p.hz).abs() <= a.envelope_resolution_hz
                    })
                {
                    continue;
                }
                let rate = format!("{:.2}", p.hz);
                match p.cause {
                    PulsingCause::Beating { harmonic: 1, .. } => beating.push(rate),
                    PulsingCause::Beating { .. } => harmonics.push(rate),
                    PulsingCause::Modulation => modulation.push(rate),
                }
            }
            let scope = match band {
                EnvelopeBand::Whole => "the whole signal",
                EnvelopeBand::Below100 => "the signal below 100 Hz",
                EnvelopeBand::Above100 => "the signal above 100 Hz",
            };
            for (rates, meaning) in [
                (
                    beating,
                    "these rates match pitch differences between detected tones (beating)",
                ),
                (
                    harmonics,
                    "these rates match multiples of pitch differences (beating harmonics)",
                ),
                (modulation, "this is a modulation pattern"),
            ] {
                if !rates.is_empty() {
                    sentences.push(format!(
                        "In {}, {scope} has loudness variation at {} Hz; {meaning}.",
                        channel_label(channel, a.channels),
                        rates.join(", ")
                    ));
                }
            }
        }
    }
    if !sentences.is_empty() {
        sentences.push(PULSING_ATTRIBUTION_LIMIT.into());
    }
    sentences
}
fn pulsing_findings(a: &auditorylab_core::measurement::SampleAnalysis) -> String {
    use auditorylab_core::measurement::{EnvelopeBand, PulsingCause};
    if a.pulsing.is_empty() {
        return "No envelope rates met the detection thresholds.".into();
    }
    a.pulsing
        .iter()
        .map(|p| {
            let cause = match p.cause {
                PulsingCause::Beating {
                    first_hz,
                    second_hz,
                    harmonic,
                } => {
                    if harmonic == 1 {
                        format!("beating consistent with {first_hz:.2} and {second_hz:.2} Hz")
                    } else {
                        format!(
                            "harmonic {harmonic} of beating consistent with \
                    {first_hz:.2} and {second_hz:.2} Hz"
                        )
                    }
                }
                PulsingCause::Modulation => "modulation pattern".into(),
            };
            let range = match p.envelope_band {
                EnvelopeBand::Whole => "whole band",
                EnvelopeBand::Below100 => "below 100 Hz",
                EnvelopeBand::Above100 => "above 100 Hz",
            };
            format!(
                "{} · {} · {:.2} Hz ({}) · {}",
                channel_label(p.channel, a.channels),
                range,
                p.hz,
                band_label(p.band),
                cause
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn low_tone_findings(
    a: &auditorylab_core::measurement::SampleAnalysis,
    lowest: Option<f64>,
) -> String {
    let mut lines = energy_rows(a)
        .map(|share| energy_row(share, a.channels))
        .collect::<Vec<_>>();
    if let Some(hz) = lowest {
        for share in a.low_energy.iter().filter(|share| share.below_hz == hz) {
            lines.push(format!(
                "{} of this recording's energy in {} is below \
                the lowest tone you reported hearing ({hz:.0} Hz).",
                percentage(share.fraction),
                channel_label(share.channel, a.channels)
            ));
        }
    }
    lines.join("\n")
}
fn analysis_limits(a: &auditorylab_core::measurement::SampleAnalysis) -> String {
    format!(
        "Envelope rates below {minimum:.2} Hz are unresolved. Digital samples only; \
        device output and hearing are not measured. \
        Quiet pairs \
        under other sounds may be missed. {PULSING_ATTRIBUTION_LIMIT} \
        Symmetric sidebands can also be synthesized as separate tones. \
        Peaks below −80 dB relative to the strongest are omitted. \
        Envelope peaks below {contrast:.0} dB of squared mean amplitude are omitted. \
        The analytic envelope excludes DC and FFT window edges. \
        Only the selected window is measured; later corruption may not be found. \
        This describes the sound, not measured brain activity.",
        minimum = a.minimum_resolved_envelope_hz,
        contrast = 10.0 * a.minimum_envelope_relative_power.log10(),
    )
}
fn channel_label(channel: usize, channels: usize) -> &'static str {
    if channels == 1 {
        "Mono"
    } else if channel == 0 {
        "Left"
    } else {
        "Right"
    }
}
fn band_label(band: Option<auditorylab_core::rhythm::Band>) -> &'static str {
    band.map_or("outside IFCN bands", auditorylab_core::rhythm::Band::label)
}
fn percentage(value: Option<f64>) -> String {
    value.map_or_else(
        || "undefined (silence)".into(),
        |v| format!("{:.1}%", v * 100.0),
    )
}
fn optional_number(value: Option<f64>) -> String {
    value.map_or_else(|| "undefined".into(), |v| format!("{v:.2}"))
}
/// Complete terminal report, using the same sections as the desktop.
pub fn analysis_description(recording: &auditorylab_app::recording::AnalyzedRecording) -> String {
    let f = recording_findings(&recording.analysis, None);
    format!(
        "Recording: {}\nFile: {}\nFormat: {} · {}\n\nSummary\n{}\n\nTones\n{}\n\n\
        Left versus right\n{}\n\nPulsing\n{}\n\nLow tones\n{}\n\n\
        Stereo and level\n{}\n\nLimits\n{}",
        recording_name(&recording.file),
        crate::display_scalar(&recording.file.path.display().to_string()),
        recording.file.format.label(),
        f.facts,
        f.summary,
        f.tones,
        f.ears,
        f.pulsing,
        f.low_tones,
        f.stereo,
        f.limits
    )
}

/// Common energy rows, in channel order and then ascending display threshold.
/// Device-specific hearing thresholds remain contextual findings, not extra bars.
pub fn energy_rows(
    samples: &auditorylab_core::measurement::SampleAnalysis,
) -> impl Iterator<Item = &auditorylab_core::measurement::EnergyShare> {
    (0..samples.channels).flat_map(move |channel| {
        [50.0, 60.0, 100.0, 300.0]
            .into_iter()
            .filter_map(move |below_hz| {
                samples
                    .low_energy
                    .iter()
                    .find(|share| share.channel == channel && share.below_hz == below_hz)
            })
    })
}

/// Shared channel/threshold label and truthful energy state for prose and native rows.
pub fn energy_row(share: &auditorylab_core::measurement::EnergyShare, channels: usize) -> String {
    let label = format!(
        "{} · below {:.0} Hz",
        channel_label(share.channel, channels),
        share.below_hz
    );
    match share.fraction {
        Some(fraction) => format!("{label}: {:.1}%", fraction * 100.0),
        None => format!("{label}: undefined (digital silence)"),
    }
}

/// Shared stage wording for recording progress; preparing/completing are indeterminate.
pub fn progress_phase(phase: auditorylab_app::job::ProgressPhase) -> &'static str {
    use auditorylab_app::job::ProgressPhase;
    match phase {
        ProgressPhase::Preparing => "Opening recording…",
        ProgressPhase::Skipping => "Reaching selected start…",
        ProgressPhase::Processing => "Measuring selected samples…",
        ProgressPhase::Completing => "Completing numerical findings…",
    }
}

#[cfg(test)]
mod tests;
