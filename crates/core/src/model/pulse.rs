//! Authored pulse intent, independent of sample-rate resolution.
use crate::Finite;
use serde::{Deserialize, Serialize};

/// A repetition process with one authoritative width and explicit edge shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PulseSpec {
    /// Repetitions per second, interpreted as the exact binary rational.
    pub rate_hz: Finite,
    /// Requested window width.
    pub width: PulseWidth,
    /// Gain within that window.
    pub shape: PulseShape,
}

/// Width editing intent; exactly one quantity is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PulseWidth {
    /// A fixed duration, unchanged when repetition rate changes.
    Duration {
        /// Window duration in seconds.
        seconds: Finite,
    },
    /// A fraction of the nominal repetition period.
    DutyCycle {
        /// Strictly between zero and one when compiled.
        fraction: Finite,
    },
}

impl PulseWidth {
    /// Whether a requested duty fraction satisfies the intrinsic domain rule.
    pub fn valid_fraction(fraction: f64) -> bool {
        fraction > 0.0 && fraction < 1.0
    }
}

/// Sampled pulse envelope; smooth edges are not band limited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PulseShape {
    /// Unit gain throughout the window, including both endpoints.
    Rectangular {},
    /// Zero-valued endpoints with positive raised-cosine edges.
    RaisedCosine {
        /// Positive rise duration in seconds.
        rise_seconds: Finite,
        /// Positive fall duration in seconds.
        fall_seconds: Finite,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WidthFields {
    kind: alloc::string::String,
    seconds: Option<Finite>,
    fraction: Option<Finite>,
}
impl<'de> Deserialize<'de> for PulseWidth {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use crate::model::table_fields::{only, required};
        use serde::de::Error;
        let v = WidthFields::deserialize(d)?;
        let present = [
            ("seconds", v.seconds.is_some()),
            ("fraction", v.fraction.is_some()),
        ];
        Ok(match v.kind.as_str() {
            "duration" => {
                only::<D::Error>(&present, &["seconds"])?;
                Self::Duration {
                    seconds: required(v.seconds, "seconds")?,
                }
            }
            "duty_cycle" => {
                only::<D::Error>(&present, &["fraction"])?;
                Self::DutyCycle {
                    fraction: required(v.fraction, "fraction")?,
                }
            }
            _ => {
                return Err(D::Error::unknown_variant(
                    &v.kind,
                    &["duration", "duty_cycle"],
                ));
            }
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShapeFields {
    kind: alloc::string::String,
    rise_seconds: Option<Finite>,
    fall_seconds: Option<Finite>,
}
impl<'de> Deserialize<'de> for PulseShape {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use crate::model::table_fields::{only, required};
        use serde::de::Error;
        let v = ShapeFields::deserialize(d)?;
        let present = [
            ("rise_seconds", v.rise_seconds.is_some()),
            ("fall_seconds", v.fall_seconds.is_some()),
        ];
        Ok(match v.kind.as_str() {
            "rectangular" => {
                only::<D::Error>(&present, &[])?;
                Self::Rectangular {}
            }
            "raised_cosine" => Self::RaisedCosine {
                rise_seconds: required(v.rise_seconds, "rise_seconds")?,
                fall_seconds: required(v.fall_seconds, "fall_seconds")?,
            },
            _ => {
                return Err(D::Error::unknown_variant(
                    &v.kind,
                    &["rectangular", "raised_cosine"],
                ));
            }
        })
    }
}
impl PulseSpec {
    pub(crate) fn get(self, p: crate::ParameterId, modulation: bool) -> Option<Finite> {
        use crate::ParameterId as P;
        match (p, modulation) {
            (P::ModulationPulseRate, true) | (P::SourcePulseRate, false) => Some(self.rate_hz),
            (P::ModulationPulseDuration, true) | (P::SourcePulseDuration, false) => {
                match self.width {
                    PulseWidth::Duration { seconds } => Some(seconds),
                    PulseWidth::DutyCycle { .. } => None,
                }
            }
            (P::ModulationDuty, true) | (P::SourceDuty, false) => match self.width {
                PulseWidth::DutyCycle { fraction } => Some(fraction),
                PulseWidth::Duration { .. } => None,
            },
            (P::ModulationRise, true) | (P::SourceRise, false) => match self.shape {
                PulseShape::RaisedCosine { rise_seconds, .. } => Some(rise_seconds),
                PulseShape::Rectangular {} => None,
            },
            (P::ModulationFall, true) | (P::SourceFall, false) => match self.shape {
                PulseShape::RaisedCosine { fall_seconds, .. } => Some(fall_seconds),
                PulseShape::Rectangular {} => None,
            },
            _ => None,
        }
    }
    pub(crate) fn set(&mut self, p: crate::ParameterId, v: Finite, modulation: bool) -> bool {
        use crate::ParameterId as P;
        match (p, modulation) {
            (P::ModulationPulseRate, true) | (P::SourcePulseRate, false) => self.rate_hz = v,
            (P::ModulationPulseDuration, true) | (P::SourcePulseDuration, false) => {
                match &mut self.width {
                    PulseWidth::Duration { seconds } => *seconds = v,
                    PulseWidth::DutyCycle { .. } => return false,
                }
            }
            (P::ModulationDuty, true) | (P::SourceDuty, false) => match &mut self.width {
                PulseWidth::DutyCycle { fraction } => *fraction = v,
                PulseWidth::Duration { .. } => return false,
            },
            (P::ModulationRise, true) | (P::SourceRise, false) => match &mut self.shape {
                PulseShape::RaisedCosine { rise_seconds, .. } => *rise_seconds = v,
                PulseShape::Rectangular {} => return false,
            },
            (P::ModulationFall, true) | (P::SourceFall, false) => match &mut self.shape {
                PulseShape::RaisedCosine { fall_seconds, .. } => *fall_seconds = v,
                PulseShape::Rectangular {} => return false,
            },
            _ => return false,
        }
        true
    }
    /// Requested window duration before frame rounding.
    pub fn requested_seconds(self) -> f64 {
        match self.width {
            PulseWidth::Duration { seconds } => seconds.get(),
            PulseWidth::DutyCycle { fraction } => fraction.get() / self.rate_hz.get(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ParameterId as P;
    use crate::test_support::finite;

    fn shaped() -> PulseSpec {
        PulseSpec {
            rate_hz: finite(4.0),
            width: PulseWidth::DutyCycle {
                fraction: finite(0.25),
            },
            shape: PulseShape::RaisedCosine {
                rise_seconds: finite(0.002),
                fall_seconds: finite(0.001),
            },
        }
    }

    #[test]
    fn duty_and_edges_are_addressed_by_their_owner_only() {
        for (modulation, duty, rise, fall, other) in [
            (
                false,
                P::SourceDuty,
                P::SourceRise,
                P::SourceFall,
                P::ModulationDuty,
            ),
            (
                true,
                P::ModulationDuty,
                P::ModulationRise,
                P::ModulationFall,
                P::SourceDuty,
            ),
        ] {
            let mut spec = shaped();
            assert_eq!(spec.get(duty, modulation), Some(finite(0.25)));
            assert_eq!(spec.get(rise, modulation), Some(finite(0.002)));
            assert_eq!(spec.get(fall, modulation), Some(finite(0.001)));
            assert_eq!(spec.get(other, modulation), None);
            assert!(spec.set(duty, finite(0.5), modulation));
            assert!(spec.set(rise, finite(0.003), modulation));
            assert!(spec.set(fall, finite(0.004), modulation));
            assert!(!spec.set(other, finite(0.5), modulation));
            assert_eq!(
                spec.width,
                PulseWidth::DutyCycle {
                    fraction: finite(0.5)
                }
            );
            assert_eq!(
                spec.shape,
                PulseShape::RaisedCosine {
                    rise_seconds: finite(0.003),
                    fall_seconds: finite(0.004),
                }
            );
        }
    }

    #[test]
    fn duty_width_requests_fraction_of_one_period() {
        assert_eq!(shaped().requested_seconds(), 0.0625);
    }
}
