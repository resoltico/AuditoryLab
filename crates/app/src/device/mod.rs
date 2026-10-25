// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Default-device playback through CPAL.
//!
//! Preparation discovers the default output, validates the source at the
//! device's actual rate and returns a prepared playback, or a stopped report
//! when Stop was accepted before stream start was admitted. Playback runs on
//! the calling thread until the session reaches a terminal outcome; the
//! stream is torn down before its report is read.
use crate::presentation::AudioErrorText;
mod error;

use crate::editing::Draft;
use crate::error::{Error, ErrorKind};
use crate::recipe::{Origin, RecipeSnapshot, Rendition};
use crate::session::supervisor::{POLL_INTERVAL, STALL_LIMIT, Supervision, time_limit};
use crate::session::transport::{BufferTime, Transport};
use crate::session::{
    LiveControl, LiveGenerator, MonitorDetail, Outcome, OutputInfo, PlaybackControl,
    PlaybackMonitor, PlaybackReport, Preparation, SampleFormat, Session, SessionDescriptor,
    source::{FiniteSource, Source},
};
use auditorylab_core::{SampleRate, Signal};
use cpal::{
    OutputCallbackInfo, Sample, SizedSample, Stream, StreamConfig, StreamInstant,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
pub(crate) use error::DeviceError;
use error::Diagnostics;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

// ALSA checks missed fault notifications when its poll expires. Give recovery
// time before the no-progress watchdog; other backends use this argument for
// setup/activation and retain their existing timeout.
const STREAM_TIMEOUT: Duration = if cfg!(target_os = "linux") {
    Duration::from_secs(1)
} else {
    STALL_LIMIT
};

/// The default output device and its backend-reported stream configuration.
struct Output {
    device: cpal::Device,
    config: StreamConfig,
    description: OutputInfo,
}

impl Output {
    fn default_device() -> Result<Self, DeviceError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(DeviceError::NoDefaultOutput)?;
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

    fn play(
        self,
        session: &Session,
        source: Source,
        frames: Option<u64>,
        monitor: Option<PlaybackMonitor>,
    ) -> Result<PlaybackReport, Error> {
        let state = Arc::clone(session.state());
        if let Some(report) = stopped_before_start(session) {
            return Ok(report);
        }
        let diagnostics = Arc::new(Diagnostics::new(Arc::clone(&state)));
        let transport =
            Transport::new(source, self.config.channels, Arc::clone(&state)).with_monitor(monitor);
        // Keep synchronous diagnostics on this worker, before publishing their
        // fixed outcome. An asynchronous writer can be paused independently.
        let mut setup_error = None;
        let terminal = match self.stream(transport, &diagnostics) {
            Ok(stream) => {
                let terminal = match stream.play() {
                    Ok(()) => supervise(
                        &state,
                        frames.map(|n| time_limit(n, self.description.sample_rate)),
                    ),
                    Err(error) => setup_failed(&state, error, &mut setup_error),
                };
                drop(stream);
                terminal
            }
            Err(error) => setup_failed(&state, error, &mut setup_error),
        };
        resolve(&state, terminal, setup_error, &diagnostics).map_err(device_error)
    }

    fn stream(
        &self,
        transport: Transport,
        diagnostics: &Arc<Diagnostics>,
    ) -> Result<Stream, cpal::Error> {
        match self.description.format {
            SampleFormat::F64 => self.stream_of(transport, diagnostics, std::convert::identity),
            SampleFormat::F32 => self.stream_of(transport, diagnostics, f32::from_sample),
            SampleFormat::I32 => self.stream_of(transport, diagnostics, i32::from_sample),
            SampleFormat::I24 => self.stream_of(transport, diagnostics, to_i24),
            SampleFormat::I16 => self.stream_of(transport, diagnostics, i16::from_sample),
        }
    }

    fn stream_of<T: SizedSample>(
        &self,
        mut transport: Transport,
        diagnostics: &Arc<Diagnostics>,
        convert: impl Fn(f64) -> T + Send + 'static,
    ) -> Result<Stream, cpal::Error> {
        let reports = Arc::clone(diagnostics);
        self.device.build_output_stream(
            self.config,
            move |output: &mut [T], info: &OutputCallbackInfo| {
                let timestamp = info.timestamp();
                transport.fill(
                    output,
                    BufferTime {
                        callback: timestamp.callback.duration_since(StreamInstant::ZERO),
                        playback: timestamp.playback.duration_since(StreamInstant::ZERO),
                    },
                    &convert,
                );
            },
            move |error| reports.report(error),
            Some(STREAM_TIMEOUT),
        )
    }
}

/// A finite source bound to the default device at the device's rate.
pub struct PreparedPlayback {
    output: Output,
    sound: Rendition,
    session: Session,
    descriptor: SessionDescriptor,
    monitor: Option<PlaybackMonitor>,
}

/// Discover the default output and validate the snapshot's mix and finite
/// envelope at the device's actual rate.
///
/// Stop is checked before discovery and after validation; when accepted
/// there, the result is a stopped report and no stream is created.
///
/// # Errors
/// Returns [`ErrorKind::Device`] for discovery failures and
/// [`ErrorKind::InvalidSound`] when the snapshot is not valid at the device
/// rate. The session's outcome becomes a preparation failure.
pub fn prepare_playback(
    session: Session,
    snapshot: &RecipeSnapshot,
) -> Result<Preparation<PreparedPlayback>, Error> {
    if let Some(report) = stopped_before_start(&session) {
        return Ok(Preparation::Stopped(report));
    }
    let output = Output::default_device().map_err(|e| failed(&session, device_error(e)))?;
    let sound = snapshot
        .at(output.description.sample_rate)
        .map_err(|e| failed(&session, e))?;
    if let Some(report) = stopped_before_start(&session) {
        return Ok(Preparation::Stopped(report));
    }
    let descriptor = SessionDescriptor {
        id: session.id(),
        title: snapshot.title(),
        origin: snapshot.origin().clone(),
        output: output.description.clone(),
        frames: Some(sound.signal().frames()),
        target: *sound.signal().target(),
    };
    Ok(Preparation::Ready(PreparedPlayback {
        output,
        sound,
        session,
        descriptor,
        monitor: None,
    }))
}

impl PreparedPlayback {
    /// Captured identity independent of frontend navigation.
    #[must_use]
    pub const fn descriptor(&self) -> &SessionDescriptor {
        &self.descriptor
    }

    /// Enable bounded observation before playback. The first call fixes the
    /// detail mode; later calls reuse that monitor.
    pub fn monitor(&mut self, detail: MonitorDetail) -> PlaybackMonitor {
        self.monitor
            .get_or_insert_with(|| {
                PlaybackMonitor::new(
                    self.descriptor.clone(),
                    detail,
                    Arc::clone(self.session.state()),
                    None,
                )
            })
            .clone()
    }

    /// The default output snapshot.
    #[must_use]
    pub const fn output(&self) -> &OutputInfo {
        &self.output.description
    }

    /// The source validated at the device's rate.
    #[must_use]
    pub fn signal(&self) -> &Signal {
        self.sound.signal()
    }

    /// A handle to this session's lifecycle.
    #[must_use]
    pub fn control(&self) -> PlaybackControl {
        self.session.control()
    }

    /// Play on this thread until completion, Stop or failure.
    ///
    /// # Errors
    /// Returns [`ErrorKind::Device`] for stream setup, backend, timing and
    /// watchdog failures.
    pub fn play(self) -> Result<PlaybackReport, Error> {
        let source = Source::Finite(FiniteSource::new(self.sound.shared_signal()));
        let frames = Some(self.sound.signal().frames());
        self.output
            .play(&self.session, source, frames, self.monitor)
    }
}

/// An open-ended live draft bound to the default device at its rate.
pub struct PreparedLive {
    output: Output,
    generator: Box<LiveGenerator>,
    session: Session,
    descriptor: SessionDescriptor,
    monitor: Option<PlaybackMonitor>,
}

/// Discover the default output and compile the draft's components at the
/// device's actual rate. The finite duration and fade are not part of live
/// playback.
///
/// # Errors
/// Returns [`ErrorKind::Device`] for discovery failures and
/// [`ErrorKind::InvalidSound`] when the components are not valid at the
/// device rate. The session's outcome becomes a preparation failure.
pub fn prepare_live(session: Session, draft: &Draft) -> Result<Preparation<PreparedLive>, Error> {
    if let Some(report) = stopped_before_start(&session) {
        return Ok(Preparation::Stopped(report));
    }
    let output = Output::default_device().map_err(|e| failed(&session, device_error(e)))?;
    let rate = output.description.sample_rate;
    let target = draft.check(rate).into_result().map_err(|e| {
        failed(
            &session,
            Error::caused(ErrorKind::InvalidSound, e.at_rate(draft.title(), rate), e),
        )
    })?;
    if let Some(report) = stopped_before_start(&session) {
        return Ok(Preparation::Stopped(report));
    }
    let generator = Box::new(LiveGenerator::with_state(
        target,
        Arc::clone(session.state()),
    ));
    let descriptor = SessionDescriptor {
        id: session.id(),
        title: draft.title().to_owned(),
        origin: Origin::Draft(draft.title().to_owned()),
        output: output.description.clone(),
        frames: None,
        target,
    };
    Ok(Preparation::Ready(PreparedLive {
        output,
        generator,
        session,
        descriptor,
        monitor: None,
    }))
}

impl PreparedLive {
    /// Captured source and device identity.
    #[must_use]
    pub const fn descriptor(&self) -> &SessionDescriptor {
        &self.descriptor
    }

    /// Enable bounded generated observation before playback. The first call
    /// fixes its detail mode.
    pub fn monitor(&mut self, detail: MonitorDetail) -> PlaybackMonitor {
        self.monitor
            .get_or_insert_with(|| {
                PlaybackMonitor::new(
                    self.descriptor.clone(),
                    detail,
                    Arc::clone(self.session.state()),
                    Some(Arc::clone(self.generator.mailbox())),
                )
            })
            .clone()
    }

    /// Snapshot of the default output used for this stream.
    #[must_use]
    pub const fn output(&self) -> &OutputInfo {
        &self.output.description
    }

    /// The device rate every update must be compiled at.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.output.description.sample_rate
    }

    /// A handle to this session's lifecycle.
    #[must_use]
    pub fn control(&self) -> PlaybackControl {
        self.session.control()
    }

    /// A handle for accepted target changes while playback runs.
    #[must_use]
    pub fn live_control(&self) -> LiveControl {
        self.generator.control()
    }

    /// Play until Stop, the frame limit or an output failure.
    ///
    /// # Errors
    /// Returns [`ErrorKind::Device`] for stream setup, backend, timing and
    /// watchdog failures.
    pub fn play(self) -> Result<PlaybackReport, Error> {
        let source = Source::Live(self.generator);
        self.output.play(&self.session, source, None, self.monitor)
    }
}

