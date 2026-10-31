//! Seekable tones and exact frame-quantized pulse trains.
use core::f64::consts::TAU;
#[cfg(test)]
const UNITS_PER_CYCLE: f64 = 18_446_744_073_709_551_616.0;

/// Shared bounded AM formula, retaining the specified evaluation order.
#[inline]
pub(crate) fn am_gain(phase: u64, depth: f64) -> f64 {
    (1.0 + depth * sine(phase)) / (1.0 + depth)
}

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
    if !(crate::validity::positive(hz) && hz < f64::from(fs) / 2.0) {
        return None;
    }
    let (mantissa, exponent) = parts(hz);
    let shift = exponent + 64;
    let fs = u128::from(fs);
    let quotient = if shift >= 0 {
        (u128::from(mantissa) << shift).div_ceil(fs)
    } else {
        // The mantissa has at most 53 bits. A denominator shifted by 64 or
        // more always gives ceil(mantissa/denominator)=1; capping the shift
        // also keeps the denominator within u128 for every supported rate.
        let right_shift = shift.unsigned_abs().min(64);
        u128::from(mantissa).div_ceil(fs << right_shift)
    };
    u64::try_from(quotient).ok()
}
/// sin(2π × phase / 2^64). The top two phase bits, rounded, choose the
/// quadrant exactly; the signed remainder within ±1/8 cycle keeps 51 bits and
/// evaluates a Taylor series whose truncation stays below 2^-53.
#[inline]
#[expect(
    clippy::cast_precision_loss,
    reason = "Arithmetic shift retains a signed 51-bit remainder, exact in binary64"
)]
pub(crate) fn sine(phase: u64) -> f64 {
    let quadrant = phase.wrapping_add(1 << 61) >> 62;
    let remainder = phase.wrapping_sub(quadrant << 62).cast_signed() >> 11;
    let x = remainder as f64 * (TAU / 9_007_199_254_740_992.0);
    let x2 = x * x;
    match quadrant {
        0 => sine_series(x, x2),
        1 => cosine_series(x2),
        // Subtraction keeps the half-cycle zero positive, like phase zero.
        2 => 0.0 - sine_series(x, x2),
        _ => -cosine_series(x2),
    }
}
/// sin x for |x| ≤ π/4, through the x^15 term.
#[inline]
fn sine_series(x: f64, x2: f64) -> f64 {
    let tail = -1.0 / 1_307_674_368_000.0;
    let tail = tail * x2 + 1.0 / 6_227_020_800.0;
    let tail = tail * x2 - 1.0 / 39_916_800.0;
    let tail = tail * x2 + 1.0 / 362_880.0;
    let tail = tail * x2 - 1.0 / 5_040.0;
    let tail = tail * x2 + 1.0 / 120.0;
    let tail = tail * x2 - 1.0 / 6.0;
    x + x * x2 * tail
}
/// cos x for |x| ≤ π/4, through the x^16 term.
#[inline]
fn cosine_series(x2: f64) -> f64 {
    let tail = 1.0 / 20_922_789_888_000.0;
    let tail = tail * x2 - 1.0 / 87_178_291_200.0;
    let tail = tail * x2 + 1.0 / 479_001_600.0;
    let tail = tail * x2 - 1.0 / 3_628_800.0;
    let tail = tail * x2 + 1.0 / 40_320.0;
    let tail = tail * x2 - 1.0 / 720.0;
    let tail = tail * x2 + 1.0 / 24.0;
    let tail = tail * x2 - 0.5;
    1.0 + x2 * tail
}
/// Whether x × k ≤ fs, exactly, for non-negative finite x, positive k and a valid sample rate.
#[expect(
    clippy::cast_sign_loss,
    reason = "Binary64 decomposition and the early exponent guards make the negated \
        exponent nonnegative before converting to a shift"
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
    if whole == fs {
        // Equality of the nonzero whole parts proves this shift fits u128.
        product == fs << shift
    } else {
        whole < fs
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Pulse {
    mantissa: u64,
    shift: u32,
    fs: u32,
    frames: u64,
    rise_frames: u64,
    fall_frames: u64,
}
impl Pulse {
    pub(crate) const fn frames(self) -> u64 {
        self.frames
    }
    /// floor(n × rate), with n below `Signal::MAX_FRAMES`.
    #[inline]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "The validated n < 2^36 and rate < 2^19 bound the floored rate product below 2^55"
    )]
    fn rate_product(self, n: u64) -> u64 {
        // n < 2^36 and rate < 2^19 keep the shifted value below 2^55.
        (u128::from(n) * u128::from(self.mantissa))
            .checked_shr(self.shift)
            .unwrap_or(0) as u64
    }
    /// floor(n × rate/fs), using floor(floor(n × rate)/fs).
    #[inline]
    fn periods(self, n: u64) -> u64 {
        self.rate_product(n) / u64::from(self.fs)
    }
    /// Extrema over every discrete frame of [start,end), without enumerating pulses.
    pub(crate) fn extent(self, start: u64, end: u64) -> crate::Extent {
        if self.rise_frames != 0 {
            let first = self.sample(start);
            let last = self.sample(end - 1);
            let initial_period = self.periods(start);
            let final_period = self.periods(end - 1);
            let a = self.local(start);
            let b = self.local(end - 1);
            let plateau_end = self.frames - 1 - self.fall_frames;
            let max = if initial_period == final_period {
                if a <= plateau_end && b >= self.rise_frames {
                    1.0
                } else {
                    first.max(last)
                }
            } else if final_period - initial_period > 1 || a <= plateau_end || b >= self.rise_frames
            {
                1.0
            } else {
                first.max(last)
            };
            return crate::Extent {
                min: if initial_period == final_period {
                    first.min(last)
                } else {
                    0.0
                },
                max,
            };
        }
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
    fn local(self, n: u64) -> u64 {
        self.local_in(n, self.periods(n))
    }
    /// Frames since the onset of period `k`, which contains frame `n`.
    #[inline]
    fn local_in(self, n: u64, k: u64) -> u64 {
        let onset = if k == 0 {
            0
        } else {
            let numerator = (u128::from(k) * u128::from(self.fs)) << self.shift;
            u64::try_from(numerator.div_ceil(u128::from(self.mantissa)))
                .unwrap_or_else(|_| unreachable!("onset precedes the bounded frame"))
        };
        n - onset
    }
    #[inline(always)]
    #[expect(
        clippy::inline_always,
        reason = "Inlining into the live frame loop avoids copying the pulse per call; \
            the callback cost check measures it"
    )]
    pub(crate) fn sample(self, n: u64) -> f64 {
        let k = self.periods(n);
        // Frame x has reached period k's onset exactly when periods(x) ≥ k,
        // that is when floor(x × rate) ≥ k × fs; comparing products avoids
        // dividing again, and only edge frames locate the onset itself.
        let onset_product = k * u64::from(self.fs);
        let open = n < self.frames || onset_product > self.rate_product(n - self.frames);
        if !open {
            return 0.0;
        }
        if self.rise_frames == 0 {
            return 1.0;
        }
        let in_rise = n
            .checked_sub(self.rise_frames)
            .is_none_or(|earlier| self.rate_product(earlier) < onset_product);
        let in_fall = !in_rise
            && n.checked_sub(self.frames - self.fall_frames)
                .is_some_and(|earlier| self.rate_product(earlier) >= onset_product);
        if !in_rise && !in_fall {
            return 1.0;
        }
        let local = self.local_in(n, k);
        let (index, length) = if in_rise {
            (local, self.rise_frames)
        } else {
            (self.frames - 1 - local, self.fall_frames)
        };
        (1.0 - libm::cos(
            core::f64::consts::PI * crate::conversion::frames_to_f64(index)
                / crate::conversion::frames_to_f64(length),
        )) / 2.0
    }
}
#[cfg(test)]
fn pulse(seconds: f64, rate_hz: f64, fs: u32) -> Option<Pulse> {
    pulse_spec(
        crate::PulseSpec {
            rate_hz: crate::Finite::new(rate_hz)?,
            width: crate::PulseWidth::Duration {
                seconds: crate::Finite::new(seconds)?,
            },
            shape: crate::PulseShape::Rectangular {},
        },
        fs,
    )
    .ok()
}
/// Compile width intent and edges without reconstructing the requested quantity.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Resolved whole frame values are checked against the 2^36 horizon before conversion"
)]
pub(crate) fn pulse_spec(spec: crate::PulseSpec, fs: u32) -> Result<Pulse, crate::PulseError> {
    use crate::{PulseError, PulseShape, PulseWidth};
    let hz = spec.rate_hz.get();
    if step(hz, fs).is_none() {
        return Err(PulseError::Rate);
    }
    let raw = match spec.width {
        PulseWidth::Duration { seconds } => {
            if seconds.get() <= 0.0 {
                return Err(PulseError::Width);
            }
            seconds.get() * f64::from(fs)
        }
        PulseWidth::DutyCycle { fraction } => {
            let fraction = fraction.get();
            if !PulseWidth::valid_fraction(fraction) {
                return Err(PulseError::Duty);
            }
            (fraction * f64::from(fs)) / hz
        }
    };
    let rounded = libm::round(raw);
    let limit = crate::conversion::frames_to_f64(crate::Signal::MAX_FRAMES);
    if !(rounded >= 1.0 && rounded <= limit) {
        return Err(PulseError::Width);
    }
    let frames = rounded as u64;
    if !times_at_most(hz, frames + 1, fs) {
        return Err(PulseError::ClosedFrame);
    }
    let (rise_frames, fall_frames) = match spec.shape {
        PulseShape::Rectangular {} => (0, 0),
        PulseShape::RaisedCosine {
            rise_seconds,
            fall_seconds,
        } => {
            let rise = libm::round(rise_seconds.get() * f64::from(fs));
            let fall = libm::round(fall_seconds.get() * f64::from(fs));
            // A rounded edge of at least one frame implies a positive request.
            if !(rise >= 1.0 && fall >= 1.0 && rise <= limit && fall <= limit) {
                return Err(PulseError::Edges);
            }
            let (rise, fall) = (rise as u64, fall as u64);
            if rise + fall > frames - 1 {
                return Err(PulseError::Edges);
            }
            (rise, fall)
        }
    };
    let (mantissa, exponent) = parts(hz);
    let shift = u32::try_from(-exponent)
        .unwrap_or_else(|_| unreachable!("sub-Nyquist frequencies have negative binary exponent"));
    Ok(Pulse {
        mantissa,
        shift,
        fs,
        frames,
        rise_frames,
        fall_frames,
    })
}
impl Pulse {
    pub(crate) const fn edges(self) -> [u64; 2] {
        [self.rise_frames, self.fall_frames]
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Carrier {
    Tone { step: u64 },
    Pulse(Pulse),
}
impl Carrier {
    pub(crate) fn sample(self, n: u64) -> f64 {
        match self {
            Self::Tone { step } => sine(n.wrapping_mul(step)),
            Self::Pulse(pulse) => pulse.sample(n),
        }
    }
}
/// Source lanes sum first, then the one shared gain scales the stereo component.
#[inline]
pub(crate) fn component_frame(
    samples: [f64; 2],
    gains: [[f64; 2]; 2],
    modulation: f64,
) -> [f64; 2] {
    let mut out = [0.0; 2];
    for (sample, gains) in samples.into_iter().zip(gains) {
        for (out, gain) in out.iter_mut().zip(gains) {
            if gain != 0.0 {
                *out += sample * gain;
            }
        }
    }
    out.map(|value| value * modulation)
}
#[inline]
pub(crate) fn modulation_gain(modulation: Option<Modulator>, n: u64) -> f64 {
    match modulation {
        None => 1.0,
        Some(Modulator::Sinusoidal { step, depth }) => am_gain(n.wrapping_mul(step), depth),
        Some(Modulator::Pulsed(pulse)) => pulse.sample(n),
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Modulator {
    Sinusoidal { step: u64, depth: f64 },
    Pulsed(Pulse),
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GeneratorLane {
    pub(crate) left: bool,
    pub(crate) right: bool,
    pub(crate) amplitude: f64,
    pub(crate) carrier: Carrier,
}
impl GeneratorLane {
    /// The linear routing gains at full envelope, left and right.
    pub(crate) const fn gains(&self) -> [f64; 2] {
        [
            if self.left { self.amplitude } else { 0.0 },
            if self.right { self.amplitude } else { 0.0 },
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// The series stays within two binary64 units of portable libm across
    /// every quadrant and its boundaries, and the axis phases are exact. The
    /// reference corrects libm's argument by the rounding of 2π × cycles.
    #[test]
    #[expect(
        clippy::cast_precision_loss,
        reason = "Reference cycles from a 53-bit phase"
    )]
    fn sine_matches_libm_and_is_exact_on_the_axes() {
        const TAU_LOW: f64 = 2.449_293_598_294_706_4e-16;
        let reference = |phase: u64| {
            let cycles = (phase.cast_signed() >> 11) as f64 / 9_007_199_254_740_992.0;
            let angle = TAU * cycles;
            let error = libm::fma(TAU, cycles, -angle) + TAU_LOW * cycles;
            libm::sin(angle) + error * libm::cos(angle)
        };
        let quarter = 1_u64 << 62;
        let mut phases = [0, quarter, 2 * quarter, 3 * quarter]
            .into_iter()
            .flat_map(|axis| {
                let eighth = 1_u64 << 61;
                [axis, axis.wrapping_add(eighth), axis.wrapping_sub(eighth)]
                    .into_iter()
                    .flat_map(|centre| (0..64_u64).map(move |d| centre.wrapping_add(d << 11)))
                    .chain([axis.wrapping_add(eighth - 1), axis.wrapping_sub(eighth + 1)])
            })
            .collect::<std::vec::Vec<_>>();
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        phases.extend((0..200_000).map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        }));
        for phase in phases {
            let value = sine(phase);
            assert!(value.abs() <= 1.0, "{phase:#x}");
            let expected = reference(phase);
            assert!(
                (value - expected).abs() <= 2.3e-16,
                "{phase:#x}: {value} {expected}"
            );
        }
        assert_eq!(sine(0).to_bits(), 0.0_f64.to_bits());
        assert_eq!(sine(quarter), 1.0);
        assert_eq!(sine(2 * quarter).to_bits(), 0.0_f64.to_bits());
        assert_eq!(sine(3 * quarter), -1.0);
    }

    /// Shaped samples equal the original rule that locates every open frame
    /// by its onset division before choosing an edge.
    #[test]
    fn shaped_edges_match_the_located_frame_reference() {
        fn reference(pulse: Pulse, n: u64) -> f64 {
            let open = n < pulse.frames
                || pulse.periods(n) * u64::from(pulse.fs) > pulse.rate_product(n - pulse.frames);
            if !open {
                return 0.0;
            }
            let local = pulse.local(n);
            let remaining = pulse.frames - 1 - local;
            let edge = if local < pulse.rise_frames {
                Some((local, pulse.rise_frames))
            } else if remaining < pulse.fall_frames {
                Some((remaining, pulse.fall_frames))
            } else {
                None
            };
            edge.map_or(1.0, |(index, length)| {
                (1.0 - libm::cos(
                    core::f64::consts::PI * crate::conversion::frames_to_f64(index)
                        / crate::conversion::frames_to_f64(length),
                )) / 2.0
            })
        }
        let finite = |value| crate::Finite::new(value).unwrap();
        for fs in [44_100, 48_000] {
            for (rate, fraction, rise, fall) in [
                (40.0, 0.5, 0.002, 0.001),
                (37.3, 0.25, 0.0005, 0.003),
                (1_000.7, 0.6, 0.0003, 0.0002),
                // Below 1 Hz neighbouring frames share a rate product, so the
                // frame after an onset compares equal to the onset product.
                (0.5, 0.5, 0.002, 0.001),
                // Edges filling the window, so rise and fall meet.
                (100.0, 0.5, 0.0025, 0.002_45),
            ] {
                let pulse = pulse_spec(
                    crate::PulseSpec {
                        rate_hz: finite(rate),
                        width: crate::PulseWidth::DutyCycle {
                            fraction: finite(fraction),
                        },
                        shape: crate::PulseShape::RaisedCosine {
                            rise_seconds: finite(rise),
                            fall_seconds: finite(fall),
                        },
                    },
                    fs,
                )
                .unwrap();
                let far = 1_u64 << 33;
                for n in (0..u64::from(fs) / 4).chain(far..far + u64::from(fs) / 20) {
                    assert_eq!(
                        pulse.sample(n).to_bits(),
                        reference(pulse, n).to_bits(),
                        "{rate} Hz at {fs}: frame {n}"
                    );
                }
            }
        }
    }

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
        reason = "Independent rational test oracles intentionally convert integer \
            ratios and rounded millisecond widths; fixture widths and indices are \
            nonnegative and fit u64"
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
        reason = "Independent rational test oracles intentionally convert integer \
            ratios and rounded millisecond widths; fixture widths and indices are \
            nonnegative and fit u64"
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
        reason = "Independent rational test oracles intentionally convert integer \
            ratios and rounded millisecond widths; fixture widths and indices are \
            nonnegative and fit u64"
    )]
    fn pulses_match_independent_integer_onsets_at_early_and_late_frames() {
        for fs in [8000, 44100, 48000, 96000, 768_000] {
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
        reason = "Independent rational test oracles intentionally convert integer \
            ratios and rounded millisecond widths; fixture widths and indices are \
            nonnegative and fit u64"
    )]
    fn fractional_and_slow_rates_do_not_open_before_the_exact_onset() {
        for (p, q) in [
            (9_007_199_254_740_991_u128, 1_u128 << 53),
            (7_205_759_403_792_793, 1 << 56),
            (1, 1 << 20),
            (625, 16),
        ] {
            for fs in [8000, 44100, 48000, 96000, 768_000] {
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
        assert_eq!(slow.sample(50_331_647_999), 0.0);
        assert_eq!(slow.sample(50_331_648_000), 1.0);
    }
    #[test]
    fn exact_gates_keep_the_tones_bits_and_have_half_open_edges() {
        let pulse = pulse(3.0 / 8000.0, 1000.0, 8000).unwrap();
        let tone = Carrier::Tone {
            step: step(500.0, 8000).unwrap(),
        };
        for n in (0..64).chain((1_u64 << 36) - 64..1 << 36) {
            let open = n % 8 < 3;
            assert_eq!(pulse.sample(n), if open { 1.0 } else { 0.0 });
            if open {
                assert_eq!(
                    component_frame([tone.sample(n), 0.0], [[1.0; 2], [0.0; 2]], pulse.sample(n))
                        [0]
                    .to_bits(),
                    tone.sample(n).to_bits()
                );
            } else {
                assert_eq!(
                    component_frame([tone.sample(n), 0.0], [[1.0; 2], [0.0; 2]], pulse.sample(n)),
                    [0.0; 2]
                );
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
            assert!(!times_at_most(x, 1, 768_000));
        }
        assert!(!times_at_most(0.1, u64::MAX, 1));
    }
    #[test]
    fn widths_beyond_the_longest_signal_are_rejected() {
        let longest = crate::conversion::frames_to_f64(crate::Signal::MAX_FRAMES);
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
