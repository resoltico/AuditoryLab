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
            merge(&mut out[bin], frame);
        }
        at += count as u64;
    }
}

/// A nonfinite sample cannot define a truthful waveform scale or level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonFiniteSample;
/// Reduce every finite sample into per-ear extrema. No sparse point selection.
/// Nonfinite input clears the output and fails rather than hiding a bad sample.
pub fn sample_trace(
    samples: &[[f64; 2]],
    bins: usize,
    out: &mut Vec<[Extent; 2]>,
) -> Result<(), NonFiniteSample> {
    out.clear();
    if samples.iter().flatten().any(|x| !x.is_finite()) {
        return Err(NonFiniteSample);
    }
    let count = bins.min(samples.len());
    if count == 0 {
        return Ok(());
    }
    out.resize(
        count,
        [Extent {
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }; 2],
    );
    for (i, frame) in samples.iter().enumerate() {
        merge(&mut out[i * count / samples.len()], frame);
    }
    Ok(())
}
fn merge(bin: &mut [Extent; 2], frame: &[f64; 2]) {
    for ear in 0..2 {
        bin[ear].min = bin[ear].min.min(frame[ear]);
        bin[ear].max = bin[ear].max.max(frame[ear]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_impulse_dc_and_polarity_survives_reduction() {
        let mut samples = [[0.25, -0.125]; 33];
        samples[3] = [1.0, -1.0];
        samples[29] = [-0.9, 0.8];
        let mut out = Vec::new();
        assert_eq!(sample_trace(&samples, 3, &mut out), Ok(()));
        assert_eq!(
            out[0],
            [
                Extent {
                    min: 0.25,
                    max: 1.0
                },
                Extent {
                    min: -1.0,
                    max: -0.125
                }
            ]
        );
        assert_eq!(
            out[1],
            [
                Extent {
                    min: 0.25,
                    max: 0.25
                },
                Extent {
                    min: -0.125,
                    max: -0.125
                }
            ]
        );
        assert_eq!(
            out[2],
            [
                Extent {
                    min: -0.9,
                    max: 0.25
                },
                Extent {
                    min: -0.125,
                    max: 0.8
                }
            ]
        );
        assert_ne!(samples[0][0], out[0][0].max); // sparse negative control misses the impulse
        samples[32][1] = f64::NAN;
        assert_eq!(sample_trace(&samples, 0, &mut out), Err(NonFiniteSample));
        assert!(out.is_empty());
        assert_eq!(sample_trace(&[], 8, &mut out), Ok(()));
        assert!(out.is_empty());
    }
}
