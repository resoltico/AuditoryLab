//! Shared descriptions of authored content and its research context.
use auditorylab_core::Component;
use auditorylab_core::ComponentForm;
use auditorylab_core::Modulation;
use auditorylab_core::Recipe;
use auditorylab_core::provenance::EvidenceScope;
use std::fmt::Write as _;

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
    /// What the study measured.
    pub measured: Option<String>,
    /// Primary source citation.
    pub citation: Option<String>,
    /// Explicit source URL, never fetched automatically.
    pub source_url: Option<String>,
    /// Primary methods locator.
    pub study: Option<String>,
    /// Disclosed adaptations.
    pub adaptation: Option<String>,
}
/// Plain-language purpose of a study condition: its declared detail, otherwise
/// its population and role. Lists and details use this one rule.
#[must_use]
pub fn studied_for(
    detail: Option<&str>,
    population: auditorylab_core::provenance::Population,
    role: auditorylab_core::provenance::ConditionRole,
) -> String {
    detail.map_or_else(
        || format!("{} study · {}", population.label(), role.label()),
        str::to_owned,
    )
}
/// The evidence record and source the research view describes: the first current
/// reference condition whose source is declared.
pub(crate) fn research_record(
    recipe: &Recipe,
) -> Option<(
    usize,
    &auditorylab_core::provenance::Evidence,
    &auditorylab_core::provenance::BibliographySource,
)> {
    recipe
        .evidence()
        .iter()
        .enumerate()
        .filter(|(_, e)| e.scope == EvidenceScope::ReferenceCondition)
        .find_map(|(index, e)| {
            recipe
                .sources()
                .iter()
                .find(|s| s.key == e.source)
                .map(|s| (index, e, s))
        })
}
fn source_link(source: &auditorylab_core::provenance::BibliographySource) -> Option<String> {
    source
        .url
        .clone()
        .or_else(|| source.doi.as_ref().map(|d| format!("https://doi.org/{d}")))
}
fn measures(evidence: &auditorylab_core::provenance::Evidence) -> Option<String> {
    (!evidence.measures.is_empty()).then(|| {
        evidence
            .measures
            .iter()
            .map(|measure| measure.label())
            .collect::<Vec<_>>()
            .join(", ")
    })
}
/// Derive concise presentation while leaving full records on the recipe.
#[must_use]
pub fn about(recipe: &Recipe) -> Option<About> {
    let description = recipe.description()?;
    let research = research_record(recipe).map(|(_, e, s)| Research {
        studied_for: Some(studied_for(
            e.population_detail.as_deref(),
            e.population,
            e.condition_role,
        )),
        summary: Some(format!("{} {}", e.summary, e.limitations)),
        measured: measures(e),
        citation: Some(s.citation.clone()),
        source_url: source_link(s),
        study: Some(e.methods_locator.clone()),
        adaptation: Some(e.adaptation.clone()),
    });
    Some(About {
        title: description.title.clone(),
        summary: description.notes.clone(),
        research,
    })
}

/// Evidence records and sources the research view does not describe, such as
/// ancestor findings, each rendered once with its scope and study key.
pub(crate) fn further_evidence(recipe: &Recipe) -> String {
    let primary = research_record(recipe);
    let mut text = String::new();
    for (index, e) in recipe.evidence().iter().enumerate() {
        if primary.is_some_and(|(used, _, _)| used == index) {
            continue;
        }
        let _ = write!(
            text,
            "Further evidence ({}, {}):\nStudied for: {}\nSummary: {} {}\nStudy: {}\n\
            Adaptation: {}\n",
            e.scope.label(),
            e.source,
            studied_for(
                e.population_detail.as_deref(),
                e.population,
                e.condition_role
            ),
            e.summary,
            e.limitations,
            e.methods_locator,
            e.adaptation
        );
        if let Some(measured) = measures(e) {
            let _ = writeln!(text, "Measured: {measured}");
        }
    }
    let primary_source = primary.map(|(_, _, s)| s.key.as_str());
    for source in recipe.sources() {
        if Some(source.key.as_str()) == primary_source {
            continue;
        }
        let _ = writeln!(text, "Further source ({}): {}", source.key, source.citation);
        if let Some(link) = source_link(source) {
            let _ = writeln!(text, "Link: {link}");
        }
    }
    text
}

/// Describe one authored component without compiling it at a sample rate.
#[must_use]
pub fn component(component: &Component) -> String {
    let form = component.form;
    let mut text = if let Some(pair) = form.pair() {
        format!(
            "{}, first {} Hz, beat offset {} Hz, second {} Hz, peaks {} / {} dBFS",
            form.pair_kind()
                .unwrap_or_else(|| unreachable!("pair source"))
                .phrase(),
            pair.first_hz,
            pair.offset_hz,
            pair.first_hz.get() + pair.offset_hz.get(),
            pair.first_peak_dbfs,
            pair.second_peak_dbfs
        )
    } else {
        voice(form)
    };
    if let Some(modulation) = form.modulation {
        let _ = write!(
            text,
            ", {}",
            match modulation {
                Modulation::Sinusoidal { rate_hz, depth } => format!(
                    "sinusoidal AM {rate_hz} Hz, depth {}%",
                    crate::compact_number(depth.get() * 100.0)
                ),
                Modulation::Pulsed { pulse } => format!(
                    "shared pulsed modulation {}",
                    crate::sound::pulse_spec(pulse)
                ),
            }
        );
    }
    text
}
fn voice(form: ComponentForm) -> String {
    let carrier = match form.source {
        auditorylab_core::SoundSource::Tone { frequency_hz, .. } => {
            format!("tone {frequency_hz} Hz")
        }
        auditorylab_core::SoundSource::PulseTrain { pulse, .. } => {
            format!("pulse train {}", crate::sound::pulse_spec(pulse))
        }
        _ => unreachable!("primitive source"),
    };
    format!(
        "{carrier}, peak {} dBFS, {}",
        form.get(auditorylab_core::ParameterId::PeakLevel)
            .unwrap_or_else(|| unreachable!("primitive level")),
        form.route()
            .unwrap_or_else(|| unreachable!("primitive route"))
            .phrase()
    )
}

/// Derived descriptions of immutable snapshot content.
pub fn snapshot_about(value: &auditorylab_app::document::recipe::RecipeSnapshot) -> Option<About> {
    about(value.recipe())
}

mod render;
pub use render::*;

#[cfg(test)]
mod tests;
