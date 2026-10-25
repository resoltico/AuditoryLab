// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Filling backend buffers from a source, estimating output position and
//! deciding when queued output has drained.
//!
//! Three positions are distinct: G, source frames generated; S, backend
//! frames submitted by earlier callbacks, including drain silence; and P,
//! the published estimate of source frames output. Generation ending freezes
//! G while S keeps advancing through silence. The backend states, for each
//! buffer, when the callback runs and when its first frame plays. Time is
//! measured from the stream's own origin.
use super::monitor::PlaybackMonitor;
use super::source::Source;
use super::{Outcome, SessionState};
use std::sync::Arc;
use std::time::Duration;

/// Signal frames rendered at a time within one device buffer.
const SCRATCH_FRAMES: usize = 1_024;

/// When a callback runs and when its buffer's first frame is played, both
/// measured from the stream's origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BufferTime {
    pub(crate) callback: Duration,
    pub(crate) playback: Duration,
}

#[derive(Clone, Copy)]
pub(crate) enum Rounding {
    Down,
    Up,
}

/// The time `frames` take at `rate_hz`, as whole nanoseconds.
pub(crate) fn frame_duration(frames: u64, rate_hz: u32, rounding: Rounding) -> Duration {
    let rate = u64::from(rate_hz);
    let scaled = (frames % rate) * 1_000_000_000;
    let round_up = matches!(rounding, Rounding::Up) && !scaled.is_multiple_of(rate);
    Duration::from_secs(frames / rate) + Duration::from_nanos(scaled / rate + u64::from(round_up))
}

/// One stream's buffer filling, progress estimate and drain decision.
pub(crate) struct Transport {
    source: Source,
    monitor: Option<PlaybackMonitor>,
    state: Arc<SessionState>,
    scratch: Vec<[f64; 2]>,
    device_channels: usize,
    rate_hz: u32,
    /// S: frames returned to the backend by earlier callbacks.
    submitted: u64,
    /// P: the published estimate of source frames output.
    estimate: u64,
    /// E: the stream position just past the last generated frame.
    end: Option<u64>,
    /// The latest conservative estimate of when the frame at E plays.
    deadline: Option<Duration>,
    last_callback: Option<Duration>,
}

impl Transport {
    pub(crate) fn new(source: Source, device_channels: u16, state: Arc<SessionState>) -> Self {
        let rate_hz = source.rate().hz();
        Self {
            source,
            monitor: None,
            state,
            scratch: vec![[0.0; 2]; SCRATCH_FRAMES],
            device_channels: usize::from(device_channels),
            rate_hz,
            submitted: 0,
            estimate: 0,
            end: None,
            deadline: None,
            last_callback: None,
        }
    }

    pub(crate) fn with_monitor(mut self, monitor: Option<PlaybackMonitor>) -> Self {
        self.monitor = monitor;
        self
    }

    /// Fill one backend buffer. A terminal outcome observed before or after
    /// rendering makes the whole buffer silent; a publication after those
    /// checks is observed by a later callback.
    pub(crate) fn fill<T: Copy>(
        &mut self,
        output: &mut [T],
        time: BufferTime,
        convert: &impl Fn(f64) -> T,
    ) {
        if self.state.outcome().is_active() {
            self.produce(output, time, convert);
        }
        // Completion is only ever recorded for a buffer that already holds silence.
        if !self.state.outcome().is_active() {
            output.fill(convert(0.0));
        }
    }

