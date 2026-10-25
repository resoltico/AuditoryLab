//! Device-independent playback sessions, live generation and monitoring.
//!
//! A [`Session`] is the single owner of one playback lifecycle. It is created
//! before device preparation and consumed by it, so one owner cannot drive
//! two streams. [`PlaybackControl`] handles can be cloned freely; every Stop
//! source holds the control of its particular session.
//!
//! One atomic word arbitrates the lifecycle. Its two active states are
//! Running and StopRequested; the rest are terminal outcomes. Accepting Stop
//! is a compare-and-swap from Running, and the first terminal publication
//! wins. Generation ending is a separate fact that closes live-update
//! admission even while the device drains.
pub(crate) mod live;
mod monitor;
mod output;
#[cfg(any(feature = "device-playback", test))]
pub(crate) mod source;
#[cfg(any(feature = "device-playback", test))]
pub(crate) mod supervisor;
#[cfg(any(feature = "device-playback", test))]
pub(crate) mod transport;

pub use live::{LiveControl, LiveGenerator, LiveUpdateError};
pub use monitor::{
    GeneratedStatus, Meter, MonitorDetail, MonitorReader, ObservationFreshness, PlaybackMonitor,
    SessionDescriptor, sample_meters,
};
pub use output::{OutputInfo, SampleFormat};

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};

/// The process-local identity of one playback session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(u64);

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Fixed lifecycle codes shared with the sample callback. Rich diagnostics
/// stay off that path, owned by whoever published the failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Outcome {
    Running = 0,
    StopRequested = 1,
    Completed = 2,
    StoppedDrained = 3,
    StoppedUndrained = 4,
    FrameLimit = 5,
    PreparationFailed = 6,
    StreamSetupFailed = 7,
    Backend = 8,
    InvalidTiming = 9,
    MisalignedBuffer = 10,
    TimedOut = 11,
    Abandoned = 12,
}

impl Outcome {
    const ALL: [Self; 13] = [
        Self::Running,
        Self::StopRequested,
        Self::Completed,
        Self::StoppedDrained,
        Self::StoppedUndrained,
        Self::FrameLimit,
        Self::PreparationFailed,
        Self::StreamSetupFailed,
        Self::Backend,
        Self::InvalidTiming,
        Self::MisalignedBuffer,
        Self::TimedOut,
        Self::Abandoned,
    ];

    fn decode(code: u8) -> Self {
        Self::ALL
            .into_iter()
            .find(|outcome| *outcome as u8 == code)
            .unwrap_or_else(|| unreachable!("only fixed outcome codes are stored"))
    }

    pub(crate) const fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::StopRequested)
    }

    pub(crate) const fn is_failure(self) -> bool {
        !self.is_active()
            && !matches!(
                self,
                Self::Completed | Self::StoppedDrained | Self::StoppedUndrained | Self::FrameLimit
            )
    }
}

/// State shared by every handle of one session. Counters are written by the
/// stream callback and read by monitors, supervisors and reports.
#[derive(Debug)]
pub(crate) struct SessionState {
    id: SessionId,
    outcome: AtomicU8,
    generated: AtomicU64,
    estimated_output: AtomicU64,
    source_ended: AtomicBool,
    #[cfg(any(feature = "device-playback", test))]
    progress: AtomicU64,
    underruns: AtomicU64,
}

