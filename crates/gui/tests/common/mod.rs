//! Recipes for behavior tests, independent of the portable text format.
use auditorylab::core::{DocumentIdentity, DocumentUuid, Recipe};

pub(crate) fn silent(seconds: f64, fade: f64) -> Recipe {
    let mut recipe = Recipe::silent(DocumentIdentity {
        id: DocumentUuid::from_random_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
        created_at: None,
        modified_at: None,
    });
    recipe.sound.duration_seconds = seconds;
    recipe.sound.fade_seconds = fade;
    recipe
}
