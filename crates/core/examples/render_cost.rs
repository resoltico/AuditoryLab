//! Offline rendering cost at 48 kHz: `cargo run -p auditorylab-core --example render_cost
//! --release`.
//! Each case renders twenty seconds. No audio device is opened.
use auditorylab_core::{
    Component, ComponentForm, ComponentId, Ears, Modulation, SampleRate, Sound, live::Engine,
};
use std::{error::Error, hint::black_box, time::Instant};
const RATE: u32 = 48_000;
const FRAMES: u32 = RATE * 20;
fn measure(components: Vec<ComponentForm>) -> Result<[f64; 3], Box<dyn Error>> {
    let rate = SampleRate::new(RATE)?;
    let components = components
        .into_iter()
        .enumerate()
        .map(|(index, form)| {
            Ok(Component {
                id: ComponentId::numbered(index).ok_or("too many component identities")?,
                form,
            })
        })
        .collect::<Result<Vec<_>, &str>>()?;
    let sound = Sound::new(
        finite(f64::from(FRAMES) / f64::from(RATE)),
        finite(0.0),
        components,
    )?;
    let signal = sound.signal(rate)?;
    let offset = u32::try_from(Sound::MAX_COMPONENTS)?;
    let components = sound
        .components()
        .iter()
        .copied()
        .map(|mut component| {
            component.id = component
                .id
                .get()
                .checked_add(offset)
                .and_then(ComponentId::new)
                .ok_or("component identity exhausted")?;
            Ok(component)
        })
        .collect::<Result<Vec<_>, &str>>()?;
    let alternate = Sound::new(sound.duration_seconds(), sound.fade_seconds(), components)?;
    let targets = [sound.target(rate)?, alternate.target(rate)?];
    let mut engine = Engine::new(rate);
    let batch = usize::try_from(engine.ramp_frames())?;
    let mut buffer = vec![[0.0; 2]; batch];
    let started = Instant::now();
    let mut frame = 0;
    while frame < FRAMES {
        let n = batch.min(usize::try_from(FRAMES - frame)?);
        assert_eq!(signal.render(u64::from(frame), &mut buffer[..n]), n);
        black_box(&buffer[..n]);
        frame += u32::try_from(n)?;
    }
    let signal_cost = started.elapsed().as_secs_f64() * 1e9 / f64::from(FRAMES);
    let mut costs = [signal_cost, 0.0, 0.0];
    for (index, crossfade) in [false, true].into_iter().enumerate() {
        engine = Engine::new(rate);
        assert!(engine.set(&targets[0]));
        assert_eq!(engine.render(&mut buffer), batch);
        let mut frame = 0;
        let mut target = 1;
        let started = Instant::now();
        while frame < FRAMES {
            if crossfade {
                // Fresh identities force actual crossfades, with up to 32 active AM voices.
                assert!(engine.set(&targets[target]));
                target ^= 1;
            }
            let n = batch.min(usize::try_from(FRAMES - frame)?);
            assert_eq!(engine.render(&mut buffer[..n]), n);
            black_box(&buffer[..n]);
            frame += u32::try_from(n)?;
        }
        costs[index + 1] = started.elapsed().as_secs_f64() * 1e9 / f64::from(FRAMES);
    }
    Ok(costs)
}
fn main() -> Result<(), Box<dyn Error>> {
    let tones = [Ears::Left, Ears::Right]
        .into_iter()
        .zip([250.0, 256.0])
        .map(|(ears, frequency_hz)| ComponentForm::Tone {
            frequency_hz: finite(frequency_hz),
            ears,
            peak_dbfs: finite(-20.0),
            modulation: None,
        })
        .collect();
    let am = (0..16)
        .map(|i| ComponentForm::Tone {
            frequency_hz: finite(200.0 + 50.0 * f64::from(i)),
            ears: Ears::Both,
            peak_dbfs: finite(-30.0),
            modulation: Some(Modulation::Am {
                rate_hz: finite(40.0),
                depth: finite(1.0),
            }),
        })
        .collect();
    println!("48 kHz, twenty seconds rendered per case, ns/frame");
    println!("case,Signal,live steady,live crossfading");
    for (name, components) in [("2 tones", tones), ("16 AM voices", am)] {
        let [signal, steady, crossfading] = measure(components)?;
        println!("{name},{signal:.2},{steady:.2},{crossfading:.2}");
    }
    Ok(())
}

const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}
