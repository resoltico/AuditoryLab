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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComponentForm {
    /// Authored tone quantities.
    Tone {
        /// The requested ears.
        ears: Ears,
        /// The requested frequency hz.
        frequency_hz: f64,
        /// The requested peak dbfs.
        peak_dbfs: f64,
        /// The requested modulation.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        modulation: Option<Modulation>,
    },
    /// Authored clicks quantities.
    Clicks {
        /// The requested ears.
        ears: Ears,
        /// The requested rate hz.
        rate_hz: f64,
        /// The requested width seconds.
        width_seconds: f64,
        /// The requested peak dbfs.
        peak_dbfs: f64,
        /// The requested modulation.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        modulation: Option<Modulation>,
    },
    /// Authored binaural_pair quantities.
    BinauralPair {
        /// The requested left hz.
        left_hz: f64,
        /// The requested offset hz.
        offset_hz: f64,
        /// The requested left peak dbfs.
        left_peak_dbfs: f64,
        /// The requested right peak dbfs.
        right_peak_dbfs: f64,
    },
    /// Authored monaural_pair quantities.
    MonauralPair {
        /// The requested ears.
        ears: Ears,
        /// The requested first hz.
        first_hz: f64,
        /// The requested offset hz.
        offset_hz: f64,
        /// The requested first peak dbfs.
        first_peak_dbfs: f64,
        /// The requested second peak dbfs.
        second_peak_dbfs: f64,
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
            Self::Nonfinite => "pair.nonfinite",
            Self::Nonpositive => "pair.nonpositive",
            Self::UnrepresentablePairDifference => "pair.unrepresentable_difference",
        })
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
impl fmt::Display for StructureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identity { id } => write!(f, "structure.identity.{id}"),
            Self::ComponentBudget { count } => write!(f, "structure.component_budget.{count}"),
            Self::VoiceBudget { count } => write!(f, "structure.voice_budget.{count}"),
        }
    }
}
impl fmt::Display for PairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.issue, self.id)
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
                let mut frequency_hz: Option<f64> = None;
                let mut peak_dbfs: Option<f64> = None;
                let mut modulation: Option<Option<Modulation>> = None;
                let mut rate_hz: Option<f64> = None;
                let mut width_seconds: Option<f64> = None;
                let mut left_hz: Option<f64> = None;
                let mut offset_hz: Option<f64> = None;
                let mut left_peak_dbfs: Option<f64> = None;
                let mut right_peak_dbfs: Option<f64> = None;
                let mut first_hz: Option<f64> = None;
                let mut first_peak_dbfs: Option<f64> = None;
                let mut second_peak_dbfs: Option<f64> = None;
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
    pub fn check_structure(components: &[Component]) -> Result<(), StructureError> {
        if components.len() > Self::MAX_COMPONENTS {
            return Err(StructureError::ComponentBudget {
                count: components.len(),
            });
        }
        let count = components.iter().map(|c| c.form.voices()).sum::<usize>();
        if count > Self::MAX_VOICES {
            return Err(StructureError::VoiceBudget { count });
        }
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
        self.components.iter().any(|c| c.form.separates_ears())
    }
    /// Nonfinite authored values, without inferring or realizing derived carriers.
    pub fn nonfinite(&self) -> impl Iterator<Item = (Option<usize>, crate::ParameterId)> + '_ {
        [
            (crate::ParameterId::Duration, self.duration_seconds),
            (crate::ParameterId::Fade, self.fade_seconds),
        ]
        .into_iter()
        .filter(|(p, v)| {
            matches!(
                crate::validity::check_value(*p, *v),
                Err(crate::ValueIssue {
                    rule: crate::ValueRule::Finite,
                    ..
                })
            )
        })
        .map(|(p, _)| (None, p))
        .chain(self.components.iter().enumerate().flat_map(|(i, c)| {
            crate::ParameterId::COMPONENT
                .into_iter()
                .filter(move |p| {
                    c.form.get(*p).is_some_and(|v| {
                        matches!(
                            crate::validity::check_value(*p, v),
                            Err(crate::ValueIssue {
                                rule: crate::ValueRule::Finite,
                                ..
                            })
                        )
                    })
                })
                .map(move |p| (Some(i), p))
        }))
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
                first_hz: left_hz,
                offset_hz,
                first_peak_dbfs: left_peak_dbfs,
                second_peak_dbfs: right_peak_dbfs,
            }),
            Self::MonauralPair {
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
                carrier: Carrier::Tone { frequency_hz },
                peak_dbfs,
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
                    rate_hz,
                    width_seconds,
                },
                peak_dbfs,
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
        match self.pair_kind() {
            Some(kind) => kind.label(),
            None => self
                .primitive()
                .unwrap_or_else(|| unreachable!("primitive form"))
                .role()
                .label(),
        }
    }
    /// Read an applicable authored quantity in model units.
    #[must_use]
    pub fn get(self, parameter: crate::ParameterId) -> Option<f64> {
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
    pub fn set(&mut self, parameter: crate::ParameterId, value: f64) -> bool {
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
    /// Exact authored comparison, including signed zeros.
    #[must_use]
    pub fn same_content(self, other: Self) -> bool {
        core::mem::discriminant(&self) == core::mem::discriminant(&other)
            && self.route() == other.route()
            && self.modulation_kind() == other.modulation_kind()
            && crate::ParameterId::COMPONENT
                .into_iter()
                .all(|p| self.get(p).map(f64::to_bits) == other.get(p).map(f64::to_bits))
    }
    pub(crate) fn intrinsic_issues(&self) -> impl Iterator<Item = crate::ValueIssue> + '_ {
        crate::ParameterId::COMPONENT.into_iter().filter_map(|p| {
            self.get(p)
                .and_then(|v| crate::validity::check_value(p, v).err())
        })
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

impl PairKind {
    /// Every explicit pair construction, in display order.
    pub const ALL: [Self; 2] = [Self::Binaural, Self::Monaural];
    const fn terms(self) -> (&'static str, &'static str) {
        match self {
            Self::Binaural => ("Binaural pair", "binaural pair"),
            Self::Monaural => ("Monaural pair", "monaural pair"),
        }
    }
    /// Domain name for a heading or choice.
    #[must_use]
    pub const fn label(self) -> &'static str {
        self.terms().0
    }
    /// Domain name within a sentence.
    #[must_use]
    pub const fn phrase(self) -> &'static str {
        self.terms().1
    }
}
