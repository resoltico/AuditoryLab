//! Shared nominal facets for all recipe origins, independent of audio validity.
use crate::core::nominal::{Analysis, Band, Completeness, Method};
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

/// Search the stable identity, current notes, attributed sources and nominal
/// parameters. Text terms are ANDed; numeric terms never use substring matching.
#[must_use]
pub fn search(snapshot: &crate::recipe::RecipeSnapshot, query: &str) -> bool {
    SearchIndex::from_snapshot(snapshot).matches(query)
}

/// Compact deduplicated terms, derived once per immutable content revision.
#[derive(Debug, Clone)]
pub(crate) struct SearchIndex {
    terms: Vec<String>,
    numbers: Vec<f64>,
}
impl SearchIndex {
    pub(crate) fn from_snapshot(snapshot: &crate::recipe::RecipeSnapshot) -> Self {
        let text = normalize(&format!(
            "{} {} {} {} {} {}",
            snapshot.origin(),
            snapshot.title(),
            crate::presentation::document_notes(snapshot),
            crate::presentation::research(snapshot)
                .iter()
                .map(|(_, value)| *value)
                .collect::<Vec<_>>()
                .join(" "),
            snapshot
                .stimulus()
                .components
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" "),
            crate::presentation::nominal_description(snapshot.nominal())
        ));
        let numbers = numeric_terms(&text);
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
