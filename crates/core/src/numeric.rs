// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Conversions between frame counts, binary64 and decibels, each stated once.

/// Convert a frame count to binary64, exact below 2^53 frames.
#[expect(
    clippy::cast_precision_loss,
    reason = "quality:core-numeric-frames-to-f64"
)]
pub(crate) const fn frames_to_f64(frames: u64) -> f64 {
    frames as f64
}

/// Convert a non-negative whole binary64 value within 0..=2^36 to a frame count.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "quality:core-numeric-whole-to-frames"
)]
pub(crate) const fn whole_to_frames(value: f64) -> u64 {
    value as u64
}

/// The linear amplitude of a level in dBFS: 0 dBFS is full scale.
pub(crate) fn amplitude(peak_dbfs: f64) -> f64 {
    libm::pow(10.0, peak_dbfs / 20.0)
}

/// The level in dBFS of a linear amplitude; silence is negative infinity.
pub(crate) fn dbfs(amplitude: f64) -> f64 {
    20.0 * libm::log10(amplitude)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decibels_and_amplitudes_convert_both_ways() {
        assert_eq!(amplitude(0.0), 1.0);
        assert!((amplitude(-20.0) - 0.1).abs() < 1e-17);
        assert!((amplitude(-6.020_599_913_279_624) - 0.5).abs() < 1e-15);
        assert_eq!(dbfs(1.0), 0.0);
        assert!((dbfs(0.1) + 20.0).abs() < 1e-12);
        assert_eq!(dbfs(0.0), f64::NEG_INFINITY);
    }
}
