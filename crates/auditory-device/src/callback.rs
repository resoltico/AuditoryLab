// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::{PlaybackError, shared::Shared};
use auditory_core::Renderer;
use cpal::{OutputStreamTimestamp, StreamInstant};
use std::sync::Arc;
use std::time::Duration;

/// Session frames rendered at a time within one device buffer.
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

/// Fills the backend's buffers from a session and decides when the session's
/// last frame has been played.
///
/// The backend states, for each buffer, when the callback runs and when the
/// buffer's first frame will be played. From the position of the session's
/// end in the stream, each buffer therefore gives an estimate of when that
/// end is played. Estimates can only move the expected time later, and
/// playback is complete once a callback runs at or after it.
pub(crate) struct Callback {
    renderer: Renderer,
    shared: Arc<Shared>,
    scratch: Vec<[f64; 2]>,
    device_channels: usize,
    sample_rate_hz: u32,
    /// Frames returned to the backend by earlier callbacks.
    submitted: u64,
    /// The position in the stream, in frames, just past the session's last frame.
    end: Option<u64>,
    /// The latest estimate of when the frame at `end` is played.
    target: Option<StreamInstant>,
    last_callback: Option<StreamInstant>,
}

impl Callback {
    pub(crate) fn new(renderer: Renderer, device_channels: u16, shared: Arc<Shared>) -> Self {
        let sample_rate_hz = renderer.sample_rate().hz();
        Self {
            renderer,
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

    /// Fill one backend buffer. A terminal result observed before or after
    /// rendering makes this whole buffer silent. A concurrent publication after
    /// those checks is observed by a later callback.
    pub(crate) fn fill<T: Copy>(
        &mut self,
        output: &mut [T],
        time: OutputStreamTimestamp,
        convert: &impl Fn(f64) -> T,
    ) {
        if self.shared.result().is_none() {
            self.produce(output, time, convert);
        }
        // Completion is only ever recorded for a buffer that already holds silence.
        if self.shared.result().is_some() {
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
            return self.shared.finish(Err(PlaybackError::MisalignedBuffer));
        }
        if output.is_empty() {
            return;
        }
        if time.playback < time.callback
            || self.last_callback.is_some_and(|last| time.callback < last)
        {
            return self.shared.finish(Err(PlaybackError::InvalidTiming));
        }
        let advancing = self.last_callback.is_none_or(|last| time.callback > last);
        self.last_callback = Some(time.callback);

        let frames = (output.len() / self.device_channels) as u64;
        let active = self.renderer.frames_remaining().min(frames);
        self.render(output, convert);
        if active > 0 {
            self.shared.mark_progress();
        }
        if self.end.is_none() && self.renderer.is_complete() {
            self.end = Some(self.submitted + active);
        }
        if let Some(end) = self.end {
            let estimate = if end >= self.submitted {
                let ahead = frame_duration(end - self.submitted, self.sample_rate_hz, Rounding::Up);
                match time.playback.checked_add(ahead) {
                    Some(estimate) => Some(estimate),
                    None => return self.shared.finish(Err(PlaybackError::InvalidTiming)),
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
                    self.shared.finish(Ok(()));
                }
            }
        }
        self.submitted += frames;
    }

    /// Render the next session frames and place them on the device's channels.
    fn render<T: Copy>(&mut self, output: &mut [T], convert: &impl Fn(f64) -> T) {
        for chunk in output.chunks_mut(SCRATCH_FRAMES * self.device_channels) {
            let frames = chunk.len() / self.device_channels;
            let scratch = &mut self.scratch[..frames];
            self.renderer.render(scratch);
            for (device_frame, session_frame) in chunk
                .chunks_exact_mut(self.device_channels)
                .zip(scratch.iter())
            {
                place(device_frame, session_frame, convert);
            }
        }
    }
}

/// Put left/right on the leading channels and silence on the rest.
fn place<T>(device_frame: &mut [T], session_frame: &[f64; 2], convert: &impl Fn(f64) -> T) {
    for (channel, slot) in device_frame.iter_mut().enumerate() {
        *slot = convert(session_frame.get(channel).copied().unwrap_or(0.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auditory_core::{Component, Ears, SampleRate, Session, Stimulus};
    use proptest::{prelude::*, test_runner::TestRunner};

    fn rate(hz: u32) -> SampleRate {
        SampleRate::new(hz).unwrap()
    }

    fn session(frames: u64, hz: u32) -> Session {
        Stimulus {
            duration_seconds: f64::from(u32::try_from(frames).unwrap()) / f64::from(hz),
            fade_seconds: 0.0,
            components: vec![Component::Tone {
                ears: Ears::Both,
                frequency_hz: 100.0,
                peak_dbfs: -20.0,
                modulation: None,
            }],
        }
        .session(rate(hz))
        .unwrap()
    }
    fn callback(frames: u64, hz: u32) -> Callback {
        Callback::new(session(frames, hz).into_renderer(), 2, Arc::default())
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
        callback.shared.result().is_none()
    }

    fn complete(callback: &Callback) -> bool {
        matches!(callback.shared.result(), Some(Ok(())))
    }

    fn failed_with(callback: &Callback, expected: &PlaybackError) -> bool {
        matches!(callback.shared.result(), Some(Err(error)) if error.to_string() == expected.to_string())
    }

    #[test]
    fn a_stereo_session_ending_in_the_second_scratch_block_drains_without_rounding_early() {
        let session = session(1_537, 44_100);
        let mut callback = Callback::new(session.into_renderer(), 2, Arc::default());
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
        // The longest session at the lowest and at the highest sample rate.
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
    fn the_session_is_rendered_in_order_across_buffers_and_followed_by_silence() {
        let mut expected = [[0.0; 2]; 3_000];
        session(3_000, 8_000).into_renderer().render(&mut expected);
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
    fn a_session_that_ends_exactly_at_a_buffer_boundary_completes_in_a_later_buffer() {
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
            assert!(failed_with(&c, &PlaybackError::InvalidTiming));
            assert_eq!(c.renderer.frames_remaining(), 99);
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
        assert!(failed_with(&c, &PlaybackError::InvalidTiming));
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
        // A frozen clock still rendered session frames.
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
    fn a_failure_during_a_buffer_silences_all_of_it_and_playback_never_resumes() {
        for failure in [
            PlaybackError::Cancelled,
            PlaybackError::TimedOut,
            PlaybackError::Backend(cpal::Error::new(cpal::ErrorKind::DeviceChanged)),
        ] {
            let shared = Arc::new(Shared::default());
            let mut c = Callback::new(
                session(8_000, 8_000).into_renderer(),
                2,
                Arc::clone(&shared),
            );
            let calls = std::cell::Cell::new(0);
            let mut output = vec![1.0; 8_192];
            c.fill(&mut output, time(0, 0), &|value| {
                calls.set(calls.get() + 1);
                if calls.get() == 2_050 {
                    shared.finish(Err(failure.clone()));
                }
                value
            });
            assert!(output.iter().all(|s| *s == 0.0));
            assert!(failed_with(&c, &failure));
            let remaining = c.renderer.frames_remaining();
            assert_eq!(fill(&mut c, 4_096, time(1, 1)), vec![0.0; 8_192]);
            assert_eq!(c.renderer.frames_remaining(), remaining);
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
                shared.finish(Err(PlaybackError::Cancelled));
                // A marked value, to show that the whole buffer is silenced afterwards.
                0.25
            } else {
                sample
            }
        });
        assert!(failed_with(&c, &PlaybackError::Cancelled));
        assert_eq!(output, [0.0; 2]);
    }

    #[test]
    fn a_buffer_holding_a_partial_frame_is_silenced_and_ends_playback() {
        let session = session(8, 8_000);
        let mut c = Callback::new(session.into_renderer(), 2, Arc::default());
        let mut partial = [1.0; 3];
        c.fill(&mut partial, time(0, 0), &|sample| sample);
        assert_eq!(partial, [0.0; 3]);
        assert!(failed_with(&c, &PlaybackError::MisalignedBuffer));
        assert_eq!(c.renderer.frames_remaining(), 8);
        let mut aligned = [1.0; 4];
        c.fill(&mut aligned, time(1, 1), &|sample| sample);
        assert_eq!(aligned, [0.0; 4]);
        assert_eq!(c.renderer.frames_remaining(), 8);
    }

    fn routed(frames: u32) -> Session {
        Stimulus {
            duration_seconds: f64::from(frames) / 8_000.0,
            fade_seconds: 0.0,
            components: vec![
                Component::Tone {
                    ears: Ears::Left,
                    frequency_hz: 257.3,
                    peak_dbfs: -20.0,
                    modulation: None,
                },
                Component::Click {
                    ears: Ears::Right,
                    rate_hz: 37.0,
                    width_seconds: 0.001,
                    peak_dbfs: -14.0,
                    modulation: None,
                },
            ],
        }
        .session(rate(8_000))
        .unwrap()
    }
    #[test]
    fn stereo_routes_are_placed_and_timed_by_frames_on_wide_devices() {
        let mut expected = vec![[0.0; 2]; 3_000];
        routed(3_000).into_renderer().render(&mut expected);
        for channels in [2, 3, 5] {
            let mut c = Callback::new(routed(3_000).into_renderer(), channels, Arc::default());
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
    fn any_sequence_of_buffers_plays_the_session_once_and_completes_only_after_its_end() {
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
                    session(frames, 8_000)
                        .into_renderer()
                        .render(&mut reference);
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
                        prop_assert!(c.shared.result().is_none_or(Result::is_ok));
                        if was_complete {
                            prop_assert!(output.iter().all(|sample| *sample == 0.0));
                        } else {
                            played.extend(output);
                        }
                        if complete(&c) {
                            // Completion needs the whole session rendered, and a callback at or
                            // after the instant the backend first gave for its end.
                            prop_assert!(c.renderer.is_complete());
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
                let mut c =
                    Callback::new(session(2053, hz).into_renderer(), channels, Arc::default());
                let mut buffer = vec![1.0; usize::from(channels) * 4099];
                let count = allocation_counter::measure(|| {
                    c.fill(&mut [], time(0, 10), &|x| x);
                    c.fill(&mut buffer, time(0, 10), &|x| x);
                    c.fill(&mut buffer, time(100, 110), &|x| x);
                    c.fill(&mut buffer, time(200, 210), &|x| x);
                });
                assert_eq!(count.count_total, 0);
                assert_eq!(count.bytes_total, 0);
                assert!(c.shared.result().is_some());
                assert!(buffer.iter().all(|x| *x == 0.0));
            }
        }
        for case in 0..3 {
            let mut callback = callback(10, 48_000);
            let mut buffer = [1.0; 18];
            if case == 0 {
                callback.shared.finish(Err(PlaybackError::Cancelled));
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
            assert!(callback.shared.result().is_some());
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
