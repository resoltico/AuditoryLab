// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The editable quantities of a recipe, their units and the component choices.
//!
//! Each [`ParameterId`] names one model quantity. Its [`ParameterDescriptor`]
//! states the label, the unit people read and type it in, and a short help
//! text. Model values keep the recipe's units (seconds, hertz, a 0..1 depth);
//! [`Unit`] converts them to and from the displayed unit.
#[cfg(test)]
use crate::Carrier;
use crate::Modulation;
#[cfg(test)]
use crate::SourceLane;
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

macro_rules! define_parameters {
    (parameters {
        component {$( $(#[$component_doc:meta])* $component:ident => ($component_key:literal,
            $component_label:expr,$component_phrase:expr,$component_unit:ident,
            $component_rule:expr); )*}
        envelope {$( $(#[$envelope_doc:meta])* $envelope:ident => ($envelope_key:literal,
            $envelope_label:expr,$envelope_phrase:expr,$envelope_unit:ident,$envelope_rule:expr);
            )*}
    } $($rest:tt)*)=>{
        /// One editable recipe quantity.
        #[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Hash)]
        pub enum ParameterId {
            $($(#[$component_doc])* $component,)*
            $($(#[$envelope_doc])* $envelope,)*
        }
        crate::vocabulary::vocabulary! {ParameterId,"parameter_id" {
            $($component=>($component_key,$component_label,$component_phrase),)*
            $($envelope=>($envelope_key,$envelope_label,$envelope_phrase),)*
        }}
        impl ParameterId {
            /// Rust variant identifier for generated source; use key() for wire data.
            #[doc(hidden)]
            pub const fn variant_name(self)->&'static str {match self {
                $(Self::$component=>stringify!($component),
                )*$(Self::$envelope=>stringify!($envelope),)*}}
            /// Quantities applicable to components, in display order.
            pub const COMPONENT:[Self;[$(stringify!($component)),*].len()]=[$(Self::$component),*];
            /// Whether this quantity belongs to a component.
            #[must_use]
            pub const fn is_component(self)->bool {match self {$(Self::$component=>true,
                )*$(Self::$envelope=>false,)*}}
            const fn unit(self)->Unit {match self {$(Self::$component=>Unit::$component_unit,
                )*$(Self::$envelope=>Unit::$envelope_unit,)*}}
            pub(crate) const fn value_rule(self)->Option<crate::ValueRule> {match self {
                $(Self::$component=>$component_rule,)*$(Self::$envelope=>$envelope_rule,)*}}
        }
    };
}
crate::parameter_schema::parameter_schema!(define_parameters);

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
    /// Label and model/display unit of this quantity.
    #[must_use]
    pub const fn descriptor(self) -> ParameterDescriptor {
        ParameterDescriptor {
            id: self,
            label: self.label(),
            unit: self.unit(),
        }
    }

    /// The sound's value of [`ParameterId::Duration`] or [`ParameterId::Fade`].
    #[must_use]
    pub const fn read_envelope(self, timing: &crate::FiniteTiming) -> Option<crate::Finite> {
        match self {
            Self::Duration => Some(timing.duration_seconds),
            Self::Fade => Some(timing.fade_seconds),
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
    /// A train of explicitly shaped unipolar pulses.
    PulseTrain,
}

/// The kind of amplitude modulation a component has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModulationKind {
    /// A sinusoidal envelope.
    Sinusoidal,
    /// Periodic gain pulses with an explicit window shape.
    Pulsed,
}

impl Modulation {
    /// The modulation's kind.
    #[must_use]
    pub const fn kind(&self) -> ModulationKind {
        match self {
            Self::Sinusoidal { .. } => ModulationKind::Sinusoidal,
            Self::Pulsed { .. } => ModulationKind::Pulsed,
        }
    }
}

crate::vocabulary::vocabulary! { CarrierKind,"carrier_kind" {
    Tone => ("tone","Tone","tone"),
    PulseTrain => ("pulse_train","Pulse train","pulse train"),
}}

crate::vocabulary::vocabulary! { ModulationKind,"modulation_kind" {
    Sinusoidal => ("sinusoidal","Sinusoidal AM","sinusoidal AM"),
    Pulsed => ("pulsed","Pulsed modulation","pulsed modulation"),
}}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChannelRouting;
    use alloc::vec::Vec;

    fn every_shape() -> Vec<SourceLane> {
        let mut shapes = Vec::new();
        for carrier in [
            Carrier::Tone {
                frequency_hz: 440.0,
            },
            Carrier::PulseTrain {
                pulse: crate::test_support::rectangular_pulse(40.0, 0.001),
            },
        ] {
            for modulation in [
                None,
                Some(Modulation::Sinusoidal {
                    rate_hz: crate::test_support::finite(8.0),
                    depth: crate::test_support::finite(0.5),
                }),
                Some(Modulation::Pulsed {
                    pulse: crate::PulseSpec {
                        rate_hz: crate::test_support::finite(8.0),
                        width: crate::PulseWidth::Duration {
                            seconds: crate::test_support::finite(0.002),
                        },
                        shape: crate::PulseShape::Rectangular {},
                    },
                }),
            ] {
                shapes.push(SourceLane {
                    route: ChannelRouting::Both,
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
                ParameterId::ModulationPulseRate,
                ParameterId::ModulationPulseDuration
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
        assert_eq!(
            ParameterId::SourcePulseDuration.descriptor().unit.symbol(),
            "ms"
        );
        assert_eq!(ParameterId::ModulationDepth.descriptor().unit.symbol(), "%");
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
        assert_eq!(
            crate::test_support::form(c).heading(),
            "Tone with sinusoidal AM"
        );
        c.modulation = Some(Modulation::Sinusoidal {
            rate_hz: crate::test_support::finite(0.0),
            depth: crate::test_support::finite(0.0),
        });
        assert_eq!(crate::test_support::form(c).heading(), "Steady tone");
        let form = crate::test_support::form(c);
        let issues: Vec<_> = form.intrinsic_issues().collect();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].parameter, ParameterId::ModulationRate);
        assert_eq!(issues[0].rule, crate::ValueRule::Positive);
        assert_eq!(ComponentId::numbered(0), ComponentId::new(1));
        assert_eq!(ComponentId::numbered(usize::MAX), None);
    }
}
