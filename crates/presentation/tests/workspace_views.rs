//! Public shared interpretation and capture contracts, independent of UI widgets.
use auditorylab_app::core::CarrierFacts;
use auditorylab_app::core::CarrierKind;
use auditorylab_app::core::ComponentForm;
use auditorylab_app::core::ComponentId;
use auditorylab_app::core::Ears;
use auditorylab_app::core::Modulation;
use auditorylab_app::core::ModulationKind;
use auditorylab_app::core::ParameterId;
use auditorylab_app::core::SampleRate;
use auditorylab_app::core::rhythm::Method;
use auditorylab_app::core::rhythm::Structure;
use auditorylab_presentation::numbers::NumberStyle;

use auditorylab_app::workspace::monitor::CaptureError;
use auditorylab_app::workspace::monitor::ModelSource;
use auditorylab_app::workspace::monitor::ModelView;
use auditorylab_app::workspace::monitor::WaveformCapture;
use auditorylab_app::workspace::monitor::WaveformScale;
fn tone(hz: f64, ears: Ears) -> ComponentForm {
    ComponentForm::Tone {
        frequency_hz: finite(hz),
        ears,
        peak_dbfs: finite(-40.0),
        modulation: None,
    }
}
#[expect(
    clippy::unwrap_used,
    reason = "Test fixture helper deliberately panics if an enumerated component ID \
        or specified target is invalid, causing its calling presentation test \
        to fail"
)]
fn summary(components: &[ComponentForm]) -> Structure {
    let components = components
        .iter()
        .enumerate()
        .map(|(i, c)| auditorylab_app::core::Component {
            id: ComponentId::numbered(i).unwrap(),
            form: *c,
        })
        .collect();
    auditorylab_app::core::rhythm::realized(
        &auditorylab_app::core::Sound::new(finite(1.0), finite(0.0), components)
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
            .target(SampleRate::DEFAULT)
            .unwrap(),
    )
}
#[test]
fn physical_meaning_has_positive_and_rejected_neighbors() {
    let click = {
        let base = tone(440.0, Ears::Both);
        ComponentForm::Clicks {
            rate_hz: finite(40.0),
            width_seconds: finite(0.001),
            ears: base.route().unwrap(),
            peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
            modulation: base.modulation(),
        }
    };
    let s = summary(&[click, tone(440.0, Ears::Both)]);
    assert_eq!(s.rhythms.len(), 1);
    assert_eq!(s.rhythms[0].method, Method::Clicks);
    let text = auditorylab_presentation::structure_summary(&(s));
    assert!(
        text.contains("40 Hz clicks")
            && text.contains("440 Hz steady tone")
            && !text.contains("400 Hz")
            && !text.contains("Multiple components")
    );
    let left = tone(380.0, Ears::Left);
    let right = tone(420.0, Ears::Right);
    let a = tone(380.0, Ears::Both);
    let b = tone(420.0, Ears::Both);
    for raw in [
        vec![left, right],
        vec![a, b],
        vec![a, tone(380.0, Ears::Both)],
        vec![a, right],
        vec![a, b, click],
    ] {
        let structure = summary(&raw);
        assert_eq!(
            structure.pairs,
            [] as [auditorylab_core::rhythm::PairRates; 0]
        );
        assert!(
            !auditorylab_presentation::structure_summary(&(structure))
                .contains("interaural difference")
        );
        assert!(
            !auditorylab_presentation::structure_summary(&(structure)).contains("monaural beat")
        );
        assert!(!auditorylab_presentation::structure_summary(&(structure)).contains("Unison"));
    }
    assert_explicit_pair_meaning();
    let gate = {
        let mut form = tone(10000.0, Ears::Both);
        form.set_modulation(Some(Modulation::Gate {
            rate_hz: finite(8.0),
            width_seconds: finite(0.001),
        }));
        form
    };
    let s = summary(&[gate, click]);
    assert_eq!(s.rhythms.len(), 2);
    assert!(
        auditorylab_presentation::structure_summary(&(s)).contains("8 Hz gate")
            && auditorylab_presentation::structure_summary(&(s)).contains("40 Hz clicks")
    );
    let active = {
        let mut form = a;
        form.set_modulation(Some(Modulation::Am {
            rate_hz: finite(39.0625),
            depth: finite(0.5),
        }));
        form
    };
    let am = summary(&[active]);
    assert_eq!(am.rhythms.len(), 1);
    assert_eq!(am.rhythms[0].method, Method::Am);
    assert!(
        auditorylab_presentation::structure_summary(&(am)).contains("39.0625")
            && auditorylab_presentation::structure_summary(&(am)).contains("50%")
    );
    let inactive = {
        let mut form = a;
        form.set_modulation(Some(Modulation::Am {
            rate_hz: finite(39.0625),
            depth: finite(0.0),
        }));
        form
    };
    assert_eq!(
        summary(&[inactive]).rhythms,
        [] as [auditorylab_core::rhythm::Descriptor; 0]
    );
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
    assert_eq!(
        auditorylab_presentation::numbers::format(NumberStyle::Compact, 39.0625),
        "39.0625"
    );
    assert_eq!(
        auditorylab_presentation::numbers::format(NumberStyle::Compact, 39.062_512_345_678_9),
        "≈39.0625"
    );
    let exact = 39.062_512_345_678_9;
    assert_eq!(
        auditorylab_app::document::input::parameter_text(ParameterId::ToneFrequency, exact),
        "39.0625123456789"
    );
    let c = {
        let base = tone(440.0, Ears::Both);
        ComponentForm::Clicks {
            rate_hz: finite(40.0),
            width_seconds: finite(0.001),
            ears: base.route().unwrap(),
            peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
            modulation: base.modulation(),
        }
    };
    let t = auditorylab_app::core::Sound::new(
        finite(1.0),
        finite(0.0),
        vec![auditorylab_app::core::Component {
            id: ComponentId::new(9).unwrap(),
            form: c,
        }],
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
    .target(SampleRate::new(44100).unwrap())
    .unwrap();
    let m = ModelView::new(&(t), "draft".into(), ModelSource::Draft);
    assert!(
        auditorylab_presentation::workspace::monitor::model_view_caption(&(m)).contains("origin 0")
            && auditorylab_presentation::workspace::monitor::model_view_caption(&(m))
                .contains("44100 Hz")
    );
    let CarrierFacts::Clicks(p) = m.projection.lanes[0].facts.carrier else {
        panic!("click lane")
    };
    assert_eq!(p.width_frames, 44);
}

#[test]
fn a_carrier_is_named_in_sentence_case() {
    let describe = |component: ComponentForm| {
        let check = auditorylab_core::Sound::new(
            finite(1.0),
            finite(0.0),
            ([(ComponentId::new(1).unwrap(), component)])
                .iter()
                .map(|(id, voice)| auditorylab_core::Component {
                    id: *id,
                    form: *voice,
                })
                .collect(),
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        .check(SampleRate::DEFAULT);
        auditorylab_presentation::voice_summary(&(check.voices()[0].1.clone().unwrap()))
    };
    let modulated = |modulation| {
        let mut form = tone(440.0, Ears::Both);
        form.set_modulation(Some(modulation));
        form
    };
    let clicks = {
        let base = tone(440.0, Ears::Both);
        ComponentForm::Clicks {
            rate_hz: finite(40.0),
            width_seconds: finite(0.001),
            ears: base.route().unwrap(),
            peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
            modulation: base.modulation(),
        }
    };
    let plain = describe(tone(440.0, Ears::Both));
    assert!(plain.starts_with("Tone 440 Hz"), "{plain}");
    let pulses = describe(clicks);
    assert!(pulses.starts_with("Pulse rate 40 "), "{pulses}");
    let am = describe(modulated(Modulation::Am {
        rate_hz: finite(8.0),
        depth: finite(0.5),
    }));
    assert!(am.contains("· carrier tone 440 Hz"), "{am}");
    let gate = describe(modulated(Modulation::Gate {
        rate_hz: finite(8.0),
        width_seconds: finite(0.001),
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
        let component = {
            let base = tone(440.0, Ears::Both);
            ComponentForm::Clicks {
                rate_hz: finite(40.0),
                width_seconds: finite(requested),
                ears: base.route().unwrap(),
                peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
                modulation: base.modulation(),
            }
        };
        let check = auditorylab_core::Sound::new(
            finite(1.0),
            finite(0.0),
            ([(ComponentId::new(1).unwrap(), component)])
                .iter()
                .map(|(id, voice)| auditorylab_core::Component {
                    id: *id,
                    form: *voice,
                })
                .collect(),
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        .check(rate);
        let facts = check.voices()[0].1.clone().unwrap();
        let brief = auditorylab_presentation::voice_summary(&(facts));
        let detail = auditorylab_presentation::voice_details(&(facts));
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
            auditorylab_app::document::input::parameter_text(ParameterId::ClickWidth, requested),
            (requested * 1000.0).to_string()
        );
    }
}

#[test]
fn every_parameter_has_shared_vocabulary_and_exact_field_attribution() {
    let rate = SampleRate::DEFAULT;
    for id in ParameterId::ALL {
        let d = id.descriptor();
        assert_eq!(
            auditorylab_presentation::parameter_input_label(id),
            format!("{} ({})", d.label, d.unit.symbol())
        );
    }
    assert_eq!(
        invalid_parameters(tone(30000.0, Ears::Both)),
        [ParameterId::ToneFrequency]
    );
    let clicks = {
        let base = tone(440.0, Ears::Both);
        ComponentForm::Clicks {
            rate_hz: finite(160.0),
            width_seconds: finite(0.010),
            ears: base.route().unwrap(),
            peak_dbfs: base.get(auditorylab_core::ParameterId::PeakLevel).unwrap(),
            modulation: base.modulation(),
        }
    };
    assert_eq!(
        invalid_parameters(clicks),
        [ParameterId::ClickRate, ParameterId::ClickWidth]
    );
    let sideband = {
        let mut form = tone(16000.0, Ears::Both);
        form.set_modulation(Some(Modulation::Am {
            rate_hz: finite(10000.0),
            depth: finite(0.5),
        }));
        form
    };
    assert_eq!(
        invalid_parameters(sideband),
        [ParameterId::ToneFrequency, ParameterId::AmRate]
    );
    assert_eq!(
        invalid_parameters({
            let mut form = tone(440.0, Ears::Both);
            form.set(auditorylab_core::ParameterId::PeakLevel, finite(1.0));
            form
        }),
        [ParameterId::PeakLevel]
    );
    let depth = {
        let mut form = tone(440.0, Ears::Both);
        form.set_modulation(Some(Modulation::Am {
            rate_hz: finite(40.0),
            depth: finite(1.5),
        }));
        form
    };
    let error = auditorylab_core::Sound::new(
        finite(1.0),
        finite(0.0),
        ([(ComponentId::new(1).unwrap(), depth)])
            .iter()
            .map(|(id, voice)| auditorylab_core::Component {
                id: *id,
                form: *voice,
            })
            .collect(),
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
    .check(rate)
    .voices()[0]
        .1
        .clone()
        .unwrap_err();
    assert_eq!(
        auditorylab_presentation::errors::acoustic::voice_error(&error),
        "Modulation depth 150% must be from 0% to 100%"
    );
    for kind in CarrierKind::ALL {
        assert_ne!(kind.label(), "");
    }
    for kind in ModulationKind::ALL {
        assert_ne!(kind.label(), "");
    }
    for ears in Ears::ALL {
        assert!(ears.label().ends_with("ear") || ears.label().ends_with("ears"));
    }
}

#[test]
fn raw_tone_timing_uses_only_explicit_active_modulation() {
    for (ears, interaural) in [(Ears::Both, false), (Ears::Left, true)] {
        let a = tone(200.0, ears);
        let mut b = tone(202.0, if interaural { Ears::Right } else { ears });
        b.set(ParameterId::PeakLevel, finite(-41.0));
        b.set_modulation(Some(Modulation::Am {
            rate_hz: finite(40.0),
            depth: finite(0.0),
        }));
        let s = summary(&[a, b]);
        assert_eq!(s.slowest_rate(), None);
        assert_eq!(s.rhythms, [] as [auditorylab_core::rhythm::Descriptor; 0]);
        assert!(s.interaural().is_none());
        b.set_modulation(Some(Modulation::Am {
            rate_hz: finite(40.0),
            depth: finite(0.001),
        }));
        assert_eq!(
            summary(&[a, b]).pairs,
            [] as [auditorylab_core::rhythm::PairRates; 0]
        );
        assert!((summary(&[a, b]).slowest_rate().unwrap() - 40.0).abs() < 1e-10);
        assert_eq!(summary(&[a, a]).slowest_rate(), None);
    }
}

const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

#[expect(
    clippy::expect_used,
    reason = "Invalid fixtures must fail this assertion helper"
)]
fn assert_explicit_pair_meaning() {
    for binaural in [true, false] {
        let pair = auditorylab_app::core::Pair {
            first_hz: finite(380.0),
            offset_hz: finite(40.0),
            first_peak_dbfs: finite(-40.0),
            second_peak_dbfs: finite(-40.0),
        };
        let form = if binaural {
            {
                auditorylab_app::core::ComponentForm::BinauralPair {
                    left_hz: pair.first_hz,
                    offset_hz: pair.offset_hz,
                    left_peak_dbfs: pair.first_peak_dbfs,
                    right_peak_dbfs: pair.second_peak_dbfs,
                }
            }
        } else {
            {
                auditorylab_app::core::ComponentForm::MonauralPair {
                    ears: Ears::Both,
                    first_hz: pair.first_hz,
                    offset_hz: pair.offset_hz,
                    first_peak_dbfs: pair.first_peak_dbfs,
                    second_peak_dbfs: pair.second_peak_dbfs,
                }
            }
        };
        let target = auditorylab_app::core::Sound::new(
            finite(1.0),
            finite(0.0),
            vec![auditorylab_app::core::Component {
                id: ComponentId::new(1).expect("valid fixture"),
                form,
            }],
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
        .target(SampleRate::DEFAULT)
        .expect("valid fixture");
        let structure = auditorylab_app::core::rhythm::realized(&target);
        assert_eq!(structure.pairs.len(), 1);
        assert!(
            auditorylab_presentation::structure_summary(&(structure)).contains(if binaural {
                "40 Hz interaural difference"
            } else {
                "40 Hz monaural beat"
            })
        );
    }
}

#[expect(
    clippy::expect_used,
    reason = "Invalid fixtures must fail this assertion helper"
)]
fn invalid_parameters(c: ComponentForm) -> Vec<ParameterId> {
    let rate = SampleRate::DEFAULT;

    auditorylab_core::Sound::new(
        finite(1.0),
        finite(0.0),
        ([(ComponentId::new(1).expect("valid fixture"), c)])
            .iter()
            .map(|(id, voice)| auditorylab_core::Component {
                id: *id,
                form: *voice,
            })
            .collect(),
    )
    .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"))
    .check(rate)
    .voices()[0]
        .1
        .clone()
        .expect_err("invalid fixture")
        .parameters()
        .to_vec()
}
