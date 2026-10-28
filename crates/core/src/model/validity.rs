//! Rate-independent authored-quantity rules shared by parsing, analysis and compilation.
use crate::ParameterId;
/// The intrinsic rule a quantity must satisfy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueRule {
    /// The value is finite and strictly positive.
    Positive,
    /// A peak is at most zero dBFS.
    Peak,
    /// Modulation depth lies in the closed interval zero through one.
    Depth,
}
/// One typed intrinsic quantity failure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueIssue {
    /// The authored quantity.
    pub parameter: ParameterId,
    /// The exact requested value.
    pub value: f64,
    /// The failed rule.
    pub rule: ValueRule,
}
pub(crate) fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}
pub(crate) fn check_value(parameter: ParameterId, value: f64) -> Result<(), ValueIssue> {
    let Some(rule) = parameter.value_rule() else {
        return Ok(());
    };
    let invalid = match rule {
        ValueRule::Positive => !positive(value),
        ValueRule::Peak => !value.is_finite() || value > 0.0,
        ValueRule::Depth => !(0.0..=1.0).contains(&value),
    };
    if invalid {
        Err(ValueIssue {
            parameter,
            value,
            rule,
        })
    } else {
        Ok(())
    }
}

impl core::fmt::Display for ValueIssue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}
impl core::error::Error for ValueIssue {}

crate::diagnostic_codes! {
    pub(crate) const CODES;
    ValueIssue {
        codes {
            Self {
                rule: ValueRule::Positive,
                ..
            } => "value.positive",
            Self {
                rule: ValueRule::Peak,
                ..
            } => "value.peak",
            Self {
                rule: ValueRule::Depth,
                ..
            } => "value.depth",
        }
        delegates {}
        params(this) {
            crate::diagnostic_params! {
                "parameter"=>this.parameter.key(),
                "value"=>this.value,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intrinsic_failures_expose_stable_codes_and_exact_requested_values() {
        for (parameter, value, code) in [
            (ParameterId::ToneFrequency, 0.0, "value.positive"),
            (ParameterId::PeakLevel, 1.0, "value.peak"),
            (ParameterId::AmDepth, 1.25, "value.depth"),
        ] {
            let failure = check_value(parameter, value).unwrap_err();
            assert_eq!(crate::Code::code(&failure), code);
            assert_eq!(failure.parameter, parameter);
            assert_eq!(failure.value.to_bits(), value.to_bits());
        }
    }
}
