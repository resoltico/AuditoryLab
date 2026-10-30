// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Ordered authored components, source relationships and weighted resource invariants.
use crate::{ChannelRouting, ComponentId, Finite, Modulation};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// A signed-offset two-tone construction. The second carrier is first + offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    /// First carrier; left for a binaural pair.
    pub first_hz: Finite,
    /// Authored signed nominal difference; never recovered from generated carriers.
    pub offset_hz: Finite,
    /// First/left tone's peak bound.
    pub first_peak_dbfs: Finite,
    /// Second/right tone's peak bound.
    pub second_peak_dbfs: Finite,
}
/// One explicitly authored source relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SoundSource {
    /// One sine oscillator.
    Tone {
        /// Digital channel routing.
        route: ChannelRouting,
        /// Carrier frequency.
        frequency_hz: Finite,
        /// Digital peak bound.
        peak_dbfs: Finite,
    },
    /// A unipolar pulse source with an explicit window shape.
    PulseTrain {
        /// Digital channel routing.
        route: ChannelRouting,
        /// Repetition rate.
        pulse: crate::PulseSpec,
        /// Digital peak bound.
        peak_dbfs: Finite,
    },
    /// Explicit separate-channel pair.
    BinauralPair {
        /// First carrier.
        first_hz: Finite,
        /// Signed difference.
        offset_hz: Finite,
        /// First peak.
        first_peak_dbfs: Finite,
        /// Second peak.
        second_peak_dbfs: Finite,
    },
    /// Explicit same-route pair.
    MonauralPair {
        /// Digital channel routing.
        route: ChannelRouting,
        /// First carrier.
        first_hz: Finite,
        /// Signed difference.
        offset_hz: Finite,
        /// First peak.
        first_peak_dbfs: Finite,
        /// Second peak.
        second_peak_dbfs: Finite,
    },
}
/// A source with one shared component modulation process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentForm {
    /// Authored sound source.
    pub source: SoundSource,
    /// Shared gain; the wire representation always requires a kind table.
    #[serde(with = "crate::modulation::optional")]
    pub modulation: Option<Modulation>,
}
impl ComponentForm {
    /// Digital routing; separate-channel pairs fix their own routes.
    pub const fn route(self) -> Option<ChannelRouting> {
        match self.source {
            SoundSource::Tone { route, .. }
            | SoundSource::PulseTrain { route, .. }
            | SoundSource::MonauralPair { route, .. } => Some(route),
            SoundSource::BinauralPair { .. } => None,
        }
    }
    /// Shared component gain process.
    pub const fn modulation(self) -> Option<Modulation> {
        self.modulation
    }
    /// Replace the shared modulation for any source, including pairs.
    pub fn set_modulation(&mut self, value: Option<Modulation>) -> bool {
        self.modulation = value;
        true
    }
    /// Read an applicable quantity in model units.
    pub fn get(self, parameter: crate::ParameterId) -> Option<Finite> {
        use crate::ParameterId as P;
        match (parameter, self.source) {
            (P::ToneFrequency, SoundSource::Tone { frequency_hz, .. }) => Some(frequency_hz),
            (
                P::PeakLevel,
                SoundSource::Tone { peak_dbfs, .. } | SoundSource::PulseTrain { peak_dbfs, .. },
            ) => Some(peak_dbfs),
            (
                P::PairFirstFrequency,
                SoundSource::BinauralPair { first_hz, .. }
                | SoundSource::MonauralPair { first_hz, .. },
            ) => Some(first_hz),
            (
                P::PairOffset,
                SoundSource::BinauralPair { offset_hz, .. }
                | SoundSource::MonauralPair { offset_hz, .. },
            ) => Some(offset_hz),
            (
                P::PairFirstLevel,
                SoundSource::BinauralPair {
                    first_peak_dbfs, ..
                }
                | SoundSource::MonauralPair {
                    first_peak_dbfs, ..
                },
            ) => Some(first_peak_dbfs),
            (
                P::PairSecondLevel,
                SoundSource::BinauralPair {
                    second_peak_dbfs, ..
                }
                | SoundSource::MonauralPair {
                    second_peak_dbfs, ..
                },
            ) => Some(second_peak_dbfs),
            _ => match self.source {
                SoundSource::PulseTrain { pulse, .. } => pulse
                    .get(parameter, false)
                    .or_else(|| self.modulation.and_then(|m| m.get(parameter))),
                _ => self.modulation.and_then(|m| m.get(parameter)),
            },
        }
    }
    /// Write only the addressed applicable quantity.
    pub fn set(&mut self, parameter: crate::ParameterId, value: Finite) -> bool {
        use crate::ParameterId as P;
        let field = match (parameter, &mut self.source) {
            (P::ToneFrequency, SoundSource::Tone { frequency_hz, .. }) => frequency_hz,
            (
                P::PeakLevel,
                SoundSource::Tone { peak_dbfs, .. } | SoundSource::PulseTrain { peak_dbfs, .. },
            ) => peak_dbfs,
            (
                P::PairFirstFrequency,
                SoundSource::BinauralPair { first_hz, .. }
                | SoundSource::MonauralPair { first_hz, .. },
            ) => first_hz,
            (
                P::PairOffset,
                SoundSource::BinauralPair { offset_hz, .. }
                | SoundSource::MonauralPair { offset_hz, .. },
            ) => offset_hz,
            (
                P::PairFirstLevel,
                SoundSource::BinauralPair {
                    first_peak_dbfs, ..
                }
                | SoundSource::MonauralPair {
                    first_peak_dbfs, ..
                },
            ) => first_peak_dbfs,
            (
                P::PairSecondLevel,
                SoundSource::BinauralPair {
                    second_peak_dbfs, ..
                }
                | SoundSource::MonauralPair {
                    second_peak_dbfs, ..
                },
            ) => second_peak_dbfs,
            (_, SoundSource::PulseTrain { pulse, .. }) if pulse.get(parameter, false).is_some() => {
                return pulse.set(parameter, value, false);
            }
            _ => {
                return self
                    .modulation
                    .as_mut()
                    .is_some_and(|m| m.set(parameter, value));
            }
        };
        *field = value;
        true
    }
}
/// An authored form and its positive stable owner identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Component {
    /// Authored owner, independent of generator lanes.
    pub id: ComponentId,
    /// Authored sound structure.
    pub form: ComponentForm,
}
/// Coupled carrier/offset failure, independent of sample-rate Nyquist checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairIssue {
    /// The computed second carrier is nonfinite.
    Nonfinite,
    /// One computed carrier is zero or negative.
    Nonpositive,
    /// A nonzero authored offset disappears during floating-point addition.
    UnrepresentablePairDifference,
}
impl core::fmt::Display for PairIssue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}
/// Why authored owners or generator capacity violate the sound structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructureError {
    /// A repeated authored identity.
    Identity {
        /// Repeated owner.
        id: ComponentId,
    },
    /// Too many expanded voices.
    VoiceBudget {
        /// Expanded voice count.
        count: usize,
    },
}
/// A requested pair cannot retain its coupled carrier arithmetic.
#[derive(Debug, Clone, PartialEq)]
pub struct PairError {
    /// The failed arithmetic rule.
    pub issue: PairIssue,
    /// Authored owner.
    pub id: ComponentId,
    /// Requested first carrier.
    pub first_hz: f64,
    /// Requested signed offset.
    pub offset_hz: f64,
}
impl core::fmt::Display for StructureError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}
impl core::fmt::Display for PairError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}
impl core::error::Error for StructureError {}
impl core::error::Error for PairError {}
impl Pair {
    /// Resolve the coupled requested carriers without a sample-rate decision.
    pub(crate) fn carriers(self, id: ComponentId) -> Result<[f64; 2], PairError> {
        let first = self.first_hz.get();
        let second = first + self.offset_hz.get();
        let issue = if !second.is_finite() {
            Some(PairIssue::Nonfinite)
        } else if !crate::validity::positive(first) || !crate::validity::positive(second) {
            Some(PairIssue::Nonpositive)
        } else if self.offset_hz.get() != 0.0 && second == first {
            Some(PairIssue::UnrepresentablePairDifference)
        } else {
            None
        };
        if let Some(issue) = issue {
            Err(PairError {
                issue,
                id,
                first_hz: first,
                offset_hz: self.offset_hz.get(),
            })
        } else {
            Ok([first, second])
        }
    }
}

