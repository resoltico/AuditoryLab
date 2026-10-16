// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Human presentation of application-owned snapshots and reports.
use crate::args::RateSelection;
use auditory_app::{
    DeclaredDescription, Ears, ExportReport, ExportView, Facts, Notice, Origin, OutputDescription,
    PlaybackReport, PlaybackView, SaveReport,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

fn scalar(value: f64) -> String {
    if value.is_finite() && value != 0.0 && (value.abs() >= 1e12 || value.abs() < 1e-6) {
        format!("{value:e}")
    } else {
        value.to_string()
    }
}
fn duration(value: f64) -> String {
    let Ok(time) = std::time::Duration::try_from_secs_f64(value) else {
        return format!("{} s", scalar(value));
    };
    if value > 0.0 && time.is_zero() {
        return format!("{} s", scalar(value));
    }
    let mut parts = Vec::new();
    let secs = time.as_secs();
    if secs / 3600 != 0 {
        parts.push(format!("{} h", secs / 3600));
    }
    if (secs % 3600) / 60 != 0 {
        parts.push(format!("{} min", (secs % 3600) / 60));
    }
    if secs % 60 != 0 || time.subsec_nanos() != 0 || parts.is_empty() {
        let fraction = if time.subsec_nanos() == 0 {
            String::new()
        } else {
            format!(".{:09}", time.subsec_nanos())
                .trim_end_matches('0')
                .into()
        };
        parts.push(format!("{}{fraction} s", secs % 60));
    }
    format!(
        "{}{}",
        if time.as_secs_f64() == value { "" } else { "~" },
        parts.join(" ")
    )
}
fn peak(value: f64) -> String {
    if value == f64::NEG_INFINITY {
        "silent".into()
    } else {
        format!("{} dBFS", scalar(value))
    }
}
/// Wrap the final displayed text at 78 terminal columns. Split overlong words
/// only at extended grapheme boundaries, preserving combining and emoji sequences.
/// Escape embedded line breaks and tabs before measuring so values cannot
/// introduce output lines, and escape other terminal controls at the same time.
fn wrapped(text: &str, limit: usize) -> String {
    let shown = crate::run::inline_text(text);
    // An indivisible cluster can itself exceed the line width (for example,
    // a long Indic conjunct). Escape only those clusters instead of splitting them.
    let mut bounded = String::new();
    for grapheme in shown.graphemes(true) {
        if grapheme.width() > limit {
            bounded.extend(grapheme.chars().flat_map(char::escape_default));
        } else {
            bounded.push_str(grapheme);
        }
    }
    let mut lines = String::new();
    let mut width = 0;
    let graphemes: Vec<_> = bounded.graphemes(true).collect();
    for word in graphemes
        .split(|grapheme| *grapheme == " ")
        .filter(|word| !word.is_empty())
    {
        let length: usize = word.iter().map(|grapheme| grapheme.width()).sum();
        if !lines.is_empty() && !lines.ends_with('\n') {
            if width + 1 + length > limit {
                lines.push('\n');
                width = 0;
            } else {
                lines.push(' ');
                width += 1;
            }
        }
        for grapheme in word {
            let columns = grapheme.width();
            if width + columns > limit {
                lines.push('\n');
                width = 0;
            }
            lines.push_str(grapheme);
            width += columns;
        }
    }
    lines.push('\n');
    lines
}

fn field(prefix: &str, value: &str) -> String {
    let indent = " ".repeat(prefix.width());
    let body = wrapped(value, 78_usize.saturating_sub(prefix.width()).max(1));
    body.lines()
        .enumerate()
        .map(|(i, line)| format!("{}{line}\n", if i == 0 { prefix } else { &indent }))
        .collect()
}
fn identity(description: &DeclaredDescription) -> String {
    let mut text = String::new();
    if let Some(about) = description.about() {
        text.push_str(&wrapped(&about.title, 78));
    }
    text.push_str(&format!(
        "Source: {}\n",
        crate::run::inline_text(&origin(description.origin()))
    ));
    text
}
pub(crate) fn listing(descriptions: &[DeclaredDescription]) -> String {
    let ids: Vec<_> = descriptions
        .iter()
        .map(|d| match d.origin() {
            Origin::BuiltIn(id) => id.clone(),
            other => origin(other),
        })
        .collect();
    let width = ids.iter().map(|id| id.width()).max().unwrap_or(6).max(6);
    let mut text = format!("{:<width$}  DURATION\n", "PRESET");
    for (description, id) in descriptions.iter().zip(ids) {
        text.push_str(&format!(
            "{id:width$}  {}\n",
            duration(description.duration_seconds())
        ));
        text.push_str(&field(
            "  Sound: ",
            description
                .about()
                .map_or("No descriptive metadata", |a| a.title.as_str()),
        ));
    }
    text
}
pub(crate) fn describe(description: &DeclaredDescription) -> String {
    let mut text = identity(description);
    text.push_str(&field(
        "Declared duration: ",
        &duration(description.duration_seconds()),
    ));
    let routes: Vec<_> = description
        .routes()
        .iter()
        .map(|route| match route {
            Ears::Both => "both ears",
            Ears::Left => "left ear only",
            Ears::Right => "right ear only",
        })
        .collect();
    text.push_str(&field(
        "Declared ear routes: ",
        &if routes.is_empty() {
            "no components (silence)".into()
        } else {
            routes.join(", ")
        },
    ));
    text.push_str("Validation: not checked at a sample rate\n");
    for supplied in description.notices() {
        text.push_str(&field("For later playback: ", notice(*supplied).trim_end()));
    }
    let Some(about) = description.about() else {
        text.push_str("No [about] descriptive metadata is present.\n");
        return text;
    };
    let missing: Vec<_> = [
        ("studied-for", &about.studied_for),
        ("summary", &about.summary),
        ("citation", &about.citation),
        ("source link", &about.source_url),
        ("study", &about.study),
        ("adaptation", &about.adaptation),
    ]
    .into_iter()
    .filter_map(|(label, value)| value.is_none().then_some(label))
    .collect();
    if !missing.is_empty() {
        text.push_str(&field("Not provided: ", &missing.join(", ")));
    }
    for (label, value) in [
        ("Studied for: ", &about.studied_for),
        ("Summary: ", &about.summary),
        ("Citation: ", &about.citation),
        ("Link: ", &about.source_url),
        ("Study: ", &about.study),
        ("Adaptation: ", &about.adaptation),
    ] {
        if let Some(value) = value {
            text.push_str(&field(label, value));
        }
    }
    text
}
pub(crate) fn resolved(facts: &Facts, selection: RateSelection) -> String {
    let declared = facts.description();
    let mut text = identity(declared);
    text.push_str(&format!("Validation: passed at {} Hz\nRate selection: {}\nDuration: {}\nDeclared duration: {} s\nResolved duration: {} s ({} frames)\nDeclared fade: {} s\nResolved fade: {} s ({} frames per edge){}\n", facts.sample_rate().hz(), match selection { RateSelection::Default=>"default", RateSelection::Explicit=>"explicit" }, duration(facts.duration_seconds()), scalar(declared.duration_seconds()), scalar(facts.duration_seconds()), facts.frames(), scalar(declared.fade_seconds()),scalar(facts.fade_seconds()),facts.fade_frames(), if facts.fade_frames()==0 { "; fading disabled" } else { "" }));
    if declared.components().is_empty() {
        text.push_str("Components: none (silence)\n");
    }
    for (index, component) in declared.components().iter().enumerate() {
        text.push_str(&field(
            &format!("Component {}: ", index + 1),
            &component.to_string(),
        ));
    }
    let [left, right] = facts.peak_dbfs();
    text.push_str(&format!(
        "Peak bound: left ear {}, right ear {}\n",
        peak(left),
        peak(right)
    ));
    text.push_str(
        "Peak bounds apply before output conversion; rounding can change them slightly.\n",
    );
    text.push_str(&wrapped(
        "Playback validates again at the output stream's sample rate. No device was opened.",
        78,
    ));
    text
}
pub(crate) fn export_start(view: &ExportView) -> String {
    let facts = view.facts();
    format!(
        "Starting export\n{}Destination: {:?}\nDuration: {}\nExport rate: {} Hz\nPress Ctrl-C to stop\n",
        identity(facts.description()),
        view.destination(),
        duration(facts.duration_seconds()),
        facts.sample_rate().hz()
    )
}
fn device(description: &OutputDescription) -> String {
    let name = description
        .name
        .as_ref()
        .map_or_else(|| "name unavailable".into(), |name| format!("{name:?}"));
    let mut text = format!("Device: {}\n", crate::run::inline_text(&name));
    text.push_str(&format!(
        "Stream rate: {} Hz\nStream channels: {}\nSample representation: {}\n",
        description.sample_rate.hz(),
        description.channels,
        description.representation
    ));
    text.push_str("Left/right use the first two channels; remaining channels are silent.\n");
    text
}
pub(crate) fn playback_start(view: &PlaybackView) -> String {
    let facts = view.facts();
    let mut text = format!(
        "Starting playback\n{}Duration: {}\n{}",
        identity(facts.description()),
        duration(facts.duration_seconds()),
        device(view.device())
    );
    for supplied in view.notices() {
        text.push_str(&notice(*supplied));
    }
    text.push_str("Press Ctrl-C to stop\n");
    text
}
pub(crate) fn saved(report: &SaveReport) -> String {
    format!(
        "Recipe saved: {} -> {:?}\n",
        origin(report.origin()),
        report.destination()
    )
}
pub(crate) fn exported(report: &ExportReport) -> String {
    let result = report.exported();
    format!(
        "Export completed: {} -> {:?}: {} frames at {} Hz ({})\n",
        origin(report.origin()),
        report.destination(),
        result.frames,
        result.sample_rate.hz(),
        result.container
    )
}
pub(crate) fn played(report: &PlaybackReport) -> String {
    let result = report.played();
    format!(
        "Playback completed: {}: {} frames at {} Hz as {}\n",
        origin(report.origin()),
        result.frames,
        result.sample_rate.hz(),
        result.representation
    )
}
pub(crate) fn notice(notice: Notice) -> String {
    match notice {
        Notice::LowStartingVolume => {
            "Keep output volume low; digital levels do not limit sound pressure.\n".into()
        }
        Notice::HeadphonesForSeparateEars => {
            "Use headphones to preserve the intended ear routing.\n".into()
        }
        Notice::BufferUnderruns { count } => format!(
            "warning: the device reported {count} buffer underruns; playback may have had gaps\n"
        ),
    }
}
pub(crate) fn origin(origin: &Origin) -> String {
    match origin {
        Origin::BuiltIn(name) => format!("{name} (built-in)"),
        Origin::File(path) => format!("{path:?}"),
        Origin::Reader(label) => format!("{label:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auditory_app::{Origin, SampleRate, SampleRepresentation, load_bytes};
    fn loaded(text: &str) -> auditory_app::LoadedPreset {
        load_bytes(text.as_bytes(), Origin::Reader("test".into())).unwrap()
    }
    #[test]
    fn human_durations_normalize_carry_and_preserve_tiny_or_invalid_declarations() {
        for (seconds, expected) in [
            (0.0, "0 s"),
            (60.0, "1 min"),
            (3600.0, "1 h"),
            (307.2, "5 min 7.2 s"),
            (59.9999999, "59.9999999 s"),
            (3599.9999999, "59 min 59.9999999 s"),
            (59.9999999999, "~1 min"),
            (3599.9999999999, "~1 h"),
            (1.0 / 768000.0, "~0.000001302 s"),
            (1e-12, "1e-12 s"),
            (1e300, "1e300 s"),
            (-1.0, "-1 s"),
            (f64::NAN, "NaN s"),
            (f64::INFINITY, "inf s"),
            (f64::NEG_INFINITY, "-inf s"),
            (18446744073709551616.0, "1.8446744073709552e19 s"),
            (68_719_476_736.0, "19088743 h 32 min 16 s"),
        ] {
            assert_eq!(duration(seconds), expected, "{seconds:?}");
        }
        for value in [1.0 / 768000.0, 59.9999999, 1e300, 1e-300, 307.2, -5.1] {
            assert_eq!(scalar(value).parse::<f64>().unwrap(), value);
        }
    }
    #[test]
    fn printed_peak_bounds_round_trip_authoritative_facts_on_both_rounding_sides() {
        for level in [-20.006_f64, -20.004, -20.005, -0.006, -89.994] {
            let preset = loaded(&format!(
                "schema_version=2\nduration_seconds=0.01\nfade_seconds=0\n[[component]]\nkind='tone'\nears='both'\nfrequency_hz=1000\npeak_dbfs={level}"
            ));
            let facts = preset.inspect(SampleRate::DEFAULT).unwrap();
            let text = resolved(&facts, RateSelection::Default);
            let line = text
                .lines()
                .find(|line| line.starts_with("Peak bound: "))
                .unwrap();
            let (left, right) = line
                .strip_prefix("Peak bound: left ear ")
                .unwrap()
                .split_once(" dBFS, right ear ")
                .unwrap();
            let parsed = [
                left.parse::<f64>().unwrap(),
                right.strip_suffix(" dBFS").unwrap().parse::<f64>().unwrap(),
            ];
            assert_eq!(
                parsed,
                facts.peak_dbfs(),
                "display changed the bound for {level}"
            );
        }
        let silence = loaded("schema_version=2\nduration_seconds=1\nfade_seconds=0");
        assert!(
            resolved(
                &silence.inspect(SampleRate::DEFAULT).unwrap(),
                RateSelection::Default
            )
            .contains("Peak bound: left ear silent, right ear silent")
        );
    }
    #[test]
    fn inspection_uses_one_snapshot_and_keeps_declared_and_resolved_fade() {
        let preset = loaded("schema_version=2\nduration_seconds=0.0000209\nfade_seconds=0.00001");
        let facts = preset.inspect(SampleRate::DEFAULT).unwrap();
        let text = resolved(&facts, RateSelection::Explicit);
        assert!(text.contains("Validation: passed at 48000 Hz\nRate selection: explicit\n"));
        assert!(text.contains("Declared duration: 0.0000209 s"));
        assert!(text.contains("Resolved duration: 0.000020833333333333333 s (1 frames)"));
        assert!(text.contains("Declared fade: 0.00001 s"));
        assert!(text.contains("Resolved fade: 0 s (0 frames per edge); fading disabled"));
        assert!(text.contains("Playback validates again"));
        assert!(resolved(&facts, RateSelection::Default).contains("Rate selection: default"));
    }
    #[test]
    fn declared_views_are_not_playability_claims_and_keep_route_assignments() {
        let preset = loaded("schema_version=2\nduration_seconds=nan\nfade_seconds=0");
        let text = describe(preset.description());
        assert!(text.contains("Declared duration: NaN s"));
        assert!(text.contains("Declared ear routes: no components (silence)"));
        assert!(text.contains("Validation: not checked at a sample rate"));
        assert!(text.contains("No [about] descriptive metadata"));
        assert!(text.contains("For later playback: Keep output volume low"));
        assert!(!text.contains("headphones"));
        let preset = loaded(
            "schema_version=2\nduration_seconds=1\nfade_seconds=0\n[[component]]\nkind='tone'\nears='left'\nfrequency_hz=440\npeak_dbfs=-20\n[[component]]\nkind='tone'\nears='right'\nfrequency_hz=440\npeak_dbfs=-20",
        );
        let text = describe(preset.description());
        assert!(text.contains("Declared ear routes: left ear only, right ear only"));
        assert!(text.contains("headphones"));
        assert!(!text.contains("binaural"));
    }
    #[test]
    fn list_keeps_intact_ids_and_indents_every_sound_continuation() {
        let id = "long-preset-id-".repeat(10);
        let title = "combining e\u{301} family 👨‍👩‍👧‍👦 ".repeat(20);
        let text = format!(
            "schema_version=2\nduration_seconds=307.2\nfade_seconds=0\n[about]\ntitle='{title}'"
        );
        let preset = load_bytes(text.as_bytes(), Origin::BuiltIn(id.clone())).unwrap();
        let result = listing(&[preset.description().clone()]);
        let lines: Vec<_> = result.lines().collect();
        assert!(lines[0].contains("PRESET") && lines[0].ends_with("DURATION"));
        assert!(lines[1].starts_with(&id) && lines[1].ends_with("5 min 7.2 s"));
        assert!(describe(preset.description()).contains(&format!("Source: {id} (built-in)\n")));
        assert!(lines[2].starts_with("  Sound: "));
        assert!(
            lines[2..]
                .iter()
                .all(|line| line.starts_with("  ") && line.width() <= 78)
        );
        let restored = lines[2..]
            .iter()
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(restored, format!("Sound: {}", title.trim()));
    }
    #[test]
    fn device_output_is_truthful_for_multichannel_missing_and_hostile_names() {
        for name in [None, Some("headphones\n\u{1b}\u{202e}".into())] {
            let description = OutputDescription {
                name,
                channels: 6,
                sample_rate: SampleRate::DEFAULT,
                representation: SampleRepresentation::F32,
            };
            let text = device(&description);
            assert!(text.contains("Stream channels: 6"));
            assert!(text.contains("Stream rate: 48000 Hz"));
            assert!(text.contains("remaining channels are silent"));
            assert!(!text.contains("stereo"));
            if description.name.is_none() {
                assert!(text.contains("name unavailable"));
            } else {
                assert!(text.contains("\\n"));
                assert!(!text.contains('\u{1b}'));
            }
        }
    }
    #[test]
    fn partial_metadata_and_bound_paths_remain_explicit() {
        let preset = loaded(
            "schema_version=2\nduration_seconds=1\nfade_seconds=0\n[about]\ntitle='Title only'",
        );
        let text = describe(preset.description());
        assert!(text.contains("Not provided: studied-for, summary, citation, source link, study,"));
        let prepared = preset
            .prepare_export(SampleRate::DEFAULT, std::path::Path::new("two  spaces.wav"))
            .unwrap();
        assert!(export_start(prepared.view()).contains("Destination: \"two  spaces.wav\"\n"));
    }
    #[test]
    fn prose_escapes_inline_controls_and_preserves_grapheme_wrapping() {
        let text = "line\n\t\r\u{1b}\u{202e} ".repeat(20);
        let result = field("  Sound: ", &text);
        assert!(
            !result.contains('\u{1b}')
                && !result.contains('\u{202e}')
                && !result.contains('\r')
                && !result.contains('\t')
        );
        assert!(result.contains("\\n\\t\\r\\u{1b}\\u{202e}"));
        assert!(
            result
                .lines()
                .all(|line| line.starts_with("  ") && line.width() <= 78)
        );
        let result = wrapped(&"👩‍🚀e\u{301}".repeat(40), 78);
        assert!(
            result
                .lines()
                .all(|line| line.width() <= 78 && !line.starts_with('\u{301}'))
        );
    }
}
