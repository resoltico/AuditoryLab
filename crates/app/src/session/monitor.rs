//! Bounded observation of the actual mixed pre-conversion output.
//!
//! Monitoring is optional and best effort. The callback attempts one
//! nonblocking lock per buffer; a held lock skips a publication but never
//! delays audio, Stop, drain or update closure, whose facts live in the
//! session's own atomics.
use super::live::Mailbox;
use super::{OutputInfo, PlaybackPhase, SessionId, SessionState};
use crate::recipe::Origin;
use auditorylab_core::{Observation, SampleRate, Target};
use std::sync::{
    Arc, Mutex, TryLockError,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

/// Immutable source identity, captured before the stream starts.
#[derive(Debug, Clone)]
pub struct SessionDescriptor {
    /// The session's identity.
    pub id: SessionId,
    /// Captured source title.
    pub title: String,
    /// Captured recipe origin.
    pub origin: Origin,
    /// Bound device configuration.
    pub output: OutputInfo,
    /// Finite source length, or None for live playback.
    pub frames: Option<u64>,
    /// The validated source at the device rate, independent of navigation
    /// and terminal fade silence.
    pub target: Target,
}

/// Amount of generated output retained by a consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorDetail {
    /// Quantities and revisions only; no sample history allocation or capture.
    Quantities,
    /// Up to 100 ms of mixed stereo samples as well as quantities.
    Waveform,
}

/// A coherent generated-frame observation, distinct from estimated output position.
#[derive(Debug, Clone, Copy)]
pub struct GeneratedStatus {
    /// Session identity; a reader never rebinds to another session.
    pub session_id: SessionId,
    /// Changes only when a publication's generated frame position changes.
    pub sequence: u64,
    /// Number of generated source frames, from the session's counter.
    pub generated_frames: u64,
    /// The timestamp-based estimate of source frames output.
    pub estimated_output_frames: u64,
    /// Latest accepted whole-target revision, independently of generation.
    pub accepted_revision: u64,
    /// The last published generated frame's actual state, or None before
    /// generation. It can be older than `generated_frames`.
    pub observation: Option<Observation>,
    /// First frame of the copied contiguous sample window.
    pub window_start: u64,
    /// Exclusive end of the copied contiguous sample window.
    pub window_end: u64,
    /// Number of discontinuities in retained coverage.
    pub gaps: u64,
    /// Callback publications skipped because a reader held the guard.
    pub missed_publications: u64,
    /// Backend-reported audio underruns; independent of monitoring gaps.
    pub underruns: u64,
    /// Current session phase.
    pub phase: PlaybackPhase,
    /// Monitoring has been disabled after a poisoned lock.
    pub disabled: bool,
}

#[cfg_attr(
    not(any(feature = "device-playback", test)),
    expect(dead_code, reason = "Only stream callbacks publish history")
)]
struct History {
    samples: Vec<[f64; 2]>,
    head: usize,
    len: usize,
    end: u64,
    gaps: u64,
    sequence: u64,
    frame: u64,
    observation: Option<Observation>,
}

#[cfg(any(feature = "device-playback", test))]
impl History {
    fn append(&mut self, start: u64, frames: &[[f64; 2]]) {
        if frames.is_empty() || self.samples.is_empty() {
            return;
        }
        if start != self.end {
            self.gaps += 1;
            self.len = 0;
            self.head = 0;
        }
        for frame in frames {
            self.samples[self.head] = *frame;
            self.head = (self.head + 1) % self.samples.len();
            self.len = (self.len + 1).min(self.samples.len());
        }
        self.end = start + frames.len() as u64;
    }
}

#[cfg(any(feature = "device-playback", test))]
/// Callback-side writer; one `try_lock` per device buffer, never a blocking lock.
pub(crate) struct Publication<'a>(std::sync::MutexGuard<'a, History>);

#[cfg(any(feature = "device-playback", test))]
impl Publication<'_> {
    pub(crate) fn append(&mut self, start: u64, frames: &[[f64; 2]]) {
        self.0.append(start, frames);
    }

    pub(crate) fn finish(&mut self, frame: u64, observation: Option<Observation>) {
        if frame != self.0.frame {
            self.0.sequence += 1;
        }
        self.0.frame = frame;
        self.0.observation = observation;
    }
}

struct State {
    descriptor: SessionDescriptor,
    detail: MonitorDetail,
    history: Mutex<History>,
    missed: AtomicU64,
    disabled: AtomicBool,
    session: Arc<SessionState>,
    revisions: Option<Mailbox>,
}

/// Handle for one prepared session's generated output.
#[derive(Clone)]
pub struct PlaybackMonitor(Arc<State>);

