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
