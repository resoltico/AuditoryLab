//! Public-library prepare/control/observe/stop without an audio device.
use auditorylab::{
    Carrier, Component, Draft, Ears, LiveGenerator, SampleRate, SourceKind, interaural_pair,
};
#[test]
fn generated_quantities_follow_valid_revisions_and_metadata_does_not_retarget() {
    let mut draft = Draft::new();
    let left = draft.add(SourceKind::Tone).unwrap();
    let right = draft.add(SourceKind::Tone).unwrap();
    let component = |frequency_hz, ears| Component {
        ears,
        peak_dbfs: -30.0,
        carrier: Carrier::Tone { frequency_hz },
        modulation: None,
    };
    draft.replace(left, component(380.0, Ears::Left));
    draft.replace(right, component(420.0, Ears::Right));
    let mut generator = LiveGenerator::new(&draft, SampleRate::DEFAULT).unwrap();
    let control = generator.control();
    assert_eq!(control.update(&draft).unwrap(), 1);
    let mut out = [[0.0; 2]; 2000];
    generator.render(&mut out);
    let first = generator.observation().unwrap();
    assert_eq!(interaural_pair(&first).unwrap().difference_hz, 40.0);
    draft.set_title("An edited research copy".into());
    draft.set_summary("My notes".into());
    assert_eq!(control.update(&draft).unwrap(), 1);
    draft.replace(right, component(428.0, Ears::Right));
    assert_eq!(control.update(&draft).unwrap(), 2);
    generator.render(&mut [[0.0; 2]; 720]);
    let mid = interaural_pair(&generator.observation().unwrap()).unwrap();
    assert!(mid.difference_hz > 40.0 && mid.difference_hz < 48.0);
    draft.replace(right, component(439.0, Ears::Right));
    assert_eq!(control.update(&draft).unwrap(), 3);
    draft.replace(right, component(448.0, Ears::Right));
    assert_eq!(control.update(&draft).unwrap(), 4);
    generator.render(&mut [[0.0; 2]; 4000]);
    let settled = generator.observation().unwrap();
    assert_eq!(settled.active_revision, 4);
    assert_eq!(settled.settled_revision, 4);
    assert_eq!(interaural_pair(&settled).unwrap().difference_hz, 68.0);
    draft.replace(right, component(f64::NAN, Ears::Right));
    assert!(control.update(&draft).is_err());
    assert_eq!(generator.observation().unwrap(), settled);
    generator.stop();
    generator.render(&mut out);
    assert!(generator.is_finished());
    assert!(out[1440..].iter().all(|f| *f == [0.0; 2]));
}
#[test]
fn generated_interaural_structure_ignores_titles_and_rejects_other_mixtures() {
    for (text, expected) in [
        ("", false),
        (
            "[[component]]\nears='both'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=380\n[[component]]\nears='both'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=420\n",
            false,
        ),
        (
            "[[component]]\nears='left'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=380\n[[component]]\nears='right'\npeak_dbfs=-30\nkind='tone'\nfrequency_hz=420\n",
            true,
        ),
    ] {
        let preset = auditorylab::load_reader(
            format!("schema_version=2\nduration_seconds=1\nfade_seconds=0\n{text}").as_bytes(),
            "called binaural even if not a pair",
        )
        .unwrap();
        let sound = preset.at(SampleRate::DEFAULT).unwrap();
        assert_eq!(
            interaural_pair(&sound.signal().observation(0).unwrap()).is_some(),
            expected
        );
    }
}

#[test]
fn editor_units_and_ranges_have_independent_boundary_oracles() {
    use auditorylab::{
        ModulationKind, Setting, default_component, default_modulation, set_setting, setting_value,
        slider_position, slider_value,
    };
    let rate = SampleRate::DEFAULT;
    assert_eq!(slider_value(Setting::Level, rate, 0.5), -40.0);
    assert_eq!(slider_value(Setting::Depth, rate, 0.5), 50.0);
    assert!(
        (slider_value(Setting::Frequency, rate, 0.5) - (20.0_f64 * 20000.0).sqrt()).abs() < 1e-10
    );
    assert_eq!(slider_position(Setting::Frequency, rate, f64::NAN), 0.0);
    let mut c = default_component(SourceKind::Tone);
    c.modulation = Some(default_modulation(ModulationKind::Sinusoidal, rate));
    assert_eq!(setting_value(&c, Setting::Depth), Some(100.0));
    set_setting(&mut c, Setting::Depth, 25.0);
    assert!(matches!(
        c.modulation,
        Some(auditorylab::Modulation::Sinusoidal { depth: 0.25, .. })
    ));
    assert_eq!(setting_value(&c, Setting::Depth), Some(25.0));
    let before = c;
    set_setting(&mut c, Setting::ClickWidth, 10.0);
    assert_eq!(c, before);
}

#[test]
fn generated_routing_text_reports_intermediate_gains_not_the_requested_endpoint() {
    use auditorylab::{Observation, SlotRole, VoiceObservation, generated_description};
    let facts = Component {
        ears: Ears::Right,
        peak_dbfs: -20.0,
        carrier: Carrier::Tone {
            frequency_hz: 440.0,
        },
        modulation: None,
    }
    .facts(SampleRate::DEFAULT)
    .unwrap();
    let mut voices = [None; 32];
    voices[0] = Some(VoiceObservation {
        id: 1,
        role: SlotRole::Continuing,
        facts,
        gains: [0.05, 0.05],
    });
    let text = generated_description(Some(&Observation {
        frame: 100,
        active_revision: 2,
        settled_revision: 1,
        transitioning: true,
        voices,
    }));
    assert!(text.contains("Both ears"), "{text}");
    assert!(text.contains("gain L -26.0 / R -26.0 dBFS"), "{text}");
    assert!(text.starts_with("Transitioning"));
    assert_eq!(auditorylab::format_hz(39.0625), "39.0625");
    assert_ne!(auditorylab::format_hz(1e-20), "0");
}