    fn produce<T: Copy>(
        &mut self,
        output: &mut [T],
        time: BufferTime,
        convert: &impl Fn(f64) -> T,
    ) {
        if !output.len().is_multiple_of(self.device_channels) {
            self.state.finish(Outcome::MisalignedBuffer);
            return;
        }
        // An empty buffer establishes no clock, progress, generation or completion.
        if output.is_empty() {
            return;
        }
        if time.playback < time.callback
            || self.last_callback.is_some_and(|last| time.callback < last)
        {
            self.state.finish(Outcome::InvalidTiming);
            return;
        }
        let advancing = self.last_callback.is_none_or(|last| time.callback > last);
        self.last_callback = Some(time.callback);

        let buffer_frames = (output.len() / self.device_channels) as u64;
        let generated = self.source.generated();
        // floor(S - latency*rate) = S - ceil(latency*rate), bounded by G and
        // never moving backwards.
        let latency = time.playback - time.callback;
        let latency_frames =
            u64::try_from((latency.as_nanos() * u128::from(self.rate_hz)).div_ceil(1_000_000_000))
                .unwrap_or(u64::MAX);
        self.estimate = self
            .estimate
            .max(generated.min(self.submitted.saturating_sub(latency_frames)));
        self.state.set_estimated_output(self.estimate);

        let stop = self.state.outcome() == Outcome::StopRequested;
        self.source.begin_buffer(stop);
        let active = self.render(output, convert);
        self.state.set_generated(self.source.generated());
        if self.source.ended() {
            self.state.end_source();
        }
        if active > 0 {
            self.state.mark_progress();
        }
        if self.end.is_none() && self.source.ended() {
            self.end = Some(self.submitted + active);
        }
        if let Some(end) = self.end {
            let candidate = if end >= self.submitted {
                let ahead = frame_duration(end - self.submitted, self.rate_hz, Rounding::Up);
                match time.playback.checked_add(ahead) {
                    Some(candidate) => Some(candidate),
                    None => {
                        self.state.finish(Outcome::InvalidTiming);
                        return;
                    }
                }
            } else {
                // After an underrun the backend can place this buffer so early that the
                // end would precede the stream's origin; such an estimate says nothing new.
                let behind = frame_duration(self.submitted - end, self.rate_hz, Rounding::Down);
                time.playback.checked_sub(behind)
            };
            self.deadline = self.deadline.max(candidate);
            if active == 0 {
                if advancing {
                    self.state.mark_progress();
                }
                if self
                    .deadline
                    .is_some_and(|deadline| time.callback >= deadline)
                {
                    // Final progress precedes the terminal Release publication.
                    self.estimate = self.source.generated();
                    self.state.set_estimated_output(self.estimate);
                    self.state.finish_drained(if self.source.is_live() {
                        Outcome::FrameLimit
                    } else {
                        Outcome::Completed
                    });
                }
            }
        }
        self.submitted += buffer_frames;
    }

    /// Render the next source frames and place them on the device's channels.
    fn render<T: Copy>(&mut self, output: &mut [T], convert: &impl Fn(f64) -> T) -> u64 {
        let mut publication = self.monitor.as_ref().and_then(PlaybackMonitor::try_publish);
        let mut active = 0;
        for chunk in output.chunks_mut(SCRATCH_FRAMES * self.device_channels) {
            let frames = chunk.len() / self.device_channels;
            let scratch = &mut self.scratch[..frames];
            let start = self.source.generated();
            let count = self.source.render(scratch);
            if let Some(publication) = publication.as_mut() {
                publication.append(start, &scratch[..count]);
            }
            active += count as u64;
            for (device_frame, signal_frame) in chunk
                .chunks_exact_mut(self.device_channels)
                .zip(scratch.iter())
            {
                place(device_frame, signal_frame, convert);
            }
        }
        if let Some(publication) = publication.as_mut() {
            publication.finish(self.source.generated(), self.source.observation());
        }
        active
    }

    #[cfg(test)]
    pub(crate) fn diagnostic_observation(&self) -> Option<auditorylab_core::Observation> {
        self.source.observation()
    }

    #[cfg(test)]
    pub(crate) const fn submitted(&self) -> u64 {
        self.submitted
    }

    #[cfg(test)]
    pub(crate) const fn end(&self) -> Option<u64> {
        self.end
    }

    #[cfg(test)]
    pub(crate) const fn deadline(&self) -> Option<Duration> {
        self.deadline
    }
}

/// Put left/right on the leading channels and silence on the rest.
fn place<T>(device_frame: &mut [T], signal_frame: &[f64; 2], convert: &impl Fn(f64) -> T) {
    for (channel, slot) in device_frame.iter_mut().enumerate() {
        *slot = convert(signal_frame.get(channel).copied().unwrap_or(0.0));
    }
}

#[cfg(test)]
mod tests;
