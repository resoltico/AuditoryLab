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
    use ParameterId as P;
    let rule = match parameter {
        P::ToneFrequency
        | P::ClickRate
        | P::ClickWidth
        | P::AmRate
        | P::GateRate
        | P::GateWidth
        | P::PairFirstFrequency
            if !positive(value) =>
        {
            Some(ValueRule::Positive)
        }
        P::PeakLevel | P::PairFirstLevel | P::PairSecondLevel
            if !value.is_finite() || value > 0.0 =>
        {
            Some(ValueRule::Peak)
        }
        P::AmDepth if !(0.0..=1.0).contains(&value) => Some(ValueRule::Depth),
        _ => None,
    };
    match rule {
        Some(rule) => Err(ValueIssue {
            parameter,
            value,
            rule,
        }),
        None => Ok(()),
    }
}
