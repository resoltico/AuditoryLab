// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::{
    PlaybackError,
    callback::{Callback, Rounding, frame_duration},
    shared::Shared,
};
use auditory_core::{SampleRate, Session, Stimulus, StimulusError};
use cpal::{
    OutputCallbackInfo, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// How long a stream may go without progress, and how long playback may take
/// beyond the session's length to start up and drain.
const STALL_LIMIT: Duration = Duration::from_secs(30);

/// How samples are represented in the device's stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleRepresentation {
    /// 64-bit floating point, which holds every sample exactly.
    F64,
    /// 32-bit floating point.
    F32,
    /// 32-bit signed integers.
    I32,
    /// 24-bit signed integers.
    I24,
    /// 16-bit signed integers.
    I16,
}

impl SampleRepresentation {
    const fn of(format: SampleFormat) -> Option<Self> {
        match format {
            SampleFormat::F64 => Some(Self::F64),
            SampleFormat::F32 => Some(Self::F32),
            SampleFormat::I32 => Some(Self::I32),
            SampleFormat::I24 => Some(Self::I24),
            SampleFormat::I16 => Some(Self::I16),
            _ => None,
        }
    }
}

impl Display for SampleRepresentation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::F64 => "64-bit float",
            Self::F32 => "32-bit float",
            Self::I32 => "32-bit integer",
            Self::I24 => "24-bit integer",
            Self::I16 => "16-bit integer",
        })
    }
}

/// What a completed playback did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Played {
    /// Number of session frames played.
    pub frames: u64,
    /// The sample rate used by the playback stream.
    pub sample_rate: SampleRate,
    /// How the samples were represented in the device's stream. All but the
    /// 64-bit floats round or quantize them.
    pub representation: SampleRepresentation,
    /// Buffer underruns the backend reported; each may be an audible gap.
    pub underruns: u64,
}

/// The discovery snapshot used for playback; the route may later change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDescription {
    /// Best-effort backend name.
    pub name: Option<String>,
    /// Number of channels in each playback stream frame.
    pub channels: u16,
    /// The backend-reported default stream sample rate.
    pub sample_rate: SampleRate,
    /// The backend-reported default stream representation.
    pub representation: SampleRepresentation,
}

/// The default output device and its backend-reported stream configuration.
pub struct Output {
    device: cpal::Device,
    config: StreamConfig,
    description: OutputDescription,
}

impl Output {
    /// Find the default output and its backend-reported default stream configuration.
    ///
    /// Discovery does not start this app's playback stream, but backend queries
    /// may open resources. Playback builds a stream from the saved rate,
    /// channels and representation; these do not certify the physical hardware rate.
    ///
    /// # Errors
    /// Returns an error when there is no default output device, the backend
    /// cannot describe it, or its default stream configuration has an unsupported
    /// sample rate/representation or fewer than two output channels.
    pub fn default_device() -> Result<Self, PlaybackError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(PlaybackError::NoDefaultOutput)?;
        let supported = device.default_output_config()?;
        let (sample_rate, representation) = supported_format(&supported)?;
        let name = device
            .description()
            .ok()
            .map(|description| description.name().to_owned());
        Ok(Self {
            device,
            config: supported.config(),
            description: OutputDescription {
                name,
                channels: supported.channels(),
                representation,
                sample_rate,
            },
        })
    }

    /// Play a session and wait until the backend's timing indicates that its
    /// last frame has been played.
    ///
    /// The left and right samples play on stream channels zero and one. The
    /// waiting thread polls `cancel` at 10 ms intervals. Scheduling and backend
    /// calls can delay observation; callbacks return silence after observing
    /// the terminal result, and sound already queued cannot be recalled.
    /// Underruns are counted. A report that
    /// the output route changed ends playback with an error, as does a
    /// stream that makes no progress for 30 seconds or outlasts the session
    /// by more than 30 seconds.
    ///
    /// # Errors
    /// Returns an error for a session of another sample rate, cancellation,
    /// a backend failure, invalid backend timing or a timeout.
    /// ```no_run
    /// fn play(output: auditory_device::Output, session: auditory_core::Session, cancel: &std::sync::atomic::AtomicBool) {
    ///     let _ = output.play(session, cancel);
    /// }
    /// ```
    /// ```compile_fail,E0308
    /// fn play(output: auditory_device::Output, renderer: auditory_core::Renderer, cancel: &std::sync::atomic::AtomicBool) {
    ///     let _ = output.play(renderer, cancel);
    /// }
    /// ```
    pub fn play(self, session: Session, cancel: &AtomicBool) -> Result<Played, PlaybackError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(PlaybackError::Cancelled);
        }
        fits(session.sample_rate(), self.description.sample_rate)?;
        let frames = session.frames();
        let time_limit = time_limit(frames, self.description.sample_rate);
        let shared = Arc::new(Shared::default());
        let callback = Callback::new(
            session.into_renderer(),
            self.config.channels,
            Arc::clone(&shared),
        );
        // Every way playback can end is recorded in `shared`; the first one stands.
        match self.stream(callback, &shared) {
            Ok(stream) => {
                match stream.play() {
                    Ok(()) => supervise(&shared, time_limit, cancel),
                    Err(error) => shared.finish(Err(error.into())),
                }
                drop(stream);
            }
            Err(error) => shared.finish(Err(error.into())),
        }
        match shared.result() {
            Some(Ok(())) => Ok(Played {
                frames,
                sample_rate: self.description.sample_rate,
                representation: self.description.representation,
                underruns: shared.underruns(),
            }),
            Some(Err(error)) => Err(error.clone()),
            // `supervise` returns only once a result is recorded.
            None => Err(PlaybackError::TimedOut),
        }
    }

    fn stream(&self, callback: Callback, shared: &Arc<Shared>) -> Result<Stream, cpal::Error> {
        match self.description.representation {
            SampleRepresentation::F64 => self.stream_of(callback, shared, std::convert::identity),
            SampleRepresentation::F32 => self.stream_of(callback, shared, f32::from_sample),
            SampleRepresentation::I32 => self.stream_of(callback, shared, i32::from_sample),
            SampleRepresentation::I24 => self.stream_of(callback, shared, to_i24),
            SampleRepresentation::I16 => self.stream_of(callback, shared, i16::from_sample),
        }
    }

    fn stream_of<T: SizedSample>(
        &self,
        mut callback: Callback,
        shared: &Arc<Shared>,
        convert: impl Fn(f64) -> T + Send + 'static,
    ) -> Result<Stream, cpal::Error> {
        let reports = Arc::clone(shared);
        self.device.build_output_stream(
            self.config,
            move |output: &mut [T], info: &OutputCallbackInfo| {
                callback.fill(output, info.timestamp(), &convert);
            },
            move |error| reports.report(error),
            Some(STALL_LIMIT),
        )
    }
}

