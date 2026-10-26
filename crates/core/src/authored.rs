// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Ordered authored components and lossless lowering into primitive generator lanes.
use crate::{Carrier, ComponentId, Ears, Lane, Modulation, Voice, VoiceId};
use alloc::vec::Vec;
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A signed-offset two-tone construction. The second carrier is first + offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    /// First carrier; left for a binaural pair.
    pub first_hz: f64,
    /// Authored signed nominal difference; never recovered from generated carriers.
    pub offset_hz: f64,
    /// First/left tone's peak bound.
    pub first_peak_dbfs: f64,
    /// Second/right tone's peak bound.
    pub second_peak_dbfs: f64,
}
/// One authored primitive or explicit two-tone construction, without an identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ComponentForm {
    /// Existing raw tone/click and optional modulation semantics.
    Voice(Voice),
    /// First tone exclusively left; second tone exclusively right.
    BinauralPair(Pair),
    /// Both tones have this identical ear route; unequal levels remain explicit.
    MonauralPair {
        /// Ear route shared by both tones.
        ears: Ears,
        /// Carrier/difference/level values.
        pair: Pair,
    },
}
/// An authored form and its positive stable owner identity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Component {
    /// Authored owner, independent of generator lanes.
    pub id: ComponentId,
    /// Authored sound structure.
    pub form: ComponentForm,
}
/// Coupled carrier/offset failure, independent of sample-rate Nyquist checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairIssue {
    /// A requested carrier, offset or computed carrier is nonfinite.
    Nonfinite,
    /// One computed carrier is zero or negative.
    Nonpositive,
    /// A nonzero authored offset disappears during floating-point addition.
    UnrepresentablePairDifference,
}
impl fmt::Display for PairIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Nonfinite => "pair carrier/offset arithmetic is nonfinite",
            Self::Nonpositive => "both pair carriers must be positive",
            Self::UnrepresentablePairDifference => "nonzero offset collapses to the first carrier",
        })
    }
}
/// Why authored nodes cannot be lowered into a bounded primitive target.
#[derive(Debug, Clone, PartialEq)]
pub enum LoweringError {
    /// Duplicate, zero or already-expanded identities are not authored owner IDs.
    Identity {
        /// Repeated authored owner.
        id: ComponentId,
    },
    /// Authored form or expanded voice count exceeds the established budget.
    VoiceBudget {
        /// Expanded generator count.
        count: usize,
    },
    /// Too many authored components.
    ComponentBudget {
        /// Authored component count.
        count: usize,
    },
    /// The first carrier and signed offset cannot produce the intended second tone.
    PairDifference {
        /// Failure in the coupled first carrier and signed offset.
        issue: PairIssue,
        /// Authored owner.
        id: ComponentId,
        /// Requested first carrier.
        first_hz: f64,
        /// Requested signed difference.
        offset_hz: f64,
    },
}
impl fmt::Display for LoweringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identity { id } => write!(f, "two authored components share identity {id}"),
            Self::ComponentBudget { count } => write!(
                f,
                "{count} authored components exceed the limit of {}",
                Sound::MAX_COMPONENTS
            ),
            Self::VoiceBudget { count } => write!(
                f,
                "{count} expanded voices exceed the limit of {}",
                Sound::MAX_VOICES
            ),
            Self::PairDifference {
                issue,
                id,
                first_hz,
                offset_hz,
            } => write!(
                f,
                "component {id}: first/left carrier {first_hz} Hz and signed offset {offset_hz} Hz: {issue}"
            ),
        }
    }
}
impl core::error::Error for LoweringError {}
impl ComponentForm {
    /// Number of primitive generators needed, before live crossfade expansion.
    #[must_use]
    pub const fn voices(self) -> usize {
        if matches!(self, Self::Voice(_)) { 1 } else { 2 }
    }
    /// Explicit signed pair parameters, independent of the selected sample rate.
    #[must_use]
    pub const fn pair(self) -> Option<Pair> {
        match self {
            Self::Voice(_) => None,
            Self::BinauralPair(pair) | Self::MonauralPair { pair, .. } => Some(pair),
        }
    }
}
/// Lower nodes in source order, preserving primitive sample formulas and pair order.
/// No reordering/regrouping of raw tones or inferred pairing occurs.
///
/// # Errors
/// Refuses invalid owners, either voice budget, and unrepresentable pair sums.
pub(crate) fn lower(components: &[Component]) -> Result<Vec<(VoiceId, Voice)>, LoweringError> {
    Sound::check_structure(components)?;
    let mut output = Vec::new();
    for component in components {
        let id = component.id;
        match component.form {
            ComponentForm::Voice(value) => output.push((id.into(), value)),
            form => {
                let (pair, first_ears, second_ears) = match form {
                    ComponentForm::BinauralPair(pair) => (pair, Ears::Left, Ears::Right),
                    ComponentForm::MonauralPair { pair, ears } => (pair, ears, ears),
                    ComponentForm::Voice(_) => unreachable!("raw form handled above"),
                };
                let second_hz = pair.first_hz + pair.offset_hz;
                // A finite IEEE binary64 sum requires finite operands; this
                // single check also detects overflow of otherwise finite inputs.
                let issue = if !second_hz.is_finite() {
                    Some(PairIssue::Nonfinite)
                } else if pair.first_hz <= 0.0 || second_hz <= 0.0 {
                    Some(PairIssue::Nonpositive)
                } else if pair.offset_hz != 0.0 && second_hz == pair.first_hz {
                    Some(PairIssue::UnrepresentablePairDifference)
                } else {
                    None
                };
                if let Some(issue) = issue {
                    return Err(LoweringError::PairDifference {
                        issue,
                        id,
                        first_hz: pair.first_hz,
                        offset_hz: pair.offset_hz,
                    });
                }
                let tone = |frequency_hz, peak_dbfs, ears| Voice {
                    carrier: Carrier::Tone { frequency_hz },
                    peak_dbfs,
                    ears,
                    modulation: None,
                };
                output.push((
                    id.into(),
                    tone(pair.first_hz, pair.first_peak_dbfs, first_ears),
                ));
                output.push((
                    id.with_lane(Lane::Secondary),
                    tone(second_hz, pair.second_peak_dbfs, second_ears),
                ));
            }
        }
    }
    Ok(output)
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DocumentComponentForm {
    Tone {
        id: ComponentId,
        ears: Ears,
        frequency_hz: f64,
        peak_dbfs: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        modulation: Option<Modulation>,
    },
    Clicks {
        id: ComponentId,
        ears: Ears,
        rate_hz: f64,
        width_seconds: f64,
        peak_dbfs: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        modulation: Option<Modulation>,
    },
    BinauralPair {
        id: ComponentId,
        left_hz: f64,
        offset_hz: f64,
        left_peak_dbfs: f64,
        right_peak_dbfs: f64,
    },
    MonauralPair {
        id: ComponentId,
        ears: Ears,
        first_hz: f64,
        offset_hz: f64,
        first_peak_dbfs: f64,
        second_peak_dbfs: f64,
    },
}
impl From<Component> for DocumentComponentForm {
    fn from(value: Component) -> Self {
        let id = value.id;
        match value.form {
            ComponentForm::Voice(Voice {
                ears,
                peak_dbfs,
                modulation,
                carrier,
            }) => match carrier {
                Carrier::Tone { frequency_hz } => Self::Tone {
                    id,
                    ears,
                    frequency_hz,
                    peak_dbfs,
                    modulation,
                },
                Carrier::Clicks {
                    rate_hz,
                    width_seconds,
                } => Self::Clicks {
                    id,
                    ears,
                    rate_hz,
                    width_seconds,
                    peak_dbfs,
                    modulation,
                },
            },
            ComponentForm::BinauralPair(Pair {
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
            }) => Self::BinauralPair {
                id,
                left_hz: first_hz,
                offset_hz,
                left_peak_dbfs: first_peak_dbfs,
                right_peak_dbfs: second_peak_dbfs,
            },
            ComponentForm::MonauralPair {
                ears,
                pair:
                    Pair {
                        first_hz,
                        offset_hz,
                        first_peak_dbfs,
                        second_peak_dbfs,
                    },
            } => Self::MonauralPair {
                id,
                ears,
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
            },
        }
    }
}
impl From<DocumentComponentForm> for Component {
    fn from(value: DocumentComponentForm) -> Self {
        let (id, form) = match value {
            DocumentComponentForm::Tone {
                id,
                ears,
                frequency_hz,
                peak_dbfs,
                modulation,
            } => (
                id,
                ComponentForm::Voice(Voice {
                    ears,
                    peak_dbfs,
                    modulation,
                    carrier: Carrier::Tone { frequency_hz },
                }),
            ),
            DocumentComponentForm::Clicks {
                id,
                ears,
                rate_hz,
                width_seconds,
                peak_dbfs,
                modulation,
            } => (
                id,
                ComponentForm::Voice(Voice {
                    ears,
                    peak_dbfs,
                    modulation,
                    carrier: Carrier::Clicks {
                        rate_hz,
                        width_seconds,
                    },
                }),
            ),
            DocumentComponentForm::BinauralPair {
                id,
                left_hz,
                offset_hz,
                left_peak_dbfs,
                right_peak_dbfs,
            } => (
                id,
                ComponentForm::BinauralPair(Pair {
                    first_hz: left_hz,
                    offset_hz,
                    first_peak_dbfs: left_peak_dbfs,
                    second_peak_dbfs: right_peak_dbfs,
                }),
            ),
            DocumentComponentForm::MonauralPair {
                id,
                ears,
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
            } => (
                id,
                ComponentForm::MonauralPair {
                    ears,
                    pair: Pair {
                        first_hz,
                        offset_hz,
                        first_peak_dbfs,
                        second_peak_dbfs,
                    },
                },
            ),
        };
        Self { id, form }
    }
}
impl Serialize for Component {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        DocumentComponentForm::from(*self).serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Component {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(DocumentComponentForm::deserialize(
            deserializer,
        )?))
    }
}

