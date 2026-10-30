//! Comfortable slider spans and deliberately quantized slider gestures.
//!
//! This is control convenience, not validation: typed values outside a
//! slider's span are kept exactly and validated by the core.
use auditorylab_core::{ModulationKind, ParameterId, SampleRate};

/// Waveform spans offered by application monitors, in milliseconds.
pub const MONITOR_SPANS_MS: [u32; 4] = [2, 10, 40, 100];
/// Initial waveform span.
pub const DEFAULT_MONITOR_SPAN_MS: u32 = 40;
/// Initial waveform scale.
pub const DEFAULT_MONITOR_SCALE: crate::workspace::monitor::WaveformScale =
    crate::workspace::monitor::WaveformScale::Fit;

/// Mapping from a slider position to a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    /// Equal positions give equal value differences.
    Linear,
    /// Equal positions give equal value ratios.
    Logarithmic,
}

/// A comfortable slider span in the quantity's displayed unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    /// Slider's lower endpoint.
    pub min: f64,
    /// Slider's upper endpoint.
    pub max: f64,
    /// Initial displayed value for a new component.
    pub default: f64,
    /// Slider mapping.
    pub scale: Scale,
}

/// Sample rates offered for inspection and export.
pub const EXPORT_RATES: [SampleRate; 5] = [
    rate(44_100),
    rate(48_000),
    rate(88_200),
    rate(96_000),
    rate(192_000),
];

const fn rate(hz: u32) -> SampleRate {
    match SampleRate::new(hz) {
        Ok(rate) => rate,
        Err(_) => panic!("offered export rates are supported sample rates"),
    }
}

/// The slider span of a component quantity at a sample rate; `None` for
/// quantities without a slider.
#[must_use]
pub fn range(id: ParameterId, rate: SampleRate) -> Option<Range> {
    use Scale::{Linear, Logarithmic};
    let (min, max, default, scale) = match id {
        ParameterId::ToneFrequency | ParameterId::PairFirstFrequency => {
            let max = 20_000.0_f64.min(0.45 * f64::from(rate.hz()));
            let min = 20.0_f64.min(max / 2.0);
            (
                min,
                max,
                (if id == ParameterId::PairFirstFrequency {
                    200.0_f64
                } else {
                    440.0_f64
                })
                .clamp(min, max),
                Logarithmic,
            )
        }
        ParameterId::PairOffset => (-80.0, 80.0, 6.0, Linear),
        ParameterId::SourcePulseRate => (0.5, 1000.0, 40.0, Logarithmic),
        ParameterId::SourcePulseDuration | ParameterId::ModulationPulseDuration => {
            (0.1, 50.0, 1.0, Logarithmic)
        }
        ParameterId::PeakLevel | ParameterId::PairFirstLevel | ParameterId::PairSecondLevel => {
            (-80.0, 0.0, -30.0, Linear)
        }
        ParameterId::ModulationRate => (0.1, 100.0, 40.0, Logarithmic),
        ParameterId::ModulationDepth => (0.0, 100.0, 100.0, Linear),
        ParameterId::ModulationPulseRate => (0.5, 100.0, 40.0, Logarithmic),
        ParameterId::SourceDuty | ParameterId::ModulationDuty => (1.0, 99.0, 50.0, Linear),
        ParameterId::SourceRise
        | ParameterId::SourceFall
        | ParameterId::ModulationRise
        | ParameterId::ModulationFall => (0.1, 50.0, 2.0, Logarithmic),
        ParameterId::Duration | ParameterId::Fade => return None,
    };
    Some(Range {
        min,
        max,
        default,
        scale,
    })
}

pub use crate::document::construction::{default_component, default_modulation, default_pair};

/// Map a normalized slider position to a displayed value within the span;
/// nonfinite positions give the default. `None` without a slider.
#[must_use]
pub fn slider_value(id: ParameterId, rate: SampleRate, position: f64) -> Option<f64> {
    let r = range(id, rate)?;
    if !position.is_finite() {
        return Some(r.default);
    }
    let value = match r.scale {
        Scale::Linear => r.min + (r.max - r.min) * position.clamp(0.0, 1.0),
        Scale::Logarithmic => r.min * (r.max / r.min).powf(position.clamp(0.0, 1.0)),
    };
    Some(quantize(id, value).clamp(r.min, r.max))
}

