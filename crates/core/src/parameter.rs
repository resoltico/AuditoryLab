// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The editable quantities of a recipe, their units and the component choices.
//!
//! Each [`ParameterId`] names one model quantity. Its [`ParameterDescriptor`]
//! states the label, the unit people read and type it in, and a short help
//! text. Model values keep the recipe's units (seconds, hertz, a 0..1 depth);
//! [`Unit`] converts them to and from the displayed unit.
use crate::{Carrier, Modulation, Sound, Voice};
use core::fmt::{self, Display, Formatter};
use serde::Serialize;

/// A positive authored owner identity, independent of expanded generator lanes.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Deserialize, Serialize,
)]
#[serde(transparent)]
pub struct ComponentId(core::num::NonZeroU32);

/// A stable generator lane within one authored component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    /// Raw primitive, or the first/left voice of a pair.
    Primary,
    /// Second/right voice of a pair.
    Secondary,
}
impl ComponentId {
    /// Construct an authored identity; zero has no identity.
    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        match core::num::NonZeroU32::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
    /// The authored identity's positive number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
    /// The identity of an element at a zero-based position, when representable.
    #[must_use]
    pub fn numbered(index: usize) -> Option<Self> {
        Self::new(u32::try_from(index).ok()?.checked_add(1)?)
    }
    /// Address a generator lane belonging to this authored component.
    #[must_use]
    pub const fn with_lane(self, lane: Lane) -> VoiceId {
        VoiceId {
            component: self,
            lane,
        }
    }
}
impl Display for ComponentId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
/// One expanded generator identity, retaining its authored owner and lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct VoiceId {
    /// Authored owner of this generator.
    pub component: ComponentId,
    /// Stable lane within the authored owner.
    pub lane: Lane,
}
impl VoiceId {
    /// The authored owner's number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.component.get()
    }
    /// The stable generator lane.
    #[must_use]
    pub const fn lane(self) -> Lane {
        self.lane
    }
    /// Address another generator of the same authored component.
    #[must_use]
    pub const fn with_lane(self, lane: Lane) -> Self {
        Self { lane, ..self }
    }
}
impl From<ComponentId> for VoiceId {
    fn from(component: ComponentId) -> Self {
        component.with_lane(Lane::Primary)
    }
}
impl Display for VoiceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.lane {
            Lane::Primary => self.component.fmt(f),
            Lane::Secondary => write!(f, "{}:second", self.component),
        }
    }
}

/// A unit a quantity is displayed and entered in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Cycles per second; model values are hertz.
    Hertz,
    /// Seconds; model values are seconds.
    Seconds,
    /// Milliseconds; model values are seconds.
    Milliseconds,
    /// Percent; model values are fractions from 0 to 1.
    Percent,
    /// Decibels relative to digital full scale; model values are dBFS.
    DecibelsFullScale,
}

impl Unit {
    /// The unit's symbol.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Hertz => "Hz",
            Self::Seconds => "s",
            Self::Milliseconds => "ms",
            Self::Percent => "%",
            Self::DecibelsFullScale => "dBFS",
        }
    }

    /// Displayed units per model unit.
    const fn scale(self) -> f64 {
        match self {
            Self::Hertz | Self::Seconds | Self::DecibelsFullScale => 1.0,
            Self::Milliseconds => 1000.0,
            Self::Percent => 100.0,
        }
    }

    /// Convert a model value to this unit.
    #[must_use]
    pub fn to_display(self, model: f64) -> f64 {
        model * self.scale()
    }

    /// Convert a value in this unit to the model's unit.
    #[must_use]
    pub fn to_model(self, display: f64) -> f64 {
        display / self.scale()
    }
}

/// One editable recipe quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterId {
    /// A tone carrier's frequency.
    ToneFrequency,
    /// A click carrier's repetition rate.
    ClickRate,
    /// A click carrier's requested pulse length.
    ClickWidth,
    /// A component's peak level.
    PeakLevel,
    /// Sine amplitude modulation's rate.
    AmRate,
    /// Sine amplitude modulation's depth.
    AmDepth,
    /// Gating's repetition rate.
    GateRate,
    /// Gating's requested open length.
    GateWidth,
    /// First/left carrier of an authored pair.
    PairFirstFrequency,
    /// Signed nominal carrier offset of an authored pair.
    PairOffset,
    /// First/left peak level of an authored pair.
    PairFirstLevel,
    /// Second/right peak level of an authored pair.
    PairSecondLevel,
    /// The finite length of saved and exported audio.
    Duration,
    /// The edge fade of saved and exported audio.
    Fade,
}

/// What a person needs to read, enter and understand one quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParameterDescriptor {
    /// The quantity described.
    pub id: ParameterId,
    /// A short name for the quantity.
    pub label: &'static str,
    /// The unit the quantity is displayed and entered in.
    pub unit: Unit,
}

