//! Shared wording: numbers, durations, diagnostics and sound descriptions.
//!
//! Frontends render these strings; they do not compose domain sentences of
//! their own. Generated and model views describe realized quantities; detail
//! views state requested and realized values together with their rate.
mod commands;
use crate::recipe::{RecipeSnapshot, Rendition};
use crate::vocabulary::{NumberStyle, Status, Term};
use auditorylab_core::{
    CarrierFacts, ComponentFacts, Ears, ModulationFacts, Observation, ParameterId, PhaseRate,
    PulseFacts, SampleRate, Sound, SoundError, VoiceObservation,
    interpretation::{Structure, ToneRelationship},
    nominal::Method,
};
pub use commands::DesktopDiagnostics;

impl crate::editing::EditRefusal {
    /// Explain why an editor command was refused without confusing authored components and voices.
    #[must_use]
    pub fn description(self) -> String {
        use crate::editing::EditRefusal;
        match self {
            EditRefusal::BudgetExceeded {
                components,
                voices,
                additional_voices,
            } => {
                let limit = match (
                    components >= Sound::MAX_COMPONENTS,
                    voices + additional_voices > Sound::MAX_VOICES,
                ) {
                    (true, true) => format!(
                        "the {}-component and {}-voice limits",
                        Sound::MAX_COMPONENTS,
                        Sound::MAX_VOICES
                    ),
                    (true, false) => format!("the {}-component limit", Sound::MAX_COMPONENTS),
                    _ => format!("the {}-voice limit", Sound::MAX_VOICES),
                };
                format!(
                    "This sound has {components} authored components and {voices} expanded voices. Adding this component requires {additional_voices} more {}; it would exceed {limit}. Each pair uses two voices.",
                    if additional_voices == 1 {
                        "voice"
                    } else {
                        "voices"
                    }
                )
            }
            EditRefusal::NothingRemoved => "No removed component is waiting to be restored.".into(),
            EditRefusal::IdentitiesExhausted => {
                "This sound has exhausted an editing identity; start a new sound to continue."
                    .into()
            }
            EditRefusal::StaleDocument
            | EditRefusal::StaleRow
            | EditRefusal::UnknownComponent
            | EditRefusal::Inapplicable => "That field no longer belongs to the open sound.".into(),
        }
    }
}

