//! Bounded, lock-free observation of actual mixed pre-conversion output.
//!
//! A prepared stream and its reader each claim one queue endpoint before audio
//! starts. The callback publishes Copy sample records to an SPSC ring and its
//! latest coherent generated state to a triple buffer. A full sample ring drops
//! new samples and records a coverage gap; it never delays generation or Stop.
use super::live::Mailbox;
use super::{OutputInfo, PlaybackPhase, SessionId, SessionState};
use crate::recipe::Origin;
use auditorylab_core::{Observation, SampleRate, Target};
use std::sync::{Arc, Mutex};
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
    /// Backend-reported audio underruns; independent of monitoring gaps.
    pub underruns: u64,
    /// Current session phase.
    pub phase: PlaybackPhase,
}

#[derive(Clone, Copy)]
struct Sample {
    frame: u64,
    value: [f64; 2],
}

#[derive(Clone, Copy, Default)]
struct Published {
    sequence: u64,
    frame: u64,
    observation: Option<Observation>,
    gaps: u64,
}

struct Endpoints {
    samples_in: Option<rtrb::Producer<Sample>>,
    samples_out: Option<rtrb::Consumer<Sample>>,
    state_in: Option<triple_buffer::Input<Published>>,
    state_out: Option<triple_buffer::Output<Published>>,
}

struct History {
    samples: Vec<[f64; 2]>,
    head: usize,
    len: usize,
    end: u64,
}
impl History {
    fn push(&mut self, sample: Sample) {
        if self.samples.is_empty() {
            return;
        }
        if sample.frame != self.end {
            self.len = 0;
            self.head = 0;
        }
        self.samples[self.head] = sample.value;
        self.head = (self.head + 1) % self.samples.len();
        self.len = (self.len + 1).min(self.samples.len());
        self.end = sample.frame + 1;
    }
}

pub(crate) struct MonitorWriter {
    samples: Option<rtrb::Producer<Sample>>,
    state: triple_buffer::Input<Published>,
    published: Published,
    next_sample: Option<u64>,
    dropping: bool,
}
impl MonitorWriter {
    pub(crate) fn append(&mut self, start: u64, frames: &[[f64; 2]]) {
        let Some(samples) = &mut self.samples else {
            return;
        };
        if frames.is_empty() {
            return;
        }
        if self.next_sample.is_some_and(|next| next != start) && !self.dropping {
            self.published.gaps += 1;
            self.dropping = true;
        }
        for (offset, value) in frames.iter().enumerate() {
            let frame = start + offset as u64;
            match samples.push(Sample {
                frame,
                value: *value,
            }) {
                Ok(()) => self.dropping = false,
                Err(_) if !self.dropping => {
                    self.published.gaps += 1;
                    self.dropping = true;
                }
                Err(_) => {}
            }
        }
        self.next_sample = Some(start + frames.len() as u64);
    }
    pub(crate) fn finish(&mut self, frame: u64, observation: Option<Observation>) {
        if frame != self.published.frame {
            self.published.sequence += 1;
        }
        self.published.frame = frame;
        self.published.observation = observation;
        self.state.write(self.published);
    }
}

struct State {
    descriptor: SessionDescriptor,
    detail: MonitorDetail,
    // Setup only: neither endpoint claim happens on the sample callback.
    endpoints: Mutex<Endpoints>,
    session: Arc<SessionState>,
    revisions: Option<Mailbox>,
}