impl ParameterId {
    /// The quantities a component can have, in display order.
    pub const COMPONENT: [Self; 12] = [
        Self::ToneFrequency,
        Self::ClickRate,
        Self::ClickWidth,
        Self::PeakLevel,
        Self::AmRate,
        Self::AmDepth,
        Self::GateRate,
        Self::GateWidth,
        Self::PairFirstFrequency,
        Self::PairOffset,
        Self::PairFirstLevel,
        Self::PairSecondLevel,
    ];

    /// Label and model/display unit of this quantity.
    #[must_use]
    pub const fn descriptor(self) -> ParameterDescriptor {
        ParameterDescriptor {
            id: self,
            label: self.label(),
            unit: self.unit(),
        }
    }

    /// Whether this quantity belongs to a component rather than the whole recipe.
    #[must_use]
    pub const fn is_component(self) -> bool {
        !matches!(self, Self::Duration | Self::Fade)
    }

    /// The sound's value of [`ParameterId::Duration`] or [`ParameterId::Fade`].
    #[must_use]
    pub const fn read_envelope(self, sound: &Sound) -> Option<crate::Finite> {
        match self {
            Self::Duration => Some(sound.duration_seconds()),
            Self::Fade => Some(sound.fade_seconds()),
            _ => None,
        }
    }
}

impl Display for ParameterId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.descriptor().label)
    }
}

/// The kind of periodic source a component has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CarrierKind {
    /// A sine tone.
    Tone,
    /// A train of rectangular clicks.
    Clicks,
}

impl Carrier {
    /// The carrier's kind.
    #[must_use]
    pub const fn kind(&self) -> CarrierKind {
        match self {
            Self::Tone { .. } => CarrierKind::Tone,
            Self::Clicks { .. } => CarrierKind::Clicks,
        }
    }
}

/// The kind of amplitude modulation a component has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModulationKind {
    /// A sinusoidal envelope.
    Am,
    /// Periodic rectangular gating.
    Gate,
}

impl ModulationKind {
    /// The name of having no modulation of either kind.
    pub const NONE_LABEL: &'static str = "None";
}

impl Modulation {
    /// The modulation's kind.
    #[must_use]
    pub const fn kind(&self) -> ModulationKind {
        match self {
            Self::Am { .. } => ModulationKind::Am,
            Self::Gate { .. } => ModulationKind::Gate,
        }
    }
}

/// What a component does in the sound, from its carrier and modulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentRole {
    /// A gated carrier: a rhythm of on and off.
    GateRhythm,
    /// A carrier with an active sinusoidal envelope.
    AmEnvelope,
    /// An unmodulated click train.
    ClickRhythm,
    /// An unmodulated tone, or one whose envelope has zero depth.
    SteadyTone,
}

impl Voice {
    /// The component's role; active modulation is never hidden.
    #[must_use]
    pub(crate) fn role(&self) -> ComponentRole {
        match self.modulation {
            Some(Modulation::Gate { .. }) => ComponentRole::GateRhythm,
            Some(Modulation::Am { depth, .. }) if depth.get() > 0.0 => ComponentRole::AmEnvelope,
            _ => match self.carrier {
                Carrier::Clicks { .. } => ComponentRole::ClickRhythm,
                Carrier::Tone { .. } => ComponentRole::SteadyTone,
            },
        }
    }
}

crate::vocabulary::vocabulary! { ParameterId,"parameter_id" {
    ToneFrequency => ("tone_frequency","Tone frequency","tone frequency"),
    ClickRate => ("click_rate","Clicks rate","clicks rate"),
    ClickWidth => ("click_width","Clicks width","clicks width"),
    PeakLevel => ("peak_level","Peak level","peak level"),
    AmRate => ("am_rate","Modulation rate","modulation rate"),
    AmDepth => ("am_depth","Modulation depth","modulation depth"),
    GateRate => ("gate_rate","Gate rate","gate rate"),
    GateWidth => ("gate_width","Gate width","gate width"),
    PairFirstFrequency => ("pair_first_frequency","First / left carrier","first / left carrier"),
    PairOffset => ("pair_offset","Signed offset","signed offset"),
    PairFirstLevel => ("pair_first_level","First / left peak","first / left peak"),
    PairSecondLevel => ("pair_second_level","Second / right peak","second / right peak"),
    Duration => ("duration","Duration","duration"),
    Fade => ("fade","Fade","fade"),
}}

crate::vocabulary::vocabulary! { CarrierKind,"carrier_kind" {
    Tone => ("tone","Tone","tone"),
    Clicks => ("clicks","Clicks","clicks"),
}}

crate::vocabulary::vocabulary! { ModulationKind,"modulation_kind" {
    Am => ("am","AM","AM"),
    Gate => ("gate","Gate","gate"),
}}

crate::vocabulary::vocabulary! { ComponentRole,"component_role" {
    GateRhythm => ("gate_rhythm","Gate rhythm","gate rhythm"),
    AmEnvelope => ("am_envelope","AM envelope","AM envelope"),
    ClickRhythm => ("click_rhythm","Click rhythm","click rhythm"),
    SteadyTone => ("steady_tone","Steady tone","steady tone"),
}}