/// Format seconds as hours, minutes and seconds; `~` marks nanosecond rounding.
#[must_use]
fn human_duration(value: f64) -> String {
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
fn clock(seconds: f64) -> String {
    let whole = std::time::Duration::try_from_secs_f64(seconds.max(0.0))
        .map_or(u64::MAX, |time| time.as_secs());
    let (hours, minutes, seconds) = (whole / 3600, (whole % 3600) / 60, whole % 60);
    if hours == 0 {
        format!("{minutes}:{seconds:02}")
    } else {
        format!("{hours}:{minutes:02}:{seconds:02}")
    }
}

/// A waveform axis limit with three significant digits, for tick labels.
#[must_use]
fn axis_limit(limit: f64) -> String {
    if !limit.is_finite() || limit <= 0.0 {
        return compact_number(limit);
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "A positive finite magnitude's decimal exponent is clamped to 0..=12 before use as a digit count"
    )]
    let decimals = (2.0 - limit.log10().floor()).clamp(0.0, 12.0) as usize;
    let text = format!("{limit:.decimals$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// One-decimal digital level; silence uses the minus-infinity symbol.
#[must_use]
fn format_dbfs(value: f64) -> String {
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
fn compact_number(value: f64) -> String {
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
    // The carrier's name opens the text alone and is lowercase after "carrier".
    let (opening, inner, value) = match facts.carrier {
        CarrierFacts::Tone(rate) => ("Tone", "tone", phase(rate)),
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

const fn route(ears: Ears) -> &'static str {
    ears.label()
}

/// A validated component's quantities, route and peak bound. `detail` adds
/// requested values and the rate they were resolved at.
#[must_use]
fn component_description(facts: &ComponentFacts, detail: bool) -> String {
    let generated = format!(
        "{} · {} · component peak bound {} dBFS",
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

/// Every generated voice with its role and actual per-ear gains.
#[must_use]
fn generated_details(observation: &Observation) -> String {
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
fn structure_description(structure: &Structure) -> String {
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
            r.kind.label()
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

fn document_notes(snapshot: &RecipeSnapshot) -> String {
    let document = &snapshot.recipe().document;
    let mut text = format!(
        "Document: {}\nCreated (declared): {}\nEdited (declared): {}\n",
        document.id,
        document
            .created_at
            .map_or_else(|| Status::Unknown.label().into(), |t| t.to_string()),
        document
            .modified_at
            .map_or_else(|| Status::Unknown.label().into(), |t| t.to_string())
    );
    if let Some(notes) = snapshot
        .about()
        .and_then(|a| a.summary.as_deref())
        .filter(|s| !s.is_empty())
    {
        text.push_str(&format!("Sound notes: {notes}\n"));
    }
    for ancestor in &snapshot.recipe().lineage {
        text.push_str(&format!("Based on: {}\nAncestor note: {}\nResearch findings describe the ancestor, not this sound.\n",ancestor.title,ancestor.note));
    }
    for source in &snapshot.recipe().sources {
        text.push_str(&format!(
            "Source {}: {}\nSource citation: {}\n",
            source.key, source.label, source.citation
        ));
        if let Some(url) = &source.url {
            text.push_str(&format!("Source URL: {url}\n"));
        }
        if let Some(doi) = &source.doi {
            text.push_str(&format!("Source DOI: {doi}\n"));
        }
    }
    for evidence in &snapshot.recipe().evidence {
        if let Some(detail) = &evidence.population_detail {
            text.push_str(&format!("Population detail: {detail}\n"));
        }
        text.push_str(&format!(
            "Measures: {}\n",
            evidence
                .measures
                .iter()
                .map(Term::label)
                .collect::<Vec<_>>()
                .join(", ")
        ));
        text.push_str(&format!("Evidence {} · source {} · population {} · role {}\nSummary: {}\nLimitations: {}\nMethods: {}\nAdaptation: {}\n",evidence.scope.label(),evidence.source,evidence.population.label(),evidence.condition_role.label(),evidence.summary,evidence.limitations,evidence.methods_locator,evidence.adaptation));
    }
    text
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

/// A save completed for an older snapshot while newer edits remain.
pub(crate) const SAVED_SNAPSHOT_STILL_DIRTY: &str =
    "Saved snapshot; newer edits remain unsaved. The sound remains open.";
/// A worker thread panicked.
pub(crate) const WORKER_FAILED: &str = "The operation thread failed.";
/// A worker thread ended without reporting a result.
/// An export ended at the person's request.
pub(crate) const EXPORT_CANCELLED: &str = "Export cancelled; no file was written.";

/// Nominal reference memberships and exact attributable rates; no EEG claim.
#[must_use]
fn nominal_description(analysis: &crate::core::nominal::Analysis) -> String {
    let mut labels: Vec<String> = analysis.bands.iter().map(ToString::to_string).collect();
    if analysis.outside_bands() {
        labels.push(Status::OtherRates.label().into());
    }
    if analysis.no_rhythm() {
        labels.push(Status::NoRhythmicRate.label().into());
    }
    if analysis.multiple() {
        labels.push(Status::MultipleRhythms.label().into());
    }
    if analysis.completeness != crate::core::nominal::Completeness::Complete {
        labels.push(Status::NeedsReview.label().into());
    }
    let rates = analysis
        .rhythms
        .iter()
        .map(|r| format!("{} Hz {}", r.hz, r.method.key()))
        .collect::<Vec<_>>()
        .join(" + ");
    if rates.is_empty() {
        labels.join(" · ")
    } else {
        format!("{} · {rates}", labels.join(" · "))
    }
}

/// Short display descriptions of acoustic facts and structures.
pub trait AudioSummary {
    /// A concise description of the represented audio state.
    #[must_use]
    fn summary(&self) -> String;
}
/// Complete display facts, including rates, attribution and generated gains.
pub trait AudioDetails {
    /// Complete display detail for the represented audio state.
    #[must_use]
    fn details(&self) -> String;
}
/// Application text for editable core quantities.
pub trait ParameterText {
    /// Exact round-tripping text in the parameter's displayed unit.
    #[must_use]
    fn input_text(self, model: f64) -> String;
    /// Field label and displayed unit.
    #[must_use]
    fn input_label(self) -> String;
}
/// Application context attached to a rate-dependent acoustic validation failure.
pub trait AudioErrorText {
    /// Name the selected sound and the rate at which validation failed.
    #[must_use]
    fn at_rate(&self, title: &str, rate: SampleRate) -> String;
}
/// Escape stored content only at a read-only display boundary.
pub trait DisplayText {
    /// Preserve paragraphs/tabs while escaping controls and bidi overrides.
    #[must_use]
    fn display_text(&self) -> String;
    /// Single-line display of a title, path or other stored scalar.
    #[must_use]
    fn display_scalar(&self) -> String;
}
impl AudioSummary for ComponentFacts {
    fn summary(&self) -> String {
        component_description(self, false)
    }
}
impl AudioDetails for ComponentFacts {
    fn details(&self) -> String {
        component_description(self, true)
    }
}
impl AudioSummary for Structure {
    fn summary(&self) -> String {
        structure_description(self)
    }
}
impl AudioSummary for Observation {
    fn summary(&self) -> String {
        Structure::of_observation(self).summary()
    }
}
impl AudioDetails for Observation {
    fn details(&self) -> String {
        generated_details(self)
    }
}
impl AudioSummary for Option<&Observation> {
    fn summary(&self) -> String {
        self.map_or_else(
            || {
                crate::vocabulary::Status::WaitingForGeneratedAudio
                    .label()
                    .into()
            },
            AudioSummary::summary,
        )
    }
}
impl AudioSummary for crate::core::nominal::Analysis {
    fn summary(&self) -> String {
        nominal_description(self)
    }
}
impl crate::vocabulary::NumberStyle {
    /// Format a numeric value for its specific display responsibility.
    #[must_use]
    pub fn format(self, value: f64) -> String {
        match self {
            Self::Duration => human_duration(value),
            Self::Clock => clock(value),
            Self::AxisLimit => axis_limit(value),
            Self::Decibels => format_dbfs(value),
            Self::Compact => compact_number(value),
            Self::CapturedMilliseconds => format!("{value:.2} ms"),
            Self::Milliseconds => format!("{value} ms"),
            Self::Hertz => format!("{value} Hz"),
        }
    }
}
impl crate::session::OutputInfo {
    /// Estimated output frame count as a transport clock at this output's rate.
    #[must_use]
    pub fn elapsed(&self, frames: u64) -> String {
        clock(self.sample_rate.seconds_in(frames))
    }
}

impl ParameterText for ParameterId {
    /// The shortest text, in the quantity's displayed unit, that enters exactly
    /// the given model value again. Entered values are never rounded.
    fn input_text(self, model: f64) -> String {
        let id = self;
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
    fn input_label(self) -> String {
        let id = self;
        let descriptor = id.descriptor();
        format!("{} ({})", descriptor.label, descriptor.unit.symbol())
    }
}
impl AudioErrorText for SoundError {
    /// A source that is not valid at a rate.
    fn at_rate(&self, title: &str, rate: SampleRate) -> String {
        let error = self;
        format!("{title} at {} Hz: {error}", rate.hz())
    }
}
impl Rendition {
    /// A realized sound's structure in one line.
    #[must_use]
    pub fn brief(&self) -> String {
        let sound = self;
        structure_description(&Structure::of_target(sound.signal().target()))
    }
    /// Facts of a realized sound at its own rate, every line at that rate.
    /// `detail` adds the model caption and requested-versus-realized values.
    #[must_use]
    pub fn description(&self, detail: bool) -> String {
        let sound = self;
        let mut text = (sound).brief();
        if detail {
            let model = crate::waveform_view::ModelView::new(
                *sound.signal().target(),
                sound.snapshot().title(),
                crate::waveform_view::ModelSource::Recipe,
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
    /// A realized sound's facts, every one at the sound's own rate.
    #[must_use]
    pub fn inspection(&self) -> String {
        let sound = self;
        let snapshot = sound.snapshot();
        let signal = sound.signal();
        let mut out = format!("{}\nSource: {}\n", snapshot.title(), snapshot.origin());
        out.push_str(&document_notes(snapshot));
        for (label, value) in (snapshot).research() {
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
        (sound).description(true)
    ));
        out
    }
}
impl RecipeSnapshot {
    /// The research text of a snapshot in its shared display order.
    #[must_use]
    pub fn research(&self) -> Vec<(&'static str, &str)> {
        let snapshot = self;
        snapshot
            .about()
            .and_then(|a| a.research.as_ref())
            .map_or_else(Vec::new, |a| {
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
    /// A library row using cached nominal classification, without target compilation.
    #[must_use]
    pub fn library_label(&self) -> String {
        let snapshot = self;
        let structure = nominal_description(snapshot.nominal());
        format!(
            "{}{}{}\n{}\n{} · {}",
            if matches!(snapshot.origin(), crate::recipe::Origin::File(_)) {
                "Imported · "
            } else {
                ""
            },
            snapshot.title().display_scalar(),
            if matches!(snapshot.origin(), crate::recipe::Origin::BuiltIn(_)) {
                " · Built-in · Read only"
            } else {
                ""
            },
            structure,
            snapshot
                .about()
                .and_then(|a| a.research.as_ref())
                .and_then(|a| a.studied_for.as_deref())
                .unwrap_or("Custom recipe"),
            human_duration(snapshot.sound().duration_seconds)
        )
    }
    /// A snapshot's research text, components and validated facts at one
    /// explicit inspection rate.
    #[must_use]
    pub fn details_at(&self, rate: SampleRate) -> String {
        let snapshot = self;
        let mut text = format!("Source: {}\n\n", snapshot.origin());
        text.push_str(&document_notes(snapshot));
        for (name, value) in (snapshot).research() {
            text.push_str(&format!("{name}: {value}\n\n"));
        }
        for (i, c) in snapshot.sound().components.iter().enumerate() {
            text.push_str(&format!("Component {}: {c}\n", i + 1));
        }
        match snapshot.at(rate) {
            Ok(sound) => {
                let s = sound.signal();
                text.push_str(&sound.description(true));
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
    pub fn description(&self) -> String {
        let snapshot = self;
        let s = snapshot.sound();
        let mut out = format!(
            "{}\nSource: {}\nDuration: {}\nFade: {} at each edge\n",
            snapshot.title(),
            snapshot.origin(),
            human_duration(s.duration_seconds),
            human_duration(s.fade_seconds)
        );
        out.push_str(&document_notes(snapshot));
        for warning in crate::library::history_warnings(snapshot.recipe()) {
            out.push_str(&format!("{warning}\n"));
        }
        for (label, value) in (snapshot).research() {
            out.push_str(&format!("{label}: {value}\n"));
        }
        let rate = SampleRate::DEFAULT;
        match snapshot.at(rate) {
            Ok(sound) => out.push_str(&format!(
                "Description at {} Hz:\n{}\n",
                rate.hz(),
                sound.description(false)
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
    /// What is about to play and on which output, with listening guidance.
    #[must_use]
    pub fn playback_preamble(
        &self,
        output: &crate::session::OutputInfo,
        signal: &auditorylab_core::Signal,
    ) -> String {
        let snapshot = self;
        format!(
            "Preparing {}\nSource: {}\nDuration: {}\nDevice: {}\n{} Hz, {} channels, {}\n{}",
            snapshot.title(),
            snapshot.origin(),
            human_duration(signal.duration_seconds()),
            output.name.as_deref().unwrap_or("default output"),
            output.sample_rate.hz(),
            output.channels,
            output.format,
            Notice::for_sound(snapshot.sound())
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}
impl crate::session::MonitorReader {
    /// One status line for a playing session.
    #[must_use]
    pub fn status_text(&self) -> String {
        let reader = self;
        let status = reader.status();
        format!(
            "{} · {} · {} · {}",
            NumberStyle::Clock.format(
                (reader.descriptor().output.sample_rate).seconds_in(status.estimated_output_frames)
            ),
            crate::waveform_view::phase_text(status.phase),
            match reader.freshness() {
                crate::session::ObservationFreshness::Waiting => "waiting for generated audio",
                crate::session::ObservationFreshness::Current => "current",
                crate::session::ObservationFreshness::Stale => "stale",
            },
            (status.observation.as_ref()).summary()
        )
    }
}
impl crate::session::PlaybackReport {
    /// The result of a playback, for any frontend.
    #[must_use]
    pub fn description(&self) -> String {
        let report = self;
        let mut text = format!(
            "{} {} frames generated; about {} output.",
            match report.end {
                crate::session::EndReason::Completed => "Played to the end.",
                crate::session::EndReason::Stopped if report.drained => "Stopped.",
                crate::session::EndReason::Stopped =>
                    "Stopped; queued output was cut off after the stop limit.",
                crate::session::EndReason::FrameLimit => "Reached the longest supported session.",
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
}
impl crate::waveform_view::CaptureError {
    /// A monitored window that cannot be drawn truthfully.
    #[must_use]
    pub fn description(self) -> String {
        let error = self;
        match error {
            crate::waveform_view::CaptureError::NonFiniteSample => {
                "Invalid waveform capture (non-finite samples); retaining the last valid window"
                    .into()
            }
            crate::waveform_view::CaptureError::InvalidSpan => {
                "Invalid waveform capture (inconsistent span); retaining the last valid window"
                    .into()
            }
        }
    }
}
impl DisplayText for str {
    /// Escape controls and bidi overrides at a read-only display boundary. Ordinary
    /// Unicode text, paragraph newlines and tabs retain their meaning. Stored content
    /// and editable fields must continue to use the original text.
    fn display_text(&self) -> String {
        let text = self;
        let mut result = String::with_capacity(text.len());
        for c in text.chars() {
            if (c.is_control() && c != '\n' && c != '\t')
                || matches!(c,'\u{061c}'|'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
            {
                result.extend(c.escape_default());
            } else {
                result.push(c);
            }
        }
        result
    }
    /// Readable single-line title/path text, without changing its stored value.
    fn display_scalar(&self) -> String {
        let text = self;
        (text)
            .display_text()
            .replace('\n', "\\n")
            .replace('\t', "\\t")
    }
}
impl Notice {
    /// Persistent desktop listening guidance, independent of the current selection.
    #[must_use]
    pub fn general_guidance() -> String {
        format!(
            "{} {}",
            Self::LowStartingVolume,
            Self::HeadphonesForSeparateEars
        )
    }

    /// Guidance shown before playing a sound.
    #[must_use]
    pub fn for_sound(sound: &Sound) -> Vec<Notice> {
        let mut notices = vec![Notice::LowStartingVolume];
        if sound.uses_separate_ears() {
            notices.push(Notice::HeadphonesForSeparateEars);
        }
        notices
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn capacity_text_names_current_counts_and_the_exceeded_budget() {
        use crate::editing::EditRefusal;
        for (components, voices, additional_voices, limit) in [
            (8, 16, 1, "the 16-voice limit"),
            (8, 16, 2, "the 16-voice limit"),
            (16, 16, 1, "the 16-component and 16-voice limits"),
            (15, 15, 2, "the 16-voice limit"),
        ] {
            let text = EditRefusal::BudgetExceeded {
                components,
                voices,
                additional_voices,
            }
            .description();
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
        use crate::controls;
        use auditorylab_core::{Component, ComponentId, PairKind};
        let sound = Sound {
            duration_seconds: 1.0,
            fade_seconds: 0.0,
            components: (1..=8)
                .map(|id| Component {
                    id: ComponentId::new(id).unwrap(),
                    form: controls::default_pair(PairKind::Binaural),
                })
                .collect(),
        };
        let target = sound.target(SampleRate::DEFAULT).unwrap();
        let text = structure_description(&Structure::of_target(&target));
        assert_eq!(sound.components.len(), 8);
        assert_eq!(target.len(), 16);
        assert!(text.starts_with("16 voices · "), "{text}");
        assert!(!text.contains("16 components"), "{text}");
    }

    #[test]
    fn axis_limits_keep_three_significant_digits() {
        assert_eq!(NumberStyle::AxisLimit.format(1.0), "1");
        assert_eq!(NumberStyle::AxisLimit.format(0.031_622_7), "0.0316");
        assert_eq!(NumberStyle::AxisLimit.format(0.5), "0.5");
        assert_eq!(NumberStyle::AxisLimit.format(0.000_123_456), "0.000123");
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
            assert_eq!(NumberStyle::Clock.format(seconds), text, "{seconds}");
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
                NumberStyle::Duration.format(seconds),
                expected,
                "{seconds:?}"
            );
        }
        assert_eq!(NumberStyle::Decibels.format(-19.999999999999996), "-20.0");
        assert_eq!(NumberStyle::Decibels.format(f64::NEG_INFINITY), "−∞");
    }

    #[test]
    fn editable_numbers_enter_the_same_model_value_again() {
        // 29 % is 0.29, whose naive display is 28.999999999999996.
        assert_eq!(0.29 * 100.0, 28.999999999999996);
        assert_eq!((ParameterId::AmDepth).input_text(0.29), "29");
        assert_eq!((ParameterId::ClickWidth).input_text(0.0011), "1.1");
        assert_eq!(
            (ParameterId::ToneFrequency).input_text(39.0625123456789),
            "39.0625123456789"
        );
        assert_eq!((ParameterId::PeakLevel).input_text(-30.0), "-30");
        assert_eq!((ParameterId::Duration).input_text(f64::NAN), "NaN");
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
                let text = (id).input_text(model);
                let back = id.descriptor().unit.to_model(text.parse().unwrap());
                assert_eq!(back.to_bits(), model.to_bits(), "{id:?} {typed} {text}");
            }
        }
        assert_eq!((ParameterId::ClickWidth).input_label(), "Clicks width (ms)");
    }

    #[test]
    fn compact_rounding_is_explicit() {
        assert_eq!(NumberStyle::Compact.format(39.0625), "39.0625");
        assert_eq!(NumberStyle::Compact.format(39.0625123456789), "≈39.0625");
        assert_eq!(NumberStyle::Compact.format(100000.0), "100000");
        assert!(
            !NumberStyle::Compact
                .format(f64::from_bits(1))
                .starts_with('0')
        );
    }
}