/// The authored sound definition; pairs remain explicit until target compilation.
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    /// Finite audio duration requested in seconds.
    pub duration_seconds: f64,
    /// Requested endpoint fade in seconds.
    pub fade_seconds: f64,
    /// Authored nodes summed in this order after stable-lane lowering.
    pub components: Vec<Component>,
}
impl Sound {
    /// Enforce authored uniqueness and both independent structural budgets.
    ///
    /// # Errors
    /// Returns one identity or capacity diagnostic before any acoustic compilation.
    pub fn check_structure(components: &[Component]) -> Result<(), LoweringError> {
        if components.len() > Self::MAX_COMPONENTS {
            return Err(LoweringError::ComponentBudget {
                count: components.len(),
            });
        }
        let count = components.iter().map(|c| c.form.voices()).sum::<usize>();
        if count > Self::MAX_VOICES {
            return Err(LoweringError::VoiceBudget { count });
        }
        for (index, component) in components.iter().enumerate() {
            if components[..index]
                .iter()
                .any(|other| other.id == component.id)
            {
                return Err(LoweringError::Identity { id: component.id });
            }
        }
        Ok(())
    }
    /// Maximum number of authored components.
    pub const MAX_COMPONENTS: usize = 16;
    /// Maximum number of expanded generator voices.
    pub const MAX_VOICES: usize = 16;

