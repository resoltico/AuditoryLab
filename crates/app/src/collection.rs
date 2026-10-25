//! Shared persistent/temporary collection summaries; no target compilation or I/O.
use crate::core::{DocumentUuid, Timestamp, nominal};
use crate::recipe::RecipeSnapshot;
use crate::{
    library::{LibraryKey, VersionToken},
    query::{self, SearchIndex},
};
use auditorylab_core::{ConditionRole, EvidenceScope, Measure, Population};
use serde::Serialize;

/// Storage ownership, independent of scientific source or authorship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Collection {
    /// Curated immutable shipped recipe.
    #[serde(rename = "builtin")]
    BuiltIn,
    /// Locally managed recipe; not necessarily user-authored or validated.
    User,
    /// Temporary opened content, excluded from persistent All by default.
    External,
}
/// One correlated source/condition facet; ancestor scope cannot assert current study.
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceFacet {
    /// Local bibliography key within this recipe.
    pub source: String,
    /// Normalized DOI identity when declared.
    pub doi: Option<String>,
    /// Current-condition versus ancestor scope.
    pub scope: EvidenceScope,
    /// Population of this same record.
    pub population: Population,
    /// Experimental role of this same record.
    pub role: ConditionRole,
    /// Measures of this condition, empty when not assessed.
    pub measures: Vec<Measure>,
}
/// Disposable index row; the managed envelope remains authoritative.
#[derive(Debug, Clone)]
pub struct EntrySummary {
    /// Stable key; never inferred from the title.
    pub key: LibraryKey,
    /// Portable identity, not a process snapshot number.
    pub document_id: DocumentUuid,
    /// Display fallback does not change the underlying portable document.
    pub title: String,
    /// Declared recipe creation, or Unknown.
    pub created_at: Option<Timestamp>,
    /// Declared recipe edit, or Unknown.
    pub modified_at: Option<Timestamp>,
    /// Local membership time, absent for built-in/external entries.
    pub added_at: Option<Timestamp>,
    /// Local Trash state, absent for active entries.
    pub trashed_at: Option<Timestamp>,
    /// Expected token for existing managed mutations.
    pub version_token: Option<VersionToken>,
    /// Cached pure nominal classification, with no device/export rate dependency.
    pub nominal: nominal::Analysis,
    /// Correlated typed evidence facets.
    pub evidence: Vec<EvidenceFacet>,
    /// Requested duration; finite but not necessarily renderable at any given rate.
    pub duration_seconds: f64,
    pub(crate) normalized_title: String,
    pub(crate) search: SearchIndex,
}
impl EntrySummary {
    /// Summarize captured content without realizing a signal or retaining its full recipe.
    #[must_use]
    pub fn from_snapshot(
        key: LibraryKey,
        snapshot: &RecipeSnapshot,
        local: Option<(Timestamp, Option<Timestamp>, VersionToken)>,
    ) -> Self {
        let recipe = snapshot.recipe();
        let title = snapshot.title();
        let (added_at, trashed_at, version_token) = local
            .map_or((None, None, None), |(added, trashed, token)| {
                (Some(added), trashed, Some(token))
            });
        let evidence = recipe
            .evidence
            .iter()
            .map(|e| EvidenceFacet {
                source: e.source.clone(),
                doi: recipe
                    .sources
                    .iter()
                    .find(|s| s.key == e.source)
                    .and_then(|s| s.doi.clone()),
                scope: e.scope,
                population: e.population,
                role: e.condition_role,
                measures: e.measures.clone(),
            })
            .collect();
        Self {
            key,
            document_id: recipe.document.id,
            normalized_title: query::normalize(&title),
            title,
            created_at: recipe.document.created_at,
            modified_at: recipe.document.modified_at,
            added_at,
            trashed_at,
            version_token,
            nominal: snapshot.nominal().clone(),
            evidence,
            duration_seconds: recipe.stimulus.duration_seconds,
            search: SearchIndex::from_snapshot(snapshot),
        }
    }
    /// Ownership derived exclusively from the runtime key.
    #[must_use]
    pub const fn collection(&self) -> Collection {
        match self.key {
            LibraryKey::BuiltIn(_) => Collection::BuiltIn,
            LibraryKey::User(_) => Collection::User,
            LibraryKey::External(_) => Collection::External,
        }
    }
    /// Shared token-AND search, including exact numeric terms and attributed prose.
    #[must_use]
    pub fn matches_search(&self, text: &str) -> bool {
        self.search.matches(text)
    }
}

