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
    let mut left = None;
    let mut right = None;
    let mut count = 0;
    for voice in observation
        .voices
        .iter()
        .flatten()
        .filter(|v| v.gains.iter().any(|g| *g > 0.0))
    {
        count += 1;
        let crate::CarrierFacts::Tone(rate) = voice.facts.carrier else {
            return None;
        };
        if voice.facts.modulation.is_some() {
            return None;
        }
        match voice.gains {
            [l, 0.0] if l > 0.0 && left.is_none() => left = Some(rate.realized_hz),
            [0.0, r] if r > 0.0 && right.is_none() => right = Some(rate.realized_hz),
            _ => return None,
        }
    }
    if count != 2 {
        return None;
    }
    let (left_hz, right_hz) = (left?, right?);
    Some(InterauralPair {
        left_hz,
        right_hz,
        difference_hz: (left_hz - right_hz).abs(),
    })
}
/// Concise Hz value, preserving positive rates too small for ordinary decimal rounding.
pub fn format_hz(value: f64) -> String {
    if value != 0.0 && value.abs() < 0.000001 {
        format!("{value:e}")
    } else {
        format!("{value:.6}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
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
            format!("{} Hz", format_hz(p.realized_hz))
        }
    };
    let pulse = |p: crate::PulseFacts| {
        format!(
            "{} Hz · {} ms ({} frames)",
            format_hz(p.rate_hz),
            p.width_seconds * 1000.0,
            p.width_frames
        )
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
    let active: Vec<_> = observation
        .voices
        .iter()
        .flatten()
        .filter(|v| v.gains.iter().any(|g| *g > 0.0))
        .collect();
    let heading = if let Some(pair) = interaural_pair(observation) {
        format!(
            "Interaural difference: {} Hz · Left tone {} Hz · Right tone {} Hz",
            format_hz(pair.difference_hz),
            format_hz(pair.left_hz),
            format_hz(pair.right_hz)
        )
    } else if active.is_empty() {
        "Silence · no active generators".into()
    } else if active.len() > 1 {
        format!("Multiple components · {} active generators", active.len())
    } else {
        voice_description(active[0])
    };
    let transition = if observation.transitioning {
        "Transitioning · "
    } else {
        ""
    };
    format!("{transition}{heading}")
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
    let observation = sound
        .signal()
        .observation(sound.signal().fade_frames())
        .or_else(|| sound.signal().observation(0));
    let mut text = generated_description(observation.as_ref());
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
    if let Some(observation) = sound.signal().observation(sound.signal().fade_frames())
        && let Some(pair) = interaural_pair(&observation)
    {
        return format!(
            "{} Hz left · {} Hz right",
            format_hz(pair.left_hz),
            format_hz(pair.right_hz)
        );
    }
    let facts = sound.signal().facts();
    if facts.is_empty() {
        return "Silence · no components".into();
    }
    if facts.len() != 1 {
        return format!("Multiple components ({})", facts.len());
    }
    let f = facts[0];
    let carrier = match f.carrier {
        crate::CarrierFacts::Tone(rate) => format!("Tone {} Hz", format_hz(rate.realized_hz)),
        crate::CarrierFacts::Clicks(p) => format!(
            "Clicks {} Hz · {} ms",
            format_hz(p.rate_hz),
            p.width_seconds * 1000.0
        ),
    };
    let description = match f.modulation {
        Some(crate::ModulationFacts::Sinusoidal { rate, depth }) if depth > 0.0 => {
            format!("AM {} Hz · {carrier}", format_hz(rate.realized_hz))
        }
        Some(crate::ModulationFacts::Gate(p)) => {
            format!("Gate {} Hz · {carrier}", format_hz(p.rate_hz))
        }
        _ => carrier,
    };
    format!(
        "{description} · {}",
        match f.ears {
            crate::Ears::Left => "Left",
            crate::Ears::Right => "Right",
            crate::Ears::Both => "Both ears",
        }
    )
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
        .then(|| format!("Window shorter than one {} Hz cycle", format_hz(rate)))
}