    /// Check a whole authored candidate and its expanded generator budget.
    #[must_use]
    pub fn check(&self, rate: crate::SampleRate) -> crate::TargetCheck {
        crate::TargetCheck::authored(rate, &self.components)
    }
    /// Compile the complete atomic authored target.
    ///
    /// # Errors
    /// Rejects pair arithmetic, budgets and all existing target-specific rules.
    pub fn target(&self, rate: crate::SampleRate) -> Result<crate::Target, crate::SoundError> {
        self.check(rate).into_result()
    }
    /// Give the authored target a finite envelope using the existing generators.
    ///
    /// # Errors
    /// Returns authored/target validation or finite-envelope errors.
    pub fn signal(&self, rate: crate::SampleRate) -> Result<crate::Signal, crate::SoundError> {
        let envelope =
            crate::signal::Envelope::resolve(rate, self.duration_seconds, self.fade_seconds)?;
        Ok(crate::Signal::new(self.target(rate)?, envelope))
    }
    /// Whether exact ear separation matters, independent of playback validity.
    #[must_use]
    pub fn uses_separate_ears(&self) -> bool {
        self.components.iter().any(|c| c.form.ears() != Ears::Both)
    }
    /// Nonfinite authored values, without inferring or realizing derived carriers.
    pub fn nonfinite(&self) -> impl Iterator<Item = (Option<usize>, crate::ParameterId)> + '_ {
        [
            (crate::ParameterId::Duration, self.duration_seconds),
            (crate::ParameterId::Fade, self.fade_seconds),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_finite())
        .map(|(p, _)| (None, p))
        .chain(self.components.iter().enumerate().flat_map(|(i, c)| {
            crate::ParameterId::COMPONENT
                .into_iter()
                .filter(move |p| c.form.read(*p).is_some_and(|v| !v.is_finite()))
                .map(move |p| (Some(i), p))
        }))
    }
}
impl ComponentForm {
    /// Authored ear route. A binaural pair requires separate left/right channels.
    #[must_use]
    pub const fn ears(self) -> Ears {
        match self {
            Self::Voice(c) => c.ears,
            Self::BinauralPair(_) => Ears::Left,
            Self::MonauralPair { ears, .. } => ears,
        }
    }
    /// Read an applicable semantic quantity, retaining exact authored binary64 values.
    #[must_use]
    pub fn read(self, parameter: crate::ParameterId) -> Option<f64> {
        use crate::ParameterId as P;
        match self {
            Self::Voice(c) => parameter.read(&c),
            form => {
                let p = form.pair()?;
                match parameter {
                    P::PairFirstFrequency => Some(p.first_hz),
                    P::PairOffset => Some(p.offset_hz),
                    P::PairFirstLevel => Some(p.first_peak_dbfs),
                    P::PairSecondLevel => Some(p.second_peak_dbfs),
                    _ => None,
                }
            }
        }
    }
    /// Set one applicable authored quantity without changing any other field.
    pub fn write(&mut self, parameter: crate::ParameterId, value: f64) -> bool {
        use crate::ParameterId as P;
        match self {
            Self::Voice(c) => parameter.write(c, value),
            Self::BinauralPair(p) | Self::MonauralPair { pair: p, .. } => {
                let slot = match parameter {
                    P::PairFirstFrequency => &mut p.first_hz,
                    P::PairOffset => &mut p.offset_hz,
                    P::PairFirstLevel => &mut p.first_peak_dbfs,
                    P::PairSecondLevel => &mut p.second_peak_dbfs,
                    _ => return false,
                };
                *slot = value;
                true
            }
        }
    }
}
impl fmt::Display for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.form {
            ComponentForm::Voice(c) => c.fmt(f),
            form => {
                let (p, method) = match form {
                    ComponentForm::BinauralPair(p) => (p, "binaural"),
                    ComponentForm::MonauralPair { pair, .. } => (pair, "monaural"),
                    ComponentForm::Voice(_) => unreachable!("primitive handled above"),
                };
                write!(
                    f,
                    "{method} pair, first {} Hz, signed offset {} Hz, second {} Hz, peaks {} / {} dBFS",
                    p.first_hz,
                    p.offset_hz,
                    p.first_hz + p.offset_hz,
                    p.first_peak_dbfs,
                    p.second_peak_dbfs
                )
            }
        }
    }
}

