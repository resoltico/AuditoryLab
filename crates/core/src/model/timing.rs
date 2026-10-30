//! Finite playback and export timing; live targets compile independently.
use crate::{Finite, SampleRate, Signal, SoundError, Target};
use serde::{Deserialize, Serialize};
/// Overall finite duration and endpoint fade, distinct from pulse shaping and live transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteTiming {
    /// Requested finite duration in seconds.
    pub duration_seconds: Finite,
    /// Requested raised-cosine fade at each finite edge.
    pub fade_seconds: Finite,
}
impl FiniteTiming {
    /// Add this finite timing to an already compiled live target.
    pub fn signal(self, target: &Target) -> Result<Signal, SoundError> {
        target.signal(self.duration_seconds.get(), self.fade_seconds.get())
    }
    /// Validate only finite timing at the selected sample rate.
    pub fn check(self, rate: SampleRate) -> Result<(), SoundError> {
        Signal::check_envelope(rate, self.duration_seconds.get(), self.fade_seconds.get())
    }
}
