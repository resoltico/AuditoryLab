// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Ordered authored components and lossless lowering into primitive generator lanes.
use crate::{Carrier, ComponentId, Ears, Finite, Lane, Modulation, Voice, VoiceId};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentForm {
    /// Authored tone quantities.
    Tone {
        /// The requested ears.
        ears: Ears,
        /// The requested frequency hz.
        frequency_hz: Finite,
        /// The requested peak dbfs.
        peak_dbfs: Finite,
        /// The requested modulation.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        modulation: Option<Modulation>,
    },
    /// Authored clicks quantities.
    Clicks {
        /// The requested ears.
        ears: Ears,
        /// The requested rate hz.
        rate_hz: Finite,
        /// The requested width seconds.
        width_seconds: Finite,
        /// The requested peak dbfs.
        peak_dbfs: Finite,
        /// The requested modulation.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        modulation: Option<Modulation>,
    },
    /// Authored binaural_pair quantities.
    BinauralPair {
        /// The requested left hz.
        left_hz: Finite,
        /// The requested offset hz.
        offset_hz: Finite,
        /// The requested left peak dbfs.
        left_peak_dbfs: Finite,
        /// The requested right peak dbfs.
        right_peak_dbfs: Finite,
    },
    /// Authored monaural_pair quantities.
    MonauralPair {
        /// The requested ears.
        ears: Ears,
        /// The requested first hz.
        first_hz: Finite,
        /// The requested offset hz.
        offset_hz: Finite,
        /// The requested first peak dbfs.
        first_peak_dbfs: Finite,
        /// The requested second peak dbfs.
        second_peak_dbfs: Finite,
    },
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
    /// A requested carrier, offset or computed carrier is nonfinite.
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
    /// Too many authored components.
    ComponentBudget {
        /// Authored component count.
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
/// Expand one authored component in stable primary/secondary generator order.
/// No reordering/regrouping of raw tones or inferred pairing occurs.
///
/// # Errors
/// Refuses unrepresentable pair carrier arithmetic.
pub(crate) fn lower(component: &Component) -> Result<Vec<(VoiceId, Voice)>, PairError> {
    let mut output = Vec::new();
    let id = component.id;
    if let Some(value) = component.form.primitive() {
        output.push((id.into(), value));
    } else {
        let form = component.form;
        {
            let pair = form
                .pair()
                .unwrap_or_else(|| unreachable!("nonprimitive forms are pairs"));
            let (first_ears, second_ears) = match form.route() {
                None => (Ears::Left, Ears::Right),
                Some(ears) => (ears, ears),
            };
            let second_hz = pair.first_hz + pair.offset_hz;
            // A finite IEEE binary64 sum requires finite operands; this
            // single check also detects overflow of otherwise finite inputs.
            let issue = if !second_hz.is_finite() {
                Some(PairIssue::Nonfinite)
            } else if !crate::validity::positive(pair.first_hz)
                || !crate::validity::positive(second_hz)
            {
                Some(PairIssue::Nonpositive)
            } else if pair.offset_hz != 0.0 && second_hz == pair.first_hz {
                Some(PairIssue::UnrepresentablePairDifference)
            } else {
                None
            };
            if let Some(issue) = issue {
                return Err(PairError {
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
    Ok(output)
}

impl Serialize for Component {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        match self.form {
            ComponentForm::Tone {
                ears,
                frequency_hz,
                peak_dbfs,
                modulation,
            } => {
                map.serialize_entry("kind", "tone")?;
                map.serialize_entry("id", &self.id)?;
                map.serialize_entry("ears", &ears)?;
                map.serialize_entry("frequency_hz", &frequency_hz)?;
                map.serialize_entry("peak_dbfs", &peak_dbfs)?;
                if let Some(modulation) = modulation {
                    map.serialize_entry("modulation", &modulation)?;
                }
            }
            ComponentForm::Clicks {
                ears,
                rate_hz,
                width_seconds,
                peak_dbfs,
                modulation,
            } => {
                map.serialize_entry("kind", "clicks")?;
                map.serialize_entry("id", &self.id)?;
                map.serialize_entry("ears", &ears)?;
                map.serialize_entry("rate_hz", &rate_hz)?;
                map.serialize_entry("width_seconds", &width_seconds)?;
                map.serialize_entry("peak_dbfs", &peak_dbfs)?;
                if let Some(modulation) = modulation {
                    map.serialize_entry("modulation", &modulation)?;
                }
            }
            ComponentForm::BinauralPair {
                left_hz,
                offset_hz,
                left_peak_dbfs,
                right_peak_dbfs,
            } => {
                map.serialize_entry("kind", "binaural_pair")?;
                map.serialize_entry("id", &self.id)?;
                map.serialize_entry("left_hz", &left_hz)?;
                map.serialize_entry("offset_hz", &offset_hz)?;
                map.serialize_entry("left_peak_dbfs", &left_peak_dbfs)?;
                map.serialize_entry("right_peak_dbfs", &right_peak_dbfs)?;
            }
            ComponentForm::MonauralPair {
                ears,
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
            } => {
                map.serialize_entry("kind", "monaural_pair")?;
                map.serialize_entry("id", &self.id)?;
                map.serialize_entry("ears", &ears)?;
                map.serialize_entry("first_hz", &first_hz)?;
                map.serialize_entry("offset_hz", &offset_hz)?;
                map.serialize_entry("first_peak_dbfs", &first_peak_dbfs)?;
                map.serialize_entry("second_peak_dbfs", &second_peak_dbfs)?;
            }
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for Component {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Component;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an authored component")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Component, M::Error> {
                use serde::de::Error;
                let mut id: Option<ComponentId> = None;
                let mut kind: Option<alloc::string::String> = None;
                let mut ears: Option<Ears> = None;
                let mut frequency_hz: Option<Finite> = None;
                let mut peak_dbfs: Option<Finite> = None;
                let mut modulation: Option<Option<Modulation>> = None;
                let mut rate_hz: Option<Finite> = None;
                let mut width_seconds: Option<Finite> = None;
                let mut left_hz: Option<Finite> = None;
                let mut offset_hz: Option<Finite> = None;
                let mut left_peak_dbfs: Option<Finite> = None;
                let mut right_peak_dbfs: Option<Finite> = None;
                let mut first_hz: Option<Finite> = None;
                let mut first_peak_dbfs: Option<Finite> = None;
                let mut second_peak_dbfs: Option<Finite> = None;
                while let Some(key) = map.next_key::<alloc::string::String>()? {
                    match key.as_str() {
                        "id" => {
                            if id.is_some() {
                                return Err(M::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        }
                        "kind" => {
                            if kind.is_some() {
                                return Err(M::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value()?);
                        }
                        "ears" => {
                            if ears.is_some() {
                                return Err(M::Error::duplicate_field("ears"));
                            }
                            ears = Some(map.next_value()?);
                        }
                        "frequency_hz" => {
                            if frequency_hz.is_some() {
                                return Err(M::Error::duplicate_field("frequency_hz"));
                            }
                            frequency_hz = Some(map.next_value()?);
                        }
                        "peak_dbfs" => {
                            if peak_dbfs.is_some() {
                                return Err(M::Error::duplicate_field("peak_dbfs"));
                            }
                            peak_dbfs = Some(map.next_value()?);
                        }
                        "modulation" => {
                            if modulation.is_some() {
                                return Err(M::Error::duplicate_field("modulation"));
                            }
                            modulation = Some(map.next_value()?);
                        }
                        "rate_hz" => {
                            if rate_hz.is_some() {
                                return Err(M::Error::duplicate_field("rate_hz"));
                            }
                            rate_hz = Some(map.next_value()?);
                        }
                        "width_seconds" => {
                            if width_seconds.is_some() {
                                return Err(M::Error::duplicate_field("width_seconds"));
                            }
                            width_seconds = Some(map.next_value()?);
                        }
                        "left_hz" => {
                            if left_hz.is_some() {
                                return Err(M::Error::duplicate_field("left_hz"));
                            }
                            left_hz = Some(map.next_value()?);
                        }
                        "offset_hz" => {
                            if offset_hz.is_some() {
                                return Err(M::Error::duplicate_field("offset_hz"));
                            }
                            offset_hz = Some(map.next_value()?);
                        }
                        "left_peak_dbfs" => {
                            if left_peak_dbfs.is_some() {
                                return Err(M::Error::duplicate_field("left_peak_dbfs"));
                            }
                            left_peak_dbfs = Some(map.next_value()?);
                        }
                        "right_peak_dbfs" => {
                            if right_peak_dbfs.is_some() {
                                return Err(M::Error::duplicate_field("right_peak_dbfs"));
                            }
                            right_peak_dbfs = Some(map.next_value()?);
                        }
                        "first_hz" => {
                            if first_hz.is_some() {
                                return Err(M::Error::duplicate_field("first_hz"));
                            }
                            first_hz = Some(map.next_value()?);
                        }
                        "first_peak_dbfs" => {
                            if first_peak_dbfs.is_some() {
                                return Err(M::Error::duplicate_field("first_peak_dbfs"));
                            }
                            first_peak_dbfs = Some(map.next_value()?);
                        }
                        "second_peak_dbfs" => {
                            if second_peak_dbfs.is_some() {
                                return Err(M::Error::duplicate_field("second_peak_dbfs"));
                            }
                            second_peak_dbfs = Some(map.next_value()?);
                        }
                        _ => {
                            return Err(M::Error::custom(alloc::format!(
                                "unknown component field `{key}`"
                            )));
                        }
                    }
                }
                let id = id.ok_or_else(|| M::Error::missing_field("id"))?;
                let kind = kind.ok_or_else(|| M::Error::missing_field("kind"))?;
                macro_rules! required {
                    ($field:ident) => {
                        $field.ok_or_else(|| M::Error::missing_field(stringify!($field)))?
                    };
                }
                macro_rules! absent { ($($field:ident),* $(,)?) => { $(if $field.is_some() { return Err(M::Error::custom(alloc::format!("unknown field `{}` for {}", stringify!($field), kind))); })* }; }
                let form = match kind.as_str() {
                    "tone" => {
                        absent!(
                            rate_hz,
                            width_seconds,
                            left_hz,
                            offset_hz,
                            left_peak_dbfs,
                            right_peak_dbfs,
                            first_hz,
                            first_peak_dbfs,
                            second_peak_dbfs
                        );
                        ComponentForm::Tone {
                            ears: required!(ears),
                            frequency_hz: required!(frequency_hz),
                            peak_dbfs: required!(peak_dbfs),
                            modulation: modulation.unwrap_or(None),
                        }
                    }
                    "clicks" => {
                        absent!(
                            frequency_hz,
                            left_hz,
                            offset_hz,
                            left_peak_dbfs,
                            right_peak_dbfs,
                            first_hz,
                            first_peak_dbfs,
                            second_peak_dbfs
                        );
                        ComponentForm::Clicks {
                            ears: required!(ears),
                            rate_hz: required!(rate_hz),
                            width_seconds: required!(width_seconds),
                            peak_dbfs: required!(peak_dbfs),
                            modulation: modulation.unwrap_or(None),
                        }
                    }
                    "binaural_pair" => {
                        absent!(
                            ears,
                            frequency_hz,
                            peak_dbfs,
                            modulation,
                            rate_hz,
                            width_seconds,
                            first_hz,
                            first_peak_dbfs,
                            second_peak_dbfs
                        );
                        ComponentForm::BinauralPair {
                            left_hz: required!(left_hz),
                            offset_hz: required!(offset_hz),
                            left_peak_dbfs: required!(left_peak_dbfs),
                            right_peak_dbfs: required!(right_peak_dbfs),
                        }
                    }
                    "monaural_pair" => {
                        absent!(
                            frequency_hz,
                            peak_dbfs,
                            modulation,
                            rate_hz,
                            width_seconds,
                            left_hz,
                            left_peak_dbfs,
                            right_peak_dbfs
                        );
                        ComponentForm::MonauralPair {
                            ears: required!(ears),
                            first_hz: required!(first_hz),
                            offset_hz: required!(offset_hz),
                            first_peak_dbfs: required!(first_peak_dbfs),
                            second_peak_dbfs: required!(second_peak_dbfs),
                        }
                    }
                    _ => {
                        return Err(M::Error::unknown_variant(
                            &kind,
                            &["tone", "clicks", "binaural_pair", "monaural_pair"],
                        ));
                    }
                };
                Ok(Component { id, form })
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}
/// Ordered authored sound with unique identities and bounded generator capacity.
/// Fields can only be set through checked construction and mutation.
/// ```compile_fail
/// use auditorylab_core::{Finite, Sound};
/// let _ = Sound {
///     duration_seconds: Finite::ZERO,
///     fade_seconds: Finite::ZERO,
///     components: Vec::new(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    duration_seconds: Finite,
    fade_seconds: Finite,
    components: Vec<Component>,
}
impl Sound {
    /// Maximum number of authored components.
    pub const MAX_COMPONENTS: usize = 16;
    /// Maximum number of expanded generator lanes.
    pub const MAX_VOICES: usize = 16;
    /// Construct a sound while checking owner uniqueness and both budgets.
    ///
    /// # Errors
    /// Returns the first identity or capacity violation.
    pub fn new(
        duration_seconds: Finite,
        fade_seconds: Finite,
        components: Vec<Component>,
    ) -> Result<Self, StructureError> {
        Self::check_structure(&components)?;
        Ok(Self {
            duration_seconds,
            fade_seconds,
            components,
        })
    }
    fn check_structure(components: &[Component]) -> Result<(), StructureError> {
        Self::check_capacity(
            components.len(),
            components.iter().map(|c| c.form.voices()).sum(),
        )?;
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
    fn check_capacity(components: usize, voices: usize) -> Result<(), StructureError> {
        if components > Self::MAX_COMPONENTS {
            return Err(StructureError::ComponentBudget { count: components });
        }
        if voices > Self::MAX_VOICES {
            return Err(StructureError::VoiceBudget { count: voices });
        }
        Ok(())
    }
    /// Check capacity for one additional form. Insertion checks identity again.
    ///
    /// # Errors
    /// Returns the exceeded authored or expanded budget.
    pub fn check_addition(&self, form: ComponentForm) -> Result<(), StructureError> {
        Self::check_capacity(
            self.components.len() + 1,
            self.components
                .iter()
                .map(|c| c.form.voices())
                .sum::<usize>()
                + form.voices(),
        )
    }
    /// Duration in seconds; target-rate envelope constraints are checked separately.
    #[must_use]
    pub const fn duration_seconds(&self) -> Finite {
        self.duration_seconds
    }
    /// Requested edge fade in seconds.
    #[must_use]
    pub const fn fade_seconds(&self) -> Finite {
        self.fade_seconds
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
    /// Replace the requested duration without changing other content.
    pub const fn set_duration(&mut self, value: Finite) {
        self.duration_seconds = value;
    }
    /// Replace the requested edge fade without changing other content.
    pub const fn set_fade(&mut self, value: Finite) {
        self.fade_seconds = value;
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
    /// Give the authored target a finite envelope.
    ///
    /// # Errors
    /// Returns target or finite-envelope validation errors.
    pub fn signal(&self, rate: crate::SampleRate) -> Result<crate::Signal, crate::SoundError> {
        let envelope = crate::signal::Envelope::resolve(
            rate,
            self.duration_seconds.get(),
            self.fade_seconds.get(),
        )?;
        Ok(crate::Signal::new(self.target(rate)?, envelope))
    }
    /// Whether any authored form needs separate ear channels.
    #[must_use]
    pub fn uses_separate_ears(&self) -> bool {
        self.components.iter().any(|c| c.form.separates_ears())
    }
}

impl ComponentForm {
    /// Number of generator lanes before live transitions.
    #[must_use]
    pub const fn voices(self) -> usize {
        match self {
            Self::Tone { .. } | Self::Clicks { .. } => 1,
            _ => 2,
        }
    }
    /// The explicit pair construction.
    #[must_use]
    pub const fn pair_kind(self) -> Option<PairKind> {
        match self {
            Self::BinauralPair { .. } => Some(PairKind::Binaural),
            Self::MonauralPair { .. } => Some(PairKind::Monaural),
            _ => None,
        }
    }
    /// Explicit signed pair parameters.
    #[must_use]
    pub const fn pair(self) -> Option<Pair> {
        match self {
            Self::BinauralPair {
                left_hz,
                offset_hz,
                left_peak_dbfs,
                right_peak_dbfs,
            } => Some(Pair {
                first_hz: left_hz.get(),
                offset_hz: offset_hz.get(),
                first_peak_dbfs: left_peak_dbfs.get(),
                second_peak_dbfs: right_peak_dbfs.get(),
            }),
            Self::MonauralPair {
                first_hz,
                offset_hz,
                first_peak_dbfs,
                second_peak_dbfs,
                ..
            } => Some(Pair {
                first_hz: first_hz.get(),
                offset_hz: offset_hz.get(),
                first_peak_dbfs: first_peak_dbfs.get(),
                second_peak_dbfs: second_peak_dbfs.get(),
            }),
            _ => None,
        }
    }
    /// The shared route, absent for a binaural pair's separate ears.
    #[must_use]
    pub const fn route(self) -> Option<Ears> {
        match self {
            Self::Tone { ears, .. }
            | Self::Clicks { ears, .. }
            | Self::MonauralPair { ears, .. } => Some(ears),
            Self::BinauralPair { .. } => None,
        }
    }
    /// Whether this form needs separate ear channels.
    #[must_use]
    pub const fn separates_ears(self) -> bool {
        !matches!(self.route(), Some(Ears::Both))
    }
    pub(crate) const fn primitive(self) -> Option<Voice> {
        match self {
            Self::Tone {
                ears,
                frequency_hz,
                peak_dbfs,
                modulation,
            } => Some(Voice {
                ears,
                carrier: Carrier::Tone {
                    frequency_hz: frequency_hz.get(),
                },
                peak_dbfs: peak_dbfs.get(),
                modulation,
            }),
            Self::Clicks {
                ears,
                rate_hz,
                width_seconds,
                peak_dbfs,
                modulation,
            } => Some(Voice {
                ears,
                carrier: Carrier::Clicks {
                    rate_hz: rate_hz.get(),
                    width_seconds: width_seconds.get(),
                },
                peak_dbfs: peak_dbfs.get(),
                modulation,
            }),
            _ => None,
        }
    }
    /// Optional modulation on a primitive form.
    #[must_use]
    pub const fn modulation(self) -> Option<Modulation> {
        match self {
            Self::Tone { modulation, .. } | Self::Clicks { modulation, .. } => modulation,
            _ => None,
        }
    }
    /// Replace modulation when the authored form supports it.
    pub fn set_modulation(&mut self, value: Option<Modulation>) -> bool {
        match self {
            Self::Tone { modulation, .. } | Self::Clicks { modulation, .. } => {
                *modulation = value;
                true
            }
            _ => false,
        }
    }
    /// Primitive carrier choice; pairs have no carrier choice.
    #[must_use]
    pub const fn primitive_kind(self) -> Option<crate::CarrierKind> {
        match self {
            Self::Tone { .. } => Some(crate::CarrierKind::Tone),
            Self::Clicks { .. } => Some(crate::CarrierKind::Clicks),
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
        crate::rhythm::Method::primary_of(self).label()
    }
    /// Read an applicable authored quantity in model units.
    #[must_use]
    pub fn get(self, parameter: crate::ParameterId) -> Option<Finite> {
        use crate::ParameterId as P;
        match (parameter, self) {
            (P::ToneFrequency, Self::Tone { frequency_hz, .. }) => Some(frequency_hz),
            (P::ClickRate, Self::Clicks { rate_hz, .. }) => Some(rate_hz),
            (P::ClickWidth, Self::Clicks { width_seconds, .. }) => Some(width_seconds),
            (P::PeakLevel, Self::Tone { peak_dbfs, .. }) => Some(peak_dbfs),
            (P::PeakLevel, Self::Clicks { peak_dbfs, .. }) => Some(peak_dbfs),
            (P::PairFirstFrequency, Self::BinauralPair { left_hz, .. }) => Some(left_hz),
            (P::PairFirstFrequency, Self::MonauralPair { first_hz, .. }) => Some(first_hz),
            (P::PairOffset, Self::BinauralPair { offset_hz, .. }) => Some(offset_hz),
            (P::PairOffset, Self::MonauralPair { offset_hz, .. }) => Some(offset_hz),
            (P::PairFirstLevel, Self::BinauralPair { left_peak_dbfs, .. }) => Some(left_peak_dbfs),
            (
                P::PairFirstLevel,
                Self::MonauralPair {
                    first_peak_dbfs, ..
                },
            ) => Some(first_peak_dbfs),
            (
                P::PairSecondLevel,
                Self::BinauralPair {
                    right_peak_dbfs, ..
                },
            ) => Some(right_peak_dbfs),
            (
                P::PairSecondLevel,
                Self::MonauralPair {
                    second_peak_dbfs, ..
                },
            ) => Some(second_peak_dbfs),
            _ => match (parameter, self.modulation()) {
                (P::AmRate, Some(Modulation::Am { rate_hz, .. }))
                | (P::GateRate, Some(Modulation::Gate { rate_hz, .. })) => Some(rate_hz),
                (P::AmDepth, Some(Modulation::Am { depth, .. })) => Some(depth),
                (P::GateWidth, Some(Modulation::Gate { width_seconds, .. })) => Some(width_seconds),
                _ => None,
            },
        }
    }
    /// Change one applicable authored quantity, leaving all others unchanged.
    pub fn set(&mut self, parameter: crate::ParameterId, value: Finite) -> bool {
        use crate::ParameterId as P;
        let slot = match (parameter, self) {
            (P::ToneFrequency, Self::Tone { frequency_hz, .. }) => frequency_hz,
            (P::ClickRate, Self::Clicks { rate_hz, .. }) => rate_hz,
            (P::ClickWidth, Self::Clicks { width_seconds, .. }) => width_seconds,
            (P::PeakLevel, Self::Tone { peak_dbfs, .. }) => peak_dbfs,
            (P::PeakLevel, Self::Clicks { peak_dbfs, .. }) => peak_dbfs,
            (P::PairFirstFrequency, Self::BinauralPair { left_hz, .. }) => left_hz,
            (P::PairFirstFrequency, Self::MonauralPair { first_hz, .. }) => first_hz,
            (P::PairOffset, Self::BinauralPair { offset_hz, .. }) => offset_hz,
            (P::PairOffset, Self::MonauralPair { offset_hz, .. }) => offset_hz,
            (P::PairFirstLevel, Self::BinauralPair { left_peak_dbfs, .. }) => left_peak_dbfs,
            (
                P::PairFirstLevel,
                Self::MonauralPair {
                    first_peak_dbfs, ..
                },
            ) => first_peak_dbfs,
            (
                P::PairSecondLevel,
                Self::BinauralPair {
                    right_peak_dbfs, ..
                },
            ) => right_peak_dbfs,
            (
                P::PairSecondLevel,
                Self::MonauralPair {
                    second_peak_dbfs, ..
                },
            ) => second_peak_dbfs,
            (
                P::AmRate,
                Self::Tone {
                    modulation: Some(Modulation::Am { rate_hz, .. }),
                    ..
                }
                | Self::Clicks {
                    modulation: Some(Modulation::Am { rate_hz, .. }),
                    ..
                },
            ) => rate_hz,
            (
                P::GateRate,
                Self::Tone {
                    modulation: Some(Modulation::Gate { rate_hz, .. }),
                    ..
                }
                | Self::Clicks {
                    modulation: Some(Modulation::Gate { rate_hz, .. }),
                    ..
                },
            ) => rate_hz,
            (
                P::AmDepth,
                Self::Tone {
                    modulation: Some(Modulation::Am { depth, .. }),
                    ..
                }
                | Self::Clicks {
                    modulation: Some(Modulation::Am { depth, .. }),
                    ..
                },
            ) => depth,
            (
                P::GateWidth,
                Self::Tone {
                    modulation: Some(Modulation::Gate { width_seconds, .. }),
                    ..
                }
                | Self::Clicks {
                    modulation: Some(Modulation::Gate { width_seconds, .. }),
                    ..
                },
            ) => width_seconds,
            _ => return false,
        };
        *slot = value;
        true
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

#[cfg(test)]
#[path = "authored/tests.rs"]
mod tests;

crate::diagnostic_codes! {
    pub(crate) const CODES;
    PairIssue {
        codes {
            Self::Nonfinite=>"pair.nonfinite",
            Self::Nonpositive=>"pair.nonpositive",
            Self::UnrepresentablePairDifference=>"pair.unrepresentable_difference",
        }
        delegates {}
    }
    StructureError {
        codes {
            Self::Identity {..}=>"structure.identity",
            Self::ComponentBudget {..}=>"structure.component_budget",
            Self::VoiceBudget {..}=>"structure.voice_budget",
        }
        delegates {}
        params(this) {match this {
            Self::Identity {id}=>crate::diagnostic_params! {"component_id"=>id.get()},
            Self::ComponentBudget {count}=>crate::diagnostic_params! {"count"=>*count,"limit"=>Sound::MAX_COMPONENTS},
            Self::VoiceBudget {count}=>crate::diagnostic_params! {"count"=>*count,"limit"=>Sound::MAX_VOICES},
        }}
    }
    PairError {
        codes {}
        delegates {Self {issue,..}=>issue,}
        params(this) {crate::diagnostic_params! {"component_id"=>this.id.get(),"first_hz"=>this.first_hz,"offset_hz"=>this.offset_hz}}
    }
}