impl std::fmt::Debug for PlaybackMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaybackMonitor")
            .field("session", &self.0.descriptor.id)
            .field("detail", &self.0.detail)
            .finish_non_exhaustive()
    }
}

impl PlaybackMonitor {
    #[cfg(any(feature = "device-playback", test))]
    pub(crate) fn new(
        descriptor: SessionDescriptor,
        detail: MonitorDetail,
        session: Arc<SessionState>,
        revisions: Option<Mailbox>,
    ) -> Self {
        let capacity = if detail == MonitorDetail::Waveform {
            capacity(descriptor.output.sample_rate)
        } else {
            0
        };
        let monitor = Self(Arc::new(State {
            descriptor,
            detail,
            history: Mutex::new(History {
                samples: vec![[0.0; 2]; capacity],
                head: 0,
                len: 0,
                end: 0,
                gaps: 0,
                sequence: 0,
                frame: 0,
                observation: None,
            }),
            missed: AtomicU64::new(0),
            disabled: AtomicBool::new(false),
            session,
            revisions,
        }));
        // Darwin's std mutex lazily allocates its OS storage. Initialize it here,
        // before the first callback's try_lock, rather than masking that cost in a test.
        drop(
            monitor
                .0
                .history
                .lock()
                .unwrap_or_else(|_| unreachable!("fresh monitor mutex")),
        );
        monitor
    }

    /// Captured session/source identity.
    #[must_use]
    pub fn descriptor(&self) -> &SessionDescriptor {
        &self.0.descriptor
    }

    #[cfg(any(feature = "device-playback", test))]
    pub(crate) fn try_publish(&self) -> Option<Publication<'_>> {
        if self.0.disabled.load(Ordering::Relaxed) {
            return None;
        }
        match self.0.history.try_lock() {
            Ok(guard) => Some(Publication(guard)),
            Err(TryLockError::WouldBlock) => {
                self.0.missed.fetch_add(1, Ordering::Relaxed);
                None
            }
            Err(TryLockError::Poisoned(_)) => {
                self.0.disabled.store(true, Ordering::Relaxed);
                None
            }
        }
    }

    /// Allocate a reusable reader off the callback, preferably before playback starts.
    #[must_use]
    pub fn reader(&self) -> MonitorReader {
        let count = if self.0.detail == MonitorDetail::Waveform {
            capacity(self.0.descriptor.output.sample_rate)
        } else {
            0
        };
        let revision = u64::from(self.0.revisions.is_some());
        MonitorReader {
            accepted: (revision, self.descriptor().target),
            monitor: self.clone(),
            samples: vec![[0.0; 2]; count],
            len: 0,
            status: GeneratedStatus {
                session_id: self.0.descriptor.id,
                sequence: 0,
                generated_frames: 0,
                estimated_output_frames: 0,
                accepted_revision: revision,
                observation: None,
                window_start: 0,
                window_end: 0,
                gaps: 0,
                missed_publications: 0,
                underruns: 0,
                phase: PlaybackPhase::Preparing,
                disabled: false,
            },
            received: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn hold_history(&self) -> std::sync::MutexGuard<'_, impl Sized> {
        self.0
            .history
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(test)]
    pub(crate) fn session(&self) -> Arc<SessionState> {
        Arc::clone(&self.0.session)
    }
}

fn capacity(rate: SampleRate) -> usize {
    usize::try_from(rate.hz().div_ceil(10))
        .unwrap_or(76_800)
        .max(1)
}

/// Freshness of a received generator observation, independent of display holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationFreshness {
    /// No source observation has been received.
    Waiting,
    /// A changed source observation was received within the last 500 ms.
    Current,
    /// No changed source observation was received for over 500 ms.
    Stale,
    /// Monitoring was disabled after a poisoned lock.
    Unavailable,
}

/// Preallocated consumer storage; reduction and formatting happen outside the shared guard.
pub struct MonitorReader {
    monitor: PlaybackMonitor,
    samples: Vec<[f64; 2]>,
    len: usize,
    status: GeneratedStatus,
    received: Option<Instant>,
    accepted: (u64, Target),
}

