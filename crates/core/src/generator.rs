// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! The sample generators behind each component. Tone and modulation phase
//! stays in cycles within `[0, 1)` and advances once per frame. Click phase
//! comes from the absolute frame index, avoiding cumulative rounding drift.
//!
//! Every generator's output is bounded by construction: a carrier lies in
//! `[-1, 1]`, a modulation envelope in `[0, 1]`, and a voice in
//! `[-amplitude, amplitude]`. Rounding to nearest is monotonic, so these
//! bounds hold for the computed values too, not just the exact ones.

use crate::numeric;
use core::f64::consts::TAU;

/// A phase accumulator advancing a fixed step of cycles per frame.
#[derive(Debug, Clone)]
pub(crate) struct Phase {
    cycles: f64,
    step: f64,
}

impl Phase {
    /// Start at phase zero. The caller has checked that `step` is in `(0, 0.5)`.
    pub(crate) const fn new(step: f64) -> Self {
        Self { cycles: 0.0, step }
    }

    /// Return the current phase and advance it by one frame.
    fn advance(&mut self) -> f64 {
        let cycles = self.cycles;
        self.cycles += self.step;
        if self.cycles >= 1.0 {
            self.cycles -= 1.0;
        }
        cycles
    }

    /// Return the sine of the current phase and advance it.
    fn sine(&mut self) -> f64 {
        libm::sin(self.advance() * TAU)
    }
}

/// Unipolar rectangular pulses: 1 while the phase of the frame is below
/// `duty`, otherwise 0, with a pulse starting at frame zero.
///
/// The phase comes from the absolute frame index, so it accumulates no drift;
/// rounding near a pulse edge may still select an adjacent frame.
#[derive(Debug, Clone)]
pub(crate) struct PulseTrain {
    step: f64,
    duty: f64,
}

impl PulseTrain {
    /// The caller has checked that `step` is a normal binary64 value below
    /// one half and that `duty` is below one.
    pub(crate) const fn new(step: f64, duty: f64) -> Self {
        Self { step, duty }
    }

    fn at(&self, frame: u64) -> f64 {
        // The frame index is exact through Session::MAX_FRAMES.
        let cycles = numeric::frames_to_f64(frame) * self.step;
        let phase = cycles - libm::floor(cycles);
        if phase < self.duty { 1.0 } else { 0.0 }
    }
}

/// The waveform of a component before its level and modulation.
#[derive(Debug, Clone)]
pub(crate) enum Carrier {
    /// A sine wave.
    Tone(Phase),
    /// Rectangular clicks.
    Click(PulseTrain),
}

impl Carrier {
    fn next(&mut self, frame: u64) -> f64 {
        match self {
            Self::Tone(phase) => phase.sine(),
            Self::Click(pulses) => pulses.at(frame),
        }
    }
}

/// Sinusoidal amplitude modulation, normalized to a peak of one:
/// `(1 + depth * sin(2π rate t)) / (1 + depth)`.
#[derive(Debug, Clone)]
pub(crate) struct Sinusoidal {
    pub(crate) phase: Phase,
    pub(crate) depth: f64,
}

impl Sinusoidal {
    fn next(&mut self) -> f64 {
        // Dividing by the same computed `1 + depth` keeps the result within [0, 1].
        (1.0 + self.depth * self.phase.sine()) / (1.0 + self.depth)
    }
}

/// A gain within `[0, 1]` applied to a carrier.
#[derive(Debug, Clone)]
pub(crate) enum Modulator {
    Sinusoidal(Sinusoidal),
    /// On/off gating by the same pulse train as clicks.
    Rectangular(PulseTrain),
}

impl Modulator {
    fn next(&mut self, frame: u64) -> f64 {
        match self {
            Self::Sinusoidal(modulation) => modulation.next(),
            Self::Rectangular(pulses) => pulses.at(frame),
        }
    }
}

/// One component, ready to render.
#[derive(Debug, Clone)]
pub(crate) struct Voice {
    pub(crate) left: bool,
    pub(crate) right: bool,
    pub(crate) amplitude: f64,
    pub(crate) carrier: Carrier,
    pub(crate) modulation: Option<Modulator>,
}

impl Voice {
    /// Return the next sample, within `[-amplitude, amplitude]`.
    #[inline]
    pub(crate) fn next(&mut self, frame: u64) -> f64 {
        let carrier = self.carrier.next(frame);
        let shaped = match &mut self.modulation {
            None => carrier,
            Some(modulation) => carrier * modulation.next(frame),
        };
        self.amplitude * shaped
    }
}

#[cfg(test)]
mod tests {
    // The platform's mathematics library, reachable only in tests, is the independent oracle.
    use super::*;
    use alloc::vec::Vec;

    fn voice(carrier: Carrier, modulation: Option<Modulator>) -> Voice {
        Voice {
            left: true,
            right: false,
            amplitude: 1.0,
            carrier,
            modulation,
        }
    }

