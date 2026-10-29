//! Numeric input text in parameter display units.
use crate::core::ParameterId;
/// Emit editable text in the parameter's displayed unit.
/// Tries one through seventeen significant digits for exact model-bit recovery.
/// Some finite model values have no binary64 inverse in that unit; those use
/// the ordinary display value, which can overflow. Unedited text must not be
/// reparsed as a new model value.
#[must_use]
pub fn parameter_text(parameter: ParameterId, model: f64) -> String {
    let id = parameter;
    let unit = id.descriptor().unit;
    let display = unit.to_display(model);
    if !display.is_finite() {
        return display.to_string();
    }
    for digits in 1..=17 {
        let text = format!("{display:.*e}", digits - 1);
        if let Ok(value) = text.parse::<f64>()
            && unit.to_model(value).to_bits() == model.to_bits()
        {
            return value.to_string();
        }
    }
    display.to_string()
}

/// Why staged text is not a usable value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputProblem {
    /// The text is not a number.
    Unparseable,
    /// The value is not a finite number, whether typed or inherited.
    Nonfinite,
}

/// Parse one explicitly edited number in the semantic parameter's display unit.
pub fn parse_parameter(
    parameter: ParameterId,
    text: &str,
) -> Result<crate::core::Finite, InputProblem> {
    let display = text
        .trim()
        .parse::<f64>()
        .map_err(|_| InputProblem::Unparseable)?;
    crate::core::Finite::new(parameter.descriptor().unit.to_model(display))
        .ok_or(InputProblem::Nonfinite)
}
impl std::fmt::Display for InputProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for InputProblem {}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    InputProblem {
        codes {
            Self::Unparseable => "input.unparseable",
            Self::Nonfinite => "input.nonfinite",
        }
        delegates {}
    }
}
