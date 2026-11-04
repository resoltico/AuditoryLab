//! An opened recording's identity, as text.
use auditorylab_app::recording::{EnvelopeBand, RecordingFile};

const PULSING_ATTRIBUTION_LIMIT: &str =
    "Beating/modulation labels describe spectral patterns, not creator intent.";

/// What recording analysis reports about the audio itself.
pub const MEASUREMENTS: &str =
    "Choose Analyze to measure this recording. No samples have been measured yet.";

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
pub fn findings(a: &auditorylab_app::recording::Analysis, lowest: Option<f64>) -> Findings {
    let facts = format!(
        "{} · {} Hz · {} channel(s) · {:.1} s\n\
        Analyzed: {:.1}–{:.1} s · resolution {:.3} Hz · envelope {:.3} Hz\n\
        Envelope window: {:.1}–{:.1} s",
        a.codec,
        a.sample_rate,
        a.channels,
        a.duration_seconds,
        a.window.start_seconds,
        a.window.end_seconds,
        a.resolution_hz,
        a.envelope_resolution_hz,
        a.envelope_window.start_seconds,
        a.envelope_window.end_seconds,
    );
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
    summary.push(ears.clone());
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
fn ear_findings(a: &auditorylab_app::recording::Analysis) -> String {
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
fn pulsing_summary(a: &auditorylab_app::recording::Analysis) -> Vec<String> {
    use auditorylab_app::recording::PulsingCause;
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
fn pulsing_findings(a: &auditorylab_app::recording::Analysis) -> String {
    use auditorylab_app::recording::{EnvelopeBand, PulsingCause};
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
fn low_tone_findings(a: &auditorylab_app::recording::Analysis, lowest: Option<f64>) -> String {
    let mut lines = a
        .low_energy
        .iter()
        .filter(|share| [50.0, 60.0, 100.0, 300.0].contains(&share.below_hz))
        .map(|e| {
            format!(
                "{} · below {:.0} Hz: {}",
                channel_label(e.channel, a.channels),
                e.below_hz,
                percentage(e.fraction)
            )
        })
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
fn analysis_limits(a: &auditorylab_app::recording::Analysis) -> String {
    format!(
        "Window {:.1}–{:.1} s; tone resolution {:.3} Hz; envelope resolution {:.3} Hz. \
        Envelope rates below {minimum:.2} Hz are unresolved. {}. Digital samples only; \
        device output and hearing are not measured. \
        Quiet pairs \
        under other sounds may be missed. {PULSING_ATTRIBUTION_LIMIT} \
        Symmetric sidebands can also be synthesized as separate tones. \
        Peaks below −80 dB relative to the strongest are omitted. \
        Envelope peaks below {contrast:.0} dB of squared mean amplitude are omitted. \
        The analytic envelope excludes DC and FFT window edges. \
        Only the selected window is measured; later corruption may not be found. \
        This describes the sound, not measured brain activity.",
        a.window.start_seconds,
        a.window.end_seconds,
        a.resolution_hz,
        a.envelope_resolution_hz,
        if a.lossy {
            "Lossy codec: compression can change measured details"
        } else {
            "Lossless decoded samples"
        },
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
    let f = findings(&recording.analysis, None);
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
