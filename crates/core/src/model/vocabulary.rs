//! Shared terms and their stable serialized or command-line keys.
use alloc::{format, string::String};
/// A heading label and the corresponding wording inside a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Term {
    /// Wording used for a heading or choice.
    pub label: &'static str,
    /// Wording used within a sentence.
    pub phrase: &'static str,
}
/// Document title, including query/control labels for the same field.
pub const TITLE: Term = Term {
    label: "Title",
    phrase: "title",
};
/// A document modification date in compact history displays.
pub const EDITED: Term = Term {
    label: "Edited",
    phrase: "edited",
};
/// Applicability wording shared by document dates and evidence declarations.
pub const NOT_APPLICABLE: Term = Term {
    label: "Not applicable",
    phrase: "not applicable",
};
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
    #[doc(hidden)]
    pub const fn new(code: &'static str) -> Self {
        Self(code)
    }
}
impl core::fmt::Display for VocabularyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}
impl core::error::Error for VocabularyError {}
/// Declare stable keys and wording alongside their owning type.
#[macro_export]
macro_rules! vocabulary {
    ($name:ident,$kind:literal { $($variant:ident => ($key:expr,$label:expr,$phrase:expr)),+ $(,)? }) => {
        impl $name {
            #[doc(hidden)]
            pub const INVALID_CODE:&'static str=concat!("vocabulary.",$kind);
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
                Self::ALL.into_iter().find(|value|value.key()==text).ok_or($crate::vocabulary::VocabularyError::new(Self::INVALID_CODE))
            }
        }
    };
}
pub(crate) use crate::vocabulary;

impl crate::Code for VocabularyError {
    fn code(&self) -> &'static str {
        self.0
    }
}
macro_rules! vocabularies {($consumer:ident)=>{$consumer! {
        ears:crate::Ears,ear:crate::Ear,pair_kind:crate::PairKind,slot_role:crate::SlotRole,
        parameters:crate::ParameterId,carrier_kind:crate::CarrierKind,modulation_kind:crate::ModulationKind,
        population:crate::Population,condition_role:crate::ConditionRole,evidence_scope:crate::EvidenceScope,measure:crate::Measure,
        band:crate::rhythm::Band,method:crate::rhythm::Method,completeness:crate::rhythm::Completeness,

}};}
macro_rules! registry {($($name:ident:$kind:ty),* $(,)?)=>{pub(crate) const CODES:&[&str]=&[$(<$kind>::INVALID_CODE),*];};}
vocabularies!(registry);

/// Persisted title of an untitled authored sound.
pub const UNTITLED_SOUND: &str = "Untitled sound";
/// Persisted title of an independent editable copy.
#[must_use]
pub fn copy_title(title: &str) -> String {
    format!("{title} (edited)")
}
/// Attribution content persisted into an independent copy.
#[must_use]
pub fn copy_note(recipe: &crate::Recipe) -> String {
    recipe.description().and_then(|description|description.notes.as_deref()).filter(|notes|!notes.is_empty()).map_or_else(
        ||"Attributed copy; source findings concern the ancestor, not this sound.".into(),
        |notes|format!("Attributed copy; source findings concern the ancestor, not this sound. Ancestor notes: {notes}"),
    )
}
/// The absence of authored amplitude modulation.
pub const NO_MODULATION: Term = Term {
    label: "None",
    phrase: "no modulation",
};

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
    vocabularies!(checks);
}

/// Heading for the SoundFacts domain concept.
pub const SOUND: Term = Term {
    label: "Sound",
    phrase: "Sound",
};

/// Heading for the NominalRhythmReferenceBand domain concept.
pub const NOMINAL_RHYTHM_REFERENCE_BAND: Term = Term {
    label: "Nominal rhythm reference band",
    phrase: "Nominal rhythm reference band",
};

/// Heading for the StructuralMethod domain concept.
pub const STRUCTURAL_METHOD: Term = Term {
    label: "Structural method",
    phrase: "Structural method",
};

/// Heading for the Ears domain concept.
pub const EARS: Term = Term {
    label: "Ears",
    phrase: "Ears",
};

/// Heading for the Carrier domain concept.
pub const CARRIER: Term = Term {
    label: "Carrier",
    phrase: "Carrier",
};

/// Heading for the Modulation domain concept.
pub const MODULATION: Term = Term {
    label: "Modulation",
    phrase: "Modulation",
};

/// The generated audio's left channel, independent of authored routing.
pub const LEFT_CHANNEL: Term = Term {
    label: "Left",
    phrase: "left",
};
/// The generated audio's right channel, independent of authored routing.
pub const RIGHT_CHANNEL: Term = Term {
    label: "Right",
    phrase: "right",
};
