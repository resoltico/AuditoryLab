//! Runtime library choices and atomic query edits.
use super::*;
use crate::core::query::{Direction, GroupBy, RhythmState, Scope, SortBy};
use crate::core::rhythm::{Band, Completeness, Method};
use crate::core::{ConditionRole, Population};
use crate::discovery::collection::CatalogQuery;

/// One library query facet, independent of control position or wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryFacet {
    Scope,
    Sort,
    Direction,
    Group,
    Band,
    Method,
    Population,
    Role,
    Analysis,
}
/// One offered semantic key and its current selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceOption {
    pub key: &'static str,
    pub label: &'static str,
    pub selected: bool,
}
/// Runtime options; absence/automatic choices are ordinary keyed options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChoiceView {
    pub options: Vec<ChoiceOption>,
}
/// Native filter entry captured together before date/query validation.
#[derive(Debug, Clone)]
pub struct FilterInput {
    pub study: String,
    pub facets: Vec<(QueryFacet, String)>,
    /// Created, modified, added; each pair is inclusive local from/through text.
    pub dates: [[String; 2]; 3],
}
impl Workspace {
    /// Current options for one library facet. Keys retain meaning across projections.
    #[must_use]
    pub fn library_choices(&self, facet: QueryFacet) -> ChoiceView {
        let q = self.catalog_query();
        let mut options = Vec::new();
        let mut option = |key, label, selected| {
            options.push(ChoiceOption {
                key,
                label,
                selected,
            })
        };
        macro_rules! values {
            ($kind:ty, $selected:expr) => {
                for value in <$kind>::ALL {
                    option(value.key(), value.label(), ($selected)(value));
                }
            };
        }
        match facet {
            QueryFacet::Scope => values!(Scope, |v| q.scope == v),
            QueryFacet::Sort => values!(SortBy, |v| q.sort == v),
            QueryFacet::Direction => {
                option("automatic", "Automatic direction", q.direction.is_none());
                values!(Direction, |v| q.direction == Some(v));
            }
            QueryFacet::Group => values!(GroupBy, |v| q.group_by == v),
            QueryFacet::Band => {
                option(
                    "any",
                    "All sounds",
                    q.rhythm.bands.is_empty() && q.rhythm.state.is_none(),
                );
                values!(Band, |v| q.rhythm.bands.contains(&v));
                values!(RhythmState, |v| q.rhythm.state == Some(v));
            }
            QueryFacet::Method => {
                option("any", "All methods", q.rhythm.methods.is_empty());
                values!(Method, |v| q.rhythm.methods.contains(&v));
            }
            QueryFacet::Population => {
                option("any", "Any population", q.populations.is_empty());
                values!(Population, |v| q.populations.contains(&v));
            }
            QueryFacet::Role => {
                option("any", "Any condition role", q.roles.is_empty());
                values!(ConditionRole, |v| q.roles.contains(&v));
            }
            QueryFacet::Analysis => {
                option("any", "Any analysis coverage", q.analysis.is_none());
                values!(Completeness, |v| q.analysis == Some(v));
            }
        }
        ChoiceView { options }
    }
    pub(super) fn edit_facets(&mut self, facets: Vec<(QueryFacet, String)>) -> Result<(), Refusal> {
        let mut query = self.catalog_query().clone();
        self.assign_facets(&mut query, facets)?;
        self.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))
    }
    fn assign_facets(
        &self,
        query: &mut CatalogQuery,
        facets: Vec<(QueryFacet, String)>,
    ) -> Result<(), Refusal> {
        for (facet, key) in facets {
            // Accept only actual offered keys, including the explicit absence key.
            if !self
                .library_choices(facet)
                .options
                .iter()
                .any(|option| option.key == key)
            {
                return Err(Refusal::SelectionChanged);
            }
            match facet {
                QueryFacet::Scope => {
                    query.scope = key.parse().map_err(|_| Refusal::SelectionChanged)?
                }
                QueryFacet::Sort => {
                    query.sort = key.parse().map_err(|_| Refusal::SelectionChanged)?
                }
                QueryFacet::Direction => query.direction = key.parse().ok(),
                QueryFacet::Group => {
                    query.group_by = key.parse().map_err(|_| Refusal::SelectionChanged)?
                }
                QueryFacet::Band => {
                    query.rhythm.bands = key.parse::<Band>().into_iter().collect();
                    query.rhythm.state = key.parse().ok();
                }
                QueryFacet::Method => query.rhythm.methods = key.parse().into_iter().collect(),
                QueryFacet::Population => query.populations = key.parse().into_iter().collect(),
                QueryFacet::Role => query.roles = key.parse().into_iter().collect(),
                QueryFacet::Analysis => query.analysis = key.parse().ok(),
            }
        }
        Ok(())
    }
    pub(super) fn edit_filters(&mut self, input: FilterInput) -> Result<(), Refusal> {
        let mut query = self.catalog_query().clone();
        let ranges = (|| {
            use crate::core::query::CalendarField;
            let [created, modified, added] = input.dates;
            Ok::<_, crate::discovery::query::QueryError>([
                crate::discovery::calendar::local_range(
                    CalendarField::Created,
                    &created[0],
                    &created[1],
                )?,
                crate::discovery::calendar::local_range(
                    CalendarField::Modified,
                    &modified[0],
                    &modified[1],
                )?,
                crate::discovery::calendar::local_range(
                    CalendarField::Added,
                    &added[0],
                    &added[1],
                )?,
            ])
        })();
        let [created, modified, added] = ranges.map_err(|error| {
            self.library_filter_error(error.clone());
            Refusal::Document(Box::new(error.into()))
        })?;
        query.created = created;
        query.modified = modified;
        query.added = added;
        query.studies = if input.study.trim().is_empty() {
            Vec::new()
        } else {
            vec![input.study]
        };
        self.assign_facets(&mut query, input.facets)?;
        self.apply(Command::Library(LibraryCommand::SetQuery(Box::new(query))))
    }
}
