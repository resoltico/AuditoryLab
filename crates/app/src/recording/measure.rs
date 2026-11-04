//! Streaming Welch measurements of mono or stereo digital samples, without I/O.
use super::{
    Analysis, AnalysisWindow, EarDifference, EnergyShare, EnvelopeBand, Pulsing, PulsingCause,
    RecordingError, SpectrumPoint, Stereo, Tone,
};
use crate::{core::rhythm::Band, job::Cancel};
use realfft::{RealFftPlanner, RealToComplex, num_complex::Complex};
use std::sync::Arc;
mod envelope;
use envelope::AnalyticEnvelope;
/// Minimum envelope peak power relative to squared mean amplitude, excluding FFT edge leakage.
pub const MIN_ENVELOPE_RELATIVE_POWER: f64 = 1e-6;

/// Accumulates a fixed captured window without retaining whole-file audio.
/// Input frames must be finite and match the channel count declared at construction.
pub struct Measurement {
    rate: u32,
    channels: usize,
    expected: u64,
    frames: u64,
    spectra: Vec<Welch>,
    envelopes: Vec<Vec<Welch>>,
    analytic: AnalyticEnvelope,
    energy: Vec<f64>,
    peak: Vec<f64>,
    sum: Vec<f64>,
    cross: f64,
    mid: f64,
    side: f64,
}
impl Measurement {
    /// Prepare bounded overlapping windows. At least two seconds are required.
    pub fn new(rate: u32, channels: usize, frames: u64) -> Result<Self, RecordingError> {
        if !(1..=2).contains(&channels) {
            return Err(RecordingError::UnsupportedChannels { channels });
        }
        if rate < 1000 || frames < u64::from(rate) * 2 {
            return Err(RecordingError::TooShort);
        }
        let n = window_size(frames, 1 << 20);
        let envelope_frames = (n * 3 / 4) as u64 * 1000 / u64::from(rate);
        let envelope_n = window_size(envelope_frames.max(2), 1 << 18);
        Ok(Self {
            rate,
            channels,
            expected: frames,
            frames: 0,
            spectra: (0..channels)
                .map(|_| Welch::new(n, f64::from(rate), false))
                .collect(),
            envelopes: (0..channels)
                .map(|_| {
                    EnvelopeBand::ALL
                        .into_iter()
                        .map(|_| Welch::new(envelope_n, 1000.0, true))
                        .collect()
                })
                .collect(),
            analytic: AnalyticEnvelope::new(n),
            energy: vec![0.0; channels],
            peak: vec![0.0; channels],
            sum: vec![0.0; channels],
            cross: 0.0,
            mid: 0.0,
            side: 0.0,
        })
    }
    /// Feed one frame in channel order. Reject malformed samples before changing state.
    pub fn push(&mut self, frame: &[f32]) -> Result<(), RecordingError> {
        if frame.len() != self.channels
            || frame.iter().any(|sample| !sample.is_finite())
            || self.frames >= self.expected
        {
            return Err(RecordingError::Decode {
                packet: 0,
                cause: "invalid measurement frame".into(),
            });
        }
        for (channel, &sample) in frame.iter().enumerate() {
            let x = f64::from(sample);
            let completed_window = self.spectra[channel].push(x);
            self.energy[channel] += x * x;
            self.sum[channel] += x;
            self.peak[channel] = self.peak[channel].max(x.abs());
            if completed_window {
                self.analytic
                    .measure(&mut self.spectra[channel], &mut self.envelopes[channel]);
            }
        }
        if self.channels == 2 {
            let (left, right) = (f64::from(frame[0]), f64::from(frame[1]));
            self.cross += left * right;
            self.mid += (left + right).powi(2);
            self.side += (left - right).powi(2);
        }
        self.frames += 1;
        Ok(())
    }
    /// Finish a complete window. Cancellation produces no partial findings.
    pub fn finish(mut self, cancel: &Cancel) -> Result<Analysis, RecordingError> {
        if self.frames != self.expected {
            return Err(RecordingError::TooShort);
        }
        if cancel.is_requested() {
            return Err(RecordingError::Cancelled);
        }
        for channel in 0..self.channels {
            if self.spectra[channel].since_window > 0 {
                self.analytic
                    .measure(&mut self.spectra[channel], &mut self.envelopes[channel]);
            }
            self.spectra[channel].finish();
        }
        let resolution = self.spectra[0].resolution();
        let envelope_resolution = self.envelopes[0][0].resolution();
        let tones: Vec<Tone> = self
            .spectra
            .iter()
            .enumerate()
            .flat_map(|(channel, spectrum)| {
                let peaks = spectrum.peaks(0.0, 2000.0, 12.0);
                let strongest = peaks.first().map_or(0.0, |peak| peak.power);
                peaks.into_iter().map(move |peak| Tone {
                    channel,
                    hz: peak.hz,
                    dbfs: power_db(peak.power).unwrap_or(-300.0),
                    relative_db: power_db(peak.power / strongest).unwrap_or(0.0),
                })
            })
            .collect();
        let ear_differences = ear_differences(&tones, &self.spectra, resolution);
        let coincident_tones = self.channels == 2
            && !tones.is_empty()
            && tones.iter().all(|tone| {
                tones.iter().any(|other| {
                    other.channel != tone.channel && (tone.hz - other.hz).abs() <= 2.0 * resolution
                })
            });
        let pulsing = self.pulsing_findings(&tones, cancel)?;
        let low_energy = self.energy_shares(resolution);
        let frames = count_f64(self.frames);
        let correlation = if self.channels == 2 {
            let left = self.energy[0] - self.sum[0].powi(2) / frames;
            let right = self.energy[1] - self.sum[1].powi(2) / frames;
            let denominator = (left * right).sqrt();
            (denominator > 0.0).then(|| {
                ((self.cross - self.sum[0] * self.sum[1] / frames) / denominator).clamp(-1.0, 1.0)
            })
        } else {
            None
        };
        let stereo = Stereo {
            peak_dbfs: self.peak.iter().map(|peak| power_db(peak * peak)).collect(),
            rms_dbfs: self
                .energy
                .iter()
                .map(|energy| power_db(energy / frames))
                .collect(),
            correlation,
            side_to_mid_db: if self.channels == 2 && self.mid > 0.0 {
                power_db(self.side / self.mid)
            } else {
                None
            },
        };
        let spectrum = self.spectra.iter().map(Welch::chart).collect();
        Ok(Analysis {
            codec: String::new(),
            lossy: false,
            duration_seconds: frames / f64::from(self.rate),
            sample_rate: self.rate,
            channels: self.channels,
            window: AnalysisWindow {
                start_seconds: 0.0,
                end_seconds: frames / f64::from(self.rate),
            },
            envelope_window: AnalysisWindow {
                start_seconds: usize_f64(self.spectra[0].window.len() / 8) / f64::from(self.rate),
                end_seconds: (frames - usize_f64(self.spectra[0].window.len() / 8))
                    / f64::from(self.rate),
            },
            resolution_hz: resolution,
            envelope_resolution_hz: envelope_resolution,
            tones,
            ear_differences,
            coincident_tones,
            pulsing,
            low_energy,
            stereo,
            spectrum,
            minimum_envelope_relative_power: MIN_ENVELOPE_RELATIVE_POWER,
            limits: vec![
                super::MeasurementLimit::DigitalSamplesOnly,
                super::MeasurementLimit::QuietPairsMayBeMissed,
                super::MeasurementLimit::PatternAttributionNotIntent,
                super::MeasurementLimit::PeaksBelowRelativeFloorOmitted,
            ],
        })
    }
    fn energy_shares(&self, resolution: f64) -> Vec<EnergyShare> {
        let mut low_energy = Vec::new();
        for (channel, spectrum) in self.spectra.iter().enumerate() {
            let total: f64 = spectrum.psd.iter().sum();
            for below_hz in [50.0, 60.0, 100.0, 300.0, 40.0] {
                let energy: f64 = spectrum
                    .psd
                    .iter()
                    .enumerate()
                    .take_while(|(bin, _)| usize_f64(*bin) * resolution < below_hz)
                    .map(|(_, power)| power)
                    .sum();
                low_energy.push(EnergyShare {
                    channel,
                    below_hz,
                    fraction: (total > 0.0).then(|| (energy / total).clamp(0.0, 1.0)),
                });
            }
        }
        low_energy
    }
    fn pulsing_findings(
        &mut self,
        tones: &[Tone],
        cancel: &Cancel,
    ) -> Result<Vec<Pulsing>, RecordingError> {
        let resolution = self.spectra[0].resolution();
        let envelope_resolution = self.envelopes[0][0].resolution();
        let mut pulsing = Vec::new();
        for channel in 0..self.channels {
            for (index, band) in EnvelopeBand::ALL.into_iter().enumerate() {
                if cancel.is_requested() {
                    return Err(RecordingError::Cancelled);
                }
                let spectrum = &mut self.envelopes[channel][index];
                spectrum.finish();
                for peak in spectrum.peaks(0.5, 100.0, 15.0) {
                    pulsing.push(Pulsing {
                        channel,
                        envelope_band: band,
                        hz: peak.hz,
                        band: Band::of(peak.hz),
                        cause: pulsing_cause(
                            peak.hz,
                            tones,
                            channel,
                            2.0 * resolution.max(envelope_resolution),
                        ),
                    });
                }
            }
        }
        Ok(pulsing)
    }
}
fn window_size(frames: u64, max: usize) -> usize {
    let bounded = usize::try_from(frames).unwrap_or(max).min(max);
    1 << bounded.ilog2()
}
#[expect(
    clippy::cast_precision_loss,
    reason = "bounded FFT indices converted to frequency coordinates"
)]
fn usize_f64(value: usize) -> f64 {
    value as f64
}
#[expect(
    clippy::cast_precision_loss,
    reason = "frame counts converted to seconds and averages"
)]
fn count_f64(value: u64) -> f64 {
    value as f64
}
fn power_db(power: f64) -> Option<f64> {
    (power > 0.0).then(|| 10.0 * power.log10())
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "positive finite frequency coordinate clamped to the PSD bounds"
)]
fn frequency_bin(hz: f64, resolution: f64, length: usize) -> usize {
    ((hz / resolution).round() as usize).min(length - 1)
}

