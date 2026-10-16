// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Renderer-owned string capacities and geometry over application-supplied bins.
use auditory_app::{DeclaredDescription, Origin, SignalSeries};
use std::fmt::{self, Write};

const PATH_BYTES_PER_BIN: usize = 32;
pub(crate) const ENDPOINT_BYTES: usize = 128;
pub(crate) const RANGE_BYTES: usize = 512;
pub(crate) const POSITION_BYTES: usize = 256;
pub(crate) const CONTEXT_OVERHEAD_BYTES: usize = 1024;
const RESIDENT_COPIES: usize = 8;
pub(crate) const CONTEXT_BYTES: usize = 64 * 1024;
pub(crate) const CAPACITY_NOTICE: &str =
    "Signal display exceeded its reserved string capacity; playback result remains separate.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CapacityError;
impl fmt::Display for CapacityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(CAPACITY_NOTICE)
    }
}
impl std::error::Error for CapacityError {}

/// Formatting cannot grow this allocation. Capacity, rather than length alone,
/// is validated on admission; each write is checked before touching the String.
pub(crate) struct BoundedText {
    value: String,
    limit: usize,
}
impl BoundedText {
    pub(crate) fn new(limit: usize) -> Result<Self, CapacityError> {
        if limit > CONTEXT_BYTES {
            return Err(CapacityError);
        }
        Self::from_string(String::with_capacity(limit), limit)
    }
    fn from_string(value: String, limit: usize) -> Result<Self, CapacityError> {
        if value.capacity() > limit {
            return Err(CapacityError);
        }
        Ok(Self { value, limit })
    }
    pub(crate) fn finish(self) -> String {
        self.value
    }
}
impl Write for BoundedText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let length = self
            .value
            .len()
            .checked_add(value.len())
            .ok_or(fmt::Error)?;
        if length > self.limit || length > self.value.capacity() {
            return Err(fmt::Error);
        }
        self.value.push_str(value);
        Ok(())
    }
}
impl From<fmt::Error> for CapacityError {
    fn from(_: fmt::Error) -> Self {
        Self
    }
}

pub(crate) fn validate_source(description: &DeclaredDescription) -> Result<(), CapacityError> {
    // Fixed bounds cover old/new source replacement equally. Stream the exact
    // Debug presentation instead of guessing escape expansion from input bytes.
    let mut text = BoundedText::new(CONTEXT_BYTES - CONTEXT_OVERHEAD_BYTES)?;
    write_origin(&mut text, description.origin())?;
    Ok(())
}
pub(crate) fn write_origin(text: &mut BoundedText, origin: &Origin) -> fmt::Result {
    match origin {
        Origin::BuiltIn(id) => write!(text, "{id} (built-in)"),
        Origin::File(path) => write!(text, "{path:?}"),
        Origin::Reader(label) => write!(text, "{label:?}"),
    }
}
fn shared_allocation_bytes(capacity: usize) -> Option<usize> {
    // Slint 1.18.1 SharedString::from(String) copies bytes plus NUL into one
    // SharedVector<u8>; its repr(C) header is AtomicIsize, size, capacity.
    // Round up to usize alignment, conservatively including trailing padding.
    let align = std::mem::align_of::<usize>();
    capacity
        .checked_add(1)?
        .checked_add(3 * std::mem::size_of::<usize>())?
        .checked_add(align - 1)
        .map(|bytes| bytes / align * align)
}
pub(crate) fn reserve_bytes() -> usize {
    let reserve = || {
        let path = shared_allocation_bytes(
            PATH_BYTES_PER_BIN.checked_mul(auditory_app::SIGNAL_MAX_BINS)?,
        )?;
        let endpoints = shared_allocation_bytes(ENDPOINT_BYTES)?.checked_mul(4)?;
        let ranges = shared_allocation_bytes(RANGE_BYTES)?.checked_mul(2)?;
        let position = shared_allocation_bytes(POSITION_BYTES)?;
        let context = shared_allocation_bytes(CONTEXT_BYTES)?;
        // Authored property holders: path main+lane+Path; endpoint main+two
        // lanes+two Slint Text items = five. Add a Rust working buffer and conversion
        // allocation, then round seven to eight. Reference-counted clones can
        // share storage; counting every holder as a full allocation is stronger.
        // Eight also covers old/new property replacement while a frame applies.
        path.checked_mul(4)?
            .checked_add(endpoints)?
            .checked_add(ranges)?
            .checked_add(position)?
            .checked_add(context)?
            .checked_mul(RESIDENT_COPIES)
    };
    reserve().unwrap_or(usize::MAX)
}

