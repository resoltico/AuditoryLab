//! Shared nominal facets for all recipe origins, independent of audio validity.
use crate::core::rhythm::{Analysis, Band, Completeness, Method};
use serde::Serialize;

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
    pub(crate) fn from_snapshot(snapshot: &crate::document::recipe::RecipeSnapshot) -> Self {
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
            crate::document::recipe::Origin::BuiltIn(id) => facts.push((*id).into()),
            crate::document::recipe::Origin::User { id, .. } => facts.push(id.to_string()),
            crate::document::recipe::Origin::File(path) => {
                facts.push(path.to_string_lossy().into_owned());
            }
            crate::document::recipe::Origin::Stream(label)
            | crate::document::recipe::Origin::Draft(label) => facts.push(label.clone()),
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
            facts.push(method.key().into());
            facts.push(method_search_terms(*method).into());
        }
        for band in &snapshot.nominal().bands {
            facts.push(band.key().into());
        }
        let mut numbers = vec![
            recipe.sound().duration_seconds().get(),
            recipe.sound().fade_seconds().get(),
        ];
        for component in recipe.sound().components() {
            component_terms(component, &mut facts, &mut numbers);
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

use crate::outcome::Outcome;
/// A discovery filter or local-date range is invalid.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryError {
    /// Explicit temporary inputs require the All collection.
    TemporaryFilesScope {
        scope: crate::discovery::query::Scope,
    },
    /// The exclusive upper bound is not after the lower bound.
    RangeOrder {
        from: crate::core::Timestamp,
        before: crate::core::Timestamp,
    },
    /// A study identity is empty or malformed.
    Study { value: String },
    /// A local date or midnight cannot be resolved uniquely.
    LocalDate { value: String },
    /// A local through date predates its from date.
    LocalOrder {
        field: crate::discovery::query::CalendarField,
        from: String,
        through: String,
    },
}
impl QueryError {
    /// Failure category.
    #[must_use]
    pub const fn outcome(&self) -> Outcome {
        match self {
            Self::TemporaryFilesScope { .. }
            | Self::RangeOrder { .. }
            | Self::Study { .. }
            | Self::LocalDate { .. }
            | Self::LocalOrder { .. } => Outcome::InvalidArgument,
        }
    }
}

/// Structural search terms, independent of labels and explanatory prose.
#[must_use]
const fn method_search_terms(method: Method) -> &'static str {
    match method {
        Method::Binaural => "binaural interaural pair",
        Method::Monaural => "monaural pair",
        Method::Clicks => "click clicks",
        Method::Gate => "gate gated rectangular",
        Method::Am => "am amplitude modulation sinusoidal",
        Method::SteadyTone => "steady tone",
    }
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for QueryError {}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    QueryError {
        codes {
            Self::TemporaryFilesScope { .. } => "query.temporary_files_scope",
            Self::RangeOrder { .. } => "query.range_order",
            Self::Study { .. } => "query.study",
            Self::LocalDate { .. } => "query.local_date",
            Self::LocalOrder { .. } => "query.local_order",
        }
        delegates {}
        params(this) {
            match this {
                Self::TemporaryFilesScope { scope } => {
                    crate::core::diagnostic_params! {"scope"=>scope.key()}
                }
                Self::RangeOrder { from, before } => {
                    crate::core::diagnostic_params! {
                        "from"=>from.to_string(),
                        "before"=>before.to_string(),
                    }
                }
                Self::Study { value } | Self::LocalDate { value } => {
                    crate::core::diagnostic_params! {"value"=>value.clone()}
                }
                Self::LocalOrder {
                    field,
                    from,
                    through,
                } => {
                    crate::core::diagnostic_params! {
                        "field"=>field.key(),
                        "from"=>from.clone(),
                        "through"=>through.clone(),
                    }
                }
            }
        }
    }
}

/// Persistent All excludes temporary entries and Trash unless explicitly selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Active Built-in and User entries.
    All,
    /// Curated immutable entries only.
    BuiltIn,
    /// Active managed entries only.
    User,
    /// Temporary Open files, not persisted by viewing.
    OpenFiles,
    /// Managed trashed records only.
    Trash,
}

/// Primary sort, with scope-dependent defaults only while Automatic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    /// Rate ascending for All/Built-in; modified descending for User.
    Automatic,
    /// Matching nominal rate.
    Rate,
    /// Normalized title.
    Title,
    /// Current declared study identity.
    Study,
    /// Storage ownership.
    Collection,
    /// Declared creation instant.
    Created,
    /// Declared edit instant.
    Modified,
    /// Local Added here instant.
    Added,
}

