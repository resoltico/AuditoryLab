// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The editable quantities of a recipe, their units and the component choices.
//!
//! Each [`ParameterId`] names one model quantity. Its [`ParameterDescriptor`]
//! states the label, the unit people read and type it in, and a short help
//! text. Model values keep the recipe's units (seconds, hertz, a 0..1 depth);
//! [`Unit`] converts them to and from the displayed unit.
use crate::{Carrier, Component, Ears, Modulation, Stimulus};
use core::fmt::{self, Display, Formatter};
use serde::Serialize;

/// The identity of a component within a target. It distinguishes components
/// that otherwise have the same values; allocation and non-reuse belong to the
/// document that owns the component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ComponentId {
    owner: u32,
    lane: Lane,
}

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
    /// Wrap a caller-chosen identity.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self {
            owner: value,
            lane: Lane::Primary,
        }
    }

    /// The identity's number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.owner
    }

    /// The generator lane; both lanes retain the same authored owner.
    #[must_use]
    pub const fn lane(self) -> Lane {
        self.lane
    }

    /// Address one lane without arithmetic, truncation or owner collisions.
    #[must_use]
    pub const fn with_lane(self, lane: Lane) -> Self {
        Self {
            owner: self.owner,
            lane,
        }
    }

    /// The identity a recipe gives the component at `index`: its position,
    /// counting from 1. `None` when the position does not fit the identity.
    #[must_use]
    pub fn numbered(index: usize) -> Option<Self> {
        u32::try_from(index).ok()?.checked_add(1).map(Self::new)
    }
}

impl Display for ComponentId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.lane {
            Lane::Primary => write!(f, "{}", self.owner),
            Lane::Secondary => write!(f, "{}:second", self.owner),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParameterId {
    /// A tone carrier's frequency.
    ToneFrequency,
    /// A click carrier's repetition rate.
    ClickRate,
    /// A click carrier's requested pulse length.
    ClickWidth,
    /// A component's peak level.
    PeakLevel,
    /// Sinusoidal amplitude modulation's rate.
    AmplitudeModulationRate,
    /// Sinusoidal amplitude modulation's depth.
    ModulationDepth,
    /// Rectangular gating's repetition rate.
    GateRate,
    /// Rectangular gating's requested open length.
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
    /// One plain sentence about the quantity's meaning.
    pub help: &'static str,
}

impl ParameterId {
    /// Every quantity, components' first in the order a component lists them.
    pub const ALL: [Self; 14] = [
        Self::ToneFrequency,
        Self::ClickRate,
        Self::ClickWidth,
        Self::PeakLevel,
        Self::AmplitudeModulationRate,
        Self::ModulationDepth,
        Self::GateRate,
        Self::GateWidth,
        Self::PairFirstFrequency,
        Self::PairOffset,
        Self::PairFirstLevel,
        Self::PairSecondLevel,
        Self::Duration,
        Self::Fade,
    ];

    /// The quantities a component can have, in display order.
    pub const COMPONENT: [Self; 12] = [
        Self::ToneFrequency,
        Self::ClickRate,
        Self::ClickWidth,
        Self::PeakLevel,
        Self::AmplitudeModulationRate,
        Self::ModulationDepth,
        Self::GateRate,
        Self::GateWidth,
        Self::PairFirstFrequency,
        Self::PairOffset,
        Self::PairFirstLevel,
        Self::PairSecondLevel,
    ];

    /// Label, unit and help.
    #[must_use]
    pub const fn descriptor(self) -> ParameterDescriptor {
        let (label, unit, help) = match self {
            Self::ToneFrequency => (
                "Tone frequency",
                Unit::Hertz,
                "How many cycles per second the sine tone has.",
            ),
            Self::ClickRate => (
                "Click rate",
                Unit::Hertz,
                "How many clicks start each second; every click starts at its exact scheduled time.",
            ),
            Self::ClickWidth => (
                "Click width",
                Unit::Milliseconds,
                "How long each click lasts; generation rounds it to whole frames at the sample rate.",
            ),
            Self::PeakLevel => (
                "Peak level",
                Unit::DecibelsFullScale,
                "The component's highest level relative to digital full scale; 0 dBFS is the maximum.",
            ),
            Self::AmplitudeModulationRate => (
                "Modulation rate",
                Unit::Hertz,
                "How many times each second the sinusoidal envelope rises and falls.",
            ),
            Self::ModulationDepth => (
                "Modulation depth",
                Unit::Percent,
                "How far the envelope dips: 0% leaves the level steady and 100% reaches silence.",
            ),
            Self::GateRate => (
                "Gate rate",
                Unit::Hertz,
                "How many times each second the gate opens.",
            ),
            Self::GateWidth => (
                "Gate width",
                Unit::Milliseconds,
                "How long each gate stays open; generation rounds it to whole frames at the sample rate.",
            ),
            Self::PairFirstFrequency => (
                "First / left carrier",
                Unit::Hertz,
                "The first carrier; changing it also changes the second carrier through the signed offset.",
            ),
            Self::PairOffset => (
                "Signed offset",
                Unit::Hertz,
                "Second carrier minus first carrier; its absolute value is the nominal pair rhythm.",
            ),
            Self::PairFirstLevel => (
                "First / left peak",
                Unit::DecibelsFullScale,
                "Peak bound of the first or left tone.",
            ),
            Self::PairSecondLevel => (
                "Second / right peak",
                Unit::DecibelsFullScale,
                "Peak bound of the second or right tone.",
            ),
            Self::Duration => ("Duration", Unit::Seconds, "The finite sound duration."),
            Self::Fade => (
                "Fade",
                Unit::Seconds,
                "The raised-cosine fade at each edge of a finite sound.",
            ),
        };
        ParameterDescriptor {
            id: self,
            label,
            unit,
            help,
        }
    }

