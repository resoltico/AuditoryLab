//! Seekability, bounds, fades and trace extrema against independent oracles.
use auditorylab_core::{Preset, SampleRate, catalog, waveform};
use proptest::prelude::*;
proptest! {
    #![proptest_config(ProptestConfig { cases: 64, max_shrink_iters: 128, failure_persistence: None, .. ProptestConfig::default() })]
    #[test]
    fn arbitrary_windows_equal_a_full_render(start in 0_usize..5000, len in 0_usize..1000, entry in 0_usize..19) {
        let mut stimulus = Preset::parse(catalog::ENTRIES[entry].text).unwrap().stimulus;
        stimulus.duration_seconds = 0.1; stimulus.fade_seconds = 0.01;
        let signal = stimulus.signal(SampleRate::DEFAULT).unwrap();
        let mut all = vec![[0.0; 2]; 6000]; signal.render(0, &mut all);
        let mut window = vec![[9.0; 2]; len]; signal.render(start as u64, &mut window);
        prop_assert_eq!(&window, &all[start..start+len]);
        prop_assert!(all.iter().flatten().all(|v| v.abs() <= 1.0));
    }
}
#[test]
fn trace_equals_naive_extrema_and_retains_one_frame_pulses() {
    let text = "schema_version = 2\nduration_seconds = 1.25\nfade_seconds = 0.0\n[[component]]\nkind = 'click'\nears = 'left'\nrate_hz = 3.0\nwidth_seconds = 0.000125\npeak_dbfs = -20.0";
    let signal = Preset::parse(text)
        .unwrap()
        .stimulus
        .signal(SampleRate::new(8000).unwrap())
        .unwrap();
    let mut out = Vec::new();
    for (start, frames, bins) in [
        (0, 10000, 3),
        (123, 4567, 1000),
        (9990, 50, 3),
        (10000, 30, 0),
    ] {
        waveform::trace(&signal, start, frames, bins, &mut out);
        let span = (start + frames).min(signal.frames()) - start;
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
    assert!(out.iter().all(|bin| bin[0].max == 0.1));
}
#[test]
fn fades_and_late_tones_follow_independent_sine_formulas() {
    let mut stimulus = Preset::parse(catalog::find("ioannou-2015-pure-tone-200-hz").unwrap().text)
        .unwrap()
        .stimulus;
    stimulus.duration_seconds = 68_719_476_736.0 / 48_000.0;
    let signal = stimulus.signal(SampleRate::DEFAULT).unwrap();
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
