//! Seekable tones and exact frame-quantized pulse trains.
use core::f64::consts::TAU;
#[cfg(test)]
const UNITS_PER_CYCLE: f64 = 18_446_744_073_709_551_616.0;

/// Exact non-negative binary64 decomposition: mantissa × 2^exponent.
fn parts(value: f64) -> (u64, i32) {
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1 << 52) - 1);
    if exponent == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), exponent - 1075)
    }
}
/// Exactly ceil(hz × 2^64 / fs), for finite 0 < hz < fs/2.
pub(crate) fn step(hz: f64, fs: u32) -> Option<u64> {
    if !(hz > 0.0 && hz < f64::from(fs) / 2.0) {
        return None;
    }
    let (mantissa, exponent) = parts(hz);
    let shift = exponent + 64;
    let fs = u128::from(fs);
    let quotient = if shift >= 0 {
        (u128::from(mantissa) << shift).div_ceil(fs)
    } else if -shift < 100 {
        u128::from(mantissa).div_ceil(fs << -shift)
    } else {
        1
    };
    u64::try_from(quotient).ok()
}
#[inline]
#[expect(clippy::cast_precision_loss, reason = "Arithmetic shift retains a signed 53-bit phase before binary64 conversion; the intentionally reduced phase bounds sine input")]
pub(crate) fn sine(phase: u64) -> f64 {
    let cycles = ((phase as i64) >> 11) as f64 / 9_007_199_254_740_992.0;
    libm::sin(TAU * cycles)
}
/// Whether x × k ≤ fs, exactly, for non-negative finite x, positive k and a valid sample rate.
#[expect(
    clippy::cast_sign_loss,
    reason = "Binary64 decomposition and the early exponent guards make the negated exponent nonnegative before converting to a shift"
)]
pub(crate) fn times_at_most(x: f64, k: u64, fs: u32) -> bool {
    if !(x >= 0.0 && x.is_finite()) {
        return false;
    }
    if x == 0.0 {
        return true;
    }
    let (mantissa, exponent) = parts(x);
    if exponent >= 0 {
        return false;
    } // x ≥ 2^52, above all sample rates.
    let product = u128::from(mantissa) * u128::from(k);
    let shift = (-exponent) as u32;
    let whole = product.checked_shr(shift).unwrap_or(0);
    let fs = u128::from(fs);
    if whole != fs {
        whole < fs
    } else {
        // Equality of the nonzero whole parts proves this shift fits u128.
        product == fs << shift
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pulse {
    mantissa: u64,
    shift: u32,
    fs: u32,
    frames: u64,
}
impl Pulse {
    pub(crate) const fn frames(self) -> u64 {
        self.frames
    }
    /// floor(n × rate/fs), with n below Signal::MAX_FRAMES.
    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The validated n < 2^36 and rate < 2^19 bound the shifted pulse count below 2^55"
    )]
    fn periods(self, n: u64) -> u64 {
        // floor(floor(n*m/2^shift)/fs) = floor(n*m/(2^shift*fs)).
        // n < 2^36 and rate < 2^19 keep the shifted value below 2^55.
        let whole = (u128::from(n) * u128::from(self.mantissa))
            .checked_shr(self.shift)
            .unwrap_or(0) as u64;
        whole / u64::from(self.fs)
    }
    /// Extrema over every discrete frame of [start,end), without enumerating pulses.
    pub(crate) fn extent(self, start: u64, end: u64) -> crate::Extent {
        let first = self.sample(start);
        let last = self.sample(end - 1);
        let crosses_onset = self.periods(end - 1) > self.periods(start);
        crate::Extent {
            min: if first == 0.0 || last == 0.0 || crosses_onset {
                0.0
            } else {
                1.0
            },
            max: if first == 1.0 || last == 1.0 || crosses_onset {
                1.0
            } else {
                0.0
            },
        }
    }
    #[inline]
    pub(crate) fn sample(self, n: u64) -> f64 {
        // A pulse starts at ceil(k*fs/rate). For n ≥ W it is open exactly
        // when that onset lies after n-W: K(n) > K(n-W), K(t)=floor(t*rate/fs).
        if n < self.frames || self.periods(n) > self.periods(n - self.frames) {
            1.0
        } else {
            0.0
        }
    }
}
/// Resolve pulse width and the exact closed-frame rule for a validated rate.
/// A width beyond [`Signal::MAX_FRAMES`](crate::Signal::MAX_FRAMES) frames
/// would leave every realizable signal inside its first pulse, so it is rejected.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Width is checked whole and in 1..=2^36; admitted sub-Nyquist rate has a negative decomposition exponent"
)]
pub(crate) fn pulse(width_seconds: f64, rate_hz: f64, fs: u32) -> Option<Pulse> {
    let frames = libm::round(width_seconds * f64::from(fs));
    if !(frames >= 1.0 && frames <= crate::numeric::frames_to_f64(crate::Signal::MAX_FRAMES)) {
        return None;
    }
    let frames = frames as u64;
    if !times_at_most(rate_hz, frames + 1, fs) {
        return None;
    }
    let (mantissa, exponent) = parts(rate_hz);
    Some(Pulse {
        mantissa,
        shift: (-exponent) as u32,
        fs,
        frames,
    })
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Carrier {
    Tone { step: u64 },
    Pulse(Pulse),
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Modulator {
    Sinusoidal { step: u64, depth: f64 },
    Rectangular(Pulse),
}
#[derive(Debug, Clone, Copy, PartialEq)]
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
            Carrier::Pulse(pulse) => pulse.sample(n),
        };
        let gain = match self.modulation {
            None => 1.0,
            Some(Modulator::Sinusoidal { step, depth }) => {
                (1.0 + depth * sine(n.wrapping_mul(step))) / (1.0 + depth)
            }
            Some(Modulator::Rectangular(pulse)) => pulse.sample(n),
        };
        self.amplitude * (carrier * gain)
    }
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
    fn pulse_extrema_match_every_sample_across_interval_boundaries() {
        for (hz, width, fs) in [
            (8.0, 0.001, 44100),
            (39.0625, 0.001, 48000),
            (16000.0, 1.0 / 48000.0, 48000),
        ] {
            let p = pulse(width, hz, fs).unwrap();
            for start in [0, 1, 42, 43, 44, 45, 1198, 1199, 1200, 1201, 6000] {
                for span in [1, 2, 3, 43, 44, 45, 1199, 1200, 1201, 1300] {
                    let actual = p.extent(start, start + span);
                    let mut min = 1.0_f64;
                    let mut max = 0.0_f64;
                    for n in start..start + span {
                        let x = p.sample(n);
                        min = min.min(x);
                        max = max.max(x);
                    }
                    assert_eq!(actual, crate::Extent { min, max }, "{hz} {start} {span}");
                }
            }
        }
    }
    #[test]
    #[expect(
        clippy::cast_precision_loss,
        reason = "Independent rational test oracles intentionally convert integer ratios and rounded millisecond widths; fixture widths and indices are nonnegative and fit u64"
    )]
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
        assert_eq!(step(0.0, 48_000), None);
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
    #[expect(
        clippy::cast_precision_loss,
        reason = "Independent rational test oracles intentionally convert integer ratios and rounded millisecond widths; fixture widths and indices are nonnegative and fit u64"
    )]
    fn phase_at_any_frame_matches_the_exact_rational_phase_within_a_constant() {
        // Phase error of n*step versus exact n*f/fs: less than n * 2^-64 cycles
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
    fn reference(n: u64, p: u128, q: u128, fs: u32, width: u64) -> bool {
        let k = u128::from(n) * p / (q * u128::from(fs));
        let onset = (k * q * u128::from(fs)).div_ceil(p);
        u128::from(n) - onset < u128::from(width)
    }
    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Independent rational test oracles intentionally convert integer ratios and rounded millisecond widths; fixture widths and indices are nonnegative and fit u64"
    )]
    fn pulses_match_independent_integer_onsets_at_early_and_late_frames() {
        for fs in [8000, 44100, 48000, 96000, 768000] {
            for rate in [8_u32, 40, 80, 1000] {
                let Some(pulse) = pulse(0.001, f64::from(rate), fs) else {
                    continue;
                };
                let width = (0.001 * f64::from(fs)).round() as u64;
                let late = (1 << 36) - 3 * u64::from(fs);
                for n in (0..3 * u64::from(fs)).chain(late..1 << 36) {
                    assert_eq!(
                        pulse.sample(n) == 1.0,
                        reference(n, u128::from(rate), 1, fs, width),
                        "{fs} {rate} {n}"
                    );
                }
            }
        }
    }
    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "Independent rational test oracles intentionally convert integer ratios and rounded millisecond widths; fixture widths and indices are nonnegative and fit u64"
    )]
    fn fractional_and_slow_rates_do_not_open_before_the_exact_onset() {
        for (p, q) in [
            (9007199254740991_u128, 1_u128 << 53),
            (7205759403792793, 1 << 56),
            (1, 1 << 20),
            (625, 16),
        ] {
            for fs in [8000, 44100, 48000, 96000, 768000] {
                let rate = p as f64 / q as f64;
                let clock = pulse(0.001, rate, fs).unwrap();
                let width = (0.001 * f64::from(fs)).round() as u64;
                for k in [0_u128, 1, 2, 100, 1000] {
                    let onset = (k * q * u128::from(fs)).div_ceil(p);
                    if onset + u128::from(width) + 2 >= 1 << 36 {
                        continue;
                    }
                    for n in onset.saturating_sub(2)..onset + u128::from(width) + 2 {
                        assert_eq!(
                            clock.sample(n as u64) == 1.0,
                            reference(n as u64, p, q, fs, width),
                            "{p}/{q} {fs} {n}"
                        );
                    }
                }
                for n in ((1_u64 << 36) - 128)..1 << 36 {
                    assert_eq!(clock.sample(n) == 1.0, reference(n, p, q, fs, width));
                }
            }
        }
        let clock = pulse(0.001, 1.0_f64.next_down(), 48000).unwrap();
        assert_eq!(clock.sample(48000), 0.0);
        assert_eq!(clock.sample(48001), 1.0);
        let slow = pulse(0.001, 2.0_f64.powi(-20), 48000).unwrap();
        assert_eq!(slow.sample(50331647999), 0.0);
        assert_eq!(slow.sample(50331648000), 1.0);
    }
    #[test]
    fn exact_gates_keep_the_tones_bits_and_have_half_open_edges() {
        let pulse = pulse(3.0 / 8000.0, 1000.0, 8000).unwrap();
        let plain = Voice {
            left: true,
            right: true,
            amplitude: 1.0,
            carrier: Carrier::Tone {
                step: step(500.0, 8000).unwrap(),
            },
            modulation: None,
        };
        let gated = Voice {
            modulation: Some(Modulator::Rectangular(pulse)),
            ..plain
        };
        let clicks = Voice {
            carrier: Carrier::Pulse(pulse),
            ..plain
        };
        for n in (0..64).chain((1_u64 << 36) - 64..1 << 36) {
            let open = n % 8 < 3;
            assert_eq!(clicks.sample(n), if open { 1.0 } else { 0.0 });
            if open {
                assert_eq!(gated.sample(n).to_bits(), plain.sample(n).to_bits());
            } else {
                assert_eq!(gated.sample(n), 0.0);
            }
        }
    }
    #[test]
    fn width_and_closed_frame_guards_have_accepted_and_rejected_neighbours() {
        for width in [0.0, -1.0, f64::NAN, f64::INFINITY, 0.4 / 48000.0] {
            assert!(pulse(width, 40.0, 48000).is_none());
        }
        assert!(pulse(0.5 / 48000.0, 40.0, 48000).is_some());
        assert!(pulse(1199.0 / 48000.0, 40.0, 48000).is_some());
        assert!(pulse(1200.0 / 48000.0, 40.0, 48000).is_none());
        assert!(pulse(1227.0 / 48000.0, 39.0625, 48000).is_some());
        assert!(pulse(1228.0 / 48000.0, 39.0625, 48000).is_none());
        assert!(pulse(2.0, 0.1, 48000).is_some());
        let clock = pulse(2.0, 0.1, 48000).unwrap();
        assert_eq!(clock.sample(95999), 1.0);
        assert_eq!(clock.sample(96000), 0.0);
        assert!(times_at_most(40.0, 1200, 48000));
        assert!(!times_at_most(40.0_f64.next_up(), 1200, 48000));
        assert!(!times_at_most(40.0, 1201, 48000));
        assert!(times_at_most(39.0625, 1228, 48000));
        assert!(!times_at_most(39.0625, 1229, 48000));
        assert!(times_at_most(f64::from_bits(1), u64::MAX, 1));
        assert!(times_at_most(0.0, u64::MAX, 1));
        for x in [-40.0, f64::NAN, f64::INFINITY, 1e300] {
            assert!(!times_at_most(x, 1, 768000));
        }
        assert!(!times_at_most(0.1, u64::MAX, 1));
    }
    #[test]
    fn widths_beyond_the_longest_signal_are_rejected() {
        let longest = crate::numeric::frames_to_f64(crate::Signal::MAX_FRAMES);
        // One frame per second: the width in seconds is the width in frames.
        let slow = 1.0 / (longest + 1.0);
        assert!(pulse(longest, slow, 1).is_some());
        assert!(pulse(longest + 1.0, slow, 1).is_none());
        assert!(pulse(2.0_f64.powi(57), 2.0_f64.powi(-86), 8192).is_none());
        let clock = pulse(longest, slow, 1).unwrap();
        for n in [0, 1, crate::Signal::MAX_FRAMES - 1] {
            assert_eq!(clock.sample(n), 1.0);
        }
    }
}
