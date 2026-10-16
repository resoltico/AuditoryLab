// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! GUI text uses immutable application context, never reconstructed domain policy.
use auditory_app::{
    AppError, DeclaredDescription, Diagnostic, Ears, ExportView, Facts, Notice, Origin,
    PlaybackView,
};
pub(crate) fn origin(value: &Origin) -> String {
    match value {
        Origin::BuiltIn(id) => format!("{id} (built-in)"),
        Origin::File(path) => format!("{path:?}"),
        Origin::Reader(label) => format!("{label:?}"),
    }
}
use crate::signal_geometry::{self, BoundedText, CapacityError};
use std::fmt::Write;

pub(crate) fn signal_range(
    series: &auditory_app::SignalSeries,
    rate: auditory_app::SampleRate,
) -> Result<String, CapacityError> {
    let mut text = BoundedText::new(signal_geometry::RANGE_BYTES)?;
    if let Some(range) = series.range() {
        write!(
            text,
            "Source frames [{}, {}), {} frames at {} Hz; {} missing interval(s).{}",
            range.start_frame(),
            range.end_frame(),
            range.end_frame() - range.start_frame(),
            rate.hz(),
            series.gaps().len(),
            if range.end_frame() - range.start_frame() < 2 {
                " Insufficient samples for a waveform."
            } else {
                ""
            }
        )?;
    } else {
        text.write_str("No observed samples; missing data is not digital silence.")?;
    }
    Ok(text.finish())
}
pub(crate) fn signal_position(
    frame: u64,
    rate: auditory_app::SampleRate,
) -> Result<String, CapacityError> {
    let hz = u64::from(rate.hz());
    let mut text = BoundedText::new(signal_geometry::POSITION_BYTES)?;
    write!(
        text,
        "{}.{:03} s · source frame end {frame} (exclusive), {} Hz",
        frame / hz,
        (frame % hz) * 1000 / hz,
        rate.hz()
    )?;
    Ok(text.finish())
}
pub(crate) fn signal_endpoints(
    series: &auditory_app::SignalSeries,
    rate: auditory_app::SampleRate,
) -> Result<[String; 2], CapacityError> {
    let Some(range) = series.range() else {
        return Ok([String::new(), String::new()]);
    };
    Ok([
        signal_endpoint("Start", range.start_frame(), rate)?,
        signal_endpoint("End excl.", range.end_frame(), rate)?,
    ])
}
fn signal_endpoint(
    label: &str,
    frame: u64,
    rate: auditory_app::SampleRate,
) -> Result<String, CapacityError> {
    let hz = u64::from(rate.hz());
    let mut text = BoundedText::new(signal_geometry::ENDPOINT_BYTES)?;
    write!(
        text,
        "{label} frame {frame}\n≈{}.{:06} s",
        frame / hz,
        (frame % hz) * 1_000_000 / hz
    )?;
    Ok(text.finish())
}
pub(crate) fn signal_context(
    signal: &crate::signal::SignalDisplay,
) -> Result<String, CapacityError> {
    let mut text = BoundedText::new(signal_geometry::CONTEXT_BYTES)?;
    if let Some(error) = &signal.error {
        write!(text, "{error}")?;
        return Ok(text.finish());
    }
    if signal.snapshot.is_none() && !signal.available() && !signal.ended() {
        text.write_str("No live trace. Play starts a new operation at source frame zero.")?;
        return Ok(text.finish());
    }
    if signal.ended() {
        write_terminal_signal_state(
            &mut text,
            signal.progress.as_ref().map(|p| p.latest_generated_frame()),
            signal.snapshot.is_some(),
        )?;
    } else if !signal.eligible() {
        text.write_str("Display suspended while the main window is inactive, minimized or reported occluded; sound continues")?;
    } else if signal.frozen {
        text.write_str("Frozen display — sound continues until the real playback result")?;
    } else if signal.reduced_motion {
        text.write_str(
            "Reduced motion — static snapshot; Refresh display requests new observations",
        )?;
    } else if let Some(progress) = &signal.progress {
        write!(
            text,
            "Observation {:?}; latest generated source cursor {} (separate from chart coverage)",
            progress.freshness(),
            progress.latest_generated_frame()
        )?;
    } else {
        text.write_str("Waiting for actual generated samples")?;
    }
    if let Some(snapshot) = &signal.snapshot {
        text.write_str("\nCaptured source: ")?;
        signal_geometry::write_origin(&mut text, snapshot.description().origin())?;
        write!(
            text,
            " · observation {} · {} Hz. Missing/partial intervals are blank; both ears use fixed ±1 digital amplitude.",
            snapshot.identity().get(),
            snapshot.sample_rate().hz()
        )?;
    }
    Ok(text.finish())
}
fn write_terminal_signal_state(
    text: &mut BoundedText,
    reported: Option<u64>,
    captured: bool,
) -> std::fmt::Result {
    text.write_str(if captured {
        "Terminal snapshot. "
    } else {
        "No samples were observed for this operation. "
    })?;
    match reported {
        Some(frame) => write!(text, "Last reported generated source frame: {frame}. ")?,
        None => text.write_str("Last reported generated source frame: unavailable. ")?,
    }
    if captured {
        text.write_str("Observations are lossy; chart coverage may end earlier. ")?;
    }
    text.write_str("Playback result is reported separately.")
}

