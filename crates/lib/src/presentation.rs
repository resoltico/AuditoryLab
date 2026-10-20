//! Shared human-readable sound descriptions.
/// Format seconds as hours, minutes and seconds; `~` marks nanosecond rounding.
pub fn human_duration(value: f64) -> String {
    fn scalar(value: f64) -> String {
        if value.is_finite() && value != 0.0 && (value.abs() >= 1e12 || value.abs() < 1e-6) {
            format!("{value:e}")
        } else {
            value.to_string()
        }
    }
    let Ok(time) = std::time::Duration::try_from_secs_f64(value) else {
        return format!("{} s", scalar(value));
    };
    if value > 0.0 && time.is_zero() {
        return format!("{} s", scalar(value));
    }
    let mut parts = Vec::new();
    let secs = time.as_secs();
    if secs / 3600 != 0 {
        parts.push(format!("{} h", secs / 3600));
    }
    if (secs % 3600) / 60 != 0 {
        parts.push(format!("{} min", (secs % 3600) / 60));
    }
    if secs % 60 != 0 || time.subsec_nanos() != 0 || parts.is_empty() {
        let fraction = if time.subsec_nanos() == 0 {
            String::new()
        } else {
            format!(".{:09}", time.subsec_nanos())
                .trim_end_matches('0')
                .into()
        };
        parts.push(format!("{}{fraction} s", secs % 60));
    }
    format!(
        "{}{}",
        if time.as_secs_f64() == value { "" } else { "~" },
        parts.join(" ")
    )
}
/// Whole-second estimated output elapsed time shared by both frontends.
pub fn playback_elapsed(rate: crate::SampleRate, frames: u64) -> String {
    {
        let seconds = rate.seconds_in(frames);
        human_duration(if seconds < 1.0 {
            seconds
        } else {
            seconds.floor()
        })
    }
}
/// One-decimal digital level; silence uses the minus-infinity symbol.
pub fn format_dbfs(value: f64) -> String {
    if value == f64::NEG_INFINITY {
        "−∞".into()
    } else {
        format!("{value:.1}")
    }
}
#[cfg(test)]
mod tests {
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
            assert_eq!(human_duration(seconds), expected, "{seconds:?}");
        }
        assert_eq!(format_dbfs(-19.999999999999996), "-20.0");
        assert_eq!(format_dbfs(f64::NEG_INFINITY), "−∞");
    }
}

