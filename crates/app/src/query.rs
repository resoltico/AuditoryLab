//! Shared nominal facets for all recipe origins, independent of audio validity.
use crate::core::rhythm::{Analysis, Band, Completeness, Method};
use serde::Serialize;

/// Special rhythm navigation; these properties can overlap band memberships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RhythmState {
    /// At least one positive rate outside the reference intervals.
    OutsideBands,
    /// Complete analysis with no rhythmic descriptors.
    None,
    /// At least two independent processes, even within one band.
    Multiple,
    /// Incomplete interpretation or document diagnostics.
    NeedsReview,
}
impl RhythmState {
    /// Shared name of the navigation state.
    #[must_use]
    pub const fn label(self) -> &'static str {
        use crate::vocabulary::Status;
        match self {
            Self::OutsideBands => Status::OtherRates.label(),
            Self::None => Status::NoRhythmicRate.label(),
            Self::Multiple => Status::MultipleRhythms.label(),
            Self::NeedsReview => Status::NeedsReview.label(),
        }
    }
}
impl core::str::FromStr for RhythmState {
    type Err = &'static str;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "outside-bands" => Ok(Self::OutsideBands),
            "none" => Ok(Self::None),
            "multiple" => Ok(Self::Multiple),
            "needs-review" => Ok(Self::NeedsReview),
            _ => Err("expected outside-bands, none, multiple or needs-review"),
        }
    }
}
/// OR within each facet; simultaneous band/method restrictions match one process.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RhythmQuery {
    /// Reference memberships. Empty means any.
    pub bands: Vec<Band>,
    /// Structural methods. Empty means any.
    pub methods: Vec<Method>,
    /// An optional special navigation constraint.
    pub state: Option<RhythmState>,
}
impl RhythmQuery {
    /// Test a cached nominal analysis, without target compilation or rendering.
    #[must_use]
    pub fn matches(&self, analysis: &Analysis) -> bool {
        analysis.matches(&self.bands, &self.methods)
            && match self.state {
                None => true,
                Some(RhythmState::OutsideBands) => analysis.outside_bands(),
                Some(RhythmState::None) => analysis.no_rhythm(),
                Some(RhythmState::Multiple) => analysis.multiple(),
                Some(RhythmState::NeedsReview) => {
                    analysis.completeness != Completeness::Complete
                        || !analysis.diagnostics.is_empty()
                }
            }
    }
}

/// Unicode-normalized lowercase text used consistently by catalog search.
#[must_use]
pub fn normalize(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfkc().flat_map(char::to_lowercase).collect()
}

