//! Bounded observation of the actual mixed pre-conversion output.
use super::shared::{Shared, Terminal};
use crate::{Origin, OutputInfo, SampleRate};
use auditorylab_core::Observation;
use std::sync::{
    Arc, Mutex, MutexGuard, TryLockError,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);
/// Immutable source identity, captured before the stream starts.
#[derive(Debug, Clone)]
pub struct SessionDescriptor {
    /// Unique process-local session identifier.
    pub id: u64,
    /// Captured source title.
    pub title: String,
    /// Captured recipe origin.
    pub origin: Origin,
    /// Bound device configuration.
    pub output: OutputInfo,
    /// Finite source length, or None for live playback.
    pub frames: Option<u64>,
    /// Initial validated source, independent of navigation and terminal fade silence.
    pub target: auditorylab_core::live::Target,
}
impl SessionDescriptor {
    pub(crate) fn new(
        title: String,
        origin: Origin,
        output: OutputInfo,
        frames: Option<u64>,
        target: auditorylab_core::live::Target,
    ) -> Self {
        Self {
            id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            title,
            origin,
            output,
            frames,
            target,
        }
    }
}
/// Amount of generated output retained by a consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorDetail {
    /// Quantities/revisions only; no sample history allocation or capture.
    Quantities,
    /// Up to 100ms of mixed stereo samples as well as quantities.
    Waveform,
}
/// Playback state derived from the existing terminal/stop authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackPhase {
    /// No frames generated yet.
    Preparing,
    /// Generating frames.
    Running,
    /// Stop requested; generator fade has not finished.
    Stopping,
    /// Generation finished; backend drain is pending.
    Draining,
    /// Finite playback completed.
    Completed,
    /// Playback stopped by request.
    Stopped,
    /// Playback failed.
    Failed,
}
/// A coherent source-frame observation, distinct from estimated output position.
#[derive(Debug, Clone, Copy)]
pub struct GeneratedStatus {
    /// Session identity; a reader never rebinds to another session.
    pub session_id: u64,
    /// Changes only when the generated frame position changes.
    pub sequence: u64,
    /// Number of generated source frames.
    pub generated_frames: u64,
    /// Latest accepted whole-target revision for this session, independently of generation.
    pub accepted_revision: u64,
    /// Last generated frame's actual generator state, or None before generation.
    pub observation: Option<Observation>,
    /// First frame of the copied contiguous sample window.
    pub window_start: u64,
    /// Exclusive end of the copied contiguous sample window.
    pub window_end: u64,
    /// Number of discontinuities in retained coverage.
    pub gaps: u64,
    /// Callback publications skipped because the reader held the guard.
    pub missed_publications: u64,
    /// Backend-reported audio underruns; independent of monitoring gaps.
    pub underruns: u64,
    /// Current authoritative playback phase.
    pub phase: PlaybackPhase,
    /// Monitoring has been disabled after a poisoned lock.
    pub disabled: bool,
}
struct History {
    samples: Vec<[f64; 2]>,
    head: usize,
    len: usize,
    end: u64,
    gaps: u64,
    sequence: u64,
    frame: u64,
    observation: Option<Observation>,
    finished: bool,
}
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
/// Callback-side writer; one try_lock per device buffer, never a blocking lock.
pub(crate) struct Publication<'a>(MutexGuard<'a, History>);
impl Publication<'_> {
    pub(crate) fn append(&mut self, start: u64, frames: &[[f64; 2]]) {
        self.0.append(start, frames);
    }
    pub(crate) fn finish(&mut self, frame: u64, observation: Option<Observation>, finished: bool) {
        if frame != self.0.frame {
            self.0.sequence += 1;
        }
        self.0.frame = frame;
        self.0.observation = observation;
        self.0.finished = finished;
    }
}
struct State {
    descriptor: SessionDescriptor,
    detail: MonitorDetail,
    history: Mutex<History>,
    missed: AtomicU64,
    disabled: AtomicBool,
    shared: Arc<Shared>,
    revisions: Option<super::source::Mailbox>,
}
/// Handle for a single prepared playback session's generated output.
#[derive(Clone)]
pub struct PlaybackMonitor(Arc<State>);
impl PlaybackMonitor {
    pub(crate) fn new(
        descriptor: SessionDescriptor,
        detail: MonitorDetail,
        revisions: Option<super::source::Mailbox>,
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
                finished: false,
            }),
            missed: AtomicU64::new(0),
            disabled: AtomicBool::new(false),
            shared: Arc::default(),
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
    pub fn descriptor(&self) -> &SessionDescriptor {
        &self.0.descriptor
    }
    pub(crate) fn shared(&self) -> Arc<Shared> {
        Arc::clone(&self.0.shared)
    }
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
}
fn capacity(rate: SampleRate) -> usize {
    usize::try_from(rate.hz().div_ceil(10))
        .unwrap_or(76_800)
        .max(1)
}
/// Freshness of a received generator observation, independent of display freezing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationFreshness {
    /// No source observation has been received.
    Waiting,
    /// A changed source observation was received within the last 500ms.
    Current,
    /// No changed source observation was received for over 500ms.
    Stale,
    /// Monitoring was disabled after a poisoned lock.
    Unavailable,
}
/// Preallocated consumer storage; reduction/formatting happen outside the shared guard.
pub struct MonitorReader {
    monitor: PlaybackMonitor,
    samples: Vec<[f64; 2]>,
    len: usize,
    status: GeneratedStatus,
    received: Option<Instant>,
    accepted: (u64, auditorylab_core::live::Target),
}
impl MonitorReader {
    /// Immutable identity of this reader's session.
    pub fn descriptor(&self) -> &SessionDescriptor {
        self.monitor.descriptor()
    }
    /// Copy the latest bounded contiguous window. False means lock contention or disabled monitoring.
    /// Existing output/terminal authority is refreshed even without a monitor publication.
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
                self.status.generated_frames = h.frame;
                self.status.observation = h.observation;
                self.status.window_end = h.end;
                self.status.window_start = h.end - count as u64;
                self.status.gaps = h.gaps;
                self.status.phase = if h.finished {
                    PlaybackPhase::Draining
                } else if h.frame > 0 {
                    PlaybackPhase::Running
                } else {
                    PlaybackPhase::Preparing
                };
                copied = true;
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Poisoned(_)) => {
                self.monitor.0.disabled.store(true, Ordering::Relaxed);
            }
        }
        if let Some(revisions) = &self.monitor.0.revisions
            && let Ok(revisions) = revisions.try_lock()
            && let Some(target) = revisions.last
        {
            self.status.accepted_revision = revisions.revision;
            self.accepted = (revisions.revision, target);
        }
        let shared = &self.monitor.0.shared;
        self.status.generated_frames = self.status.generated_frames.max(shared.frames());
        self.status.missed_publications = self.monitor.0.missed.load(Ordering::Relaxed);
        self.status.underruns = shared.underruns();
        self.status.disabled = self.monitor.0.disabled.load(Ordering::Relaxed);
        self.status.phase = match shared.terminal() {
            Terminal::Complete if shared.stop_requested() => PlaybackPhase::Stopped,
            Terminal::Complete => PlaybackPhase::Completed,
            Terminal::Cancelled => PlaybackPhase::Stopped,
            Terminal::Running
                if shared.stop_requested() && self.status.phase != PlaybackPhase::Draining =>
            {
                PlaybackPhase::Stopping
            }
            Terminal::Running
                if self.status.phase == PlaybackPhase::Preparing
                    && self.status.generated_frames > 0 =>
            {
                PlaybackPhase::Running
            }
            Terminal::Running => self.status.phase,
            _ => PlaybackPhase::Failed,
        };
        copied
    }
    /// Latest status; generated quantities have their own sequence/frame identity.
    pub const fn status(&self) -> GeneratedStatus {
        self.status
    }
    /// Age since receipt of a genuinely changed source observation, not since redraw.
    pub fn age(&self) -> Option<Duration> {
        self.received.map(|at| at.elapsed())
    }
    /// Shared freshness policy; redrawing or polling an unchanged sequence does not refresh it.
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
    /// Last observed accepted target/revision, refreshed by poll. No second mailbox lock.
    /// It is a model snapshot, not a claim that the revision has been generated.
    pub fn model_target(&self) -> (u64, auditorylab_core::live::Target) {
        self.accepted
    }
    /// Complete a missing terminal publication only after the supervising worker
    /// has exited with an error and its stream has been dropped. Existing outcomes win.
    pub fn worker_exited_with_error(&self) {
        self.monitor.0.shared.finish(Terminal::StreamSetupFailed);
    }
    /// Coherent shared session wording; a Stop fade's terminal zeros do not rename its recipe.
    pub fn description(&self) -> String {
        if matches!(
            self.status.phase,
            PlaybackPhase::Completed | PlaybackPhase::Stopped | PlaybackPhase::Failed
        ) {
            format!(
                "{} · {}",
                if self.descriptor().frames.is_some() {
                    "Captured recipe"
                } else {
                    "Last accepted target"
                },
                crate::SoundSummary::target(&self.accepted.1).description()
            )
        } else {
            format!(
                "Generated audio · {}",
                crate::generated_description(self.status.observation.as_ref())
            )
        }
    }
    /// Copied actual mixed samples. They are not microphone samples or an acoustic playhead.
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
#[allow(clippy::cast_precision_loss, reason = "bounded monitor sample count")]
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
pub use auditorylab_core::waveform::sample_trace;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::{callback::Callback, source::Source};
    use crate::{Carrier, Component, Ears, Origin, SampleFormat, Stimulus};
    fn descriptor(hz: u32) -> SessionDescriptor {
        SessionDescriptor::new(
            "test".into(),
            Origin::Stream("synthetic".into()),
            OutputInfo {
                name: Some("synthetic callback".into()),
                channels: 2,
                sample_rate: SampleRate::new(hz).unwrap(),
                format: SampleFormat::F64,
            },
            Some(u64::from(hz)),
            auditorylab_core::live::Target::silence(SampleRate::new(hz).unwrap()),
        )
    }
    fn signal(hz: u32) -> crate::Signal {
        Stimulus {
            duration_seconds: 1.0,
            fade_seconds: 0.0,
            components: vec![Component {
                ears: Ears::Left,
                peak_dbfs: -20.0,
                carrier: Carrier::Tone {
                    frequency_hz: 100.0,
                },
                modulation: None,
            }],
        }
        .signal(SampleRate::new(hz).unwrap())
        .unwrap()
    }
    fn time(nanos: u64) -> cpal::OutputStreamTimestamp {
        let at = cpal::StreamInstant::from_nanos(nanos);
        cpal::OutputStreamTimestamp {
            callback: at,
            playback: at,
        }
    }
    #[test]
    fn rms_of_one_impulse_in_four_is_half_the_peak_at_tiny_and_large_levels() {
        for level in [1.0_f64, 1e-200, 1e200] {
            let samples = [[level, -level], [0.0, 0.0], [0.0, 0.0], [0.0, 0.0]];
            let meters = sample_meters(&samples);
            let expected_peak = 20.0 * level.log10();
            let expected_rms = expected_peak - 6.020599913279624;
            for meter in meters {
                assert!((meter.peak_dbfs - expected_peak).abs() < 1e-10);
                assert!((meter.rms_dbfs - expected_rms).abs() < 1e-10);
            }
        }
    }
    #[test]
    fn real_callback_samples_match_history_with_finite_stop_and_contention() {
        let monitor = PlaybackMonitor::new(descriptor(8000), MonitorDetail::Waveform, None);
        let mut reader = monitor.reader();
        let mut a =
            Callback::new(signal(8000), 2, monitor.shared()).with_monitor(Some(monitor.clone()));
        let other = Arc::<Shared>::default();
        let mut b = Callback::new(signal(8000), 2, other.clone());
        let mut x = [0.0; 1024];
        let mut y = x;
        a.fill(&mut x, time(0), &|v| v);
        b.fill(&mut y, time(0), &|v| v);
        assert_eq!(x, y);
        assert!(reader.poll(100));
        assert_eq!(
            reader.samples(),
            x.as_chunks::<2>()
                .0
                .iter()
                .map(|f| [f[0], f[1]])
                .collect::<Vec<_>>()
        );
        let guard = monitor.0.history.lock().unwrap();
        a.fill(&mut x, time(64_000_000), &|v| v);
        b.fill(&mut y, time(64_000_000), &|v| v);
        assert_eq!(x, y);
        assert!(!reader.poll(100));
        assert_eq!(reader.status().generated_frames, 1024);
        drop(guard);
        a.fill(&mut x, time(128_000_000), &|v| v);
        b.fill(&mut y, time(128_000_000), &|v| v);
        assert_eq!(x, y);
        reader.poll(100);
        assert_eq!(reader.status().gaps, 1);
        assert_eq!(reader.status().missed_publications, 1);
        assert_eq!(reader.status().underruns, 0);
        assert_eq!(reader.status().window_start, 1024);
        assert_eq!(reader.samples().len(), 512);
        monitor.0.shared.request_stop();
        other.request_stop();
        a.fill(&mut x, time(192_000_000), &|v| v);
        b.fill(&mut y, time(192_000_000), &|v| v);
        assert_eq!(x, y);
        reader.poll(100);
        assert_eq!(reader.status().phase, PlaybackPhase::Draining);
        a.fill(&mut x, time(256_000_000), &|v| v);
        b.fill(&mut y, time(256_000_000), &|v| v);
        assert_eq!(x, y);
        reader.poll(100);
        assert_eq!(reader.status().phase, PlaybackPhase::Stopped);
    }
    #[test]
    fn ring_limits_wrap_large_writes_and_age_are_bounded() {
        for hz in [1, 768000] {
            let m = PlaybackMonitor::new(descriptor(hz), MonitorDetail::Waveform, None);
            let cap = capacity(SampleRate::new(hz).unwrap());
            assert_eq!(m.0.history.lock().unwrap().samples.len(), cap);
            let data = vec![[0.5, -0.25]; cap + 17];
            {
                let mut p = m.try_publish().unwrap();
                p.append(0, &data);
                p.finish(data.len() as u64, None, false);
            }
            let mut r = m.reader();
            assert!(r.poll(100));
            assert_eq!(r.samples().len(), cap);
            assert_eq!(r.status().window_start, 17);
            let received = r.received;
            r.poll(100);
            assert_eq!(r.received, received);
            {
                let mut p = m.try_publish().unwrap();
                p.append(data.len() as u64 + 100, &[[0.0; 2]]);
                p.finish(data.len() as u64 + 101, None, false);
            }
            r.poll(100);
            assert_eq!(r.samples(), [[0.0; 2]]);
            assert_eq!(r.status().gaps, 1);
        }
    }
    #[test]
    fn poisoned_monitor_never_fails_audio_and_terminal_survives_missing_snapshot() {
        let m = PlaybackMonitor::new(descriptor(8000), MonitorDetail::Waveform, None);
        let c = m.clone();
        assert!(
            std::thread::spawn(move || {
                let _guard = c.0.history.lock().unwrap();
                panic!("injected monitor poisoning")
            })
            .join()
            .is_err()
        );
        let mut callback = Callback::new(signal(8000), 2, m.shared()).with_monitor(Some(m.clone()));
        let mut out = [0.0; 1024];
        callback.fill(&mut out, time(0), &|v| v);
        assert!(out.iter().any(|v| *v != 0.0));
        let mut reader = m.reader();
        assert!(!reader.poll(40));
        assert!(reader.status().disabled);
        m.0.shared.finish(Terminal::Backend);
        reader.poll(40);
        assert_eq!(reader.status().phase, PlaybackPhase::Failed);
    }
    #[test]
    fn callbacks_and_preallocated_reads_allocate_nothing() {
        let m = PlaybackMonitor::new(descriptor(8000), MonitorDetail::Waveform, None);
        let mut reader = m.reader();
        let mut callback = Callback::new(signal(8000), 2, m.shared()).with_monitor(Some(m));
        let mut out = [0.0; 4096];
        let measured = allocation_counter::measure(|| {
            callback.fill(&mut out, time(0), &|v| v);
            reader.poll(100);
            std::hint::black_box(sample_meters(reader.samples()));
        });
        assert_eq!(measured.count_total, 0);
        let m = PlaybackMonitor::new(descriptor(8000), MonitorDetail::Quantities, None);
        assert!(m.0.history.lock().unwrap().samples.is_empty());
        assert!(m.reader().samples.is_empty());
    }
    #[test]
    fn digital_meters_and_extrema_use_the_entire_window() {
        let frames = [[0.5, 0.0], [-0.5, 0.0]];
        let meters = sample_meters(&frames);
        assert!((meters[0].peak_dbfs + 6.020599913279624).abs() < 1e-12);
        assert_eq!(meters[0].rms_dbfs, meters[0].peak_dbfs);
        assert_eq!(meters[1].peak_dbfs, f64::NEG_INFINITY);
        assert_eq!(sample_meters(&[])[0].rms_dbfs, f64::NEG_INFINITY);
        let mut data = [[0.0; 2]; 1024];
        data[500][0] = 1.0;
        let mut bins = Vec::new();
        sample_trace(&data, 16, &mut bins).unwrap();
        assert!(bins.iter().any(|b| b[0].max == 1.0));
    }
    #[test]
    fn actual_live_crossfades_and_stop_match_with_or_without_monitoring() {
        let rate = SampleRate::DEFAULT;
        let mut draft = crate::Draft::new();
        for n in 0..16 {
            let id = draft.add(crate::SourceKind::Tone).unwrap();
            draft.replace(
                id,
                Component {
                    ears: Ears::Both,
                    peak_dbfs: -30.0,
                    carrier: Carrier::Tone {
                        frequency_hz: 200.0 + f64::from(n) * 20.0,
                    },
                    modulation: None,
                },
            );
        }
        let a = crate::LiveGenerator::new(&draft, rate).unwrap();
        let b = crate::LiveGenerator::new(&draft, rate).unwrap();
        let ca = a.control();
        let cb = b.control();
        let mut desc = descriptor(rate.hz());
        desc.frames = None;
        let monitor =
            PlaybackMonitor::new(desc, MonitorDetail::Waveform, Some(Arc::clone(&a.mailbox)));
        let mut reader = monitor.reader();
        let other = Arc::<Shared>::default();
        let mut a = Callback::new(Source::Live(Box::new(a)), 2, monitor.shared())
            .with_monitor(Some(monitor.clone()));
        let mut b = Callback::new(Source::Live(Box::new(b)), 2, other.clone());
        let mut x = [0.0; 2048];
        let mut y = x;
        for block in 0..12_u64 {
            if block == 4 {
                let components = draft.components().to_vec();
                for (id, c) in components {
                    draft.replace(
                        id,
                        Component {
                            carrier: Carrier::Click {
                                rate_hz: 40.0,
                                width_seconds: 0.001,
                            },
                            ..c
                        },
                    );
                }
                assert_eq!(ca.update(&draft).unwrap(), 2);
                assert_eq!(cb.update(&draft).unwrap(), 2);
            }
            if block == 8 {
                monitor.0.shared.request_stop();
                other.request_stop();
            }
            let guard = (block == 6).then(|| monitor.0.history.lock().unwrap());
            a.fill(&mut x, time(block * 1024 * 1_000_000_000 / 48000), &|v| v);
            b.fill(&mut y, time(block * 1024 * 1_000_000_000 / 48000), &|v| v);
            assert_eq!(x.map(f64::to_bits), y.map(f64::to_bits));
            drop(guard);
            reader.poll(100);
            if block == 4 {
                let obs = reader.status().observation.unwrap();
                assert_eq!(obs.voices.iter().flatten().count(), 32);
                assert_eq!(reader.status().accepted_revision, 2);
                assert_eq!(obs.active_revision, 2);
                assert_eq!(obs.settled_revision, 1);
            }
        }
        assert_eq!(reader.status().phase, PlaybackPhase::Stopped);
        assert_eq!(reader.status().missed_publications, 1);
        assert_eq!(reader.status().underruns, 0);
    }
    #[test]
    fn a_real_stalled_reader_never_blocks_callback_completion() {
        let monitor = PlaybackMonitor::new(descriptor(8000), MonitorDetail::Waveform, None);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let other = monitor.clone();
        let holder = std::thread::spawn(move || {
            let _guard = other.0.history.lock().unwrap();
            ready_tx.send(()).unwrap();
            let _ = release_rx.recv_timeout(Duration::from_secs(3));
        });
        ready_rx.recv().unwrap();
        let mut callback =
            Callback::new(signal(8000), 2, monitor.shared()).with_monitor(Some(monitor.clone()));
        let mut output = [0.0; 1024];
        for n in 0..10 {
            callback.fill(&mut output, time(n * 64_000_000), &|v| v);
        }
        assert_eq!(monitor.0.missed.load(Ordering::Relaxed), 10);
        let _ = release_tx.send(());
        holder.join().unwrap();
        let mut reader = monitor.reader();
        reader.poll(40);
        assert_eq!(reader.status().generated_frames, 5120);
        assert_eq!(reader.status().phase, PlaybackPhase::Running);
        callback.fill(&mut output, time(640_000_000), &|v| v);
        reader.poll(40);
        assert_eq!(reader.status().gaps, 1);
    }
    #[test]
    fn unchanged_receipts_do_not_hide_stale_observations() {
        let monitor = PlaybackMonitor::new(descriptor(8000), MonitorDetail::Quantities, None);
        let mut reader = monitor.reader();
        assert_eq!(reader.freshness(), ObservationFreshness::Waiting);
        {
            let mut p = monitor.try_publish().unwrap();
            p.finish(1, None, false);
        }
        reader.poll(0);
        assert_eq!(reader.freshness(), ObservationFreshness::Current);
        reader.received = Some(Instant::now() - Duration::from_secs(1));
        reader.poll(0);
        assert_eq!(reader.freshness(), ObservationFreshness::Stale);
    }
    #[test]
    #[ignore = "synthetic callback timing qualification; no audio device"]
    fn callback_monitor_costs() {
        for hz in [48000, 768000] {
            for frames in [64_usize, 256, 1024, 4096] {
                for detail in [
                    None,
                    Some(MonitorDetail::Quantities),
                    Some(MonitorDetail::Waveform),
                ] {
                    let rate = SampleRate::new(hz).unwrap();
                    let mut tones = crate::Draft::new();
                    for n in 0..16 {
                        let id = tones.add(crate::SourceKind::Tone).unwrap();
                        tones.replace(
                            id,
                            Component {
                                ears: Ears::Both,
                                peak_dbfs: -30.0,
                                carrier: Carrier::Tone {
                                    frequency_hz: 200.0 + f64::from(n) * 20.0,
                                },
                                modulation: Some(crate::Modulation::Sinusoidal {
                                    rate_hz: 39.0625,
                                    depth: 1.0,
                                }),
                            },
                        );
                    }
                    let mut pulses = tones.clone();
                    for (id, c) in tones.components() {
                        pulses.replace(
                            *id,
                            Component {
                                carrier: Carrier::Click {
                                    rate_hz: 40.0,
                                    width_seconds: 0.001,
                                },
                                ..*c
                            },
                        );
                    }
                    let generator = crate::LiveGenerator::new(&tones, rate).unwrap();
                    let control = generator.control();
                    let mut desc = descriptor(hz);
                    desc.frames = None;
                    let monitor = detail.map(|d| {
                        PlaybackMonitor::new(desc, d, Some(Arc::clone(&generator.mailbox)))
                    });
                    let mut reader = monitor.as_ref().map(PlaybackMonitor::reader);
                    let shared = monitor
                        .as_ref()
                        .map_or_else(Arc::default, PlaybackMonitor::shared);
                    let mut callback =
                        Callback::new(Source::Live(Box::new(generator)), 2, Arc::clone(&shared))
                            .with_monitor(monitor.clone());
                    let mut output = vec![0.0; frames * 2];
                    let mut timings = Vec::with_capacity(300);
                    let mut next_poll = 0;
                    for n in 0..300_u64 {
                        if n > 0 && n % 60 == 0 {
                            control
                                .update(if (n / 60) % 2 == 0 { &tones } else { &pulses })
                                .unwrap();
                        }
                        let guard = if n % 25 == 0 {
                            monitor.as_ref().map(|m| m.0.history.lock().unwrap())
                        } else {
                            None
                        };
                        let start = Instant::now();
                        callback.fill(
                            &mut output,
                            time(n * frames as u64 * 1_000_000_000 / u64::from(hz)),
                            &|v| v,
                        );
                        timings.push(start.elapsed().as_nanos());
                        drop(guard);
                        if n * frames as u64 >= next_poll {
                            if let Some(reader) = reader.as_mut() {
                                reader.poll(100);
                            }
                            next_poll = n * frames as u64 + u64::from(hz) / 20;
                        }
                    }
                    timings.sort_unstable();
                    println!(
                        "rate={hz} frames={frames} monitor={detail:?} median_ns={} p95_ns={} max_ns={} budget_ns={} retained_bytes={} consumer_bytes={} misses={} backend_underruns={}",
                        timings[150],
                        timings[285],
                        timings[299],
                        frames as u64 * 1_000_000_000 / u64::from(hz),
                        monitor
                            .as_ref()
                            .map_or(0, |m| m.0.history.lock().unwrap().samples.len() * 16),
                        reader.as_ref().map_or(0, |r| r.samples.len() * 16),
                        monitor
                            .as_ref()
                            .map_or(0, |m| m.0.missed.load(Ordering::Relaxed)),
                        shared.underruns()
                    );
                }
            }
        }
    }
}