/// Ordered authored sound with unique identities and bounded generator capacity.
/// Fields can only be set through checked construction and mutation.
/// ```compile_fail
/// use auditorylab_core::Sound;
/// let _ = Sound {
///     components: Vec::new(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    components: Vec<Component>,
}
impl Sound {
    /// Maximum number of authored components.
    pub const MAX_COMPONENTS: usize = Self::MAX_VOICES;
    /// Maximum number of expanded generator lanes.
    pub const MAX_VOICES: usize = 16;
    /// Construct a sound while checking owner uniqueness and the weighted lane budget.
    ///
    /// # Errors
    /// Returns the first identity or capacity violation.
    pub fn new(components: Vec<Component>) -> Result<Self, StructureError> {
        Self::check_structure(&components)?;
        Ok(Self { components })
    }
    fn check_structure(components: &[Component]) -> Result<(), StructureError> {
        Self::check_capacity(components.iter().map(|c| c.form.voices()).sum())?;
        for (index, component) in components.iter().enumerate() {
            if components[..index]
                .iter()
                .any(|other| other.id == component.id)
            {
                return Err(StructureError::Identity { id: component.id });
            }
        }
        Ok(())
    }
    fn check_capacity(voices: usize) -> Result<(), StructureError> {
        if voices > Self::MAX_VOICES {
            return Err(StructureError::VoiceBudget { count: voices });
        }
        Ok(())
    }
    /// Check capacity for one additional form. Insertion checks identity again.
    ///
    /// # Errors
    /// Returns the exceeded weighted lane budget.
    pub fn check_addition(&self, form: ComponentForm) -> Result<(), StructureError> {
        Self::check_capacity(
            self.components
                .iter()
                .map(|c| c.form.voices())
                .sum::<usize>()
                + form.voices(),
        )
    }
    /// Authored components in summation order.
    #[must_use]
    pub fn components(&self) -> &[Component] {
        &self.components
    }
    /// One authored component by its stable identity.
    #[must_use]
    pub fn component(&self, id: ComponentId) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }
    /// Append a component, leaving the sound unchanged on refusal.
    ///
    /// # Errors
    /// Returns an identity or capacity violation.
    pub fn push(&mut self, component: Component) -> Result<(), StructureError> {
        self.insert(self.components.len(), component)
    }
    /// Insert at an authored position, leaving the sound unchanged on refusal.
    ///
    /// # Errors
    /// Returns an identity or capacity violation.
    /// # Panics
    /// Panics when the index exceeds the component count, like `Vec::insert`.
    pub fn insert(&mut self, index: usize, component: Component) -> Result<(), StructureError> {
        let mut candidate = self.components.clone();
        candidate.insert(index, component);
        Self::check_structure(&candidate)?;
        self.components = candidate;
        Ok(())
    }
    /// Remove an owner, returning its position and complete authored form.
    pub fn remove(&mut self, id: ComponentId) -> Option<(usize, Component)> {
        let index = self.components.iter().position(|c| c.id == id)?;
        Some((index, self.components.remove(index)))
    }
    /// Replace an existing owner's form. Unknown owners return `Ok(false)`.
    ///
    /// # Errors
    /// Returns a capacity violation and retains the original sound.
    pub fn replace(
        &mut self,
        id: ComponentId,
        form: ComponentForm,
    ) -> Result<bool, StructureError> {
        let Some(index) = self.components.iter().position(|c| c.id == id) else {
            return Ok(false);
        };
        let mut candidate = self.components.clone();
        candidate[index].form = form;
        Self::check_structure(&candidate)?;
        self.components = candidate;
        Ok(true)
    }
    /// Compile every generator lane at the chosen rate.
    #[must_use]
    pub fn check(&self, rate: crate::SampleRate) -> crate::TargetCheck {
        crate::TargetCheck::authored(rate, self)
    }
    /// Compile the complete authored target.
    ///
    /// # Errors
    /// Returns pair arithmetic or target-rate validation errors.
    pub fn target(&self, rate: crate::SampleRate) -> Result<crate::Target, crate::SoundError> {
        self.check(rate).into_result()
    }
    /// Whether any authored form needs separate ear channels.
    #[must_use]
    pub fn uses_separate_channels(&self) -> bool {
        self.components.iter().any(|c| c.form.separates_channels())
    }
}