    #[test]
    fn a_tone_follows_the_sine_of_its_phase_and_stays_within_one_cycle() {
        let mut phase = Phase::new(375.0 / 8_000.0);
        for frame in 0..500_000_u32 {
            let expected = (TAU * 375.0 * f64::from(frame) / 8_000.0).sin();
            let sample = phase.sine();
            if frame < 4_096 {
                assert!((sample - expected).abs() < 1e-11, "frame {frame}");
            }
            assert!(sample.abs() <= 1.0);
            assert!((0.0..1.0).contains(&phase.cycles));
        }
    }

    #[test]
    fn clicks_are_unit_pulses_of_the_requested_width_at_the_requested_rate() {
        // 40 Hz, 1 ms at 48 kHz: 48 frames on in every 1200.
        let step = 40.0 / 48_000.0;
        let mut clicks = voice(Carrier::Click(PulseTrain::new(step, 0.001 * 40.0)), None);
        let samples: Vec<f64> = (0..48_000).map(|frame| clicks.next(frame)).collect();
        assert!(samples.iter().all(|s| *s == 0.0 || *s == 1.0));
        let onsets: Vec<usize> = (0..samples.len())
            .filter(|&i| samples[i] == 1.0 && (i == 0 || samples[i - 1] == 0.0))
            .collect();
        assert_eq!(onsets.len(), 40);
        for (n, onset) in onsets.iter().enumerate() {
            assert!(onset.abs_diff(n * 1_200) <= 1, "click {n} at {onset}");
            let width = samples[*onset..].iter().take_while(|s| **s == 1.0).count();
            assert!((47..=49).contains(&width), "click {n} is {width} frames");
        }
    }

    #[test]
    fn a_click_covers_the_half_open_start_of_each_period() {
        // 1000 Hz at 8 kHz steps the phase by exactly 1/8; a 3/8 duty cycle ends exactly on a frame.
        let mut clicks = voice(Carrier::Click(PulseTrain::new(0.125, 0.375)), None);
        let samples: Vec<f64> = (0..16).map(|frame| clicks.next(frame)).collect();
        let period = [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(samples, [period, period].concat());
    }

    #[test]
    fn sinusoidal_modulation_has_the_carrier_and_two_sidebands_at_the_expected_levels() {
        // depth 1: (1 + sin m)/2 * sin c = sin c / 2 + [cos(c - m) - cos(c + m)] / 4.
        let rate = 8_000.0;
        let mut am = voice(
            Carrier::Tone(Phase::new(1_000.0 / rate)),
            Some(Modulator::Sinusoidal(Sinusoidal {
                phase: Phase::new(125.0 / rate),
                depth: 1.0,
            })),
        );
        let samples: Vec<f64> = (0..8_000).map(|frame| am.next(frame)).collect();
        let line = |hz: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            for (n, s) in samples.iter().enumerate() {
                let phase = TAU * hz * f64::from(u32::try_from(n).unwrap()) / rate;
                re += s * phase.cos();
                im += s * phase.sin();
            }
            2.0 * re.hypot(im) / 8_000.0
        };
        for (hz, expected) in [(1_000.0, 0.5), (875.0, 0.25), (1_125.0, 0.25), (750.0, 0.0)] {
            assert!((line(hz) - expected).abs() < 1e-9, "{hz} Hz");
        }
        for (n, s) in samples.iter().enumerate() {
            let t = f64::from(u32::try_from(n).unwrap()) / rate;
            let expected = (TAU * 1_000.0 * t).sin() * (1.0 + (TAU * 125.0 * t).sin()) / 2.0;
            assert!((s - expected).abs() < 1e-11);
        }
    }

    #[test]
    fn partial_modulation_follows_its_formula_and_peaks_at_one() {
        // 40 Hz at 8 kHz: one period is 200 frames, and frame 50 is the sine's crest.
        for depth in [0.25, 0.5, 0.75] {
            let mut envelope = Sinusoidal {
                phase: Phase::new(40.0 / 8_000.0),
                depth,
            };
            let values: Vec<f64> = (0..200).map(|_| envelope.next()).collect();
            for (n, value) in (0_u32..).zip(&values) {
                let expected =
                    (1.0 + depth * (TAU * 40.0 * f64::from(n) / 8_000.0).sin()) / (1.0 + depth);
                assert!((value - expected).abs() < 1e-12, "{depth} at {n}");
            }
            assert!((values[50] - 1.0).abs() < 1e-12, "{depth}");
            assert!(
                (values[150] - (1.0 - depth) / (1.0 + depth)).abs() < 1e-12,
                "{depth}"
            );
        }
    }

