//! Complete list composition and query execution, shared by both frontends.
use super::{
    Cancel,
    library::{DiagnosticDetail, LibraryContext, LibraryDiagnostic},
};
use crate::core::query::Scope;
use crate::document::origin::LibraryKey;
use std::{collections::HashMap, path::PathBuf};
use {
    crate::discovery::collection::CatalogQuery, crate::discovery::collection::EntrySummary,
    crate::discovery::collection::ResultGroup, crate::discovery::query::QueryError,
    crate::outcome::Outcome, crate::services::OperationError,
};
/// Explicit query and temporary input files. No discovery read initializes a root.
#[derive(Debug)]
pub struct ListRequest {
    /// Committed typed query.
    pub query: CatalogQuery,
    /// Temporary bounded regular inputs, allowed only in All.
    pub files: Vec<PathBuf>,
}
/// An owned, executed query with publication-independent completeness evidence.
#[derive(Debug)]
pub struct Listing {
    query: CatalogQuery,
    entries: Vec<EntrySummary>,
    groups: Vec<ResultGroup>,
    complete: bool,
    diagnostics: Vec<LibraryDiagnostic>,
    stages: Vec<std::path::PathBuf>,
    outcome: Option<Outcome>,
}
impl Listing {
    /// Exact query used for these rows.
    #[must_use]
    pub const fn query(&self) -> &CatalogQuery {
        &self.query
    }
    /// Each matching key once, in query order.
    #[must_use]
    pub fn entries(&self) -> &[EntrySummary] {
        &self.entries
    }
    /// Virtual group memberships; repetitions do not increase the unique count.
    #[must_use]
    pub fn groups(&self) -> &[ResultGroup] {
        &self.groups
    }
    /// Whether requested discovery was complete.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }
    /// Retained storage evidence, including exact portable failures.
    #[must_use]
    pub fn diagnostics(&self) -> &[LibraryDiagnostic] {
        &self.diagnostics
    }
    /// Owned staging artifacts retained for recovery.
    #[must_use]
    pub fn stages(&self) -> &[std::path::PathBuf] {
        &self.stages
    }
    /// Frontend-independent incomplete/cancellation outcome.
    #[must_use]
    pub const fn outcome(&self) -> Option<Outcome> {
        self.outcome
    }
}
/// Compose built-ins, managed inventory and explicit files, then execute one query.
///
/// # Errors
/// Rejects invalid query/file composition and invalid explicit inputs. Root failure
/// remains a typed incomplete diagnostic while healthy Built-in rows remain available.
pub fn list(
    mut request: ListRequest,
    context: &LibraryContext,
    cancel: &Cancel,
) -> Result<Listing, OperationError> {
    request.query.validate()?;
    if cancel.is_requested() {
        return Err(crate::storage::library::LibraryError::Cancelled.into());
    }
    if !request.files.is_empty() && request.query.scope != Scope::All {
        return Err(QueryError::TemporaryFilesScope {
            scope: request.query.scope,
        }
        .into());
    }
    let mut entries = if matches!(request.query.scope, Scope::User | Scope::Trash) {
        Vec::new()
    } else {
        super::catalog()?
            .into_iter()
            .map(|snapshot| {
                let crate::document::recipe::Origin::BuiltIn(id) = snapshot.origin() else {
                    unreachable!("catalog loader retains each embedded Built-in origin")
                };
                EntrySummary::from_snapshot(
                    LibraryKey::BuiltIn(
                        crate::core::catalog::CatalogId::new(id)
                            .unwrap_or_else(|| unreachable!("validated catalog name")),
                    ),
                    &snapshot,
                    None,
                )
            })
            .collect()
    };
    let (mut complete, mut outcome, mut diagnostics, mut stages) =
        (true, None, Vec::new(), Vec::new());
    if request.query.scope != Scope::BuiltIn {
        match context.library() {
            Ok(library) => {
                let inventory = library.scan(cancel);
                entries.extend(inventory.entries);
                complete = inventory.complete;
                outcome = inventory.failure_kind;
                diagnostics = inventory.diagnostics;
                stages = inventory.stages;
            }
            Err(
                error @ (crate::storage::library::LibraryError::InvalidBuiltin { .. }
                | crate::storage::library::LibraryError::DuplicateBuiltin { .. }),
            ) => return Err(error.into()),
            Err(error) => {
                complete = false;
                outcome = Some(Outcome::IncompleteInventory);
                diagnostics.push(LibraryDiagnostic {
                    storage_id: "configured root".into(),

                    detail: DiagnosticDetail::Failure(Box::new(error)),
                });
            }
        }
    }
    request.query.include_external = !request.files.is_empty();
    for path in request.files {
        let resolved = super::resolve(super::Source::File(path), context)?;
        entries.push(resolved.summary);
    }
    let result = request.query.execute(&entries)?;
    let keys: Vec<_> = result
        .entries
        .iter()
        .map(|entry| entry.key.clone())
        .collect();
    let groups = result.groups;
    // Move selected rows rather than cloning potentially large evidence summaries.
    // Every key comes from the borrowed query result over this same input generation.
    let mut by_key: HashMap<_, _> = entries
        .into_iter()
        .map(|entry| (entry.key.clone(), entry))
        .collect();
    let entries = keys
        .into_iter()
        .map(|key| {
            by_key
                .remove(&key)
                .unwrap_or_else(|| unreachable!("query results refer to unique input keys"))
        })
        .collect();
    Ok(Listing {
        query: request.query,
        entries,
        groups,
        complete,
        diagnostics,
        stages,
        outcome,
    })
}