impl ComponentForm {
    /// Number of generator lanes before live transitions.
    #[must_use]
    pub const fn voices(self) -> usize {
        match self.source {
            SoundSource::Tone { .. } | SoundSource::PulseTrain { .. } => 1,
            _ => 2,
        }
    }
    /// The explicit pair construction.
    #[must_use]
    pub const fn pair_kind(self) -> Option<PairKind> {
        match self.source {
            SoundSource::BinauralPair { .. } => Some(PairKind::Binaural),
            SoundSource::MonauralPair { .. } => Some(PairKind::Monaural),
            _ => None,
        }
    }
    /// Explicit signed pair parameters.
    #[must_use]
    pub const fn pair(self) -> Option<Pair> {
        match self.source {
            SoundSource::BinauralPair {
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
            }
            | SoundSource::MonauralPair {
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
                ..
            } => Some(Pair {
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
            }),
            _ => None,
        }
    }

    /// Whether this form needs separate ear channels.
    #[must_use]
    pub const fn separates_channels(self) -> bool {
        !matches!(self.route(), Some(ChannelRouting::Both))
    }
    /// Primitive carrier choice; pairs have no carrier choice.
    #[must_use]
    pub const fn primitive_kind(self) -> Option<crate::CarrierKind> {
        match self.source {
            SoundSource::Tone { .. } => Some(crate::CarrierKind::Tone),
            SoundSource::PulseTrain { .. } => Some(crate::CarrierKind::PulseTrain),
            _ => None,
        }
    }
    /// Optional modulation kind.
    #[must_use]
    pub const fn modulation_kind(self) -> Option<crate::ModulationKind> {
        match self.modulation() {
            Some(m) => Some(m.kind()),
            None => None,
        }
    }
    /// Domain heading without target-rate inference.
    #[must_use]
    pub fn heading(self) -> &'static str {
        let modulation = match self.modulation {
            Some(Modulation::Sinusoidal { depth, .. }) if depth.get() == 0.0 => None,
            other => other,
        };
        match (self.source, modulation) {
            (SoundSource::Tone { .. }, None) => crate::vocabulary::STEADY_TONE.label,
            (SoundSource::Tone { .. }, Some(Modulation::Sinusoidal { .. })) => {
                "Tone with sinusoidal AM"
            }
            (SoundSource::Tone { .. }, Some(Modulation::Pulsed { .. })) => {
                crate::vocabulary::PULSED_TONE.label
            }
            (SoundSource::PulseTrain { .. }, None) => crate::CarrierKind::PulseTrain.label(),
            (SoundSource::PulseTrain { .. }, Some(Modulation::Sinusoidal { .. })) => {
                "Pulse train with sinusoidal AM"
            }
            (SoundSource::PulseTrain { .. }, Some(Modulation::Pulsed { .. })) => {
                "Pulse train with pulsed modulation"
            }
            (SoundSource::BinauralPair { .. }, None) => PairKind::Binaural.label(),
            (SoundSource::BinauralPair { .. }, Some(Modulation::Sinusoidal { .. })) => {
                "Binaural pair with sinusoidal AM"
            }
            (SoundSource::BinauralPair { .. }, Some(Modulation::Pulsed { .. })) => {
                "Pulsed binaural pair"
            }
            (SoundSource::MonauralPair { .. }, None) => PairKind::Monaural.label(),
            (SoundSource::MonauralPair { .. }, Some(Modulation::Sinusoidal { .. })) => {
                "Monaural pair with sinusoidal AM"
            }
            (SoundSource::MonauralPair { .. }, Some(Modulation::Pulsed { .. })) => {
                "Pulsed monaural pair"
            }
        }
    }

    pub(crate) fn intrinsic_issues(&self) -> impl Iterator<Item = crate::ValueIssue> + '_ {
        crate::ParameterId::COMPONENT.into_iter().filter_map(|p| {
            self.get(p)
                .and_then(|v| crate::validity::check_value(p, v.get()).err())
        })
    }
}
/// The two supported explicit pair constructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairKind {
    /// Separate left/right tones.
    Binaural,
    /// Two tones on one identical route.
    Monaural,
}

