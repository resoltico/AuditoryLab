// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Generated samples are observation facts, never a playback terminal result.

use crate::{DeclaredDescription, SampleRate};
use auditory_core::{SignalHistory, SignalView};
use auditory_device::{ObservationConsumer, ObservationProducer};
use std::{
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

/// Maximum output ranges per ear and per view.
pub const SIGNAL_MAX_BINS: usize = auditory_core::MAX_SIGNAL_BINS;
/// App-owned capacities plus the caller's checked presentation reservation.
pub const SIGNAL_MEMORY_LIMIT: usize = 8 * 1024 * 1024;
const FRESH_LIMIT: Duration = Duration::from_millis(250);

/// Rejected observation request. Audio synthesis limits are unaffected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationError {
    /// A query must request between one and 1,024 bins.
    InvalidBins,
    /// Detail must be greater than zero and at most 100 ms.
    InvalidWindow,
    /// Caller samples exceed the actual rounded 100 ms raw capacity.
    TooManySamples {
        /// Maximum accepted samples at this rate.
        limit: usize,
        /// Caller-provided sample count.
        observed: usize,
    },
    /// Source range overflowed or exceeded the synthesis frame domain.
    InvalidSourceRange,
    /// Caller amplitudes must be finite and within the digital [-1, 1] range.
    InvalidSamples,
    /// Monitoring allocations exceed the product budget.
    Capacity {
        /// Required bytes, or usize::MAX when arithmetic overflowed.
        bytes: usize,
    },
}
impl fmt::Display for ObservationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBins => f.write_str("signal views require 1 through 1024 bins"),
            Self::TooManySamples { limit, observed } => write!(
                f,
                "signal analysis accepts at most {limit} frames at this rate; received {observed}"
            ),
            Self::InvalidSourceRange => {
                f.write_str("signal source range exceeds the supported frame domain")
            }
            Self::InvalidSamples => {
                f.write_str("signal samples must be finite digital amplitudes within [-1, 1]")
            }
            Self::InvalidWindow => {
                f.write_str("signal detail requires a window greater than zero and at most 100 ms")
            }
            Self::Capacity { bytes } => write!(
                f,
                "signal observation needs {bytes} bytes, exceeding the 8 MiB monitoring budget"
            ),
        }
    }
}
impl std::error::Error for ObservationError {}

/// Pure bounded analysis of caller-owned, already-generated stereo samples.
/// These values are not certified as actual playback and have no captured
/// source identity. Production playback uses [`SignalObservation`] instead.
/// At very low rates the rounded 100 ms capacity can contain one actual sample;
/// its integer extent is reported without interpolation.
///
/// # Errors
/// Rejects invalid bins, oversized input, invalid source ranges and nonfinite or
/// out-of-range amplitudes. No audio device is queried.
pub fn analyze_samples(
    rate: SampleRate,
    start_frame: u64,
    samples: &[[f64; 2]],
    bins: usize,
) -> Result<SignalSeries, ObservationError> {
    if !(1..=SIGNAL_MAX_BINS).contains(&bins) {
        return Err(ObservationError::InvalidBins);
    }
    let limit = rate.hz().div_ceil(10) as usize;
    if samples.len() > limit {
        return Err(ObservationError::TooManySamples {
            limit,
            observed: samples.len(),
        });
    }
    let mut history = SignalHistory::new(rate);
    history
        .feed(start_frame, samples)
        .map_err(|error| match error {
            auditory_core::HistoryError::Samples => ObservationError::InvalidSamples,
            auditory_core::HistoryError::FrameRange | auditory_core::HistoryError::OutOfOrder => {
                ObservationError::InvalidSourceRange
            }
        })?;
    Ok(series(history.detail(samples.len() as u64, bins)))
}

