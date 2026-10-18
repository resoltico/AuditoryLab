//! Integer phase and frame-quantized periodic generators.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "bounded bit fields and exact rational oracles"
)]
use core::f64::consts::TAU;

/// One cycle is 2^64 phase units.
#[cfg(test)]
const UNITS_PER_CYCLE: f64 = 18_446_744_073_709_551_616.0;

/// `ceil(hz / rate * 2^64)`, exactly, for finite `0 <= hz < rate / 2`.
/// Rounding up keeps every computed phase at or just after the exact one, so
/// a frame that falls exactly on a period boundary is assigned to the new period.
/// Returns `None` when the frequency is outside that range.
pub(crate) fn step(hz: f64, rate: u32) -> Option<u64> {
    if !(hz >= 0.0 && hz < f64::from(rate) / 2.0) {
        return None;
    }
    if hz == 0.0 {
        return Some(0);
    }
    // hz = mantissa * 2^exponent exactly.
    let bits = hz.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1 << 52) - 1);
    let (mantissa, exponent) = if raw_exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), raw_exponent - 1075)
    };
    let shift = exponent + 64;
    let rate = u128::from(rate);
    let quotient = if shift >= 0 {
        // mantissa < 2^53 and hz < 2^19 bound shift so the product fits.
        let numerator = u128::from(mantissa) << shift;
        numerator.div_ceil(rate)
    } else if -shift < 100 {
        let denominator = rate << -shift;
        u128::from(mantissa).div_ceil(denominator)
    } else {
        1
    };
    u64::try_from(quotient).ok()
}

/// The fraction of a cycle held by `phase`, in `[-0.5, 0.5)`.
#[inline]
fn signed_cycles(phase: u64) -> f64 {
    // The top 53 bits as a signed fraction: exact conversion.
    ((phase as i64) >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
}

#[inline]
fn sine(phase: u64) -> f64 {
    libm::sin(TAU * signed_cycles(phase))
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Carrier {
    Tone { step: u64 },
    Pulse { step: u64, open: u64 },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Modulator {
    Sinusoidal { step: u64, depth: f64 },
    Rectangular { step: u64, open: u64 },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Voice {
    pub(crate) left: bool,
    pub(crate) right: bool,
    pub(crate) amplitude: f64,
    pub(crate) carrier: Carrier,
    pub(crate) modulation: Option<Modulator>,
}

impl Voice {
    #[inline]
    pub(crate) fn sample(&self, n: u64) -> f64 {
        let carrier = match self.carrier {
            Carrier::Tone { step } => sine(n.wrapping_mul(step)),
            Carrier::Pulse { step, open } => {
                if n.wrapping_mul(step) < open {
                    1.0
                } else {
                    0.0
                }
            }
        };
        let gain = match self.modulation {
            None => 1.0,
            Some(Modulator::Sinusoidal { step, depth }) => {
                (1.0 + depth * sine(n.wrapping_mul(step))) / (1.0 + depth)
            }
            Some(Modulator::Rectangular { step, open }) => {
                if n.wrapping_mul(step) < open {
                    1.0
                } else {
                    0.0
                }
            }
        };
        self.amplitude * (carrier * gain)
    }
}

/// Whether `x * k <= fs` holds exactly, for finite non-negative `x`.
pub(crate) fn times_at_most(x: f64, k: u64, fs: u32) -> bool {
    if !(x >= 0.0 && x.is_finite()) {
        return false;
    }
    if x == 0.0 {
        return true;
    }
    let bits = x.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1 << 52) - 1);
    let (mantissa, exponent) = if raw_exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), raw_exponent - 1075)
    };
    // mantissa * k < 2^73; compare mantissa * k * 2^exponent with fs exactly.
    let product = u128::from(mantissa) * u128::from(k);
    let fs = u128::from(fs);
    if exponent >= 0 {
        // x is then at least 2^52, above every sample rate.
        false
    } else if -exponent >= 108 {
        true
    } else {
        product <= fs << -exponent
    }
}