/// Deliberate slider gestures: 3 significant digits for logarithmic quantities,
/// 0.1 dB for level, and whole percentage points for depth. Typed entry
/// never uses this.
#[must_use]
pub fn quantize(id: ParameterId, value: f64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let rounded = match id {
        ParameterId::ModulationDepth => value.round(),
        id if id.descriptor().unit == auditorylab_core::Unit::DecibelsFullScale => {
            format!("{value:.1}").parse().unwrap_or(value)
        }
        _ => format!("{value:.2e}").parse().unwrap_or(value),
    };
    if rounded.is_finite() { rounded } else { value }
}

/// Map a model value to a clamped slider position. A nonfinite value leaves
/// the knob at zero without changing the exact field. `None` without a slider.
#[must_use]
pub fn slider_position(id: ParameterId, rate: SampleRate, model: f64) -> Option<f64> {
    let r = range(id, rate)?;
    let value = id.descriptor().unit.to_display(model);
    if !value.is_finite() {
        return Some(0.0);
    }
    Some(
        match r.scale {
            Scale::Linear => (value - r.min) / (r.max - r.min),
            Scale::Logarithmic => (value.clamp(r.min, r.max) / r.min).ln() / (r.max / r.min).ln(),
        }
        .clamp(0.0, 1.0),
    )
}

/// Offered modulation states, including the unmodulated state as a real choice.
#[must_use]
pub fn modulation_choices() -> Vec<Option<ModulationKind>> {
    std::iter::once(None)
        .chain(ModulationKind::ALL.into_iter().map(Some))
        .collect()
}

