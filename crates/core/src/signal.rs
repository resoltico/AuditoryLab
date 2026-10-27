// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::{SampleRate, SoundError, Target, VoiceFacts, VoiceId, numeric};
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
    /// Resolve a duration and fade to whole frames at `rate`.
    ///
    /// Errors follow the finite order: duration, fade, then overlapping fades.
    pub(crate) fn resolve(
        rate: SampleRate,
        duration_seconds: f64,
        fade_seconds: f64,
    ) -> Result<Self, SoundError> {
        let frames = rate
            .frames_in(duration_seconds)
            .filter(|frames| *frames > 0)
            .ok_or(SoundError::Duration {
                seconds: duration_seconds,
                sample_rate_hz: rate.hz(),
            })?;
        let fade_frames = rate.frames_in(fade_seconds).ok_or(SoundError::Fade {
            seconds: fade_seconds,
            sample_rate_hz: rate.hz(),
        })?;
        if fade_frames > (frames - 1) / 2 {
            return Err(SoundError::FadesOverlap {
                fade_frames,
                frames,
                sample_rate_hz: rate.hz(),
            });
        }
        Ok(Self {
            frames,
            fade_frames,
        })
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

/// A sound checked at a sample rate: a known number of stereo frames,
/// ready to render from the first.
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    target: Target,
    envelope: Envelope,
}

impl Signal {
    /// The most frames a session can hold: over 16 days at 48,000 Hz and
    /// over 24 hours at [`SampleRate::MAX_HZ`]. Every frame count up to it is
    /// exact in binary64.
    pub const MAX_FRAMES: u64 = 1 << 36;

    pub(crate) const fn new(target: Target, envelope: Envelope) -> Self {
        Self { target, envelope }
    }

    /// Check a duration and fade at `rate` with the same rules and order as
    /// [`Sound::signal`](crate::Sound::signal), without voices.
    ///
    /// # Errors
    /// Returns [`SoundError::Duration`], [`SoundError::Fade`] or
    /// [`SoundError::FadesOverlap`].
    pub fn check_envelope(
        rate: SampleRate,
        duration_seconds: f64,
        fade_seconds: f64,
    ) -> Result<(), SoundError> {
        Envelope::resolve(rate, duration_seconds, fade_seconds).map(|_| ())
    }

    /// Return the sample rate of the frames.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.target.sample_rate()
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
        self.sample_rate().seconds_in(self.envelope.frames)
    }

    /// Return the fade at each edge in seconds, for presentation.
    #[must_use]
    pub fn fade_seconds(&self) -> f64 {
        self.sample_rate().seconds_in(self.envelope.fade_frames)
    }

    /// Return each ear's summed amplitude bound in dBFS. It is not a measured
    /// or necessarily attained maximum. Silence is negative
    /// infinity.
    #[must_use]
    pub fn peak_dbfs(&self) -> [f64; 2] {
        self.target.peak_dbfs()
    }

    /// The validated voices, without the finite envelope.
    #[must_use]
    pub const fn target(&self) -> &Target {
        &self.target
    }

    /// Each voice's identity and validated facts, in order.
    pub fn voices(&self) -> impl Iterator<Item = (VoiceId, &VoiceFacts)> {
        self.target.voices()
    }

    /// Observe a generated frame's finite-source quantities, or None outside the signal.
    #[must_use]
    pub fn observation(&self, frame: u64) -> Option<crate::Observation> {
        if frame >= self.frames() {
            return None;
        }
        let gain = self.envelope.gain_at(frame);
        let mut voices = [None; crate::Observation::MAX_VOICES];
        for (slot, entry) in voices.iter_mut().zip(self.target.mix().entries()) {
            *slot = Some(crate::VoiceObservation {
                id: entry.id,
                role: crate::SlotRole::Continuing,
                facts: entry.facts,
                gains: entry.voice.gains().map(|level| level * gain),
            });
        }
        Some(crate::Observation {
            frame,
            active_revision: 1,
            settled_revision: 1,
            transitioning: false,
            voices,
        })
    }

    /// Evaluate one frame; the tail is silence.
    #[must_use]
    #[inline]
    pub fn frame(&self, n: u64) -> [f64; 2] {
        if n >= self.frames() {
            return [0.0; 2];
        }
        let mut out = [0.0; 2];
        for entry in self.target.mix().entries() {
            let voice = &entry.voice;
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
