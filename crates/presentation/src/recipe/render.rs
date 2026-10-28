//! recipe render presentation.
use crate::{
    Explain, Notice, format_dbfs, human_duration, nominal_description, structure_description,
    voice_description,
};
use auditorylab_app::document::recipe::RecipeSnapshot;
use auditorylab_app::document::recipe::Rendition;
use auditorylab_app::vocabulary::Status;
use auditorylab_core::SampleRate;
use std::fmt::Write as _;
use std::path::Path;
pub(crate) fn document_notes(snapshot: &RecipeSnapshot) -> String {
    let document = &snapshot.recipe().document();
    let mut text = format!(
        "Document: {}\nCreated (declared): {}\nEdited (declared): {}\n",
        document.id,
        document
            .created_at
            .map_or_else(|| Status::Unknown.label().into(), |t| t.to_string()),
        document
            .modified_at
            .map_or_else(|| Status::Unknown.label().into(), |t| t.to_string())
    );
    if let Some(notes) = crate::recipe::snapshot_about(snapshot)
        .and_then(|a| a.summary)
        .filter(|s| !s.is_empty())
    {
        let _ = writeln!(text, "Sound notes: {notes}");
    }
    for ancestor in snapshot.recipe().lineage() {
        let _ = write!(
            text,
            "Based on: {}\nAncestor note: {}\nResearch findings describe the \
            ancestor, not this sound.\n",
            ancestor.title, ancestor.note
        );
    }
    for source in snapshot.recipe().sources() {
        let _ = write!(
            text,
            "Source {}: {}\nSource citation: {}\n",
            source.key, source.label, source.citation
        );
        if let Some(url) = &source.url {
            let _ = writeln!(text, "Source URL: {url}");
        }
        if let Some(doi) = &source.doi {
            let _ = writeln!(text, "Source DOI: {doi}");
        }
    }
    for evidence in snapshot.recipe().evidence() {
        if let Some(detail) = &evidence.population_detail {
            let _ = writeln!(text, "Population detail: {detail}");
        }
        let _ = writeln!(
            text,
            "Measures: {}",
            evidence
                .measures
                .iter()
                .map(|measure| measure.label())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let _ = write!(
            text,
            "Evidence {} · source {} · population {} · role {}\nSummary: \
            {}\nLimitations: {}\nMethods: {}\nAdaptation: {}\n",
            evidence.scope.label(),
            evidence.source,
            evidence.population.label(),
            evidence.condition_role.label(),
            evidence.summary,
            evidence.limitations,
            evidence.methods_locator,
            evidence.adaptation
        );
    }
    text
}

/// Escape controls and bidi overrides at a read-only display boundary. Ordinary
/// Unicode text, paragraph newlines and tabs retain their meaning. Stored content
/// and editable fields must continue to use the original text.
pub fn display_text(value: &str) -> String {
    let text = value;
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        if (c.is_control() && c != '\n' && c != '\t')
            || matches!(c,
                '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}')
        {
            result.extend(c.escape_default());
        } else {
            result.push(c);
        }
    }
    result
}