/// A pulse of `round(width * fs)` frames, halves away from zero, starting on the
/// first frame at or after each period's onset: open while `phase < frames * step`.
/// Returns the open threshold, or `None` unless the pulse lasts at least one frame
/// and leaves at least one closed frame in every period: `(frames + 1) * rate <= fs`.
pub(crate) fn open(width_seconds: f64, rate_hz: f64, step: u64, fs: u32) -> Option<u64> {
    let frames = libm::round(width_seconds * f64::from(fs));
    if !(frames >= 1.0 && frames <= f64::from(fs)) {
        return None;
    }
    let frames = frames as u64;
    if !times_at_most(rate_hz, frames + 1, fs) {
        return None;
    }
    frames.checked_mul(step)
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Exact reference: round(p/q * 2^64) for rational hz = p/q.
    fn reference_step(p: u128, q: u128, rate: u128) -> u64 {
        let denominator = q * rate;
        u64::try_from((p << 64).div_ceil(denominator)).unwrap()
    }

    #[test]
    fn steps_are_exactly_rounded_for_representable_frequencies() {
        for rate in [1_u32, 8_000, 44_100, 48_000, 96_000, 768_000] {
            for (p, q) in [
                (807_u128, 2_u128),
                (625, 16),
                (10_000, 1),
                (1, 1),
                (39, 1),
                (1, 1024),
            ] {
                let hz = p as f64 / q as f64;
                if hz >= f64::from(rate) / 2.0 {
                    assert_eq!(step(hz, rate), None);
                    continue;
                }
                assert_eq!(
                    step(hz, rate),
                    Some(reference_step(p, q, u128::from(rate))),
                    "{hz} at {rate}"
                );
            }
        }
        // Very low frequencies take the right-shift branch.
        for (p, q) in [
            (1_u128, 1_u128 << 20),
            (1, 1 << 40),
            (3, 1 << 50),
            (1, 1 << 70),
        ] {
            let hz = p as f64 / q as f64;
            let expected = u64::try_from((p << 64).div_ceil(q * 48_000)).unwrap();
            assert_eq!(step(hz, 48_000), Some(expected), "{hz}");
        }
        assert_eq!(step(f64::NAN, 48_000), None);
        assert_eq!(step(-1.0, 48_000), None);
        assert_eq!(step(24_000.0, 48_000), None);
        assert_eq!(
            step(24_000.0_f64.next_down(), 48_000).map(|s| s < 1 << 63),
            Some(true)
        );
        // The smallest positive frequency still advances.
        assert_eq!(step(f64::from_bits(1), 48_000), Some(1));
    }

    #[test]
    fn phase_at_any_frame_matches_the_exact_rational_phase_within_a_constant() {
        // Phase error of n*step versus exact n*f/fs: at most n * 2^-65 cycles
        // from rounding step, independent of accumulation order.
        let rate = 48_000_u32;
        for (p, q) in [(807_u128, 2_u128), (625, 16), (10_000, 1)] {
            let s = step(p as f64 / q as f64, rate).unwrap();
            for n in [
                0_u64,
                1,
                47_999,
                48_000,
                172_800_000,
                (1 << 36) - 1,
                1 << 36,
            ] {
                let exact = ((u128::from(n) * p) << 64) / (q * u128::from(rate)) % (1 << 64);
                let actual = u128::from(n.wrapping_mul(s));
                let diff = exact
                    .abs_diff(actual)
                    .min((1 << 64) - exact.abs_diff(actual));
                // Below n units: step is rounded up by less than one unit.
                assert!(diff <= u128::from(n) + 1, "{p}/{q} n={n}: {diff}");
                // In cycles: below 2^-28 even at 2^36 frames.
                assert!((diff as f64) / UNITS_PER_CYCLE < 2f64.powi(-28));
            }
        }
    }
}

#[cfg(test)]
mod pulses {
    use super::*;

    /// Independent integer reference: pulse k starts at ceil(k * fs / rate)
    /// for an integer rate and lasts `width_frames`.
    fn reference_on(n: u64, rate: u64, fs: u64, width_frames: u64) -> bool {
        let k = n * rate / fs; // the period containing n (floor)
        let onset = (k * fs).div_ceil(rate);
        let (k, onset) = if onset > n {
            (k - 1, ((k - 1) * fs).div_ceil(rate))
        } else {
            (k, onset)
        };
        let _ = k;
        n - onset < width_frames
    }

    #[test]
    fn every_pulse_has_the_same_width_and_starts_on_the_first_frame_of_its_period() {
        for fs in [8_000_u32, 44_100, 48_000, 96_000, 768_000] {
            for rate in [8_u32, 40, 80, 1000] {
                let s = step(f64::from(rate), fs).unwrap();
                let Some(o) = open(0.001, f64::from(rate), s, fs) else {
                    continue;
                };
                let w = (0.001 * f64::from(fs)).round() as u64;
                let late = (1_u64 << 36) - 3 * u64::from(fs);
                for n in (0..3 * u64::from(fs)).chain(late..late + 3 * u64::from(fs)) {
                    let actual = n.wrapping_mul(s) < o;
                    assert_eq!(
                        actual,
                        reference_on(n, u64::from(rate), u64::from(fs), w),
                        "fs={fs} rate={rate} n={n}"
                    );
                }
            }
        }
    }

