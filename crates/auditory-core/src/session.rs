// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::generator::Voice;
use crate::{SampleRate, numeric};
use alloc::vec::Vec;
use core::f64::consts::PI;

/// The length of a session and its per-frame gain. A positive resolved
/// `fade_frames` gives raised-cosine edges from silence to one and back;
/// zero gives unit gain throughout the active frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Envelope {
    frames: u64,
    fade_frames: u64,
}

impl Envelope {
    /// The caller has checked `0 < frames <= Session::MAX_FRAMES` and
    /// `2 * fade_frames < frames`.
    pub(crate) const fn new(frames: u64, fade_frames: u64) -> Self {
        Self {
            frames,
            fade_frames,
        }
    }

    /// The gain of `frame`, within `[0, 1]`; zero past the end.
    pub(crate) fn gain_at(self, frame: u64) -> f64 {
        if frame >= self.frames {
            return 0.0;
        }
        let depth = frame.min(self.frames - 1 - frame);
        if depth >= self.fade_frames {
            return 1.0;
        }
        let position = numeric::frames_to_f64(depth) / numeric::frames_to_f64(self.fade_frames);
        0.5 - 0.5 * libm::cos(PI * position)
    }
}

/// A stimulus checked at a sample rate: a known number of stereo frames,
/// ready to render from the first.
#[derive(Debug, Clone)]
pub struct Session {
    sample_rate: SampleRate,
    envelope: Envelope,
    voices: Vec<Voice>,
    peak: [f64; 2],
}

impl Session {
    /// The most frames a session can hold: over 16 days at 48,000 Hz and
    /// over 24 hours at [`SampleRate::MAX_HZ`]. Every frame count up to it is
    /// exact in binary64.
    pub const MAX_FRAMES: u64 = 1 << 36;

    pub(crate) const fn new(
        sample_rate: SampleRate,
        envelope: Envelope,
        voices: Vec<Voice>,
        peak: [f64; 2],
    ) -> Self {
        Self {
            sample_rate,
            envelope,
            voices,
            peak,
        }
    }

    /// Return the sample rate of the frames.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.sample_rate
    }

    /// Return the number of frames.
    #[must_use]
    pub const fn frames(&self) -> u64 {
        self.envelope.frames
    }

    /// Return the fade length at each edge, in frames.
    #[must_use]
    pub const fn fade_frames(&self) -> u64 {
        self.envelope.fade_frames
    }

    /// Return the length in seconds, for presentation; division can round.
    #[must_use]
    pub fn duration_seconds(&self) -> f64 {
        self.sample_rate.seconds_in(self.envelope.frames)
    }

    /// Return the fade at each edge in seconds, for presentation.
    #[must_use]
    pub fn fade_seconds(&self) -> f64 {
        self.sample_rate.seconds_in(self.envelope.fade_frames)
    }

    /// Return each ear's summed amplitude bound in dBFS. It is not a measured
    /// or necessarily attained maximum. Silence is negative
    /// infinity.
    #[must_use]
    pub fn peak_dbfs(&self) -> [f64; 2] {
        self.peak.map(numeric::dbfs)
    }

    /// Start rendering at the first frame.
    #[must_use]
    pub fn into_renderer(self) -> Renderer {
        Renderer {
            session: self,
            cursor: 0,
        }
    }
}

/// Produces the frames of a session in order.
///
/// Rendering allocates no memory, takes no lock and performs no I/O, so it
/// can run on a device's real-time thread. Every sample is finite and
/// within `[-1, 1]`.
#[derive(Debug, Clone)]
pub struct Renderer {
    session: Session,
    cursor: u64,
}

impl Renderer {
    /// Return the sample rate of the frames.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.session.sample_rate
    }

    /// Return the number of frames not yet rendered.
    #[must_use]
    pub const fn frames_remaining(&self) -> u64 {
        self.session.envelope.frames - self.cursor
    }

    /// Return whether every frame has been rendered.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.frames_remaining() == 0
    }

    /// Fill `frames` with the next frames, left ear then right, and return
    /// how many came from the session; the rest of `frames` is silence.
    pub fn render(&mut self, frames: &mut [[f64; 2]]) -> usize {
        let count = usize::try_from(self.frames_remaining())
            .map_or(frames.len(), |remaining| remaining.min(frames.len()));
        let (active, silent) = frames.split_at_mut(count);
        for frame in active {
            // Summed in the order the session's peak was summed.
            let (mut left, mut right) = (0.0, 0.0);
            for voice in &mut self.session.voices {
                let sample = voice.next(self.cursor);
                if voice.left {
                    left += sample;
                }
                if voice.right {
                    right += sample;
                }
            }
            let gain = self.session.envelope.gain_at(self.cursor);
            *frame = [left * gain, right * gain];
            self.cursor += 1;
        }
        silent.fill([0.0; 2]);
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{prelude::*, test_runner::Config, test_runner::TestRunner};

    #[test]
    fn the_fade_is_a_raised_cosine_between_silent_edges_and_unity() {
        let envelope = Envelope::new(11, 4);
        // Independent oracle: sin^2(pi/2 * depth / fade), depth counted from the nearer edge.
        let expected = |depth: f64| (core::f64::consts::FRAC_PI_2 * depth / 4.0).sin().powi(2);
        for frame in 0..4_u32 {
            let actual = envelope.gain_at(u64::from(frame));
            assert!((actual - expected(f64::from(frame))).abs() < 1e-15);
            assert_eq!(
                envelope.gain_at(10 - u64::from(frame)).to_bits(),
                actual.to_bits()
            );
        }
        assert_eq!(envelope.gain_at(0), 0.0);
        for frame in 4..=6 {
            assert_eq!(envelope.gain_at(frame), 1.0);
        }
        for frame in [11, 12, u64::MAX] {
            assert_eq!(envelope.gain_at(frame), 0.0);
        }
        let unfaded = Envelope::new(3, 0);
        assert!((0..3).all(|frame| unfaded.gain_at(frame) == 1.0));
    }

    #[test]
    fn the_gain_never_leaves_zero_to_one() {
        let config = Config {
            cases: 256,
            max_shrink_iters: 256,
            max_shrink_time: 1_000,
            failure_persistence: None,
            ..Config::default()
        };
        TestRunner::new(config)
            .run(
                &(1u64..=100_000, any::<u64>(), any::<u64>()),
                |(frames, fade, frame)| {
                    let envelope = Envelope::new(frames, fade % frames.div_ceil(2));
                    prop_assert!((0.0..=1.0).contains(&envelope.gain_at(frame % (frames + 2))));
                    Ok(())
                },
            )
            .unwrap();
    }
}
