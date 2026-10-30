//! Application-owned teaching choices and illustrative rates; core owns band membership.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
use crate::core::rhythm::Band;

/// Revisit-able orientation destinations, independent of document identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Overview,
    Library,
    PitchAndRhythm,
    SoundTypes,
    RhythmBands,
}
impl Topic {
    /// The application-owned reading and navigation order.
    pub const ALL: [Self; 5] = [
        Self::Overview,
        Self::Library,
        Self::PitchAndRhythm,
        Self::SoundTypes,
        Self::RhythmBands,
    ];
    /// Semantic page identity, independent of menu position.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Library => "library",
            Self::PitchAndRhythm => "pitch-and-rhythm",
            Self::SoundTypes => "sound-types",
            Self::RhythmBands => "rhythm-bands",
        }
    }
    /// Current position in the application-owned order.
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|topic| *topic == self)
            .unwrap_or_else(|| unreachable!("every topic has an order"))
    }
}

mod visuals;
pub use visuals::{Construction, TeachingVisual};

/// Current learning destination. The workspace separately retains its working view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LearningState {
    pub topic: Topic,
    pub band: Band,
    pub construction: Construction,
}
/// Explicit examples, constructed by existing application constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Example {
    PulsedTone,
}
impl Example {
    /// Actual application-owned construction, without generating a document identity.
    pub fn form(self) -> crate::core::ComponentForm {
        match self {
            Self::PulsedTone => crate::document::construction::CreationKind::PulsedTone.component(),
        }
    }

    /// Capture complete content and fresh identity at activation time.
    pub fn recipe(self) -> Result<crate::core::Recipe, crate::services::OperationError> {
        match self {
            Self::PulsedTone => crate::document::construction::new_recipe(
                crate::document::construction::CreationKind::PulsedTone,
            ),
        }
    }
}
/// Typed requests; display wording is presentation-owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearningCommand {
    Open(Topic),
    SelectBand(Band),
    SelectConstruction(Construction),
    ReturnToWork,
    BrowseLibrary,
    CreateExample(Example),
}
/// One illustrative sinusoid, never an audio carrier or a band definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandExample {
    pub band: Band,
    pub example_hz: f64,
}
/// Read the same factual input used by the optional artwork exporter.
pub fn band_examples() -> Vec<BandExample> {
    #[derive(serde::Deserialize)]
    struct Examples {
        examples: Vec<Entry>,
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        band: String,
        example_hz: f64,
    }
    let examples: Examples = serde_json::from_str(include_str!("band_examples.json"))
        .unwrap_or_else(|_| unreachable!("tracked learning examples are tested"));
    examples
        .examples
        .into_iter()
        .map(|entry| BandExample {
            band: match entry.band.as_str() {
                "delta" => Band::Delta,
                "theta" => Band::Theta,
                "alpha" => Band::Alpha,
                "beta" => Band::Beta,
                "gamma" => Band::Gamma,
                _ => unreachable!("tracked band names are tested"),
            },
            example_hz: entry.example_hz,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn illustrative_rates_and_boundaries_match_core_independently() {
        let examples = band_examples();
        assert_eq!(
            examples.iter().map(|e| e.example_hz).collect::<Vec<_>>(),
            [2., 6., 10., 20., 32.]
        );
        assert_eq!(
            examples.iter().map(|e| e.band).collect::<Vec<_>>(),
            [
                Band::Delta,
                Band::Theta,
                Band::Alpha,
                Band::Beta,
                Band::Gamma
            ]
        );
        for example in examples {
            assert_eq!(Band::of(example.example_hz), Some(example.band));
        }
        for (rate, band) in [
            (0.1, Band::Delta),
            (4., Band::Theta),
            (8., Band::Alpha),
            (13., Band::Alpha),
            (30., Band::Beta),
            (80., Band::Gamma),
        ] {
            assert_eq!(Band::of(rate), Some(band));
        }
        assert_eq!(Band::of(13.001), Some(Band::Beta));
        assert_eq!(Band::of(30.001), Some(Band::Gamma));
        assert_eq!(Band::of(80.001), None);
        assert_eq!(
            [
                Band::Delta,
                Band::Theta,
                Band::Alpha,
                Band::Beta,
                Band::Gamma
            ]
            .map(Band::interval),
            ["0.1–<4 Hz", "4–<8 Hz", "8–13 Hz", ">13–30 Hz", ">30–80 Hz"]
        );
    }
}

#[cfg(test)]
mod artwork_contract {
    use crate::core::rhythm::Band;

    fn ranges_match_core(svg: &str) -> bool {
        [
            Band::Delta,
            Band::Theta,
            Band::Alpha,
            Band::Beta,
            Band::Gamma,
        ]
        .into_iter()
        .all(|band| {
            let lettering = band
                .interval()
                .replace('–', "-")
                .replace("Hz", "HZ")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let marker = format!("data-lettering=\"{lettering}\"");
            let Some((_, group)) = svg.split_once(&marker) else {
                return false;
            };
            let Some((group, _)) = group.split_once("</g>") else {
                return false;
            };
            let rendered: Option<String> = group
                .split("xlink:href=\"#g")
                .skip(1)
                .map(|reference| {
                    reference
                        .split_once('"')?
                        .0
                        .parse::<u32>()
                        .ok()
                        .and_then(char::from_u32)
                })
                .collect();
            let expected = band
                .interval()
                .replace('–', "-")
                .replace("Hz", "HZ")
                .replace(' ', "");
            rendered.as_deref() == Some(&expected)
                && svg.contains(&format!(
                    "data-lettering=\"{}\"",
                    band.to_string().to_uppercase()
                ))
        })
    }

    #[test]
    fn exported_and_master_ribbon_lettering_matches_compiled_core_intervals() {
        for svg in [
            include_str!("../../../../artwork/onboarding/assets/spectral-ribbon-light.svg"),
            include_str!("../../../../artwork/onboarding/assets/spectral-ribbon-dark.svg"),
            include_str!("../../../../artwork/onboarding/source/spectral-ribbon.svg.in"),
        ] {
            assert!(ranges_match_core(svg));
            assert!(
                !ranges_match_core(&svg.replacen("xlink:href=\"#g52\"", "xlink:href=\"#g53\"", 1)),
                "wrong rendered glyph must fail"
            );
            assert!(
                !ranges_match_core(&svg.replace("&gt;30-80 HZ", "30-80 HZ")),
                "wrong Gamma inclusion must fail"
            );
        }
    }
}
