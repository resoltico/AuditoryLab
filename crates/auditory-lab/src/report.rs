// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Presentation of library-owned values.
use auditory_core::{Preset, Session};
use auditory_device::{OutputDescription, Played};
use auditory_wav::Exported;
use std::path::Path;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

fn seconds(value: f64) -> String {
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}
fn peak(value: f64) -> String {
    if value == f64::NEG_INFINITY {
        "silent".into()
    } else {
        format!("{value:.2} dBFS")
    }
}
pub(crate) fn resolved(origin: &str, preset: &Preset, session: &Session) -> String {
    let mut text = format!(
        "preset: {origin}\nsample rate: {} Hz\nlength: {} s ({} frames)\nfade: {} s ({} frames at each edge)\n",
        session.sample_rate().hz(),
        seconds(session.duration_seconds()),
        session.frames(),
        seconds(session.fade_seconds()),
        session.fade_frames()
    );
    if preset.stimulus.components.is_empty() {
        text.push_str("components: none (silence)\n");
    }
    for (index, component) in preset.stimulus.components.iter().enumerate() {
        text.push_str(&format!("component {}: {component}\n", index + 1));
    }
    let [left, right] = session.peak_dbfs();
    text.push_str(&format!(
        "peak bound: left ear {}, right ear {}\n",
        peak(left),
        peak(right)
    ));
    if let Some(about) = &preset.about {
        text.push_str(&format!("title: {:?}\n", about.title));
    }
    text
}
/// Wrap the final displayed text at 78 terminal columns. Split overlong words
/// only at extended grapheme boundaries, preserving combining and emoji sequences.
/// Escape embedded line breaks and tabs before measuring so values cannot
/// introduce output lines, and escape other terminal controls at the same time.
fn wrapped(text: &str) -> String {
    let shown = crate::run::terminal_text(text)
        .replace('\n', "\\n")
        .replace('\t', "\\t");
    // An indivisible cluster can itself exceed the line width (for example,
    // a long Indic conjunct). Escape only those clusters instead of splitting them.
    let mut bounded = String::new();
    for grapheme in shown.graphemes(true) {
        if grapheme.width() > 78 {
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
            if width + 1 + length > 78 {
                lines.push('\n');
                width = 0;
            } else {
                lines.push(' ');
                width += 1;
            }
        }
        for grapheme in word {
            let columns = grapheme.width();
            if width + columns > 78 {
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

/// Plain-language text about a preset, for listeners choosing a sound,
/// followed by its research detail.
pub(crate) fn about(origin: &str, preset: &Preset) -> String {
    let Some(about) = &preset.about else {
        return wrapped(&format!("{origin} has no [about] table describing it."));
    };
    let mut text = wrapped(&about.title);
    if let Some(studied_for) = &about.studied_for {
        text.push_str(&wrapped(&format!("Studied for: {studied_for}")));
    }
    if let Some(summary) = &about.summary {
        text.push('\n');
        text.push_str(&wrapped(summary));
    }
    if about.citation.is_some() || about.source_url.is_some() {
        text.push('\n');
    }
    if let Some(citation) = &about.citation {
        text.push_str(&wrapped(&format!("Source: {citation}")));
    }
    if let Some(source_url) = &about.source_url {
        text.push_str(&wrapped(source_url));
    }
    for (label, value) in [("Study", &about.study), ("Adaptation", &about.adaptation)] {
        if let Some(value) = value {
            text.push('\n');
            text.push_str(&wrapped(&format!("{label}: {value}")));
        }
    }
    text
}

pub(crate) fn listing(name: &str, preset: &Preset, width: usize) -> String {
    let title = preset
        .about
        .as_ref()
        .map_or("", |about| about.title.as_str());
    format!(
        "{name:width$}  {:>6} s  {title}\n",
        seconds(preset.stimulus.duration_seconds)
    )
}
pub(crate) fn start(action: &str, origin: &str, preset: &Preset, session: &Session) -> String {
    let title = preset
        .about
        .as_ref()
        .map_or_else(String::new, |p| format!(" ({:?})", p.title));
    format!(
        "{action} {origin}{title}: {} s ({} frames), {} Hz",
        seconds(session.duration_seconds()),
        session.frames(),
        session.sample_rate().hz()
    )
}
pub(crate) fn device(description: &OutputDescription) -> String {
    let name = description.name.as_ref().map_or_else(
        || "default output (name unavailable)".into(),
        |name| format!("{name:?}"),
    );
    format!(
        "; device {name}, {} device channels, {}\n",
        description.channels, description.representation
    )
}
pub(crate) fn exported(exported: Exported, origin: &str, destination: &Path) -> String {
    format!(
        "exported {origin} -> {destination:?}: {} frames at {} Hz ({})\n",
        exported.frames,
        exported.sample_rate.hz(),
        exported.container
    )
}
pub(crate) fn played(played: Played, origin: &str) -> String {
    format!(
        "played {origin}: {} frames at {} Hz as {}\n",
        played.frames,
        played.sample_rate.hz(),
        played.representation
    )
}
pub(crate) fn playback_warnings(played: Played) -> String {
    if played.underruns == 0 {
        String::new()
    } else {
        format!(
            "warning: the device reported {} buffer underruns; playback may have had gaps\n",
            played.underruns
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use auditory_core::{SampleRate, catalog};
    use auditory_device::SampleRepresentation;
    #[test]
    fn displayed_domain_values_and_optional_start_title_are_quoted() {
        let mut p = Preset::parse(catalog::ENTRIES[0].text).unwrap();
        p.about.as_mut().unwrap().title = "hostile\nexported forged\t\u{1b}[2J".into();
        let s = p.stimulus.session(SampleRate::DEFAULT).unwrap();
        let line = start("exporting", "input", &p, &s);
        assert!(!line.contains('\n'));
        assert!(line.contains("hostile\\nexported forged\\t\\u{1b}[2J"));
        assert!(
            resolved("input", &p, &s)
                .ends_with("title: \"hostile\\nexported forged\\t\\u{1b}[2J\"\n")
        );
        p.about = None;
        assert_eq!(
            start("exporting", "input", &p, &s),
            "exporting input: 300 s (14400000 frames), 48000 Hz"
        );
    }
    #[test]
    fn silence_and_routing_notices_describe_the_contract() {
        let p = Preset::parse("schema_version=2\nduration_seconds=1\nfade_seconds=0").unwrap();
        let s = p.stimulus.session(SampleRate::DEFAULT).unwrap();
        assert!(resolved("input", &p, &s).contains(
            "components: none (silence)\npeak bound: left ear silent, right ear silent\n"
        ));

        let p = Preset::parse(
            catalog::find("orozco-perez-2020-binaural-7-hz")
                .unwrap()
                .text,
        )
        .unwrap();
        assert!(
            resolved(
                "input",
                &p,
                &p.stimulus.session(SampleRate::DEFAULT).unwrap()
            )
            .contains("left ear")
        );
    }
    #[test]
    fn seconds_device_and_underrun_reporting_preserve_stream_semantics() {
        assert_eq!(seconds(1.2345678), "1.234568");
        assert_eq!(seconds(0.1), "0.1");
        assert_eq!(seconds(1.0 / 768_000.0), "0.000001");
        let description = OutputDescription {
            name: Some("bad\u{1b}[2J".into()),
            channels: 5,
            sample_rate: SampleRate::DEFAULT,
            representation: SampleRepresentation::F32,
        };
        assert!(device(&description).contains("\\u{1b}"));
        let result = Played {
            frames: 4,
            sample_rate: SampleRate::DEFAULT,
            representation: SampleRepresentation::F32,
            underruns: 2,
        };
        assert!(playback_warnings(result).contains("2 buffer underruns"));
        assert_eq!(
            playback_warnings(Played {
                underruns: 0,
                ..result
            }),
            ""
        );
    }
}
