// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::shared::{Shared, Terminal};
use auditory_core::Renderer;
use cpal::{OutputStreamTimestamp, StreamInstant};
use std::sync::Arc;
use std::time::Duration;

/// Session frames rendered at a time within one device buffer.
const SCRATCH_FRAMES: usize = crate::OBSERVATION_FRAMES;

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
    observation: Option<crate::ObservationProducer>,
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
            observation: None,
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

    pub(crate) fn observe(&mut self, observation: Option<crate::ObservationProducer>) {
        self.observation = observation;
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
        let active = self.render(output, convert);
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
                    self.shared.finish(Terminal::Complete);
                }
            }
        }
        self.submitted += frames;
    }

    /// Render the next session frames and place them on the device's channels.
    fn render<T: Copy>(&mut self, output: &mut [T], convert: &impl Fn(f64) -> T) -> u64 {
        let mut active = 0;
        for chunk in output.chunks_mut(SCRATCH_FRAMES * self.device_channels) {
            let frames = chunk.len() / self.device_channels;
            let scratch = &mut self.scratch[..frames];
            let span = self.renderer.render(scratch);
            active += span.frames() as u64;
            if let Some(observation) = &mut self.observation {
                observation.publish(span, scratch);
            }
            for (device_frame, session_frame) in chunk
                .chunks_exact_mut(self.device_channels)
                .zip(scratch.iter())
            {
                place(device_frame, session_frame, convert);
            }
        }
        active
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
        callback.shared.terminal() == Terminal::Running
    }

    fn complete(callback: &Callback) -> bool {
        callback.shared.terminal() == Terminal::Complete
    }

    fn terminal_is(callback: &Callback, expected: &Terminal) -> bool {
        callback.shared.terminal() == *expected
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
            assert!(terminal_is(&c, &Terminal::InvalidTiming));
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
                    shared.finish(terminal);
                }
                value
            });
            assert!(output.iter().all(|s| *s == 0.0));
            assert!(terminal_is(&c, &terminal));
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
        let session = session(8, 8_000);
        let mut c = Callback::new(session.into_renderer(), 2, Arc::default());
        let mut partial = [1.0; 3];
        c.fill(&mut partial, time(0, 0), &|sample| sample);
        assert_eq!(partial, [0.0; 3]);
        assert!(terminal_is(&c, &Terminal::MisalignedBuffer));
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
            let remaining = callback.renderer.frames_remaining();
            callback.shared.finish(terminal);
            let mut buffer = [1.0; 18];
            let count = allocation_counter::measure(|| {
                callback.fill(&mut buffer, time(0, 1), &|sample| sample)
            });
            assert_eq!(count.count_total, 0);
            assert_eq!(count.bytes_total, 0);
            assert_eq!(callback.renderer.frames_remaining(), remaining);
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

#[cfg(test)]
mod render_budget {
    use super::*;
    use auditory_core::{Component, Ears, Modulation, SampleRate, Stimulus};
    use cpal::Sample;
    use std::{
        hint::black_box,
        time::{Duration, Instant},
    };

