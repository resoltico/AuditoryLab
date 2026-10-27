//! Finite authored numbers with exact binary64 identity.
use core::{
    fmt,
    hash::{Hash, Hasher},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A finite binary64 value. Equality and hashing preserve signed-zero bits.
/// Construct values with [`Finite::new`]; the representation is private.
/// ```compile_fail
/// let _ = auditorylab_core::Finite(f64::NAN);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Finite(f64);
impl Finite {
    /// Positive zero.
    pub const ZERO: Self = Self(0.0);
    /// Construct a finite value, rejecting NaN and both infinities.
    #[must_use]
    pub const fn new(value: f64) -> Option<Self> {
        if value.is_finite() {
            Some(Self(value))
        } else {
            None
        }
    }
    /// The original binary64 value, unchanged.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}
impl PartialEq for Finite {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}
impl Eq for Finite {}
impl Hash for Finite {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}
impl Serialize for Finite {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.0)
    }
}
impl<'de> Deserialize<'de> for Finite {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| serde::de::Error::custom("expected a finite number"))
    }
}
impl fmt::Display for Finite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    #[test]
    fn rejects_nonfinite_values_and_preserves_finite_bits() {
        for value in [
            f64::NAN,
            f64::from_bits(0x7ff8_0000_0000_0123),
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert!(Finite::new(value).is_none());
        }
        for value in [
            0.0,
            -0.0,
            f64::MAX,
            -f64::MAX,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
        ] {
            let finite = Finite::new(value).unwrap();
            assert_eq!(finite.get().to_bits(), value.to_bits());
        }
        let plus = Finite::new(0.0).unwrap();
        let minus = Finite::new(-0.0).unwrap();
        assert_ne!(plus, minus);
        let hash = |value: Finite| {
            let mut state = DefaultHasher::new();
            value.hash(&mut state);
            state.finish()
        };
        assert_ne!(hash(plus), hash(minus));
    }
    #[test]
    fn serde_rejects_nonfinite_numbers_and_round_trips_signed_zero() {
        #[derive(Deserialize, Serialize)]
        struct Number {
            value: Finite,
        }
        for text in ["value=nan", "value=inf", "value=-inf"] {
            let error = toml::from_str::<Number>(text).err().unwrap();
            assert!(error.message().contains("expected a finite number"));
            assert_eq!(error.span(), Some(6..text.len()));
        }
        for value in [0.0, -0.0, f64::MAX, f64::from_bits(1)] {
            let text = toml::to_string(&Number {
                value: Finite::new(value).unwrap(),
            })
            .unwrap();
            let back = toml::from_str::<Number>(&text).unwrap();
            assert_eq!(back.value.get().to_bits(), value.to_bits());
        }
    }
}
