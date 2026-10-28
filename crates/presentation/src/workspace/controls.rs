//! Shared interaction wording.

/// Explanation for permanently deleting a trashed entry.
#[must_use]
pub fn purge_description(title: &str, key: &str) -> String {
    let title = crate::display_scalar(title);
    format!(
        "Delete “{title}”\n{key}\nThis permanently removes this trashed User \
            entry. External files and independent copies remain unchanged."
    )
}

/// Save/discard prompt for the current document.
#[must_use]
pub fn unsaved_description(title: &str) -> String {
    format!("Save changes to “{title}” before continuing?")
}

/// Add-component label derived from its actual default role.
#[must_use]
pub fn add_component(kind: auditorylab_core::CarrierKind) -> String {
    format!(
        "Add {}",
        auditorylab_app::workspace::controls::default_component(kind)
            .heading()
            .to_lowercase()
    )
}

/// Add-pair wording rendered from the owning construction term.
#[must_use]
pub fn add_pair(kind: auditorylab_core::PairKind) -> String {
    format!("Add {}", kind.phrase())
}