    /// Whether this quantity belongs to a component rather than the whole recipe.
    #[must_use]
    pub const fn is_component(self) -> bool {
        !matches!(self, Self::Duration | Self::Fade)
    }

    /// The component's value of this quantity in model units, or `None` when
    /// its carrier or modulation does not have it.
    #[must_use]
    pub fn read(self, component: &Component) -> Option<f64> {
        match (self, component.carrier, component.modulation) {
            (Self::PeakLevel, _, _) => Some(component.peak_dbfs),
            (Self::ToneFrequency, Carrier::Tone { frequency_hz }, _) => Some(frequency_hz),
            (Self::ClickRate, Carrier::Click { rate_hz, .. }, _) => Some(rate_hz),
            (Self::ClickWidth, Carrier::Click { width_seconds, .. }, _) => Some(width_seconds),
            (Self::AmplitudeModulationRate, _, Some(Modulation::Sinusoidal { rate_hz, .. }))
            | (Self::GateRate, _, Some(Modulation::Rectangular { rate_hz, .. })) => Some(rate_hz),
            (Self::ModulationDepth, _, Some(Modulation::Sinusoidal { depth, .. })) => Some(depth),
            (Self::GateWidth, _, Some(Modulation::Rectangular { width_seconds, .. })) => {
                Some(width_seconds)
            }
            _ => None,
        }
    }

    /// Set the component's value of this quantity in model units. Returns
    /// `false`, changing nothing, when the component does not have it.
    pub fn write(self, component: &mut Component, value: f64) -> bool {
        let slot = match (self, &mut component.carrier, &mut component.modulation) {
            (Self::PeakLevel, _, _) => &mut component.peak_dbfs,
            (Self::ToneFrequency, Carrier::Tone { frequency_hz }, _) => frequency_hz,
            (Self::ClickRate, Carrier::Click { rate_hz, .. }, _) => rate_hz,
            (Self::ClickWidth, Carrier::Click { width_seconds, .. }, _) => width_seconds,
            (Self::AmplitudeModulationRate, _, Some(Modulation::Sinusoidal { rate_hz, .. }))
            | (Self::GateRate, _, Some(Modulation::Rectangular { rate_hz, .. })) => rate_hz,
            (Self::ModulationDepth, _, Some(Modulation::Sinusoidal { depth, .. })) => depth,
            (Self::GateWidth, _, Some(Modulation::Rectangular { width_seconds, .. })) => {
                width_seconds
            }
            _ => return false,
        };
        *slot = value;
        true
    }

    /// The stimulus's value of [`ParameterId::Duration`] or [`ParameterId::Fade`].
    #[must_use]
    pub const fn read_envelope(self, stimulus: &Stimulus) -> Option<f64> {
        match self {
            Self::Duration => Some(stimulus.duration_seconds),
            Self::Fade => Some(stimulus.fade_seconds),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CarrierKind {
    /// A sine tone.
    Tone,
    /// A train of rectangular clicks.
    Clicks,
}

impl CarrierKind {
    /// Every carrier kind in display order.
    pub const ALL: [Self; 2] = [Self::Tone, Self::Clicks];

    /// A short name for the kind.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tone => "Tone",
            Self::Clicks => "Clicks",
        }
    }
}

impl Carrier {
    /// The carrier's kind.
    #[must_use]
    pub const fn kind(&self) -> CarrierKind {
        match self {
            Self::Tone { .. } => CarrierKind::Tone,
            Self::Click { .. } => CarrierKind::Clicks,
        }
    }
}

/// The kind of amplitude modulation a component has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModulationKind {
    /// A sinusoidal envelope.
    Sinusoidal,
    /// Rectangular gating.
    Rectangular,
}

impl ModulationKind {
    /// Every modulation kind in display order.
    pub const ALL: [Self; 2] = [Self::Sinusoidal, Self::Rectangular];