pub(crate) fn notice(value: &Notice) -> String {
    match value {
        Notice::LowStartingVolume => {
            "Keep output volume low; digital levels do not limit sound pressure.".into()
        }
        Notice::HeadphonesForSeparateEars => {
            "Use headphones to preserve separate-ear routing.".into()
        }
        Notice::BufferUnderruns { count } => {
            format!("The backend reported {count} buffer underruns; sound may have had gaps.")
        }
    }
}
pub(crate) fn description(value: &DeclaredDescription) -> String {
    let mut text = format!(
        "Source: {}\nDeclared duration: {} seconds\nDeclared fade: {} seconds\nDeclared ear assignments: {}\nValidation: not checked at a sample rate\n",
        origin(value.origin()),
        value.duration_seconds(),
        value.fade_seconds(),
        if value.routes().is_empty() {
            "no components (silence)".into()
        } else {
            value
                .routes()
                .iter()
                .map(|route| match route {
                    Ears::Both => "both ears",
                    Ears::Left => "left ear only",
                    Ears::Right => "right ear only",
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    if matches!(value.origin(), Origin::File(_)) {
        text.push_str(
            "Imported immutable snapshot; edits on disk are read only when explicitly reopened.\n",
        );
    }
    for supplied in value.notices() {
        text.push_str(&format!("For later playback: {}\n", notice(supplied)));
    }
    if let Some(about) = value.about() {
        text.push_str(&format!("\n{}\n", about.title));
        for (label, field) in [
            ("Studied for", &about.studied_for),
            ("Summary", &about.summary),
            ("Citation", &about.citation),
            ("Source link", &about.source_url),
            ("Study", &about.study),
            ("Adaptation", &about.adaptation),
        ] {
            if let Some(field) = field {
                text.push_str(&format!("\n{label}: {field}\n"));
            }
        }
    } else {
        text.push_str("No [about] descriptive metadata provided.\n");
    }
    text
}
// Compact labels cannot inherit the arbitrary line structure of recipe metadata.
// The complete declared title remains available in the research description.
fn inline_title(title: &str) -> String {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        "No title".into()
    } else {
        title
    }
}
pub(crate) fn source_title(value: &DeclaredDescription) -> String {
    value.about().map_or_else(
        || origin(value.origin()),
        |about| inline_title(&about.title),
    )
}
pub(crate) fn catalog_label(value: &DeclaredDescription) -> String {
    format!(
        "{} — {} seconds — {}",
        origin(value.origin()),
        value.duration_seconds(),
        inline_title(value.about().map_or("No title", |a| a.title.as_str()))
    )
}
pub(crate) fn facts(value: &Facts) -> String {
    let declared = value.description();
    format!(
        "Source: {}\nValidation passed at {} Hz\nDeclared/resolved duration: {} / {} seconds\nFrames: {}\nDeclared/resolved fade: {} / {} seconds\nFade frames per edge: {}{}\nComponents:\n{}\nPer-ear pre-conversion peak bounds: {:?} dBFS\nPlayback validates again at its actual stream rate; no physical hardware-rate certification.\n",
        origin(declared.origin()),
        value.sample_rate().hz(),
        declared.duration_seconds(),
        value.duration_seconds(),
        value.frames(),
        declared.fade_seconds(),
        value.fade_seconds(),
        value.fade_frames(),
        if value.fade_frames() == 0 {
            " (fading disabled)"
        } else {
            ""
        },
        declared
            .components()
            .iter()
            .enumerate()
            .map(|(i, c)| format!("  {}: {c}", i + 1))
            .collect::<Vec<_>>()
            .join("\n"),
        value.peak_dbfs()
    )
}
pub(crate) fn export(view: &ExportView) -> String {
    format!(
        "Preparing new-file export\nDestination: {:?}\n{}\nStop requests cooperative cancellation. Successful publication remains complete if a request races afterward.",
        view.destination(),
        facts(view.facts())
    )
}
pub(crate) fn playback(view: &PlaybackView) -> String {
    let device = view.device();
    format!(
        "Starting playback\nSource: {}\nDuration: {} seconds\nDevice: {:?}\nStream rate: {} Hz\nStream channels: {}\nRepresentation: {}\nLeft/right use the first two channels; extra channels are silent.\n{}\nStop requests cancellation; already queued sound cannot be recalled.",
        origin(view.facts().description().origin()),
        view.facts().duration_seconds(),
        device.name.as_deref().unwrap_or("name unavailable"),
        device.sample_rate.hz(),
        device.channels,
        device.representation,
        view.notices()
            .iter()
            .map(notice)
            .collect::<Vec<_>>()
            .join("\n")
    )
}
pub(crate) fn error(error: &AppError) -> String {
    let advice = match error.diagnostic() {
        Diagnostic::Input(auditory_app::InputDiagnostic::NotRegularFile) => {
            "Choose a regular UTF-8 TOML file. Named pipes and devices are not recipe documents; intentional streams use the CLI's --stdin input."
        }
        Diagnostic::Input(_) => {
            "Check the recipe encoding, schema and format; use a built-in recipe as a starting point."
        }
        Diagnostic::Validation(_) => {
            "Correct the identified parameters or choose a supported validation rate."
        }
        Diagnostic::Device(_) => {
            "Check the default output and its permissions/configuration. No automatic retry is promised."
        }
        Diagnostic::Observation(_) => {
            "Signal observation could not be prepared; playback did not start."
        }
        Diagnostic::Filesystem(_) => {
            "Check the requested path, folder and permissions; choose another filename if the destination exists."
        }
        Diagnostic::Cancelled => "The operation observed cooperative cancellation.",
    };
    format!(
        "{advice}\n\nTyped diagnostic: {:?}\n\nTechnical details: {error}\nClassification: {:?}\n",
        error.diagnostic(),
        error.class()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn terminal_signal_state(reported: Option<u64>, captured: bool) -> String {
        let mut text = BoundedText::new(signal_geometry::CONTEXT_BYTES).unwrap();
        write_terminal_signal_state(&mut text, reported, captured).unwrap();
        text.finish()
    }
    #[test]
    fn plot_endpoints_use_the_actual_exclusive_range_and_preserve_tiny_extents() {
        let rate = auditory_app::SampleRate::new(768_000).unwrap();
        let first = auditory_app::analyze_samples(rate, 0, &[[1.0, 0.0]], 1).unwrap();
        assert_eq!(
            signal_endpoints(&first, rate).unwrap(),
            [
                "Start frame 0\n≈0.000000 s",
                "End excl. frame 1\n≈0.000001 s"
            ]
        );
        let series = auditory_app::analyze_samples(rate, 767_999, &[[1.0, 0.0]], 1).unwrap();
        assert_eq!(
            signal_endpoints(&series, rate).unwrap(),
            [
                "Start frame 767999\n≈0.999998 s",
                "End excl. frame 768000\n≈1.000000 s"
            ]
        );
        let empty = auditory_app::analyze_samples(rate, 768_000, &[], 1).unwrap();
        assert_eq!(signal_endpoints(&empty, rate).unwrap(), ["", ""]);
        // Bound the presentation type's entire integer domain without asking
        // the analyzer to accept a source frame outside its session domain.
        for hz in [1, 768_000] {
            let rate = auditory_app::SampleRate::new(hz).unwrap();
            for label in ["Start", "End excl."] {
                let text = signal_endpoint(label, u64::MAX, rate).unwrap();
                assert!(
                    text.capacity() <= 128,
                    "endpoint buffer exceeds renderer reserve"
                );
            }
        }
    }
    #[test]
    fn monitor_caption_and_context_allocations_obey_renderer_capacities() {
        let rate = auditory_app::SampleRate::new(1).unwrap();
        let series = auditory_app::analyze_samples(rate, 0, &[[1.0, -1.0]], 1).unwrap();
        let range = signal_range(&series, rate).unwrap();
        assert_eq!(range.capacity(), signal_geometry::RANGE_BYTES);
        let position = signal_position(u64::MAX, rate).unwrap();
        assert_eq!(position.capacity(), signal_geometry::POSITION_BYTES);
        let [start, end] = signal_endpoints(&series, rate).unwrap();
        assert_eq!(start.capacity(), signal_geometry::ENDPOINT_BYTES);
        assert_eq!(end.capacity(), signal_geometry::ENDPOINT_BYTES);
        let display = crate::signal::SignalDisplay::default();
        let context = signal_context(&display).unwrap();
        assert_eq!(context.capacity(), signal_geometry::CONTEXT_BYTES);
        let mut error_display = crate::signal::SignalDisplay::default();
        error_display.error = Some(crate::signal::DisplayError::Observation(
            auditory_app::ObservationError::Capacity { bytes: usize::MAX },
        ));
        assert!(
            signal_context(&error_display)
                .unwrap()
                .contains(&usize::MAX.to_string())
        );
    }
    #[test]
    fn compact_titles_are_single_line_without_changing_research_metadata() {
        let loaded = auditory_app::load_bytes(
            b"schema_version=2\nduration_seconds=1\nfade_seconds=0\n[about]\ntitle=\"\"\"First line\nSecond\t line\nThird line\"\"\"\n",
            auditory_app::Origin::Reader("title check".into()),
        )
        .unwrap();
        let declared = loaded.description();
        assert_eq!(source_title(declared), "First line Second line Third line");
        assert!(catalog_label(declared).ends_with("First line Second line Third line"));
        assert!(!catalog_label(declared).contains(['\n', '\r', '\t']));
        assert_eq!(
            declared.about().unwrap().title,
            "First line\nSecond\t line\nThird line"
        );
        assert!(description(declared).contains("First line\nSecond\t line\nThird line"));
        assert_eq!(
            inline_title("First\r\n\u{0085}\u{2028}\u{2029}Second"),
            "First Second"
        );
        assert_eq!(inline_title("\t \r\n\u{0085}\u{2028}\u{2029}"), "No title");
    }
    #[test]
    fn terminal_signal_text_distinguishes_unavailable_zero_and_lost_coverage() {
        let unavailable = terminal_signal_state(None, false);
        assert!(unavailable.contains("No samples were observed"));
        assert!(unavailable.contains("frame: unavailable"));
        assert!(!unavailable.contains("frame: 0"));
        let zero = terminal_signal_state(Some(0), false);
        assert!(zero.contains("frame: 0."));
        let captured = terminal_signal_state(Some(48_000), true);
        assert!(captured.contains("frame: 48000."));
        assert!(captured.contains("chart coverage may end earlier"));
        assert!(captured.contains("Playback result is reported separately"));
    }
    #[test]
    fn ended_without_observations_does_not_show_idle_start_guidance() {
        let mut display = crate::signal::SignalDisplay::default();
        display.finish();
        let text = signal_context(&display).unwrap();
        assert!(text.contains("No samples were observed for this operation"));
        assert!(!text.contains("Play starts"));
    }
}
