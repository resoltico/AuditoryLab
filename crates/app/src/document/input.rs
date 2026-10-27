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
