//! Recipes for behavior tests, independent of the portable text format.
use auditorylab_app::core::{DocumentIdentity, DocumentUuid, Recipe};

pub(crate) fn silent(seconds: f64, fade: f64) -> Recipe {
    let mut recipe = Recipe::builder(DocumentIdentity {
        id: DocumentUuid::from_random_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
        created_at: None,
        modified_at: None,
    });
    recipe.sound.duration_seconds = seconds;
    recipe.sound.fade_seconds = fade;
    match recipe.build() {
        Ok(recipe) => recipe,
        Err(error) => panic!("invalid test recipe: {error}"),
    }
}
