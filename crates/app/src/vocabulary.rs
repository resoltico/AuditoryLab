//! Application operations, statuses and discovery absence terms.
use auditorylab_core::{vocabulary, vocabulary::Term};
/// Every sound after non-rhythm restrictions.
pub const ALL_SOUNDS: Term = Term {
    label: "All sounds",
    phrase: "all sounds",
};
/// Absence of a structural-method restriction.
pub const ALL_METHODS: Term = Term {
    label: "All methods",
    phrase: "all methods",
};
/// Absence of a population restriction.
pub const ANY_POPULATION: Term = Term {
    label: "Any population",
    phrase: "any population",
};
/// Absence of a condition-role restriction.
pub const ANY_CONDITION_ROLE: Term = Term {
    label: "Any condition role",
    phrase: "any condition role",
};
/// Absence of an analysis-coverage restriction.
pub const ANY_ANALYSIS_COVERAGE: Term = Term {
    label: "Any analysis coverage",
    phrase: "any analysis coverage",
};
/// Use the query sort’s normal direction.
pub const AUTOMATIC_DIRECTION: Term = Term {
    label: "Automatic direction",
    phrase: "automatic direction",
};
/// Shared operation keys and terms for commands, reports and controls.
///
/// Transport-specific choices live here as operations to avoid another enum
/// that would duplicate these labels in the application view layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Operation {
    /// List.
    List,
    /// List Trash.
    ListTrash,
    /// Describe.
    Describe,
    /// Inspect.
    Inspect,
    /// Portable recipe.
    Recipe,
    /// Export WAV.
    Export,
    /// Play.
    Play,
    /// Stop audio.
    Stop,
    /// Library location.
    LibraryPath,
    /// Initialize configured library.
    LibraryInit,
    /// Add to User Library.
    LibraryAdd,
    /// Update saved recipe.
    LibraryUpdate,
    /// Rename saved recipe.
    LibraryRename,
    /// Move saved entry to Trash.
    LibraryTrash,
    /// Restore from Trash.
    LibraryRestore,
    /// Permanently delete.
    LibraryPurge,
    /// Check library.
    LibraryCheck,
    /// Refresh.
    LibraryRefresh,
    /// Read saved recipe.
    LibraryRead,
    /// New Sound.
    NewSound,
    /// Edit a copy.
    EditCopy,
    /// Save sound.
    Save,
    /// Save independent copy.
    SaveCopy,
    /// Open Recipe.
    OpenRecipe,
    /// Export portable recipe.
    ExportRecipe,
    /// Stop current & play recipe.
    StopCurrentAndPlayRecipe,
    /// Play selected sound.
    PlaySelectedSound,
    /// Play live.
    PlayLive,
    /// Edit saved User recipe.
    EditSavedRecipe,
    /// Edit external file.
    EditExternalFile,
    /// Close open file.
    CloseOpenFile,
    /// Show saved.
    ShowSaved,
    /// Cancel export.
    CancelExport,
    /// Add component.
    AddComponent,
    /// Undo remove.
    UndoRemove,
    /// Remove.
    RemoveComponent,
}
vocabulary! {Operation,"operation" {
    List => ("list","List","list"),
    ListTrash => ("list_trash","List Trash","list Trash"),
    Describe => ("describe","Describe","describe"),
    Inspect => ("inspect","Inspect","inspect"),
    Recipe => ("recipe","Portable recipe","portable recipe"),
    Export => ("export","Export WAV","export WAV"),
    Play => ("play","Play","play"),
    Stop => ("stop","Stop audio","stop audio"),
    LibraryPath => ("library_path","Library location","library location"),
    LibraryInit => ("library_init","Initialize configured library","initialize configured library"),
    LibraryAdd => ("library_add","Add to User Library","add to User Library"),
    LibraryUpdate => ("library_update","Update saved recipe","update saved recipe"),
    LibraryRename => ("library_rename","Rename saved recipe","rename saved recipe"),
    LibraryTrash => ("library_trash","Move saved entry to Trash","move saved entry to Trash"),
    LibraryRestore => ("library_restore","Restore from Trash","restore from Trash"),
    LibraryPurge => ("library_purge","Permanently delete","permanently delete"),
    LibraryCheck => ("library_check","Check library","check library"),
    LibraryRefresh => ("library_refresh","Refresh","refresh"),
    LibraryRead => ("library_read","Read saved recipe","read saved recipe"),
    NewSound => ("new_sound","New Sound","new Sound"),
    EditCopy => ("edit_copy","Edit a copy","edit a copy"),
    Save => ("save","Save sound","save sound"),
    SaveCopy => ("save_copy","Save independent copy","save independent copy"),
    OpenRecipe => ("open_recipe","Open Recipe","open Recipe"),
    ExportRecipe => ("export_recipe","Export portable recipe","export portable recipe"),
    StopCurrentAndPlayRecipe => ("stop_current_and_play_recipe","Stop current & play recipe","stop current & play recipe"),
    PlaySelectedSound => ("play_selected_sound","Play selected sound","play selected sound"),
    PlayLive => ("play_live","Play live","play live"),
    EditSavedRecipe => ("edit_saved_recipe","Edit saved User recipe","edit saved User recipe"),
    EditExternalFile => ("edit_external_file","Edit external file","edit external file"),
    CloseOpenFile => ("close_open_file","Close open file","close open file"),
    ShowSaved => ("show_saved","Show saved","show saved"),
    CancelExport => ("cancel_export","Cancel export","cancel export"),
    AddComponent => ("add_component","Add component","add component"),
    UndoRemove => ("undo_remove","Undo remove","undo remove"),
    RemoveComponent => ("remove_component","Remove","remove"),
}}

