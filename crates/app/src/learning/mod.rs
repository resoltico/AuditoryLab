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

/// Primary research/context references offered by the guide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearningSource {
    EegGlossary,
    MemoryStudy,
    SleepStages,
}
impl LearningSource {
    pub const ALL: [Self; 3] = [Self::EegGlossary, Self::MemoryStudy, Self::SleepStages];
    /// The source record, independent of the application's release documentation.
    pub const fn url(self) -> &'static str {
        match self {
            Self::EegGlossary => "https://pmc.ncbi.nlm.nih.gov/articles/PMC6123891/",
            Self::MemoryStudy => "https://pubmed.ncbi.nlm.nih.gov/16837600/",
            Self::SleepStages => "https://www.nhlbi.nih.gov/health/sleep/stages-of-sleep",
        }
    }
}

/// Current learning destination. The workspace separately retains its working view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LearningState {
    pub topic: Topic,
    pub band: Band,
    pub construction: Construction,
}
impl Default for LearningState {
    fn default() -> Self {
        Self {
            topic: Topic::Overview,
            band: Band::Alpha,
            construction: Construction::SteadyTone,
        }
    }
}
/// Typed requests; display wording is presentation-owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LearningCommand {
    /// Reopen the session’s last guide location, without resetting its choices.
    Resume,
    Open(Topic),
    SelectBand(Band),
    SelectConstruction(Construction),
    BrowseLibrary,
    CreateExample(Construction),
}
/// One illustrative sinusoid, never an audio carrier or a band definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandExample {
    pub band: Band,
    pub example_hz: f64,
}
impl BandExample {
    /// One second of a normalized, zero-phase sine schematic, including both ends.
    /// These are teaching marks, never rendered audio or measured EEG.
    pub fn normalized_wave(self) -> Vec<f64> {
        (0..=1024)
            .map(|index| {
                (std::f64::consts::TAU * self.example_hz * f64::from(index) / 1024.0).sin()
            })
            .collect()
    }
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
    fn schematic_samples_have_independent_zero_crossing_and_quarter_cycle_checks() {
        for example in band_examples() {
            let wave = example.normalized_wave();
            assert_eq!(wave.len(), 1025);
            assert!(wave[0].abs() < 1e-12 && wave[1024].abs() < 1e-12);
            assert!(
                wave.iter()
                    .all(|value| value.is_finite() && value.abs() <= 1.0)
            );
            let upward = wave
                .windows(2)
                .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
                .count();
            assert_eq!(
                f64::from(u32::try_from(upward).unwrap()),
                example.example_hz
            );
        }
        let delta = band_examples()[0].normalized_wave();
        assert!((delta[128] - 1.0).abs() < 1e-12);
        assert!(delta[256].abs() < 1e-12);
        assert!((delta[384] + 1.0).abs() < 1e-12);
    }

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
