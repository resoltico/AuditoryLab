// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::synthesis::Voice;
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
    /// The caller has checked `0 < frames <= Signal::MAX_FRAMES` and
    /// `2 * fade_frames < frames`.
    pub(crate) const fn new(frames: u64, fade_frames: u64) -> Self {
        Self {
            frames,
            fade_frames,
        }
    }

    /// The gain of `frame`, within `[0, 1]`; zero past the end.
    #[inline]
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
pub struct Signal {
    sample_rate: SampleRate,
    envelope: Envelope,
    voices: Vec<Voice>,
    peak: [f64; 2],
}

impl Signal {
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

    /// Evaluate one frame; the tail is silence.
    #[must_use]
    #[inline]
    pub fn frame(&self, n: u64) -> [f64; 2] {
        if n >= self.frames() {
            return [0.0; 2];
        }
        let mut out = [0.0; 2];
        for voice in &self.voices {
            let value = voice.sample(n);
            if voice.left {
                out[0] += value;
            }
            if voice.right {
                out[1] += value;
            }
        }
        let gain = self.envelope.gain_at(n);
        out.map(|sample| sample * gain)
    }
    /// Render at any frame, zeroing the inactive tail; return active frames.
    pub fn render(&self, start: u64, out: &mut [[f64; 2]]) -> usize {
        let active = usize::try_from(self.frames().saturating_sub(start))
            .map_or(out.len(), |n| n.min(out.len()));
        for (offset, frame) in out[..active].iter_mut().enumerate() {
            *frame = self.frame(start + offset as u64);
        }
        out[active..].fill([0.0; 2]);
        active
    }
}