/// Identity created for one prepared observed operation, unrelated to selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalIdentity(u64);
impl SignalIdentity {
    /// Process-local operation identity.
    pub fn get(self) -> u64 {
        self.0
    }
}
/// Actual source-frame interval, with an exclusive end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalRange {
    start: u64,
    end: u64,
}
impl SignalRange {
    /// First source frame in the interval.
    pub fn start_frame(self) -> u64 {
        self.start
    }
    /// Exclusive end; silent device tails cannot extend it.
    pub fn end_frame(self) -> u64 {
        self.end
    }
}
/// Actual minimum and maximum of available generated samples in one ear.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalExtrema {
    min: f64,
    max: f64,
}
impl SignalExtrema {
    /// Minimum digital amplitude.
    pub fn min(self) -> f64 {
        self.min
    }
    /// Maximum digital amplitude.
    pub fn max(self) -> f64 {
        self.max
    }
}
/// A range reduction supplied by shared analysis, never frontend resynthesis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalBin {
    range: SignalRange,
    ears: [SignalExtrema; 2],
    complete: bool,
}
impl SignalBin {
    /// Actual analyzed interval, not a clipped claim about a larger bucket.
    pub fn range(self) -> SignalRange {
        self.range
    }
    /// Available left-ear sample extrema.
    pub fn left(self) -> SignalExtrema {
        self.ears[0]
    }
    /// Available right-ear sample extrema.
    pub fn right(self) -> SignalExtrema {
        self.ears[1]
    }
    /// False if any source coverage in this interval is unavailable.
    /// Renderers must skip or visibly segment these bins.
    pub fn complete(self) -> bool {
        self.complete
    }
}
/// One bounded detail or anchored peak-range series.
#[derive(Debug)]
pub struct SignalSeries {
    range: Option<SignalRange>,
    bins: Box<[SignalBin]>,
    gaps: Box<[SignalRange]>,
}
impl SignalSeries {
    /// Actual covered extent; it can contain explicitly marked unavailable parts.
    pub fn range(&self) -> Option<SignalRange> {
        self.range
    }
    /// At most 1,024 actual reductions; wholly empty intervals have no bin.
    pub fn bins(&self) -> &[SignalBin] {
        &self.bins
    }
    /// Regions containing unavailable coverage, including incomplete buckets.
    /// An incomplete bucket can also contain available samples; this is not a
    /// claim that every sample in the marked interval was lost.
    pub fn gaps(&self) -> &[SignalRange] {
        &self.gaps
    }
}
/// Freshness uses app-owned monotonic receipt history and source coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalFreshness {
    /// No post-start/resume samples have been accepted.
    Waiting,
    /// Genuinely new source coverage has arrived within the observation limit.
    Fresh,
    /// Source coverage has stalled or remains behind the independent cursor.
    Stale,
    /// Presentation is suspended; audio continues independently.
    Suspended,
}
/// Copied generation and coverage facts, never an audible playhead or result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalProgress {
    latest: u64,
    observed: Option<u64>,
    freshness: SignalFreshness,
    age: Option<Duration>,
}
impl SignalProgress {
    /// Independent authoritative generated cursor, including dropped packets.
    pub fn latest_generated_frame(&self) -> u64 {
        self.latest
    }
    /// Exclusive last observed source frame; gaps can precede it.
    pub fn observed_through(&self) -> Option<u64> {
        self.observed
    }
    /// Current app-derived coverage state.
    pub fn freshness(&self) -> SignalFreshness {
        self.freshness
    }
    /// Monotonic time since genuinely new accepted coverage.
    pub fn since_new_coverage(&self) -> Option<Duration> {
        self.age
    }
}
/// Immutable shared view; clone its Arc rather than copying its buffers.
#[derive(Debug)]
pub struct SignalSnapshot {
    identity: SignalIdentity,
    description: Arc<DeclaredDescription>,
    rate: SampleRate,
    detail: SignalSeries,
    peak: SignalSeries,
    progress: SignalProgress,
}
impl SignalSnapshot {
    /// Operation that actually generated these samples.
    pub fn identity(&self) -> SignalIdentity {
        self.identity
    }
    /// Original source declarations, unaffected by later source selection.
    pub fn description(&self) -> &DeclaredDescription {
        &self.description
    }
    /// Prepared source sample rate.
    pub fn sample_rate(&self) -> SampleRate {
        self.rate
    }
    /// Recent exact raw-sample reductions.
    pub fn detail(&self) -> &SignalSeries {
        &self.detail
    }
    /// At most two seconds of whole anchored history buckets.
    pub fn peak_range(&self) -> &SignalSeries {
        &self.peak
    }
    /// Observation facts captured with this view, distinct from current polling.
    pub fn progress(&self) -> &SignalProgress {
        &self.progress
    }
}