/// Persistent All excludes temporary entries and Trash unless explicitly selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scope {
    /// Active Built-in and User entries.
    All,
    /// Curated immutable entries only.
    #[serde(rename = "builtin")]
    BuiltIn,
    /// Active managed entries only.
    User,
    /// Temporary Open files, not persisted by viewing.
    OpenFiles,
    /// Managed trashed records only.
    Trash,
}
/// Primary sort, with scope-dependent defaults only while Automatic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
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
impl std::str::FromStr for SortBy {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rate" => Ok(Self::Rate),
            "title" => Ok(Self::Title),
            "study" => Ok(Self::Study),
            "collection" => Ok(Self::Collection),
            "created" => Ok(Self::Created),
            "modified" => Ok(Self::Modified),
            "added" => Ok(Self::Added),
            _ => Err("expected rate, title, study, collection, created, modified or added"),
        }
    }
}
/// Direction applies only to the primary value; nulls and tie-breaks stay stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Ascending primary values.
    Asc,
    /// Descending primary values.
    Desc,
}
impl std::str::FromStr for Direction {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "asc" => Ok(Self::Asc),
            "desc" => Ok(Self::Desc),
            _ => Err("expected asc or desc"),
        }
    }
}
/// Optional repeated-key grouping; the flat result is always unique by key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
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
impl std::str::FromStr for GroupBy {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "band" => Ok(Self::Band),
            "method" => Ok(Self::Method),
            "study" => Ok(Self::Study),
            "collection" => Ok(Self::Collection),
            _ => Err("expected none, band, method, study or collection"),
        }
    }
}
/// Half-open explicit instants. Unknown dates never match a bounded interval.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DateRange {
    /// Inclusive instant.
    pub from: Option<Timestamp>,
    /// Exclusive instant.
    pub before: Option<Timestamp>,
}
impl DateRange {
    /// Verify strict ordering without treating an inverse range as empty results.
    ///
    /// # Errors
    /// Rejects before <= from.
    pub fn validate(&self) -> Result<(), crate::error::Error> {
        if self.from.zip(self.before).is_some_and(|(a, b)| b <= a) {
            Err(crate::error::Error::new(
                crate::error::ErrorKind::InvalidArgument,
                "date range requires before strictly after from",
            ))
        } else {
            Ok(())
        }
    }
    /// Compare instants precisely; no timezone-string or epoch substitution.
    #[must_use]
    pub fn contains(&self, value: Option<Timestamp>) -> bool {
        if self.from.is_none() && self.before.is_none() {
            return true;
        }
        value.is_some_and(|value| {
            self.from.is_none_or(|from| value >= from)
                && self.before.is_none_or(|before| value < before)
        })
    }
}
/// One correlated catalog query shared by Slint and ordinary CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogQuery {
    /// Persistent/temporary/Trash scope.
    pub scope: Scope,
    /// Explicit CLI temporary inputs may be added to All for that invocation.
    pub include_external: bool,
    /// Normalized token-AND search input.
    pub search: String,
    /// Correlated rhythmic facets.
    pub rhythm: query::RhythmQuery,
    /// DOI or local source-key alternatives, within one current evidence record.
    pub studies: Vec<String>,
    /// Role alternatives correlated to study/population on the same record.
    pub roles: Vec<ConditionRole>,
    /// Population alternatives correlated to study/role on the same record.
    pub populations: Vec<Population>,
    /// Optional nominal coverage restriction.
    pub analysis: Option<nominal::Completeness>,
    /// Declared creation filter.
    pub created: DateRange,
    /// Declared edit filter.
    pub modified: DateRange,
    /// Local membership filter.
    pub added: DateRange,
    /// Primary sort; explicit choices survive scope changes.
    pub sort: SortBy,
    /// Explicit direction, otherwise the sort's normal default.
    pub direction: Option<Direction>,
    /// Group memberships reference unique flat keys.
    pub group_by: GroupBy,
}
impl Default for CatalogQuery {
    fn default() -> Self {
        Self {
            scope: Scope::All,
            include_external: false,
            search: String::new(),
            rhythm: query::RhythmQuery::default(),
            studies: Vec::new(),
            roles: Vec::new(),
            populations: Vec::new(),
            analysis: None,
            created: DateRange::default(),
            modified: DateRange::default(),
            added: DateRange::default(),
            sort: SortBy::Automatic,
            direction: None,
            group_by: GroupBy::None,
        }
    }
}
/// A group references keys, never divergent duplicate entry objects.
#[derive(Debug, Clone, Serialize)]
pub struct ResultGroup {
    /// Stable group identifier.
    pub key: String,
    /// Human label.
    pub label: String,
    /// Unique keys within this group, in result order.
    pub keys: Vec<LibraryKey>,
}
/// Borrowed results cannot accidentally outlive or mutate their input generation.
#[derive(Debug)]
pub struct QueryResult<'a> {
    /// Each matching key once, in deterministic order.
    pub entries: Vec<&'a EntrySummary>,
    /// Optional overlapping virtual groups.
    pub groups: Vec<ResultGroup>,
}
impl QueryResult<'_> {
    /// Distinct sounds, never descriptor/group membership count.
    #[must_use]
    pub fn unique_count(&self) -> usize {
        self.entries.len()
    }
}
/// Alternative counts omit their own facet restriction and retain correlation.
#[derive(Debug, Clone, Serialize)]
pub struct FacetCounts {
    /// Five prominent reference-band alternatives.
    pub bands: Vec<(nominal::Band, usize)>,
    /// Structural method alternatives.
    pub methods: Vec<(nominal::Method, usize)>,
}
impl CatalogQuery {
    /// Clear restrictions while retaining collection, sort, direction and grouping.
    #[must_use]
    pub fn cleared_filters(&self) -> Self {
        Self {
            scope: self.scope,
            sort: self.sort,
            direction: self.direction,
            group_by: self.group_by,
            ..Self::default()
        }
    }