#[cfg(test)]
const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auditorylab_core::CarrierKind;
    use auditorylab_core::ComponentForm;
    use auditorylab_core::ComponentId;

    fn valid(component: ComponentForm, rate: SampleRate) -> bool {
        auditorylab_core::test_support::FiniteFixture::new(
            finite(1.0),
            finite(0.0),
            ([(ComponentId::new(1).unwrap(), component)])
                .iter()
                .map(|(id, voice)| auditorylab_core::Component {
                    id: *id,
                    form: *voice,
                })
                .collect(),
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        .check(rate)
        .target()
        .is_some()
    }

    #[test]
    fn slider_gestures_have_bounded_quantization_and_preserve_positive_endpoints() {
        for hz in [1, 8000, 44100, 48000, 768_000] {
            let rate = SampleRate::new(hz).unwrap();
            for id in ParameterId::COMPONENT {
                let r = range(id, rate).unwrap();
                let mut last = r.min;
                for i in 0..=1000 {
                    let position = f64::from(i) / 1000.0;
                    let raw = match r.scale {
                        Scale::Linear => r.min + (r.max - r.min) * position,
                        Scale::Logarithmic => r.min * (r.max / r.min).powf(position),
                    };
                    let value = slider_value(id, rate, position).unwrap();
                    let tolerance = match id {
                        ParameterId::PeakLevel
                        | ParameterId::PairFirstLevel
                        | ParameterId::PairSecondLevel => 0.050_000_1,
                        ParameterId::ModulationDepth => 0.500_001,
                        _ => raw.abs() * 0.005 + 1e-12,
                    };
                    assert!(
                        (raw - value).abs() <= tolerance,
                        "{hz} {id:?} {raw} -> {value}"
                    );
                    assert!((r.min..=r.max).contains(&value) && value >= last);
                    let model = id.descriptor().unit.to_model(value);
                    assert!((0.0..=1.0).contains(&slider_position(id, rate, model).unwrap()));
                    last = value;
                }
            }
        }
        assert_eq!(
            quantize(ParameterId::SourcePulseRate, 117.769_070_344_013_4),
            118.0
        );
        assert_eq!(quantize(ParameterId::SourcePulseRate, 39.0625), 39.1);
        assert_eq!(quantize(ParameterId::ModulationRate, 0.123_456), 0.123);
        assert_eq!(quantize(ParameterId::PeakLevel, -30.123), -30.1);
        assert_eq!(quantize(ParameterId::ModulationDepth, 39.6), 40.0);
        assert_eq!(range(ParameterId::Duration, SampleRate::DEFAULT), None);
    }

    #[test]
    fn independent_boundary_oracles_for_spans_and_positions() {
        let rate = SampleRate::DEFAULT;
        assert_eq!(slider_value(ParameterId::PeakLevel, rate, 0.5), Some(-40.0));
        assert_eq!(
            slider_value(ParameterId::ModulationDepth, rate, 0.5),
            Some(50.0)
        );
        assert_eq!(
            slider_value(ParameterId::ToneFrequency, rate, 0.5),
            Some(632.0)
        );
        assert_eq!(
            slider_position(ParameterId::ToneFrequency, rate, f64::NAN),
            Some(0.0)
        );
        // A typed value beyond the comfortable span leaves the knob at its end.
        assert_eq!(
            slider_position(ParameterId::ToneFrequency, rate, 10.0),
            Some(0.0)
        );
        assert_eq!(
            slider_position(ParameterId::SourcePulseDuration, rate, 0.05),
            Some(1.0)
        );
    }

    fn parameter_fixture(id: ParameterId) -> ComponentForm {
        let mut component = default_component(CarrierKind::Tone);
        if matches!(
            id,
            ParameterId::SourcePulseRate | ParameterId::SourcePulseDuration
        ) {
            component = default_component(CarrierKind::PulseTrain);
        }
        if matches!(
            id,
            ParameterId::ModulationRate | ParameterId::ModulationDepth
        ) {
            component.set_modulation(Some(default_modulation(ModulationKind::Sinusoidal)));
        }
        if matches!(
            id,
            ParameterId::ModulationPulseRate | ParameterId::ModulationPulseDuration
        ) {
            component.set_modulation(Some(default_modulation(ModulationKind::Pulsed)));
        }
        if matches!(
            id,
            ParameterId::SourceDuty | ParameterId::SourceRise | ParameterId::SourceFall
        ) {
            component = crate::document::construction::CreationKind::PulseTrain.component();
            if let crate::core::SoundSource::PulseTrain { pulse, .. } = &mut component.source {
                pulse.width = crate::core::PulseWidth::DutyCycle {
                    fraction: finite(0.5),
                };
                pulse.shape = crate::core::PulseShape::RaisedCosine {
                    rise_seconds: finite(0.002),
                    fall_seconds: finite(0.002),
                };
            }
        }
        if matches!(
            id,
            ParameterId::ModulationDuty | ParameterId::ModulationRise | ParameterId::ModulationFall
        ) {
            component = crate::document::construction::CreationKind::PulsedTone.component();
        }
        component
    }

    #[test]
    fn defaults_are_valid_at_audio_rates_and_frequency_spans_remain_positive() {
        for hz in [44_100, 48_000, 96_000] {
            let rate = SampleRate::new(hz).unwrap();
            for kind in CarrierKind::ALL {
                for modulation in ModulationKind::ALL {
                    let mut component = default_component(kind);
                    assert!(valid(component, rate));
                    component.set_modulation(Some(default_modulation(modulation)));
                    assert!(valid(component, rate), "{kind:?} {modulation:?}");
                }
            }
            for id in ParameterId::COMPONENT {
                let r = range(id, rate).unwrap();
                assert!(r.min < r.max && r.default >= r.min && r.default <= r.max);
                if r.scale == Scale::Logarithmic {
                    assert!(r.min > 0.0);
                }
                if matches!(
                    id,
                    ParameterId::PairFirstFrequency
                        | ParameterId::PairOffset
                        | ParameterId::PairFirstLevel
                        | ParameterId::PairSecondLevel
                ) {
                    for kind in [
                        auditorylab_core::PairKind::Binaural,
                        auditorylab_core::PairKind::Monaural,
                    ] {
                        let mut form = default_pair(kind);
                        assert!(form.set(id, finite(id.descriptor().unit.to_model(r.default))));
                        let authored = [auditorylab_core::Component {
                            id: ComponentId::new(1).unwrap(),
                            form,
                        }];
                        assert!(
                            auditorylab_core::test_support::FiniteFixture::new(
                                finite(1.0),
                                finite(0.0),
                                authored.to_vec()
                            )
                            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
                            .check(rate)
                            .target()
                            .is_some()
                        );
                    }
                    continue;
                }
                let mut component = parameter_fixture(id);
                assert!(component.set(id, finite(id.descriptor().unit.to_model(r.default))));
                assert!(valid(component, rate), "{id:?} default at {hz}");
            }
        }
        for hz in [1, 44, 45, 977, 978, 22_050] {
            let rate = SampleRate::new(hz).unwrap();
            let r = range(ParameterId::ToneFrequency, rate).unwrap();
            assert!(r.min > 0.0 && r.min < r.max && r.max < 0.5 * f64::from(hz));
            let component = {
                let base = default_component(CarrierKind::Tone);
                ComponentForm {
                    source: auditorylab_core::SoundSource::Tone {
                        frequency_hz: finite(r.default),
                        route: base.route().unwrap(),
                        peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
                    },
                    modulation: base.modulation(),
                }
            };
            assert!(valid(component, rate));
        }
        assert!(EXPORT_RATES.contains(&SampleRate::DEFAULT));
    }
}