/// Quantize through a saturated 32-bit integer: the direct conversion maps
/// positive full scale one step beyond the 24-bit range.
fn to_i24(sample: f64) -> cpal::I24 {
    cpal::I24::from_sample(i32::from_sample(sample))
}

impl Output {
    /// Borrow the configuration snapshot without opening a stream.
    #[must_use]
    pub const fn description(&self) -> &OutputDescription {
        &self.description
    }

    /// Build a stimulus at the configured sample rate.
    ///
    /// # Errors
    /// Returns what [`Stimulus::session`] returns at that rate.
    pub fn session(&self, stimulus: &Stimulus) -> Result<Session, StimulusError> {
        stimulus.session(self.description.sample_rate)
    }
}

/// Validate the single discovery snapshot without opening a stream.
fn supported_format(
    supported: &cpal::SupportedStreamConfig,
) -> Result<(SampleRate, SampleRepresentation), PlaybackError> {
    let representation = SampleRepresentation::of(supported.sample_format()).ok_or(
        PlaybackError::UnsupportedSampleFormat(supported.sample_format()),
    )?;
    let hz = supported.sample_rate();
    let sample_rate =
        SampleRate::new(hz).map_err(|_| PlaybackError::UnsupportedSampleRate { hz })?;
    if supported.channels() < 2 {
        return Err(PlaybackError::TooFewChannels {
            channels: supported.channels(),
        });
    }
    Ok((sample_rate, representation))
}

fn fits(session_rate: SampleRate, device_rate: SampleRate) -> Result<(), PlaybackError> {
    if session_rate != device_rate {
        return Err(PlaybackError::SampleRateMismatch {
            session_hz: session_rate.hz(),
            device_hz: device_rate.hz(),
        });
    }
    Ok(())
}

/// The session's length, rounded up to a nanosecond, plus the stall limit.
fn time_limit(frames: u64, rate: SampleRate) -> Duration {
    frame_duration(frames, rate.hz(), Rounding::Up) + STALL_LIMIT
}

/// Decides from the elapsed time and the progress count when a stream has
/// stalled or outlasted its limit.
struct Watchdog {
    time_limit: Duration,
    last_progress: u64,
    last_progress_at: Duration,
}

impl Watchdog {
    const fn new(time_limit: Duration) -> Self {
        Self {
            time_limit,
            last_progress: 0,
            last_progress_at: Duration::ZERO,
        }
    }