/// Narrow two-tone structure; no claim of perceived beat or biological response.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InterauralPair {
    /// Generated left tone frequency.
    pub left_hz: f64,
    /// Generated right tone frequency.
    pub right_hz: f64,
    /// Absolute frequency difference.
    pub difference_hz: f64,
}
/// Interpret exactly one unmodulated tone exclusively in each ear, with no other active voice.
pub fn interaural_pair(observation: &crate::Observation) -> Option<InterauralPair> {
    let Some(crate::ToneRelationship::Interaural { left, right }) =
        crate::SoundSummary::generated(observation).relationship
    else {
        return None;
    };
    Some(InterauralPair {
        left_hz: left,
        right_hz: right,
        difference_hz: (left - right).abs(),
    })
}
/// Compact six-significant-digit number; approximation is explicit and positive tiny values never become zero.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "quality:lib-presentation-compact-number"
)]
pub fn compact_number(value: f64) -> String {
    if !value.is_finite() || value == 0.0 {
        return value.to_string();
    }
    let decimals = (5.0 - value.abs().log10().floor()).clamp(0.0, 12.0) as usize;
    let text = if value.abs() < 1e-6 || value.abs() >= 1e6 {
        format!("{value:.5e}")
    } else if decimals == 0 {
        format!("{value:.0}")
    } else {
        format!("{value:.decimals$}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    };
    let rounded = text.parse::<f64>().unwrap_or(value);
    format!(
        "{}{text}",
        if (rounded - value).abs() > value.abs() * 1e-12 {
            "≈"
        } else {
            ""
        }
    )
}
/// Shared requested/realized carrier, modulation, route and width description.
fn generator_description(facts: &crate::ComponentFacts, exact: bool) -> String {
    let phase = |p: crate::PhaseRate| {
        if exact && p.requested_hz != p.realized_hz {
            format!(
                "requested {} Hz; realized {} Hz",
                p.requested_hz, p.realized_hz
            )
        } else {
            format!("{} Hz", compact_number(p.realized_hz))
        }
    };
    let pulse = |p: crate::PulseFacts| {
        if exact {
            format!(
                "{} Hz · requested {} ms · realized {} frames",
                p.rate_hz,
                p.width_seconds * 1000.0,
                p.width_frames
            )
        } else {
            format!(
                "{} repetitions/s · nominal {} ms between onsets · {} ms open",
                compact_number(p.rate_hz),
                compact_number(1000.0 / p.rate_hz),
                compact_number(p.width_seconds * 1000.0)
            )
        }
    };
    let carrier = match facts.carrier {
        crate::CarrierFacts::Tone(rate) => format!("Tone {}", phase(rate)),
        crate::CarrierFacts::Clicks(p) => format!("Pulse rate {}", pulse(p)),
    };
    match facts.modulation {
        None => carrier,
        Some(crate::ModulationFacts::Sinusoidal { rate, depth }) if depth > 0.0 => format!(
            "Modulation {} · depth {}% · carrier {carrier}",
            phase(rate),
            depth * 100.0
        ),
        Some(crate::ModulationFacts::Sinusoidal { .. }) => {
            format!("{carrier} · Modulation inactive (depth 0%)")
        }
        Some(crate::ModulationFacts::Gate(p)) => format!("Gate {} · carrier {carrier}", pulse(p)),
    }
}

/// Shared requested/realized carrier, modulation, route and width description.
pub fn component_description(facts: &crate::ComponentFacts, exact: bool) -> String {
    format!(
        "{} · {} · component peak bound {} dBFS",
        generator_description(facts, exact),
        match facts.ears {
            crate::Ears::Both => "Both ears",
            crate::Ears::Left => "Left",
            crate::Ears::Right => "Right",
        },
        format_dbfs(facts.peak_dbfs)
    )
}

fn voice_description(voice: &crate::VoiceObservation) -> String {
    let route = match voice.gains {
        [l, r] if l > 0.0 && r > 0.0 => "Both ears",
        [l, _] if l > 0.0 => "Left",
        [_, r] if r > 0.0 => "Right",
        _ => "Zero gain",
    };
    let db = |gain: f64| {
        if gain == 0.0 {
            f64::NEG_INFINITY
        } else {
            20.0 * gain.log10()
        }
    };
    format!(
        "{} · {route} · gain L {} / R {} dBFS",
        generator_description(&voice.facts, false),
        format_dbfs(db(voice.gains[0])),
        format_dbfs(db(voice.gains[1]))
    )
}

/// Summarize the last generated frame, preserving structural transition voices.
pub fn generated_description(observation: Option<&crate::Observation>) -> String {
    let Some(observation) = observation else {
        return "Waiting for generated audio".into();
    };
    crate::SoundSummary::generated(observation).description()
}
/// Full generated voice list, including role and actual per-ear gains.
pub fn generated_details(observation: &crate::Observation) -> String {
    observation
        .voices
        .iter()
        .flatten()
        .map(|v| {
            format!(
                "Component {} · {:?}: {} · gains L {} / R {}",
                v.id,
                v.role,
                voice_description(v),
                v.gains[0],
                v.gains[1]
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
/// Preview/inspection facts shared by the CLI and GUI.
pub fn sound_description(sound: &crate::Sound, exact: bool) -> String {
    let mut text = sound_brief(sound);
    if exact && let Ok(target) = crate::recipe_target(sound.preset(), sound.signal().sample_rate())
    {
        text.push_str(&format!(
            "\n{}",
            crate::ModelView::new(target, sound.preset().title(), crate::ModelSource::Recipe)
                .caption()
        ));
    }
    for (i, facts) in sound.signal().facts().iter().enumerate() {
        text.push_str(&format!(
            "\nComponent {}: {}",
            i + 1,
            component_description(facts, exact)
        ));
    }
    text
}
/// Compact catalog structure, with full generator quantities available in the detail description.
pub fn sound_brief(sound: &crate::Sound) -> String {
    crate::recipe_target(sound.preset(), sound.signal().sample_rate())
        .map(|t| crate::SoundSummary::target(&t).description())
        .unwrap_or_else(|e| e.to_string())
}

/// Search source identity, title, research text and component meaning together.
pub fn catalog_matches(preset: &crate::Preset, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let text = format!(
        "{} {} {} {}",
        preset.origin(),
        preset.title(),
        preset
            .research()
            .iter()
            .map(|(_, v)| *v)
            .collect::<Vec<_>>()
            .join(" "),
        preset
            .stimulus()
            .components
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    );
    text.to_lowercase().contains(&query)
}
/// Explain when the selected observation window cannot show a complete primary rhythm cycle.
pub fn window_cycle_hint(
    observation: Option<&crate::Observation>,
    window_ms: f64,
) -> Option<String> {
    let observation = observation?;
    let mut rates = Vec::new();
    if let Some(pair) = interaural_pair(observation)
        && pair.difference_hz > 0.0
    {
        rates.push(pair.difference_hz);
    }
    for v in observation
        .voices
        .iter()
        .flatten()
        .filter(|v| v.gains.iter().any(|g| *g > 0.0))
    {
        match v.facts.modulation {
            Some(crate::ModulationFacts::Sinusoidal { rate, depth }) if depth > 0.0 => {
                rates.push(rate.realized_hz)
            }
            Some(crate::ModulationFacts::Gate(p)) => rates.push(p.rate_hz),
            _ => {}
        }
        if let crate::CarrierFacts::Clicks(p) = v.facts.carrier {
            rates.push(p.rate_hz);
        }
    }
    let rate = rates.into_iter().reduce(f64::min)?;
    (window_ms * rate < 1000.0)
        .then(|| format!("Window shorter than one {} Hz cycle", compact_number(rate)))
}
