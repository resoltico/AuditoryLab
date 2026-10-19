//! Exact per-ear extrema for a window of the signal.
use crate::Signal;
use alloc::vec::Vec;
/// The lowest and highest sample in a bin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Extent {
    /// Minimum sample.
    pub min: f64,
    /// Maximum sample.
    pub max: f64,
}
/// Trace every frame of a clipped window, reusing the output allocation.
pub fn trace(signal: &Signal, start: u64, frames: u64, bins: usize, out: &mut Vec<[Extent; 2]>) {
    out.clear();
    let end = start.saturating_add(frames).min(signal.frames());
    if end <= start || bins == 0 {
        return;
    }
    let span = end - start;
    let bins = bins.min(usize::try_from(span).unwrap_or(usize::MAX));
    out.resize(
        bins,
        [Extent {
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }; 2],
    );
    let mut block = [[0.0; 2]; 256];
    let mut at = start;
    while at < end {
        let count = usize::try_from((end - at).min(256)).unwrap_or(256);
        signal.render(at, &mut block[..count]);
        for (offset, frame) in block[..count].iter().enumerate() {
            let bin = ((u128::from(at - start) + offset as u128) * bins as u128 / u128::from(span))
                as usize;
            for ear in 0..2 {
                out[bin][ear].min = out[bin][ear].min.min(frame[ear]);
                out[bin][ear].max = out[bin][ear].max.max(frame[ear]);
            }
        }
        at += count as u64;
    }
}
