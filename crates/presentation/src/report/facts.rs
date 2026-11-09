//! Explicit schema-4 generator facts; model field changes cannot silently change wire shape.
use auditorylab_core::{self as core, ChannelRouting, ComponentId, PairKind, VoiceFacts};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(super) struct Facts {
    pair: Option<Pair>,
    route: ChannelRouting,
    peak_dbfs: f64,
    carrier: Carrier,
    modulation: Option<Modulation>,
}
#[derive(Debug, Clone, Serialize)]
struct Rate {
    requested_hz: f64,
    realized_hz: f64,
}
impl From<core::PhaseRate> for Rate {
    fn from(value: core::PhaseRate) -> Self {
        Self {
            requested_hz: value.requested_hz,
            realized_hz: value.realized_hz,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
struct Pair {
    owner: ComponentId,
    kind: PairKind,
    first_hz: f64,
    offset_hz: f64,
    second_hz: f64,
    realized_offset_hz: Option<f64>,
}
#[derive(Debug, Clone, Serialize)]
struct Pulse {
    rate_hz: f64,
    requested_width_seconds: f64,
    width_frames: u64,
    sample_rate: u32,
    width: Width,
    shape: Shape,
    edge_frames: [u64; 2],
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Width {
    Duration { seconds: f64 },
    DutyCycle { fraction: f64 },
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Shape {
    Rectangular,
    RaisedCosine {
        rise_seconds: f64,
        fall_seconds: f64,
    },
}
impl From<core::PulseFacts> for Pulse {
    fn from(value: core::PulseFacts) -> Self {
        Self {
            rate_hz: value.rate_hz,
            requested_width_seconds: value.requested_width_seconds,
            width_frames: value.width_frames,
            sample_rate: value.sample_rate.hz(),
            edge_frames: value.edge_frames,
            width: match value.width {
                core::PulseWidth::Duration { seconds } => Width::Duration {
                    seconds: seconds.get(),
                },
                core::PulseWidth::DutyCycle { fraction } => Width::DutyCycle {
                    fraction: fraction.get(),
                },
            },
            shape: match value.shape {
                core::PulseShape::Rectangular {} => Shape::Rectangular,
                core::PulseShape::RaisedCosine {
                    rise_seconds,
                    fall_seconds,
                } => Shape::RaisedCosine {
                    rise_seconds: rise_seconds.get(),
                    fall_seconds: fall_seconds.get(),
                },
            },
        }
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Carrier {
    Tone(Rate),
    PulseTrain(Pulse),
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Modulation {
    Sinusoidal { rate: Rate, depth: f64 },
    Pulsed(Pulse),
}
impl From<VoiceFacts> for Facts {
    fn from(value: VoiceFacts) -> Self {
        Self {
            pair: value.pair.map(|p| Pair {
                owner: p.owner,
                kind: p.kind,
                first_hz: p.first_hz,
                offset_hz: p.offset_hz,
                second_hz: p.second_hz,
                realized_offset_hz: p.realized_offset_hz,
            }),
            route: value.route,
            peak_dbfs: value.peak_dbfs,
            carrier: match value.carrier {
                core::CarrierFacts::Tone(rate) => Carrier::Tone(rate.into()),
                core::CarrierFacts::PulseTrain(pulse) => Carrier::PulseTrain(pulse.into()),
            },
            modulation: value.modulation.map(|m| match m {
                core::ModulationFacts::Sinusoidal { rate, depth } => Modulation::Sinusoidal {
                    rate: rate.into(),
                    depth,
                },
                core::ModulationFacts::Pulsed(pulse) => Modulation::Pulsed(pulse.into()),
            }),
        }
    }
}
