//! Editable recipe values, control ranges and publication.
use crate::{
    About, Carrier, Component, Ears, Error, ErrorKind, Modulation, Origin, Preset, Replace,
    SampleRate, Stimulus,
};
use auditorylab_core::live::Target;
use std::{io::Write, path::Path, sync::Arc};
/// Stable identity of a component within its draft.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ComponentId(u32);
impl ComponentId {
    /// Stable number for frontend row bindings; numbers are never reused.
    pub const fn get(self) -> u32 {
        self.0
    }
}
/// A carrier offered by the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// A sine tone.
    Tone,
    /// A train of rectangular clicks.
    Clicks,
}
/// A numeric component control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Setting {
    /// Tone frequency.
    Frequency,
    /// Click repetition rate.
    ClickRate,
    /// Click width, displayed in milliseconds.
    ClickWidth,
    /// Component peak level.
    Level,
    /// Sinusoidal modulation rate.
    ModulationRate,
    /// Sinusoidal modulation depth.
    Depth,
    /// Rectangular gate repetition rate.
    GateRate,
    /// Gate width, displayed in milliseconds.
    GateWidth,
}
impl Setting {
    /// Controls in frontend binding order.
    pub const ALL: [Self; 8] = [
        Self::Frequency,
        Self::ClickRate,
        Self::ClickWidth,
        Self::Level,
        Self::ModulationRate,
        Self::Depth,
        Self::GateRate,
        Self::GateWidth,
    ];
}
/// Mapping from a slider position to a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    /// Equal positions give equal value differences.
    Linear,
    /// Equal positions give equal value ratios.
    Logarithmic,
}
/// A comfortable slider span; typed values are validated by the core instead.
#[derive(Clone, Copy, Debug)]
pub struct Range {
    /// Slider's lower endpoint.
    pub min: f64,
    /// Slider's upper endpoint.
    pub max: f64,
    /// Initial value for this control.
    pub default: f64,
    /// Slider mapping.
    pub scale: Scale,
    /// Display unit; widths use milliseconds.
    pub unit: &'static str,
}
/// Slider ranges and defaults for a sample rate.
pub fn range(setting: Setting, rate: SampleRate) -> Range {
    use Scale::{Linear, Logarithmic};
    let (min, max, default, scale, unit) = match setting {
        Setting::Frequency => {
            let max = 20_000.0_f64.min(0.45 * f64::from(rate.hz()));
            let min = 20.0_f64.min(max / 2.0);
            (min, max, 440.0_f64.clamp(min, max), Logarithmic, "Hz")
        }
        Setting::ClickRate => (0.5, 1000.0, 40.0, Logarithmic, "Hz"),
        Setting::ClickWidth | Setting::GateWidth => (0.1, 50.0, 1.0, Logarithmic, "ms"),
        Setting::Level => (-80.0, 0.0, -30.0, Linear, "dBFS"),
        Setting::ModulationRate => (0.1, 100.0, 40.0, Logarithmic, "Hz"),
        Setting::Depth => (0.0, 100.0, 100.0, Linear, "%"),
        Setting::GateRate => (0.5, 100.0, 40.0, Logarithmic, "Hz"),
    };
    Range {
        min,
        max,
        default,
        scale,
        unit,
    }
}
/// A quiet unmodulated component, routed to both ears.
pub fn default_component(kind: SourceKind) -> Component {
    Component {
        ears: Ears::Both,
        peak_dbfs: -30.0,
        carrier: match kind {
            SourceKind::Tone => Carrier::Tone {
                frequency_hz: 440.0,
            },
            SourceKind::Clicks => Carrier::Click {
                rate_hz: 40.0,
                width_seconds: 0.001,
            },
        },
        modulation: None,
    }
}
/// Envelope kinds offered by component editors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModulationKind {
    /// Sinusoidal amplitude modulation.
    Sinusoidal,
    /// Rectangular gating.
    Gate,
}
/// Default parameters for an envelope kind, in the core model's units.
pub fn default_modulation(kind: ModulationKind, rate: SampleRate) -> Modulation {
    match kind {
        ModulationKind::Sinusoidal => Modulation::Sinusoidal {
            rate_hz: range(Setting::ModulationRate, rate).default,
            depth: range(Setting::Depth, rate).default / 100.0,
        },
        ModulationKind::Gate => Modulation::Rectangular {
            rate_hz: range(Setting::GateRate, rate).default,
            width_seconds: range(Setting::GateWidth, rate).default / 1000.0,
        },
    }
}
/// Read a control value in its display unit (widths in ms, depth in percent).
pub fn setting_value(c: &Component, s: Setting) -> Option<f64> {
    match (s, c.carrier, c.modulation) {
        (Setting::Level, _, _) => Some(c.peak_dbfs),
        (Setting::Frequency, Carrier::Tone { frequency_hz }, _) => Some(frequency_hz),
        (Setting::ClickRate, Carrier::Click { rate_hz, .. }, _) => Some(rate_hz),
        (Setting::ClickWidth, Carrier::Click { width_seconds, .. }, _) => {
            Some(width_seconds * 1000.0)
        }
        (Setting::ModulationRate, _, Some(Modulation::Sinusoidal { rate_hz, .. }))
        | (Setting::GateRate, _, Some(Modulation::Rectangular { rate_hz, .. })) => Some(rate_hz),
        (Setting::Depth, _, Some(Modulation::Sinusoidal { depth, .. })) => Some(depth * 100.0),
        (Setting::GateWidth, _, Some(Modulation::Rectangular { width_seconds, .. })) => {
            Some(width_seconds * 1000.0)
        }
        _ => None,
    }
}
/// Set a control value in its display unit; unrelated source/envelope shapes remain unchanged. Validation occurs when applying a whole target.
pub fn set_setting(c: &mut Component, s: Setting, value: f64) {
    match s {
        Setting::Level => c.peak_dbfs = value,
        Setting::Frequency => {
            if let Carrier::Tone { frequency_hz } = &mut c.carrier {
                *frequency_hz = value;
            }
        }
        Setting::ClickRate => {
            if let Carrier::Click { rate_hz, .. } = &mut c.carrier {
                *rate_hz = value;
            }
        }
        Setting::ClickWidth => {
            if let Carrier::Click { width_seconds, .. } = &mut c.carrier {
                *width_seconds = value / 1000.0;
            }
        }
        Setting::ModulationRate => {
            if let Some(Modulation::Sinusoidal { rate_hz, .. }) = &mut c.modulation {
                *rate_hz = value;
            }
        }
        Setting::Depth => {
            if let Some(Modulation::Sinusoidal { depth, .. }) = &mut c.modulation {
                *depth = value / 100.0;
            }
        }
        Setting::GateRate => {
            if let Some(Modulation::Rectangular { rate_hz, .. }) = &mut c.modulation {
                *rate_hz = value;
            }
        }
        Setting::GateWidth => {
            if let Some(Modulation::Rectangular { width_seconds, .. }) = &mut c.modulation {
                *width_seconds = value / 1000.0;
            }
        }
    }
}
/// Map a normalized slider position to the shared comfortable range; nonfinite input uses its default.
pub fn slider_value(setting: Setting, rate: SampleRate, position: f64) -> f64 {
    let r = range(setting, rate);
    if !position.is_finite() {
        return r.default;
    }
    match r.scale {
        Scale::Linear => r.min + (r.max - r.min) * position.clamp(0.0, 1.0),
        Scale::Logarithmic => r.min * (r.max / r.min).powf(position.clamp(0.0, 1.0)),
    }
}
/// Map a control value to a clamped slider position; invalid nonfinite values leave the knob at zero without changing the exact field.
pub fn slider_position(setting: Setting, rate: SampleRate, value: f64) -> f64 {
    let r = range(setting, rate);
    if !value.is_finite() {
        return 0.0;
    }
    match r.scale {
        Scale::Linear => (value - r.min) / (r.max - r.min),
        Scale::Logarithmic => (value.clamp(r.min, r.max) / r.min).ln() / (r.max / r.min).ln(),
    }
    .clamp(0.0, 1.0)
}
fn component_message(error: auditorylab_core::ComponentError) -> String {
    match error {
        auditorylab_core::ComponentError::Depth { depth } => format!(
            "Depth {}% must be finite and between 0 and 100%",
            depth * 100.0
        ),
        _ => error.to_string(),
    }
}
/// Row-specific messages and overall live validity.
pub struct DraftCheck {
    /// Each component's own error, independently of other rows.
    pub components: Vec<(ComponentId, Option<String>)>,
    /// Per-ear peak bounds, or an overall target error.
    pub sound: Result<[f64; 2], String>,
}
/// Editable sound parameters independent of a device rate.
#[derive(Clone, Debug)]
pub struct Draft {
    title: String,
    summary: String,
    duration: f64,
    fade: f64,
    components: Vec<(ComponentId, Component)>,
    next_id: u32,
    based_on: Option<Preset>,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            title: "Untitled sound".into(),
            summary: String::new(),
            duration: 60.0,
            fade: 0.1,
            components: Vec::new(),
            next_id: 0,
            based_on: None,
        }
    }
}
impl Draft {
    /// An empty, silent draft of sixty seconds with a 0.1-second export fade.
    pub fn new() -> Self {
        Self::default()
    }
    /// Copy sound parameters; retain attributed source context separately from blank custom notes.
    pub fn from_preset(preset: &Preset) -> Self {
        let mut draft = Self::new();
        draft.title = format!("{} (edited)", preset.title());
        draft.based_on = Some(preset.clone());
        draft.duration = preset.stimulus().duration_seconds;
        draft.fade = preset.stimulus().fade_seconds;
        for component in &preset.stimulus().components {
            draft
                .components
                .push((ComponentId(draft.next_id), *component));
            // Loaded recipes are bounded by MAX_BYTES, so their component count fits u32.
            draft.next_id += 1;
        }
        draft
    }
    /// Attributed source recipe, excluded from serialized custom recipe metadata.
    pub fn based_on(&self) -> Option<&Preset> {
        self.based_on.as_ref()
    }
    /// Compare saved content, excluding component identities and transient source context.
    pub fn same_content(&self, other: &Self) -> bool {
        self.title == other.title
            && self.summary == other.summary
            && self.duration.to_bits() == other.duration.to_bits()
            && self.fade.to_bits() == other.fade.to_bits()
            && self.components.len() == other.components.len()
            && self
                .components
                .iter()
                .zip(&other.components)
                .all(|((_, a), (_, b))| a == b)
    }
    /// Components and their stable identities in display order.
    pub fn components(&self) -> &[(ComponentId, Component)] {
        &self.components
    }
    /// Add a quiet component, refusing a seventeenth row or exhausted identities.
    pub fn add(&mut self, kind: SourceKind) -> Result<ComponentId, Error> {
        if self.components.len() >= Stimulus::MAX_COMPONENTS {
            return Err(Error::new(
                ErrorKind::InvalidSound,
                format!(
                    "a sound can contain at most {} components",
                    Stimulus::MAX_COMPONENTS
                ),
            ));
        }
        let next = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::InvalidSound, "component identities exhausted"))?;
        let id = ComponentId(self.next_id);
        self.next_id = next;
        self.components.push((id, default_component(kind)));
        Ok(id)
    }
    /// Replace an existing row; unknown identities leave the draft unchanged.
    pub fn replace(&mut self, id: ComponentId, component: Component) {
        if let Some((_, value)) = self.components.iter_mut().find(|(key, _)| *key == id) {
            *value = component;
        }
    }
    /// Remove a row without reusing its identity.
    pub fn remove(&mut self, id: ComponentId) {
        self.components.retain(|(key, _)| *key != id);
    }
    /// Editable title.
    pub fn title(&self) -> &str {
        &self.title
    }
    /// Set the title.
    pub fn set_title(&mut self, title: String) {
        self.title = title;
    }
    /// Editable summary.
    pub fn summary(&self) -> &str {
        &self.summary
    }
    /// Set the summary.
    pub fn set_summary(&mut self, summary: String) {
        self.summary = summary;
    }
    /// Duration used by recipe saving and WAV export.
    pub fn duration_seconds(&self) -> f64 {
        self.duration
    }
    /// Set the saved/exported duration without changing a running stream.
    pub fn set_duration_seconds(&mut self, seconds: f64) {
        self.duration = seconds;
    }
    /// Fade used by recipe saving and WAV export.
    pub fn fade_seconds(&self) -> f64 {
        self.fade
    }
    /// Set the saved/exported fade without changing live ramps.
    pub fn set_fade_seconds(&mut self, seconds: f64) {
        self.fade = seconds;
    }
    /// A recipe containing the current parameters and only title/summary text.
    pub fn preset(&self) -> Preset {
        Preset {
            origin: Origin::Draft(self.title.clone()),
            recipe: Arc::new(auditorylab_core::Preset {
                stimulus: Stimulus {
                    duration_seconds: self.duration,
                    fade_seconds: self.fade,
                    components: self.components.iter().map(|(_, c)| *c).collect(),
                },
                about: Some(About {
                    title: self.title.clone(),
                    summary: Some(self.summary.clone()),
                    studied_for: None,
                    citation: None,
                    source_url: None,
                    study: None,
                    adaptation: None,
                }),
            }),
        }
    }
    /// Validate every row and the total peak bound at a rate.
    pub fn check(&self, rate: SampleRate) -> DraftCheck {
        DraftCheck {
            components: self
                .components
                .iter()
                .map(|(id, c)| (*id, c.validate(rate).err().map(component_message)))
                .collect(),
            sound: self
                .target(rate)
                .map(|t| t.peak_dbfs())
                .map_err(|e| e.to_string()),
        }
    }
    /// Validate a complete live target at the stream's sample rate.
    pub fn target(&self, rate: SampleRate) -> Result<Target, Error> {
        let components: Vec<_> = self.components.iter().map(|(id, c)| (id.0, *c)).collect();
        Target::new(rate, &components).map_err(|e| {
            let message = match &e {
                auditorylab_core::StimulusError::Component { number, error } => {
                    format!("component {number}: {}", component_message(error.clone()))
                }
                _ => e.to_string(),
            };
            Error::caused(ErrorKind::InvalidSound, message, e)
        })
    }
    /// Save finite recipe numbers within the reader's byte limit, using staged publication.
    /// Rate-dependent invalid components may be saved for further editing.
    pub fn save(&self, path: &Path, replace: Replace) -> Result<(), Error> {
        let finite = self.duration.is_finite()
            && self.fade.is_finite()
            && self.components.iter().all(|(_, c)| {
                c.peak_dbfs.is_finite()
                    && match c.carrier {
                        Carrier::Tone { frequency_hz } => frequency_hz.is_finite(),
                        Carrier::Click {
                            rate_hz,
                            width_seconds,
                        } => rate_hz.is_finite() && width_seconds.is_finite(),
                    }
                    && c.modulation.is_none_or(|m| match m {
                        Modulation::Sinusoidal { rate_hz, depth } => {
                            rate_hz.is_finite() && depth.is_finite()
                        }
                        Modulation::Rectangular {
                            rate_hz,
                            width_seconds,
                        } => rate_hz.is_finite() && width_seconds.is_finite(),
                    })
            });
        if !finite {
            return Err(Error::new(
                ErrorKind::InvalidPreset,
                "recipe numbers must be finite",
            ));
        }
        let text = self
            .preset()
            .recipe
            .to_toml()
            .map_err(|e| Error::caused(ErrorKind::InvalidPreset, e.to_string(), e))?;
        if text.len() > auditorylab_core::Preset::MAX_BYTES {
            return Err(Error::new(
                ErrorKind::InvalidPreset,
                format!(
                    "recipe exceeds {} bytes",
                    auditorylab_core::Preset::MAX_BYTES
                ),
            ));
        }
        crate::publish::publish(path, replace, &crate::Cancel::default(), |file| {
            file.write_all(text.as_bytes()).map_err(|e| {
                Error::caused(ErrorKind::Write, format!("cannot save to {path:?}: {e}"), e)
            })
        })
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn slider_mappings_round_trip_every_setting_at_all_supported_rates() {
        for hz in [1, 22_050, 44_100, 48_000, 96_000] {
            let rate = SampleRate::new(hz).unwrap();
            for setting in Setting::ALL {
                for position in [0.0, 0.001, 0.25, 0.5, 0.99, 1.0] {
                    let value = slider_value(setting, rate, position);
                    assert!(
                        (slider_position(setting, rate, value) - position).abs() < 1e-12,
                        "{hz} {setting:?}"
                    );
                }
            }
        }
    }

    use super::*;
    #[test]
    fn identities_are_never_reused_and_operations_address_the_right_row() {
        let mut d = Draft::new();
        let a = d.add(SourceKind::Tone).unwrap();
        let b = d.add(SourceKind::Clicks).unwrap();
        d.remove(a);
        let c = d.add(SourceKind::Tone).unwrap();
        assert_ne!(a, c);
        let replacement = Component {
            peak_dbfs: -50.0,
            ..default_component(SourceKind::Tone)
        };
        d.replace(b, replacement);
        assert_eq!(d.components()[0], (b, replacement));
        d.replace(a, default_component(SourceKind::Clicks));
        assert_eq!(d.components().len(), 2);
        while d.components().len() < 16 {
            d.add(SourceKind::Tone).unwrap();
        }
        assert!(d.add(SourceKind::Clicks).is_err());
    }
    #[test]
    fn editing_keeps_sound_and_source_context_without_copying_research_claims() {
        let p = crate::catalog().unwrap().remove(0);
        let d = Draft::from_preset(&p);
        assert_eq!(d.title(), format!("{} (edited)", p.title()));
        assert_eq!(d.duration_seconds(), p.stimulus().duration_seconds);
        assert_eq!(d.fade_seconds(), p.stimulus().fade_seconds);
        assert_eq!(d.preset().stimulus().components, p.stimulus().components);
        let saved = d.preset();
        let a = saved.about().unwrap();
        assert_eq!(a.summary.as_deref(), Some(""));
        assert_eq!(d.based_on().unwrap().origin(), p.origin());
        assert!(
            a.citation.is_none()
                && a.study.is_none()
                && a.source_url.is_none()
                && a.adaptation.is_none()
                && a.studied_for.is_none()
        );
    }
    #[test]
    fn row_errors_and_overall_peak_are_separate() {
        let mut d = Draft::new();
        let a = d.add(SourceKind::Tone).unwrap();
        let b = d.add(SourceKind::Tone).unwrap();
        d.replace(
            b,
            Component {
                carrier: Carrier::Tone {
                    frequency_hz: 30_000.0,
                },
                ..default_component(SourceKind::Tone)
            },
        );
        let c = d.check(SampleRate::DEFAULT);
        assert!(c.components[0].1.is_none());
        assert!(c.components[1].1.is_some());
        assert!(c.sound.is_err());
        for id in [a, b] {
            d.replace(
                id,
                Component {
                    peak_dbfs: 0.0,
                    ..default_component(SourceKind::Tone)
                },
            );
        }
        let c = d.check(SampleRate::DEFAULT);
        assert!(c.components.iter().all(|(_, error)| error.is_none()));
        assert!(c.sound.unwrap_err().contains("peak"));
    }
    #[test]
    fn save_round_trips_and_keeps_existing_destinations_on_rejection() {
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
        std::fs::create_dir_all(&base).unwrap();
        let dir = tempfile::tempdir_in(base).unwrap();
        let path = dir.path().join("recipe.toml");
        let mut d = Draft::new();
        d.add(SourceKind::Clicks).unwrap();
        d.set_title("Quotes \" 🎧".into());
        d.save(&path, Replace::Never).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let p = crate::load_file(&path).unwrap();
        assert_eq!(p.stimulus(), d.preset().stimulus());
        assert!(matches!(
            d.save(&path, Replace::Never).unwrap_err().kind(),
            ErrorKind::DestinationExists
        ));
        d.set_title("🎧".repeat(auditorylab_core::Preset::MAX_BYTES));
        assert_eq!(
            d.save(&path, Replace::Allowed).unwrap_err().kind(),
            ErrorKind::InvalidPreset
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        d.set_title("Small".into());
        d.set_duration_seconds(f64::NAN);
        assert!(d.save(&path, Replace::Allowed).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    #[test]
    fn ranges_and_defaults_are_valid_at_audio_rates_and_frequency_spans_remain_positive() {
        for hz in [44_100, 48_000, 96_000] {
            let rate = SampleRate::new(hz).unwrap();
            for kind in [SourceKind::Tone, SourceKind::Clicks] {
                default_component(kind).validate(rate).unwrap();
            }
            for setting in Setting::ALL {
                let r = range(setting, rate);
                assert!(r.min < r.max && r.default >= r.min && r.default <= r.max);
                if r.scale == Scale::Logarithmic {
                    assert!(r.min > 0.0);
                }
                let mut component = default_component(SourceKind::Tone);
                match setting {
                    Setting::Frequency => {
                        component.carrier = Carrier::Tone {
                            frequency_hz: r.default,
                        }
                    }
                    Setting::ClickRate => {
                        component.carrier = Carrier::Click {
                            rate_hz: r.default,
                            width_seconds: 0.001,
                        }
                    }
                    Setting::ClickWidth => {
                        component.carrier = Carrier::Click {
                            rate_hz: 40.0,
                            width_seconds: r.default / 1000.0,
                        }
                    }
                    Setting::Level => component.peak_dbfs = r.default,
                    Setting::ModulationRate => {
                        component.modulation = Some(Modulation::Sinusoidal {
                            rate_hz: r.default,
                            depth: 1.0,
                        })
                    }
                    Setting::Depth => {
                        component.modulation = Some(Modulation::Sinusoidal {
                            rate_hz: 40.0,
                            depth: r.default / 100.0,
                        })
                    }
                    Setting::GateRate => {
                        component.modulation = Some(Modulation::Rectangular {
                            rate_hz: r.default,
                            width_seconds: 0.001,
                        })
                    }
                    Setting::GateWidth => {
                        component.modulation = Some(Modulation::Rectangular {
                            rate_hz: 40.0,
                            width_seconds: r.default / 1000.0,
                        })
                    }
                }
                component.validate(rate).unwrap();
            }
        }
        for hz in [1, 44, 45, 977, 978, 22_050] {
            let rate = SampleRate::new(hz).unwrap();
            let r = range(Setting::Frequency, rate);
            assert!(r.min > 0.0 && r.min < r.max && r.max < 0.5 * f64::from(hz));
            Component {
                carrier: Carrier::Tone {
                    frequency_hz: r.default,
                },
                ..default_component(SourceKind::Tone)
            }
            .validate(rate)
            .unwrap();
        }
    }
    #[test]
    fn oversized_import_is_preserved_and_rejected_as_a_live_target() {
        let text = format!(
            "schema_version=2\nduration_seconds=1\nfade_seconds=0\n{}",
            "[[component]]\nears=\"both\"\npeak_dbfs=-40\nkind=\"tone\"\nfrequency_hz=440\n"
                .repeat(17)
        );
        let p = crate::load_reader(text.as_bytes(), "oversized").unwrap();
        let d = Draft::from_preset(&p);
        assert_eq!(d.components().len(), 17);
        assert!(d.target(SampleRate::DEFAULT).is_err());
        assert_eq!(d.preset().stimulus().components, p.stimulus().components);
    }
    #[test]
    fn invalid_updates_preserve_the_waiting_live_target() {
        let rate = SampleRate::DEFAULT;
        let mailbox = Arc::new(std::sync::Mutex::new(
            crate::playback::source::Updates::default(),
        ));
        let control = crate::LiveControl {
            rate,
            mailbox: mailbox.clone(),
        };
        let mut d = Draft::new();
        control.update(&d).unwrap();
        let id = d.add(SourceKind::Tone).unwrap();
        d.replace(
            id,
            Component {
                carrier: Carrier::Tone {
                    frequency_hz: 30_000.0,
                },
                ..default_component(SourceKind::Tone)
            },
        );
        assert!(control.update(&d).is_err());
        assert_eq!(
            mailbox
                .lock()
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .1
                .peak_dbfs(),
            [f64::NEG_INFINITY; 2]
        );
    }
}
