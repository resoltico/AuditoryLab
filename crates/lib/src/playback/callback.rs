// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::source::Source;
use crate::playback::shared::{Shared, Terminal};
use cpal::{OutputStreamTimestamp, StreamInstant};
use std::sync::Arc;
use std::time::Duration;

/// Signal frames rendered at a time within one device buffer.
const SCRATCH_FRAMES: usize = 1_024;

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

/// Fills the backend's buffers from a signal and decides when the signal's
/// last frame has been played.
///
/// The backend states, for each buffer, when the callback runs and when the
/// buffer's first frame will be played. From the position of the signal's
/// end in the stream, each buffer therefore gives an estimate of when that
/// end is played. Estimates can only move the expected time later, and
/// playback is complete once a callback runs at or after it.
pub(crate) struct Callback {
    source: Source,
    monitor: Option<super::monitor::PlaybackMonitor>,
    progress: crate::Progress,
    shared: Arc<Shared>,
    scratch: Vec<[f64; 2]>,
    device_channels: usize,
    sample_rate_hz: u32,
    /// Frames returned to the backend by earlier callbacks.
    submitted: u64,
    /// The position in the stream, in frames, just past the signal's last frame.
    end: Option<u64>,
    /// The latest estimate of when the frame at `end` is played.
    target: Option<StreamInstant>,
    last_callback: Option<StreamInstant>,
}

impl Callback {
    pub(crate) fn new(
        source: impl Into<Source>,
        device_channels: u16,
        shared: Arc<Shared>,
    ) -> Self {
        let source = source.into();
        let sample_rate_hz = source.rate().hz();
        Self {
            source,
            monitor: None,
            progress: crate::Progress::default(),
            shared,
            scratch: vec![[0.0; 2]; SCRATCH_FRAMES],
            device_channels: usize::from(device_channels),
            sample_rate_hz,
            submitted: 0,
            end: None,
            target: None,
            last_callback: None,
        }
    }

    pub(crate) fn with_monitor(mut self, monitor: Option<super::monitor::PlaybackMonitor>) -> Self {
        self.monitor = monitor;
        self
    }
    pub(crate) fn with_progress(mut self, progress: crate::Progress) -> Self {
        self.progress = progress;
        self
    }
    #[cfg(test)]
    fn remaining(&self) -> u64 {
        self.source
            .frames()
            .unwrap_or(0)
            .saturating_sub(self.source.frame())
    }
    fn complete(&self) -> bool {
        self.source.finished()
    }

    /// Fill one backend buffer. A terminal result observed before or after
    /// rendering makes this whole buffer silent. A concurrent publication after
    /// those checks is observed by a later callback.
    pub(crate) fn fill<T: Copy>(
        &mut self,
        output: &mut [T],
        time: OutputStreamTimestamp,
        convert: &impl Fn(f64) -> T,
    ) {
        if self.shared.terminal() == Terminal::Running {
            self.produce(output, time, convert);
        }
        // Completion is only ever recorded for a buffer that already holds silence.
        if self.shared.terminal() != Terminal::Running {
            output.fill(convert(0.0));
        }
    }

    fn produce<T: Copy>(
        &mut self,
        output: &mut [T],
        time: OutputStreamTimestamp,
        convert: &impl Fn(f64) -> T,
    ) {
        if !output.len().is_multiple_of(self.device_channels) {
            return self.shared.finish(Terminal::MisalignedBuffer);
        }
        if output.is_empty() {
            return;
        }
        if time.playback < time.callback
            || self.last_callback.is_some_and(|last| time.callback < last)
        {
            return self.shared.finish(Terminal::InvalidTiming);
        }
        let advancing = self.last_callback.is_none_or(|last| time.callback > last);
        self.last_callback = Some(time.callback);

        let frames = (output.len() / self.device_channels) as u64;
        let latency = time.playback.duration_since(time.callback);
        // floor(cursor - latency*rate) = cursor - ceil(latency*rate).
        let latency_frames = u64::try_from(
            (latency.as_nanos() * u128::from(self.sample_rate_hz)).div_ceil(1_000_000_000),
        )
        .unwrap_or(u64::MAX);
        self.progress
            .set(self.source.frame().saturating_sub(latency_frames));
        self.source.begin_buffer(self.shared.stop_requested());
        let active = self.render(output, convert);
        self.shared.set_frames(self.source.frame());
        if active > 0 {
            self.shared.mark_progress();
        }
        if self.end.is_none() && self.complete() {
            self.end = Some(self.submitted + active);
        }
        if let Some(end) = self.end {
            let estimate = if end >= self.submitted {
                let ahead = frame_duration(end - self.submitted, self.sample_rate_hz, Rounding::Up);
                match time.playback.checked_add(ahead) {
                    Some(estimate) => Some(estimate),
                    None => return self.shared.finish(Terminal::InvalidTiming),
                }
            } else {
                // After an underrun the backend can place this buffer so early that the end
                // would precede the stream's origin; such an estimate says nothing new.
                let behind =
                    frame_duration(self.submitted - end, self.sample_rate_hz, Rounding::Down);
                time.playback.checked_sub(behind)
            };
            self.target = self.target.max(estimate);
            if active == 0 {
                if advancing {
                    self.shared.mark_progress();
                }
                if self.target.is_some_and(|target| time.callback >= target) {
                    self.shared.finish(if self.source.cancelled() {
                        Terminal::Cancelled
                    } else {
                        Terminal::Complete
                    });
                }
            }
        }
        self.submitted += frames;
    }