/// Compact deduplicated terms, derived once per immutable content revision.
#[derive(Debug, Clone)]
pub(crate) struct SearchIndex {
    terms: Vec<String>,
    numbers: Vec<f64>,
}
impl SearchIndex {
    pub(crate) fn from_snapshot(snapshot: &crate::recipe::RecipeSnapshot) -> Self {
        use crate::core::{Carrier, ComponentForm, Ears, Modulation, ParameterId};
        let recipe = snapshot.recipe();
        let mut facts = vec![recipe.document().id.to_string()];
        facts.extend(recipe.document().created_at.iter().map(ToString::to_string));
        facts.extend(
            recipe
                .document()
                .modified_at
                .iter()
                .map(ToString::to_string),
        );
        match snapshot.origin() {
            crate::recipe::Origin::BuiltIn(id) => facts.push((*id).into()),
            crate::recipe::Origin::User { id, .. } => facts.push(id.to_string()),
            crate::recipe::Origin::File(path) => facts.push(path.to_string_lossy().into_owned()),
            crate::recipe::Origin::Stream(label) | crate::recipe::Origin::Draft(label) => {
                facts.push(label.clone())
            }
        }
        if let Some(description) = &recipe.description() {
            facts.push(description.title.clone());
            facts.extend(description.notes.iter().cloned());
        }
        for ancestor in recipe.lineage() {
            facts.extend([ancestor.title.clone(), ancestor.note.clone()]);
            facts.extend(ancestor.catalog_id.iter().cloned());
            facts.extend(ancestor.document_id.iter().map(ToString::to_string));
        }
        for source in recipe.sources() {
            facts.extend([
                source.key.clone(),
                source.label.clone(),
                source.citation.clone(),
            ]);
            facts.extend(source.url.iter().cloned());
            facts.extend(source.doi.iter().cloned());
        }
        for evidence in recipe.evidence() {
            facts.extend([
                evidence.source.clone(),
                evidence.methods_locator.clone(),
                evidence.summary.clone(),
                evidence.limitations.clone(),
                evidence.adaptation.clone(),
            ]);
            facts.extend(evidence.population_detail.iter().cloned());
            // Serialized enum keys describe the facts, independent of presentation.
            facts.push(serde_json::to_string(&evidence.population).unwrap_or_default());
            facts.push(serde_json::to_string(&evidence.condition_role).unwrap_or_default());
            facts.push(serde_json::to_string(&evidence.measures).unwrap_or_default());
        }
        for method in &snapshot.nominal().methods {
            facts.push(crate::vocabulary::method_search_terms(*method).into());
        }
        for band in &snapshot.nominal().bands {
            facts.push(band.key().into());
        }
        let mut numbers = vec![recipe.sound().duration_seconds, recipe.sound().fade_seconds];
        for component in &recipe.sound().components {
            let ears = match component.form {
                ComponentForm::Voice(voice) => {
                    facts.push(
                        match voice.carrier {
                            Carrier::Tone { .. } => "tone carrier frequency",
                            Carrier::Clicks { .. } => "click clicks rate width",
                        }
                        .into(),
                    );
                    if let Some(modulation) = voice.modulation {
                        facts.push(
                            match modulation {
                                Modulation::Am { .. } => {
                                    "sinusoidal amplitude modulation am depth rate"
                                }
                                Modulation::Gate { .. } => "rectangular gate rate width",
                            }
                            .into(),
                        );
                    }
                    voice.ears
                }
                ComponentForm::BinauralPair(_) => {
                    facts.push("pair left right".into());
                    Ears::Left
                }
                ComponentForm::MonauralPair { ears, .. } => {
                    facts.push("pair".into());
                    ears
                }
            };
            facts.push(
                match ears {
                    Ears::Left => "left",
                    Ears::Right => "right",
                    Ears::Both => "both ears",
                }
                .into(),
            );
            for parameter in ParameterId::COMPONENT {
                if let Some(value) = component.form.read(parameter) {
                    numbers.extend([value, parameter.descriptor().unit.to_display(value)]);
                }
            }
            if let Some(pair) = component.form.pair() {
                numbers.push(pair.first_hz + pair.offset_hz);
            }
        }
        numbers.extend(snapshot.nominal().rhythms.iter().map(|r| r.hz));
        let text = normalize(&facts.join(" "));
        numbers.extend(numeric_terms(&text));
        numbers.retain(|n| n.is_finite());
        let terms = text
            .split_whitespace()
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        Self { terms, numbers }
    }
    pub(crate) fn matches(&self, query: &str) -> bool {
        let query = normalize(query);
        query
            .split_whitespace()
            .all(|term| match term.parse::<f64>() {
                Ok(n) if n.is_finite() => self.numbers.contains(&n),
                _ => self.terms.iter().any(|word| word.contains(term)),
            })
    }
}

// Numeric terms are complete tokens/quantities, never fragments of identifiers.
fn numeric_terms(text: &str) -> Vec<f64> {
    text.split(|c: char| {
        c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | ':' | '·')
    })
    .filter_map(|term| {
        let term = term.trim_matches(['\"', '\'']).trim_end_matches('.');
        let number = term.parse::<f64>().ok().or_else(|| {
            ["hz", "ms", "dbfs", "db", "s", "%"]
                .into_iter()
                .find_map(|unit| term.strip_suffix(unit)?.parse::<f64>().ok())
        })?;
        number.is_finite().then_some(number)
    })
    .collect()
}
