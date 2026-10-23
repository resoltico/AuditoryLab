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
#[expect(
    clippy::unwrap_used,
    reason = "Test fixture helper deliberately panics if an enumerated component ID or specified target is invalid, causing its calling presentation test to fail"
)]
fn summary(components: &[Component]) -> Structure {
    let pairs: Vec<_> = components
        .iter()
        .enumerate()
        .map(|(i, c)| (ComponentId::numbered(i).unwrap(), *c))
        .collect();
    Structure::of_target(&Target::new(SampleRate::DEFAULT, &pairs).unwrap())
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
    let text = structure_description(&s);
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
    assert!(structure_description(&summary(&[a, tone(380.0, Ears::Both)])).contains("Unison"));
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
    assert!(
        structure_description(&s).contains("8 Hz gate")
            && structure_description(&s).contains("40 Hz clicks")
    );
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
    assert!(
        structure_description(&am).contains("39.0625")
            && structure_description(&am).contains("50%")
    );
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
    let exact = 39.0625123456789;
    assert_eq!(
        editable_number(ParameterId::ToneFrequency, exact),
        "39.0625123456789"
    );
    let c = Component {
        carrier: Carrier::Click {
            rate_hz: 40.0,
            width_seconds: 0.001,
        },
        ..tone(440.0, Ears::Both)
    };
    let t = Target::new(SampleRate::new(44100).unwrap(), &[(ComponentId::new(9), c)]).unwrap();
    let m = ModelView::new(t, "draft".into(), ModelSource::Draft);
    assert!(m.caption().contains("origin 0") && m.caption().contains("44100 Hz"));
    let CarrierFacts::Clicks(p) = m.projection.lanes[0].facts.carrier else {
        panic!("click lane")
    };
    assert_eq!(p.width_frames, 44);
}

#[test]
fn a_carrier_is_named_in_sentence_case() {
    let describe = |component: Component| {
        let check = TargetCheck::new(SampleRate::DEFAULT, &[(ComponentId::new(1), component)]);
        component_description(&check.components()[0].1.clone().unwrap(), false)
    };
    let modulated = |modulation| Component {
        modulation: Some(modulation),
        ..tone(440.0, Ears::Both)
    };
    let clicks = Component {
        carrier: Carrier::Click {
            rate_hz: 40.0,
            width_seconds: 0.001,
        },
        ..tone(440.0, Ears::Both)
    };
    let plain = describe(tone(440.0, Ears::Both));
    assert!(plain.starts_with("Tone 440 Hz"), "{plain}");
    let pulses = describe(clicks);
    assert!(pulses.starts_with("Pulse rate 40 "), "{pulses}");
    let am = describe(modulated(Modulation::Sinusoidal {
        rate_hz: 8.0,
        depth: 0.5,
    }));
    assert!(am.contains("· carrier tone 440 Hz"), "{am}");
    let gate = describe(modulated(Modulation::Rectangular {
        rate_hz: 8.0,
        width_seconds: 0.001,
    }));
    assert!(gate.contains("· carrier tone 440 Hz"), "{gate}");
}

#[test]
fn pulse_widths_are_requested_in_editing_and_realized_in_generated_views() {
    // 0.1 ms at 8 kHz realizes one frame, 0.125 ms; 1 ms at 44.1 kHz realizes 44 frames.
    for (hz, requested, frames, realized) in [
        (8_000, 0.0001, 1, "0.125"),
        (44_100, 0.001, 44, "≈0.997732"),
    ] {
        let rate = SampleRate::new(hz).unwrap();
        let component = Component {
            carrier: Carrier::Click {
                rate_hz: 40.0,
                width_seconds: requested,
            },
            ..tone(440.0, Ears::Both)
        };
        let check = TargetCheck::new(rate, &[(ComponentId::new(1), component)]);
        let facts = check.components()[0].1.clone().unwrap();
        let brief = component_description(&facts, false);
        let detail = component_description(&facts, true);
        assert!(brief.contains(&format!("{realized} ms open")), "{brief}");
        assert!(
            detail.contains(&format!("requested {} ms", requested * 1000.0)),
            "{detail}"
        );
        assert!(
            detail.contains(&format!("realized {frames} frame")),
            "{detail}"
        );
        assert!(detail.contains(&format!("at {hz} Hz")), "{detail}");
        assert_eq!(
            editable_number(ParameterId::ClickWidth, requested),
            (requested * 1000.0).to_string()
        );
    }
}

#[test]
fn every_parameter_has_shared_vocabulary_and_exact_field_attribution() {
    let rate = SampleRate::DEFAULT;
    for id in ParameterId::ALL {
        let d = id.descriptor();
        assert_eq!(labeled(id), format!("{} ({})", d.label, d.unit.symbol()));
    }
    let check = |c: Component| {
        TargetCheck::new(rate, &[(ComponentId::new(1), c)]).components()[0]
            .1
            .clone()
            .unwrap_err()
            .parameters()
            .to_vec()
    };
    assert_eq!(
        check(tone(30000.0, Ears::Both)),
        [ParameterId::ToneFrequency]
    );
    let clicks = Component {
        carrier: Carrier::Click {
            rate_hz: 160.0,
            width_seconds: 0.010,
        },
        ..tone(440.0, Ears::Both)
    };
    assert_eq!(
        check(clicks),
        [ParameterId::ClickRate, ParameterId::ClickWidth]
    );
    let sideband = Component {
        modulation: Some(Modulation::Sinusoidal {
            rate_hz: 10000.0,
            depth: 0.5,
        }),
        ..tone(16000.0, Ears::Both)
    };
    assert_eq!(
        check(sideband),
        [
            ParameterId::ToneFrequency,
            ParameterId::AmplitudeModulationRate
        ]
    );
    assert_eq!(
        check(Component {
            peak_dbfs: 1.0,
            ..tone(440.0, Ears::Both)
        }),
        [ParameterId::PeakLevel]
    );
    let depth = Component {
        modulation: Some(Modulation::Sinusoidal {
            rate_hz: 40.0,
            depth: 1.5,
        }),
        ..tone(440.0, Ears::Both)
    };
    let error = TargetCheck::new(rate, &[(ComponentId::new(1), depth)]).components()[0]
        .1
        .clone()
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Modulation depth 150% must be from 0% to 100%"
    );
    for kind in CarrierKind::ALL {
        assert!(!kind.label().is_empty());
    }
    for kind in ModulationKind::ALL {
        assert!(!kind.label().is_empty());
    }
    for ears in Ears::ALL {
        assert!(ears.label().ends_with("ear") || ears.label().ends_with("ears"));
    }
}
