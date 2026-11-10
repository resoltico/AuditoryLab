use super::*;
use auditorylab_app::recording::Analysis;
use auditorylab_core::measurement::{EarDifference, Measurement, SampleAnalysis};

fn silence(channels: usize) -> SampleAnalysis {
    let mut measurement = Measurement::new(1000, channels, 2000).unwrap();
    for _ in 0..2000 {
        measurement.push(&vec![0.0; channels]).unwrap();
    }
    measurement.finish(&|| false).unwrap()
}

#[test]
fn absolute_completed_facts_are_unique_and_limits_do_not_restate_coordinates() {
    for (start, source, analyzed, envelope, duration) in [
        (
            0,
            Some(6000),
            "Analyzed: 0.0–2.0 s",
            "Envelope window: 0.1–1.9 s",
            "source duration 6.0 s",
        ),
        (
            3000,
            Some(6000),
            "Analyzed: 3.0–5.0 s",
            "Envelope window: 3.1–4.9 s",
            "source duration 6.0 s",
        ),
        (
            4000,
            Some(6000),
            "Analyzed: 4.0–6.0 s",
            "Envelope window: 4.1–5.9 s",
            "source duration 6.0 s",
        ),
        (
            3000,
            None,
            "Analyzed: 3.0–5.0 s",
            "Envelope window: 3.1–4.9 s",
            "source duration unknown",
        ),
    ] {
        let analysis = Analysis::complete("PCM".into(), false, source, start, silence(1)).unwrap();
        let before = analysis.clone();
        let f = recording_findings(&analysis, None);
        assert!(f.facts.contains(analyzed), "{}", f.facts);
        assert!(f.facts.contains(envelope), "{}", f.facts);
        assert!(f.facts.contains(duration));
        assert_eq!(f.facts.matches("Analyzed:").count(), 1);
        assert_eq!(f.facts.matches("Envelope window:").count(), 1);
        assert_eq!(f.facts.matches("Tone resolution:").count(), 1);
        assert!(!f.facts.contains("relative"));
        assert!(!f.limits.contains("Window "));
        assert!(!f.limits.contains("resolution"));
        for caveat in [
            "Envelope rates below",
            "device output and hearing are not measured",
            "not creator intent",
            "later corruption may not be found",
            "not measured brain activity",
            "Lossless decoded samples",
        ] {
            assert!(f.limits.contains(caveat), "{}", f.limits);
        }
        let context = auditorylab_app::recording::InterpretationContext::from(
            auditorylab_app::device_check::DeviceCheck::default().assessment(),
        );
        assert_eq!(contextual_findings(&analysis, context).facts, f.facts);
        assert_eq!(analysis, before);
    }
    let relative = findings(&silence(1), None);
    assert!(relative.facts.starts_with("Sample-relative coordinates\n"));
    assert!(relative.facts.contains("Analyzed: 0.0–2.0 s"));
}

#[test]
fn candidate_overview_preserves_distinct_empty_states_and_never_lists_representatives() {
    let samples = silence(2);
    let mut facts = (*samples).clone();
    assert!(ear_overview(&facts).starts_with("No channel-exclusive tone pair"));
    facts.coincident_tones = true;
    assert!(ear_overview(&facts).starts_with("No left/right tone difference found"));
    facts.channels = 1;
    assert_eq!(
        ear_overview(&facts),
        "Mono recording; no left/right comparison is available."
    );
    facts.channels = 2;
    facts.coincident_tones = false;
    // Hand-specified unsorted comparisons; equal differences are still distinct pairs.
    let pairs = [
        (330.0, 335.0),
        (200.0, 205.0),
        (350.0, 355.0),
        (210.0, 215.0),
        (340.0, 345.0),
        (220.0, 225.0),
        (320.0, 325.0),
        (230.0, 235.0),
        (310.0, 315.0),
        (240.0, 245.0),
        (300.0, 305.0),
        (250.0, 255.0),
        (290.0, 295.0),
        (260.0, 265.0),
        (280.0, 285.0),
        (270.0, 275.0),
    ];
    facts.ear_differences = pairs
        .into_iter()
        .map(|(left_hz, right_hz)| EarDifference {
            left_hz,
            right_hz,
            difference_hz: 5.0,
            band: Some(auditorylab_core::rhythm::Band::Theta),
        })
        .collect();
    assert_eq!(
        ear_overview(&facts),
        "16 left-versus-right tone comparison candidates met the detection thresholds. See Left versus right for the complete list."
    );
    let details = ear_findings(&facts);
    assert_eq!(details.lines().count(), 16);
    let expected = [
        ("330.00", "335.00"),
        ("200.00", "205.00"),
        ("350.00", "355.00"),
        ("210.00", "215.00"),
        ("340.00", "345.00"),
        ("220.00", "225.00"),
        ("320.00", "325.00"),
        ("230.00", "235.00"),
        ("310.00", "315.00"),
        ("240.00", "245.00"),
        ("300.00", "305.00"),
        ("250.00", "255.00"),
        ("290.00", "295.00"),
        ("260.00", "265.00"),
        ("280.00", "285.00"),
        ("270.00", "275.00"),
    ];
    for (line, (left, right)) in details.lines().zip(expected) {
        assert_eq!(
            line,
            format!("Left {left} Hz · right {right} Hz · difference 5.00 Hz (Theta)")
        );
    }
    assert!(!ear_overview(&facts).contains("330.00"));
    facts.ear_differences.truncate(1);
    assert_eq!(
        ear_overview(&facts),
        "1 left-versus-right tone comparison candidate met the detection thresholds. See Left versus right for the complete list."
    );
    let small = ear_overview(&facts).len();
    facts.ear_differences = facts
        .ear_differences
        .iter()
        .cycle()
        .take(1600)
        .cloned()
        .collect();
    assert!(ear_overview(&facts).len() < small + 10);
}
