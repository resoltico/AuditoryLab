use super::*;
#[expect(
    clippy::cast_possible_truncation,
    reason = "test signals intentionally quantized to decoder f32 samples"
)]
fn measure(signal: impl Fn(f64) -> [f64; 2]) -> Analysis {
    let rate = 8000;
    let mut measurement = Measurement::new(rate, 2, u64::from(rate) * 10).unwrap();
    for frame in 0..rate * 10 {
        let samples = signal(f64::from(frame) / f64::from(rate));
        measurement.push(&samples.map(|x| x as f32)).unwrap();
    }
    measurement.finish(&Cancel::default()).unwrap()
}
fn sine(hz: f64, t: f64) -> f64 {
    (std::f64::consts::TAU * hz * t).sin() * 0.1
}
fn has_rate(a: &Analysis, hz: f64, modulation: bool) -> bool {
    a.pulsing.iter().any(|p| {
        p.envelope_band == EnvelopeBand::Whole
            && (p.hz - hz).abs() < a.envelope_resolution_hz
            && matches!(p.cause, PulsingCause::Modulation) == modulation
    })
}
#[test]
fn binaural_and_monaural_pairs_have_distinct_evidence() {
    let binaural = measure(|t| [sine(200.0, t), sine(210.0, t)]);
    assert_eq!(binaural.ear_differences.len(), 1, "{binaural:#?}");
    assert!((binaural.ear_differences[0].difference_hz - 10.0).abs() < binaural.resolution_hz);
    assert_eq!(binaural.ear_differences[0].band, Some(Band::Alpha));
    assert!(!binaural.coincident_tones);
    assert!(!has_rate(&binaural, 10.0, false));
    let monaural = measure(|t| [sine(200.0, t) + sine(210.0, t); 2]);
    assert_eq!(monaural.ear_differences, Vec::new());
    assert!(monaural.coincident_tones);
    assert!(has_rate(&monaural, 10.0, false), "{:?}", monaural.pulsing);
}
#[test]
fn amplitude_modulation_is_not_a_carrier_or_two_tone_beating() {
    let am =
        measure(|t| [sine(1000.0, t) * (1.0 + 0.8 * (std::f64::consts::TAU * 40.0 * t).cos()); 2]);
    assert!(has_rate(&am, 40.0, true), "{:?}", am.pulsing);
    assert!(!has_rate(&am, 40.0, false));
    let plain = measure(|t| [sine(1000.0, t); 2]);
    assert!(
        plain
            .pulsing
            .iter()
            .find(|p| p.envelope_band == EnvelopeBand::Whole)
            .is_none(),
        "{:?}",
        plain.pulsing
    );
}
#[test]
fn low_carrier_is_not_its_rectification_rhythm_and_levels_are_calibrated() {
    let plain = measure(|t| [sine(40.0, t); 2]);
    assert!((plain.tones[0].hz - 40.0).abs() < plain.resolution_hz);
    assert!(
        (plain.tones[0].dbfs + 23.0103).abs() < 0.5,
        "{:?}",
        plain.tones
    );
    assert!(plain.low_energy[0].fraction.unwrap() >= 0.99);
    assert!(!has_rate(&plain, 80.0, true));
    assert!(
        !plain
            .pulsing
            .iter()
            .any(|p| p.envelope_band == EnvelopeBand::Whole),
        "{:?}",
        plain.pulsing
    );
    assert!(!has_rate(&plain, 80.0, false));
    assert!((plain.stereo.correlation.unwrap() - 1.0).abs() < 1e-8);
}
#[test]
fn silence_cancellation_and_invalid_frames_are_explicit() {
    let silence = measure(|_| [0.0; 2]);
    assert_eq!(silence.tones, Vec::new());
    assert_eq!(silence.pulsing, Vec::new());
    assert!(!silence.coincident_tones);
    assert_eq!(silence.low_energy[0].fraction, None);
    assert_eq!(silence.stereo.correlation, None);
    let mut m = Measurement::new(1000, 1, 2000).unwrap();
    assert!(m.push(&[f32::NAN]).is_err());
    assert!(m.push(&[0.0, 0.0]).is_err());
    for _ in 0..2000 {
        m.push(&[0.0]).unwrap();
    }
    let cancel = Cancel::default();
    cancel.request();
    assert!(matches!(m.finish(&cancel), Err(RecordingError::Cancelled)));
}

