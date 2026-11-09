//! Explicit schema-4 query and root values; semantic enum keys remain with their owners.
use auditorylab_app::{
    discovery::{
        collection::{CatalogQuery, DateRange},
        query::{Direction, GroupBy, RhythmState, Scope, SortBy},
    },
    services::library::{RootInfo, RootSource},
};
use auditorylab_core::{
    Timestamp,
    provenance::{ConditionRole, Population},
    rhythm::{self, Band, Method},
};
use serde::Serialize;
use std::path::PathBuf;
#[derive(Debug, Clone, Serialize)]
pub(super) struct Query {
    scope: Scope,
    include_external: bool,
    search: String,
    rhythm: Rhythm,
    studies: Vec<String>,
    roles: Vec<ConditionRole>,
    populations: Vec<Population>,
    analysis: Option<rhythm::Completeness>,
    created: Range,
    modified: Range,
    added: Range,
    sort: SortBy,
    direction: Option<Direction>,
    group_by: GroupBy,
}
#[derive(Debug, Clone, Serialize)]
struct Rhythm {
    bands: Vec<Band>,
    methods: Vec<Method>,
    state: Option<RhythmState>,
}
#[derive(Debug, Clone, Serialize)]
struct Range {
    from: Option<Timestamp>,
    before: Option<Timestamp>,
}
impl From<&DateRange> for Range {
    fn from(value: &DateRange) -> Self {
        Self {
            from: value.from,
            before: value.before,
        }
    }
}
impl From<&CatalogQuery> for Query {
    fn from(q: &CatalogQuery) -> Self {
        Self {
            scope: q.scope,
            include_external: q.include_external,
            search: q.search.clone(),
            rhythm: Rhythm {
                bands: q.rhythm.bands.clone(),
                methods: q.rhythm.methods.clone(),
                state: q.rhythm.state,
            },
            studies: q.studies.clone(),
            roles: q.roles.clone(),
            populations: q.populations.clone(),
            analysis: q.analysis,
            created: (&q.created).into(),
            modified: (&q.modified).into(),
            added: (&q.added).into(),
            sort: q.sort,
            direction: q.direction,
            group_by: q.group_by,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct Root {
    path: PathBuf,
    source: RootSource,
    exists: bool,
    layout: Option<u32>,
    initialized: bool,
}
impl From<RootInfo> for Root {
    fn from(value: RootInfo) -> Self {
        Self {
            path: value.path,
            source: value.source,
            exists: value.exists,
            layout: value.layout,
            initialized: value.initialized,
        }
    }
}
