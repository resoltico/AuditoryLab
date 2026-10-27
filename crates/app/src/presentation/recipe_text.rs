//! Shared descriptions of authored content and its research context.
use crate::core::{Carrier, Component, ComponentForm, EvidenceScope, Modulation, Recipe, Voice};

/// Derived presentation of current title/notes and a primary current condition.
/// This is a view, not a second persisted metadata authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct About {
    /// Current title.
    pub title: String,
    /// Current own notes.
    pub summary: Option<String>,
    /// Primary current-condition context; complete evidence remains on Recipe.
    pub research: Option<Research>,
}
/// A concise derived research view. Full records retain their explicit scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Research {
    /// Declared population/role context.
    pub studied_for: Option<String>,
    /// Findings with limitations.
    pub summary: Option<String>,
    /// Primary source citation.
    pub citation: Option<String>,
    /// Explicit source URL, never fetched automatically.
    pub source_url: Option<String>,
    /// Primary methods locator.
    pub study: Option<String>,
    /// Disclosed adaptations.
    pub adaptation: Option<String>,
}
/// Derive concise presentation while leaving full records on the recipe.
#[must_use]
pub fn about(recipe: &Recipe) -> Option<About> {
    let description = recipe.description().as_ref()?;
    let research = recipe
        .evidence()
        .iter()
        .find(|e| e.scope == EvidenceScope::ReferenceCondition)
        .and_then(|e| {
            recipe
                .sources()
                .iter()
                .find(|s| s.key == e.source)
                .map(|s| Research {
                    studied_for: e.population_detail.clone().or_else(|| {
                        Some(format!(
                            "{} study · {}",
                            e.population.label(),
                            e.condition_role.label()
                        ))
                    }),
                    summary: Some(format!("{} {}", e.summary, e.limitations)),
                    citation: Some(s.citation.clone()),
                    source_url: s
                        .url
                        .clone()
                        .or_else(|| s.doi.as_ref().map(|d| format!("https://doi.org/{d}"))),
                    study: Some(e.methods_locator.clone()),
                    adaptation: Some(e.adaptation.clone()),
                })
        });
    Some(About {
        title: description.title.clone(),
        summary: description.notes.clone(),
        research,
    })
}

/// Attribution text stored on a newly independent copy.
#[must_use]
pub fn copy_note(recipe: &Recipe) -> String {
    recipe.description().as_ref().and_then(|d| d.notes.as_deref()).filter(|s| !s.is_empty()).map_or_else(
        || "Attributed copy; source findings concern the ancestor, not this sound.".into(),
        |notes| format!("Attributed copy; source findings concern the ancestor, not this sound. Ancestor notes: {notes}"),
    )
}

/// Describe one authored component without compiling it at a sample rate.
#[must_use]
pub fn component(component: &Component) -> String {
    match component.form {
        ComponentForm::Voice(v) => voice(v),
        ComponentForm::BinauralPair(pair) | ComponentForm::MonauralPair { pair, .. } => {
            let kind = match component.form {
                ComponentForm::BinauralPair(_) => crate::core::PairKind::Binaural,
                _ => crate::core::PairKind::Monaural,
            };
            format!(
                "{}, first {} Hz, signed offset {} Hz, second {} Hz, peaks {} / {} dBFS",
                kind.phrase(),
                pair.first_hz,
                pair.offset_hz,
                pair.first_hz + pair.offset_hz,
                pair.first_peak_dbfs,
                pair.second_peak_dbfs
            )
        }
    }
}
fn voice(voice: Voice) -> String {
    let carrier = match voice.carrier {
        Carrier::Tone { frequency_hz } => {
            format!("{} {frequency_hz} Hz", voice.carrier.kind().phrase())
        }
        Carrier::Clicks {
            rate_hz,
            width_seconds,
        } => format!(
            "{} {rate_hz} Hz, {width_seconds} s wide",
            voice.carrier.kind().phrase()
        ),
    };
    let mut text = format!(
        "{carrier}, peak {} dBFS, {}",
        voice.peak_dbfs,
        voice.ears.phrase()
    );
    if let Some(modulation) = voice.modulation {
        text.push_str(&format!(
            ", {}",
            match modulation {
                Modulation::Am { rate_hz, depth } =>
                    format!("{} {rate_hz} Hz, depth {depth}", modulation.kind().phrase()),
                Modulation::Gate {
                    rate_hz,
                    width_seconds,
                } => format!(
                    "{} {rate_hz} Hz, {width_seconds} s open",
                    modulation.kind().phrase()
                ),
            }
        ));
    }
    text
}

#[cfg(test)]
mod tests;
