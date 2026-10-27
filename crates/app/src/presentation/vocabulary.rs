//! Application vocabulary and stable discovery terms.
use crate::core::rhythm::Method;
use crate::presentation::DisplayText;

/// Structural search terms, independent of labels and explanatory prose.
#[must_use]
pub const fn method_search_terms(method: Method) -> &'static str {
    match method {
        Method::Binaural => "binaural interaural pair",
        Method::Monaural => "monaural pair",
        Method::Clicks => "click clicks",
        Method::Gate => "gate gated rectangular",
        Method::Am => "am amplitude modulation sinusoidal",
        Method::SteadyTone => "steady-tone steady tone",
    }
}

// The macro derives the UI interface inventory from the typed vocabulary.
macro_rules! vocabulary {
    ($($category:ident { $($variant:ident => ($property:literal, $label:expr)),* $(,)? })*) => {
        $(
            #[doc = concat!("Application-owned ", stringify!($category), " vocabulary.")]
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum $category {
                $(#[doc = concat!("The ", stringify!($variant), " term.") ] $variant,)*
            }
            impl $category {
                /// Display wording shared by the frontends.
                #[must_use]
                pub const fn label(self) -> &'static str {
                    match self { $(Self::$variant => $label,)* }
                }
                /// Accessible name when no more specific context is needed.
                #[must_use]
                pub const fn accessible_name(self) -> &'static str { self.label() }
            }
        )*
        /// Slint property names and their labels, derived from the typed terms.
        pub const UI_TERMS: &[(&str, &str)] = &[$($(($property, $category::$variant.label()),)*)*];
    }
}
vocabulary! {
    Action {
        StopCurrentAndPlayRecipe => ("action-stop-current-and-play-recipe", "Stop current & play recipe"),
        PlaySelectedSound => ("action-play-selected-sound", "Play selected sound"),
        PlayLive => ("action-play-live", "Play live"),
        StopAudio => ("action-stop-audio", "Stop audio"),
    }
    Facet {
        MixedPopulation => ("facet-mixed-population", crate::core::Population::Mixed.label()),
        UnspecifiedPopulation => ("facet-unspecified-population", crate::core::Population::Unspecified.label()),
    }
    Status {
        OtherRates => ("status-other-rates", crate::discovery::query::RhythmState::OutsideBands.label()),
        NoRhythmicRate => ("status-no-rhythmic-rate", crate::discovery::query::RhythmState::None.label()),
        MultipleRhythms => ("status-multiple-rhythms", crate::discovery::query::RhythmState::Multiple.label()),
        NeedsReview => ("status-needs-review", crate::discovery::query::RhythmState::NeedsReview.label()),
        Saved => ("status-saved", "Saved"),
        SavedWithWarning => ("status-saved-with-warning", "Saved with warnings"),
        Unchanged => ("status-unchanged", "Unchanged"),
        Unknown => ("status-unknown", "Unknown"),
        None => ("status-none", "None"),
        InventoryIncomplete => ("status-inventory-incomplete", "Incomplete inventory: valid results above do not establish an empty/complete library."),
        OperationIncomplete => ("status-operation-incomplete", "Operation not completed"),
        OperationConfirmed => ("status-operation-confirmed", "Operation confirmed"),
        WaitingForGeneratedAudio => ("status-waiting-for-generated-audio", "Waiting for generated audio"),
        Complete => ("status-complete", crate::core::rhythm::Completeness::Complete.label()),
        Invalid => ("status-invalid", crate::core::rhythm::Completeness::Invalid.label()),
        EmptyFilteredLibrary => ("status-empty-filtered-library", "No recipes match the current filters. Clear the restrictions to see every recipe."),
    }
    Caption {
        ReferenceCondition => ("caption-reference-condition", crate::core::EvidenceScope::ReferenceCondition.label()),
        Ancestor => ("caption-ancestor", crate::core::EvidenceScope::Ancestor.label()),
        Eeg => ("caption-eeg", crate::core::Measure::Eeg.label()),
        Behavior => ("caption-behavior", crate::core::Measure::Behavior.label()),
        SubjectiveReport => ("caption-subjective-report", crate::core::Measure::Subjective.label()),
        Sleep => ("caption-sleep", crate::core::Measure::Sleep.label()),
        Hearing => ("caption-hearing", crate::core::Measure::Hearing.label()),
        Other => ("caption-other", crate::core::Measure::Other.label()),
        Human => ("caption-human", crate::core::Population::Human.label()),
        Mouse => ("caption-mouse", crate::core::Population::Mouse.label()),
        OtherAnimal => ("caption-other-animal", crate::core::Population::OtherAnimal.label()),
        NotApplicable => ("caption-not-applicable", crate::core::Population::NotApplicable.label()),
        TestCondition => ("caption-test-condition", crate::core::ConditionRole::Test.label()),
        ComparisonCondition => ("caption-comparison-condition", crate::core::ConditionRole::Comparison.label()),
        MixedRoles => ("caption-mixed-roles", crate::core::ConditionRole::Mixed.label()),
        UnspecifiedRole => ("caption-unspecified-role", crate::core::ConditionRole::Unspecified.label()),
    }
}
impl Action {
    /// Labels for Library playback and deliberate session replacement.
    #[must_use]
    pub const fn library_play_labels() -> [&'static str; 2] {
        [
            Self::PlaySelectedSound.label(),
            Self::StopCurrentAndPlayRecipe.label(),
        ]
    }
    /// Labels needed to size the transport across its states.
    #[must_use]
    pub const fn transport_labels() -> [&'static str; 3] {
        [
            Self::PlaySelectedSound.label(),
            Self::PlayLive.label(),
            Self::StopAudio.label(),
        ]
    }
}

impl Action {
    /// Explanation for permanently deleting a trashed entry.
    #[must_use]
    pub fn purge_description(title: &str, key: &str) -> String {
        let title = (title).display_scalar();
        format!(
            "Delete “{title}”\n{key}\nThis permanently removes this trashed User entry. External files and independent copies remain unchanged."
        )
    }
    /// Save/discard prompt for the current document.
    #[must_use]
    pub fn unsaved_description(title: &str) -> String {
        format!("Save changes to “{title}” before continuing?")
    }
    /// Add-component label derived from its actual default role.
    #[must_use]
    pub fn add_component(kind: crate::core::CarrierKind) -> String {
        format!(
            "Add {}",
            crate::document::controls::default_component(kind)
                .role()
                .label()
                .to_lowercase()
        )
    }
}

/// Numeric display policy, separate from exact model values and input text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberStyle {
    /// Requested durations with meaningful units and explicit rounding.
    Duration,
    /// Whole elapsed transport time.
    Clock,
    /// Three significant digits for a waveform scale.
    AxisLimit,
    /// One-decimal digital level, with a silence symbol.
    Decibels,
    /// Compact significant digits with explicit approximation.
    Compact,
    /// Captured monitor window with two decimal places and its unit.
    CapturedMilliseconds,
    /// Available monitor time-window choices and their unit.
    Milliseconds,
    /// A sample-rate choice and its unit.
    Hertz,
}
