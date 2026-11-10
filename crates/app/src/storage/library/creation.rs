//! A final managed-creation candidate, captured before admission can replace an editor.
use super::LibraryError;
use crate::core::{Recipe, RecipeError, Timestamp};
use std::sync::Arc;

/// Complete new managed content and the single observation that dates its creation.
/// The recipe already includes its final creation/modification fields and byte validation.
#[derive(Debug, Clone)]
#[must_use = "the prepared creation must be published or explicitly abandoned"]
pub struct Creation {
    recipe: Arc<Recipe>,
    observed: Timestamp,
}
impl Creation {
    /// Capture current creation dates and validate the final canonical recipe.
    /// This is operation admission, never an availability query or draft-opening action.
    ///
    /// # Errors
    /// Refuses an unsupported clock, invalid metadata or oversized final bytes.
    pub fn prepare(recipe: Arc<Recipe>) -> Result<Self, LibraryError> {
        Self::at(recipe, crate::clock::observed_now()?)
    }
    fn at(recipe: Arc<Recipe>, observed: Timestamp) -> Result<Self, LibraryError> {
        let recipe = dated(Arc::unwrap_or_clone(recipe), observed)
            .map_err(|error| LibraryError::Recipe { error })?;
        Ok(Self {
            recipe: Arc::new(recipe),
            observed,
        })
    }
    /// Captured immutable content; publication must not rewrite its identity or dates.
    #[must_use]
    pub fn recipe(&self) -> &Arc<Recipe> {
        &self.recipe
    }
    pub(super) const fn observed(&self) -> Timestamp {
        self.observed
    }
    /// Side-effect-free final creation check for an existing undated editor candidate.
    pub(crate) fn readiness(recipe: &Recipe) -> Result<(), RecipeError> {
        dated(recipe.clone(), date_shape()).map(|_| ())
    }
    /// Exact managed-copy field-shape preview, without observing time or allocating identities.
    pub(crate) fn copy_preview(
        source: &Recipe,
        catalog_id: Option<String>,
        note: String,
        title: String,
    ) -> Result<(), RecipeError> {
        // Timestamp's supported UTC profile always emits a four-digit year and nine
        // fractional digits. The independently pinned representation tests protect this plan.
        let shape = date_shape();
        source.copy_preview(Some(shape), Some(shape), catalog_id, note, title)
    }
}

fn dated(recipe: Recipe, observed: Timestamp) -> Result<Recipe, RecipeError> {
    let mut candidate = recipe.into_builder();
    candidate.document.created_at = Some(observed);
    candidate.document.modified_at = Some(observed);
    let recipe = candidate.build()?;
    recipe.to_toml()?;
    Ok(recipe)
}
fn date_shape() -> Timestamp {
    Timestamp::from_unix(0, 0)
        .unwrap_or_else(|| unreachable!("Unix epoch is in the portable timestamp profile"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn worker_publishes_the_exact_prepared_candidate_and_single_captured_observation() {
        let directory = tempfile::tempdir().unwrap();
        let library =
            crate::services::library::Library::at_path(directory.path().join("library")).unwrap();
        assert_eq!(library.initialize().unwrap().warnings, []);
        let observed: Timestamp = "2001-02-03T04:05:06.123456789Z".parse().unwrap();
        let original = Arc::new(crate::core::test_support::silent(2.0, 0.0));
        let prepared = Creation::at(original.clone(), observed).unwrap();
        let captured = prepared.recipe().clone();
        assert_eq!(original.document().created_at, None);
        let outcome = library
            .apply(
                crate::services::library::Mutation::Create(prepared),
                &crate::job::Cancel::default(),
            )
            .unwrap();
        let record = outcome.record.unwrap();
        assert_eq!(record.snapshot.recipe(), captured.as_ref());
        assert_eq!(record.recipe_toml.as_ref(), captured.to_toml().unwrap());
        assert_eq!(record.added_at, Some(observed));
        assert_eq!(captured.document().created_at, Some(observed));
        assert_eq!(captured.document().modified_at, Some(observed));
    }
}
