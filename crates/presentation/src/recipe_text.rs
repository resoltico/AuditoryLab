//! Shared descriptions of authored content and its research context.
use auditorylab_core::Component;
use auditorylab_core::ComponentForm;
use auditorylab_core::Modulation;
use auditorylab_core::Recipe;
use auditorylab_core::provenance::EvidenceScope;

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
    let description = recipe.description()?;
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

/// Describe one authored component without compiling it at a sample rate.
#[must_use]
pub fn component(component: &Component) -> String {
    let form = component.form;
    if let Some(pair) = form.pair() {
        let kind = form
            .pair_kind()
            .unwrap_or_else(|| unreachable!("pair kind"));
        format!(
            "{}, first {} Hz, signed offset {} Hz, second {} Hz, peaks {} / {} dBFS",
            kind.phrase(),
            pair.first_hz,
            pair.offset_hz,
            pair.first_hz.get() + pair.offset_hz.get(),
            pair.first_peak_dbfs,
            pair.second_peak_dbfs
        )
    } else {
        voice(form)
    }
}

fn voice(form: ComponentForm) -> String {
    let carrier = match form {
        ComponentForm::Tone { frequency_hz, .. } => format!(
            "{} {frequency_hz} Hz",
            auditorylab_core::CarrierKind::Tone.phrase()
        ),
        ComponentForm::Clicks {
            rate_hz,
            width_seconds,
            ..
        } => format!(
            "{} {rate_hz} Hz, {width_seconds} s wide",
            auditorylab_core::CarrierKind::Clicks.phrase()
        ),
        _ => unreachable!("primitive description"),
    };
    let mut text = format!(
        "{carrier}, peak {} dBFS, {}",
        form.get(auditorylab_core::ParameterId::PeakLevel)
            .unwrap_or_else(|| unreachable!("primitive level")),
        form.route()
            .unwrap_or_else(|| unreachable!("primitive route"))
            .phrase()
    );
    if let Some(modulation) = form.modulation() {
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

/// Derived descriptions of immutable snapshot content.
pub trait SnapshotText {
    /// Research and listening description, without retaining wording in document state.
    fn about(&self) -> Option<About>;
}
impl SnapshotText for auditorylab_app::document::recipe::RecipeSnapshot {
    fn about(&self) -> Option<About> {
        about(self.recipe())
    }
}