    #[test]
    fn an_exact_clock_has_a_half_open_edge_at_every_period() {
        // 1000 Hz at 8 kHz steps exactly 1/8 cycle; 3 frames open, 5 closed.
        let s = step(1_000.0, 8_000).unwrap();
        assert_eq!(s, 1 << 61);
        let o = open(3.0 / 8_000.0, 1_000.0, s, 8_000).unwrap();
        let tone = Voice {
            left: true,
            right: true,
            amplitude: 1.0,
            carrier: Carrier::Pulse { step: s, open: o },
            modulation: None,
        };
        let ungated = Voice {
            carrier: Carrier::Tone {
                step: step(500.0, 8_000).unwrap(),
            },
            ..tone
        };
        let gated = Voice {
            modulation: Some(Modulator::Rectangular { step: s, open: o }),
            ..ungated
        };
        for n in (0..64).chain((1_u64 << 36) - 64..1 << 36) {
            let on = n % 8 < 3;
            assert_eq!(tone.sample(n), if on { 1.0 } else { 0.0 }, "frame {n}");
            if on {
                assert_eq!(
                    gated.sample(n).to_bits(),
                    ungated.sample(n).to_bits(),
                    "frame {n}"
                );
            } else {
                assert_eq!(gated.sample(n), 0.0, "frame {n}");
            }
        }
    }

    #[test]
    fn invalid_widths_are_rejected() {
        let s = step(40.0, 48_000).unwrap();
        assert!(open(0.0, 40.0, s, 48_000).is_none());
        assert!(open(0.4 / 48_000.0, 40.0, s, 48_000).is_none());
        assert!(open(0.6 / 48_000.0, 40.0, s, 48_000).is_some());
        // 1199 frames leave one closed frame in a 1200-frame period; 1200 do not.
        assert!(open(1199.0 / 48_000.0, 40.0, s, 48_000).is_some());
        assert!(open(1200.0 / 48_000.0, 40.0, s, 48_000).is_none());
    }
}

#[cfg(test)]
mod validation_and_trace {
    use super::*;

    #[test]
    fn closed_frame_rule_is_exact_at_its_boundary() {
        let s = step(40.0, 48_000).unwrap();
        // 1199 open frames leave one closed frame in a 1200-frame period.
        assert!(open(1199.0 / 48_000.0, 40.0, s, 48_000).is_some());
        assert!(open(1200.0 / 48_000.0, 40.0, s, 48_000).is_none());
        assert!(times_at_most(40.0, 1200, 48_000));
        assert!(!times_at_most(40.0_f64.next_up(), 1200, 48_000));
        assert!(!times_at_most(40.0, 1201, 48_000));
        assert!(times_at_most(39.0625, 1228, 48_000)); // 47968.75
        assert!(!times_at_most(39.0625, 1229, 48_000)); // 48007.8125
        assert!(times_at_most(f64::from_bits(1), u64::MAX, 1));
        assert!(!times_at_most(f64::NAN, 1, 1));
        assert!(!times_at_most(-40.0, 1, 48_000));
        assert!(!times_at_most(f64::INFINITY, 1, 48_000));
        assert!(times_at_most(0.0, u64::MAX, 1));
        // A fractional 1228.8-frame period: 1227 open frames always leave a
        // closed one; 1228 would not in the 1228-frame periods.
        let s39 = step(39.0625, 48_000).unwrap();
        assert!(open(1227.0 / 48_000.0, 39.0625, s39, 48_000).is_some());
        assert!(open(1228.0 / 48_000.0, 39.0625, s39, 48_000).is_none());
        assert!(!times_at_most(1e300, 1, 768_000));
        // The last closed frame really is closed, and the next period opens.
        let o = open(1199.0 / 48_000.0, 40.0, s, 48_000).unwrap();
        for k in [0_u64, 1, 1000, 57_000_000] {
            let base = k * 1200;
            assert!(base.wrapping_mul(s) < o);
            assert!((base + 1198).wrapping_mul(s) < o);
            assert!((base + 1199).wrapping_mul(s) >= o);
        }
    }
}
