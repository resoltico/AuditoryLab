//! Checked recipe construction for behavioral tests; no serialization or sample oracle.
use auditorylab_core::{
    Component, ComponentForm, ComponentId, DocumentIdentity, DocumentUuid, Recipe, Sound,
};

pub(crate) fn recipe(duration_seconds: f64, fade_seconds: f64, voices: &[ComponentForm]) -> Recipe {
    let sound = Sound::new(
        finite(duration_seconds),
        finite(fade_seconds),
        voices
            .iter()
            .enumerate()
            .map(|(index, voice)| Component {
                id: match ComponentId::numbered(index) {
                    Some(id) => id,
                    None => panic!("behavioral fixture exceeds the owner range"),
                },
                form: *voice,
            })
            .collect(),
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
    match Recipe::silent(DocumentIdentity {
        id: DocumentUuid::from_random_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
        created_at: None,
        modified_at: None,
    })
    .with_sound(sound)
    {
        Ok(recipe) => recipe,
        Err(error) => panic!("invalid behavioral fixture: {error}"),
    }
}

const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}