/// Direction applies only to the primary value; nulls and tie-breaks stay stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Ascending primary values.
    Asc,
    /// Descending primary values.
    Desc,
}

/// Optional repeated-key grouping; the flat result is always unique by key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBy {
    /// No groups.
    None,
    /// Reference bands/special rhythm states.
    Band,
    /// Structural methods.
    Method,
    /// Current-condition studies.
    Study,
    /// Ownership.
    Collection,
}

/// Special rhythm navigation; these properties can overlap band memberships.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

auditorylab_core::vocabulary! { Scope,"scope" {
    All => ("all","All · Built-in + User","all · Built-in + User"),
    BuiltIn => ("builtin","Built-in · Read only","built-in · Read only"),
    User => ("user","User","user"),
    OpenFiles => ("open_files","Open files · Temporary","open files · Temporary"),
    Trash => ("trash","Trash","trash"),
}}

auditorylab_core::vocabulary! { SortBy,"sort_by" {
    Automatic => ("automatic","Automatic order","automatic order"),
    Rate => ("rate","Nominal rate","nominal rate"),
    Title => ("title",auditorylab_core::vocabulary::TITLE.label,
        auditorylab_core::vocabulary::TITLE.phrase),
    Created => ("created",CalendarField::Created.label(),CalendarField::Created.phrase()),
    Modified => ("modified","Modified","modified"),
    Added => ("added",CalendarField::Added.label(),CalendarField::Added.phrase()),
    Study => ("study","Study","study"),
    Collection => ("collection","Collection","collection"),
}}

auditorylab_core::vocabulary! { Direction,"direction" {
    Asc => ("asc","Ascending","ascending"),
    Desc => ("desc","Descending","descending"),
}}

auditorylab_core::vocabulary! { GroupBy,"group_by" {
    None => ("none","No grouping","no grouping"),
    Band => ("band","Band groups","band groups"),
    Method => ("method","Method groups","method groups"),
    Study => ("study","Study groups","study groups"),
    Collection => ("collection","Collection groups","collection groups"),
}}

auditorylab_core::vocabulary! { RhythmState,"rhythm_state" {
    OutsideBands => ("outside_bands","Other rates","other rates"),
    None => ("none","No rhythmic rate","no rhythmic rate"),
    Multiple => ("multiple","Multiple rhythms","multiple rhythms"),
    NeedsReview => ("needs_review","Needs review","needs review"),
}}

/// Calendar-date field pair being edited by a native date-range control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarField {
    /// Portable recipe creation instants.
    Created,
    /// Portable recipe modification instants.
    Modified,
    /// Directory-owned library membership instants.
    Added,
}
auditorylab_core::vocabulary! {CalendarField,"calendar_field" {
    Created=>("created","Created","created"),
    Modified=>("modified","Modified","modified"),
    Added=>("added","Added here","added here"),
}}

fn component_terms(
    component: &crate::core::Component,
    facts: &mut Vec<String>,
    numbers: &mut Vec<f64>,
) {
    use crate::core::{Ears, Modulation, ParameterId};

    let form = component.form;
    if let Some(kind) = form.primitive_kind() {
        facts.push(
            match kind {
                crate::core::CarrierKind::Tone => "tone carrier frequency",
                crate::core::CarrierKind::Clicks => "click clicks rate width",
            }
            .into(),
        );
        if let Some(modulation) = form.modulation() {
            facts.push(
                match modulation {
                    Modulation::Am { .. } => "sinusoidal amplitude modulation am depth rate",
                    Modulation::Gate { .. } => "rectangular gate rate width",
                }
                .into(),
            );
        }
    } else {
        facts.push(
            if form.route().is_none() {
                "pair left right"
            } else {
                "pair"
            }
            .into(),
        );
    }
    if let Some(ears) = form.route() {
        facts.push(
            match ears {
                Ears::Left => "left",
                Ears::Right => "right",
                Ears::Both => "both ears",
            }
            .into(),
        );
    }
    for parameter in ParameterId::COMPONENT {
        if let Some(value) = component.form.get(parameter) {
            let value = value.get();
            numbers.extend([value, parameter.descriptor().unit.to_display(value)]);
        }
    }
    if let Some(pair) = component.form.pair() {
        numbers.push(pair.first_hz.get() + pair.offset_hz.get());
    }
}
