//! Shared wording: numbers, durations, diagnostics and sound descriptions.
//!
//! Frontends render these strings; they do not compose domain sentences of
//! their own. Generated and model views describe realized quantities; detail
//! views state requested and realized values together with their rate.
use crate::{RecipeSnapshot, Sound};
use auditorylab_core::{
    CarrierFacts, ComponentFacts, Ears, ModulationFacts, Observation, ParameterId, PhaseRate,
    PulseFacts, SampleRate, Stimulus, StimulusError, VoiceObservation,
    interpretation::{RhythmKind, Structure, ToneRelationship},
};

/// Format seconds as hours, minutes and seconds; `~` marks nanosecond rounding.
#[must_use]
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

/// A transport clock of whole seconds: `m:ss`, or `h:mm:ss` from one hour.
/// Partial seconds are dropped, so the clock advances once per second.
#[must_use]
pub fn clock(seconds: f64) -> String {
    let whole = std::time::Duration::try_from_secs_f64(seconds.max(0.0))
        .map_or(u64::MAX, |time| time.as_secs());
    let (hours, minutes, seconds) = (whole / 3600, (whole % 3600) / 60, whole % 60);
    if hours == 0 {
        format!("{minutes}:{seconds:02}")
    } else {
        format!("{hours}:{minutes:02}:{seconds:02}")
    }
}

/// Estimated output elapsed time as a transport clock.
#[must_use]
pub fn playback_elapsed(rate: SampleRate, frames: u64) -> String {
    clock(rate.seconds_in(frames))
}

/// The labels the play/stop transport control can show, in no particular
/// order; a frontend can size the control for the widest.
pub const TRANSPORT_LABELS: [&str; 3] = [PLAY_SELECTED, PLAY_LIVE, STOP_AUDIO];
pub(crate) const PLAY_SELECTED: &str = "Play selected sound";
pub(crate) const PLAY_LIVE: &str = "Play live";
pub(crate) const STOP_AUDIO: &str = "Stop audio";

/// One-decimal digital level; silence uses the minus-infinity symbol.
#[must_use]
pub fn format_dbfs(value: f64) -> String {
    if value == f64::NEG_INFINITY {
        "−∞".into()
    } else {
        format!("{value:.1}")
    }
}

/// Compact six-significant-digit number; approximation is explicit and
/// positive tiny values never become zero.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "The decimal count is clamped to the finite integer range 0..=12 before casting to usize; rounding formats presentation text only"
)]
#[must_use]
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

/// The shortest text, in the quantity's displayed unit, that enters exactly
/// the given model value again. Entered values are never rounded.
#[must_use]
pub fn editable_number(id: ParameterId, model: f64) -> String {
    let unit = id.descriptor().unit;
    let display = unit.to_display(model);
    if !display.is_finite() {
        return display.to_string();
    }
    for digits in 1..=17 {
        let text = format!("{display:.*e}", digits - 1);
        if let Ok(value) = text.parse::<f64>()
            && unit.to_model(value).to_bits() == model.to_bits()
        {
            return value.to_string();
        }
    }
    display.to_string()
}

/// A quantity's label with its unit, as on an input.
#[must_use]
pub fn labeled(id: ParameterId) -> String {
    let descriptor = id.descriptor();
    format!("{} ({})", descriptor.label, descriptor.unit.symbol())
}

/// A source that is not valid at a rate.
#[must_use]
pub fn invalid_at_rate(title: &str, rate: SampleRate, error: &StimulusError) -> String {
    format!("{title} at {} Hz: {error}", rate.hz())
}

