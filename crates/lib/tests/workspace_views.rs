//! Public shared interpretation and capture contracts, independent of UI widgets.
use auditorylab::*;
fn tone(hz: f64, ears: Ears) -> Component {
    Component {
        ears,
        peak_dbfs: -40.0,
        carrier: Carrier::Tone { frequency_hz: hz },
        modulation: None,
    }
}
#[allow(
    clippy::unwrap_used,
    reason = "bounded valid test fixtures with explicit panic diagnostics"
)]
fn summary(components: &[Component]) -> SoundSummary {
    let pairs: Vec<_> = components
        .iter()
        .enumerate()
        .map(|(i, c)| (u32::try_from(i).unwrap(), *c))
        .collect();
    SoundSummary::target(&Target::new(SampleRate::DEFAULT, &pairs).unwrap())
}
#[test]
fn physical_meaning_has_positive_and_rejected_neighbors() {
    let click = Component {
        carrier: Carrier::Click {
            rate_hz: 40.0,
            width_seconds: 0.001,
        },
        ..tone(440.0, Ears::Both)
    };
    let s = summary(&[click, tone(440.0, Ears::Both)]);
    assert_eq!(s.rhythms.len(), 1);
    assert_eq!(s.rhythms[0].kind, RhythmKind::Clicks);
    let text = s.description();
    assert!(
        text.contains("40 Hz clicks")
            && text.contains("440 Hz steady tone")
            && !text.contains("400 Hz")
            && !text.contains("Multiple components")
    );
    let left = tone(380.0, Ears::Left);
    let right = tone(420.0, Ears::Right);
    assert!(matches!(
        summary(&[left, right]).relationship,
        Some(ToneRelationship::Interaural { .. })
    ));
    let a = tone(380.0, Ears::Both);
    let b = tone(420.0, Ears::Both);
    assert!(matches!(
        summary(&[a, b]).relationship,
        Some(ToneRelationship::Monaural { .. })
    ));
    assert!(
        summary(&[a, tone(380.0, Ears::Both)])
            .description()
            .contains("Unison")
    );
    for bad in [
        vec![
            a,
            Component {
                peak_dbfs: -41.0,
                ..b
            },
        ],
        vec![a, right],
        vec![a, b, click],
        vec![
            a,
            Component {
                modulation: Some(Modulation::Sinusoidal {
                    rate_hz: 8.0,
                    depth: 0.0,
                }),
                ..b
            },
        ],
    ] {
        assert!(summary(&bad).relationship.is_none());
    }
    let gate = Component {
        modulation: Some(Modulation::Rectangular {
            rate_hz: 8.0,
            width_seconds: 0.001,
        }),
        ..tone(10000.0, Ears::Both)
    };
    let s = summary(&[gate, click]);
    assert_eq!(s.rhythms.len(), 2);
    assert!(s.description().contains("8 Hz gate") && s.description().contains("40 Hz clicks"));
    let active = Component {
        modulation: Some(Modulation::Sinusoidal {
            rate_hz: 39.0625,
            depth: 0.5,
        }),
        ..a
    };
    let am = summary(&[active]);
    assert_eq!(am.rhythms.len(), 1);
    assert_eq!(am.rhythms[0].kind, RhythmKind::AmplitudeModulation);
    assert!(am.description().contains("39.0625") && am.description().contains("50%"));
    let inactive = Component {
        modulation: Some(Modulation::Sinusoidal {
            rate_hz: 39.0625,
            depth: 0.0,
        }),
        ..a
    };
    assert!(summary(&[inactive]).rhythms.is_empty());
}
#[test]
fn captures_fit_both_ears_without_changing_meters_and_reject_invalid_data() {
    let samples = [[0.0, 0.0], [0.1, -0.01], [-0.2, 0.03], [0.0, 0.0]];
    let rate = SampleRate::DEFAULT;
    let fixed = WaveformCapture::samples(
        "source A".into(),
        rate,
        (7, 11),
        &samples,
        WaveformScale::Fixed,
    )
    .unwrap();
    let fit = WaveformCapture::samples(
        "source A".into(),
        rate,
        (7, 11),
        &samples,
        WaveformScale::Fit,
    )
    .unwrap();
    assert_eq!(fixed.limit(), 1.0);
    assert_eq!(fit.limit(), 0.2);
    assert_eq!(fixed.bins(), fit.bins());
    assert_eq!(fixed.meters()[0].peak_dbfs, fit.meters()[0].peak_dbfs);
    assert_eq!(fixed.meters()[1].rms_dbfs, fit.meters()[1].rms_dbfs);
    let frozen = fit.clone();
    let _other = WaveformCapture::samples(
        "source B".into(),
        SampleRate::new(44100).unwrap(),
        (0, 4),
        &samples,
        WaveformScale::Fixed,
    )
    .unwrap();
    assert_eq!(frozen.title(), "source A");
    assert_eq!(frozen.span(), (7, 11));
    assert_eq!(frozen.limit(), 0.2);
    assert_eq!(frozen.rate(), rate);
    assert_eq!(
        WaveformCapture::samples(
            "bad".into(),
            rate,
            (0, 1),
            &[[f64::NAN, 0.0]],
            WaveformScale::Fit
        )
        .unwrap_err(),
        CaptureError::NonFiniteSample
    );
    assert_eq!(
        WaveformCapture::samples(
            "bad span".into(),
            rate,
            (0, 7),
            &samples,
            WaveformScale::Fixed
        )
        .unwrap_err(),
        CaptureError::InvalidSpan
    );
    let tiny = WaveformCapture::samples(
        "tiny".into(),
        rate,
        (0, 2),
        &[[1e-200, -1e-200]; 2],
        WaveformScale::Fit,
    )
    .unwrap();
    assert_eq!(tiny.limit(), 1e-200);
    assert!((tiny.meters()[0].rms_dbfs + 4000.0).abs() < 1e-10);
    let loud = WaveformCapture::samples(
        "above scale".into(),
        rate,
        (0, 1),
        &[[2.0, 0.5]],
        WaveformScale::Fixed,
    )
    .unwrap();
    assert!(loud.clipped());
    assert!(loud.meters()[0].peak_dbfs > 0.0);
    let silence = WaveformCapture::samples(
        "silence".into(),
        rate,
        (0, 4),
        &[[0.0; 2]; 4],
        WaveformScale::Fit,
    )
    .unwrap();
    assert_eq!(silence.limit(), 1.0);
    assert_eq!(silence.meters()[0].peak_dbfs, f64::NEG_INFINITY);
}
#[test]
fn compact_rounding_never_becomes_exact_entry_and_model_width_remains_quantized() {
    assert_eq!(compact_number(39.0625), "39.0625");
    assert_eq!(compact_number(39.0625123456789), "≈39.0625");
    assert_eq!(compact_number(100000.0), "100000");
    assert!(!compact_number(f64::from_bits(1)).starts_with('0'));
    let mut c = tone(440.0, Ears::Both);
    let exact = 39.0625123456789;
    set_setting(&mut c, Setting::Frequency, exact);
    assert_eq!(setting_value(&c, Setting::Frequency), Some(exact));
    let _ = summary(&[c]).description();
    assert_eq!(setting_value(&c, Setting::Frequency), Some(exact));
    let c = Component {
        carrier: Carrier::Click {
            rate_hz: 40.0,
            width_seconds: 0.001,
        },
        ..c
    };
    let t = Target::new(SampleRate::new(44100).unwrap(), &[(9, c)]).unwrap();
    let m = ModelView::new(t, "draft".into(), ModelSource::Draft);
    assert!(m.caption().contains("origin 0") && m.caption().contains("44100 Hz"));
    if let CarrierFacts::Clicks(p) = m.projection.lanes[0].facts.carrier {
        assert_eq!(p.width_frames, 44);
    } else {
        panic!()
    }
}