impl SessionState {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            id: SessionId(NEXT_SESSION.fetch_add(1, Ordering::Relaxed)),
            outcome: AtomicU8::new(Outcome::Running as u8),
            generated: AtomicU64::new(0),
            estimated_output: AtomicU64::new(0),
            source_ended: AtomicBool::new(false),
            #[cfg(any(feature = "device-playback", test))]
            progress: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
        })
    }

    pub(crate) const fn id(&self) -> SessionId {
        self.id
    }

    /// The current outcome word, read with Acquire so that a terminal
    /// publication's preceding writes are visible.
    pub(crate) fn outcome(&self) -> Outcome {
        Outcome::decode(self.outcome.load(Ordering::Acquire))
    }

    /// Accept Stop exactly once, while the session is still running.
    pub(crate) fn request_stop(&self) -> StopRequest {
        match self.outcome.compare_exchange(
            Outcome::Running as u8,
            Outcome::StopRequested as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => StopRequest::Accepted,
            Err(code) if Outcome::decode(code) == Outcome::StopRequested => {
                StopRequest::AlreadyRequested
            }
            Err(_) => StopRequest::AlreadyFinished,
        }
    }

    /// Publish a terminal outcome from either active state. The first
    /// terminal publication wins; the returned value is the winner.
    pub(crate) fn finish(&self, terminal: Outcome) -> Outcome {
        debug_assert!(!terminal.is_active());
        let mut current = self.outcome.load(Ordering::Acquire);
        loop {
            if !Outcome::decode(current).is_active() {
                return Outcome::decode(current);
            }
            match self.outcome.compare_exchange_weak(
                current,
                terminal as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return terminal,
                Err(actual) => current = actual,
            }
        }
    }

    /// Publish successful drain: Completed or FrameLimit while running,
    /// Stopped when Stop was accepted first. The choice and the publication
    /// use the same word, so Stop cannot slip between them.
    #[cfg(any(feature = "device-playback", feature = "test-support", test))]
    pub(crate) fn finish_drained(&self, natural: Outcome) -> Outcome {
        let mut current = self.outcome.load(Ordering::Acquire);
        loop {
            let terminal = match Outcome::decode(current) {
                Outcome::Running => natural,
                Outcome::StopRequested => Outcome::StoppedDrained,
                finished => return finished,
            };
            match self.outcome.compare_exchange_weak(
                current,
                terminal as u8,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return terminal,
                Err(actual) => current = actual,
            }
        }
    }

    /// Force the end of an accepted Stop whose drain did not complete.
    #[cfg(any(feature = "device-playback", test))]
    pub(crate) fn finish_stop_undrained(&self) -> Outcome {
        match self.outcome.compare_exchange(
            Outcome::StopRequested as u8,
            Outcome::StoppedUndrained as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Outcome::StoppedUndrained,
            Err(code) => Outcome::decode(code),
        }
    }

    /// Whether live updates are admitted: running, Stop not accepted, and
    /// generation not ended.
    pub(crate) fn admits_updates(&self) -> bool {
        self.outcome() == Outcome::Running && !self.source_ended()
    }

    pub(crate) fn end_source(&self) {
        self.source_ended.store(true, Ordering::Release);
    }

    pub(crate) fn source_ended(&self) -> bool {
        self.source_ended.load(Ordering::Acquire)
    }

    #[cfg(any(feature = "device-playback", test))]
    pub(crate) fn set_generated(&self, frames: u64) {
        self.generated.store(frames, Ordering::Relaxed);
    }

    pub(crate) fn generated(&self) -> u64 {
        self.generated.load(Ordering::Relaxed)
    }

    #[cfg(any(feature = "device-playback", test))]
    pub(crate) fn set_estimated_output(&self, frames: u64) {
        self.estimated_output.store(frames, Ordering::Relaxed);
    }

    pub(crate) fn estimated_output(&self) -> u64 {
        self.estimated_output.load(Ordering::Relaxed)
    }

    #[cfg(any(feature = "device-playback", test))]
    pub(crate) fn mark_progress(&self) {
        self.progress.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(any(feature = "device-playback", test))]
    pub(crate) fn progress(&self) -> u64 {
        self.progress.load(Ordering::Relaxed)
    }

    #[cfg_attr(
        not(any(feature = "device-playback", test)),
        expect(dead_code, reason = "Only device backends report underruns")
    )]
    pub(crate) fn count_underrun(&self) {
        self.underruns.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// The phase, by priority: terminal outcome; generation ended (draining);
    /// Stop accepted; frames generated; otherwise preparing.
    pub(crate) fn phase(&self) -> PlaybackPhase {
        match self.outcome() {
            Outcome::Completed => PlaybackPhase::Completed,
            Outcome::FrameLimit => PlaybackPhase::FrameLimit,
            Outcome::StoppedDrained | Outcome::StoppedUndrained => PlaybackPhase::Stopped,
            outcome if outcome.is_failure() => PlaybackPhase::Failed,
            _ if self.source_ended() => PlaybackPhase::Draining,
            Outcome::StopRequested => PlaybackPhase::Stopping,
            _ if self.generated() > 0 => PlaybackPhase::Running,
            _ => PlaybackPhase::Preparing,
        }
    }

    /// The report of a successful terminal outcome, read after the stream
    /// is torn down; `None` while active or after a failure.
    #[cfg(any(feature = "device-playback", feature = "test-support", test))]
    pub(crate) fn report(&self) -> Option<PlaybackReport> {
        let (end, drained) = match self.outcome() {
            Outcome::Completed => (EndReason::Completed, true),
            Outcome::FrameLimit => (EndReason::FrameLimit, true),
            Outcome::StoppedDrained => (EndReason::Stopped, true),
            Outcome::StoppedUndrained => (EndReason::Stopped, false),
            _ => return None,
        };
        Some(PlaybackReport {
            end,
            generated_frames: self.generated(),
            estimated_output_frames: self.estimated_output(),
            underruns: self.underruns(),
            drained,
        })
    }
}