fn pulse_detail(p: PulseFacts) -> String {
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

fn pulse_brief(p: PulseFacts) -> String {
    format!(
        "{} repetitions/s · nominal {} ms between onsets · {} ms open",
        compact_number(p.rate_hz),
        compact_number(1000.0 / p.rate_hz),
        compact_number(p.realized_width_seconds() * 1000.0)
    )
}

/// Requested and realized carrier and modulation quantities. Brief views
/// state realized values; detail states both, with the rate.
fn generator_description(facts: &ComponentFacts, detail: bool) -> String {
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
    let carrier = match facts.carrier {
        CarrierFacts::Tone(rate) => format!("Tone {}", phase(rate)),
        CarrierFacts::Clicks(p) => format!("Pulse rate {}", pulse(p)),
    };
    match facts.modulation {
        None => carrier,
        Some(ModulationFacts::Sinusoidal { rate, depth }) if depth > 0.0 => format!(
            "Modulation {} · depth {}% · carrier {carrier}",
            phase(rate),
            compact_number(depth * 100.0)
        ),
        Some(ModulationFacts::Sinusoidal { .. }) => {
            format!("{carrier} · Modulation inactive (depth 0%)")
        }
        Some(ModulationFacts::Gate(p)) => format!("Gate {} · carrier {carrier}", pulse(p)),
    }
}

const fn route(ears: Ears) -> &'static str {
    ears.label()
}

/// A validated component's quantities, route and peak bound. `detail` adds
/// requested values and the rate they were resolved at.
#[must_use]
pub fn component_description(facts: &ComponentFacts, detail: bool) -> String {
    format!(
        "{} · {} · component peak bound {} dBFS",
        generator_description(facts, detail),
        route(facts.ears),
        format_dbfs(facts.peak_dbfs)
    )
}

fn gain_route(gains: [f64; 2]) -> &'static str {
    match gains {
        [l, r] if l > 0.0 && r > 0.0 => Ears::Both.label(),
        [l, _] if l > 0.0 => Ears::Left.label(),
        [_, r] if r > 0.0 => Ears::Right.label(),
        _ => "Zero gain",
    }
}

