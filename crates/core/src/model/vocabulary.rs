//! Shared terms and their stable serialized or command-line keys.
use core::fmt;
/// A heading label and the corresponding wording inside a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Term {
    /// Wording used for a heading or choice.
    pub label: &'static str,
    /// Wording used within a sentence.
    pub phrase: &'static str,
}
/// One finite vocabulary with stable keys and owned labels.
pub trait Vocabulary: Copy + 'static {
    /// Every supported value in display order.
    const ALL: &'static [Self];
    /// This value's heading and sentence wording.
    fn term(self) -> Term;
    /// Stable serialized or command-line key.
    fn key(self) -> &'static str;
}
/// An input does not name a supported vocabulary value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VocabularyError(&'static str);
impl VocabularyError {
    pub(crate) const fn new(code: &'static str) -> Self {
        Self(code)
    }
}
impl fmt::Display for VocabularyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl core::error::Error for VocabularyError {}
macro_rules! vocabulary {
    ($name:ident,$kind:literal { $($variant:ident => ($key:expr,$label:expr,$phrase:expr)),+ $(,)? }) => {
        impl $name {
            /// Every supported value in display order.
            pub const ALL:[Self;[$(stringify!($variant)),+].len()]=[$(Self::$variant),+];
            /// Heading and sentence wording owned by this vocabulary.
            #[must_use]
            pub const fn term(self)->$crate::vocabulary::Term {match self {$(Self::$variant=>$crate::vocabulary::Term {label:$label,phrase:$phrase}),+}}
            /// Heading or choice label.
            #[must_use]
            pub const fn label(self)->&'static str {self.term().label}
            /// Wording inside a sentence.
            #[must_use]
            pub const fn phrase(self)->&'static str {self.term().phrase}
            /// Stable serialized or command-line key.
            #[must_use]
            pub const fn key(self)->&'static str {match self {$(Self::$variant=>$key),+}}
        }
        impl $crate::vocabulary::Vocabulary for $name {
            const ALL:&'static[Self]=&Self::ALL;
            fn term(self)->$crate::vocabulary::Term {self.term()}
            fn key(self)->&'static str {self.key()}
        }
        impl serde::Serialize for $name {
            fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error> {
                serializer.serialize_str(self.key())
            }
        }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D:serde::Deserializer<'de>>(deserializer:D)->Result<Self,D::Error> {
                let key=<alloc::string::String as serde::Deserialize>::deserialize(deserializer)?;
                key.parse().map_err(serde::de::Error::custom)
            }
        }
        impl core::str::FromStr for $name {
            type Err=$crate::vocabulary::VocabularyError;
            fn from_str(text:&str)->Result<Self,Self::Err> {
                Self::ALL.into_iter().find(|value|value.key()==text).ok_or($crate::vocabulary::VocabularyError::new(concat!("vocabulary.",$kind)))
            }
        }
    };
}
pub(crate) use vocabulary;

/// Shared action labels used by both frontends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Stop current & play recipe.
    StopCurrentAndPlayRecipe,
    /// Play selected sound.
    PlaySelectedSound,
    /// Play live.
    PlayLive,
    /// Stop audio.
    StopAudio,
}
vocabulary! { Action,"action" {
    StopCurrentAndPlayRecipe => ("stop_current_and_play_recipe","Stop current & play recipe","stop current & play recipe"),
    PlaySelectedSound => ("play_selected_sound","Play selected sound","play selected sound"),
    PlayLive => ("play_live","Play live","play live"),
    StopAudio => ("stop_audio","Stop audio","stop audio"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{collections::BTreeSet, format};
    use serde::Serialize;
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
        assert!(error.0.starts_with("vocabulary."));
        assert!(
            error
                .0
                .bytes()
                .all(|ch| ch.is_ascii_lowercase() || ch == b'.' || ch == b'_')
        );
    }
    macro_rules! checks {($($test:ident:$kind:ty),* $(,)?)=>{$(#[test] fn $test(){check::<$kind>();})*};}
    checks! {
        ears:crate::Ears,ear:crate::Ear,pair_kind:crate::PairKind,slot_role:crate::SlotRole,
        parameters:crate::ParameterId,carrier_kind:crate::CarrierKind,modulation_kind:crate::ModulationKind,
        population:crate::Population,condition_role:crate::ConditionRole,evidence_scope:crate::EvidenceScope,measure:crate::Measure,
        band:crate::rhythm::Band,method:crate::rhythm::Method,completeness:crate::rhythm::Completeness,
        scope:crate::query::Scope,sort:crate::query::SortBy,direction:crate::query::Direction,group:crate::query::GroupBy,rhythm_state:crate::query::RhythmState,
        calendar_field:crate::query::CalendarField,action:Action,status:Status,
    }
}