impl ComponentForm {
    /// Exact authored comparison for dirty/no-op semantics, including signed zeros.
    #[must_use]
    pub fn same_content(self, other: Self) -> bool {
        let same_shape = match (self, other) {
            (Self::Voice(a), Self::Voice(b)) => {
                a.ears == b.ears
                    && a.carrier.kind() == b.carrier.kind()
                    && a.modulation.map(|m| m.kind()) == b.modulation.map(|m| m.kind())
            }
            (Self::BinauralPair(_), Self::BinauralPair(_)) => true,
            (Self::MonauralPair { ears: a, .. }, Self::MonauralPair { ears: b, .. }) => a == b,
            _ => false,
        };
        same_shape
            && crate::ParameterId::COMPONENT
                .into_iter()
                .all(|p| self.read(p).map(f64::to_bits) == other.read(p).map(f64::to_bits))
    }
}

impl ComponentForm {
    /// Primitive carrier choice for existing primitive options; pairs have no choice.
    #[must_use]
    pub const fn primitive_kind(self) -> Option<crate::CarrierKind> {
        match self {
            Self::Voice(c) => Some(c.carrier.kind()),
            _ => None,
        }
    }
    /// Optional modulation; authored pairs cannot carry modulation fields.
    #[must_use]
    pub const fn modulation_kind(self) -> Option<crate::ModulationKind> {
        match self {
            Self::Voice(c) => match c.modulation {
                Some(m) => Some(m.kind()),
                None => None,
            },
            _ => None,
        }
    }
    /// Domain heading without target-rate inference.
    #[must_use]
    pub fn heading(self) -> &'static str {
        match self {
            Self::Voice(c) => c.role().label(),
            Self::BinauralPair(_) => "Binaural pair",
            Self::MonauralPair { .. } => "Monaural pair",
        }
    }
}

/// The two supported explicit pair constructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairKind {
    /// Separate left/right tones.
    Binaural,
    /// Two tones on one identical route.
    Monaural,
}
impl ComponentForm {
    /// Whether ear routing is editable for this authored form.
    #[must_use]
    pub const fn routing_editable(self) -> bool {
        !matches!(self, Self::BinauralPair(_))
    }
}

impl Sound {
    /// Exact authored sound equality, including owner/order and signed-zero bits.
    #[must_use]
    pub fn same_content(&self, other: &Self) -> bool {
        self.duration_seconds.to_bits() == other.duration_seconds.to_bits()
            && self.fade_seconds.to_bits() == other.fade_seconds.to_bits()
            && self.components.len() == other.components.len()
            && self
                .components
                .iter()
                .zip(&other.components)
                .all(|(a, b)| a.id == b.id && a.form.same_content(b.form))
    }
}

#[cfg(test)]
mod tests;
