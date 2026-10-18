// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::playback::{
    PlaybackError,
    callback::{Callback, Rounding, frame_duration},
    shared::{Shared, Terminal},
};
use auditorylab_core::{SampleRate, Signal};
use cpal::{
    OutputCallbackInfo, Sample, SizedSample, Stream, StreamConfig,
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
pub enum SampleFormat {
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

impl SampleFormat {
    const fn of(format: cpal::SampleFormat) -> Option<Self> {
        match format {
            cpal::SampleFormat::F64 => Some(Self::F64),
            cpal::SampleFormat::F32 => Some(Self::F32),
            cpal::SampleFormat::I32 => Some(Self::I32),
            cpal::SampleFormat::I24 => Some(Self::I24),
            cpal::SampleFormat::I16 => Some(Self::I16),
            _ => None,
        }
    }
}

impl Display for SampleFormat {
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
pub(crate) struct Played {
    /// Number of session frames played.
    pub frames: u64,
    /// The sample rate used by the playback stream.
    pub sample_rate: SampleRate,
    /// How the samples were represented in the device's stream. All but the
    /// 64-bit floats round or quantize them.
    pub format: SampleFormat,
    /// Buffer underruns the backend reported; each may be an audible gap.
    pub underruns: u64,
}

/// The discovery snapshot used for playback; the route may later change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputInfo {
    /// Best-effort backend name.
    pub name: Option<String>,
    /// Number of channels in each playback stream frame.
    pub channels: u16,
    /// The backend-reported default stream sample rate.
    pub sample_rate: SampleRate,
    /// The backend-reported default stream format.
    pub format: SampleFormat,
}

/// The default output device and its backend-reported stream configuration.
pub(crate) struct Output {
    device: cpal::Device,
    config: StreamConfig,
    description: OutputInfo,
}

impl Output {
    /// Find the default output and its backend-reported default stream configuration.
    ///
    /// Discovery does not start this app's playback stream, but backend queries
    /// may open resources. Playback builds a stream from the saved rate,
    /// channels and format; these do not certify the physical hardware rate.
    ///
    /// # Errors
    /// Returns an error when there is no default output device, the backend
    /// cannot describe it, or its default stream configuration has an unsupported
    /// sample rate/format or fewer than two output channels.
    pub(crate) fn default_device() -> Result<Self, PlaybackError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(PlaybackError::NoDefaultOutput)?;
        let supported = device.default_output_config()?;
        let (sample_rate, format) = supported_format(&supported)?;
        let name = device
            .description()
            .ok()
            .map(|description| description.name().to_owned());
        Ok(Self {
            device,
            config: supported.config(),
            description: OutputInfo {
                name,
                channels: supported.channels(),
                format,
                sample_rate,
            },
        })
    }

    pub(crate) fn play(
        self,
        session: Arc<Signal>,
        cancel: &AtomicBool,
        progress: crate::Progress,
    ) -> Result<Played, PlaybackError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(PlaybackError::Cancelled);
        }
        fits(session.sample_rate(), self.description.sample_rate)?;
        let frames = session.frames();
        let time_limit = time_limit(frames, self.description.sample_rate);
        let shared = Arc::new(Shared::default());
        let callback = Callback::new(session, self.config.channels, Arc::clone(&shared));
        let callback = callback.with_progress(progress);
        // Keep synchronous diagnostics on this worker, before publishing their
        // fixed outcome. An asynchronous writer can be paused independently.
        let mut setup_error = None;
        let terminal = match self.stream(callback, &shared) {
            Ok(stream) => {
                let terminal = match stream.play() {
                    Ok(()) => supervise(&shared, time_limit, cancel),
                    Err(error) => setup_failed(&shared, error, &mut setup_error),
                };
                drop(stream);
                terminal
            }
            Err(error) => setup_failed(&shared, error, &mut setup_error),
        };
        resolve(&shared, terminal, setup_error)?;
        Ok(Played {
            frames,
            sample_rate: self.description.sample_rate,
            format: self.description.format,
            underruns: shared.underruns(),
        })
    }

    fn stream(&self, callback: Callback, shared: &Arc<Shared>) -> Result<Stream, cpal::Error> {
        match self.description.format {
            SampleFormat::F64 => self.stream_of(callback, shared, std::convert::identity),
            SampleFormat::F32 => self.stream_of(callback, shared, f32::from_sample),
            SampleFormat::I32 => self.stream_of(callback, shared, i32::from_sample),
            SampleFormat::I24 => self.stream_of(callback, shared, to_i24),
            SampleFormat::I16 => self.stream_of(callback, shared, i16::from_sample),
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
pub(crate) fn to_i24(sample: f64) -> cpal::I24 {
    cpal::I24::from_sample(i32::from_sample(sample))
}

impl Output {
    /// Borrow the configuration snapshot without opening a stream.
    #[must_use]
    pub(crate) const fn description(&self) -> &OutputInfo {
        &self.description
    }
}

/// Validate the single discovery snapshot without opening a stream.
fn supported_format(
    supported: &cpal::SupportedStreamConfig,
) -> Result<(SampleRate, SampleFormat), PlaybackError> {
    let format = SampleFormat::of(supported.sample_format()).ok_or(
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
    Ok((sample_rate, format))
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
fn supervise(shared: &Shared, time_limit: Duration, cancel: &AtomicBool) -> Terminal {
    let started = Instant::now();
    let mut watchdog = Watchdog::new(time_limit);
    loop {
        let terminal = shared.terminal();
        if terminal != Terminal::Running {
            return terminal;
        }
        if cancel.load(Ordering::Relaxed) {
            shared.finish(Terminal::Cancelled);
            return shared.terminal();
        }
        if watchdog.expired(started.elapsed(), shared.progress()) {
            shared.finish(Terminal::TimedOut);
            return shared.terminal();
        }
        thread::sleep(POLL_INTERVAL);
    }
}

pub(crate) fn setup_failed(
    shared: &Shared,
    error: cpal::Error,
    local: &mut Option<cpal::Error>,
) -> Terminal {
    *local = Some(error);
    shared.finish(Terminal::StreamSetupFailed);
    shared.terminal()
}

/// Resolve only on the worker, after stream teardown. Backend payload storage
/// precedes its terminal publication; setup diagnostics never enter that slot.
pub(crate) fn resolve(
    shared: &Shared,
    terminal: Terminal,
    setup_error: Option<cpal::Error>,
) -> Result<(), PlaybackError> {
    match terminal {
        Terminal::Complete => Ok(()),
        Terminal::Cancelled => Err(PlaybackError::Cancelled),
        Terminal::TimedOut => Err(PlaybackError::TimedOut),
        Terminal::InvalidTiming => Err(PlaybackError::InvalidTiming),
        Terminal::MisalignedBuffer => Err(PlaybackError::MisalignedBuffer),
        Terminal::Backend => Err(PlaybackError::Backend(shared.backend_error().clone())),
        Terminal::StreamSetupFailed => {
            let Some(error) = setup_error else {
                unreachable!("setup failure retains its worker-local error");
            };
            Err(PlaybackError::Backend(error))
        }
        Terminal::Running => {
            unreachable!("supervision and setup failure return a published outcome")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(hz: u32) -> SampleRate {
        SampleRate::new(hz).unwrap()
    }

    #[test]
    fn discovery_requires_stereo_or_wider_and_a_supported_rate_and_format() {
        for channels in [0, 1, 2, 5, u16::MAX] {
            let snapshot = cpal::SupportedStreamConfig::new(
                channels,
                44_100,
                cpal::SupportedBufferSize::Unknown,
                cpal::SampleFormat::F32,
            );
            let result = supported_format(&snapshot);
            if channels < 2 {
                assert!(
                    matches!(result, Err(PlaybackError::TooFewChannels { channels: rejected }) if rejected == channels)
                );
            } else {
                assert!(matches!(result, Ok((rate, SampleFormat::F32)) if rate.hz() == 44_100));
            }
        }
        for hz in [0, 768_001] {
            let snapshot = cpal::SupportedStreamConfig::new(
                2,
                hz,
                cpal::SupportedBufferSize::Unknown,
                cpal::SampleFormat::F32,
            );
            assert!(
                matches!(supported_format(&snapshot), Err(PlaybackError::UnsupportedSampleRate { hz: rejected }) if rejected == hz)
            );
        }
        let snapshot = cpal::SupportedStreamConfig::new(
            2,
            48_000,
            cpal::SupportedBufferSize::Unknown,
            cpal::SampleFormat::U16,
        );
        assert!(matches!(
            supported_format(&snapshot),
            Err(PlaybackError::UnsupportedSampleFormat(
                cpal::SampleFormat::U16
            ))
        ));
    }

    #[test]
    fn five_stream_formats_are_supported_and_named() {
        for (native, format) in [
            (cpal::SampleFormat::F64, SampleFormat::F64),
            (cpal::SampleFormat::F32, SampleFormat::F32),
            (cpal::SampleFormat::I32, SampleFormat::I32),
            (cpal::SampleFormat::I24, SampleFormat::I24),
            (cpal::SampleFormat::I16, SampleFormat::I16),
        ] {
            assert_eq!(SampleFormat::of(native), Some(format));
            assert!(!format.to_string().is_empty());
        }
        assert_eq!(SampleFormat::of(cpal::SampleFormat::U16), None);
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
    fn integer_formats_reach_but_never_pass_full_scale() {
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
        assert!(matches!(shared.terminal(), Terminal::Cancelled));

        let shared = Shared::default();
        shared.finish(Terminal::Complete);
        supervise(&shared, Duration::ZERO, &AtomicBool::new(true));
        assert!(shared.terminal() == Terminal::Complete);
    }

    #[test]
    fn supervision_times_out_a_stream_that_has_outlasted_its_limit() {
        let shared = Arc::new(Shared::default());
        // Completion half a second later comes too late: the limit has passed.
        let stream = Arc::clone(&shared);
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(500));
            stream.finish(Terminal::Complete);
        });
        supervise(&shared, Duration::ZERO, &AtomicBool::new(false));
        assert!(matches!(shared.terminal(), Terminal::TimedOut));
        worker.join().unwrap();
    }

    #[test]
    fn supervision_waits_for_a_result_recorded_by_another_thread() {
        let shared = Arc::new(Shared::default());
        let stream = Arc::clone(&shared);
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            stream.finish(Terminal::Complete);
        });
        // The limit is far beyond the wait, yet lets the test end if the result is never seen.
        supervise(&shared, Duration::from_secs(10), &AtomicBool::new(false));
        assert!(shared.terminal() == Terminal::Complete);
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod quantization {
    use super::*;
    #[test]
    fn integer_steps_and_endpoint_saturation_are_distinct_from_float_rounding() {
        for value in [
            0.0,
            -0.0,
            f64::from(f32::from_bits(1)),
            -f64::from(f32::from_bits(1)),
        ] {
            assert_eq!(i16::from_sample(value), 0);
            assert_eq!(i32::from_sample(value), 0);
        }
        let step16 = 1.0 / 32_768.0;
        let step32 = 1.0 / 2_147_483_648.0;
        for (steps, expected) in [
            (0.0, 0),
            (0.75, 0),
            (-0.75, 0),
            (1.0, 1),
            (-1.0, -1),
            (1.75, 1),
            (-1.75, -1),
        ] {
            assert_eq!(i16::from_sample(steps * step16), expected);
            assert_eq!(i32::from_sample(steps * step32), i32::from(expected));
        }
        // I24 deliberately goes through saturated I32, then discards low bits.
        // Signed right shift floors negative half-steps rather than rounding them.
        let step24 = 1.0 / 8_388_608.0;
        for (steps, expected) in [
            (0.0, 0),
            (0.5, 0),
            (-0.5, -1),
            (1.0, 1),
            (-1.0, -1),
            (1.5, 1),
            (-1.5, -2),
        ] {
            assert_eq!(to_i24(steps * step24).inner(), expected);
        }
        assert_eq!(i16::from_sample(1.0_f64), i16::MAX);
        assert_eq!(i16::from_sample(-1.0_f64), i16::MIN);
        assert_eq!(i32::from_sample(1.0_f64), i32::MAX);
        assert_eq!(i32::from_sample(-1.0_f64), i32::MIN);
        assert_eq!(to_i24(1.0).inner(), 8_388_607);
        assert_eq!(to_i24(-1.0).inner(), -8_388_608);
        for sample in [-1.0, -0.37, -1e-20, 0.0, 1e-20, 0.37, 1.0] {
            assert!((f64::from(i16::from_sample(sample)) * step16 - sample).abs() <= step16);
            assert!((f64::from(i32::from_sample(sample)) * step32 - sample).abs() <= step32);
            assert!((f64::from(to_i24(sample).inner()) * step24 - sample).abs() <= step24 + step32);
        }
    }
    #[test]
    fn real_callback_conversion_preserves_first_two_channels_and_zeros_every_extra_channel() {
        use crate::playback::shared::Shared;
        use auditorylab_core::{Component, Ears, Stimulus};
        let session = Stimulus {
            duration_seconds: 8.0 / 8_000.0,
            fade_seconds: 0.0,
            components: vec![
                Component {
                    ears: Ears::Left,
                    peak_dbfs: 0.0,
                    modulation: None,
                    carrier: crate::Carrier::Tone {
                        frequency_hz: 2_000.0,
                    },
                },
                Component {
                    ears: Ears::Right,
                    peak_dbfs: -20.0,
                    modulation: None,
                    carrier: crate::Carrier::Tone {
                        frequency_hz: 1_000.0,
                    },
                },
            ],
        }
        .signal(SampleRate::new(8_000).unwrap())
        .unwrap();
        let mut callback = Callback::new(session, 5, Arc::new(Shared::default()));
        let mut output = [cpal::I24::from_sample(0.5_f64); 40];
        let now = cpal::StreamInstant::from_nanos(0);
        callback.fill(
            &mut output,
            cpal::OutputStreamTimestamp {
                callback: now,
                playback: now,
            },
            &to_i24,
        );
        assert_eq!(output[5].inner(), 8_388_607);
        assert_eq!(output[15].inner(), -8_388_608);
        assert!(output[11].inner() > 800_000 && output[11].inner() < 850_000);
        for frame in output.as_chunks::<5>().0 {
            assert!(frame[2..].iter().all(|sample| sample.inner() == 0));
        }
    }
}
