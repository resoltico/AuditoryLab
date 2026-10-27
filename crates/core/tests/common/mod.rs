//! Checked recipe construction for behavioral tests; no serialization or sample oracle.
use auditorylab_core::{
    Component, ComponentForm, ComponentId, DocumentIdentity, DocumentUuid, Recipe, Sound,
};

pub(crate) fn recipe(duration_seconds: f64, fade_seconds: f64, voices: &[ComponentForm]) -> Recipe {
    let sound = Sound {
        duration_seconds,
        fade_seconds,
        components: voices
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
    };
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