/// Shared status labels used by both frontends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Saved.
    Saved,
    /// Saved with warnings.
    SavedWithWarning,
    /// Unchanged.
    Unchanged,
    /// Unknown.
    Unknown,
    /// None.
    None,
    /// Operation not completed.
    OperationIncomplete,
    /// Operation confirmed.
    OperationConfirmed,
    /// Waiting for generated audio.
    WaitingForGeneratedAudio,
}
vocabulary! { Status,"status" {
    Saved => ("saved","Saved","saved"),
    SavedWithWarning => ("saved_with_warning","Saved with warnings","saved with warnings"),
    Unchanged => ("unchanged","Unchanged","unchanged"),
    Unknown => ("unknown","Unknown","unknown"),
    None => ("none","None","none"),
    OperationIncomplete => ("operation_incomplete","Operation not completed","operation not completed"),
    OperationConfirmed => ("operation_confirmed","Operation confirmed","operation confirmed"),
    WaitingForGeneratedAudio => ("waiting_for_generated_audio","Waiting for generated audio","waiting for generated audio"),
}}

macro_rules! vocabularies {($consumer:ident)=>{$consumer! {
        scope:crate::discovery::query::Scope,sort:crate::discovery::query::SortBy,direction:crate::discovery::query::Direction,group:crate::discovery::query::GroupBy,rhythm_state:crate::discovery::query::RhythmState,
        calendar_field:crate::discovery::query::CalendarField,operation:Operation,status:Status,
}};}
macro_rules! registry {($($name:ident:$kind:ty),* $(,)?)=>{pub(crate) const CODES:&[&str]=&[$(<$kind>::INVALID_CODE),*];};}
vocabularies!(registry);

#[cfg(test)]
mod tests {
    use super::*;
    use auditorylab_core::Code;
    use auditorylab_core::vocabulary::{Vocabulary, VocabularyError};
    use serde::Serialize;
    use std::collections::BTreeSet;
    fn check<
        T: Vocabulary
            + Serialize
            + serde::de::DeserializeOwned
            + core::str::FromStr<Err = VocabularyError>
            + core::fmt::Debug
            + Eq,
    >() {
        #[derive(Serialize, serde::Deserialize)]
        struct Wire<T> {
            value: T,
        }
        let mut keys = BTreeSet::new();
        assert!(!T::ALL.is_empty());
        for value in T::ALL.iter().copied() {
            assert!(keys.insert(value.key()), "duplicate key {}", value.key());
            assert!(
                value
                    .key()
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_lowercase)
            );
            assert!(
                value.key().split('_').all(|word| !word.is_empty()
                    && word
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())),
                "not snake_case: {}",
                value.key()
            );
            assert!(!value.term().label.is_empty() && !value.term().phrase.is_empty());
            let serialized = toml::to_string(&Wire { value })
                .unwrap_or_else(|error| panic!("vocabulary serialization: {error}"));
            assert_eq!(serialized, format!("value = {:?}\n", value.key()));
            assert_eq!(value.key().parse::<T>(), Ok(value));
            assert_eq!(toml::from_str::<Wire<T>>(&serialized).unwrap().value, value);
        }
        let error = "__unknown__".parse::<T>().unwrap_err();
        assert!(error.code().starts_with("vocabulary."));
        assert!(
            error
                .code()
                .bytes()
                .all(|ch| ch.is_ascii_lowercase() || ch == b'.' || ch == b'_')
        );
    }
    macro_rules! checks {($($test:ident:$kind:ty),* $(,)?)=>{$(#[test] fn $test(){check::<$kind>();})*};}
    vocabularies!(checks);
}
