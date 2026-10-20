//! Bounded origin-zero timing and envelope projections, independent of live phase history.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bounded positive model frames, bins and finite validated phase/rate quantities"
)]
use crate::{
    CarrierFacts, ComponentFacts, Extent, ModulationFacts, SampleRate,
    synthesis::{Carrier, Modulator, Voice, sine},
};
use alloc::vec::Vec;
/// Physical process represented by a model lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    /// A continuous carrier, with no invented low-rate pulsing.
    Steady,
    /// Exact frame-quantized click schedule.
    Clicks,
    /// Exact frame-quantized gate schedule.
    Gate,
    /// Sinusoidal amplitude envelope bounds, not carrier waveform samples.
    Envelope,
}
/// One independently attributed process. Dense periods aggregate into bounded columns.
#[derive(Debug, Clone)]
pub struct ModelLane {
    /// Caller component identity.
    pub id: u32,
    /// Process type.
    pub kind: ModelKind,
    /// Validated physical quantities.
    pub facts: ComponentFacts,
    /// Per-column envelope bounds/occupancy over every frame, not point subsampling.
    pub columns: Vec<Extent>,
}
/// A static target projection, explicitly anchored at frame zero.
#[derive(Debug, Clone)]
pub struct TimingModel {
    /// Model sample rate, independent of output elapsed time.
    pub rate: SampleRate,
    /// Exclusive end frame, from origin zero; at most four seconds.
    pub frames: u64,
    /// A slow rhythmic process cannot complete one cycle in the capped span.
    pub insufficient_cycle: bool,
    /// Up to two physical processes per component; at most 32 lanes × 512 columns.
    pub lanes: Vec<ModelLane>,
}
pub(crate) fn project(
    rate: SampleRate,
    voices: &[Option<(u32, Voice)>; 16],
    facts: &[Option<ComponentFacts>; 16],
    bins: usize,
) -> TimingModel {
    let mut slowest = f64::INFINITY;
    for (entry, facts) in voices.iter().zip(facts) {
        let (Some((_, voice)), Some(f)) = (entry, facts) else {
            continue;
        };
        if voice.amplitude == 0.0 {
            continue;
        }
        if let CarrierFacts::Clicks(p) = f.carrier {
            slowest = slowest.min(p.rate_hz);
        }
        match f.modulation {
            Some(ModulationFacts::Gate(p)) => slowest = slowest.min(p.rate_hz),
            Some(ModulationFacts::Sinusoidal { rate, depth }) if depth > 0.0 => {
                slowest = slowest.min(rate.realized_hz)
            }
            _ => {}
        }
    }
    let seconds = (4.0 / slowest).clamp(0.25, 4.0);
    let frames = libm::ceil(seconds * f64::from(rate.hz())).max(1.0) as u64;
    let count = bins.clamp(1, 512).min(frames as usize);
    let mut model = TimingModel {
        rate,
        frames,
        insufficient_cycle: slowest.is_finite() && seconds * slowest < 1.0,
        lanes: Vec::new(),
    };
    for (entry, facts) in voices.iter().zip(facts) {
        let (Some((id, voice)), Some(facts)) = (entry, facts) else {
            continue;
        };
        if voice.amplitude == 0.0 {
            continue;
        }
        let mut lane = |kind, envelope: &dyn Fn(u64, u64) -> Extent| {
            let columns = (0..count)
                .map(|i| {
                    // Same frame-to-column partition as the all-sample extrema reducer.
                    let start = (i as u64 * frames).div_ceil(count as u64);
                    let end = ((i + 1) as u64 * frames).div_ceil(count as u64);
                    envelope(start, end)
                })
                .collect();
            model.lanes.push(ModelLane {
                id: *id,
                kind,
                facts: *facts,
                columns,
            });
        };
        if let Carrier::Pulse(pulse) = voice.carrier {
            lane(ModelKind::Clicks, &|s, e| pulse.extent(s, e));
        }
        match voice.modulation {
            Some(Modulator::Rectangular(pulse)) => {
                lane(ModelKind::Gate, &|s, e| pulse.extent(s, e))
            }
            Some(Modulator::Sinusoidal { step, depth }) if depth > 0.0 => {
                lane(ModelKind::Envelope, &|s, e| envelope(step, depth, s, e))
            }
            _ if matches!(voice.carrier, Carrier::Tone { .. }) => {
                lane(ModelKind::Steady, &|_, _| Extent { min: 1.0, max: 1.0 })
            }
            _ => {}
        }
    }
    model
}
// Bounds of the continuous phase arc used by the discrete synthesizer. A dense
// column covering an entire period is the full envelope range, not a sampled peak.
fn envelope(step: u64, depth: f64, start: u64, end: u64) -> Extent {
    let a = u128::from(start) * u128::from(step);
    let b = u128::from(end - 1) * u128::from(step);
    let cycle = 1_u128 << 64;
    let mut low = sine(a as u64).min(sine(b as u64));
    let mut high = sine(a as u64).max(sine(b as u64));
    let crosses = |offset: u128| {
        let first = if a <= offset {
            offset
        } else {
            offset + (a - offset).div_ceil(cycle) * cycle
        };
        first <= b
    };
    if crosses(cycle / 4) {
        high = 1.0;
    }
    if crosses(cycle * 3 / 4) {
        low = -1.0;
    }
    Extent {
        min: (1.0 + depth * low) / (1.0 + depth),
        max: (1.0 + depth * high) / (1.0 + depth),
    }
}