    /// Validate explicit interval and study token syntax before discovery/mutation.
    ///
    /// # Errors
    /// Rejects inverse dates or malformed/empty study selectors.
    pub fn validate(&self) -> Result<(), crate::error::Error> {
        self.created.validate()?;
        self.modified.validate()?;
        self.added.validate()?;
        for study in &self.studies {
            if study.trim().is_empty()
                || study == "key:"
                || study.chars().any(char::is_control)
                || (study.starts_with("10.")
                    || study.starts_with("https://doi.org/")
                    || study.starts_with("doi:"))
                    && auditorylab_core::provenance::normalize_doi(study).is_none()
            {
                return Err(crate::error::Error::new(
                    crate::error::ErrorKind::InvalidArgument,
                    "study must be a nonempty source key or valid DOI identity",
                ));
            }
        }
        Ok(())
    }
    /// Resolve Automatic using the current scope without overwriting an explicit sort.
    #[must_use]
    pub fn effective_sort(&self) -> (SortBy, Direction) {
        let sort = if self.sort == SortBy::Automatic {
            if matches!(self.scope, Scope::User | Scope::Trash) {
                SortBy::Modified
            } else {
                SortBy::Rate
            }
        } else {
            self.sort
        };
        let direction = self.direction.unwrap_or(
            if matches!(sort, SortBy::Created | SortBy::Modified | SortBy::Added) {
                Direction::Desc
            } else {
                Direction::Asc
            },
        );
        (sort, direction)
    }
    /// Test one immutable summary, without I/O, device discovery or rendering.
    #[must_use]
    pub fn matches(&self, entry: &EntrySummary) -> bool {
        let collection = entry.collection();
        let scope = match self.scope {
            Scope::All => {
                entry.trashed_at.is_none()
                    && (collection != Collection::External || self.include_external)
            }
            Scope::BuiltIn => collection == Collection::BuiltIn,
            Scope::User => collection == Collection::User && entry.trashed_at.is_none(),
            Scope::OpenFiles => collection == Collection::External,
            Scope::Trash => collection == Collection::User && entry.trashed_at.is_some(),
        };
        scope
            && entry.matches_search(&self.search)
            && self.rhythm.matches(&entry.nominal)
            && self
                .analysis
                .is_none_or(|a| entry.nominal.completeness == a)
            && self.created.contains(entry.created_at)
            && self.modified.contains(entry.modified_at)
            && self.added.contains(entry.added_at)
            && self.matches_evidence(entry)
    }
    fn matches_evidence(&self, entry: &EntrySummary) -> bool {
        if self.studies.is_empty() && self.roles.is_empty() && self.populations.is_empty() {
            return true;
        }
        entry.evidence.iter().any(|e| {
            e.scope == EvidenceScope::ReferenceCondition
                && (self.studies.is_empty()
                    || self.studies.iter().any(|s| {
                        s.strip_prefix("key:").unwrap_or(s) == e.source
                            || auditorylab_core::provenance::normalize_doi(s)
                                .is_some_and(|doi| e.doi.as_ref() == Some(&doi))
                    }))
                && (self.roles.is_empty() || self.roles.contains(&e.role))
                && (self.populations.is_empty() || self.populations.contains(&e.population))
        })
    }
    /// Query a generation, preserving unique keys and deterministic tie-breaks.
    ///
    /// # Errors
    /// Refuses invalid input ranges/tokens before producing an apparently empty result.
    pub fn execute<'a>(
        &self,
        input: &'a [EntrySummary],
    ) -> Result<QueryResult<'a>, crate::error::Error> {
        self.validate()?;
        let mut seen = std::collections::BTreeSet::new();
        let mut entries = input
            .iter()
            .filter(|entry| self.matches(entry) && seen.insert(entry.key.clone()))
            .collect::<Vec<_>>();
        entries.sort_by(|a, b| self.compare(a, b));
        let groups = self.groups(&entries);
        Ok(QueryResult { entries, groups })
    }
    fn compare(&self, a: &EntrySummary, b: &EntrySummary) -> std::cmp::Ordering {
        let (sort, direction) = self.effective_sort();
        let primary = match sort {
            SortBy::Automatic => unreachable!("effective_sort resolves automatic"),
            SortBy::Rate => optional_compare(
                rate(a, &self.rhythm),
                rate(b, &self.rhythm),
                direction,
                f64::total_cmp,
            ),
            SortBy::Title => directed(a.normalized_title.cmp(&b.normalized_title), direction),
            SortBy::Study => optional_compare(study(a), study(b), direction, Ord::cmp),
            SortBy::Collection => directed(a.collection().cmp(&b.collection()), direction),
            SortBy::Created => optional_compare(a.created_at, b.created_at, direction, Ord::cmp),
            SortBy::Modified => optional_compare(a.modified_at, b.modified_at, direction, Ord::cmp),
            SortBy::Added => optional_compare(a.added_at, b.added_at, direction, Ord::cmp),
        };
        primary
            .then_with(|| a.normalized_title.cmp(&b.normalized_title))
            .then_with(|| a.key.cmp(&b.key))
    }
    /// Count matching unique keys without sorting or constructing group memberships.
    ///
    /// # Errors
    /// Rejects the same invalid ranges/tokens as result execution.
    pub fn count(&self, input: &[EntrySummary]) -> Result<usize, crate::error::Error> {
        self.validate()?;
        Ok(input
            .iter()
            .filter(|e| self.matches(e))
            .map(|e| &e.key)
            .collect::<std::collections::BTreeSet<_>>()
            .len())
    }
    /// Disjunctive unique-key counts; no per-row target validity work.
    #[must_use]
    pub fn facet_counts(&self, input: &[EntrySummary]) -> FacetCounts {
        let count = |q: &Self| {
            input
                .iter()
                .filter(|e| q.matches(e))
                .map(|e| &e.key)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        };
        let bands = nominal::Band::ALL
            .into_iter()
            .map(|band| {
                let mut q = self.clone();
                q.rhythm.bands = vec![band];
                q.rhythm.state = None;
                (band, count(&q))
            })
            .collect();
        let methods = nominal::Method::ALL
            .into_iter()
            .map(|method| {
                let mut q = self.clone();
                q.rhythm.methods = vec![method];
                (method, count(&q))
            })
            .collect();
        FacetCounts { bands, methods }
    }
    fn groups(&self, entries: &[&EntrySummary]) -> Vec<ResultGroup> {
        let mut groups = std::collections::BTreeMap::<String, ResultGroup>::new();
        for entry in entries {
            let mut values = Vec::new();
            match self.group_by {
                GroupBy::None => {}
                GroupBy::Band => {
                    for band in &entry.nominal.bands {
                        values.push((band.key().into(), band.to_string()));
                    }
                    if entry.nominal.outside_bands() {
                        values.push(("other-rates".into(), "Other rates".into()));
                    }
                    if entry.nominal.no_rhythm() {
                        values.push(("no-rhythm".into(), "No rhythmic rate".into()));
                    }
                    if entry.nominal.multiple() {
                        values.push(("multiple-rhythms".into(), "Multiple rhythms".into()));
                    }
                    if entry.nominal.completeness != nominal::Completeness::Complete
                        || !entry.nominal.diagnostics.is_empty()
                    {
                        values.push(("needs-review".into(), "Needs review".into()));
                    }
                }
                GroupBy::Method => {
                    for method in &entry.nominal.methods {
                        values.push((method.key().into(), method.key().into()));
                    }
                    if values.is_empty() {
                        values.push(("none".into(), "No declared method".into()));
                    }
                }
                GroupBy::Study => {
                    for e in entry
                        .evidence
                        .iter()
                        .filter(|e| e.scope == EvidenceScope::ReferenceCondition)
                    {
                        let label = e.doi.as_deref().unwrap_or(&e.source).to_owned();
                        values.push((format!("study:{label}"), label));
                    }
                    if values.is_empty() {
                        values.push((
                            "no-current-study".into(),
                            "No current study declaration".into(),
                        ));
                    }
                }
                GroupBy::Collection => {
                    let label = match entry.collection() {
                        Collection::BuiltIn => "Built-in",
                        Collection::User => "User",
                        Collection::External => "Open files",
                    };
                    values.push((label.into(), label.into()));
                }
            }
            for (key, label) in values {
                let group = groups.entry(key.clone()).or_insert_with(|| ResultGroup {
                    key,
                    label,
                    keys: Vec::new(),
                });
                if !group.keys.contains(&entry.key) {
                    group.keys.push(entry.key.clone());
                }
            }
        }
        let mut groups = groups.into_values().collect::<Vec<_>>();
        if self.group_by == GroupBy::Band {
            let order = [
                "delta",
                "theta",
                "alpha",
                "beta",
                "gamma",
                "other-rates",
                "no-rhythm",
                "multiple-rhythms",
                "needs-review",
            ];
            groups.sort_by_key(|group| {
                order
                    .iter()
                    .position(|key| *key == group.key)
                    .unwrap_or(order.len())
            });
        }
        if self.group_by == GroupBy::Collection {
            groups.sort_by_key(|group| match group.key.as_str() {
                "Built-in" => 0,
                "User" => 1,
                _ => 2,
            });
        }
        groups
    }
}
fn directed(order: std::cmp::Ordering, direction: Direction) -> std::cmp::Ordering {
    if direction == Direction::Desc {
        order.reverse()
    } else {
        order
    }
}
fn optional_compare<T>(
    a: Option<T>,
    b: Option<T>,
    direction: Direction,
    compare: impl FnOnce(&T, &T) -> std::cmp::Ordering,
) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => directed(compare(&a, &b), direction),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}
fn rate(entry: &EntrySummary, query: &query::RhythmQuery) -> Option<f64> {
    entry
        .nominal
        .matching_rate(&query.bands, &query.methods)
        .or_else(|| entry.nominal.rhythms.iter().map(|r| r.hz).reduce(f64::min))
}
fn study(entry: &EntrySummary) -> Option<String> {
    entry
        .evidence
        .iter()
        .filter(|e| e.scope == EvidenceScope::ReferenceCondition)
        .map(|e| e.doi.as_deref().unwrap_or(&e.source).to_owned())
        .min()
}
