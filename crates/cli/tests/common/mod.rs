//! Recipes for behavior tests, independent of the portable text format.
use auditorylab_app::core::{DocumentIdentity, DocumentUuid, Recipe};

pub(crate) fn silent(seconds: f64, fade: f64) -> Recipe {
    let mut recipe = Recipe::builder(DocumentIdentity {
        id: DocumentUuid::from_random_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
        created_at: None,
        modified_at: None,
    });
    recipe.content.sound.set_duration(finite(seconds));
    recipe.content.sound.set_fade(finite(fade));
    match recipe.build() {
        Ok(recipe) => recipe,
        Err(error) => panic!("invalid test recipe: {error}"),
    }
}

const fn finite(value: f64) -> auditorylab_app::core::Finite {
    match auditorylab_app::core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}