    /// Render the next signal frames and place them on the device's channels.
    fn render<T: Copy>(&mut self, output: &mut [T], convert: &impl Fn(f64) -> T) -> u64 {
        let mut publication = self.monitor.as_ref().and_then(|m| m.try_publish());
        let mut active = 0;
        for chunk in output.chunks_mut(SCRATCH_FRAMES * self.device_channels) {
            let frames = chunk.len() / self.device_channels;
            let scratch = &mut self.scratch[..frames];
            let start = self.source.frame();
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
            publication.finish(
                self.source.frame(),
                self.source.observation(),
                self.source.finished(),
            );
        }
        active
    }
}

/// Put left/right on the leading channels and silence on the rest.
fn place<T>(device_frame: &mut [T], signal_frame: &[f64; 2], convert: &impl Fn(f64) -> T) {
    for (channel, slot) in device_frame.iter_mut().enumerate() {
        *slot = convert(signal_frame.get(channel).copied().unwrap_or(0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auditorylab_core::{Component, Ears, SampleRate, Signal, Stimulus};
    use proptest::{prelude::*, test_runner::TestRunner};

    fn rate(hz: u32) -> SampleRate {
        SampleRate::new(hz).unwrap()
    }

    fn signal(frames: u64, hz: u32) -> Signal {
        Stimulus {
            duration_seconds: f64::from(u32::try_from(frames).unwrap()) / f64::from(hz),
            fade_seconds: 0.0,
            components: vec![Component {
                ears: Ears::Both,
                peak_dbfs: -20.0,
                modulation: None,
                carrier: crate::Carrier::Tone {
                    frequency_hz: 100.0,
                },
            }],
        }
        .signal(rate(hz))
        .unwrap()
    }
    fn callback(frames: u64, hz: u32) -> Callback {
        Callback::new(signal(frames, hz), 2, Arc::default())
    }

    fn time(callback: u64, playback: u64) -> OutputStreamTimestamp {
        OutputStreamTimestamp {
            callback: StreamInstant::from_millis(callback),
            playback: StreamInstant::from_millis(playback),
        }
    }

    fn nanos(callback: u64, playback: u64) -> OutputStreamTimestamp {
        OutputStreamTimestamp {
            callback: StreamInstant::from_nanos(callback),
            playback: StreamInstant::from_nanos(playback),
        }
    }

    fn fill(callback: &mut Callback, frames: usize, at: OutputStreamTimestamp) -> Vec<f64> {
        let mut output = vec![1.0; frames * callback.device_channels];
        callback.fill(&mut output, at, &|sample| sample);
        output
    }

    fn running(callback: &Callback) -> bool {
        callback.shared.terminal() == Terminal::Running
    }

    fn complete(callback: &Callback) -> bool {
        callback.shared.terminal() == Terminal::Complete
    }

    fn terminal_is(callback: &Callback, expected: &Terminal) -> bool {
        callback.shared.terminal() == *expected
    }

    #[test]
    fn progress_subtracts_device_latency_from_the_buffer_start() {
        let progress = crate::Progress::default();
        let mut callback = callback(8000, 8000).with_progress(progress.clone());
        fill(&mut callback, 800, time(0, 50));
        assert_eq!(progress.frames(), 0);
        fill(&mut callback, 800, time(100, 150));
        assert_eq!(progress.frames(), 400);
        fill(&mut callback, 800, time(200, 200));
        assert_eq!(progress.frames(), 1600);
    }

    #[test]
    fn fractional_latency_never_reports_a_frame_before_it_is_audible() {
        let progress = crate::Progress::default();
        let mut callback = callback(8000, 8000).with_progress(progress.clone());
        fill(&mut callback, 800, nanos(0, 1));
        assert_eq!(progress.frames(), 0);
        fill(&mut callback, 800, nanos(100_000_000, 100_000_001));
        assert_eq!(progress.frames(), 799);
        fill(&mut callback, 800, nanos(200_000_000, 200_125_001));
        assert_eq!(progress.frames(), 1598);
    }

    #[test]
    fn a_stereo_signal_ending_in_the_second_scratch_block_drains_without_rounding_early() {
        let signal = signal(1_537, 44_100);
        let mut callback = Callback::new(signal, 2, Arc::default());
        let output = fill(&mut callback, 2_048, nanos(0, 10_000_000));
        assert_eq!(callback.end, Some(1_537));
        // 10 ms + ceil(1537 * 1e9 / 44100), independently calculated.
        let target = 44_852_608;
        assert_eq!(callback.target, Some(StreamInstant::from_nanos(target)));
        assert!(output[2_048..3_074].iter().any(|sample| *sample != 0.0));
        assert!(output[3_074..].iter().all(|sample| *sample == 0.0));
        assert!(running(&callback));
        // Subtract floor(511 * 1e9 / 44100); the end cannot move earlier.
        fill(&mut callback, 256, nanos(target - 1, target + 11_587_301));
        assert!(running(&callback));
        assert_eq!(callback.target, Some(StreamInstant::from_nanos(target)));
        // Now 767 frames have been submitted past the end.
        let silence = fill(&mut callback, 256, nanos(target, target + 17_392_290));
        assert!(complete(&callback));
        assert!(silence.iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn frame_durations_round_to_the_requested_side_of_a_nanosecond() {
        assert_eq!(
            frame_duration(1, 3, Rounding::Up),
            Duration::from_nanos(333_333_334)
        );
        assert_eq!(
            frame_duration(1, 3, Rounding::Down),
            Duration::from_nanos(333_333_333)
        );
        for rounding in [Rounding::Down, Rounding::Up] {
            assert_eq!(frame_duration(0, 48_000, rounding), Duration::ZERO);
            assert_eq!(
                frame_duration(96_000, 48_000, rounding),
                Duration::from_secs(2)
            );
            assert_eq!(
                frame_duration(72_000, 48_000, rounding),
                Duration::from_millis(1_500)
            );
        }
        // The longest signal at the lowest and at the highest sample rate.
        assert_eq!(
            frame_duration(1 << 36, 1, Rounding::Up),
            Duration::from_secs(1 << 36)
        );
        assert_eq!(
            frame_duration(1 << 36, 768_000, Rounding::Up),
            Duration::new(89_478, 485_333_334)
        );
    }

    #[test]
    fn the_signal_is_rendered_in_order_across_buffers_and_followed_by_silence() {
        let mut expected = [[0.0; 2]; 3_000];
        signal(3_000, 8_000).render(0, &mut expected);
        let mut c = callback(3_000, 8_000);
        // The first buffer spans more than two scratch blocks.
        let mut actual = fill(&mut c, 2_500, time(0, 10));
        actual.extend(fill(&mut c, 700, time(300, 320)));
        assert_eq!(
            actual[..6_000],
            expected.into_iter().flatten().collect::<Vec<_>>()
        );
        assert_eq!(actual[6_000..], [0.0; 400]);
        assert!(actual[..3_000].iter().any(|sample| sample.abs() > 0.09));
        assert_eq!(c.end, Some(3_000));
        assert_eq!(c.submitted, 3_200);
        assert!(running(&c));
    }

    #[test]
    fn a_huge_final_buffer_is_not_completion_and_the_end_is_placed_within_it() {
        let mut c = callback(2_050, 8_000);
        fill(&mut c, 8_000, time(0, 1_000));
        assert!(running(&c));
        // 1 s, plus 2050 frames at 8000 Hz.
        assert_eq!(c.target, Some(StreamInstant::from_nanos(1_256_250_000)));
        fill(&mut c, 8, time(1_000, 2_000));
        assert!(running(&c));
        assert_eq!(c.target, Some(StreamInstant::from_nanos(1_256_250_000)));
        fill(&mut c, 8, time(1_500, 2_001));
        assert!(complete(&c));
    }

    #[test]
    fn a_signal_that_ends_exactly_at_a_buffer_boundary_completes_in_a_later_buffer() {
        // A steady stream: an 8-frame buffer every millisecond, played 5 ms later.
        let mut c = callback(16, 8_000);
        fill(&mut c, 8, time(0, 5));
        assert_eq!(c.end, None);
        fill(&mut c, 8, time(1, 6));
        assert_eq!(c.end, Some(16));
        // That buffer's first frame is played at 6 ms and its eight frames take 1 ms.
        let target = Some(StreamInstant::from_millis(7));
        assert_eq!(c.target, target);
        for millisecond in 2..7 {
            fill(&mut c, 8, time(millisecond, millisecond + 5));
            assert_eq!(c.target, target);
            assert!(running(&c), "complete at {millisecond} ms");
        }
        fill(&mut c, 8, time(7, 12));
        assert!(complete(&c));
    }

    #[test]
    fn the_expected_end_rounds_a_fractional_frame_time_forward() {
        let mut c = callback(1, 8_001);
        fill(&mut c, 1, time(0, 0));
        assert_eq!(c.target, Some(StreamInstant::from_nanos(124_985)));
    }

    #[test]
    fn a_later_estimate_rounds_the_silence_since_the_end_down() {
        let mut c = callback(1, 8_001);
        fill(&mut c, 2, time(0, 0));
        assert_eq!(c.target, Some(StreamInstant::from_nanos(124_985)));
        // One silent frame was submitted after the end: 124984.37 ns, counted as 124984.
        fill(&mut c, 1, nanos(1_000_000_000, 1_000_124_985));
        assert_eq!(c.target, Some(StreamInstant::from_nanos(1_000_000_001)));
        assert!(running(&c));
        fill(&mut c, 1, nanos(1_000_000_001, 1_000_124_985));
        assert!(complete(&c));
    }

    #[test]
    fn buffers_queued_before_the_device_starts_keep_the_end_in_the_future() {
        // The device has not started: every buffer joins a queue that is not consumed.
        let mut c = callback(8, 8_000);
        let mut submitted_frames = 0;
        for callback_ms in [0, 1_000, 5_000, 15_000] {
            let playback_ms = callback_ms + submitted_frames / 8;
            fill(&mut c, 8, time(callback_ms, playback_ms));
            submitted_frames += 8;
            assert!(running(&c));
            assert_eq!(c.target, Some(StreamInstant::from_millis(callback_ms + 1)));
        }
        // The device runs and has consumed everything submitted so far.
        fill(&mut c, 8, time(15_010, 15_010));
        assert!(complete(&c));
    }

    #[test]
    fn an_estimate_before_the_stream_origin_or_an_earlier_one_keeps_the_expected_end() {
        let mut c = callback(1, 8_000);
        fill(&mut c, 8_000, time(0, 0));
        let target = c.target;
        assert_eq!(target, Some(StreamInstant::from_nanos(125_000)));
        // The end would lie a second before the origin.
        fill(&mut c, 8, time(0, 0));
        assert_eq!(c.target, target);
        assert!(running(&c));

        let mut c = callback(8, 8_000);
        fill(&mut c, 8, time(0, 1_000));
        let target = c.target;
        // The backend revises its prediction downwards.
        fill(&mut c, 8, time(100, 100));
        assert_eq!(c.target, target);
        assert!(running(&c));
        fill(&mut c, 8, time(1_001, 1_001));
        assert_eq!(c.target, target);
        assert!(complete(&c));
    }

    #[test]
    fn clocks_that_run_backwards_or_beyond_their_range_end_playback() {
        // Playback before its own callback, and a callback before the previous one.
        for at in [time(4, 3), time(2, 2)] {
            let mut c = callback(100, 8_000);
            assert_eq!(fill(&mut c, 1, time(3, 3)), [0.0; 2]);
            assert!(running(&c));
            assert_eq!(fill(&mut c, 1, at), [0.0; 2]);
            assert!(terminal_is(&c, &Terminal::InvalidTiming));
            assert_eq!(c.remaining(), 99);
        }
        // An equal callback time is not a regression.
        let mut c = callback(100, 8_000);
        fill(&mut c, 1, time(3, 3));
        fill(&mut c, 1, time(3, 3));
        assert!(running(&c));
        // The end cannot be placed beyond the clock's range.
        let mut c = callback(1, 8_000);
        let last = StreamInstant::new(u64::MAX, 999_999_999);
        let output = fill(
            &mut c,
            1,
            OutputStreamTimestamp {
                callback: last,
                playback: last,
            },
        );
        assert!(terminal_is(&c, &Terminal::InvalidTiming));
        assert_eq!(output, [0.0; 2]);
    }

    #[test]
    fn empty_buffers_establish_neither_progress_nor_completion_nor_a_clock() {
        let mut c = callback(1, 8_000);
        fill(&mut c, 1, time(5, 5));
        let progress = c.shared.progress();
        fill(&mut c, 0, time(1_000, 1_000));
        assert_eq!(c.shared.progress(), progress);
        assert!(running(&c));
        // The empty buffer's later time did not become the clock to compare with.
        fill(&mut c, 1, time(6, 6));
        assert!(complete(&c));
    }

    #[test]
    fn progress_is_rendered_frames_and_then_an_advancing_clock() {
        let mut active = callback(80_000, 8_000);
        fill(&mut active, 0, time(0, 0));
        fill(&mut active, 0, time(15_000, 15_000));
        assert_eq!(active.shared.progress(), 0);
        fill(&mut active, 8, time(0, 0));
        assert_eq!(active.shared.progress(), 1);
        // A frozen clock still rendered signal frames.
        fill(&mut active, 8, time(0, 0));
        assert_eq!(active.shared.progress(), 2);

        let mut draining = callback(8, 8_000);
        fill(&mut draining, 8, time(0, 60_000));
        assert_eq!(draining.shared.progress(), 1);
        fill(&mut draining, 8, time(0, 60_000));
        fill(&mut draining, 0, time(30_000, 60_000));
        assert_eq!(draining.shared.progress(), 1);
        fill(&mut draining, 8, time(1, 60_001));
        assert_eq!(draining.shared.progress(), 2);
        assert!(running(&draining));
    }

    #[test]
    fn a_terminal_publication_during_a_buffer_silences_it_and_rendering_never_resumes() {
        for terminal in [
            Terminal::Cancelled,
            Terminal::TimedOut,
            Terminal::Backend,
            Terminal::Complete,
            Terminal::InvalidTiming,
            Terminal::MisalignedBuffer,
            Terminal::StreamSetupFailed,
        ] {
            let shared = Arc::new(Shared::default());
            let mut c = Callback::new(signal(8_000, 8_000), 2, Arc::clone(&shared));
            let calls = std::cell::Cell::new(0);
            let mut output = vec![1.0; 8_192];
            c.fill(&mut output, time(0, 0), &|value| {
                calls.set(calls.get() + 1);
                if calls.get() == 2_050 {
                    shared.finish(terminal);
                }
                value
            });
            assert!(output.iter().all(|s| *s == 0.0));
            assert!(terminal_is(&c, &terminal));
            let remaining = c.remaining();
            assert_eq!(fill(&mut c, 4_096, time(1, 1)), vec![0.0; 8_192]);
            assert_eq!(c.remaining(), remaining);
        }
    }

    #[test]
    fn a_failure_while_the_last_silence_is_written_precedes_completion() {
        let mut c = callback(1, 8_000);
        fill(&mut c, 1, time(0, 0));
        let shared = Arc::clone(&c.shared);
        let calls = std::cell::Cell::new(0);
        let mut output = [1.0; 2];
        // This buffer would complete playback; the failure lands as it is converted.
        c.fill(&mut output, time(1, 1), &|sample| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                shared.finish(Terminal::Cancelled);
                // A marked value, to show that the whole buffer is silenced afterwards.
                0.25
            } else {
                sample
            }
        });
        assert!(terminal_is(&c, &Terminal::Cancelled));
        assert_eq!(output, [0.0; 2]);
    }

    #[test]
    fn a_buffer_holding_a_partial_frame_is_silenced_and_ends_playback() {
        let signal = signal(8, 8_000);
        let mut c = Callback::new(signal, 2, Arc::default());
        let mut partial = [1.0; 3];
        c.fill(&mut partial, time(0, 0), &|sample| sample);
        assert_eq!(partial, [0.0; 3]);
        assert!(terminal_is(&c, &Terminal::MisalignedBuffer));
        assert_eq!(c.remaining(), 8);
        let mut aligned = [1.0; 4];
        c.fill(&mut aligned, time(1, 1), &|sample| sample);
        assert_eq!(aligned, [0.0; 4]);
        assert_eq!(c.remaining(), 8);
    }

    #[test]
    fn finite_stop_fades_then_waits_for_the_device_to_drain() {
        let mut c = callback(1_000, 8_000);
        fill(&mut c, 32, time(0, 0));
        c.shared.request_stop();
        let out = fill(&mut c, 512, time(4, 54));
        assert!(out[..478].iter().any(|s| *s != 0.0));
        assert!(out[478..].iter().all(|s| *s == 0.0));
        assert_eq!(c.end, Some(272));
        assert_eq!(c.shared.frames(), 272);
        assert!(running(&c));
        fill(&mut c, 8, time(83, 114));
        assert!(running(&c));
        fill(&mut c, 8, time(84, 115));
        assert!(terminal_is(&c, &Terminal::Cancelled));
        assert!(matches!(
            crate::playback::output::resolve(&c.shared, c.shared.terminal(), None),
            Err(crate::playback::PlaybackError::Cancelled)
        ));
    }
    #[test]
    fn the_finite_envelope_matches_an_independent_ramp_across_chunks_and_buffers() {
        for (total, before, rate_hz) in [(10_000, 500, 48_000), (640, 600, 8_000)] {
            let reference = signal(total, rate_hz);
            let mut c = callback(total, rate_hz);
            fill(&mut c, usize::try_from(before).unwrap(), time(0, 0));
            c.shared.request_stop();
            let ramp = (u64::from(rate_hz) * 3 / 100).max(1).min(total - before);
            let mut actual = fill(&mut c, 1_100, time(100, 150));
            actual.extend(fill(&mut c, 512, time(120, 180)));
            for (t, frame) in actual.as_chunks::<2>().0.iter().enumerate() {
                let expected = if (t as u64) < ramp {
                    reference.frame(before + t as u64).map(|v| {
                        v * (1.0
                            - f64::from(u32::try_from(t + 1).unwrap())
                                / f64::from(u32::try_from(ramp).unwrap()))
                    })
                } else {
                    [0.0; 2]
                };
                assert_eq!(*frame, expected, "frame {t}");
            }
        }
    }

    #[test]
    fn a_finite_stop_uses_only_the_remaining_frames() {
        for remaining in [1, 40] {
            let mut c = callback(20 + remaining, 8_000);
            fill(&mut c, 20, time(0, 0));
            c.shared.request_stop();
            let out = fill(&mut c, 64, time(3, 3));
            let last = usize::try_from(remaining - 1).unwrap() * 2;
            assert!(out[last..].iter().all(|s| *s == 0.0));
            assert_eq!(c.end, Some(20 + remaining));
        }
    }
    fn live_callback() -> (Callback, crate::playback::source::Mailbox) {
        let mailbox = Arc::new(std::sync::Mutex::new(
            super::super::source::Updates::default(),
        ));
        mailbox.lock().unwrap().pending =
            Some((1, auditorylab_core::live::Target::silence(rate(8_000))));
        let source = Source::Live(Box::new(crate::LiveGenerator {
            engine: auditorylab_core::live::Engine::new(rate(8_000)),
            mailbox: mailbox.clone(),
        }));
        (Callback::new(source, 2, Arc::default()), mailbox)
    }
    #[test]
    fn mailbox_is_polled_once_per_buffer_and_updates_wait_for_the_next_buffer() {
        let (mut c, mailbox) = live_callback();
        let target = auditorylab_core::live::Target::new(
            rate(8_000),
            &[(
                1,
                Component {
                    ears: Ears::Both,
                    peak_dbfs: -30.0,
                    carrier: crate::Carrier::Tone {
                        frequency_hz: 440.0,
                    },
                    modulation: None,
                },
            )],
        )
        .unwrap();
        let mut output = [1.0; 4_096];
        let calls = std::cell::Cell::new(0);
        c.fill(&mut output, time(0, 0), &|v| {
            if calls.replace(calls.get() + 1) == 0 {
                mailbox.lock().unwrap().pending = Some((2, target));
            }
            v
        });
        assert!(output.iter().all(|v| *v == 0.0));
        let out = fill(&mut c, 512, time(256, 256));
        assert!(out.iter().any(|v| *v != 0.0));
    }
    #[test]
    fn live_stop_completes_and_reports_the_rendered_frame_count() {
        let (mut c, _) = live_callback();
        fill(&mut c, 512, time(0, 0));
        c.shared.request_stop();
        let out = fill(&mut c, 512, time(100, 100));
        assert!(out.iter().all(|v| *v == 0.0));
        assert_eq!(c.shared.frames(), 752);
        assert!(running(&c));
        fill(&mut c, 512, time(131, 131));
        assert!(complete(&c));
        assert!(crate::playback::output::resolve(&c.shared, c.shared.terminal(), None).is_ok());
    }
    #[test]
    fn live_callbacks_do_not_allocate_or_wait_for_a_busy_or_poisoned_mailbox() {
        let (mut c, mailbox) = live_callback();
        let guard = mailbox.lock().unwrap();
        let mut out = [1.0; 1_024];
        let measured = allocation_counter::measure(|| c.fill(&mut out, time(0, 0), &|v| v));
        assert_eq!(measured.count_total, 0);
        assert!(out.iter().all(|v| *v == 0.0));
        drop(guard);
        let measured = allocation_counter::measure(|| {
            c.fill(&mut out, time(64, 64), &|v| v);
            c.shared.request_stop();
            c.fill(&mut out, time(128, 128), &|v| v);
        });
        assert_eq!(measured.count_total, 0);
        let (mut c, mailbox) = live_callback();
        assert!(
            std::thread::spawn(move || {
                let _guard = mailbox.lock().unwrap();
                panic!("injected poisoned control");
            })
            .join()
            .is_err()
        );
        let measured = allocation_counter::measure(|| c.fill(&mut out, time(0, 0), &|v| v));
        assert_eq!(measured.count_total, 0);
        assert!(out.iter().all(|v| *v == 0.0));
    }

    fn routed(frames: u32) -> Signal {
        Stimulus {
            duration_seconds: f64::from(frames) / 8_000.0,
            fade_seconds: 0.0,
            components: vec![
                Component {
                    ears: Ears::Left,
                    peak_dbfs: -20.0,
                    modulation: None,
                    carrier: crate::Carrier::Tone {
                        frequency_hz: 257.3,
                    },
                },
                Component {
                    ears: Ears::Right,
                    peak_dbfs: -14.0,
                    modulation: None,
                    carrier: crate::Carrier::Click {
                        rate_hz: 37.0,
                        width_seconds: 0.001,
                    },
                },
            ],
        }
        .signal(rate(8_000))
        .unwrap()
    }
    #[test]
    fn stereo_routes_are_placed_and_timed_by_frames_on_wide_devices() {
        let mut expected = vec![[0.0; 2]; 3_000];
        routed(3_000).render(0, &mut expected);
        for channels in [2, 3, 5] {
            let mut c = Callback::new(routed(3_000), channels, Arc::default());
            let mut output = fill(&mut c, 2_500, time(0, 10));
            assert_eq!(c.submitted, 2_500);
            assert_eq!(c.end, None);
            output.extend(fill(&mut c, 700, time(300, 320)));
            assert_eq!(c.submitted, 3_200);
            assert_eq!(c.end, Some(3_000));
            assert_eq!(c.target, Some(StreamInstant::from_micros(382_500)));
            for (index, frame) in output.chunks_exact(usize::from(channels)).enumerate() {
                assert_eq!(
                    &frame[..2],
                    &expected.get(index).copied().unwrap_or([0.0; 2])
                );
                assert!(frame[2..].iter().all(|s| *s == 0.0));
            }
            assert!(expected.iter().any(|f| f[0] != f[1]));
            assert!(running(&c));
        }
    }

    /// One backend buffer: frames, callback advance and queue delay in microseconds.
    type Step = (usize, u64, u64);

    #[test]
    fn any_sequence_of_buffers_plays_the_signal_once_and_completes_only_after_its_end() {
        let config = proptest::test_runner::Config {
            cases: 256,
            max_shrink_iters: 1_024,
            max_shrink_time: 1_000,
            failure_persistence: None,
            ..proptest::test_runner::Config::default()
        };
        let steps =
            proptest::collection::vec((0usize..=3_000, 0u64..=400_000, 0u64..=900_000), 1..=40);
        TestRunner::new(config)
            .run(
                &(1u64..=6_000, steps),
                |(frames, steps): (u64, Vec<Step>)| {
                    let mut c = callback(frames, 8_000);
                    let mut reference = vec![[0.0; 2]; usize::try_from(frames).unwrap()];
                    signal(frames, 8_000).render(0, &mut reference);
                    let expected: Vec<_> = reference.into_iter().flatten().collect();
                    let mut played = Vec::new();
                    let mut clock = 0;
                    for (buffer_frames, advance, delay) in steps {
                        clock += advance;
                        let was_complete = complete(&c);
                        let at = OutputStreamTimestamp {
                            callback: StreamInstant::from_micros(clock),
                            playback: StreamInstant::from_micros(clock + delay),
                        };
                        let output = fill(&mut c, buffer_frames, at);
                        prop_assert!(matches!(
                            c.shared.terminal(),
                            Terminal::Running | Terminal::Complete
                        ));
                        if was_complete {
                            prop_assert!(output.iter().all(|sample| *sample == 0.0));
                        } else {
                            played.extend(output);
                        }
                        if complete(&c) {
                            // Completion needs the whole signal rendered, and a callback at or
                            // after the instant the backend first gave for its end.
                            prop_assert!(c.complete());
                            prop_assert!(c.target.is_some_and(|target| at.callback >= target));
                        }
                    }
                    let rendered = played.len().min(expected.len());
                    prop_assert_eq!(&played[..rendered], &expected[..rendered]);
                    prop_assert!(played[rendered..].iter().all(|sample| *sample == 0.0));
                    Ok(())
                },
            )
            .unwrap();
    }

    #[test]
    fn callback_paths_allocate_nothing_including_terminal_buffers() {
        for channels in [2, 3, 5] {
            for hz in [44_100, 48_000] {
                let mut c = Callback::new(signal(2053, hz), channels, Arc::default());
                let mut buffer = vec![1.0; usize::from(channels) * 4099];
                let count = allocation_counter::measure(|| {
                    c.fill(&mut [], time(0, 10), &|x| x);
                    c.fill(&mut buffer, time(0, 10), &|x| x);
                    c.fill(&mut buffer, time(100, 110), &|x| x);
                    c.fill(&mut buffer, time(200, 210), &|x| x);
                });
                assert_eq!(count.count_total, 0);
                assert_eq!(count.bytes_total, 0);
                assert!(c.shared.terminal() != Terminal::Running);
                assert!(buffer.iter().all(|x| *x == 0.0));
            }
        }
        for case in 0..3 {
            let mut callback = callback(10, 48_000);
            let mut buffer = [1.0; 18];
            if case == 0 {
                callback.shared.finish(Terminal::Cancelled);
            }
            if case == 2 {
                callback.device_channels = 5;
            }
            let count = allocation_counter::measure(|| {
                callback.fill(
                    &mut buffer,
                    if case == 1 { time(2, 1) } else { time(0, 1) },
                    &|x| x,
                );
            });
            assert_eq!(count.count_total, 0);
            assert_eq!(count.bytes_total, 0);
            assert!(callback.shared.terminal() != Terminal::Running);
            assert_eq!(buffer, [0.0; 18]);
        }
    }

    #[test]
    fn every_terminal_code_silences_without_rendering_or_allocating() {
        for terminal in [
            Terminal::Complete,
            Terminal::Cancelled,
            Terminal::TimedOut,
            Terminal::InvalidTiming,
            Terminal::MisalignedBuffer,
            Terminal::Backend,
            Terminal::StreamSetupFailed,
        ] {
            let mut callback = callback(10, 48_000);
            let remaining = callback.remaining();
            callback.shared.finish(terminal);
            let mut buffer = [1.0; 18];
            let count = allocation_counter::measure(|| {
                callback.fill(&mut buffer, time(0, 1), &|sample| sample)
            });
            assert_eq!(count.count_total, 0);
            assert_eq!(count.bytes_total, 0);
            assert_eq!(callback.remaining(), remaining);
            assert_eq!(buffer, [0.0; 18]);
        }
    }

    #[test]
    fn callback_measurement_detects_allocating_conversion() {
        let mut c = callback(10, 48_000);
        let mut buffer = [0.0; 40];
        let count = allocation_counter::measure(|| {
            c.fill(&mut buffer, time(0, 1), &|x| {
                std::hint::black_box(vec![0.5; 2]);
                x
            })
        });
        assert!(count.count_total > 0);
        assert!(count.bytes_total > 0);
    }
}
