// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::{Signal, conversion};

/// Frames per second, from [`SampleRate::MIN_HZ`] to [`SampleRate::MAX_HZ`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SampleRate(u32);

/// A sample rate outside [`SampleRate::MIN_HZ`] through [`SampleRate::MAX_HZ`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleRateError {
    /// The rejected rate in frames per second.
    pub hz: u32,
}

impl core::fmt::Display for SampleRateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(crate::Code::code(self))
    }
}

impl core::error::Error for SampleRateError {}

impl SampleRate {
    /// The lowest sample rate, in frames per second.
    pub const MIN_HZ: u32 = 1;

    /// The highest sample rate, in frames per second.
    pub const MAX_HZ: u32 = 768_000;

    /// The rate to render at when the destination imposes none.
    pub const DEFAULT: Self = Self(48_000);

    /// Validate a sample rate in frames per second.
    ///
    /// # Errors
    /// Returns [`SampleRateError`] outside [`SampleRate::MIN_HZ`] through
    /// [`SampleRate::MAX_HZ`].
    pub const fn new(hz: u32) -> Result<Self, SampleRateError> {
        if hz < Self::MIN_HZ || hz > Self::MAX_HZ {
            return Err(SampleRateError { hz });
        }
        Ok(Self(hz))
    }

    /// Return the number of frames per second.
    #[must_use]
    pub const fn hz(self) -> u32 {
        self.0
    }

    pub(crate) fn as_f64(self) -> f64 {
        f64::from(self.0)
    }

    pub(crate) fn nyquist_hz(self) -> f64 {
        self.as_f64() / 2.0
    }

    /// The whole number of frames nearest to `seconds`, halfway cases away
    /// from zero; `None` unless `seconds` is finite, non-negative and spans
    /// at most [`Signal::MAX_FRAMES`].
    pub(crate) fn frames_in(self, seconds: f64) -> Option<u64> {
        let frames = libm::round(seconds * self.as_f64());
        // NaN fails both comparisons, so the accepted range is stated positively.
        (seconds >= 0.0 && frames <= conversion::frames_to_f64(Signal::MAX_FRAMES))
            .then(|| conversion::whole_to_frames(frames))
    }

    /// The duration of `frames` in seconds, for presentation; division can round.
    pub fn seconds_in(self, frames: u64) -> f64 {
        conversion::frames_to_f64(frames) / self.as_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(hz: u32) -> SampleRate {
        SampleRate::new(hz).unwrap()
    }

    #[test]
    fn sample_rates_have_inclusive_bounds_and_a_default() {
        for hz in [0, 768_001, u32::MAX] {
            assert_eq!(SampleRate::new(hz), Err(SampleRateError { hz }));
        }
        assert_eq!(rate(1).hz(), 1);
        assert_eq!(rate(768_000).hz(), 768_000);
        assert_eq!(SampleRate::DEFAULT, rate(48_000));
    }

    #[test]
    fn time_spans_resolve_to_the_nearest_frame() {
        let cd = rate(44_100);
        // 0.1 * 44100 is 4410.000000000001 in binary64; nearest-frame rounding absorbs it.
        assert_eq!(cd.frames_in(0.1), Some(4_410));
        assert_eq!(cd.frames_in(0.0), Some(0));
        assert_eq!(cd.frames_in(-0.0), Some(0));
        assert_eq!(cd.frames_in(1.0 / 88_200.0), Some(1));
        assert_eq!(cd.frames_in(0.49 / 44_100.0), Some(0));
        assert_eq!(rate(48_000).frames_in(307.2), Some(14_745_600));
        for seconds in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1e-9, 1e300] {
            assert_eq!(rate(48_000).frames_in(seconds), None, "{seconds}");
        }
        let unit = rate(1);
        assert_eq!(unit.frames_in(68_719_476_736.0), Some(Signal::MAX_FRAMES));
        assert_eq!(unit.frames_in(68_719_476_736.5), None);
        assert_eq!(rate(48_000).seconds_in(72_000), 1.5);
    }
}

crate::diagnostic_codes! {
    pub(crate) const CODES;
    SampleRateError {
        codes {Self {..}=>"sample_rate.range",}
        delegates {}
        params(this) {crate::diagnostic_params! {"hz"=>this.hz,"min_hz"=>SampleRate::MIN_HZ,"max_hz"=>SampleRate::MAX_HZ}}
    }
}
