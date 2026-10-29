//! Authored shared component gain, independent of source relationships.
use serde::{Deserialize, Serialize};
/// A nonnegative gain shared by every lane of one component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Modulation {
    /// Bounded `(1 + depth*sin(phase))/(1+depth)` amplitude modulation.
    Sinusoidal {
        /// Modulation frequency in hertz.
        rate_hz: crate::Finite,
        /// Depth from zero to one.
        depth: crate::Finite,
    },
    /// A pulse envelope applied to the whole source.
    Pulsed {
        /// Requested repetition, width intent and edge shape.
        pulse: crate::PulseSpec,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fields {
    kind: alloc::string::String,
    rate_hz: Option<crate::Finite>,
    depth: Option<crate::Finite>,
    pulse: Option<crate::PulseSpec>,
}
impl Fields {
    fn decode<E: serde::de::Error>(self) -> Result<Option<Modulation>, E> {
        use crate::model::table_fields::{only, required};
        let present = [
            ("rate_hz", self.rate_hz.is_some()),
            ("depth", self.depth.is_some()),
            ("pulse", self.pulse.is_some()),
        ];
        Ok(match self.kind.as_str() {
            "none" => {
                only::<E>(&present, &[])?;
                None
            }
            "sinusoidal" => {
                only::<E>(&present, &["rate_hz", "depth"])?;
                Some(Modulation::Sinusoidal {
                    rate_hz: required(self.rate_hz, "rate_hz")?,
                    depth: required(self.depth, "depth")?,
                })
            }
            "pulsed" => {
                only::<E>(&present, &["pulse"])?;
                Some(Modulation::Pulsed {
                    pulse: required(self.pulse, "pulse")?,
                })
            }
            _ => {
                return Err(E::unknown_variant(
                    &self.kind,
                    &["none", "sinusoidal", "pulsed"],
                ));
            }
        })
    }
}
impl<'de> Deserialize<'de> for Modulation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        Fields::deserialize(d)?
            .decode()?
            .ok_or_else(|| D::Error::custom("expected active modulation"))
    }
}
impl Modulation {
    pub(crate) fn get(self, p: crate::ParameterId) -> Option<crate::Finite> {
        use crate::ParameterId as P;
        match self {
            Self::Sinusoidal { rate_hz, depth } => match p {
                P::ModulationRate => Some(rate_hz),
                P::ModulationDepth => Some(depth),
                _ => None,
            },
            Self::Pulsed { pulse } => pulse.get(p, true),
        }
    }
    pub(crate) fn set(&mut self, p: crate::ParameterId, v: crate::Finite) -> bool {
        use crate::ParameterId as P;
        match self {
            Self::Sinusoidal { rate_hz, depth } => match p {
                P::ModulationRate => {
                    *rate_hz = v;
                    true
                }
                P::ModulationDepth => {
                    *depth = v;
                    true
                }
                _ => false,
            },
            Self::Pulsed { pulse } => pulse.set(p, v, true),
        }
    }
}
/// The canonical optional representation always has a required kind table.
pub(crate) mod optional {
    use super::{Deserialize, Fields, Modulation, Serialize};
    #[derive(Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    enum Empty {
        None {},
    }
    #[expect(
        clippy::ref_option,
        reason = "Serde with-module serializers receive the actual optional field by reference"
    )]
    pub(crate) fn serialize<S: serde::Serializer>(
        v: &Option<Modulation>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        match v {
            Some(v) => v.serialize(s),
            None => Empty::None {}.serialize(s),
        }
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Modulation>, D::Error> {
        Fields::deserialize(d)?.decode()
    }
}
