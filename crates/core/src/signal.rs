// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Pure bounded reduction of already-generated stereo samples.
use crate::{SampleRate, Session};
use alloc::{collections::VecDeque, vec, vec::Vec};
use core::{fmt, mem::size_of};

/// Maximum returned bins, each containing separate left/right measurements.
pub const MAX_SIGNAL_BINS: usize = 1_024;
/// Exclusive integer source-frame interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRange {
    /// First included frame.
    pub start: u64,
    /// First excluded frame.
    pub end: u64,
}
/// Actual per-ear sample extrema, with no channel averaging or normalization.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StereoExtrema {
    /// Minimum left/right sample.
    pub min: [f64; 2],
    /// Maximum left/right sample.
    pub max: [f64; 2],
}
impl StereoExtrema {
    /// Actual absolute sample peak in each ear, without averaging or calibration.
    #[must_use]
    pub fn sample_peak(self) -> [f64; 2] {
        [
            self.min[0].abs().max(self.max[0].abs()),
            self.min[1].abs().max(self.max[1].abs()),
        ]
    }
    fn sample(samples: [f64; 2]) -> Self {
        Self {
            min: samples,
            max: samples,
        }
    }
    fn merge(&mut self, other: Self) {
        for ear in 0..2 {
            self.min[ear] = self.min[ear].min(other.min[ear]);
            self.max[ear] = self.max[ear].max(other.max[ear]);
        }
    }
}
/// A measured interval; incomplete intervals must not be connected by plot paths.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalBin {
    /// Exact interval represented, never an arbitrary clipping of coarse extrema.
    pub range: FrameRange,
    /// Extrema of available samples, or none when the interval is wholly missing.
    pub extrema: Option<StereoExtrema>,
    /// Every frame in this interval was observed. False does not mean silence.
    pub complete: bool,
}
/// Immutable bounded reduction, allocated only on the querying thread.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalView {
    bins: Vec<SignalBin>,
}
impl SignalView {
    /// At most [`MAX_SIGNAL_BINS`] measured intervals in source order.
    #[must_use]
    pub fn bins(&self) -> &[SignalBin] {
        &self.bins
    }
    /// Actual range represented by the bins, including explicitly missing coverage.
    #[must_use]
    pub fn span(&self) -> Option<FrameRange> {
        Some(FrameRange {
            start: self.bins.first()?.range.start,
            end: self.bins.last()?.range.end,
        })
    }
    /// Owned bin capacity and view metadata, excluding allocator bookkeeping.
    #[must_use]
    pub fn capacity_bytes(&self) -> usize {
        size_of::<Self>() + self.bins.capacity() * size_of::<SignalBin>()
    }
}
/// An invalid feed; no history state changes on rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryError {
    /// Samples overlap or precede already observed source coverage.
    OutOfOrder,
    /// Source indices exceed the synthesis frame domain.
    FrameRange,
    /// Samples must be finite digital amplitudes in [-1, 1].
    Samples,
}
impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OutOfOrder => "Observation source frames are out of order",
            Self::FrameRange => "Observation exceeds the source-frame domain",
            Self::Samples => "Observation contains invalid digital samples",
        })
    }
}
impl core::error::Error for HistoryError {}
#[derive(Clone, Copy)]
struct RawFrame {
    index: u64,
    samples: [f64; 2],
}
#[derive(Clone, Copy)]
struct Bucket {
    index: u64,
    covered: u64,
    extrema: Option<StereoExtrema>,
}
/// Bounded recent signal history: at most 100 ms raw detail and 2 s coarse ranges.
/// Coarse boundaries are anchored to source frame zero. Queries never rerender.
pub struct SignalHistory {
    raw: VecDeque<RawFrame>,
    buckets: VecDeque<Bucket>,
    raw_limit: usize,
    bucket_limit: usize,
    width: u64,
    through: Option<u64>,
}
impl SignalHistory {
    /// Preallocate all feed storage for this actual sample rate.
    #[must_use]
    pub fn new(rate: SampleRate) -> Self {
        let raw_limit = usize::try_from(rate.hz().div_ceil(10)).unwrap_or(usize::MAX);
        let recent = u64::from(rate.hz()) * 2;
        let width = recent.div_ceil(2_048);
        let bucket_limit = usize::try_from(recent / width).unwrap_or(usize::MAX);
        Self {
            raw: VecDeque::with_capacity(raw_limit),
            buckets: VecDeque::with_capacity(bucket_limit),
            raw_limit,
            bucket_limit,
            width,
            through: None,
        }
    }
    /// Required feed storage and metadata without allocating. On the supported
    /// native targets this equals the instantiated VecDeque capacities.
    #[must_use]
    pub fn required_capacity_bytes(rate: SampleRate) -> usize {
        let raw = usize::try_from(rate.hz().div_ceil(10)).unwrap_or(usize::MAX);
        let recent = u64::from(rate.hz()) * 2;
        let width = recent.div_ceil(2_048);
        let buckets = usize::try_from(recent / width).unwrap_or(usize::MAX);
        raw.checked_mul(size_of::<RawFrame>())
            .and_then(|n| {
                buckets
                    .checked_mul(size_of::<Bucket>())
                    .and_then(|b| n.checked_add(b))
            })
            .and_then(|n| n.checked_add(size_of::<Self>()))
            .unwrap_or(usize::MAX)
    }
    /// Actual owned capacities and metadata; allocator/RSS overhead is excluded.
    #[must_use]
    pub fn capacity_bytes(&self) -> usize {
        size_of::<Self>()
            + self.raw.capacity() * size_of::<RawFrame>()
            + self.buckets.capacity() * size_of::<Bucket>()
    }
    /// Exclusive last observed source end, independent of playback completion.
    #[must_use]
    pub const fn observed_through(&self) -> Option<u64> {
        self.through
    }
    /// Forget retained coverage without reallocating.
    pub fn clear(&mut self) {
        self.raw.clear();
        self.buckets.clear();
        self.through = None;
    }
    /// Add a source prefix without allocation. Gaps remain explicit, never zeros.
    /// Work is O(samples + at most 2048 missing coarse buckets).
    ///
    /// # Errors
    /// Rejects overlapping, invalid or out-of-domain samples without changing state.
    pub fn feed(&mut self, start: u64, samples: &[[f64; 2]]) -> Result<(), HistoryError> {
        let end = start
            .checked_add(samples.len() as u64)
            .filter(|&end| end <= Session::MAX_FRAMES)
            .ok_or(HistoryError::FrameRange)?;
        if samples.is_empty() {
            return Ok(());
        }
        if self.through.is_some_and(|previous| start < previous) {
            return Err(HistoryError::OutOfOrder);
        }
        if !samples
            .iter()
            .flatten()
            .all(|x| x.is_finite() && (-1.0..=1.0).contains(x))
        {
            return Err(HistoryError::Samples);
        }
        for (offset, &samples) in samples.iter().enumerate() {
            let index = start + offset as u64;
            if self.raw.len() == self.raw_limit {
                self.raw.pop_front();
            }
            self.raw.push_back(RawFrame { index, samples });
            let bucket_index = index / self.width;
            if self
                .buckets
                .back()
                .is_none_or(|last| last.index != bucket_index)
            {
                let begin = self
                    .buckets
                    .back()
                    .map_or(bucket_index, |last| last.index + 1);
                // A large discontinuity replaces old buckets in bounded work.
                let begin = begin.max(bucket_index.saturating_sub(self.bucket_limit as u64 - 1));
                if self
                    .buckets
                    .back()
                    .is_some_and(|last| begin > last.index + 1)
                {
                    self.buckets.clear();
                }
                for missing in begin..=bucket_index {
                    if self.buckets.len() == self.bucket_limit {
                        self.buckets.pop_front();
                    }
                    self.buckets.push_back(Bucket {
                        index: missing,
                        covered: 0,
                        extrema: None,
                    });
                }
            }
            if let Some(bucket) = self.buckets.back_mut() {
                bucket.covered += 1;
                merge(&mut bucket.extrema, StereoExtrema::sample(samples));
            }
        }
        self.through = Some(end);
        Ok(())
    }
    /// Exact recent detail from retained raw samples, up to 100 ms and 1024 bins.
    /// Requested windows shorter than two samples retain their actual extent.
    #[must_use]
    pub fn detail(&self, window_frames: u64, max_bins: usize) -> SignalView {
        let Some(end) = self.through else {
            return SignalView { bins: Vec::new() };
        };
        let frames = window_frames.min(self.raw_limit as u64).min(end);
        let count = max_bins
            .min(MAX_SIGNAL_BINS)
            .min(usize::try_from(frames).unwrap_or(usize::MAX));
        if count == 0 {
            return SignalView { bins: Vec::new() };
        }
        let start = end - frames;
        let mut bins: Vec<_> = (0..count)
            .map(|i| SignalBin {
                range: FrameRange {
                    start: start + frames * i as u64 / count as u64,
                    end: start + frames * (i + 1) as u64 / count as u64,
                },
                extrema: None,
                complete: false,
            })
            .collect();
        let mut covered = vec![0_u64; count];
        let mut bin = 0;
        for sample in self.raw.iter().filter(|sample| sample.index >= start) {
            while bin + 1 < count && sample.index >= bins[bin].range.end {
                bin += 1;
            }
            merge(
                &mut bins[bin].extrema,
                StereoExtrema::sample(sample.samples),
            );
            covered[bin] += 1;
        }
        for (bin, covered) in bins.iter_mut().zip(covered) {
            bin.complete = covered == bin.range.end - bin.range.start;
        }
        SignalView { bins }
    }
    /// Recent peak ranges: recombine whole retained anchored buckets and report
    /// their actual interval. Never apply a coarse bucket to a clipped subrange.
    #[must_use]
    pub fn recent(&self, max_bins: usize) -> SignalView {
        let count = max_bins.min(MAX_SIGNAL_BINS);
        if count == 0 || self.buckets.is_empty() {
            return SignalView { bins: Vec::new() };
        }
        let group = self.buckets.len().div_ceil(count);
        let mut bins = Vec::with_capacity(self.buckets.len().div_ceil(group));
        let end = self.through.unwrap_or(0);
        for (offset, bucket) in self.buckets.iter().enumerate() {
            let range = FrameRange {
                start: bucket.index * self.width,
                end: ((bucket.index + 1) * self.width).min(end),
            };
            if offset.is_multiple_of(group) {
                bins.push(SignalBin {
                    range,
                    extrema: None,
                    complete: true,
                });
            }
            if let Some(bin) = bins.last_mut() {
                bin.range.end = range.end;
                bin.complete &= bucket.covered == range.end - range.start;
                if let Some(extrema) = bucket.extrema {
                    merge(&mut bin.extrema, extrema);
                }
            }
        }
        SignalView { bins }
    }
}
fn merge(target: &mut Option<StereoExtrema>, value: StereoExtrema) {
    if let Some(target) = target {
        target.merge(value);
    } else {
        *target = Some(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Component, Ears, Modulation, Stimulus};
    fn rate(hz: u32) -> SampleRate {
        SampleRate::new(hz).unwrap()
    }
    fn oracle(samples: &[(u64, [f64; 2])], bin: &SignalBin) {
        let available: Vec<_> = samples
            .iter()
            .filter(|(index, _)| *index >= bin.range.start && *index < bin.range.end)
            .collect();
        assert_eq!(
            bin.complete,
            available.len() as u64 == bin.range.end - bin.range.start
        );
        if available.is_empty() {
            assert!(bin.extrema.is_none());
            return;
        }
        let extrema = bin.extrema.unwrap();
        for ear in 0..2 {
            let min = available
                .iter()
                .map(|(_, sample)| sample[ear])
                .fold(f64::INFINITY, f64::min);
            let max = available
                .iter()
                .map(|(_, sample)| sample[ear])
                .fold(f64::NEG_INFINITY, f64::max);
            assert_eq!(extrema.min[ear], min);
            assert_eq!(extrema.max[ear], max);
        }
    }
    #[test]
    fn actual_capacities_and_all_supported_rate_formulas_are_bounded() {
        for hz in 1..=SampleRate::MAX_HZ {
            assert!(SignalHistory::required_capacity_bytes(rate(hz)) < 2 * 1024 * 1024);
        }
        std::println!(
            "SIGNAL_HISTORY_CAPACITY,rate=768000,raw_frame_bytes={},bucket_bytes={},history_bytes={},bin_bytes={},max_query_counter_scratch={}",
            size_of::<RawFrame>(),
            size_of::<Bucket>(),
            SignalHistory::required_capacity_bytes(rate(768000)),
            size_of::<SignalBin>(),
            MAX_SIGNAL_BINS * size_of::<u64>()
        );
        for hz in [
            1, 2, 9, 10, 11, 1023, 1024, 1025, 8000, 44100, 48000, 96000, 767999, 768000,
        ] {
            let history = SignalHistory::new(rate(hz));
            assert_eq!(
                history.capacity_bytes(),
                SignalHistory::required_capacity_bytes(rate(hz))
            );
            assert!(history.bucket_limit <= 2048 && history.raw_limit >= 1);
            assert!(history.bucket_limit as u64 * history.width <= u64::from(hz) * 2);
        }
    }
    #[test]
    fn anchored_whole_buckets_and_exact_detail_preserve_unequal_ears_clicks_and_gaps() {
        let mut history = SignalHistory::new(rate(44100));
        let mut samples = Vec::new();
        for start in [0_u32, 103, 204, 4097, 90000] {
            let block: Vec<_> = (0..97)
                .map(|offset| {
                    let index = start + offset;
                    let pulse = if index % 17 == 3 { 1.0 } else { 0.0 };
                    [pulse, -f64::from(index % 31) / 31.0]
                })
                .collect();
            history.feed(u64::from(start), &block).unwrap();
            samples.extend(
                block
                    .into_iter()
                    .enumerate()
                    .map(|(i, frame)| (u64::from(start) + i as u64, frame)),
            );
            for bins in [1, 3, 41, 1024, 100000] {
                let recent = history.recent(bins);
                assert!(recent.bins().len() <= 1024);
                for bin in recent.bins() {
                    oracle(&samples, bin);
                    assert_eq!(bin.range.start % history.width, 0);
                }
                for window in [0, 1, 2, 23, 4410, u64::MAX] {
                    for bin in history.detail(window, bins).bins() {
                        oracle(&samples, bin);
                    }
                }
            }
        }
        let recent = history.recent(1);
        assert!(!recent.bins()[0].complete);
        assert_eq!(recent.bins()[0].extrema.unwrap().max[0], 1.0);
        // Every-17th subsampling at offset zero would miss the one-frame pulses.
        let wrong_peak = samples
            .iter()
            .filter(|(index, _)| index % 17 == 0)
            .map(|(_, sample)| sample[0])
            .fold(0.0, f64::max);
        assert_eq!(wrong_peak, 0.0);
        assert_ne!(wrong_peak, recent.bins()[0].extrema.unwrap().max[0]);
        assert!(
            samples
                .iter()
                .any(|(i, sample)| *i % 17 == 3 && sample[0] == 1.0)
        );
    }
    #[test]
    fn real_synthesis_feeds_plain_binaural_am_and_gated_ranges_without_resynthesis() {
        for hz in [8000, 44100, 48000, 768000] {
            for shape in 0..3 {
                let components = vec![
                    Component::Tone {
                        ears: Ears::Left,
                        frequency_hz: if shape == 2 && hz >= 44100 {
                            10000.0
                        } else {
                            f64::from(hz) / 16.0
                        },
                        peak_dbfs: -20.0,
                        modulation: match shape {
                            0 => None,
                            1 => Some(Modulation::Sinusoidal {
                                rate_hz: 8.0,
                                depth: 1.0,
                            }),
                            _ => Some(Modulation::Rectangular {
                                rate_hz: 40.0,
                                width_seconds: 0.001,
                            }),
                        },
                    },
                    Component::Tone {
                        ears: Ears::Right,
                        frequency_hz: f64::from(hz) / 15.0,
                        peak_dbfs: -30.0,
                        modulation: None,
                    },
                ];
                let session = Stimulus {
                    duration_seconds: 0.1,
                    fade_seconds: 0.001,
                    components,
                }
                .session(rate(hz))
                .unwrap();
                let total = session.frames();
                let mut renderer = session.into_renderer();
                let mut history = SignalHistory::new(rate(hz));
                let mut raw = Vec::new();
                let mut block = [[0.0; 2]; 997];
                let block_frames = u64::try_from(block.len()).unwrap();
                // Never let a broken renderer completion predicate determine the
                // test's iteration count or grow the retained oracle indefinitely.
                for chunk in 0..total.div_ceil(block_frames) {
                    let offset = chunk * block_frames;
                    let remaining = total - offset;
                    let expected = usize::try_from(remaining.min(block_frames)).unwrap();
                    assert_eq!(renderer.frames_remaining(), remaining);
                    assert!(!renderer.is_complete());
                    let span = renderer.render(&mut block);
                    assert_eq!(span.start(), offset);
                    assert_eq!(span.frames(), expected);
                    assert_eq!(span.end(), offset + u64::try_from(expected).unwrap());
                    assert_eq!(
                        renderer.frames_remaining(),
                        remaining - u64::try_from(expected).unwrap()
                    );
                    assert!(
                        block[expected..]
                            .iter()
                            .flatten()
                            .all(|sample| sample.to_bits() == 0)
                    );
                    history.feed(span.start(), &block[..span.frames()]).unwrap();
                    raw.extend(
                        block[..span.frames()]
                            .iter()
                            .enumerate()
                            .map(|(i, sample)| (span.start() + i as u64, *sample)),
                    );
                }
                assert_eq!(u64::try_from(raw.len()).unwrap(), total);
                assert_eq!(renderer.frames_remaining(), 0);
                assert!(renderer.is_complete());
                block.fill([1.0; 2]);
                let eof = renderer.render(&mut block);
                assert_eq!(eof.start(), total);
                assert_eq!(eof.frames(), 0);
                assert_eq!(eof.end(), total);
                assert_eq!(renderer.frames_remaining(), 0);
                assert!(renderer.is_complete());
                assert!(block.iter().flatten().all(|sample| sample.to_bits() == 0));
                for bin in history.detail(u64::MAX, 143).bins() {
                    oracle(&raw, bin);
                }
                for bin in history.recent(41).bins() {
                    oracle(&raw, bin);
                }
            }
        }
    }
    #[test]
    fn gaps_are_never_silence_and_invalid_feeds_do_not_mutate_history() {
        let mut history = SignalHistory::new(rate(48000));
        history.feed(100, &[[0.0, 0.0]; 4]).unwrap();
        history.feed(200, &[[0.25, -0.75]; 4]).unwrap();
        let before = history.recent(1024);
        assert!(
            before
                .bins()
                .iter()
                .any(|bin| bin.extrema.is_none() && !bin.complete)
        );
        for (start, sample, error) in [
            (203, [0.0; 2], HistoryError::OutOfOrder),
            (204, [f64::NAN, 0.0], HistoryError::Samples),
            (204, [0.0, 1.1], HistoryError::Samples),
            (Session::MAX_FRAMES, [0.0; 2], HistoryError::FrameRange),
        ] {
            assert_eq!(history.feed(start, &[sample]), Err(error));
            assert_eq!(history.recent(1024), before);
        }
        let count = allocation_counter::measure(|| {
            history.feed(1000000, &[[1.0, -1.0]; 1024]).unwrap();
            history.feed(1001024, &[[0.0; 2]; 1024]).unwrap();
            history.clear();
            history.feed(0, &[[0.0; 2]; 1024]).unwrap();
        });
        assert_eq!((count.count_total, count.bytes_total), (0, 0));
        let negative = allocation_counter::measure(|| {
            std::hint::black_box(vec![[0.0; 2]; 10]);
        });
        assert!(negative.bytes_total > 0);
    }
    #[test]
    fn low_rates_keep_actual_extents_and_empty_queries_are_empty() {
        let mut history = SignalHistory::new(rate(1));
        assert!(history.detail(1, 1024).bins().is_empty());
        history.feed(0, &[[0.25, -0.5]]).unwrap();
        let detail = history.detail(1, 1024);
        assert_eq!(detail.span(), Some(FrameRange { start: 0, end: 1 }));
        assert_eq!(detail.bins().len(), 1);
        assert!(detail.bins()[0].complete);
        assert!(history.recent(0).bins().is_empty());
        assert!(history.detail(0, 1024).bins().is_empty());
        assert!(history.detail(1, 0).bins().is_empty());
    }
}

#[cfg(test)]
mod gap_controls {
    use super::*;
    #[test]
    fn a_partial_bucket_gap_survives_recombination_and_a_single_click_survives_reduction() {
        let mut history = SignalHistory::new(SampleRate::new(44100).unwrap());
        history.feed(0, &[[0.0, 0.0]; 2]).unwrap();
        history.feed(4, &[[1.0, -0.25], [0.0, 0.0]]).unwrap();
        history.feed(100, &[[0.0, 0.0]; 100]).unwrap();
        let range = history.recent(1);
        let bin = &range.bins()[0];
        assert!(!bin.complete);
        assert_eq!(bin.extrema.unwrap().sample_peak(), [1.0, 0.25]);
        // A stride-four sampler would read only zeros and incorrectly lose this
        // frame-four pulse when anchored at offset one.
        let independent = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        assert_eq!(
            independent
                .iter()
                .skip(1)
                .step_by(4)
                .copied()
                .fold(0.0, f64::max),
            0.0
        );
        assert_eq!(bin.extrema.unwrap().max[0], 1.0);
        history.clear();
        history
            .feed(
                Session::MAX_FRAMES - 3,
                &[[0.0, 0.0], [0.5, -1.0], [0.0, 0.0]],
            )
            .unwrap();
        let detail = history.detail(3, 1024);
        assert_eq!(detail.span().unwrap().end, Session::MAX_FRAMES);
        assert_eq!(detail.bins()[1].extrema.unwrap().sample_peak(), [0.5, 1.0]);
    }
}

#[cfg(test)]
mod polarity_controls {
    use super::*;
    #[test]
    fn per_ear_silence_and_polarity_inversion_have_independent_extrema() {
        let rate = SampleRate::new(48000).unwrap();
        let mut left = SignalHistory::new(rate);
        let mut right = SignalHistory::new(rate);
        let mut inverse = SignalHistory::new(rate);
        let mut silent = SignalHistory::new(rate);
        let samples: Vec<_> = (0_u32..3000)
            .map(|i| [f64::from(i % 59) / 58.0 - 0.5, 0.0])
            .collect();
        let right_samples: Vec<_> = samples.iter().map(|s| [0.0, s[0]]).collect();
        let inverses: Vec<_> = samples.iter().map(|s| [-s[0], 0.0]).collect();
        left.feed(0, &samples).unwrap();
        right.feed(0, &right_samples).unwrap();
        inverse.feed(0, &inverses).unwrap();
        silent.feed(0, &vec![[0.0; 2]; 3000]).unwrap();
        for broad in [false, true] {
            let query = |history: &SignalHistory| {
                if broad {
                    history.recent(43)
                } else {
                    history.detail(3000, 43)
                }
            };
            let a = query(&left);
            let b = query(&right);
            let c = query(&inverse);
            let zeros = query(&silent);
            for (((a, b), c), zero) in a
                .bins()
                .iter()
                .zip(b.bins())
                .zip(c.bins())
                .zip(zeros.bins())
            {
                assert_eq!(a.range, b.range);
                assert_eq!(a.range, c.range);
                assert!(a.complete && b.complete && c.complete && zero.complete);
                let a = a.extrema.unwrap();
                let b = b.extrema.unwrap();
                let c = c.extrema.unwrap();
                assert_eq!(a.min[1], 0.0);
                assert_eq!(a.max[1], 0.0);
                assert_eq!(b.min[0], 0.0);
                assert_eq!(b.max[0], 0.0);
                assert_eq!(a.min[0], b.min[1]);
                assert_eq!(a.max[0], b.max[1]);
                assert_eq!(c.min[0], -a.max[0]);
                assert_eq!(c.max[0], -a.min[0]);
                assert_eq!(zero.extrema.unwrap().sample_peak(), [0.0, 0.0]);
            }
        }
    }
}
