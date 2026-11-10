//! Device-check and recording wording through the public app types.
#![cfg(test)]
use auditorylab_app::device_check::{ChannelResult, DeviceAnswer, DeviceCheck, DeviceTest};
use auditorylab_app::recording::{ContainerFormat, RecordingFile};
use auditorylab_presentation::{device_check, recording, report::Report};

#[test]
fn every_test_prompt_names_its_tone_route_and_question() {
    assert_eq!(
        device_check::test_prompt(DeviceTest::LeftChannel),
        "Plays a 440 Hz tone for 3 seconds at -20 dBFS on the left channel only. Where did you \
        hear it?"
    );
    assert_eq!(
        device_check::test_prompt(DeviceTest::Tone40Hz),
        "Plays a 40 Hz tone for 3 seconds at -20 dBFS on both channels. Did you hear it?"
    );
}

#[test]
fn results_say_what_the_output_means_for_binaural_pairs_and_low_tones() {
    let mut check = DeviceCheck::default();
    let text = |check: &DeviceCheck| device_check::assessment(check.assessment());
    assert!(text(&check).starts_with("Answer both channel tests"));
    check
        .record(DeviceTest::LeftChannel, DeviceAnswer::Both)
        .unwrap();
    check
        .record(DeviceTest::RightChannel, DeviceAnswer::Both)
        .unwrap();
    for test in [DeviceTest::Tone100Hz, DeviceTest::Tone60Hz] {
        check.record(test, DeviceAnswer::Heard).unwrap();
    }
    check
        .record(DeviceTest::Tone40Hz, DeviceAnswer::NotHeard)
        .unwrap();
    assert_eq!(
        text(&check),
        "Both ears hear both channels, as with a single speaker or a mono audio setting. \
        Binaural pairs cannot work on this output: each ear needs its own tone.\nThe lowest \
        tone you heard was 60 Hz. Sounds built on lower tones may be faint or missing on this \
        output. Raising the volume to hear them makes everything else much louder."
    );
    assert!(
        device_check::channel_result(ChannelResult::OneSide(DeviceTest::RightChannel))
            .starts_with("Only the right side made sound. Check the left earpiece")
    );
}

#[test]
fn recordings_render_identity_and_json_without_claiming_measurements() {
    let file = RecordingFile {
        path: "/music/take.flac".into(),
        bytes: 999,
        format: ContainerFormat::Flac,
    };
    assert_eq!(
        recording::recording_description(&file),
        "Recording: take.flac\nFile: /music/take.flac\nSize: 999 bytes\nFormat: FLAC\n\
        Measurements: Choose Analyze to measure this recording. No samples have been measured yet."
    );
    let json = serde_json::to_value(Report::recording(&file)).unwrap();
    assert_eq!(json["operation"], "analyze_recording");
    assert_eq!(
        json["recording"],
        serde_json::json!({"path": "/music/take.flac", "bytes": 999, "format": "flac"})
    );
    let bigger = RecordingFile {
        bytes: 4_398_933,
        ..file
    };
    assert_eq!(recording::recording_size(&bigger), "4.4 MB (4398933 bytes)");
}

#[test]
fn measured_findings_state_the_device_check_comparison_and_all_measurement_limits() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../app/tests/fixtures/recording/tone.flac");
    let measured = auditorylab_app::services::analyze_recording(
        &path,
        auditorylab_app::recording::AnalysisOptions::default(),
        &auditorylab_app::job::Cancel::default(),
        &auditorylab_app::job::Progress::default(),
    )
    .unwrap();
    let f = recording::findings(&measured.analysis, Some(60.0));
    assert!(
        f.low_tones
            .contains("lowest tone you reported hearing (60 Hz)")
    );
    assert!(f.facts.contains("Envelope window:"));
    for limit in [
        "Digital samples only",
        "creator intent",
        "-60 dB",
        "FFT window edges",
    ] {
        assert!(f.limits.contains(limit), "{}", f.limits);
    }
    let without_check = recording::findings(&measured.analysis, None);
    assert!(!without_check.low_tones.contains("reported hearing"));
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "independent synthetic samples use decoder f32 precision"
)]
fn summary_signal(
    channels: usize,
    signal: impl Fn(f64) -> [f64; 2],
) -> auditorylab_core::measurement::SampleAnalysis {
    let mut m = auditorylab_core::measurement::Measurement::new(8000, channels, 160_000).unwrap();
    for frame in 0..160_000 {
        let samples = signal(f64::from(frame) / 8000.0).map(|x| x as f32);
        m.push(&samples[..channels]).unwrap();
    }
    m.finish(&|| false).unwrap()
}
fn tone(hz: f64, t: f64) -> f64 {
    0.1 * (std::f64::consts::TAU * hz * t).sin()
}

