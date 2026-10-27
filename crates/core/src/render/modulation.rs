//! Authored amplitude modulation with finite quantities.
use serde::{Deserialize, Deserializer, Serialize};
/// A periodic change of a voice's amplitude. Every modulation's gain
/// lies within `[0, 1]`, so it never raises the voice's peak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Modulation {
    /// Nominal gain follows `(1 + depth * sin(2π rate_hz t)) / (1 + depth)`,
    /// starting at phase zero. Validated gain is bounded by one; sampled
    /// values need not attain that bound.
    Am {
        /// Modulation frequency in hertz.
        rate_hz: crate::Finite,
        /// Modulation depth from 0 (none) to 1 (full).
        depth: crate::Finite,
    },
    /// Gain 1 during the first `width_seconds` of each `1 / rate_hz` period
    /// and 0 otherwise, the first period starting at time zero: the same
    /// pulse train as a click, gating the carrier on and off.
    Gate {
        /// Gates per second.
        rate_hz: crate::Finite,
        /// Length of each open gate in seconds.
        width_seconds: crate::Finite,
    },
}

impl<'de> Deserialize<'de> for Modulation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Modulation;
            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("amplitude modulation")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Modulation, M::Error> {
                use serde::de::Error;
                let (mut kind, mut rate_hz, mut depth, mut width_seconds) =
                    (None, None, None, None);
                while let Some(key) = map.next_key::<alloc::string::String>()? {
                    match key.as_str() {
                        "kind" => {
                            if kind.is_some() {
                                return Err(M::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value::<alloc::string::String>()?);
                        }
                        "rate_hz" => {
                            if rate_hz.is_some() {
                                return Err(M::Error::duplicate_field("rate_hz"));
                            }
                            rate_hz = Some(map.next_value()?);
                        }
                        "depth" => {
                            if depth.is_some() {
                                return Err(M::Error::duplicate_field("depth"));
                            }
                            depth = Some(map.next_value()?);
                        }
                        "width_seconds" => {
                            if width_seconds.is_some() {
                                return Err(M::Error::duplicate_field("width_seconds"));
                            }
                            width_seconds = Some(map.next_value()?);
                        }
                        _ => {
                            return Err(M::Error::unknown_field(
                                &key,
                                &["kind", "rate_hz", "depth", "width_seconds"],
                            ));
                        }
                    }
                }
                let kind = kind.ok_or_else(|| M::Error::missing_field("kind"))?;
                let rate_hz = rate_hz.ok_or_else(|| M::Error::missing_field("rate_hz"))?;
                match kind.as_str() {
                    "am" => {
                        if width_seconds.is_some() {
                            return Err(M::Error::unknown_field(
                                "width_seconds",
                                &["kind", "rate_hz", "depth"],
                            ));
                        }
                        Ok(Modulation::Am {
                            rate_hz,
                            depth: depth.ok_or_else(|| M::Error::missing_field("depth"))?,
                        })
                    }
                    "gate" => {
                        if depth.is_some() {
                            return Err(M::Error::unknown_field(
                                "depth",
                                &["kind", "rate_hz", "width_seconds"],
                            ));
                        }
                        Ok(Modulation::Gate {
                            rate_hz,
                            width_seconds: width_seconds
                                .ok_or_else(|| M::Error::missing_field("width_seconds"))?,
                        })
                    }
                    _ => Err(M::Error::unknown_variant(&kind, &["am", "gate"])),
                }
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

macro_rules! define_modulation_quantities {
    (parameters {$($parameters:tt)*} fields {$($fields:tt)*} kinds {$($kinds:tt)*}
     modulation_quantities {$( $kind:ident {$($field:ident=>$parameter:ident),*} )*})=>{
        impl Modulation {
            pub(crate) fn get(self,parameter:crate::ParameterId)->Option<crate::Finite> {
                match (parameter,self) {$($( (crate::ParameterId::$parameter,Self::$kind {$field,..})=>Some($field),)*)* _=>None,}
            }
            pub(crate) fn set(&mut self,parameter:crate::ParameterId,value:crate::Finite)->bool {
                let field=match (parameter,self) {$($( (crate::ParameterId::$parameter,Self::$kind {$field,..})=>$field,)*)* _=>return false,};
                *field=value;true
            }
        }
    };
}
crate::component_schema::component_schema!(define_modulation_quantities);