    fn workload(hz: u32, frames: u32, voices: usize, shape: &str, channels: u16) -> Callback {
        assert!((1..=16).contains(&voices));
        let components = (0..voices)
            .map(|index| Component::Tone {
                ears: match index % 3 {
                    0 => Ears::Both,
                    1 => Ears::Left,
                    _ => Ears::Right,
                },
                frequency_hz: f64::from(hz) / 64.0,
                peak_dbfs: -36.0,
                modulation: match shape {
                    "plain" => None,
                    "sam" => Some(Modulation::Sinusoidal {
                        rate_hz: f64::from(hz) / 512.0,
                        depth: 1.0,
                    }),
                    "gated" => Some(Modulation::Rectangular {
                        rate_hz: f64::from(hz) / 512.0,
                        width_seconds: 128.0 / f64::from(hz),
                    }),
                    _ => panic!("unknown workload"),
                },
            })
            .collect();
        let session = Stimulus {
            duration_seconds: f64::from(frames) / f64::from(hz),
            fade_seconds: 0.0,
            components,
        }
        .session(SampleRate::new(hz).unwrap())
        .unwrap();
        assert_eq!(session.frames(), u64::from(frames));
        Callback::new(session.into_renderer(), channels, Arc::default())
    }
    fn active_block<T: Copy>(
        callback: &mut Callback,
        output: &mut [T],
        offset: u64,
        convert: &impl Fn(f64) -> T,
    ) -> Result<Duration, &'static str> {
        let block = output.len() / callback.device_channels;
        if block == 0 || !output.len().is_multiple_of(callback.device_channels) {
            return Err("empty or partial block");
        }
        if callback.renderer.frames_remaining() < u64::try_from(block).unwrap() {
            return Err("insufficient active frames; completed/silent tails cannot count");
        }
        let before = callback.renderer.frames_remaining();
        let nanos = frame_duration(offset, callback.sample_rate_hz, Rounding::Down).as_nanos();
        let instant = cpal::StreamInstant::from_nanos(u64::try_from(nanos).unwrap());
        let time = cpal::OutputStreamTimestamp {
            callback: instant,
            playback: instant,
        };
        let started = Instant::now();
        callback.fill(black_box(output), time, black_box(convert));
        let elapsed = started.elapsed();
        if before - callback.renderer.frames_remaining() != u64::try_from(block).unwrap()
            || callback.shared.terminal() != Terminal::Running
        {
            return Err("callback did not render a complete active block");
        }
        Ok(elapsed)
    }
    #[test]
    fn offline_measurement_rejects_empty_partial_and_completed_workloads() {
        let mut callback = workload(48_000, 64, 1, "plain", 2);
        assert!(active_block(&mut callback, &mut [] as &mut [f64], 0, &|x| x).is_err());
        assert!(active_block(&mut callback, &mut [0.0; 3], 0, &|x| x).is_err());
        assert!(active_block(&mut callback, &mut [0.0; 128], 0, &|x| x).is_ok());
        assert!(active_block(&mut callback, &mut [0.0; 128], 64, &|x| x).is_err());
    }
    fn measure<T: Copy + PartialEq>(
        hz: u32,
        block: u32,
        voices: usize,
        shape: &str,
        channels: u16,
        format: &str,
        mode: &str,
        convert: impl Fn(f64) -> T,
    ) {
        const ITERATIONS: u32 = 128;
        let transport = if matches!(mode, "legacy" | "disabled") {
            None
        } else {
            Some(crate::observation_channel(SampleRate::new(hz).unwrap()))
        };
        let warmup = if mode == "full" {
            16.max(u32::try_from(transport.as_ref().unwrap().1.packet_capacity()).unwrap() + 1)
        } else {
            16
        };
        let frames = (warmup + ITERATIONS + 1) * block;
        let mut callback = workload(hz, frames, voices, shape, channels);
        let mut consumer = None;
        if let Some((producer, receiver)) = transport {
            callback.observe(Some(producer));
            if mode != "disconnected" {
                consumer = Some(receiver);
            }
        }
        let drain = |consumer: &mut Option<crate::ObservationConsumer>| {
            if mode == "drained"
                && let Some(consumer) = consumer
            {
                for _ in 0..consumer.packet_capacity() {
                    if let Some(packet) = consumer.pop() {
                        black_box(packet);
                    } else {
                        break;
                    }
                }
            }
        };
        let mut output =
            vec![convert(0.0); usize::try_from(block).unwrap() * usize::from(channels)];
        for index in 0..warmup {
            active_block(
                &mut callback,
                &mut output,
                u64::from(index * block),
                &convert,
            )
            .unwrap();
            drain(&mut consumer);
        }
        if mode == "full" {
            assert!(callback.observation.as_ref().unwrap().queue_full());
        }
        let mut timings = Vec::with_capacity(usize::try_from(ITERATIONS).unwrap());
        let mut nonzero_blocks = 0;
        let initial = callback.renderer.frames_remaining();
        for index in warmup..warmup + ITERATIONS {
            timings.push(
                active_block(
                    &mut callback,
                    &mut output,
                    u64::from(index * block),
                    &convert,
                )
                .unwrap(),
            );
            if output.iter().any(|sample| *sample != convert(0.0)) {
                nonzero_blocks += 1;
            }
            for frame in output.chunks_exact(usize::from(channels)) {
                assert!(frame[2..].iter().all(|sample| *sample == convert(0.0)));
            }
            black_box(&output);
            drain(&mut consumer);
        }
        let active = initial - callback.renderer.frames_remaining();
        assert_eq!(active, u64::from(ITERATIONS * block));
        assert!(active > 0 && nonzero_blocks > 0);
        timings.sort_unstable();
        let median = timings[64];
        let p95 = timings[121];
        let maximum = timings[127];
        let buffer = f64::from(block) / f64::from(hz);
        let total: Duration = timings.iter().sum();
        let label = if mode == "legacy" {
            "BENCH".to_owned()
        } else {
            format!("OBS_BENCH,{mode}")
        };
        println!(
            "{label},{hz},{block},{voices},{shape},{channels},{format},{ITERATIONS},{active},{nonzero_blocks},{},{},{},{},{},{}",
            total.as_nanos(),
            timings[0].as_nanos(),
            median.as_nanos(),
            p95.as_nanos(),
            maximum.as_nanos(),
            p95.as_secs_f64() / buffer
        );
    }
    #[test]
    #[ignore = "offline timing report, no threshold; run explicitly with --release --nocapture"]
    fn warmed_production_callback_render_and_conversion_budget() {
        report_environment();
        println!(
            "COLUMNS,hz,block,voices,shape,channels,format,iterations,active_frames,nonzero_blocks,total_ns,min_ns,median_ns,p95_ns,max_ns,p95_buffer_ratio"
        );
        for hz in [44_100, 48_000, 96_000] {
            for block in [64, 256, 1_024] {
                for voices in [1, 4, 16] {
                    for shape in ["plain", "sam", "gated"] {
                        for channels in [2, 6] {
                            measure(
                                hz,
                                block,
                                voices,
                                shape,
                                channels,
                                "f64",
                                "legacy",
                                core::convert::identity,
                            );
                            measure(
                                hz,
                                block,
                                voices,
                                shape,
                                channels,
                                "f32",
                                "legacy",
                                f32::from_sample,
                            );
                            measure(
                                hz,
                                block,
                                voices,
                                shape,
                                channels,
                                "i32",
                                "legacy",
                                i32::from_sample,
                            );
                            measure(
                                hz,
                                block,
                                voices,
                                shape,
                                channels,
                                "i24",
                                "legacy",
                                crate::output::to_i24,
                            );
                            measure(
                                hz,
                                block,
                                voices,
                                shape,
                                channels,
                                "i16",
                                "legacy",
                                i16::from_sample,
                            );
                        }
                    }
                }
            }
        }
    }
    fn report_environment() {
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        assert_eq!(profile, "release", "measure the release profile");
        println!(
            "PROFILE,release; overflow-checks=true; target={}-{}; version={}",
            std::env::consts::ARCH,
            std::env::consts::OS,
            env!("CARGO_PKG_VERSION")
        );
        println!(
            "SOURCE,{}",
            std::env::var("AUDITORYLAB_BENCH_SOURCE").unwrap_or_else(|_| {
                "unqualified working tree; retain source-file identities with the report".into()
            })
        );
        let rust = std::process::Command::new("rustc")
            .arg("-vV")
            .output()
            .unwrap();
        assert!(rust.status.success());
        println!(
            "TOOLCHAIN,{}",
            String::from_utf8(rust.stdout).unwrap().replace('\n', "; ")
        );
        #[cfg(target_os = "macos")]
        {
            let cpu = std::process::Command::new("/usr/sbin/sysctl")
                .args(["-n", "machdep.cpu.brand_string"])
                .output()
                .unwrap();
            assert!(cpu.status.success());
            println!("CPU,{}", String::from_utf8(cpu.stdout).unwrap().trim());
        }
        #[cfg(target_os = "linux")]
        {
            let info = std::fs::read_to_string("/proc/cpuinfo").unwrap();
            println!(
                "CPU,{}",
                info.lines()
                    .find(|line| line.starts_with("model name") || line.starts_with("Hardware"))
                    .unwrap_or("not exposed by /proc/cpuinfo")
            );
        }
        #[cfg(windows)]
        println!(
            "CPU,{}",
            std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unavailable".into())
        );
    }
    #[test]
    #[ignore = "offline enabled/disabled transport timing, no threshold or audio"]
    fn observed_production_callback_budget() {
        report_environment();
        println!(
            "OBS_COLUMNS,mode,hz,block,voices,shape,channels,format,iterations,active_frames,nonzero_blocks,total_ns,min_ns,median_ns,p95_ns,max_ns,p95_buffer_ratio"
        );
        for hz in [44_100, 48_000, 96_000, 768_000] {
            for block in [64, 256, 1024] {
                for voices in [1, 4, 16] {
                    for shape in ["plain", "sam", "gated"] {
                        for channels in [2, 6] {
                            for mode in ["disabled", "drained", "full", "disconnected"] {
                                measure(
                                    hz,
                                    block,
                                    voices,
                                    shape,
                                    channels,
                                    "f64",
                                    mode,
                                    core::convert::identity,
                                );
                                measure(
                                    hz,
                                    block,
                                    voices,
                                    shape,
                                    channels,
                                    "f32",
                                    mode,
                                    f32::from_sample,
                                );
                                measure(
                                    hz,
                                    block,
                                    voices,
                                    shape,
                                    channels,
                                    "i32",
                                    mode,
                                    i32::from_sample,
                                );
                                measure(
                                    hz,
                                    block,
                                    voices,
                                    shape,
                                    channels,
                                    "i24",
                                    mode,
                                    crate::output::to_i24,
                                );
                                measure(
                                    hz,
                                    block,
                                    voices,
                                    shape,
                                    channels,
                                    "i16",
                                    mode,
                                    i16::from_sample,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod observation_tests {
    use super::*;
    use auditory_core::{Component, Ears, SampleRate, Stimulus};
    fn callback() -> Callback {
        let session = Stimulus {
            duration_seconds: 1537.0 / 44100.0,
            fade_seconds: 5.0 / 44100.0,
            components: vec![
                Component::Tone {
                    ears: Ears::Left,
                    frequency_hz: 1000.0,
                    peak_dbfs: -20.0,
                    modulation: None,
                },
                Component::Tone {
                    ears: Ears::Right,
                    frequency_hz: 1003.0,
                    peak_dbfs: -26.0,
                    modulation: None,
                },
            ],
        }
        .session(SampleRate::new(44100).unwrap())
        .unwrap();
        Callback::new(session.into_renderer(), 6, Arc::default())
    }
    fn time(ms: u64) -> OutputStreamTimestamp {
        OutputStreamTimestamp {
            callback: StreamInstant::from_millis(ms),
            playback: StreamInstant::from_millis(ms),
        }
    }
    #[test]
    fn observation_never_changes_pcm_routing_tails_or_terminal_arbitration() {
        for mode in 0..4 {
            let mut baseline = callback();
            let mut observed = callback();
            let (producer, consumer) = crate::observation_channel(
                SampleRate::new(if mode == 2 { 1 } else { 44100 }).unwrap(),
            );
            observed.observe(Some(producer));
            let mut consumer = Some(consumer);
            if mode == 3 {
                consumer.take();
            }
            for (index, frames) in [0, 1, 1025, 2049, 16, 4097].into_iter().enumerate() {
                let mut expected = vec![1.0; frames * 6];
                let mut actual = expected.clone();
                baseline.fill(&mut expected, time(index as u64 * 100), &|x| x);
                let count = allocation_counter::measure(|| {
                    observed.fill(&mut actual, time(index as u64 * 100), &|x| x)
                });
                assert_eq!((count.count_total, count.bytes_total), (0, 0));
                assert_eq!(
                    actual.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                    expected.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
                );
                assert_eq!(observed.shared.terminal(), baseline.shared.terminal());
                assert_eq!(observed.end, baseline.end);
                assert_eq!(observed.target, baseline.target);
                assert_eq!(observed.submitted, baseline.submitted);
                if mode == 0
                    && let Some(consumer) = &mut consumer
                {
                    for _ in 0..consumer.packet_capacity() {
                        let Some(packet) = consumer.pop() else { break };
                        assert!(packet.start() + packet.samples().len() as u64 <= 1537);
                    }
                }
            }
            assert_eq!(observed.shared.terminal(), Terminal::Complete);
            if let Some(mut consumer) = consumer {
                assert_eq!(consumer.latest_generated(), 1537);
                if mode == 2 {
                    let packet = consumer.pop().unwrap();
                    assert_eq!(packet.samples().len(), 1);
                    assert!(consumer.pop().is_none());
                }
            }
        }
    }
    #[test]
    fn enabled_full_cancelled_malformed_timing_and_tail_paths_allocate_nothing() {
        for case in 0..6 {
            let mut c = callback();
            let (producer, consumer) = crate::observation_channel(SampleRate::new(1).unwrap());
            c.observe(Some(producer));
            if case == 1 {
                drop(consumer);
            } // disconnected
            let mut output = vec![1.0; 4099 * 6];
            if case == 2 {
                c.shared.finish(Terminal::Cancelled);
            }
            if case == 3 {
                output.pop();
            }
            let timestamp = if case == 4 {
                OutputStreamTimestamp {
                    callback: StreamInstant::from_millis(2),
                    playback: StreamInstant::from_millis(1),
                }
            } else {
                time(0)
            };
            let count = allocation_counter::measure(|| {
                c.fill(&mut output, timestamp, &|x| x);
                c.fill(&mut output, time(100), &|x| x);
            });
            assert_eq!((count.count_total, count.bytes_total), (0, 0));
            assert!(c.shared.terminal() != Terminal::Running);
        }
    }
    #[test]
    fn generation_observation_does_not_claim_a_cancelled_device_buffer_was_played() {
        let mut c = callback();
        let (producer, mut consumer) = crate::observation_channel(SampleRate::new(44100).unwrap());
        c.observe(Some(producer));
        let shared = Arc::clone(&c.shared);
        let mut output = [1.0; 6144];
        c.fill(&mut output, time(0), &|x| {
            shared.finish(Terminal::Cancelled);
            x
        });
        assert!(output.iter().all(|x| *x == 0.0));
        assert_eq!(c.shared.terminal(), Terminal::Cancelled);
        let packet = consumer.pop().unwrap();
        assert_eq!(packet.start(), 0);
        assert_eq!(packet.samples().len(), 1024);
        assert!(packet.samples().iter().flatten().any(|x| *x != 0.0));
        assert_eq!(consumer.latest_generated(), 1024);
    }
}
