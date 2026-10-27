//! A clock observation that cannot be represented by portable history.
/// The exact observed system clock was outside the supported timestamp profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockError {
    /// Exact observation used for the attempted action.
    pub observed: std::time::SystemTime,
}