struct Welch {
    buffer: Vec<f64>,
    window: Vec<f64>,
    fft: Arc<dyn RealToComplex<f64>>,
    input: Vec<f64>,
    output: Vec<Complex<f64>>,
    scratch: Vec<Complex<f64>>,
    psd: Vec<f64>,
    windows: u32,
    rate: f64,
    centered: bool,
    mean_power: f64,
    since_window: usize,
    position: usize,
}
struct Peak {
    hz: f64,
    power: f64,
}
impl Welch {
    fn new(n: usize, rate: f64, centered: bool) -> Self {
        let fft = RealFftPlanner::new().plan_fft_forward(n);
        Self {
            buffer: Vec::with_capacity(n),
            window: (0..n)
                .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * usize_f64(i) / usize_f64(n)).cos())
                .collect(),
            input: fft.make_input_vec(),
            output: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            psd: vec![0.0; n / 2 + 1],
            fft,
            windows: 0,
            rate,
            centered,
            mean_power: 0.0,
            since_window: 0,
            position: 0,
        }
    }
    fn rate_integer(&self) -> u64 {
        // Both constructors use exact integral file or envelope sample rates.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "positive integral sample rates represented exactly as f64"
        )]
        let rate = self.rate as u64;
        rate
    }
    fn close_segment(&mut self) {
        if self.since_window > 0 && self.buffer.len() == self.window.len() {
            self.accumulate();
        }
        self.buffer.clear();
        self.position = 0;
        self.since_window = 0;
    }
    fn resolution(&self) -> f64 {
        self.rate / usize_f64(self.window.len())
    }
    fn push(&mut self, sample: f64) -> bool {
        if self.buffer.len() < self.window.len() {
            self.buffer.push(sample);
            if self.buffer.len() == self.window.len() {
                self.accumulate();
                self.since_window = 0;
                return true;
            }
        } else {
            self.buffer[self.position] = sample;
            self.position = (self.position + 1) % self.window.len();
            self.since_window += 1;
            if self.since_window == self.window.len() / 2 {
                self.accumulate();
                self.since_window = 0;
                return true;
            }
        }
        false
    }
    fn accumulate(&mut self) {
        let mean = if self.centered {
            self.buffer.iter().sum::<f64>() / usize_f64(self.buffer.len())
        } else {
            0.0
        };
        self.mean_power += mean * mean;
        for (i, (input, window)) in self.input.iter_mut().zip(&self.window).enumerate() {
            *input = (self.buffer[(i + self.position) % self.window.len()] - mean) * window;
        }
        // All dimensions come from the same FFT plan, so this cannot be a length failure.
        if self
            .fft
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .is_err()
        {
            unreachable!("FFT buffers created by their plan");
        }
        let scale = self.rate * self.window.iter().map(|w| w * w).sum::<f64>();
        let last = self.psd.len() - 1;
        for (bin, (sum, value)) in self.psd.iter_mut().zip(&self.output).enumerate() {
            *sum += value.norm_sqr() * if bin == 0 || bin == last { 1.0 } else { 2.0 } / scale;
        }
        self.windows += 1;
    }
    fn finish(&mut self) {
        // Include the last full window when a partial hop remains; no padding or
        // samples outside the captured interval contribute to the periodogram.
        if self.since_window > 0 {
            self.accumulate();
            self.since_window = 0;
        }
        if self.windows > 0 {
            self.mean_power /= f64::from(self.windows);
            for power in &mut self.psd {
                *power /= f64::from(self.windows);
            }
        }
    }
    fn peaks(&self, low: f64, high: f64, prominence_db: f64) -> Vec<Peak> {
        let resolution = self.resolution();
        let strongest = self.psd.iter().copied().fold(0.0, f64::max);
        let mut peaks = Vec::new();
        let envelope_median = if self.centered {
            let start = frequency_bin(low, resolution, self.psd.len());
            let end = frequency_bin(high, resolution, self.psd.len());
            let mut values = self.psd[start..=end].to_vec();
            let middle = values.len() / 2;
            values.select_nth_unstable_by(middle, f64::total_cmp);
            Some(values[middle])
        } else {
            None
        };
        for bin in 1..self.psd.len() - 1 {
            let hz = usize_f64(bin) * resolution;
            let power = self.psd[bin];
            if hz < low
                || hz > high
                || power <= self.psd[bin - 1]
                || power < self.psd[bin + 1]
                || power <= strongest * 1e-8
                || power <= 1e-24
            {
                continue;
            }
            // Local median over ±2 Hz for tones; envelopes use the measured 0.5–100 Hz band.
            let median = if let Some(median) = envelope_median {
                median
            } else {
                let radius = frequency_bin(2.0, resolution, self.psd.len()).max(4);
                let start = bin.saturating_sub(radius);
                let end = (bin + radius + 1).min(self.psd.len());
                let mut local: Vec<f64> = self.psd[start..end]
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| (i + start).abs_diff(bin) > 2)
                    .map(|(_, power)| *power)
                    .collect();
                let middle = local.len() / 2;
                local.select_nth_unstable_by(middle, f64::total_cmp);
                local[middle]
            };
            if power < median.max(1e-30) * 10.0_f64.powf(prominence_db / 10.0) {
                continue;
            }
            let (a, b, c) = (
                self.psd[bin - 1].max(1e-300).ln(),
                power.ln(),
                self.psd[bin + 1].max(1e-300).ln(),
            );
            let offset = (0.5 * (a - c) / (a - 2.0 * b + c)).clamp(-0.5, 0.5);
            let integrated: f64 = self.psd
                [bin.saturating_sub(2)..=(bin + 2).min(self.psd.len() - 1)]
                .iter()
                .sum::<f64>()
                * resolution;
            if self.centered && integrated < self.mean_power * MIN_ENVELOPE_RELATIVE_POWER {
                continue;
            }
            peaks.push(Peak {
                hz: (usize_f64(bin) + offset) * resolution,
                power: integrated,
            });
        }
        peaks.sort_by(|a, b| b.power.total_cmp(&a.power));
        peaks.truncate(16);
        peaks
    }
    fn power_at(&self, hz: f64) -> f64 {
        let bin = frequency_bin(hz, self.resolution(), self.psd.len());
        self.psd[bin.saturating_sub(1)..=(bin + 1).min(self.psd.len() - 1)]
            .iter()
            .sum()
    }
    fn chart(&self) -> Vec<SpectrumPoint> {
        // At most 400 log-spaced buckets, retaining narrow peaks for the native chart.
        let mut chart = Vec::new();
        for bucket in 0..400 {
            let low = 20.0 * 100.0_f64.powf(f64::from(bucket) / 400.0);
            let high = 20.0 * 100.0_f64.powf(f64::from(bucket + 1) / 400.0);
            if low >= self.rate / 2.0 {
                break;
            }
            let start = frequency_bin(low, self.resolution(), self.psd.len());
            let end = frequency_bin(high, self.resolution(), self.psd.len());
            let power = self.psd[start..=end].iter().copied().fold(0.0, f64::max);
            chart.push(SpectrumPoint {
                hz: (low * high).sqrt(),
                dbfs_per_hz: power_db(power).unwrap_or(-160.0).max(-160.0),
            });
        }
        chart
    }
}
fn ear_differences(tones: &[Tone], spectra: &[Welch], resolution: f64) -> Vec<EarDifference> {
    if spectra.len() != 2 {
        return Vec::new();
    }
    let mut pairs = Vec::new();
    for left in tones.iter().filter(|tone| tone.channel == 0) {
        for right in tones.iter().filter(|tone| tone.channel == 1) {
            let difference = (left.hz - right.hz).abs();
            if !(0.5..=100.0).contains(&difference)
                || difference <= 2.0 * resolution
                || (left.dbfs - right.dbfs).abs() > 6.0
            {
                continue;
            }
            if spectra[0].power_at(left.hz) >= spectra[1].power_at(left.hz) * 10.0_f64.powf(1.5)
                && spectra[1].power_at(right.hz)
                    >= spectra[0].power_at(right.hz) * 10.0_f64.powf(1.5)
            {
                pairs.push(EarDifference {
                    left_hz: left.hz,
                    right_hz: right.hz,
                    difference_hz: difference,
                    band: Band::of(difference),
                });
            }
        }
    }
    pairs
}
fn pulsing_cause(hz: f64, tones: &[Tone], channel: usize, tolerance: f64) -> PulsingCause {
    let tones: Vec<&Tone> = tones
        .iter()
        .filter(|tone| tone.channel == channel)
        .collect();
    // Symmetric sidebands and harmonic trains are consistent with modulation.
    // Their origin is not identifiable from samples, so reports disclose this convention.
    if tones.iter().any(|center| {
        tones
            .iter()
            .any(|low| (center.hz - low.hz - hz).abs() <= tolerance)
            && tones
                .iter()
                .any(|high| (high.hz - center.hz - hz).abs() <= tolerance)
    }) {
        return PulsingCause::Modulation;
    }
    // Prefer an actual pair difference before attributing higher envelope harmonics.
    // Differences below 0.5 Hz fall outside the measured envelope range.
    for harmonic in 1..=200 {
        for (i, first) in tones.iter().enumerate() {
            for second in &tones[i + 1..] {
                let difference = (first.hz - second.hz).abs();
                if difference >= 0.5 && (difference * f64::from(harmonic) - hz).abs() <= tolerance {
                    return PulsingCause::Beating {
                        first_hz: first.hz,
                        second_hz: second.hz,
                        harmonic,
                    };
                }
            }
        }
    }
    PulsingCause::Modulation
}

#[cfg(test)]
mod tests;
