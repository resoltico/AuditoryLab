//! Analytic amplitude envelopes on bounded, overlapping FFT windows.
use super::{Complex, EnvelopeBand, RealFftPlanner, Welch, usize_f64};
use realfft::ComplexToReal;
use std::sync::Arc;

pub(super) struct AnalyticEnvelope {
    inverse: Arc<dyn ComplexToReal<f64>>,
    bins: Vec<Complex<f64>>,
    scratch: Vec<Complex<f64>>,
    quadrature: Vec<f64>,
    low: Vec<f64>,
    low_quadrature: Vec<f64>,
}
impl AnalyticEnvelope {
    pub(super) fn new(n: usize) -> Self {
        let inverse = RealFftPlanner::new().plan_fft_inverse(n);
        Self {
            bins: inverse.make_input_vec(),
            scratch: inverse.make_scratch_vec(),
            quadrature: inverse.make_output_vec(),
            low: inverse.make_output_vec(),
            low_quadrature: inverse.make_output_vec(),
            inverse,
        }
    }
    pub(super) fn measure(&mut self, audio: &mut Welch, envelopes: &mut [Welch]) {
        let n = audio.window.len();
        // The tone spectrum already apodizes the same ordered samples with a
        // periodic Hann window. Reuse it to avoid a discontinuous FFT boundary.
        let reference = spectral_power(&audio.output, EnvelopeBand::Whole, audio.resolution());
        // Remove the Hann-weighted DC estimate before envelope analysis. The
        // periodic Hann's only nonzero DFT coefficients are N/2 at DC and −N/4
        // at ±1. Level/PSD statistics already retain the original DC energy.
        let dc = 2.0 * audio.output[0].re / usize_f64(n);
        audio.output[0] = Complex::new(0.0, 0.0);
        audio.output[1].re += dc * usize_f64(n) / 4.0;
        let total = spectral_power(&audio.output, EnvelopeBand::Whole, audio.resolution());
        if total <= reference * 1e-8 || reference == 0.0 {
            return;
        }
        let low_energy = spectral_power(&audio.output, EnvelopeBand::Below100, audio.resolution());
        self.transform(&audio.output, audio.resolution(), true, false);
        inverse(
            &self.inverse,
            &mut self.bins,
            &mut self.quadrature,
            &mut self.scratch,
        );
        self.transform(&audio.output, audio.resolution(), false, true);
        inverse(
            &self.inverse,
            &mut self.bins,
            &mut self.low,
            &mut self.scratch,
        );
        self.transform(&audio.output, audio.resolution(), true, true);
        inverse(
            &self.inverse,
            &mut self.bins,
            &mut self.low_quadrature,
            &mut self.scratch,
        );
        let energy = [total, low_energy, (total - low_energy).max(0.0)];
        for ((index, envelope), band) in envelopes.iter_mut().enumerate().zip(EnvelopeBand::ALL) {
            // Do not interpret filter leakage or numerical noise as a separate band.
            // This is the same −80 dB relative power floor used for tone detection.
            if energy[index] <= reference * 1e-8 {
                continue;
            }
            let (mut clock, mut count, mut sum) = (0_u64, 0_u32, 0.0);
            // Discard FFT boundary regions. The remaining central 75% overlaps
            // adjacent windows and avoids interpreting periodic extension edges.
            for i in n / 8..n - n / 8 {
                let scale = usize_f64(n) * audio.window[i];
                let x = audio.buffer[(i + audio.position) % n] - dc;
                let q = self.quadrature[i] / scale;
                let low = self.low[i] / scale;
                let low_q = self.low_quadrature[i] / scale;
                let amplitude = match band {
                    EnvelopeBand::Whole => x.hypot(q),
                    EnvelopeBand::Below100 => low.hypot(low_q),
                    EnvelopeBand::Above100 => (x - low).hypot(q - low_q),
                };
                sum += amplitude;
                count += 1;
                clock += 1000;
                if clock >= audio.rate_integer() {
                    clock -= audio.rate_integer();
                    let _ = envelope.push(sum / f64::from(count));
                    sum = 0.0;
                    count = 0;
                }
            }
            envelope.close_segment();
        }
    }
    fn transform(&mut self, source: &[Complex<f64>], resolution: f64, hilbert: bool, low: bool) {
        let last = source.len() - 1;
        for (bin, (target, value)) in self.bins.iter_mut().zip(source).enumerate() {
            *target = if (low && !EnvelopeBand::Below100.includes(usize_f64(bin) * resolution))
                || (hilbert && (bin == 0 || bin == last))
            {
                Complex::new(0.0, 0.0)
            } else if hilbert {
                Complex::new(value.im, -value.re)
            } else {
                *value
            };
        }
    }
}
// Unnormalized one-sided Parseval power; common normalization cancels in ratios.
fn spectral_power(spectrum: &[Complex<f64>], band: EnvelopeBand, resolution: f64) -> f64 {
    let last = spectrum.len() - 1;
    spectrum
        .iter()
        .enumerate()
        .filter(|(bin, _)| band.includes(usize_f64(*bin) * resolution))
        .map(|(bin, value)| value.norm_sqr() * if bin == 0 || bin == last { 1.0 } else { 2.0 })
        .sum()
}
fn inverse(
    plan: &Arc<dyn ComplexToReal<f64>>,
    bins: &mut [Complex<f64>],
    output: &mut [f64],
    scratch: &mut [Complex<f64>],
) {
    if plan.process_with_scratch(bins, output, scratch).is_err() {
        unreachable!("inverse FFT buffers created by their plan");
    }
}
