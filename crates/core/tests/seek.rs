//! Seekability, bounds, fades and trace extrema against independent oracles.
use auditorylab_core::test_support as common;
use auditorylab_core::{ComponentForm, Ears, Modulation, Recipe, SampleRate, catalog, waveform};
use proptest::prelude::*;
proptest! {
    #![proptest_config(ProptestConfig { cases: 64, max_shrink_iters: 128, failure_persistence: None,
        .. ProptestConfig::default() })]
    #[test]
    fn arbitrary_windows_equal_a_full_render(start in 0_usize..5000, len in 0_usize..1000,
        entry in 0_usize..catalog::ENTRIES.len()) {
        let mut sound = Recipe::parse(&catalog::ENTRIES[entry].text()).unwrap().sound().clone();
        sound.set_duration(finite(0.1)); sound.set_fade(finite(0.01));
        let signal = sound.signal(SampleRate::DEFAULT).unwrap();
        let mut all = vec![[0.0; 2]; 6000]; signal.render(0, &mut all);
        let mut window = vec![[9.0; 2]; len]; signal.render(start as u64, &mut window);
        prop_assert_eq!(&window, &all[start..start+len]);
        prop_assert!(all.iter().flatten().all(|v| v.abs() <= 1.0));
    }
}
#[test]
fn trace_equals_naive_extrema_and_retains_one_frame_pulses() {
    let signal = common::recipe(
        1.25,
        0.0,
        &[ComponentForm::Clicks {
            rate_hz: finite(3.0),
            width_seconds: finite(0.000_125),
            ears: Ears::Left,
            peak_dbfs: finite(-20.0),
            modulation: None,
        }],
    )
    .sound()
    .clone()
    .signal(SampleRate::new(8000).unwrap())
    .unwrap();
    let mut out = Vec::new();
    for (start, frames, bins, expected_bins) in [
        (0, 10000, 3, 3),
        (123, 4567, 1000, 1000),
        (9990, 50, 3, 3),
        (9998, 50, 8, 2),
        (123, 4567, 0, 0),
        (123, 0, 3, 0),
        (10000, 30, 3, 0),
        (10001, 30, 3, 0),
        (u64::MAX, 30, 3, 0),
    ] {
        // Reuse nonempty storage so empty windows must clear an earlier trace.
        out.push([waveform::Extent { min: 9.0, max: 9.0 }; 2]);
        waveform::trace(&signal, start, frames, bins, &mut out);
        assert_eq!(out.len(), expected_bins, "window {start}/{frames}/{bins}");
        let span = start
            .saturating_add(frames)
            .min(signal.frames())
            .saturating_sub(start);
        for (bin, extents) in out.iter().enumerate() {
            let lo = start + (bin as u64 * span).div_ceil(out.len() as u64);
            let hi = start + ((bin as u64 + 1) * span).div_ceil(out.len() as u64);
            for (ear, extent) in extents.iter().enumerate() {
                let values = (lo..hi).map(|n| signal.frame(n)[ear]);
                assert_eq!(extent.min, values.clone().fold(f64::INFINITY, f64::min));
                assert_eq!(extent.max, values.fold(f64::NEG_INFINITY, f64::max));
            }
        }
    }
    waveform::trace(&signal, 0, 10000, 3, &mut out);
    assert_eq!(out.len(), 3);
    assert!(out.iter().all(|bin| bin[0].max == 0.1));
}
#[test]
fn fades_and_late_tones_follow_independent_sine_formulas() {
    let mut sound = Recipe::parse(
        &catalog::find("ioannou-2015-pure-tone-200-hz")
            .unwrap()
            .text(),
    )
    .unwrap()
    .sound()
    .clone();
    sound.set_duration(finite(68_719_476_736.0 / 48_000.0));
    let signal = sound.signal(SampleRate::DEFAULT).unwrap();
    for n in [0, 1, 4799, 4800, 4801, (1 << 36) - 4801, (1 << 36) - 1] {
        let depth = n.min(signal.frames() - 1 - n);
        let gain = if depth >= signal.fade_frames() {
            1.0
        } else {
            (std::f64::consts::FRAC_PI_2 * f64::from(u32::try_from(depth).unwrap()) / 4800.0)
                .sin()
                .powi(2)
        };
        let cycles = f64::from(u32::try_from(n % 240).unwrap()) / 240.0;
        let expected = 0.1 * (std::f64::consts::TAU * cycles).sin() * gain;
        assert!((signal.frame(n)[0] - expected).abs() < 3e-9);
    }
}
#[test]
fn sinusoidal_modulation_at_early_and_late_frames_matches_rational_cycles() {
    let signal = common::recipe(
        1_431_655.765_333_333_3,
        0.0,
        &[ComponentForm::Tone {
            frequency_hz: finite(8000.0),
            ears: Ears::Both,
            peak_dbfs: finite(-20.0),
            modulation: Some(Modulation::Am {
                rate_hz: finite(39.0625),
                depth: finite(0.7),
            }),
        }],
    )
    .sound()
    .clone()
    .signal(SampleRate::DEFAULT)
    .unwrap();
    for n in [0_u64, 1, 23456, 172_800_000, (1 << 36) - 100] {
        let carrier = f64::from(u32::try_from(u128::from(n) * 8000 % 48000).unwrap()) / 48000.0;
        let modulation = f64::from(u32::try_from(u128::from(n) * 625 % (48000 * 16)).unwrap())
            / (48000.0 * 16.0);
        let expected = 0.1
            * (std::f64::consts::TAU * carrier).sin()
            * (1.0 + 0.7 * (std::f64::consts::TAU * modulation).sin())
            / 1.7;
        let phase_bound =
            f64::from(u32::try_from(n >> 16).unwrap()) * 65536.0 / 18_446_744_073_709_551_616.0;
        assert!(
            (signal.frame(n)[0] - expected).abs()
                <= 0.2 * std::f64::consts::TAU * phase_bound + 1e-13
        );
    }
}
#[test]
fn raised_cosine_is_symmetric_and_matches_sine_squared() {
    let signal = common::recipe(
        0.125_125,
        0.008,
        &[ComponentForm::Clicks {
            rate_hz: finite(0.5),
            width_seconds: finite(0.25),
            ears: Ears::Both,
            peak_dbfs: finite(0.0),
            modulation: None,
        }],
    )
    .sound()
    .clone()
    .signal(SampleRate::new(8000).unwrap())
    .unwrap();
    assert_eq!(signal.frames(), 1001);
    assert_eq!(signal.fade_frames(), 64);
    for n in 0..1001_u32 {
        let depth = n.min(1000 - n);
        let expected = if depth >= 64 {
            1.0
        } else {
            (std::f64::consts::FRAC_PI_2 * f64::from(depth) / 64.0)
                .sin()
                .powi(2)
        };
        assert!((signal.frame(u64::from(n))[0] - expected).abs() < 4e-16);
        assert_eq!(
            signal.frame(u64::from(n)),
            signal.frame(u64::from(1000 - n))
        );
    }
    assert_eq!(signal.frame(u64::MAX), [0.0; 2]);
    let mut tail = [[1.0; 2]; 3];
    assert_eq!(signal.render(u64::MAX, &mut tail), 0);
    assert_eq!(tail, [[0.0; 2]; 3]);
}
#[test]
fn public_signals_preserve_slow_onsets_and_widths_longer_than_a_second() {
    let recipe = |rate: f64, width: f64| {
        common::recipe(
            3.0,
            0.0,
            &[ComponentForm::Clicks {
                rate_hz: finite(rate),
                width_seconds: finite(width),
                ears: Ears::Both,
                peak_dbfs: finite(-20.0),
                modulation: None,
            }],
        )
    };
    let signal = recipe(1.0_f64.next_down(), 0.001)
        .sound()
        .clone()
        .signal(SampleRate::DEFAULT)
        .unwrap();
    for onset in [48001, 96001] {
        assert_eq!(signal.frame(onset - 1), [0.0; 2]);
        assert_eq!(signal.frame(onset), [0.1; 2]);
        assert_eq!(signal.frame(onset + 47), [0.1; 2]);
        assert_eq!(signal.frame(onset + 48), [0.0; 2]);
    }
    let signal = recipe(0.1, 2.0)
        .sound()
        .clone()
        .signal(SampleRate::DEFAULT)
        .unwrap();
    assert_eq!(signal.frame(95999), [0.1; 2]);
    assert_eq!(signal.frame(96000), [0.0; 2]);
}

const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}