impl ParameterId {
    const fn unit(self) -> Unit {
        match self {
            Self::ToneFrequency => Unit::Hertz,
            Self::ClickRate => Unit::Hertz,
            Self::ClickWidth => Unit::Milliseconds,
            Self::PeakLevel => Unit::DecibelsFullScale,
            Self::AmRate => Unit::Hertz,
            Self::AmDepth => Unit::Percent,
            Self::GateRate => Unit::Hertz,
            Self::GateWidth => Unit::Milliseconds,
            Self::PairFirstFrequency => Unit::Hertz,
            Self::PairOffset => Unit::Hertz,
            Self::PairFirstLevel => Unit::DecibelsFullScale,
            Self::PairSecondLevel => Unit::DecibelsFullScale,
            Self::Duration => Unit::Seconds,
            Self::Fade => Unit::Seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ears;
    use alloc::vec::Vec;

    fn every_shape() -> Vec<Voice> {
        let mut shapes = Vec::new();
        for carrier in [
            Carrier::Tone {
                frequency_hz: 440.0,
            },
            Carrier::Clicks {
                rate_hz: 40.0,
                width_seconds: 0.001,
            },
        ] {
            for modulation in [
                None,
                Some(Modulation::Am {
                    rate_hz: crate::test_support::finite(8.0),
                    depth: crate::test_support::finite(0.5),
                }),
                Some(Modulation::Gate {
                    rate_hz: crate::test_support::finite(8.0),
                    width_seconds: crate::test_support::finite(0.002),
                }),
            ] {
                shapes.push(Voice {
                    ears: Ears::Both,
                    peak_dbfs: -30.0,
                    carrier,
                    modulation,
                });
            }
        }
        shapes
    }

    #[test]
    fn every_component_quantity_reads_back_what_was_written_and_nothing_else() {
        for shape in every_shape().into_iter().map(crate::test_support::form) {
            for id in ParameterId::COMPONENT {
                let mut changed = shape;
                let applies = shape.get(id).is_some();
                assert_eq!(
                    changed.set(id, crate::test_support::finite(0.25)),
                    applies,
                    "{id:?}"
                );
                if applies {
                    assert_eq!(changed.get(id), Some(crate::test_support::finite(0.25)));
                    for other in ParameterId::COMPONENT {
                        if other != id {
                            assert_eq!(changed.get(other), shape.get(other));
                        }
                    }
                } else {
                    assert_eq!(changed, shape);
                }
            }
        }
        // Independently: a gated tone has frequency, level, gate rate and width.
        let gated = crate::test_support::form(every_shape()[2]);
        let present: Vec<_> = ParameterId::COMPONENT
            .into_iter()
            .filter(|id| gated.get(*id).is_some())
            .collect();
        assert_eq!(
            present,
            [
                ParameterId::ToneFrequency,
                ParameterId::PeakLevel,
                ParameterId::GateRate,
                ParameterId::GateWidth
            ]
        );
    }

    #[test]
    fn units_convert_both_ways_with_their_stated_scale() {
        assert_eq!(Unit::Milliseconds.to_display(0.001), 1.0);
        assert_eq!(Unit::Milliseconds.to_model(25.0), 0.025);
        assert_eq!(Unit::Percent.to_display(0.5), 50.0);
        assert_eq!(Unit::Percent.to_model(25.0), 0.25);
        for unit in [Unit::Hertz, Unit::Seconds, Unit::DecibelsFullScale] {
            assert_eq!(unit.to_display(-3.5), -3.5);
            assert_eq!(unit.to_model(-3.5), -3.5);
        }
        assert_eq!(ParameterId::ClickWidth.descriptor().unit.symbol(), "ms");
        assert_eq!(ParameterId::AmDepth.descriptor().unit.symbol(), "%");
        for id in ParameterId::ALL {
            let descriptor = id.descriptor();
            assert_eq!(descriptor.id, id);
            assert!(!descriptor.label.is_empty() && !id.phrase().is_empty());
            assert_eq!(id.is_component(), ParameterId::COMPONENT.contains(&id));
        }
    }

    #[test]
    fn finite_invalid_rates_never_hide_active_modulation() {
        let mut c = every_shape()[1];
        assert_eq!(c.role(), ComponentRole::AmEnvelope);
        c.modulation = Some(Modulation::Am {
            rate_hz: crate::test_support::finite(0.0),
            depth: crate::test_support::finite(0.0),
        });
        assert_eq!(c.role(), ComponentRole::SteadyTone);
        let form = crate::test_support::form(c);
        let issues: Vec<_> = form.intrinsic_issues().collect();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].parameter, ParameterId::AmRate);
        assert_eq!(issues[0].rule, crate::ValueRule::Positive);
        assert_eq!(ComponentId::numbered(0), ComponentId::new(1));
        assert_eq!(ComponentId::numbered(usize::MAX), None);
    }
}