    fn expired(&mut self, elapsed: Duration, progress: u64) -> bool {
        if progress != self.last_progress {
            self.last_progress = progress;
            self.last_progress_at = elapsed;
        }
        elapsed >= self.time_limit || elapsed.saturating_sub(self.last_progress_at) >= STALL_LIMIT
    }
}

/// Wait until playback has a result, recording cancellation or a timeout
/// when that comes first.
fn supervise(shared: &Shared, time_limit: Duration, cancel: &AtomicBool) {
    let started = Instant::now();
    let mut watchdog = Watchdog::new(time_limit);
    loop {
        if shared.result().is_some() {
            return;
        }
        if cancel.load(Ordering::Relaxed) {
            return shared.finish(Err(PlaybackError::Cancelled));
        }
        if watchdog.expired(started.elapsed(), shared.progress()) {
            return shared.finish(Err(PlaybackError::TimedOut));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(hz: u32) -> SampleRate {
        SampleRate::new(hz).unwrap()
    }

    #[test]
    fn discovery_requires_stereo_or_wider_and_a_supported_rate_and_representation() {
        for channels in [0, 1, 2, 5, u16::MAX] {
            let snapshot = cpal::SupportedStreamConfig::new(
                channels,
                44_100,
                cpal::SupportedBufferSize::Unknown,
                SampleFormat::F32,
            );
            let result = supported_format(&snapshot);
            if channels < 2 {
                assert!(
                    matches!(result, Err(PlaybackError::TooFewChannels { channels: rejected }) if rejected == channels)
                );
            } else {
                assert!(
                    matches!(result, Ok((rate, SampleRepresentation::F32)) if rate.hz() == 44_100)
                );
            }
        }
        for hz in [0, 768_001] {
            let snapshot = cpal::SupportedStreamConfig::new(
                2,
                hz,
                cpal::SupportedBufferSize::Unknown,
                SampleFormat::F32,
            );
            assert!(
                matches!(supported_format(&snapshot), Err(PlaybackError::UnsupportedSampleRate { hz: rejected }) if rejected == hz)
            );
        }
        let snapshot = cpal::SupportedStreamConfig::new(
            2,
            48_000,
            cpal::SupportedBufferSize::Unknown,
            SampleFormat::U16,
        );
        assert!(matches!(
            supported_format(&snapshot),
            Err(PlaybackError::UnsupportedSampleFormat(SampleFormat::U16))
        ));
    }

    #[test]
    fn five_stream_representations_are_supported_and_named() {
        for (format, representation, name) in [
            (SampleFormat::F64, SampleRepresentation::F64, "64-bit float"),
            (SampleFormat::F32, SampleRepresentation::F32, "32-bit float"),
            (
                SampleFormat::I32,
                SampleRepresentation::I32,
                "32-bit integer",
            ),
            (
                SampleFormat::I24,
                SampleRepresentation::I24,
                "24-bit integer",
            ),
            (
                SampleFormat::I16,
                SampleRepresentation::I16,
                "16-bit integer",
            ),
        ] {
            assert_eq!(SampleRepresentation::of(format), Some(representation));
            assert_eq!(representation.to_string(), name);
        }
        for format in [
            SampleFormat::U16,
            SampleFormat::U24,
            SampleFormat::U32,
            SampleFormat::I8,
            SampleFormat::U8,
            SampleFormat::DsdU8,
        ] {
            assert_eq!(SampleRepresentation::of(format), None);
        }
    }

    #[test]
    fn a_session_must_use_the_device_rate() {
        assert!(fits(rate(44_100), rate(44_100)).is_ok());
        assert!(matches!(
            fits(rate(44_100), rate(48_000)),
            Err(PlaybackError::SampleRateMismatch {
                session_hz: 44_100,
                device_hz: 48_000
            })
        ));
    }

    #[test]
    fn integer_representations_reach_but_never_pass_full_scale() {
        // Independent constants: negative and positive full scale, and silence.
        assert_eq!(i16::from_sample(-1.0_f64), -32_768);
        assert_eq!(i16::from_sample(0.0_f64), 0);
        assert_eq!(i16::from_sample(-0.0_f64), 0);
        assert_eq!(i16::from_sample(1.0_f64), 32_767);
        assert_eq!(i16::from_sample(0.5_f64), 16_384);
        assert_eq!(i32::from_sample(-1.0_f64), -2_147_483_648);
        assert_eq!(i32::from_sample(0.0_f64), 0);
        assert_eq!(i32::from_sample(1.0_f64), 2_147_483_647);
        assert_eq!(i32::from_sample(0.5_f64), 1_073_741_824);
        assert_eq!(to_i24(-1.0).inner(), -8_388_608);
        assert_eq!(to_i24(0.0).inner(), 0);
        assert_eq!(to_i24(1.0).inner(), 8_388_607);
        assert_eq!(to_i24(0.5).inner(), 4_194_304);
        // Positive full scale as the three bytes a 24-bit device receives.
        assert_eq!(&to_i24(1.0).inner().to_le_bytes()[..3], &[0xff, 0xff, 0x7f]);
        assert_eq!(&to_i24(-1.0).inner().to_le_bytes()[..3], &[0, 0, 0x80]);
        for sample in [-1.0, -0.999_999_9, -1e-9, 0.0, 1e-9, 0.999_999_9, 1.0] {
            assert!((-8_388_608..=8_388_607).contains(&to_i24(sample).inner()));
        }
        // Direct multiplication/rounding exceeds the signed 24-bit range.
        assert_eq!(cpal::I24::from_sample(1.0_f64).inner(), 8_388_608);
    }

    #[test]
    fn a_float_stream_receives_the_nearest_binary32() {
        for sample in [-1.0, -0.1, -0.0, 0.0, 0.1, 1.0 / 3.0, 1.0] {
            let converted: f32 = f32::from_sample(sample);
            assert!(
                (f64::from(converted) - sample).abs()
                    <= sample.abs() * f64::from(f32::EPSILON) / 2.0
            );
        }
        assert_eq!(f32::from_sample(-0.0_f64).to_bits(), (-0.0_f32).to_bits());
    }

    #[test]
    fn the_time_limit_is_the_session_length_rounded_up_plus_the_stall_limit() {
        assert_eq!(time_limit(1, rate(8_001)), Duration::new(30, 124_985));
        assert_eq!(time_limit(480_000, rate(48_000)), Duration::from_secs(40));
        assert_eq!(
            time_limit(1 << 36, rate(1)),
            Duration::from_secs((1 << 36) + 30)
        );
    }

    #[test]
    fn the_watchdog_requires_progress_and_enforces_the_time_limit() {
        let mut stalled = Watchdog::new(Duration::from_secs(3_630));
        assert!(!stalled.expired(Duration::from_secs(29), 0));
        assert!(stalled.expired(Duration::from_secs(30), 0));

        let mut progressing = Watchdog::new(Duration::from_secs(60));
        assert!(!progressing.expired(Duration::from_secs(29), 1));
        assert!(!progressing.expired(Duration::from_secs(58), 2));
        // Progress does not extend the overall limit.
        assert!(progressing.expired(Duration::from_secs(60), 3));

        let mut resumed = Watchdog::new(Duration::from_secs(3_630));
        assert!(!resumed.expired(Duration::from_secs(20), 1));
        // Unchanged progress keeps the earlier time as the last progress.
        assert!(!resumed.expired(Duration::from_secs(49), 1));
        assert!(resumed.expired(Duration::from_secs(50), 1));

        let limit = time_limit(1, rate(8_001));
        let mut exact = Watchdog::new(limit);
        assert!(!exact.expired(limit - Duration::from_nanos(1), 1));
        assert!(exact.expired(limit, 2));
    }

    #[test]
    fn supervision_records_cancellation_and_leaves_an_earlier_result_alone() {
        let shared = Shared::default();
        supervise(&shared, Duration::from_secs(3_600), &AtomicBool::new(true));
        assert!(matches!(
            shared.result(),
            Some(Err(PlaybackError::Cancelled))
        ));

        let shared = Shared::default();
        shared.finish(Ok(()));
        supervise(&shared, Duration::ZERO, &AtomicBool::new(true));
        assert!(matches!(shared.result(), Some(Ok(()))));
    }

    #[test]
    fn supervision_times_out_a_stream_that_has_outlasted_its_limit() {
        let shared = Arc::new(Shared::default());
        // Completion half a second later comes too late: the limit has passed.
        let stream = Arc::clone(&shared);
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(500));
            stream.finish(Ok(()));
        });
        supervise(&shared, Duration::ZERO, &AtomicBool::new(false));
        assert!(matches!(
            shared.result(),
            Some(Err(PlaybackError::TimedOut))
        ));
        worker.join().unwrap();
    }

    #[test]
    fn supervision_waits_for_a_result_recorded_by_another_thread() {
        let shared = Arc::new(Shared::default());
        let stream = Arc::clone(&shared);
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            stream.finish(Ok(()));
        });
        // The limit is far beyond the wait, yet lets the test end if the result is never seen.
        supervise(&shared, Duration::from_secs(10), &AtomicBool::new(false));
        assert!(matches!(shared.result(), Some(Ok(()))));
        worker.join().unwrap();
    }
}