/// Publish an early Stopped report when Stop was accepted before stream
/// start was admitted.
fn stopped_before_start(session: &Session) -> Option<PlaybackReport> {
    let state = session.state();
    match state.outcome() {
        Outcome::Running => None,
        Outcome::StopRequested => {
            state.finish_drained(Outcome::Completed);
            state.report()
        }
        _ => state.report(),
    }
}

fn failed(session: &Session, error: Error) -> Error {
    session.state().finish(Outcome::PreparationFailed);
    error
}

fn device_error(error: DeviceError) -> Error {
    Error::boxed(ErrorKind::Device, error.to_string(), Box::new(error))
}

/// Quantize through a saturated 32-bit integer: the direct conversion maps
/// positive full scale one step beyond the 24-bit range.
fn to_i24(sample: f64) -> cpal::I24 {
    cpal::I24::from_sample(i32::from_sample(sample))
}

const fn sample_format(format: cpal::SampleFormat) -> Option<SampleFormat> {
    match format {
        cpal::SampleFormat::F64 => Some(SampleFormat::F64),
        cpal::SampleFormat::F32 => Some(SampleFormat::F32),
        cpal::SampleFormat::I32 => Some(SampleFormat::I32),
        cpal::SampleFormat::I24 => Some(SampleFormat::I24),
        cpal::SampleFormat::I16 => Some(SampleFormat::I16),
        _ => None,
    }
}

