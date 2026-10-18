// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! A public facade consumer needs neither Slint nor a native audio device.
use auditorylab::{ObservationError, SampleRate, analyze_samples};

#[test]
fn public_analysis_preserves_one_frame_pulses_and_separate_ear_extrema() {
    let rate = SampleRate::new(1000).unwrap();
    let samples: Vec<_> = (0..100)
        .map(|i| [if i % 17 == 3 { 1.0 } else { 0.0 }, -0.25])
        .collect();
    let view = analyze_samples(rate, 123, &samples, 9).unwrap();
    assert_eq!(view.range().unwrap().start_frame(), 123);
    assert_eq!(view.range().unwrap().end_frame(), 223);
    assert!(view.gaps().is_empty());
    for bin in view.bins() {
        let first = usize::try_from(bin.range().start_frame() - 123).unwrap();
        let end = usize::try_from(bin.range().end_frame() - 123).unwrap();
        let expected_max = samples[first..end]
            .iter()
            .map(|x| x[0])
            .fold(f64::NEG_INFINITY, f64::max);
        let expected_min = samples[first..end]
            .iter()
            .map(|x| x[0])
            .fold(f64::INFINITY, f64::min);
        assert!(bin.complete());
        assert_eq!(bin.left().min(), expected_min);
        assert_eq!(bin.left().max(), expected_max);
        assert_eq!(bin.right().min(), -0.25);
        assert_eq!(bin.right().max(), -0.25);
    }
    assert!(view.bins().iter().any(|bin| bin.left().max() == 1.0));
    assert!(samples.iter().step_by(17).all(|sample| sample[0] == 0.0));
}

#[test]
fn public_analysis_rejects_invalid_inputs_and_reports_actual_low_rate_extent() {
    let rate = SampleRate::new(1).unwrap();
    let view = analyze_samples(rate, 42, &[[0.5, 0.0]], 1024).unwrap();
    assert_eq!(view.bins().len(), 1);
    assert_eq!(view.range().unwrap().start_frame(), 42);
    assert_eq!(view.range().unwrap().end_frame(), 43);
    assert!(matches!(
        analyze_samples(rate, 0, &[[0.0; 2]; 2], 1),
        Err(ObservationError::TooManySamples {
            limit: 1,
            observed: 2
        })
    ));
    assert!(matches!(
        analyze_samples(rate, u64::MAX, &[[0.0; 2]], 1),
        Err(ObservationError::InvalidSourceRange)
    ));
    assert!(matches!(
        analyze_samples(rate, 0, &[[f64::NAN, 0.0]], 1),
        Err(ObservationError::InvalidSamples)
    ));
    assert!(matches!(
        analyze_samples(rate, 0, &[[0.0, -1.01]], 1),
        Err(ObservationError::InvalidSamples)
    ));
    assert!(matches!(
        analyze_samples(rate, 0, &[], 0),
        Err(ObservationError::InvalidBins)
    ));
    assert!(analyze_samples(rate, 0, &[], 1).unwrap().bins().is_empty());
}
