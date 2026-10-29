// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::synthesis::{self, Carrier as Generator, GeneratorLane, Modulator};
use crate::{Channel, ChannelRouting, Modulation, ParameterId, SampleRate, VoiceError, conversion};

/// Cold source-lane definition; the compiled component owns modulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SourceLane {
    /// Channel routing.
    pub(crate) route: ChannelRouting,
    /// Peak amplitude in dBFS.
    pub(crate) peak_dbfs: f64,
    /// Periodic sound source.
    pub(crate) carrier: Carrier,
    /// Optional amplitude modulation.
    pub(crate) modulation: Option<Modulation>,
}
/// A periodic sound source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Carrier {
    /// A sine wave.
    Tone {
        /// Frequency in hertz.
        frequency_hz: f64,
    },
    /// An authored unipolar pulse train.
    PulseTrain {
        /// Repetition, width and edges.
        pulse: crate::PulseSpec,
    },
}
impl SourceLane {
    /// Check every value at `rate` and build the generator.
    pub(crate) fn compile(&self, rate: SampleRate) -> Result<GeneratorLane, VoiceError> {
        let peak_dbfs = self.peak_dbfs;
        if crate::validity::check_value(ParameterId::PeakLevel, peak_dbfs).is_err() {
            return Err(VoiceError::PeakDbfs { peak_dbfs });
        }
        let carrier = match self.carrier {
            Carrier::Tone { frequency_hz, .. } => Generator::Tone {
                step: frequency_step(ParameterId::ToneFrequency, frequency_hz, rate)?,
            },
            Carrier::PulseTrain { pulse } => {
                Generator::Pulse(synthesis::pulse_spec(pulse, rate.hz()).map_err(|error| {
                    VoiceError::Pulse {
                        modulation: false,
                        width: pulse.width,
                        rate_hz: pulse.rate_hz.get(),
                        sample_rate_hz: rate.hz(),
                        error,
                    }
                })?)
            }
        };
        compile_modulation(self.modulation, rate)?;
        check_sideband(self, self.modulation, rate)?;
        let route = self.route;
        Ok(GeneratorLane {
            left: route.includes(Channel::Left),
            right: route.includes(Channel::Right),
            amplitude: conversion::amplitude(peak_dbfs),
            carrier,
        })
    }
}

pub(crate) fn compile_modulation(
    requested: Option<Modulation>,
    rate: SampleRate,
) -> Result<Option<Modulator>, VoiceError> {
    let modulation = match requested {
        None => None,
        Some(Modulation::Sinusoidal { rate_hz, depth }) => {
            let (rate_hz, depth) = (rate_hz.get(), depth.get());
            let step = frequency_step(ParameterId::ModulationRate, rate_hz, rate)?;
            if crate::validity::check_value(ParameterId::ModulationDepth, depth).is_err() {
                return Err(VoiceError::Depth { depth });
            }
            Some(Modulator::Sinusoidal { step, depth })
        }
        Some(Modulation::Pulsed { pulse }) => Some(Modulator::Pulsed(
            synthesis::pulse_spec(pulse, rate.hz()).map_err(|error| VoiceError::Pulse {
                modulation: true,
                width: pulse.width,
                rate_hz: pulse.rate_hz.get(),
                sample_rate_hz: rate.hz(),
                error,
            })?,
        )),
    };
    Ok(modulation)
}
pub(crate) fn check_sideband(
    source: &SourceLane,
    modulation: Option<Modulation>,
    rate: SampleRate,
) -> Result<(), VoiceError> {
    if let (Carrier::Tone { frequency_hz }, Some(Modulation::Sinusoidal { rate_hz, depth })) =
        (source.carrier, modulation)
        && depth.get() > 0.0
        && frequency_hz + rate_hz.get() >= rate.nyquist_hz()
    {
        return Err(VoiceError::Sideband {
            frequency_hz,
            rate_hz: rate_hz.get(),
            sample_rate_hz: rate.hz(),
        });
    }
    Ok(())
}
fn frequency_step(parameter: ParameterId, hz: f64, rate: SampleRate) -> Result<u64, VoiceError> {
    crate::validity::check_value(parameter, hz)
        .ok()
        .and_then(|()| synthesis::step(hz, rate.hz()))
        .ok_or(VoiceError::Frequency {
            parameter,
            hz,
            sample_rate_hz: rate.hz(),
        })
}
#[cfg(test)]
mod tests;