crate::vocabulary::vocabulary! { PairKind,"pair_kind" {
    Binaural => ("binaural","Binaural pair","binaural pair"),
    Monaural => ("monaural","Monaural pair","monaural pair"),
}}

crate::diagnostic_codes! {
    pub(crate) const CODES;
    PairIssue {
        codes {
            Self::Nonfinite => "pair.nonfinite",
            Self::Nonpositive => "pair.nonpositive",
            Self::UnrepresentablePairDifference => "pair.unrepresentable_difference",
        }
        delegates {}
    }
    StructureError {
        codes {
            Self::Identity { .. } => "structure.identity",
            Self::VoiceBudget { .. } => "structure.voice_budget",
        }
        delegates {}
        params(this) {
            match this {
                Self::Identity { id } => crate::diagnostic_params! {"component_id"=>id.get()},
                Self::VoiceBudget { count } => {
                    crate::diagnostic_params! {
                        "count"=>*count,
                        "limit"=>Sound::MAX_VOICES,
                    }
                }
            }
        }
    }
    PairError {
        codes {}
        delegates {
            Self { issue, .. } => issue,
        }
        params(this) {
            crate::diagnostic_params! {
                "component_id"=>this.id.get(),
                "first_hz"=>this.first_hz,
                "offset_hz"=>this.offset_hz,
            }
        }
    }
}

#[cfg(test)]
mod tests;
