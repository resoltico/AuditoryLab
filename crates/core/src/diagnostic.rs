//! Data-free diagnostic identifiers and separately serialized values.
use alloc::{
    boxed::Box,
    collections::BTreeMap,
    string::{String, ToString},
};

/// One named diagnostic value. Nonfinite quantities use their explicit text
/// because JSON has no numeric representation for them.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(untagged)]
pub enum ParamValue {
    /// Exact nonnegative integer.
    Unsigned(u128),
    /// Exact signed integer.
    Signed(i128),
    /// Finite floating-point quantity.
    Number(crate::Finite),
    /// Captured text, identifier or explicit nonfinite quantity.
    Text(String),
    /// A captured condition.
    Bool(bool),
}
/// Named values attached to a fixed diagnostic code.
pub type Params = BTreeMap<&'static str, ParamValue>;
/// A typed error or warning's machine identifier, independent of its data.
pub trait Code {
    /// Fixed `<owner>.<rule>` identifier; never includes captured values.
    fn code(&self) -> &'static str;
    /// Captured values, kept separate from the identifier and human wording.
    fn params(&self) -> Params {
        Params::new()
    }
}
impl<T: Code + ?Sized> Code for Box<T> {
    fn code(&self) -> &'static str {
        (**self).code()
    }
    fn params(&self) -> Params {
        (**self).params()
    }
}
macro_rules! unsigned {($($ty:ty),*)=>{$(impl From<$ty> for ParamValue {fn from(value:$ty)->Self {
    Self::Unsigned(u128::from(value))}})*};}
macro_rules! signed {($($ty:ty),*)=>{$(impl From<$ty> for ParamValue {fn from(value:$ty)->Self {
    Self::Signed(i128::from(value))}})*};}
unsigned! {
    u8,
    u16,
    u32,
    u64
}
signed! {
    i8,
    i16,
    i32,
    i64
}
impl From<f64> for ParamValue {
    fn from(value: f64) -> Self {
        match crate::Finite::new(value) {
            Some(finite) => Self::Number(finite),
            None => Self::Text(value.to_string()),
        }
    }
}

impl From<bool> for ParamValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}
impl From<String> for ParamValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}
impl From<&str> for ParamValue {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}

/// Declare an owner's codes once, generating both its enumerable registry and
/// its implementations. Delegating wrappers inherit cause codes and values;
/// additional parameters retain the wrapper's captured context.
#[doc(hidden)]
#[macro_export]
macro_rules! diagnostic_codes {
    ($(#[$registry_attr:meta])* $visibility:vis const $registry:ident; $( $ty:ty {
        codes {$( $pattern:pat => $code:literal, )*}
        delegates {$( $delegate:pat => $cause:expr, )*}
        $(params($this:ident) $extra:block)?
    })*)=>{
        $(#[$registry_attr])*
        $visibility const $registry:&[&str]=&[$($($code,)*)*];
        $(impl $crate::diagnostic::Code for $ty {
            fn code(&self)->&'static str {
                match self {$( $pattern=>$code,
                    )* $( $delegate=>$crate::diagnostic::Code::code($cause), )*}
            }
            fn params(&self)->$crate::diagnostic::Params {
                let params=match self {
                    $( $pattern=>$crate::diagnostic::Params::new(), )*
                    $( $delegate=>$crate::diagnostic::Code::params($cause), )*
                };
                $(let $this=self; let params={let mut combined=params;combined.extend($extra);
                    combined};)?
                params
            }
        })*
    };
}
/// Build a small parameter map without coupling owners to a JSON library.
#[doc(hidden)]
#[macro_export]
macro_rules! diagnostic_params {
    ($($key:literal => $value:expr),* $(,)?)=>{
        $crate::diagnostic::Params::from([$(($key,$crate::diagnostic::ParamValue::from($value))),*])
    };
}

/// Enumerable codes grouped by their authoritative core owner.
pub const OWNERS: &[&[&str]] = &[
    crate::authored::CODES,
    crate::contract::CODES,
    crate::rate::CODES,
    crate::recipe::CODES,
    crate::validity::CODES,
    crate::rhythm::CODES,
    crate::voice::CODES,
    crate::vocabulary::CODES,
];

impl From<usize> for ParamValue {
    fn from(value: usize) -> Self {
        Self::Unsigned(value as u128)
    }
}
impl From<isize> for ParamValue {
    fn from(value: isize) -> Self {
        Self::Signed(value as i128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn values_stay_out_of_codes_and_survive_delegating_wrappers() {
        let rate = crate::SampleRateError { hz: 768_001 };
        assert_eq!(rate.code(), "sample_rate.range");
        assert_eq!(rate.params()["hz"], ParamValue::Unsigned(768_001));
        let structure = crate::StructureError::Identity {
            id: crate::ComponentId::new(17).unwrap(),
        };
        let wrapped = crate::RecipeError::Structure(structure);
        assert_eq!(wrapped.code(), "structure.identity");
        assert_eq!(wrapped.params()["component_id"], ParamValue::Unsigned(17));
        assert_eq!(wrapped.params(), structure.params());
        let version = crate::RecipeError::UnsupportedVersion { found: 3 };
        assert_eq!(version.code(), "recipe.schema_version");
        assert_eq!(version.params()["found"], ParamValue::Unsigned(3));
        assert_eq!(version.params()["supported"], ParamValue::Unsigned(4));
        assert_eq!(
            ParamValue::from(f64::INFINITY),
            ParamValue::Text("inf".into())
        );
        assert_eq!(
            ParamValue::from(f64::NEG_INFINITY),
            ParamValue::Text("-inf".into())
        );
        assert_eq!(ParamValue::from(f64::NAN), ParamValue::Text("NaN".into()));
    }
}