/// Map supplied extrema to fixed ±1 axes. Every bar is a separate subpath:
/// neither missing bins nor partial buckets acquire a connecting line.
pub(crate) fn paths(series: &SignalSeries) -> Result<[String; 2], CapacityError> {
    let bound = PATH_BYTES_PER_BIN
        .checked_mul(series.bins().len())
        .ok_or(CapacityError)?;
    if series.bins().len() > auditory_app::SIGNAL_MAX_BINS {
        return Err(CapacityError);
    }
    let Some(range) = series.range() else {
        return Ok([String::new(), String::new()]);
    };
    let Ok(width) = u32::try_from(range.end_frame() - range.start_frame()) else {
        return Err(CapacityError);
    };
    if width == 0 {
        return Ok([String::new(), String::new()]);
    }
    let mut paths = [BoundedText::new(bound)?, BoundedText::new(bound)?];
    for bin in series.bins() {
        if !bin.complete() {
            continue;
        }
        let bin_range = bin.range();
        let start = u32::try_from(bin_range.start_frame() - range.start_frame())
            .map_err(|_| CapacityError)?;
        let end = u32::try_from(bin_range.end_frame() - range.start_frame())
            .map_err(|_| CapacityError)?;
        let x = (f64::from(start) + f64::from(end)) * 512.0 / f64::from(width);
        for (path, extrema) in paths.iter_mut().zip([bin.left(), bin.right()]) {
            let low = (1.0 - extrema.min()) * 50.0;
            let high = (1.0 - extrema.max()) * 50.0;
            // Maximum formatted bar is 29 bytes (x<=1024, y<=100); reserve32.
            write!(path, "M{x:.3} {low:.3}V{high:.3}h0.5")?;
        }
    }
    Ok(paths.map(BoundedText::finish))
}

#[cfg(test)]
mod tests {

    #[test]
    fn supplied_per_ear_ranges_map_to_fixed_axes_and_separate_subpaths() {
        use super::*;
        use auditory_app::{SampleRate, analyze_samples};
        let series = analyze_samples(
            SampleRate::new(1000).unwrap(),
            100,
            &[[0.2, -0.8], [-0.4, 0.6], [0.0, 0.0], [1.0, -1.0]],
            2,
        )
        .unwrap();
        let [left, right] = paths(&series).unwrap();
        assert_eq!(left, "M256.000 70.000V40.000h0.5M768.000 50.000V0.000h0.5");
        assert_eq!(
            right,
            "M256.000 90.000V20.000h0.5M768.000 100.000V50.000h0.5"
        );
    }
    #[test]
    fn empty_and_maximal_geometry_obey_actual_allocation_bounds() {
        use super::*;
        use auditory_app::{SampleRate, analyze_samples};
        let rate = SampleRate::new(768_000).unwrap();
        let empty = analyze_samples(rate, 0, &[], 1024).unwrap();
        assert_eq!(paths(&empty).unwrap(), ["", ""]);
        let samples = vec![[-1.0, 1.0]; 76_800];
        let series = analyze_samples(rate, 2_000_000, &samples, 1024).unwrap();
        let mut result = None;
        let allocation = allocation_counter::measure(|| result = Some(paths(&series).unwrap()));
        assert_eq!(allocation.count_total, 2);
        assert_eq!(allocation.bytes_total, 2 * 1024 * PATH_BYTES_PER_BIN as u64);
        for path in result.unwrap() {
            assert_eq!(path.matches('M').count(), 1024);
            assert_eq!(path.capacity(), 1024 * PATH_BYTES_PER_BIN);
            assert!(!path.contains("NaN"));
        }
    }
    #[test]
    fn short_overallocated_string_and_growth_are_rejected() {
        use super::*;
        let mut value = String::with_capacity(10 * 1024 * 1024);
        value.push_str("M0.000 0.000V1.000h0.500");
        assert_eq!(value.len(), 24);
        assert!(BoundedText::from_string(value, 32).is_err());
        let mut text = BoundedText::new(4).unwrap();
        text.write_str("1234").unwrap();
        assert!(text.write_str("5").is_err());
        assert_eq!(text.finish(), "1234");
    }
    #[test]
    fn source_preflight_uses_exact_escaping_and_fixed_replacement_budget() {
        use super::*;
        use auditory_app::SampleRate;
        let short = auditory_app::load_bytes(
            b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0\n",
            Origin::Reader("short".into()),
        )
        .unwrap();
        validate_source(short.description()).unwrap();
        let long = auditory_app::load_bytes(
            b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0\n",
            Origin::Reader("\u{1}".repeat(20_000)),
        )
        .unwrap();
        assert!(validate_source(long.description()).is_err());
        // A display admission failure does not redefine source/audio validity.
        assert_eq!(long.inspect(SampleRate::DEFAULT).unwrap().frames(), 480);
        assert!(reserve_bytes() < auditory_app::SIGNAL_MEMORY_LIMIT);
        assert!(shared_allocation_bytes(usize::MAX).is_none());
    }
    #[test]
    fn pinned_shared_string_conversion_has_one_accounted_allocation() {
        use super::*;
        let input = "x".repeat(ENDPOINT_BYTES);
        let mut retained = None;
        let allocation =
            allocation_counter::measure(|| retained = Some(slint::SharedString::from(input)));
        assert_eq!(allocation.count_total, 1);
        assert!(allocation.bytes_total <= shared_allocation_bytes(ENDPOINT_BYTES).unwrap() as u64);
        let retained = retained.unwrap();
        assert_eq!(retained.len(), ENDPOINT_BYTES);
        let mut holders = None;
        let clones = allocation_counter::measure(|| {
            holders = Some(std::array::from_fn::<_, RESIDENT_COPIES, _>(|_| {
                retained.clone()
            }));
        });
        assert_eq!(
            clones.count_total, 0,
            "pinned SharedString binding clones must share their allocation"
        );
        assert_eq!(holders.unwrap().len(), RESIDENT_COPIES);
    }
}