/// UI-owned, backend-opaque observation capability. It carries no playback
/// control; dropping it cannot cancel, complete, or block the audio operation.
pub struct SignalObservation {
    identity: SignalIdentity,
    description: Arc<DeclaredDescription>,
    rate: SampleRate,
    consumer: ObservationConsumer,
    history: SignalHistory,
    poll_budget: usize,
    capacity_bytes: usize,
    presentation_reserved_bytes: usize,
    last_poll: Instant,
    last_new: Option<Instant>,
    floor: u64,
    suspended: bool,
    progress: SignalProgress,
}
impl SignalObservation {
    pub(crate) fn new(
        description: DeclaredDescription,
        rate: SampleRate,
        presentation_reserved_bytes: usize,
    ) -> Result<(ObservationProducer, Self), ObservationError> {
        let expected_owned = accounted_bytes(
            auditory_device::transport_capacity_bytes(rate)
                .checked_add(description_capacity(&description))
                .ok_or(ObservationError::Capacity { bytes: usize::MAX })?,
            SignalHistory::required_capacity_bytes(rate),
        )?;
        combined_capacity(expected_owned, presentation_reserved_bytes)?;
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let identity = SignalIdentity(NEXT.fetch_add(1, Ordering::Relaxed));
        let (producer, consumer) = auditory_device::observation_channel(rate);
        let history = SignalHistory::new(rate);
        let owned_capacity_bytes = accounted_bytes(
            consumer
                .capacity_bytes()
                .checked_add(description_capacity(&description))
                .ok_or(ObservationError::Capacity { bytes: usize::MAX })?,
            history.capacity_bytes(),
        )?;
        let capacity_bytes = combined_capacity(owned_capacity_bytes, presentation_reserved_bytes)?;
        let poll_budget = consumer.packet_capacity();
        Ok((
            producer,
            Self {
                identity,
                description: Arc::new(description),
                rate,
                consumer,
                history,
                poll_budget,
                capacity_bytes,
                presentation_reserved_bytes,
                last_poll: Instant::now(),
                last_new: None,
                floor: 0,
                suspended: false,
                progress: SignalProgress {
                    latest: 0,
                    observed: None,
                    freshness: SignalFreshness::Waiting,
                    age: None,
                },
            },
        ))
    }
    /// Identity of this observed operation.
    pub fn identity(&self) -> SignalIdentity {
        self.identity
    }
    /// Admitted app-owned capacities plus the caller's explicit presentation
    /// reservation. This is not process RSS or proof of arbitrary caller usage.
    pub fn capacity_bytes(&self) -> usize {
        self.capacity_bytes
    }
    /// Capacity owned by the observation and its bounded snapshot/query buffers.
    pub fn owned_capacity_bytes(&self) -> usize {
        self.capacity_bytes - self.presentation_reserved_bytes
    }
    /// Presentation storage reserved by the caller, whose construction limits
    /// remain that caller's responsibility.
    pub fn presentation_reserved_bytes(&self) -> usize {
        self.presentation_reserved_bytes
    }
    /// Maximum packets inspected per poll, even if the producer keeps writing.
    pub fn packets_per_poll(&self) -> usize {
        self.poll_budget
    }
    /// Drain a fixed packet budget and refresh app-owned monotonic facts.
    pub fn poll(&mut self) -> SignalProgress {
        self.poll_at(Instant::now())
    }
    fn poll_at(&mut self, now: Instant) -> SignalProgress {
        let latest = self.consumer.latest_generated();
        if now.saturating_duration_since(self.last_poll) > FRESH_LIMIT {
            // Capturing a generation frontier prevents an old queued packet's
            // receipt time from turning a one-second backlog into a live trace.
            self.floor = latest;
        }
        self.last_poll = now;
        for _ in 0..self.poll_budget {
            let Some(packet) = self.consumer.pop() else {
                break;
            };
            if self.suspended || packet.start() < self.floor {
                continue;
            }
            let before = self.history.observed_through();
            if self.history.feed(packet.start(), packet.samples()).is_ok()
                && self.history.observed_through() != before
            {
                self.last_new = Some(now);
            }
        }
        // A producer can publish while this bounded drain runs. Read again
        // after consuming so the copied cursor cannot predate an accepted packet.
        let latest = self.consumer.latest_generated();
        let observed = self.history.observed_through();
        let age = self
            .last_new
            .map(|time| now.saturating_duration_since(time));
        let lag_limit = u64::from(self.rate.hz()).div_ceil(4);
        let behind = observed.is_some_and(|end| latest.saturating_sub(end) > lag_limit);
        let freshness = if self.suspended {
            SignalFreshness::Suspended
        } else if self.last_new.is_none() {
            SignalFreshness::Waiting
        } else if behind || age.is_some_and(|age| age > FRESH_LIMIT) {
            SignalFreshness::Stale
        } else {
            SignalFreshness::Fresh
        };
        self.progress = SignalProgress {
            latest,
            observed,
            freshness,
            age,
        };
        self.progress.clone()
    }
    /// Stop retaining live history while sound continues. Keep polling to drain
    /// bounded visual data; a separately retained snapshot stays immutable.
    pub fn suspend(&mut self) {
        self.suspended = true;
        self.history.clear();
        self.last_new = None;
        self.floor = self.consumer.latest_generated();
        self.progress.freshness = SignalFreshness::Suspended;
        self.progress.observed = None;
        self.progress.age = None;
    }
    /// Begin a fresh history after freeze/hidden presentation. Packets preceding
    /// this frontier are discarded rather than replayed as current data.
    pub fn resume(&mut self) {
        self.suspended = false;
        self.history.clear();
        self.last_new = None;
        self.floor = self.consumer.latest_generated();
        self.last_poll = Instant::now();
        self.progress = SignalProgress {
            latest: self.floor,
            observed: None,
            freshness: SignalFreshness::Waiting,
            age: None,
        };
    }
    /// Analyze retained samples using the shared core service. At low rates an
    /// extent can contain one sample or none; no interpolation is introduced.
    pub fn snapshot(
        &mut self,
        detail_window: Duration,
        bins: usize,
    ) -> Result<Arc<SignalSnapshot>, ObservationError> {
        if !(1..=SIGNAL_MAX_BINS).contains(&bins) {
            return Err(ObservationError::InvalidBins);
        }
        if detail_window.is_zero() || detail_window > Duration::from_millis(100) {
            return Err(ObservationError::InvalidWindow);
        }
        let frames =
            (detail_window.as_nanos() * u128::from(self.rate.hz())).div_ceil(1_000_000_000);
        let frames = u64::try_from(frames).map_err(|_| ObservationError::InvalidWindow)?;
        Ok(Arc::new(SignalSnapshot {
            identity: self.identity,
            description: Arc::clone(&self.description),
            rate: self.rate,
            detail: series(self.history.detail(frames, bins)),
            peak: series(self.history.recent(bins)),
            progress: self.progress.clone(),
        }))
    }
}