#[test]
fn summary_explains_beating_harmonics_and_differing_channel_modulation() {
    let a = summary_signal(2, |t| {
        [
            tone(200.0, t) + tone(210.0, t),
            tone(1000.0, t) * (1.0 + 0.8 * (std::f64::consts::TAU * 40.0 * t).cos()),
        ]
    });
    let f = recording::findings(&a, None);
    assert!(
        f.summary.contains(
            "In Left, the whole signal has loudness variation at 10.00 Hz; these rates match \
        pitch differences between detected tones (beating)."
        ),
        "{}",
        f.summary
    );
    assert!(f.summary.contains("beating harmonics"), "{}", f.summary);
    assert!(
        f.summary.contains(
            "In Right, the whole signal has loudness variation at 40.00 Hz; \
        this is a modulation pattern."
        ),
        "{}",
        f.summary
    );
    assert!(f.summary.contains("not creator intent"));
    assert!(
        f.pulsing
            .contains("beating consistent with 200.00 and 210.00 Hz")
    );
    assert!(f.pulsing.contains("modulation pattern"));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../app/tests/fixtures/recording/tone.flac");
    let completed = auditorylab_app::services::analyze_recording(
        &path,
        auditorylab_app::recording::AnalysisOptions::default(),
        &auditorylab_app::job::Cancel::default(),
        &auditorylab_app::job::Progress::default(),
    )
    .unwrap();
    let report = Report::analyzed_recording(&completed);
    let json = serde_json::to_value(report).unwrap();
    assert!(
        json["recording"]["analysis"]["pulsing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["channel"] == 1 && p["cause"]["kind"] == "modulation")
    );
}
#[test]
fn summary_includes_right_only_and_additional_band_specific_findings() {
    let right_only = summary_signal(2, |t| {
        [
            tone(1000.0, t),
            tone(1000.0, t) * (1.0 + 0.8 * (std::f64::consts::TAU * 40.0 * t).cos()),
        ]
    });
    let summary = recording::findings(&right_only, None).summary;
    assert!(
        summary.contains(
            "In Right, the whole signal has loudness variation at 40.00 Hz; \
        this is a modulation pattern."
        ),
        "{summary}"
    );
    assert!(!summary.contains("In Left,"), "{summary}");
    let a = summary_signal(2, |t| {
        [
            tone(1000.0, t),
            tone(3000.0, t) * (1.0 + 0.8 * (std::f64::consts::TAU * 3.0 * t).cos())
                + 0.4 * (tone(40.0, t) + tone(43.0, t)),
        ]
    });
    let s = recording::findings(&a, None).summary;
    assert!(s.contains("In Right, the whole signal"), "{s}");
    assert!(
        s.contains("In Right, the signal above 100 Hz") && s.contains("modulation pattern"),
        "{s}"
    );
    assert!(!s.contains("In Left,"), "{s}");
    assert!(
        s.lines()
            .any(|line| line.starts_with("In Right, the signal below 100 Hz")
                && line.contains("18.00")
                && line.contains("beating harmonics")),
        "{s}"
    );
    assert!(
        !s.contains("In Right, the signal below 100 Hz has loudness variation at 3.00 Hz;"),
        "whole-band attribution already covers this rate: {s}"
    );
}
#[test]
fn summary_names_mono_modulation_and_does_not_invent_silent_rates() {
    let a = summary_signal(1, |t| {
        [
            tone(1000.0, t) * (1.0 + 0.8 * (std::f64::consts::TAU * 40.0 * t).cos()),
            0.0,
        ]
    });
    let s = recording::findings(&a, None).summary;
    assert!(
        s.contains(
            "In Mono, the whole signal has loudness variation at 40.00 Hz; \
        this is a modulation pattern."
        ),
        "{s}"
    );
    assert!(
        !s.contains("In Mono, the signal above 100 Hz"),
        "duplicate band rate: {s}"
    );
    let silent = summary_signal(2, |_| [0.0; 2]);
    let s = recording::findings(&silent, None).summary;
    assert!(!s.contains("loudness variation"), "{s}");
}

#[test]
fn undefined_energy_and_zero_have_distinct_shared_text() {
    let mut share = auditorylab_core::measurement::EnergyShare {
        channel: 0,
        below_hz: 50.0,
        fraction: None,
    };
    assert_eq!(
        recording::energy_row(&share, 1),
        "Mono · below 50 Hz: undefined (digital silence)"
    );
    share.fraction = Some(0.0);
    assert_eq!(recording::energy_row(&share, 2), "Left · below 50 Hz: 0.0%");
}
#[test]
fn common_energy_rows_own_selection_and_channel_threshold_order() {
    use auditorylab_core::measurement::{EnergyShare, Measurement, test_support};
    let mut measurement = Measurement::new(1000, 2, 2000).unwrap();
    for _ in 0..2000 {
        measurement.push(&[0.0, 0.0]).unwrap();
    }
    let samples = test_support::with_energy_shares(
        measurement.finish(&|| false).unwrap(),
        [
            (1, 300.0, Some(0.5)),
            (0, 40.0, Some(0.9)),
            (0, 100.0, Some(0.25)),
            (1, 50.0, Some(0.0)),
            (0, 300.0, Some(1.0)),
            (1, 100.0, None),
            (0, 60.0, Some(0.0)),
            (1, 40.0, Some(0.8)),
            (1, 60.0, Some(0.125)),
            (0, 50.0, None),
        ]
        .into_iter()
        .map(|(channel, below_hz, fraction)| EnergyShare {
            channel,
            below_hz,
            fraction,
        })
        .collect(),
    );
    let expected = [
        "Left · below 50 Hz: undefined (digital silence)",
        "Left · below 60 Hz: 0.0%",
        "Left · below 100 Hz: 25.0%",
        "Left · below 300 Hz: 100.0%",
        "Right · below 50 Hz: 0.0%",
        "Right · below 60 Hz: 12.5%",
        "Right · below 100 Hz: undefined (digital silence)",
        "Right · below 300 Hz: 50.0%",
    ];
    let rows = recording::energy_rows(&samples)
        .map(|share| recording::energy_row(share, 2))
        .collect::<Vec<_>>();
    assert_eq!(rows, expected);
    assert_eq!(
        recording::findings(&samples, None)
            .low_tones
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn current_device_context_reinterprets_retained_samples_without_mutation() {
    use auditorylab_app::device_check::{Assessment, LowToneResult};
    use auditorylab_app::recording::{Analysis, InterpretationContext};
    let a = Analysis::complete(
        "PCM".into(),
        false,
        None,
        0,
        summary_signal(1, |_| [0.0; 2]),
    )
    .unwrap();
    let captured = a.clone();
    for channels in [
        ChannelResult::Incomplete,
        ChannelResult::Separate,
        ChannelResult::Swapped,
        ChannelResult::Mixed,
        ChannelResult::Silent,
        ChannelResult::OneSide(DeviceTest::LeftChannel),
        ChannelResult::Inconsistent,
    ] {
        for low_tones in [
            LowToneResult::Incomplete,
            LowToneResult::AllHeard,
            LowToneResult::LowestHeard {
                test: DeviceTest::Tone60Hz,
            },
            LowToneResult::NoneHeard,
        ] {
            let context = InterpretationContext::from(Assessment {
                channels,
                low_tones,
            });
            let f = recording::contextual_findings(&a, context);
            assert!(f.ears.contains(&device_check::channel_result(channels)));
            assert!(
                f.low_tones
                    .contains(&device_check::low_tone_result(low_tones))
            );
            assert_eq!(a, captured);
        }
    }
}