/// Cloneable identity/setup handle for one prepared monitoring stream.
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
    pub(crate) fn new(
        descriptor: SessionDescriptor,
        detail: MonitorDetail,
        session: Arc<SessionState>,
        revisions: Option<Mailbox>,
    ) -> Self {
        let (samples_in, samples_out) = if detail == MonitorDetail::Waveform {
            let (input, output) = rtrb::RingBuffer::new(capacity(descriptor.output.sample_rate));
            (Some(input), Some(output))
        } else {
            (None, None)
        };
        let (state_in, state_out) = triple_buffer::triple_buffer(&Published::default());
        Self(Arc::new(State {
            descriptor,
            detail,
            endpoints: Mutex::new(Endpoints {
                samples_in,
                samples_out,
                state_in: Some(state_in),
                state_out: Some(state_out),
            }),
            session,
            revisions,
        }))
    }
    /// Captured session/source identity.
    #[must_use]
    pub fn descriptor(&self) -> &SessionDescriptor {
        &self.0.descriptor
    }

    pub(crate) fn writer(&self) -> Option<MonitorWriter> {
        let mut endpoints = self
            .0
            .endpoints
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = endpoints.state_in.take()?;
        Some(MonitorWriter {
            samples: endpoints.samples_in.take(),
            state,
            published: Published::default(),
            next_sample: None,
            dropping: false,
        })
    }
    /// Claim the session's sole reader and allocate its bounded history off the callback.
    /// Returns None if another reader already owns the consumer endpoints.
    #[must_use]
    pub fn reader(&self) -> Option<MonitorReader> {
        let mut endpoints = self
            .0
            .endpoints
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = endpoints.state_out.take()?;
        let count = if self.0.detail == MonitorDetail::Waveform {
            capacity(self.descriptor().output.sample_rate)
        } else {
            0
        };
        let revision = u64::from(self.0.revisions.is_some());
        Some(MonitorReader {
            accepted: (revision, self.descriptor().target),
            monitor: self.clone(),
            input: endpoints.samples_out.take(),
            state,
            published_frames: 0,
            history: History {
                samples: vec![[0.0; 2]; count],
                head: 0,
                len: 0,
                end: 0,
            },
            samples: vec![[0.0; 2]; count],
            len: 0,
            status: GeneratedStatus {
                session_id: self.descriptor().id,
                sequence: 0,
                generated_frames: 0,
                estimated_output_frames: 0,
                accepted_revision: revision,
                observation: None,
                window_start: 0,
                window_end: 0,
                gaps: 0,
                underruns: 0,
                phase: PlaybackPhase::Preparing,
            },
            received: None,
        })
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
}

/// One consumer's preallocated history, copied display window and coherent state.
pub struct MonitorReader {
    monitor: PlaybackMonitor,
    input: Option<rtrb::Consumer<Sample>>,
    state: triple_buffer::Output<Published>,
    published_frames: u64,
    history: History,
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
    /// Read a coherent state watermark and drain its available sample prefix into owned history.
    /// Session counters, phase and underruns are refreshed even without a new observation.
    /// Returns whether samples or a changed generated state were received.
    pub fn poll(&mut self, window_ms: u32) -> bool {
        // Freeze a coherent state watermark before draining, so a concurrent
        // callback cannot make a newly copied window perpetually lag its state.
        let published = *self.state.read();
        let mut changed = false;
        if let Some(input) = &mut self.input {
            // A bounded prefix excludes samples whose state is not published yet.
            for _ in 0..input.slots() {
                if !input
                    .peek()
                    .is_ok_and(|sample| sample.frame < published.frame)
                {
                    break;
                }
                if let Ok(sample) = input.pop() {
                    self.history.push(sample);
                    changed = true;
                }
            }
        }
        if published.sequence != self.status.sequence {
            self.received = Some(Instant::now());
            changed = true;
        }
        self.status.sequence = published.sequence;
        self.status.observation = published.observation;
        self.status.gaps = published.gaps;
        self.published_frames = published.frame;
        let h = &self.history;
        let count = usize::try_from(
            (u64::from(self.descriptor().output.sample_rate.hz()) * u64::from(window_ms.min(100)))
                .div_ceil(1000),
        )
        .unwrap_or(self.samples.len())
        .max(1)
        .min(h.len)
        .min(self.samples.len());
        for i in 0..count {
            self.samples[i] = h.samples[(h.head + h.samples.len() - count + i) % h.samples.len()];
        }
        self.len = count;
        self.status.window_end = h.end;
        self.status.window_start = h.end - count as u64;
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
        self.status.underruns = session.underruns();
        self.status.phase = session.phase();
        changed
    }
    pub(crate) fn has_current_window(&self) -> bool {
        self.len > 0 && self.history.end == self.published_frames
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
        if self.status.sequence == 0 {
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
    /// Last observed accepted target/revision, distinct from what generated.
    #[must_use]
    pub const fn accepted(&self) -> (u64, Target) {
        self.accepted
    }
    /// Actual mixed samples from the copied contiguous window.
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

/// Reduce a copied window outside the callback. Silence uses negative infinity.
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