#[test]
fn low_tone_pair_sums_and_beat_harmonics_are_not_independent_modulation() {
    let a = measure(|t| [sine(40.0, t) + sine(43.0, t); 2]);
    assert!(has_rate(&a, 3.0, false));
    assert!(
        !a.pulsing
            .iter()
            .any(|p| (p.hz - 83.0).abs() < 2.0 * a.resolution_hz)
    );
    assert!(
        !a.pulsing
            .iter()
            .any(|p| p.envelope_band == EnvelopeBand::Whole && p.cause == PulsingCause::Modulation),
        "{:?}",
        a.pulsing
    );
}

#[test]
fn tiny_envelope_variations_do_not_cross_the_reported_contrast_floor() {
    let a = measure(|t| {
        [sine(1000.0, t) * (1.0 + 0.0001 * (std::f64::consts::TAU * 40.0 * t).cos()); 2]
    });
    assert!(!has_rate(&a, 40.0, true));
    assert_eq!(a.minimum_envelope_relative_power, 1e-6);
}

#[test]
#[expect(
    clippy::cast_possible_truncation,
    reason = "test oracle quantized to decoder f32 samples"
)]
fn mono_and_fractional_millisecond_rates_preserve_the_modulation_rate() {
    for rate in [44_100, 48_000, 768_000] {
        let frames = u64::from(rate) * 3;
        let mut measurement = Measurement::new(rate, 1, frames).unwrap();
        for frame in 0..rate * 3 {
            let t = f64::from(frame) / f64::from(rate);
            let sample = sine(1000.0, t) * (1.0 + 0.8 * (std::f64::consts::TAU * 40.0 * t).cos());
            measurement.push(&[sample as f32]).unwrap();
        }
        let a = measurement.finish(&Cancel::default()).unwrap();
        assert!(has_rate(&a, 40.0, true), "{rate}: {:?}", a.pulsing);
        assert_eq!(a.stereo.correlation, None);
        assert_eq!(a.ear_differences, Vec::new());
        assert!(a.envelope_window.start_seconds > a.window.start_seconds);
        assert!(a.envelope_window.end_seconds < a.window.end_seconds);
        let mut plain = Measurement::new(rate, 1, frames).unwrap();
        for frame in 0..rate * 3 {
            plain
                .push(&[sine(40.0, f64::from(frame) / f64::from(rate)) as f32])
                .unwrap();
        }
        let plain = plain.finish(&Cancel::default()).unwrap();
        assert!(
            !plain
                .pulsing
                .iter()
                .any(|p| p.envelope_band == EnvelopeBand::Whole),
            "{rate}: a plain low carrier is not modulation: {:?}",
            plain.pulsing
        );
    }
}

#[test]
fn strong_modulation_is_not_attributed_to_an_unrelated_weak_beat_harmonic() {
    // The modulated carrier lies above the tone table's 2 kHz ceiling. Its
    // 30 Hz envelope must not become a supposed third harmonic of the weak 10 Hz pair.
    let a = measure(|t| {
        [sine(3000.0, t) * (1.0 + 0.8 * (std::f64::consts::TAU * 30.0 * t).cos())
            + 0.1 * (sine(200.0, t) + sine(210.0, t)); 2]
    });
    assert!(
        has_rate(&a, 10.0, false),
        "the weak beat is observed before harmonic attribution"
    );
    assert!(
        has_rate(&a, 30.0, true),
        "strong AM must retain its modulation attribution"
    );
}

#[test]
fn dc_offsets_are_level_energy_but_do_not_modulate_a_steady_tone() {
    let a = measure(|t| [0.1 + sine(40.0, t); 2]);
    assert!(
        !a.pulsing
            .iter()
            .any(|p| p.envelope_band == EnvelopeBand::Whole),
        "a DC offset does not create amplitude modulation"
    );
    assert!(a.low_energy[0].fraction.unwrap() > 0.99);
    assert!((a.stereo.rms_dbfs[0].unwrap() - 10.0 * 0.015_f64.log10()).abs() < 0.01);
}
#[test]
fn beating_attribution_respects_the_measured_band() {
    let a = measure(|t| {
        [sine(3000.0, t) * (1.0 + 0.8 * (std::f64::consts::TAU * 3.0 * t).cos())
            + 0.4 * (sine(40.0, t) + sine(43.0, t)); 2]
    });
    assert!(
        a.pulsing
            .iter()
            .any(|p| p.envelope_band == EnvelopeBand::Above100
                && (p.hz - 3.0).abs() < a.envelope_resolution_hz
                && p.cause == PulsingCause::Modulation),
        "low tones cannot explain the high-band envelope"
    );
    assert!(
        a.pulsing
            .iter()
            .any(|p| p.envelope_band == EnvelopeBand::Below100
                && (p.hz - 3.0).abs() < a.envelope_resolution_hz
                && matches!(p.cause, PulsingCause::Beating { harmonic: 1, .. }))
    );
}