impl std::fmt::Debug for MonitorReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonitorReader")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl MonitorReader {
    /// Immutable identity of this reader's session.
    #[must_use]
    pub fn descriptor(&self) -> &SessionDescriptor {
        self.monitor.descriptor()
    }

    /// Copy the latest bounded contiguous window. Returns false for lock
    /// contention or disabled monitoring. Session counters, phase and
    /// underruns are refreshed even without a monitor publication.
    pub fn poll(&mut self, window_ms: u32) -> bool {
        let mut copied = false;
        match self.monitor.0.history.try_lock() {
            Ok(h) => {
                let count = usize::try_from(
                    (u64::from(self.descriptor().output.sample_rate.hz())
                        * u64::from(window_ms.min(100)))
                    .div_ceil(1000),
                )
                .unwrap_or(self.samples.len())
                .max(1)
                .min(h.len)
                .min(self.samples.len());
                if h.sequence != self.status.sequence {
                    self.received = Some(Instant::now());
                }
                for i in 0..count {
                    let at = (h.head + h.samples.len() - count + i) % h.samples.len();
                    self.samples[i] = h.samples[at];
                }
                self.len = count;
                self.status.sequence = h.sequence;
                self.status.observation = h.observation;
                self.status.window_end = h.end;
                self.status.window_start = h.end - count as u64;
                self.status.gaps = h.gaps;
                copied = true;
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Poisoned(_)) => {
                self.monitor.0.disabled.store(true, Ordering::Relaxed);
            }
        }
        if let Some(revisions) = &self.monitor.0.revisions
            && let Ok(updates) = revisions.try_lock()
            && let Some(accepted) = updates.accepted
        {
            self.status.accepted_revision = accepted.0;
            self.accepted = accepted;
        }
        let session = &self.monitor.0.session;
        self.status.generated_frames = session.generated();
        self.status.estimated_output_frames = session.estimated_output();
        self.status.missed_publications = self.monitor.0.missed.load(Ordering::Relaxed);
        self.status.underruns = session.underruns();
        self.status.disabled = self.monitor.0.disabled.load(Ordering::Relaxed);
        self.status.phase = session.phase();
        copied
    }

    /// Latest status; generated quantities have their own sequence/frame identity.
    #[must_use]
    pub const fn status(&self) -> GeneratedStatus {
        self.status
    }

    /// Age since receipt of a genuinely changed source observation, not since redraw.
    #[must_use]
    pub fn age(&self) -> Option<Duration> {
        self.received.map(|at| at.elapsed())
    }

    /// Shared freshness policy; polling an unchanged sequence does not refresh it.
    #[must_use]
    pub fn freshness(&self) -> ObservationFreshness {
        if self.status.disabled {
            ObservationFreshness::Unavailable
        } else if self.status.sequence == 0 {
            ObservationFreshness::Waiting
        } else if self
            .age()
            .is_some_and(|age| age > Duration::from_millis(500))
        {
            ObservationFreshness::Stale
        } else {
            ObservationFreshness::Current
        }
    }

    /// The last observed accepted target and revision, refreshed by poll
    /// without a second mailbox lock. Accepted is not a claim that it generated.
    #[must_use]
    pub const fn accepted(&self) -> (u64, Target) {
        self.accepted
    }

    /// Copied actual mixed samples. They are not microphone samples or an acoustic playhead.
    #[must_use]
    pub fn samples(&self) -> &[[f64; 2]] {
        &self.samples[..self.len]
    }
}

/// Measured digital levels for one ear over exactly the supplied window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    /// Maximum absolute sample, in dBFS.
    pub peak_dbfs: f64,
    /// Root-mean-square sample, in dBFS.
    pub rms_dbfs: f64,
}

/// Reduce a copied window outside the monitoring mutex. Silence uses negative infinity.
#[expect(
    clippy::cast_precision_loss,
    reason = "RMS normalization divides by a positive slice length; binary64 count rounding is acceptable for the reported meter and never changes generated samples"
)]
#[must_use]
pub fn sample_meters(samples: &[[f64; 2]]) -> [Meter; 2] {
    let mut peak = [0.0_f64; 2];
    for frame in samples {
        for ear in 0..2 {
            peak[ear] = peak[ear].max(frame[ear].abs());
        }
    }
    let mut normalized = [0.0; 2];
    for frame in samples {
        for ear in 0..2 {
            if peak[ear] > 0.0 {
                let ratio = frame[ear] / peak[ear];
                normalized[ear] += ratio * ratio;
            }
        }
    }
    let db = |v: f64| {
        if v == 0.0 {
            f64::NEG_INFINITY
        } else {
            20.0 * v.log10()
        }
    };
    std::array::from_fn(|ear| Meter {
        peak_dbfs: db(peak[ear]),
        rms_dbfs: if peak[ear] == 0.0 {
            f64::NEG_INFINITY
        } else {
            db(peak[ear]) + 10.0 * (normalized[ear] / samples.len().max(1) as f64).log10()
        },
    })
}

#[cfg(test)]
mod tests;
