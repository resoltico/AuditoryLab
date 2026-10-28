//! sound presentation.
use crate::*;
use auditorylab_core::{
    CarrierFacts, Ears, ModulationFacts, Observation, ParameterId, PhaseRate, PulseFacts,
    VoiceFacts, VoiceObservation, rhythm::Method, rhythm::Structure,
};
pub(crate) fn pulse_detail(p: PulseFacts) -> String {
    format!(
        "{} Hz · requested {} ms · realized {} frame{} ({} ms) at {} Hz",
        p.rate_hz,
        p.requested_width_seconds * 1000.0,
        p.width_frames,
        if p.width_frames == 1 { "" } else { "s" },
        compact_number(p.realized_width_seconds() * 1000.0),
        p.sample_rate.hz()
    )
}

pub(crate) fn pulse_brief(p: PulseFacts) -> String {
    format!(
        "{} repetitions/s · nominal {} ms between onsets · {} ms open",
        compact_number(p.rate_hz),
        compact_number(1000.0 / p.rate_hz),
        compact_number(p.realized_width_seconds() * 1000.0)
    )
}

/// Requested and realized carrier and modulation quantities. Brief views

/// state realized values; detail states both, with the rate.

pub(crate) fn generator_description(facts: &VoiceFacts, detail: bool) -> String {
    let phase = |p: PhaseRate| {
        if detail && p.requested_hz != p.realized_hz {
            format!(
                "requested {} Hz; realized {} Hz",
                p.requested_hz, p.realized_hz
            )
        } else {
            format!("{} Hz", compact_number(p.realized_hz))
        }
    };
    let pulse = |p: PulseFacts| {
        if detail {
            pulse_detail(p)
        } else {
            pulse_brief(p)
        }
    };
    // The carrier's name opens the text alone and is lowercase after "carrier".
    let (opening, inner, value) = match facts.carrier {
        CarrierFacts::Tone(rate) => (
            auditorylab_core::CarrierKind::Tone.label(),
            auditorylab_core::CarrierKind::Tone.phrase(),
            phase(rate),
        ),
        CarrierFacts::Clicks(p) => ("Pulse rate", "pulse rate", pulse(p)),
    };
    match facts.modulation {
        None => format!("{opening} {value}"),
        Some(ModulationFacts::Am { rate, depth }) if depth > 0.0 => format!(
            "Modulation {} · depth {}% · carrier {inner} {value}",
            phase(rate),
            compact_number(depth * 100.0)
        ),
        Some(ModulationFacts::Am { .. }) => {
            format!("{opening} {value} · Modulation inactive (depth 0%)")
        }
        Some(ModulationFacts::Gate(p)) => format!("Gate {} · carrier {inner} {value}", pulse(p)),
    }
}

pub(crate) const fn route(ears: Ears) -> &'static str {
    ears.label()
}

/// A validated voice's quantities, route and peak bound. `detail` adds

/// requested values and the rate they were resolved at.

#[must_use]
pub(crate) fn voice_description(facts: &VoiceFacts, detail: bool) -> String {
    let generated = format!(
        "{} · {} · voice peak bound {} dBFS",
        generator_description(facts, detail),
        route(facts.ears),
        format_dbfs(facts.peak_dbfs)
    );
    if detail && let Some(pair) = facts.pair {
        format!(
            "Authored pair {} · nominal signed offset {} Hz · requested carriers {} / {} Hz · {generated}",
            pair.owner, pair.offset_hz, pair.first_hz, pair.second_hz
        )
    } else {
        generated
    }
}

pub(crate) fn gain_route(gains: [f64; 2]) -> &'static str {
    match gains {
        [l, r] if l > 0.0 && r > 0.0 => Ears::Both.label(),
        [l, _] if l > 0.0 => Ears::Left.label(),
        [_, r] if r > 0.0 => Ears::Right.label(),
        _ => "Zero gain",
    }
}

pub(crate) fn observation_description(voice: &VoiceObservation) -> String {
    let db = |gain: f64| {
        if gain == 0.0 {
            f64::NEG_INFINITY
        } else {
            20.0 * gain.log10()
        }
    };
    format!(
        "{} · {} · gain L {} / R {} dBFS",
        generator_description(&voice.facts, false),
        gain_route(voice.gains),
        format_dbfs(db(voice.gains[0])),
        format_dbfs(db(voice.gains[1]))
    )
}