/// Validate the single discovery snapshot without opening a stream.
fn supported_format(
    supported: &cpal::SupportedStreamConfig,
) -> Result<(SampleRate, SampleFormat), DeviceError> {
    let format = sample_format(supported.sample_format()).ok_or(
        DeviceError::UnsupportedSampleFormat(supported.sample_format()),
    )?;
    let hz = supported.sample_rate();
    let sample_rate = SampleRate::new(hz).map_err(|_| DeviceError::UnsupportedSampleRate { hz })?;
    if supported.channels() < 2 {
        return Err(DeviceError::TooFewChannels {
            channels: supported.channels(),
        });
    }
    Ok((sample_rate, format))
}

fn supervise(state: &crate::session::SessionState, limit: Option<Duration>) -> Outcome {
    let started = Instant::now();
    let mut supervision = Supervision::new(limit);
    loop {
        if let Some(outcome) = supervision.poll(state, started.elapsed()) {
            return outcome;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn setup_failed(
    state: &crate::session::SessionState,
    error: cpal::Error,
    local: &mut Option<cpal::Error>,
) -> Outcome {
    *local = Some(error);
    state.finish(Outcome::StreamSetupFailed)
}

/// Resolve only on the worker, after stream teardown. Backend payload storage
/// precedes its terminal publication; setup diagnostics never enter that slot.
fn resolve(
    state: &crate::session::SessionState,
    terminal: Outcome,
    setup_error: Option<cpal::Error>,
    diagnostics: &Diagnostics,
) -> Result<PlaybackReport, DeviceError> {
    match terminal {
        Outcome::Backend => Err(DeviceError::Backend(diagnostics.backend_error())),
        Outcome::StreamSetupFailed => match setup_error {
            Some(error) => Err(DeviceError::Backend(error)),
            None => unreachable!("setup failure retains its worker-local error"),
        },
        Outcome::InvalidTiming => Err(DeviceError::InvalidTiming),
        Outcome::MisalignedBuffer => Err(DeviceError::MisalignedBuffer),
        Outcome::TimedOut => Err(DeviceError::TimedOut),
        Outcome::PreparationFailed | Outcome::Abandoned => Err(DeviceError::Abandoned),
        _ => state.report().ok_or_else(|| {
            unreachable!("supervision and setup failure return a published outcome")
        }),
    }
}

#[cfg(test)]
mod tests;
