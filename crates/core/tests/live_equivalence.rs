//! Steady live output and seekable finite output share the exact sample contract.
use auditorylab_core::{
    Component, ComponentForm, ComponentId, Ears, Recipe, SampleRate, Sound, catalog, live::Engine,
};

#[test]
fn every_builtin_matches_bit_for_bit_after_both_startup_envelopes() {
    assert!(
        !catalog::ENTRIES.is_empty(),
        "catalog discovery must not be empty"
    );
    for hz in [44_100_u32, 48_000] {
        let rate = SampleRate::new(hz).unwrap();
        for entry in catalog::ENTRIES {
            let recipe = Recipe::parse(entry.text).unwrap();
            let signal = recipe.sound().signal(rate).unwrap();
            let mut engine = Engine::new(rate);
            assert!(engine.set(&recipe.sound().target(rate).unwrap()));
            let start = signal.fade_frames().max(engine.ramp_frames()) + 1;
            let end = start + 3 * u64::from(hz);
            assert!(end < signal.frames() - signal.fade_frames());
            let mut frames = vec![[0.0; 2]; usize::try_from(end).unwrap()];
            assert_eq!(engine.render(&mut frames), frames.len());
            for n in start..end {
                assert_eq!(
                    frames[usize::try_from(n).unwrap()].map(f64::to_bits),
                    signal.frame(n).map(f64::to_bits),
                    "{} at {hz} Hz frame {n}",
                    entry.id
                );
            }
        }
    }
}

#[test]
fn a_different_carrier_is_detected_by_the_same_bit_comparison() {
    let rate = SampleRate::DEFAULT;
    let sound = |frequency_hz| Sound {
        duration_seconds: 1.0,
        fade_seconds: 0.0,
        components: vec![Component {
            id: ComponentId::new(1).unwrap(),
            form: ComponentForm::Tone {
                frequency_hz,
                ears: Ears::Both,
                peak_dbfs: -20.0,
                modulation: None,
            },
        }],
    };
    let signal = sound(200.0).signal(rate).unwrap();
    let mut engine = Engine::new(rate);
    assert!(engine.set(&sound(201.0).target(rate).unwrap()));
    let start = engine.ramp_frames() + 1;
    let end = start + 480;
    let mut frames = vec![[0.0; 2]; usize::try_from(end).unwrap()];
    engine.render(&mut frames);
    assert!(
        (start..end).any(|n| frames[usize::try_from(n).unwrap()].map(f64::to_bits)
            != signal.frame(n).map(f64::to_bits))
    );
}