/// Compose independently owned capacities without knowing renderer construction.
pub(super) fn combined_capacity(
    owned: usize,
    presentation: usize,
) -> Result<usize, ObservationError> {
    let bytes = owned
        .checked_add(presentation)
        .ok_or(ObservationError::Capacity { bytes: usize::MAX })?;
    if bytes > SIGNAL_MEMORY_LIMIT {
        Err(ObservationError::Capacity { bytes })
    } else {
        Ok(bytes)
    }
}

fn accounted_bytes(transport: usize, history: usize) -> Result<usize, ObservationError> {
    // Four app-produced snapshot-sized buffers cover bounded current output,
    // consumer handoff, query intermediates and allocation during build.
    // Arbitrarily retained additional consumer snapshots are outside this cap.
    let series = SIGNAL_MAX_BINS * std::mem::size_of::<SignalBin>()
        + (SIGNAL_MAX_BINS + 1) * std::mem::size_of::<SignalRange>();
    let snapshots =
        4 * (2 * series + std::mem::size_of::<SignalSnapshot>() + 2 * std::mem::size_of::<usize>());
    let bytes = transport
        .checked_add(history)
        .and_then(|n| n.checked_add(snapshots))
        .and_then(|n| n.checked_add(SIGNAL_MAX_BINS * std::mem::size_of::<u64>()))
        .and_then(|n| n.checked_add(std::mem::size_of::<SignalObservation>()))
        .ok_or(ObservationError::Capacity { bytes: usize::MAX })?;
    if bytes > SIGNAL_MEMORY_LIMIT {
        Err(ObservationError::Capacity { bytes })
    } else {
        Ok(bytes)
    }
}
fn description_capacity(description: &DeclaredDescription) -> usize {
    let origin = match &description.origin {
        crate::Origin::BuiltIn(name) | crate::Origin::Reader(name) => name.capacity(),
        crate::Origin::File(path) => path.capacity(),
    };
    // Preset content is already shared with the loaded source/preparation view.
    std::mem::size_of::<DeclaredDescription>()
        + 2 * std::mem::size_of::<usize>()
        + origin
        + description.routes.capacity() * std::mem::size_of::<crate::Ears>()
        + description.notices.capacity() * std::mem::size_of::<crate::Notice>()
}
fn series(view: SignalView) -> SignalSeries {
    let range = view.span().map(|r| SignalRange {
        start: r.start,
        end: r.end,
    });
    let mut bins = Vec::with_capacity(view.bins().len());
    let mut gaps = Vec::with_capacity(view.bins().len() + 1);
    let mut through = range.map(|r| r.start);
    for bin in view.bins() {
        let r = SignalRange {
            start: bin.range.start,
            end: bin.range.end,
        };
        if let Some(end) = through
            && end < r.start
        {
            add_gap(
                &mut gaps,
                SignalRange {
                    start: end,
                    end: r.start,
                },
            );
        }
        if !bin.complete || bin.extrema.is_none() {
            add_gap(&mut gaps, r);
        }
        if let Some(extrema) = bin.extrema {
            bins.push(SignalBin {
                range: r,
                complete: bin.complete,
                ears: [
                    SignalExtrema {
                        min: extrema.min[0],
                        max: extrema.max[0],
                    },
                    SignalExtrema {
                        min: extrema.min[1],
                        max: extrema.max[1],
                    },
                ],
            });
        }
        through = Some(r.end);
    }
    SignalSeries {
        range,
        bins: bins.into_boxed_slice(),
        gaps: gaps.into_boxed_slice(),
    }
}
fn add_gap(gaps: &mut Vec<SignalRange>, range: SignalRange) {
    if let Some(last) = gaps.last_mut()
        && last.end >= range.start
    {
        last.end = last.end.max(range.end);
    } else {
        gaps.push(range);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn description(name: &str) -> DeclaredDescription {
        crate::load_bytes(
            b"schema_version=2\nduration_seconds=1\nfade_seconds=0",
            crate::Origin::Reader(name.into()),
        )
        .unwrap()
        .description
    }
    #[test]
    fn snapshot_is_immutable_bounded_and_tied_to_its_source() {
        fn send<T: Send>() {}
        fn sync<T: Send + Sync>() {}
        send::<SignalObservation>();
        sync::<SignalSnapshot>();
        let (_, mut observation) =
            SignalObservation::new(description("first"), SampleRate::DEFAULT, 0).unwrap();
        observation
            .history
            .feed(0, &[[0.75, -0.25], [-0.5, 0.125]])
            .unwrap();
        let snapshot = observation
            .snapshot(Duration::from_millis(100), 1024)
            .unwrap();
        let (_, other) =
            SignalObservation::new(description("second"), SampleRate::DEFAULT, 0).unwrap();
        assert_ne!(snapshot.identity(), other.identity());
        assert_eq!(
            snapshot.description().origin(),
            &crate::Origin::Reader("first".into())
        );
        assert_eq!(
            snapshot.detail().range().unwrap(),
            SignalRange { start: 0, end: 2 }
        );
        assert_eq!(snapshot.detail().bins()[0].left().min(), 0.75);
        assert_eq!(snapshot.detail().bins()[0].right().max(), -0.25);
        observation.suspend();
        assert_eq!(snapshot.detail().range().unwrap().end_frame(), 2);
        assert!(
            observation
                .snapshot(Duration::from_millis(100), 1024)
                .unwrap()
                .detail()
                .bins()
                .is_empty()
        );
        assert!(observation.snapshot(Duration::ZERO, 1).is_err());
        assert!(observation.snapshot(Duration::from_millis(101), 1).is_err());
        assert!(observation.snapshot(Duration::from_millis(1), 0).is_err());
        assert!(
            observation
                .snapshot(Duration::from_millis(1), 1025)
                .is_err()
        );
    }
    #[test]
    fn gaps_are_unavailable_coverage_and_never_digital_silence() {
        let (_, mut observation) =
            SignalObservation::new(description("gaps"), SampleRate::new(1000).unwrap(), 0).unwrap();
        observation.history.feed(0, &[[0.5, -0.5]; 10]).unwrap();
        observation.history.feed(20, &[[0.75, -0.75]; 10]).unwrap();
        let snapshot = observation
            .snapshot(Duration::from_millis(100), 30)
            .unwrap();
        assert!(!snapshot.detail().gaps().is_empty());
        assert!(
            snapshot
                .detail()
                .bins()
                .iter()
                .all(|bin| bin.left().min() > 0.0 && bin.right().max() < 0.0)
        );
        assert!(snapshot.detail().bins().len() <= 30);
        assert!(snapshot.detail().gaps().len() <= 31);
    }
    #[test]
    fn one_hertz_exposes_one_actual_sample_without_interpolation() {
        let (_, mut observation) =
            SignalObservation::new(description("low rate"), SampleRate::new(1).unwrap(), 0)
                .unwrap();
        observation.history.feed(0, &[[0.25, 0.0]]).unwrap();
        let snapshot = observation
            .snapshot(Duration::from_millis(2), 1024)
            .unwrap();
        assert_eq!(snapshot.detail().bins().len(), 1);
        assert_eq!(
            snapshot.detail().bins()[0].range(),
            SignalRange { start: 0, end: 1 }
        );
        assert_eq!(
            snapshot.detail().bins()[0].right(),
            SignalExtrema { min: 0.0, max: 0.0 }
        );
    }
    #[test]
    fn future_monitoring_capacity_growth_rejects_before_audio() {
        assert!(matches!(
            accounted_bytes(usize::MAX, 1),
            Err(ObservationError::Capacity { .. })
        ));
        assert!(matches!(
            accounted_bytes(SIGNAL_MEMORY_LIMIT, 0),
            Err(ObservationError::Capacity { .. })
        ));
        assert_eq!(combined_capacity(100, 200).unwrap(), 300);
        assert!(combined_capacity(usize::MAX, 1).is_err());
        assert!(combined_capacity(1, SIGNAL_MEMORY_LIMIT).is_err());
        let (_, observation) = SignalObservation::new(
            description("independent presentation reserve"),
            SampleRate::DEFAULT,
            2048,
        )
        .unwrap();
        assert_eq!(observation.presentation_reserved_bytes(), 2048);
        assert_eq!(
            observation.capacity_bytes(),
            observation.owned_capacity_bytes() + 2048
        );
    }
    #[test]
    fn all_supported_rates_fit_app_owned_types_and_checked_presentation_reservations() {
        let context = description_capacity(&description("budget"));
        for hz in SampleRate::MIN_HZ..=SampleRate::MAX_HZ {
            let rate = SampleRate::new(hz).unwrap();
            let bytes = accounted_bytes(
                auditory_device::transport_capacity_bytes(rate) + context,
                SignalHistory::required_capacity_bytes(rate),
            )
            .unwrap();
            assert!(bytes <= SIGNAL_MEMORY_LIMIT, "rate={hz} bytes={bytes}");
        }
        for hz in [1, 2, 44100, 48000, 96000, 767999, 768000] {
            let rate = SampleRate::new(hz).unwrap();
            let (_, observation) = SignalObservation::new(description("budget"), rate, 0).unwrap();
            assert_eq!(
                observation.capacity_bytes(),
                accounted_bytes(
                    auditory_device::transport_capacity_bytes(rate)
                        + description_capacity(&description("budget")),
                    SignalHistory::required_capacity_bytes(rate)
                )
                .unwrap()
            );
            assert!(observation.packets_per_poll() > 0);
            let owned = observation.owned_capacity_bytes();
            drop(observation);
            let reserve = SIGNAL_MEMORY_LIMIT - owned;
            let (_, exact) = SignalObservation::new(description("budget"), rate, reserve).unwrap();
            assert_eq!(exact.owned_capacity_bytes(), owned);
            assert_eq!(exact.presentation_reserved_bytes(), reserve);
            assert_eq!(exact.capacity_bytes(), SIGNAL_MEMORY_LIMIT);
            drop(exact);
            assert!(SignalObservation::new(description("budget"), rate, reserve + 1).is_err());
        }
    }
    fn render(
        producer: &mut ObservationProducer,
        renderer: &mut auditory_core::Renderer,
        frames: usize,
    ) {
        let mut block = vec![[0.0; 2]; frames];
        let span = renderer.render(&mut block);
        let _ = producer.publish(span, &block);
    }
    fn renderer(rate: SampleRate, seconds: u64) -> auditory_core::Renderer {
        crate::load_bytes(format!("schema_version=2\nduration_seconds={seconds}\nfade_seconds=0\n[[component]]\nkind='click'\nears='left'\nrate_hz=40\nwidth_seconds=0.001\npeak_dbfs=-6\n").as_bytes(), crate::Origin::Reader("offline observed".into())).unwrap().session(rate, crate::Operation::Playback).unwrap().into_renderer()
    }
    #[test]
    fn non_gui_worker_transports_real_generated_pulses_without_sampling_them_away() {
        let rate = SampleRate::new(1000).unwrap();
        let (mut producer, mut observation) =
            SignalObservation::new(description("worker pulse source"), rate, 0).unwrap();
        let worker = std::thread::spawn(move || {
            let mut renderer = renderer(rate, 1);
            render(&mut producer, &mut renderer, 1000);
        });
        worker.join().unwrap();
        let progress = observation.poll();
        assert_eq!(progress.latest_generated_frame(), 1000);
        assert_eq!(progress.observed_through(), Some(1000));
        assert_eq!(progress.freshness(), SignalFreshness::Fresh);
        let snapshot = observation
            .snapshot(Duration::from_millis(100), 16)
            .unwrap();
        assert_eq!(
            snapshot.peak_range().range().unwrap(),
            SignalRange {
                start: 0,
                end: 1000
            }
        );
        let peak = 10_f64.powf(-6.0 / 20.0);
        assert!(snapshot.peak_range().bins().iter().all(|bin| bin.complete()
            && (bin.left().max() - peak).abs() < 1e-15
            && bin.right() == SignalExtrema { min: 0.0, max: 0.0 }));
        assert!(
            snapshot
                .detail()
                .bins()
                .iter()
                .any(|bin| bin.left().max() > 0.0)
        );
    }
    #[test]
    fn stalled_consumer_discards_old_backlog_then_accepts_only_post_frontier_coverage() {
        let rate = SampleRate::new(48000).unwrap();
        let (mut producer, mut observation) =
            SignalObservation::new(description("stall"), rate, 0).unwrap();
        let mut renderer = renderer(rate, 3);
        let start = observation.last_poll;
        render(&mut producer, &mut renderer, 1024);
        assert_eq!(observation.poll_at(start).observed_through(), Some(1024));
        // Overflow, but independent cursor must continue beyond queue coverage.
        for _ in 0..50 {
            render(&mut producer, &mut renderer, 1024);
        }
        let stalled = observation.poll_at(start + Duration::from_secs(1));
        assert_eq!(stalled.latest_generated_frame(), 51 * 1024);
        assert_eq!(stalled.observed_through(), Some(1024));
        assert_eq!(stalled.freshness(), SignalFreshness::Stale);
        render(&mut producer, &mut renderer, 1024);
        let recovered = observation.poll_at(start + Duration::from_millis(1020));
        assert_eq!(recovered.observed_through(), Some(52 * 1024));
        assert_eq!(recovered.freshness(), SignalFreshness::Fresh);
        assert_eq!(recovered.since_new_coverage(), Some(Duration::ZERO));
        let snapshot = observation
            .snapshot(Duration::from_millis(100), 128)
            .unwrap();
        assert!(!snapshot.peak_range().gaps().is_empty());
        let frozen = Arc::clone(&snapshot);
        observation.suspend();
        render(&mut producer, &mut renderer, 1024);
        assert_eq!(observation.poll().freshness(), SignalFreshness::Suspended);
        observation.resume();
        assert_eq!(observation.poll().freshness(), SignalFreshness::Waiting);
        render(&mut producer, &mut renderer, 1024);
        assert_eq!(observation.poll().freshness(), SignalFreshness::Fresh);
        assert_eq!(frozen.progress().observed_through(), Some(52 * 1024));
        assert_eq!(
            observation
                .snapshot(Duration::from_millis(100), 128)
                .unwrap()
                .identity(),
            frozen.identity()
        );
    }
    #[test]
    fn concurrent_generation_never_reports_a_cursor_before_accepted_coverage() {
        let rate = SampleRate::new(48000).unwrap();
        let (mut producer, mut observation) =
            SignalObservation::new(description("concurrent"), rate, 0).unwrap();
        let worker = std::thread::spawn(move || {
            let mut renderer = renderer(rate, 20);
            for _ in 0..500 {
                render(&mut producer, &mut renderer, 1024);
            }
        });
        while !worker.is_finished() {
            let progress = observation.poll();
            assert!(
                progress
                    .observed_through()
                    .is_none_or(|end| end <= progress.latest_generated_frame())
            );
            std::thread::yield_now();
        }
        worker.join().unwrap();
        let progress = observation.poll();
        assert_eq!(progress.latest_generated_frame(), 500 * 1024);
        assert!(
            progress
                .observed_through()
                .is_none_or(|end| end <= progress.latest_generated_frame())
        );
    }
    #[test]
    #[ignore = "explicit maximum-rate observation/reduction timing qualification"]
    fn maximum_rate_observation_and_snapshot_budget() {
        let rate = SampleRate::new(SampleRate::MAX_HZ).unwrap();
        let (mut producer, mut observation) =
            SignalObservation::new(description("maximum-rate offline qualification"), rate, 0)
                .unwrap();
        let mut renderer = renderer(rate, 3);
        let mut block = [[0.0; 2]; 1024];
        let mut times = Vec::with_capacity(60);
        for _ in 0..60 {
            for _ in 0..25 {
                let span = renderer.render(&mut block);
                assert!(producer.publish(span, &block));
            }
            let begin = Instant::now();
            let progress = observation.poll();
            let snapshot = observation
                .snapshot(Duration::from_millis(100), 1024)
                .unwrap();
            times.push(begin.elapsed().as_nanos());
            assert_eq!(
                progress.observed_through(),
                Some(progress.latest_generated_frame())
            );
            assert!(snapshot.detail().bins().len() <= 1024);
            assert!(snapshot.peak_range().bins().len() <= 1024);
            assert!(snapshot.detail().gaps().len() <= 1025);
            assert!(snapshot.peak_range().gaps().len() <= 1025);
        }
        times.sort_unstable();
        println!(
            "OBSERVATION_MAX_RATE,hz={},polls={},active_frames={},capacity_bytes={},owned_capacity_bytes={},presentation_reserved_bytes={},packets_per_poll={},min_ns={},median_ns={},p95_ns={},max_ns={}",
            rate.hz(),
            times.len(),
            60 * 25 * 1024,
            observation.capacity_bytes(),
            observation.owned_capacity_bytes(),
            observation.presentation_reserved_bytes(),
            observation.packets_per_poll(),
            times[0],
            times[30],
            times[56],
            times[59]
        );
    }
}