    #[test]
    fn rectangular_modulation_gates_a_continuous_tone_with_the_click_pulse_train() {
        // 1000 Hz at 8 kHz gated at 1000 Hz with a 3/8 duty: the first three frames of each period.
        let rate = 8_000.0;
        let mut gated = voice(
            Carrier::Tone(Phase::new(1_000.0 / rate)),
            Some(Modulator::Rectangular(PulseTrain::new(
                1_000.0 / rate,
                0.375,
            ))),
        );
        for frame in 0..64_u32 {
            let tone = (TAU * 1_000.0 * f64::from(frame) / rate).sin();
            let open = frame % 8 < 3;
            let sample = gated.next(u64::from(frame));
            if open {
                assert!((sample - tone).abs() < 1e-12, "frame {frame}");
            } else {
                // The product of a negative sine and a closed gate is negative zero.
                assert_eq!(sample, 0.0, "frame {frame}");
            }
        }
    }

    #[test]
    fn modulation_envelopes_never_leave_zero_to_one_at_any_depth() {
        for depth in [0.0, 0.25, 0.999_999, 1.0] {
            let mut envelope = Sinusoidal {
                phase: Phase::new(0.1234),
                depth,
            };
            for _ in 0..100_000 {
                let value = envelope.next();
                assert!((0.0..=1.0).contains(&value), "{depth}: {value}");
            }
        }
    }
}

#[cfg(test)]
mod click_clock_tests {
    use super::*;

    #[test]
    fn absolute_click_phase_does_not_accumulate_long_period_drift() {
        let rate = 48_000.0 / 500_000_000.0;
        let mut click = Carrier::Click(PulseTrain::new(rate / 48_000.0, rate / 48_000.0));
        for frame in [499_999_993, 499_999_994, 499_999_999] {
            assert_eq!(click.next(frame), 0.0, "frame {frame}");
        }
        assert_eq!(click.next(500_000_000), 1.0);
        assert_eq!(click.next(500_000_001), 0.0);
    }

    #[test]
    fn absolute_click_phase_documents_adjacent_frame_boundary_rounding() {
        let rate = 7.367_306_782_540_053e-7;
        let mut click = Carrier::Click(PulseTrain::new(rate / 48_000.0, (1.0 / 48_000.0) * rate));
        let boundary = 65_152_709_690;
        assert_eq!(click.next(boundary - 1), 0.0);
        assert_eq!(click.next(boundary), 0.0);
        assert_eq!(click.next(boundary + 1), 1.0);
        assert_eq!(click.next(boundary + 2), 0.0);
    }
}

#[cfg(test)]
mod characterization {
    use super::*;