/// Readable single-line title/path text, without changing its stored value.
pub fn display_scalar(value: &str) -> String {
    let text = value;
    crate::display_text(text)
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

/// Human description from the same captured data used by structured output.
pub fn recipe_description(value: &auditorylab_app::services::Description<'_>) -> String {
    crate::snapshot_description_with_history(value.source().snapshot(), value.history_warnings())
}

/// The research text of a snapshot in its shared display order.
pub fn snapshot_research(value: &RecipeSnapshot) -> Vec<(&'static str, String)> {
    crate::recipe::snapshot_about(value)
        .and_then(|a| a.research)
        .map_or_else(Vec::new, |a| {
            [
                ("Studied for", a.studied_for),
                ("Summary", a.summary),
                ("Citation", a.citation),
                ("Source", a.source_url),
                ("Study", a.study),
                ("Adaptation", a.adaptation),
            ]
            .into_iter()
            .filter_map(|(label, value)| value.map(|text| (label, text)))
            .collect()
        })
}

/// A library row using cached nominal classification, without target compilation.
pub fn snapshot_library_label(value: &RecipeSnapshot) -> String {
    let snapshot = value;
    let structure = nominal_description(snapshot.nominal());
    format!(
        "{}{}{}\n{}\n{} · {}",
        if matches!(
            snapshot.origin(),
            auditorylab_app::document::recipe::Origin::File(_)
        ) {
            "Imported · "
        } else {
            ""
        },
        crate::display_scalar(&(snapshot.title())),
        if matches!(
            snapshot.origin(),
            auditorylab_app::document::recipe::Origin::BuiltIn(_)
        ) {
            " · Built-in · Read only"
        } else {
            ""
        },
        structure,
        crate::recipe::snapshot_about(snapshot)
            .and_then(|a| a.research)
            .and_then(|a| a.studied_for)
            .unwrap_or_else(|| "Custom recipe".into()),
        human_duration(snapshot.sound().duration_seconds().get())
    )
}

/// A snapshot's research text, components and validated facts at one
/// explicit inspection rate.
pub fn snapshot_details_at(value: &RecipeSnapshot, rate: SampleRate) -> String {
    let snapshot = value;
    let mut text = format!("Source: {}\n\n", snapshot.origin());
    text.push_str(&document_notes(snapshot));
    for (name, value) in crate::snapshot_research(snapshot) {
        let _ = write!(text, "{name}: {value}\n\n");
    }
    for c in snapshot.sound().components() {
        let _ = writeln!(text, "Component {}: {}", c.id, crate::recipe::component(c));
    }
    match snapshot.at(rate) {
        Ok(sound) => {
            let s = sound.signal();
            text.push_str(&crate::rendition_description(&(sound), true));
            let _ = write!(
                text,
                "\n{} Hz · {} frames · {}\nFade: {} frames\nPeak bounds: {} / {} dBFS",
                rate.hz(),
                s.frames(),
                human_duration(s.duration_seconds()),
                s.fade_frames(),
                format_dbfs(s.peak_dbfs()[0]),
                format_dbfs(s.peak_dbfs()[1])
            );
        }
        Err(e) => text.push_str(&e.explain()),
    }
    text
}

/// A recipe's title, source, timing, research text and description at the
/// default rate, labelled with that rate.
pub fn snapshot_description(value: &RecipeSnapshot) -> String {
    crate::snapshot_description_with_history(
        value,
        &auditorylab_app::services::history_warnings(value.recipe()),
    )
}

pub fn snapshot_description_with_history(
    value: &RecipeSnapshot,
    warnings: &[auditorylab_app::services::StorageWarning],
) -> String {
    let snapshot = value;
    let s = snapshot.sound();
    let mut out = format!(
        "{}\nSource: {}\nDuration: {}\nFade: {} at each edge\n",
        snapshot.title(),
        snapshot.origin().explain(),
        human_duration(s.duration_seconds().get()),
        human_duration(s.fade_seconds().get())
    );
    out.push_str(&document_notes(snapshot));
    for warning in warnings {
        let _ = writeln!(out, "{}", crate::errors::publication::warning(warning));
    }
    for (label, value) in crate::snapshot_research(snapshot) {
        let _ = writeln!(out, "{label}: {value}");
    }
    let rate = SampleRate::DEFAULT;
    match snapshot.at(rate) {
        Ok(sound) => {
            let _ = write!(
                out,
                "Description at {} Hz:\n{}\n",
                rate.hz(),
                crate::rendition_description(&(sound), false)
            );
        }
        Err(error) => {
            for component in s.components() {
                let _ = writeln!(
                    out,
                    "Component {}: {}",
                    component.id,
                    crate::recipe::component(component)
                );
            }
            let _ = writeln!(out, "{error}");
        }
    }
    out
}

/// What is about to play and on which output, with listening guidance.
pub fn snapshot_playback_preamble(
    value: &RecipeSnapshot,
    output: &auditorylab_app::services::playback::OutputInfo,
    signal: &auditorylab_core::Signal,
) -> String {
    let snapshot = value;
    format!(
        "Preparing {}\nSource: {}\nDuration: {}\nDevice: {}\n{} Hz, {} channels, {}\n{}",
        snapshot.title(),
        snapshot.origin().explain(),
        human_duration(signal.duration_seconds()),
        output.name.as_deref().unwrap_or("default output"),
        output.sample_rate.hz(),
        output.channels,
        crate::sample_format_label(output.format),
        Notice::for_sound(snapshot.sound())
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// A realized sound's structure in one line.
pub fn rendition_brief(value: &Rendition) -> String {
    let sound = value;
    structure_description(&auditorylab_core::rhythm::realized(sound.signal().target()))
}

/// Facts of a realized sound at its own rate, every line at that rate.
/// `detail` adds the model caption and requested-versus-realized values.
pub fn rendition_description(value: &Rendition, detail: bool) -> String {
    let sound = value;
    let mut text = crate::rendition_brief(sound);
    if detail {
        let model = auditorylab_app::workspace::monitor::ModelView::new(
            sound.signal().target(),
            sound.snapshot().title(),
            auditorylab_app::workspace::monitor::ModelSource::Recipe,
        );
        let _ = write!(
            text,
            "\n{}",
            crate::workspace::monitor::model_view_caption(&(model))
        );
    }
    for (id, facts) in sound.signal().voices() {
        let _ = write!(
            text,
            "\n{}: {}",
            crate::errors::acoustic::voice_reference(id),
            voice_description(facts, detail)
        );
    }
    text
}

/// A realized sound's facts, every one at the sound's own rate.
pub fn rendition_inspection(value: &Rendition) -> String {
    let sound = value;
    let snapshot = sound.snapshot();
    let signal = sound.signal();
    let mut out = format!("{}\nSource: {}\n", snapshot.title(), snapshot.origin());
    out.push_str(&document_notes(snapshot));
    for (label, value) in crate::snapshot_research(snapshot) {
        let _ = writeln!(out, "{label}: {value}");
    }
    let _ = write!(
        out,
        "Sample rate: {} Hz\nFrames: {}\nDuration: {}\nFade: {} frames ({} \
            s)\nPeak bounds: left {} dBFS, right {} dBFS\n{}",
        signal.sample_rate().hz(),
        signal.frames(),
        human_duration(signal.duration_seconds()),
        signal.fade_frames(),
        signal.fade_seconds(),
        format_dbfs(signal.peak_dbfs()[0]),
        format_dbfs(signal.peak_dbfs()[1]),
        crate::rendition_description(sound, true)
    );
    out
}

/// Explain the captured export and selected rate before writing.
pub fn rendition_export_preamble(value: &Rendition, path: &Path) -> String {
    format!(
        "Exporting {} to {} at {} Hz. Press Ctrl-C to stop.",
        value.snapshot().title(),
        crate::display_scalar(&(path.display().to_string())),
        value.signal().sample_rate().hz()
    )
}