/// Every generated voice with its role and actual per-ear gains.

#[must_use]
pub(crate) fn generated_details(observation: &Observation) -> String {
    observation
        .voices
        .iter()
        .flatten()
        .map(|v| {
            format!(
                "{} · {}: {} · gains L {} / R {}",
                crate::errors::acoustic::voice_reference(v.id),
                v.role.label(),
                observation_description(v),
                v.gains[0],
                v.gains[1]
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The physical structure in one line; no global frequency is guessed for

/// independent processes and no perceptual effect is claimed.

#[must_use]
pub(crate) fn structure_description(structure: &Structure) -> String {
    let relation = structure
        .pairs
        .first()
        .filter(|_| structure.voices.len() == 2)
        .map(|pair| match pair.kind {
            auditorylab_core::PairKind::Binaural if pair.first == pair.second => format!(
                "Unison · Left/right {} Hz steady tones",
                compact_number(pair.first)
            ),
            auditorylab_core::PairKind::Binaural => format!(
                "{} Hz interaural difference · Left {} / Right {} Hz steady tones",
                compact_number((pair.first - pair.second).abs()),
                compact_number(pair.first),
                compact_number(pair.second)
            ),
            auditorylab_core::PairKind::Monaural if pair.first == pair.second => {
                format!("Unison · {} Hz steady tones", compact_number(pair.first))
            }
            auditorylab_core::PairKind::Monaural => format!(
                "{} Hz monaural beat · {} + {} Hz steady tones",
                compact_number((pair.first - pair.second).abs()),
                compact_number(pair.first),
                compact_number(pair.second)
            ),
        });
    let mut parts = Vec::new();
    for r in &structure.rhythms {
        let detail = structure
            .voices
            .iter()
            .find(|v| v.id == r.owner && v.role == r.role)
            .map(|v| match (r.method, v.facts.carrier, v.facts.modulation) {
                (Method::Am, _, Some(ModulationFacts::Am { depth, .. })) => {
                    format!(" · depth {}%", compact_number(depth * 100.0))
                }
                (Method::Gate, _, Some(ModulationFacts::Gate(p)))
                | (Method::Clicks, CarrierFacts::Clicks(p), _) => {
                    format!(
                        " · {} ms",
                        compact_number(p.realized_width_seconds() * 1000.0)
                    )
                }
                _ => String::new(),
            })
            .unwrap_or_default();
        let role = if structure.changing {
            match r.role {
                auditorylab_core::SlotRole::Outgoing => "Outgoing ",
                auditorylab_core::SlotRole::Incoming => "Incoming ",
                auditorylab_core::SlotRole::Continuing => "",
            }
        } else {
            ""
        };
        parts.push(format!(
            "{role}{} Hz {}{detail}",
            compact_number(r.hz),
            r.method.phrase()
        ));
    }
    for v in &structure.voices {
        if let CarrierFacts::Tone(rate) = v.facts.carrier {
            let steady = match v.facts.modulation {
                None => true,
                Some(ModulationFacts::Am { depth, .. }) => depth == 0.0,
                Some(ModulationFacts::Gate(_)) => false,
            };
            parts.push(format!(
                "{} Hz {}tone",
                compact_number(rate.realized_hz),
                if steady { "steady " } else { "carrier " }
            ));
        }
    }
    let text = relation.unwrap_or_else(|| {
        if parts.is_empty() {
            "Silence · no active generators".into()
        } else if parts.len() > 4 {
            format!(
                "{} voices · {} + {} more processes (voice details)",
                structure.voices.len(),
                parts[..4].join(" + "),
                parts.len() - 4
            )
        } else {
            parts.join(" + ")
        }
    });
    let routes: Vec<_> = structure
        .voices
        .iter()
        .map(|v| gain_route(v.gains))
        .collect();
    let route = routes
        .first()
        .filter(|first| routes.iter().all(|r| r == *first))
        .map(|s| format!(" · {s}"))
        .unwrap_or_default();
    format!(
        "{}{text}{route}",
        if structure.changing {
            "Changing… · "
        } else {
            ""
        }
    )
}

/// Nominal reference memberships and exact attributable rates; no EEG claim.

#[must_use]
pub(crate) fn nominal_description(analysis: &auditorylab_core::rhythm::Analysis) -> String {
    let mut labels: Vec<String> = analysis.bands.iter().map(ToString::to_string).collect();
    if analysis.outside_bands() {
        labels.push(
            auditorylab_app::discovery::query::RhythmState::OutsideBands
                .label()
                .into(),
        );
    }
    if analysis.no_rhythm() {
        labels.push(
            auditorylab_app::discovery::query::RhythmState::None
                .label()
                .into(),
        );
    }
    if analysis.multiple() {
        labels.push(
            auditorylab_app::discovery::query::RhythmState::Multiple
                .label()
                .into(),
        );
    }
    if analysis.completeness != auditorylab_core::rhythm::Completeness::Complete {
        labels.push(
            auditorylab_app::discovery::query::RhythmState::NeedsReview
                .label()
                .into(),
        );
    }
    let rates = analysis
        .rhythms
        .iter()
        .map(|r| format!("{} Hz {}", r.hz, r.method.phrase()))
        .collect::<Vec<_>>()
        .join(" + ");
    if rates.is_empty() {
        labels.join(" · ")
    } else {
        format!("{} · {rates}", labels.join(" · "))
    }
}

/// Short display descriptions of acoustic facts and structures.

/// Complete display facts, including rates, attribution and generated gains.

/// Application text for editable core quantities.

/// Application context attached to a rate-dependent acoustic validation failure.

/// Escape stored content only at a read-only display boundary.

pub fn voice_summary(value: &VoiceFacts) -> String {
    voice_description(value, false)
}

pub fn voice_details(value: &VoiceFacts) -> String {
    voice_description(value, true)
}

pub fn structure_summary(value: &Structure) -> String {
    structure_description(value)
}

pub fn observation_summary(value: &Observation) -> String {
    crate::structure_summary(&(auditorylab_core::rhythm::realized(value)))
}

pub fn observation_details(value: &Observation) -> String {
    generated_details(value)
}

pub fn optional_observation_summary(value: &Option<&Observation>) -> String {
    value.map_or_else(
        || {
            auditorylab_app::vocabulary::Status::WaitingForGeneratedAudio
                .label()
                .into()
        },
        crate::observation_summary,
    )
}

pub fn analysis_summary(value: &auditorylab_core::rhythm::Analysis) -> String {
    nominal_description(value)
}

pub fn parameter_help(value: ParameterId) -> &'static str {
    match value {
        ParameterId::ToneFrequency => "How many cycles per second the sine tone has.",
        ParameterId::ClickRate => {
            "How many clicks start each second; every click starts at its exact scheduled time."
        }
        ParameterId::ClickWidth => {
            "How long each click lasts; generation rounds it to whole frames at the sample rate."
        }
        ParameterId::PeakLevel => {
            "The component's highest level relative to digital full scale; 0 dBFS is the maximum."
        }
        ParameterId::AmRate => {
            "How many times each second the sinusoidal envelope rises and falls."
        }
        ParameterId::AmDepth => {
            "How far the envelope dips: 0% leaves the level steady and 100% reaches silence."
        }
        ParameterId::GateRate => "How many times each second the gate opens.",
        ParameterId::GateWidth => {
            "How long each gate stays open; generation rounds it to whole frames at the sample rate."
        }
        ParameterId::PairFirstFrequency => {
            "The first carrier; changing it also changes the second carrier through the signed offset."
        }
        ParameterId::PairOffset => {
            "Second carrier minus first carrier; its absolute value is the nominal pair rhythm."
        }
        ParameterId::PairFirstLevel => "Peak bound of the first or left tone.",
        ParameterId::PairSecondLevel => "Peak bound of the second or right tone.",
        ParameterId::Duration => "The finite sound duration.",
        ParameterId::Fade => "The raised-cosine fade at each edge of a finite sound.",
    }
}

/// A quantity's label with its unit, as on an input.

pub fn parameter_input_label(value: ParameterId) -> String {
    let id = value;
    let descriptor = id.descriptor();
    format!("{} ({})", descriptor.label, descriptor.unit.symbol())
}

/// Render a typed intrinsic rhythm-analysis diagnostic.

#[must_use]
pub fn rhythm_diagnostic(issue: &auditorylab_core::rhythm::Issue) -> String {
    use auditorylab_core::{ValueRule, rhythm::Issue};
    match issue {
        Issue::Pair(error) => crate::errors::core::pair_error(error),
        Issue::Value { owner, issue } => {
            let parameter = issue.parameter.descriptor();
            format!(
                "{}: {} {} {} must be {}",
                crate::errors::acoustic::voice_reference(*owner),
                parameter.label,
                compact_number(parameter.unit.to_display(issue.value)),
                parameter.unit.symbol(),
                match issue.rule {
                    ValueRule::Positive => "finite and positive",
                    ValueRule::Peak => "at most 0 dBFS",
                    ValueRule::Depth => "between 0% and 100%",
                }
            )
        }
    }
}

#[cfg(test)]
pub(crate) const fn finite(value: f64) -> auditorylab_core::Finite {
    match auditorylab_core::Finite::new(value) {
        Some(value) => value,
        None => panic!("nonfinite fixture value"),
    }
}

#[cfg(test)]
mod tests {
    use auditorylab_core::{SampleRate, Sound};
    #[test]
    fn capacity_text_names_current_counts_and_the_exceeded_budget() {
        use auditorylab_app::document::editing::EditRefusal;
        for (components, voices, additional_voices, limit) in [
            (8, 16, 1, "the 16-voice limit"),
            (8, 16, 2, "the 16-voice limit"),
            (16, 16, 1, "the 16-component and 16-voice limits"),
            (15, 15, 2, "the 16-voice limit"),
        ] {
            let text = crate::edit_refusal_description(EditRefusal::BudgetExceeded {
                components,
                voices,
                additional_voices,
            });
            assert!(
                text.starts_with(&format!(
                    "This sound has {components} authored components and {voices} expanded voices."
                )),
                "{text}"
            );
            assert!(
                text.contains(&format!("requires {additional_voices} more ")),
                "{text}"
            );
            assert!(text.contains(&format!("exceed {limit}.")), "{text}");
            assert!(text.ends_with("Each pair uses two voices."), "{text}");
        }
    }

    #[test]
    fn expanded_pair_structure_counts_voices_instead_of_authored_components() {
        use auditorylab_app::workspace::controls;
        use auditorylab_core::{Component, ComponentId, PairKind};
        let sound = Sound::new(
            finite(1.0),
            finite(0.0),
            (1..=8)
                .map(|id| Component {
                    id: ComponentId::new(id).unwrap(),
                    form: controls::default_pair(PairKind::Binaural),
                })
                .collect(),
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
        let target = sound.target(SampleRate::DEFAULT).unwrap();
        let text = structure_description(&auditorylab_core::rhythm::realized(&target));
        assert_eq!(sound.components().len(), 8);
        assert_eq!(target.len(), 16);
        assert!(text.starts_with("16 voices · "), "{text}");
        assert!(!text.contains("16 components"), "{text}");
    }

    #[test]
    fn axis_limits_keep_three_significant_digits() {
        assert_eq!(crate::numbers::format(NumberStyle::AxisLimit, 1.0), "1");
        assert_eq!(
            crate::numbers::format(NumberStyle::AxisLimit, 0.031_622_7),
            "0.0316"
        );
        assert_eq!(crate::numbers::format(NumberStyle::AxisLimit, 0.5), "0.5");
        assert_eq!(
            crate::numbers::format(NumberStyle::AxisLimit, 0.000_123_456),
            "0.000123"
        );
    }

    #[test]
    fn the_transport_clock_counts_whole_seconds_minutes_and_hours() {
        for (seconds, text) in [
            (0.0, "0:00"),
            (0.037, "0:00"),
            (9.99, "0:09"),
            (10.0, "0:10"),
            (300.0, "5:00"),
            (3599.9, "59:59"),
            (3600.0, "1:00:00"),
            (93_784.0, "26:03:04"),
            (-1.0, "0:00"),
        ] {
            assert_eq!(
                crate::numbers::format(NumberStyle::Clock, seconds),
                text,
                "{seconds}"
            );
        }
    }

    use super::*;

    #[test]
    fn durations_keep_the_published_carry_tiny_and_invalid_cases() {
        for (seconds, expected) in [
            (0.0, "0 s"),
            (45.0, "45 s"),
            (60.0, "1 min"),
            (3600.0, "1 h"),
            (5400.0, "1 h 30 min"),
            (307.2, "5 min 7.2 s"),
            (59.9999999, "59.9999999 s"),
            (3599.9999999, "59 min 59.9999999 s"),
            (59.9999999999, "~1 min"),
            (3599.9999999999, "~1 h"),
            (1.0 / 768000.0, "~0.000001302 s"),
            (1e-12, "1e-12 s"),
            (1e300, "1e300 s"),
            (-1.0, "-1 s"),
            (f64::NAN, "NaN s"),
            (f64::INFINITY, "inf s"),
            (f64::NEG_INFINITY, "-inf s"),
            (18446744073709551616.0, "1.8446744073709552e19 s"),
            (68_719_476_736.0, "19088743 h 32 min 16 s"),
        ] {
            assert_eq!(
                crate::numbers::format(NumberStyle::Duration, seconds),
                expected,
                "{seconds:?}"
            );
        }
        assert_eq!(
            crate::numbers::format(NumberStyle::Decibels, -19.999999999999996),
            "-20.0"
        );
        assert_eq!(
            crate::numbers::format(NumberStyle::Decibels, f64::NEG_INFINITY),
            "−∞"
        );
    }

    #[test]
    fn editable_numbers_enter_the_same_model_value_again() {
        // 29 % is 0.29, whose naive display is 28.999999999999996.
        assert_eq!(0.29 * 100.0, 28.999999999999996);
        assert_eq!(
            auditorylab_app::document::input::parameter_text(ParameterId::AmDepth, 0.29),
            "29"
        );
        assert_eq!(
            auditorylab_app::document::input::parameter_text(ParameterId::ClickWidth, 0.0011),
            "1.1"
        );
        assert_eq!(
            auditorylab_app::document::input::parameter_text(
                ParameterId::ToneFrequency,
                39.0625123456789
            ),
            "39.0625123456789"
        );
        assert_eq!(
            auditorylab_app::document::input::parameter_text(ParameterId::PeakLevel, -30.0),
            "-30"
        );
        assert_eq!(
            auditorylab_app::document::input::parameter_text(ParameterId::Duration, f64::NAN),
            "NaN"
        );
        // Every value entered as displayed text is shown so that it enters the
        // same model value again.
        for id in ParameterId::ALL {
            assert!(crate::parameter_help(id).ends_with('.'));
            for typed in [
                "29",
                "1.1",
                "0.001",
                "123456.789",
                "-3",
                "0.3333333333333333",
                "1e-9",
            ] {
                let model = id.descriptor().unit.to_model(typed.parse().unwrap());
                let text = auditorylab_app::document::input::parameter_text(id, model);
                let back = id.descriptor().unit.to_model(text.parse().unwrap());
                assert_eq!(back.to_bits(), model.to_bits(), "{id:?} {typed} {text}");
            }
        }
        assert_eq!(
            crate::parameter_input_label(ParameterId::ClickWidth),
            "Clicks width (ms)"
        );
    }

    #[test]
    fn compact_rounding_is_explicit() {
        assert_eq!(
            crate::numbers::format(NumberStyle::Compact, 39.0625),
            "39.0625"
        );
        assert_eq!(
            crate::numbers::format(NumberStyle::Compact, 39.0625123456789),
            "≈39.0625"
        );
        assert_eq!(
            crate::numbers::format(NumberStyle::Compact, 100000.0),
            "100000"
        );
        assert!(!crate::numbers::format(NumberStyle::Compact, f64::from_bits(1)).starts_with('0'));
    }
}