    // See docs/signal-verification.md: u=2^-53, one rounded division,
    // <=u absolute error per bounded addition, and exact Sterbenz subtraction.
    const U: f64 = f64::EPSILON / 2.0;
    fn reference(frame: u64, numerator: u32, denominator: u32, hz: u32) -> f64 {
        let period = u128::from(denominator) * u128::from(hz);
        let remainder = (u128::from(frame) * u128::from(numerator)) % period;
        f64::from(u32::try_from(remainder).unwrap()) / f64::from(u32::try_from(period).unwrap())
    }
    fn budget(frame: u64, frequency: f64, hz: u32) -> f64 {
        let n = numeric::frames_to_f64(frame);
        n * U * (frequency / f64::from(hz)) + n * U + U
    }
    fn close(actual: f64, expected: f64, limit: f64) -> bool {
        let difference = (actual - expected).abs();
        (0.0..1.0).contains(&actual) && difference.min((1.0 - difference).abs()) <= limit
    }
    fn check_phase(numerator: u32, denominator: u32, hz: u32, frames: u64) {
        let frequency = f64::from(numerator) / f64::from(denominator);
        let mut phase = Phase::new(frequency / f64::from(hz));
        let mut checkpoint = 0;
        for frame in 0..frames {
            let actual = phase.advance();
            // Keep the long run inexpensive, but include both sides of its end.
            if frame < 8 || frame == checkpoint || frame + 8 >= frames {
                if frame == checkpoint {
                    checkpoint += u64::from(hz);
                }
                let expected = reference(frame, numerator, denominator, hz);
                let tolerance = budget(frame, frequency, hz);
                assert!(
                    close(actual, expected, tolerance),
                    "{frequency} Hz / {hz}, frame {frame}: {actual} vs {expected}, limit {tolerance}"
                );
                // Sine is 1-Lipschitz in radians. Add a separate 32u allowance
                // for bounded-angle argument/kernel/native-reference evaluation.
                let sample = libm::sin(actual * TAU);
                let oracle = (expected * TAU).sin();
                assert!((sample - oracle).abs() <= TAU * tolerance + 32.0 * U);
            }
        }
    }
    #[test]
    fn declared_frequencies_match_independent_rational_phase_and_sine_references() {
        for hz in [44_100, 48_000, 96_000] {
            for (numerator, denominator) in [(807, 2), (625, 16), (10_000, 1)] {
                check_phase(numerator, denominator, hz, u64::from(hz) * 2);
            }
        }
    }
    #[test]
    #[ignore = "offline long-session characterization; run explicitly in release mode"]
    fn hour_long_accumulation_matches_the_declared_frequency_budget() {
        for hz in [44_100, 48_000, 96_000] {
            // The longest built-in tone-pip sessions run for one hour. Check
            // their 10 kHz carrier plus the slow 39.0625 Hz catalog SAM clock.
            for (numerator, denominator) in [(10_000, 1), (625, 16)] {
                check_phase(numerator, denominator, hz, u64::from(hz) * 3_600);
            }
        }
    }
    fn check_pulse(train: &PulseTrain, frame: u64, hz: u32, rate: u32) -> bool {
        let phase = reference(frame, rate, 1, hz);
        let duty = f64::from(rate) / 1_000.0;
        // Independent decimal 1 ms width and integer declared rate. Division
        // and multiplication each contribute <=u relative uncertainty. Floor
        // and the fractional subtraction add none for these bounded indices.
        let uncertainty = 2.0 * U * numeric::frames_to_f64(frame) * f64::from(rate) / f64::from(hz)
            + 2.0 * U * duty
            + U;
        let near_edge = phase.min(1.0 - phase).min((phase - duty).abs()) <= uncertainty;
        let expected = if phase < duty { 1.0 } else { 0.0 };
        train.at(frame) == expected || near_edge
    }
    #[test]
    fn late_pulse_edges_and_44100_hz_counts_follow_rational_boundaries() {
        for hz in [44_100, 48_000, 96_000, 768_000] {
            for rate in [8, 40, 80] {
                let train =
                    PulseTrain::new(f64::from(rate) / f64::from(hz), 0.001 * f64::from(rate));
                let last_cycle = crate::Session::MAX_FRAMES * u64::from(rate) / u64::from(hz) - 1;
                for cycle in [0, 1, u64::from(rate) * 3_600 - 1, last_cycle] {
                    let onset = (u128::from(cycle) * u128::from(hz)).div_ceil(u128::from(rate));
                    let end = (u128::from(cycle) * u128::from(hz) * 1_000
                        + u128::from(hz) * u128::from(rate))
                    .div_ceil(u128::from(rate) * 1_000);
                    for edge in [onset, end] {
                        let edge = u64::try_from(edge).unwrap();
                        for frame in edge.saturating_sub(2)..=edge + 2 {
                            assert!(frame <= crate::Session::MAX_FRAMES);
                            assert!(
                                check_pulse(&train, frame, hz, rate),
                                "{hz}/{rate}: frame {frame}"
                            );
                        }
                    }
                }
            }
        }
        let train = PulseTrain::new(40.0 / 44_100.0, 0.04);
        let mut onsets = 0;
        let mut previous = false;
        let mut width = 0;
        for frame in 0..44_100 {
            let active = train.at(frame) == 1.0;
            assert!(check_pulse(&train, frame, 44_100, 40));
            if active && !previous {
                onsets += 1;
            }
            if active {
                width += 1;
            }
            if !active && previous {
                assert!((43..=46).contains(&width));
                width = 0;
            }
            previous = active;
        }
        assert_eq!(onsets, 40);
        // Exactly representable clocks have an unambiguous half-open edge even
        // at the frame limit, independent of a multiplication using the step.
        let exact = PulseTrain::new(0.125, 0.375);
        for frame in crate::Session::MAX_FRAMES - 16..=crate::Session::MAX_FRAMES {
            assert_eq!(exact.at(frame), if frame % 8 < 3 { 1.0 } else { 0.0 });
        }
    }
    #[test]
    fn characterization_detects_phase_rate_and_half_open_edge_defects() {
        let n = 48_000;
        let expected = reference(n, 807, 2, 48_000);
        let limit = budget(n, 403.5, 48_000);
        for (step, initial) in [
            (403.5 / 48_000.0, 0.0),
            (403.6 / 48_000.0, 0.0),
            (403.5 / 48_000.0, 0.01),
        ] {
            let mut phase = Phase::new(step);
            phase.cycles = initial;
            for _ in 0..n {
                phase.advance();
            }
            assert_eq!(
                close(phase.cycles, expected, limit),
                initial == 0.0 && step == 403.5 / 48_000.0
            );
        }
        let expected_off = PulseTrain::new(0.125, 0.375).at(3);
        assert_eq!(expected_off, 0.0);
        assert_ne!(
            PulseTrain::new(0.125, 0.375_f64.next_up()).at(3),
            expected_off
        );
        assert!(!check_pulse(
            &PulseTrain::new(41.0 / 44_100.0, 0.041),
            1_080,
            44_100,
            40
        ));
    }
}