fn voice_description(voice: &VoiceObservation) -> String {
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

/// The last generated frame's structure, keeping transition voices.
#[must_use]
pub fn generated_description(observation: Option<&Observation>) -> String {
    observation.map_or_else(
        || "Waiting for generated audio".into(),
        |observation| structure_description(&Structure::of_observation(observation)),
    )
}

/// Every generated voice with its role and actual per-ear gains.
#[must_use]
pub fn generated_details(observation: &Observation) -> String {
    observation
        .voices
        .iter()
        .flatten()
        .map(|v| {
            format!(
                "Component {} · {}: {} · gains L {} / R {}",
                v.id,
                v.role.label(),
                voice_description(v),
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
pub fn structure_description(structure: &Structure) -> String {
    let relation = match structure.relationship {
        Some(ToneRelationship::Interaural { left, right }) => Some(if left == right {
            format!(
                "Unison · Left/right {} Hz steady tones",
                compact_number(left)
            )
        } else {
            format!(
                "{} Hz interaural difference · Left {} / Right {} Hz steady tones",
                compact_number((left - right).abs()),
                compact_number(left),
                compact_number(right)
            )
        }),
        Some(ToneRelationship::Monaural { first, second }) => Some(if first == second {
            format!("Unison · {} Hz steady tones", compact_number(first))
        } else {
            format!(
                "{} Hz monaural beat · {} + {} Hz steady tones",
                compact_number((first - second).abs()),
                compact_number(first),
                compact_number(second)
            )
        }),
        None => None,
    };
    let mut parts = Vec::new();
    for r in &structure.rhythms {
        let detail = structure
            .voices
            .iter()
            .find(|v| v.id == r.id && v.role == r.role)
            .map(|v| match (r.kind, v.facts.carrier, v.facts.modulation) {
                (
                    RhythmKind::AmplitudeModulation,
                    _,
                    Some(ModulationFacts::Sinusoidal { depth, .. }),
                ) => {
                    format!(" · depth {}%", compact_number(depth * 100.0))
                }
                (RhythmKind::Gate, _, Some(ModulationFacts::Gate(p)))
                | (RhythmKind::Clicks, CarrierFacts::Clicks(p), _) => {
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
            r.kind.label()
        ));
    }
    for v in &structure.voices {
        if let CarrierFacts::Tone(rate) = v.facts.carrier {
            let steady = match v.facts.modulation {
                None => true,
                Some(ModulationFacts::Sinusoidal { depth, .. }) => depth == 0.0,
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
                "{} components · {} + {} more processes (component details)",
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

/// A realized sound's structure in one line.
#[must_use]
pub fn sound_brief(sound: &Sound) -> String {
    structure_description(&Structure::of_target(sound.signal().target()))
}

/// Facts of a realized sound at its own rate, every line at that rate.
/// `detail` adds the model caption and requested-versus-realized values.
#[must_use]
pub fn sound_description(sound: &Sound, detail: bool) -> String {
    let mut text = sound_brief(sound);
    if detail {
        let model = crate::ModelView::new(
            *sound.signal().target(),
            sound.snapshot().title(),
            crate::ModelSource::Recipe,
        );
        text.push_str(&format!("\n{}", model.caption()));
    }
    for (id, facts) in sound.signal().components() {
        text.push_str(&format!(
            "\nComponent {id}: {}",
            component_description(facts, detail)
        ));
    }
    text
}

/// The research text of a snapshot in its shared display order.
#[must_use]
pub fn research(snapshot: &RecipeSnapshot) -> Vec<(&'static str, &str)> {
    snapshot.about().map_or_else(Vec::new, |a| {
        [
            ("Studied for", &a.studied_for),
            ("Summary", &a.summary),
            ("Citation", &a.citation),
            ("Source", &a.source_url),
            ("Study", &a.study),
            ("Adaptation", &a.adaptation),
        ]
        .into_iter()
        .filter_map(|(label, value)| value.as_deref().map(|text| (label, text)))
        .collect()
    })
}

/// Shared listening guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// Start quietly.
    LowStartingVolume,
    /// Use separate ear channels.
    HeadphonesForSeparateEars,
    /// Reported buffer underruns.
    Underruns(u64),
}

impl std::fmt::Display for Notice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LowStartingVolume => f.write_str("Turn the volume down before playing."),
            Self::HeadphonesForSeparateEars => {
                f.write_str("Use headphones for sounds that differ between the ears.")
            }
            Self::Underruns(n) => write!(f, "The audio backend reported {n} buffer underruns."),
        }
    }
}

/// Guidance shown before playing a stimulus.
#[must_use]
pub fn notices(stimulus: &Stimulus) -> Vec<Notice> {
    let mut notices = vec![Notice::LowStartingVolume];
    if stimulus.uses_separate_ears() {
        notices.push(Notice::HeadphonesForSeparateEars);
    }
    notices
}

/// A library row: title, structure at `rate`, what it was studied for and
/// its duration.
#[must_use]
pub fn library_label(snapshot: &RecipeSnapshot, rate: SampleRate) -> String {
    let structure = snapshot.at(rate).map_or_else(
        |_| format!("Not valid at {} Hz; see details", rate.hz()),
        |sound| sound_brief(&sound),
    );
    format!(
        "{}{}\n{}\n{} · {}",
        if matches!(snapshot.origin(), crate::Origin::File(_)) {
            "Imported · "
        } else {
            ""
        },
        snapshot.title(),
        structure,
        snapshot
            .about()
            .and_then(|a| a.studied_for.as_deref())
            .unwrap_or("Custom recipe"),
        human_duration(snapshot.stimulus().duration_seconds)
    )
}

/// A snapshot's research text, components and validated facts at one
/// explicit inspection rate.
#[must_use]
pub fn recipe_details(snapshot: &RecipeSnapshot, rate: SampleRate) -> String {
    let mut text = format!("Source: {}\n\n", snapshot.origin());
    for (name, value) in research(snapshot) {
        text.push_str(&format!("{name}: {value}\n\n"));
    }
    for (i, c) in snapshot.stimulus().components.iter().enumerate() {
        text.push_str(&format!("Component {}: {c}\n", i + 1));
    }
    match snapshot.at(rate) {
        Ok(sound) => {
            let s = sound.signal();
            text.push_str(&sound_description(&sound, true));
            text.push_str(&format!(
                "\n{} Hz · {} frames · {}\nFade: {} frames\nPeak bounds: {} / {} dBFS",
                rate.hz(),
                s.frames(),
                human_duration(s.duration_seconds()),
                s.fade_frames(),
                format_dbfs(s.peak_dbfs()[0]),
                format_dbfs(s.peak_dbfs()[1])
            ));
        }
        Err(e) => text.push_str(&e.to_string()),
    }
    text
}

/// A recipe's title, source, timing, research text and description at the
/// default rate, labelled with that rate.
#[must_use]
pub fn describe(snapshot: &RecipeSnapshot) -> String {
    let s = snapshot.stimulus();
    let mut out = format!(
        "{}\nSource: {}\nDuration: {}\nFade: {} at each edge\n",
        snapshot.title(),
        snapshot.origin(),
        human_duration(s.duration_seconds),
        human_duration(s.fade_seconds)
    );
    for (label, value) in research(snapshot) {
        out.push_str(&format!("{label}: {value}\n"));
    }
    let rate = SampleRate::DEFAULT;
    match snapshot.at(rate) {
        Ok(sound) => out.push_str(&format!(
            "Description at {} Hz:\n{}\n",
            rate.hz(),
            sound_description(&sound, false)
        )),
        Err(error) => {
            for (i, component) in s.components.iter().enumerate() {
                out.push_str(&format!("Component {}: {component}\n", i + 1));
            }
            out.push_str(&format!("{error}\n"));
        }
    }
    out
}

/// A realized sound's facts, every one at the sound's own rate.
#[must_use]
pub fn inspection(sound: &Sound) -> String {
    let snapshot = sound.snapshot();
    let signal = sound.signal();
    let mut out = format!("{}\nSource: {}\n", snapshot.title(), snapshot.origin());
    for (label, value) in research(snapshot) {
        out.push_str(&format!("{label}: {value}\n"));
    }
    out.push_str(&format!(
        "Sample rate: {} Hz\nFrames: {}\nDuration: {}\nFade: {} frames ({} s)\nPeak bounds: left {} dBFS, right {} dBFS\n{}",
        signal.sample_rate().hz(),
        signal.frames(),
        human_duration(signal.duration_seconds()),
        signal.fade_frames(),
        signal.fade_seconds(),
        format_dbfs(signal.peak_dbfs()[0]),
        format_dbfs(signal.peak_dbfs()[1]),
        sound_description(sound, true)
    ));
    out
}

/// The built-in recipes, one paragraph each: ID, title, duration and what
/// they were studied for.
#[must_use]
pub fn catalog_listing(snapshots: &[RecipeSnapshot]) -> String {
    let mut text = String::new();
    for snapshot in snapshots {
        if let crate::Origin::BuiltIn(id) = snapshot.origin() {
            text.push_str(&format!(
                "{id}\n  {} ({})\n",
                snapshot.title(),
                human_duration(snapshot.stimulus().duration_seconds)
            ));
            if let Some(a) = snapshot.about().and_then(|a| a.studied_for.as_deref()) {
                text.push_str(&format!("  {a}\n"));
            }
        }
    }
    text
}

/// What is about to play and on which output, with listening guidance.
#[must_use]
pub fn playback_preamble(
    snapshot: &RecipeSnapshot,
    output: &crate::OutputInfo,
    signal: &auditorylab_core::Signal,
) -> String {
    format!(
        "Preparing {}\nSource: {}\nDuration: {}\nDevice: {}\n{} Hz, {} channels, {}\n{}",
        snapshot.title(),
        snapshot.origin(),
        human_duration(signal.duration_seconds()),
        output.name.as_deref().unwrap_or("default output"),
        output.sample_rate.hz(),
        output.channels,
        output.format,
        notices(snapshot.stimulus())
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// One status line for a playing session.
#[must_use]
pub fn playback_status(reader: &crate::MonitorReader) -> String {
    let status = reader.status();
    format!(
        "{} · {} · {} · {}",
        playback_elapsed(
            reader.descriptor().output.sample_rate,
            status.estimated_output_frames
        ),
        crate::phase_text(status.phase),
        match reader.freshness() {
            crate::ObservationFreshness::Waiting => "waiting for generated audio",
            crate::ObservationFreshness::Current => "current",
            crate::ObservationFreshness::Stale => "stale",
            crate::ObservationFreshness::Unavailable => "monitoring unavailable",
        },
        generated_description(status.observation.as_ref())
    )
}

/// The result of a playback, for any frontend.
#[must_use]
pub fn playback_result(report: &crate::PlaybackReport) -> String {
    let mut text = format!(
        "{} {} frames generated; about {} output.",
        match report.end {
            crate::EndReason::Completed => "Played to the end.",
            crate::EndReason::Stopped if report.drained => "Stopped.",
            crate::EndReason::Stopped => "Stopped; queued output was cut off after the stop limit.",
            crate::EndReason::FrameLimit => "Reached the longest supported session.",
        },
        report.generated_frames,
        report.estimated_output_frames
    );
    if report.underruns > 0 {
        text.push('\n');
        text.push_str(&Notice::Underruns(report.underruns).to_string());
    }
    text
}

/// A save completed for an older snapshot while newer edits remain.
pub(crate) const SAVED_SNAPSHOT_STILL_DIRTY: &str =
    "Saved snapshot; newer edits remain unsaved. The sound remains open.";
/// A worker thread panicked.
pub(crate) const WORKER_FAILED: &str = "The operation thread failed.";
/// A worker thread ended without reporting a result.
pub(crate) const WORKER_ENDED_WITHOUT_RESULT: &str = "The operation thread ended without a result.";
/// An export ended at the person's request.
pub(crate) const EXPORT_CANCELLED: &str = "Export cancelled; no file was written.";

/// A monitored window that cannot be drawn truthfully.
#[must_use]
pub fn capture_error_text(error: crate::CaptureError) -> String {
    match error {
        crate::CaptureError::NonFiniteSample => {
            "Invalid waveform capture (non-finite samples); retaining the last valid window".into()
        }
        crate::CaptureError::InvalidSpan => {
            "Invalid waveform capture (inconsistent span); retaining the last valid window".into()
        }
    }
}

/// Explain when the selected observation window cannot show a complete
/// cycle of the slowest active rhythm.
#[must_use]
pub fn window_cycle_hint(observation: Option<&Observation>, window_ms: f64) -> Option<String> {
    let rate = Structure::of_observation(observation?).slowest_rate()?;
    (window_ms * rate < 1000.0)
        .then(|| format!("Window shorter than one {} Hz cycle", compact_number(rate)))
}

#[cfg(test)]
mod tests {
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
            assert_eq!(super::clock(seconds), text, "{seconds}");
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
            assert_eq!(human_duration(seconds), expected, "{seconds:?}");
        }
        assert_eq!(format_dbfs(-19.999999999999996), "-20.0");
        assert_eq!(format_dbfs(f64::NEG_INFINITY), "−∞");
    }

    #[test]
    fn editable_numbers_enter_the_same_model_value_again() {
        // 29 % is 0.29, whose naive display is 28.999999999999996.
        assert_eq!(0.29 * 100.0, 28.999999999999996);
        assert_eq!(editable_number(ParameterId::ModulationDepth, 0.29), "29");
        assert_eq!(editable_number(ParameterId::ClickWidth, 0.0011), "1.1");
        assert_eq!(
            editable_number(ParameterId::ToneFrequency, 39.0625123456789),
            "39.0625123456789"
        );
        assert_eq!(editable_number(ParameterId::PeakLevel, -30.0), "-30");
        assert_eq!(editable_number(ParameterId::Duration, f64::NAN), "NaN");
        // Every value entered as displayed text is shown so that it enters the
        // same model value again.
        for id in ParameterId::ALL {
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
                let text = editable_number(id, model);
                let back = id.descriptor().unit.to_model(text.parse().unwrap());
                assert_eq!(back.to_bits(), model.to_bits(), "{id:?} {typed} {text}");
            }
        }
        assert_eq!(labeled(ParameterId::ClickWidth), "Click width (ms)");
    }

    #[test]
    fn compact_rounding_is_explicit() {
        assert_eq!(compact_number(39.0625), "39.0625");
        assert_eq!(compact_number(39.0625123456789), "≈39.0625");
        assert_eq!(compact_number(100000.0), "100000");
        assert!(!compact_number(f64::from_bits(1)).starts_with('0'));
    }
}