    /// A short name for the kind.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Sinusoidal => "Sinusoidal",
            Self::Rectangular => "Gate",
        }
    }

    /// The name of having no modulation of either kind.
    pub const NONE_LABEL: &'static str = "None";
}

impl Modulation {
    /// The modulation's kind.
    #[must_use]
    pub const fn kind(&self) -> ModulationKind {
        match self {
            Self::Sinusoidal { .. } => ModulationKind::Sinusoidal,
            Self::Rectangular { .. } => ModulationKind::Rectangular,
        }
    }
}

impl Ears {
    /// Every route in display order.
    pub const ALL: [Self; 3] = [Self::Both, Self::Left, Self::Right];

    /// A short name for the route.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Both => "Both ears",
            Self::Left => "Left ear",
            Self::Right => "Right ear",
        }
    }
}

/// What a component does in the sound, from its carrier and modulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentRole {
    /// A gated carrier: a rhythm of on and off.
    GateRhythm,
    /// A carrier with an active sinusoidal envelope.
    AmplitudeModulation,
    /// An unmodulated click train.
    ClickRhythm,
    /// An unmodulated tone, or one whose envelope has zero depth.
    SteadyTone,
}

impl ComponentRole {
    /// A short name for the role.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::GateRhythm => "Gate rhythm",
            Self::AmplitudeModulation => "AM envelope",
            Self::ClickRhythm => "Click rhythm",
            Self::SteadyTone => "Steady tone",
        }
    }
}

impl Component {
    /// The component's role; active modulation is never hidden.
    #[must_use]
    pub fn role(&self) -> ComponentRole {
        match self.modulation {
            Some(Modulation::Rectangular { .. }) => ComponentRole::GateRhythm,
            Some(Modulation::Sinusoidal { depth, .. }) if depth > 0.0 => {
                ComponentRole::AmplitudeModulation
            }
            _ => match self.carrier {
                Carrier::Click { .. } => ComponentRole::ClickRhythm,
                Carrier::Tone { .. } => ComponentRole::SteadyTone,
            },
        }
    }

    /// The component's quantities that are not finite numbers.
    pub fn nonfinite(&self) -> impl Iterator<Item = ParameterId> + '_ {
        ParameterId::COMPONENT
            .into_iter()
            .filter(|id| id.read(self).is_some_and(|value| !value.is_finite()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn every_shape() -> Vec<Component> {
        let mut shapes = Vec::new();
        for carrier in [
            Carrier::Tone {
                frequency_hz: 440.0,
            },
            Carrier::Click {
                rate_hz: 40.0,
                width_seconds: 0.001,
            },
        ] {
            for modulation in [
                None,
                Some(Modulation::Sinusoidal {
                    rate_hz: 8.0,
                    depth: 0.5,
                }),
                Some(Modulation::Rectangular {
                    rate_hz: 8.0,
                    width_seconds: 0.002,
                }),
            ] {
                shapes.push(Component {
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
        for shape in every_shape() {
            for id in ParameterId::COMPONENT {
                let mut changed = shape;
                let applies = id.read(&shape).is_some();
                assert_eq!(id.write(&mut changed, 0.25), applies, "{id:?}");
                if applies {
                    assert_eq!(id.read(&changed), Some(0.25));
                    for other in ParameterId::COMPONENT {
                        if other != id {
                            assert_eq!(other.read(&changed), other.read(&shape));
                        }
                    }
                } else {
                    assert_eq!(changed, shape);
                }
            }
        }
        // Independently: a gated tone has frequency, level, gate rate and width.
        let gated = every_shape()[2];
        let present: Vec<_> = ParameterId::COMPONENT
            .into_iter()
            .filter(|id| id.read(&gated).is_some())
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
        assert_eq!(ParameterId::ModulationDepth.descriptor().unit.symbol(), "%");
        for id in ParameterId::ALL {
            let descriptor = id.descriptor();
            assert_eq!(descriptor.id, id);
            assert!(!descriptor.label.is_empty() && descriptor.help.ends_with('.'));
            assert_eq!(id.is_component(), ParameterId::COMPONENT.contains(&id));
        }
    }

    #[test]
    fn nonfinite_quantities_are_named_and_roles_never_hide_active_modulation() {
        let mut c = every_shape()[1];
        assert_eq!(c.role(), ComponentRole::AmplitudeModulation);
        c.modulation = Some(Modulation::Sinusoidal {
            rate_hz: f64::INFINITY,
            depth: 0.0,
        });
        assert_eq!(c.role(), ComponentRole::SteadyTone);
        c.peak_dbfs = f64::NAN;
        assert_eq!(
            c.nonfinite().collect::<Vec<_>>(),
            [ParameterId::PeakLevel, ParameterId::AmplitudeModulationRate]
        );
        assert_eq!(ComponentId::numbered(0), Some(ComponentId::new(1)));
        assert_eq!(ComponentId::numbered(usize::MAX), None);
    }
}