/// The unique owner of one playback lifecycle. It is not `Clone`: device
/// preparation consumes it, and the prepared playback owns it until its
/// stream is torn down. Dropping an owner whose lifecycle is still active
/// closes it, so surviving controls cannot keep admitting work.
#[derive(Debug)]
pub struct Session {
    state: Arc<SessionState>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// Allocate a new session identity in the Running state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SessionState::new(),
        }
    }

    /// The session's identity.
    #[must_use]
    pub fn id(&self) -> SessionId {
        self.state.id()
    }

    /// A handle that addresses this session.
    #[must_use]
    pub fn control(&self) -> PlaybackControl {
        PlaybackControl {
            state: Arc::clone(&self.state),
        }
    }

    #[cfg(any(feature = "device-playback", feature = "test-support", test))]
    pub(crate) fn state(&self) -> &Arc<SessionState> {
        &self.state
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Existing terminal outcomes are never replaced.
        self.state.finish(Outcome::Abandoned);
    }
}

/// A cloneable handle to one session's lifecycle.
#[derive(Debug, Clone)]
pub struct PlaybackControl {
    state: Arc<SessionState>,
}

impl PlaybackControl {
    /// The session this handle addresses.
    #[must_use]
    pub fn session(&self) -> SessionId {
        self.state.id()
    }

    /// Request Stop. Acceptance closes live-update admission at once; audio
    /// already generated or queued may still play.
    pub fn request_stop(&self) -> StopRequest {
        self.state.request_stop()
    }

    /// The session's current phase.
    #[must_use]
    pub fn phase(&self) -> PlaybackPhase {
        self.state.phase()
    }

    /// Source frames generated so far, including a Stop fade.
    #[must_use]
    pub fn generated_frames(&self) -> u64 {
        self.state.generated()
    }

    /// The timestamp-based estimate of source frames output so far.
    #[must_use]
    pub fn estimated_output_frames(&self) -> u64 {
        self.state.estimated_output()
    }
}

/// The result of a Stop request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopRequest {
    /// This request accepted Stop for a running session.
    Accepted,
    /// Stop had already been accepted.
    AlreadyRequested,
    /// The session had already reached a terminal outcome.
    AlreadyFinished,
}

/// Why a successful playback ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// A finite source played to its end.
    Completed,
    /// Stop was accepted before completion was published.
    Stopped,
    /// A live source reached the longest supported session.
    FrameLimit,
}

/// What a successful playback did. Device timing is not an acoustic
/// measurement, and generated frames are not a claim that all were heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackReport {
    /// Why playback ended.
    pub end: EndReason,
    /// Source frames generated, including any Stop fade.
    pub generated_frames: u64,
    /// The final timestamp-based estimate of source frames output.
    pub estimated_output_frames: u64,
    /// Backend-reported underruns.
    pub underruns: u64,
    /// Whether the queued end passed the conservative timing condition, or
    /// nothing was ever queued.
    pub drained: bool,
}

/// The result of device preparation: ready to play, or stopped before any
/// stream existed.
#[derive(Debug)]
pub enum Preparation<P> {
    /// Prepared playback, ready to start.
    Ready(P),
    /// Stop was observed before stream start was admitted.
    Stopped(PlaybackReport),
}

/// A session's lifecycle as presented, derived from its shared state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackPhase {
    /// No frames generated yet.
    Preparing,
    /// Generating frames.
    Running,
    /// Stop accepted; generation has not ended.
    Stopping,
    /// Generation ended; queued output is draining.
    Draining,
    /// A finite source completed.
    Completed,
    /// Playback stopped by request.
    Stopped,
    /// A live source reached the frame limit.
    FrameLimit,
    /// Playback failed.
    Failed,
}

impl PlaybackPhase {
    /// Whether the session has reached a terminal outcome.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Stopped | Self::FrameLimit | Self::Failed
        )
    }
}

#[cfg(test)]
mod tests;
