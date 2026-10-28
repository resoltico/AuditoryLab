//! errors context presentation.
use auditorylab_core::{SampleRate, SoundError};
/// A source that is not valid at a rate.

pub fn sound_error_at_rate(value: &SoundError, title: &str, rate: SampleRate) -> String {
    let error = value;
    format!(
        "{title} at {} Hz: {}",
        rate.hz(),
        crate::errors::acoustic::sound_error(error)
    )
}
