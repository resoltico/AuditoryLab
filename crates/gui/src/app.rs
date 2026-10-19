//! Desktop selection, editing, validation and operation transitions.
use auditorylab::{
    Carrier, Component, ComponentId, Draft, Ears, LiveControl, Modulation, Origin, Preset,
    SampleRate, Scale, Setting, Sound, SourceKind, format_dbfs, human_duration, range,
};
use std::collections::HashMap;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Operation {
    Import,
    Play,
    Live,
    Export,
    Save,
}
trait TargetControl {
    fn sample_rate(&self) -> SampleRate;
    fn update(&self, draft: &Draft) -> Result<(), auditorylab::Error>;
}
impl TargetControl for LiveControl {
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate()
    }
    fn update(&self, draft: &Draft) -> Result<(), auditorylab::Error> {
        self.update(draft)
    }
}
pub(crate) struct Workbench {
    pub(crate) draft: Draft,
    control: Option<Box<dyn TargetControl>>,
    pub(crate) messages: HashMap<ComponentId, String>,
    pub(crate) parse_errors: HashMap<(ComponentId, Setting), String>,
    pub(crate) pending: HashMap<(ComponentId, Setting), String>,
    pub(crate) pending_metadata: HashMap<String, String>,
    pub(crate) metadata_errors: HashMap<&'static str, String>,
    pub(crate) peak: String,
    pub(crate) preview: Option<Sound>,
}
impl Workbench {
    pub(crate) fn new(draft: Draft, rate: SampleRate) -> Self {
        let mut this = Self {
            draft,
            control: None,
            messages: HashMap::new(),
            parse_errors: HashMap::new(),
            metadata_errors: HashMap::new(),
            pending: HashMap::new(),
            pending_metadata: HashMap::new(),
            peak: String::new(),
            preview: None,
        };
        this.refresh(rate);
        this
    }
    pub(crate) fn rate(&self, export: SampleRate) -> SampleRate {
        self.control
            .as_ref()
            .map_or(export, |control| control.sample_rate())
    }
    pub(crate) fn ready(&mut self, control: LiveControl, export: SampleRate) {
        self.attach(Box::new(control), export);
    }
    fn attach(&mut self, control: Box<dyn TargetControl>, export: SampleRate) {
        self.control = Some(control);
        // Edits made during device preparation must reach the first available control.
        self.refresh(export);
    }
    pub(crate) fn refresh(&mut self, export: SampleRate) {
        let rate = self.rate(export);
        let check = self.draft.check(rate);
        self.messages = check
            .components
            .into_iter()
            .filter_map(|(id, e)| e.map(|e| (id, e)))
            .collect();
        for ((id, _), error) in &self.parse_errors {
            self.messages
                .entry(*id)
                .and_modify(|message| {
                    message.push_str("; ");
                    message.push_str(error);
                })
                .or_insert_with(|| error.clone());
        }
        self.peak = match check.sound {
            Ok(peak) if self.parse_errors.is_empty() => {
                let mut preview = self.draft.clone();
                // Match the live engine's entire frame lifetime, without export edge fades.
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "MAX_FRAMES is an exactly representable power of two"
                )]
                let duration = auditorylab::Signal::MAX_FRAMES as f64 / f64::from(rate.hz());
                preview.set_duration_seconds(duration);
                preview.set_fade_seconds(0.0);
                self.preview = preview.preset().at(rate).ok();
                if let Some(control) = &self.control {
                    // Validated above at this exact rate; a poisoned mailbox is still actionable.
                    if let Err(error) = control.update(&self.draft) {
                        return self.peak = error.to_string();
                    }
                }
                format!(
                    "Peak bounds: {} / {} dBFS",
                    format_dbfs(peak[0]),
                    format_dbfs(peak[1])
                )
            }
            Ok(_) => "Correct the numeric fields to apply this sound.".into(),
            Err(error) => error,
        };
    }
    pub(crate) fn id(&self, text: &str) -> Option<ComponentId> {
        let number = text.parse::<u32>().ok()?;
        self.draft
            .components()
            .iter()
            .find(|(id, _)| id.get() == number)
            .map(|(id, _)| *id)
    }
    pub(crate) fn edit(&mut self, id: ComponentId, setting: Setting, text: &str, rate: SampleRate) {
        if !self
            .draft
            .components()
            .iter()
            .any(|(key, c)| *key == id && value(c, setting).is_some())
        {
            return;
        }
        self.pending.remove(&(id, setting));
        match text.parse::<f64>() {
            Ok(value) if value.is_finite() => {
                self.parse_errors.remove(&(id, setting));
                if let Some((_, component)) =
                    self.draft.components().iter().find(|(key, _)| *key == id)
                {
                    let mut component = *component;
                    set_value(&mut component, setting, value);
                    self.draft.replace(id, component);
                }
            }
            _ => {
                self.parse_errors
                    .insert((id, setting), format!("{setting:?}: enter a finite number"));
            }
        }
        self.refresh(rate);
    }
    pub(crate) fn choice(&mut self, id: ComponentId, choice: i32, index: i32, rate: SampleRate) {
        let Some((_, component)) = self.draft.components().iter().find(|(key, _)| *key == id)
        else {
            return;
        };
        let mut c = *component;
        match choice {
            0 => {
                c.ears = match index {
                    1 => Ears::Left,
                    2 => Ears::Right,
                    _ => Ears::Both,
                }
            }
            1 => {
                c.carrier = auditorylab::default_component(if index == 1 {
                    SourceKind::Clicks
                } else {
                    SourceKind::Tone
                })
                .carrier
            }
            2 => {
                c.modulation = match index {
                    1 => Some(Modulation::Sinusoidal {
                        rate_hz: range(Setting::ModulationRate, rate).default,
                        depth: range(Setting::Depth, rate).default,
                    }),
                    2 => Some(Modulation::Rectangular {
                        rate_hz: range(Setting::GateRate, rate).default,
                        width_seconds: range(Setting::GateWidth, rate).default / 1000.0,
                    }),
                    _ => None,
                }
            }
            _ => return,
        }
        // Discard parser errors only for controls that the kind change removes.
        self.parse_errors
            .retain(|(key, setting), _| *key != id || value(&c, *setting).is_some());
        self.pending
            .retain(|(key, setting), _| *key != id || value(&c, *setting).is_some());
        self.draft.replace(id, c);
        self.refresh(rate);
    }
    pub(crate) fn remove(&mut self, id: ComponentId, rate: SampleRate) {
        self.draft.remove(id);
        self.pending.retain(|(key, _), _| *key != id);
        self.parse_errors.retain(|(key, _), _| *key != id);
        self.refresh(rate);
    }
    pub(crate) fn metadata(&mut self, key: &str, text: &str) {
        self.pending_metadata.remove(key);
        match key {
            "title" => self.draft.set_title(text.into()),
            "summary" => self.draft.set_summary(text.into()),
            "duration" | "fade" => {
                let key = if key == "duration" {
                    "duration"
                } else {
                    "fade"
                };
                match text.parse::<f64>() {
                    Ok(value) if value.is_finite() => {
                        self.metadata_errors.remove(key);
                        if key == "duration" {
                            self.draft.set_duration_seconds(value);
                        } else {
                            self.draft.set_fade_seconds(value);
                        }
                    }
                    _ => {
                        self.metadata_errors
                            .insert(key, format!("{key}: enter a finite number"));
                    }
                }
            }
            _ => {}
        }
    }
    pub(crate) fn commit_pending(&mut self, rate: SampleRate) {
        for ((id, setting), text) in std::mem::take(&mut self.pending) {
            self.edit(id, setting, &text, rate);
        }
        for (key, text) in std::mem::take(&mut self.pending_metadata) {
            self.metadata(&key, &text);
        }
    }
    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty() || !self.pending_metadata.is_empty()
    }
    pub(crate) fn can_save(&self) -> bool {
        self.parse_errors.is_empty() && self.metadata_errors.is_empty()
    }
}
pub(crate) fn value(c: &Component, s: Setting) -> Option<f64> {
    match (s, c.carrier, c.modulation) {
        (Setting::Level, _, _) => Some(c.peak_dbfs),
        (Setting::Frequency, Carrier::Tone { frequency_hz }, _) => Some(frequency_hz),
        (Setting::ClickRate, Carrier::Click { rate_hz, .. }, _) => Some(rate_hz),
        (Setting::ClickWidth, Carrier::Click { width_seconds, .. }, _) => {
            Some(width_seconds * 1000.0)
        }
        (Setting::ModulationRate, _, Some(Modulation::Sinusoidal { rate_hz, .. }))
        | (Setting::GateRate, _, Some(Modulation::Rectangular { rate_hz, .. })) => Some(rate_hz),
        (Setting::Depth, _, Some(Modulation::Sinusoidal { depth, .. })) => Some(depth),
        (Setting::GateWidth, _, Some(Modulation::Rectangular { width_seconds, .. })) => {
            Some(width_seconds * 1000.0)
        }
        _ => None,
    }
}
fn set_value(c: &mut Component, s: Setting, value: f64) {
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
                *depth = value;
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
pub(crate) fn slider_value(setting: Setting, rate: SampleRate, position: f64) -> f64 {
    let r = range(setting, rate);
    match r.scale {
        Scale::Linear => r.min + (r.max - r.min) * position.clamp(0.0, 1.0),
        Scale::Logarithmic => r.min * (r.max / r.min).powf(position.clamp(0.0, 1.0)),
    }
}
pub(crate) fn slider_position(setting: Setting, rate: SampleRate, value: f64) -> f64 {
    let r = range(setting, rate);
    match r.scale {
        Scale::Linear => (value - r.min) / (r.max - r.min),
        Scale::Logarithmic => (value.clamp(r.min, r.max) / r.min).ln() / (r.max / r.min).ln(),
    }
    .clamp(0.0, 1.0)
}
pub(crate) struct App {
    pub(crate) presets: Vec<Preset>,
    pub(crate) selected: usize,
    pub(crate) rate: SampleRate,
    pub(crate) operation: Option<Operation>,
    pub(crate) closing: bool,
    pub(crate) playback: Option<Sound>,
    pub(crate) status: String,
    pub(crate) workbench: Workbench,
    pub(crate) editing: bool,
    pub(crate) operation_editing: bool,
}
impl App {
    pub(crate) fn new(presets: Vec<Preset>) -> Self {
        Self {
            presets,
            selected: 0,
            rate: SampleRate::DEFAULT,
            operation: None,
            closing: false,
            playback: None,
            status: String::new(),
            workbench: Workbench::new(Draft::new(), SampleRate::DEFAULT),
            editing: false,
            operation_editing: false,
        }
    }
    pub(crate) fn preset(&self) -> &Preset {
        &self.presets[self.selected]
    }
    pub(crate) fn select(&mut self, index: usize) {
        if self.operation.is_none() && index < self.presets.len() {
            self.selected = index;
            self.status.clear();
        }
    }
    pub(crate) fn start(&mut self, op: Operation) -> bool {
        if self.operation.is_some() || self.closing {
            return false;
        }
        self.operation = Some(op);
        self.operation_editing = self.editing;
        self.status.clear();
        true
    }
    pub(crate) fn finish(&mut self) {
        self.operation = None;
        self.playback = None;
        self.workbench.control = None;
        self.workbench.refresh(self.rate);
    }
    pub(crate) fn open_workbench(&mut self) {
        if self.operation.is_some() {
            return;
        }
        if self.preset().stimulus().components.len() > 16 {
            self.status="Workbench supports at most 16 components; edit the recipe file to reduce its component count.".into();
            return;
        }
        self.workbench = Workbench::new(Draft::from_preset(self.preset()), self.rate);
        self.editing = true;
    }
    pub(crate) fn imported(&mut self, result: Result<Preset, auditorylab::Error>) -> bool {
        let preset = match result {
            Ok(p) => p,
            Err(e) => {
                self.status = e.to_string();
                return false;
            }
        };
        self.status.clear();
        if let Some(index) = self
            .presets
            .iter()
            .position(|p| p.origin() == preset.origin())
        {
            self.presets[index] = preset;
            self.selected = index;
        } else {
            self.selected = self.presets.len();
            self.presets.push(preset);
        }
        true
    }
    pub(crate) fn sound(&self) -> Result<Sound, auditorylab::Error> {
        self.playback
            .clone()
            .map_or_else(|| self.preset().at(self.rate), Ok)
    }
}
pub(crate) fn label(p: &Preset) -> String {
    format!(
        "{}{}\n{} · {}",
        if matches!(p.origin(), Origin::File(_)) {
            "Imported: "
        } else {
            ""
        },
        p.title(),
        p.about()
            .and_then(|a| a.studied_for.as_deref())
            .unwrap_or("Custom recipe"),
        human_duration(p.stimulus().duration_seconds)
    )
}
pub(crate) fn scope_start(sound: &Sound, playing: bool, position: u64, frames: u64) -> u64 {
    if playing {
        position.saturating_sub(frames)
    } else {
        sound.signal().fade_frames()
    }
}
pub(crate) fn details(p: &Preset, rate: SampleRate) -> String {
    let mut text = String::new();
    for (name, value) in p.research() {
        text.push_str(&format!("{name}: {value}\n\n"));
    }
    for (i, c) in p.stimulus().components.iter().enumerate() {
        text.push_str(&format!("Component {}: {c}\n", i + 1));
    }
    match p.at(rate) {
        Ok(sound) => {
            let s = sound.signal();
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_import_preserves_the_selected_recipe_and_list() {
        let mut app = App::new(auditorylab::catalog().unwrap());
        app.select(3);
        let origin = app.preset().origin().clone();
        let count = app.presets.len();
        let error = auditorylab::load_reader(b"not a recipe".as_slice(), "bad import").unwrap_err();
        assert!(!app.imported(Err(error)));
        assert_eq!(app.selected, 3);
        assert_eq!(app.preset().origin(), &origin);
        assert_eq!(app.presets.len(), count);
        assert!(app.status.contains("bad import"));
    }
    #[test]
    fn selection_is_locked_and_close_waits_for_the_active_operation() {
        let mut app = App::new(auditorylab::catalog().unwrap());
        app.select(3);
        assert_eq!(app.selected, 3);
        assert!(app.start(Operation::Play));
        assert!(!app.start(Operation::Export));
        app.select(4);
        assert_eq!(app.selected, 3);
        app.closing = true;
        assert!(app.operation.is_some());
        app.finish();
        assert!(!app.start(Operation::Play));
        assert_eq!(app.selected, 3);
    }
    #[test]
    fn imports_replace_the_same_origin_and_scope_tracks_the_sound_rate() {
        let mut app = App::new(auditorylab::catalog().unwrap());
        let preset = auditorylab::load_reader(
            b"schema_version=2\nduration_seconds=1\nfade_seconds=0\n".as_slice(),
            "import",
        )
        .unwrap();
        app.imported(Ok(preset.clone()));
        app.imported(Ok(preset));
        assert_eq!(app.presets.len(), 20);
        let sound = app.sound().unwrap();
        assert_eq!(scope_start(&sound, true, 1000, 480), 520);
        assert_eq!(scope_start(&sound, true, 10, 480), 0);
        assert_eq!(scope_start(&sound, false, 1000, 480), 0);
    }
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
    type Sent = std::rc::Rc<std::cell::RefCell<Vec<Draft>>>;
    struct Recorder {
        rate: SampleRate,
        sent: Sent,
    }
    impl TargetControl for Recorder {
        fn sample_rate(&self) -> SampleRate {
            self.rate
        }
        fn update(&self, draft: &Draft) -> Result<(), auditorylab::Error> {
            draft.target(self.rate)?;
            self.sent.borrow_mut().push(draft.clone());
            Ok(())
        }
    }
    #[test]
    fn deferred_ready_sends_current_valid_draft_and_invalid_edits_keep_last_target() {
        let rate = SampleRate::new(44_100).unwrap();
        let mut w = Workbench::new(Draft::new(), SampleRate::DEFAULT);
        let id = w.draft.add(SourceKind::Tone).unwrap();
        w.edit(id, Setting::Frequency, "1000", SampleRate::DEFAULT);
        let sent = Sent::default();
        w.attach(
            Box::new(Recorder {
                rate,
                sent: sent.clone(),
            }),
            SampleRate::DEFAULT,
        );
        assert_eq!(sent.borrow().len(), 1);
        assert_eq!(
            value(&sent.borrow()[0].components()[0].1, Setting::Frequency),
            Some(1000.0)
        );
        assert_eq!(w.preview.as_ref().unwrap().signal().sample_rate(), rate);
        let expected = w
            .preview
            .as_ref()
            .unwrap()
            .preset()
            .stimulus()
            .components
            .clone();
        w.edit(id, Setting::Frequency, "23000", SampleRate::DEFAULT);
        assert!(w.messages[&id].contains("frequency"));
        assert_eq!(sent.borrow().len(), 1);
        assert_eq!(
            w.preview.as_ref().unwrap().preset().stimulus().components,
            expected
        );
        w.edit(id, Setting::Frequency, "1e-", SampleRate::DEFAULT);
        assert!(w.messages[&id].contains("finite number"));
        assert_eq!(sent.borrow().len(), 1);
        w.edit(id, Setting::Frequency, "10", SampleRate::DEFAULT);
        assert!(w.messages.is_empty());
        assert_eq!(sent.borrow().len(), 2);
        assert_eq!(
            value(&w.draft.components()[0].1, Setting::Frequency),
            Some(10.0)
        );
        assert_eq!(slider_position(Setting::Frequency, rate, 10.0), 0.0);
    }
    #[test]
    fn invalid_draft_before_ready_leaves_initial_target_unchanged() {
        let mut w = Workbench::new(Draft::new(), SampleRate::DEFAULT);
        let id = w.draft.add(SourceKind::Tone).unwrap();
        w.edit(id, Setting::Frequency, "23000", SampleRate::DEFAULT);
        let sent = Sent::default();
        w.attach(
            Box::new(Recorder {
                rate: SampleRate::new(44_100).unwrap(),
                sent: sent.clone(),
            }),
            SampleRate::DEFAULT,
        );
        assert!(sent.borrow().is_empty());
        assert!(!w.messages.is_empty());
    }
    #[test]
    fn preview_spans_the_live_lifetime_instead_of_ending_at_one_hour() {
        for hz in [44_100, 48_000, 96_000] {
            let rate = SampleRate::new(hz).unwrap();
            let mut draft = Draft::new();
            draft.add(SourceKind::Tone).unwrap();
            let w = Workbench::new(draft, rate);
            let signal = w.preview.as_ref().unwrap().signal();
            assert_eq!(signal.frames(), auditorylab::Signal::MAX_FRAMES);
            assert_eq!(signal.fade_frames(), 0);
            let mut bins = Vec::new();
            for start in [u64::from(hz) * 3601, signal.frames() - 1000] {
                auditorylab::trace(signal, start, 480, 64, &mut bins);
                assert!(
                    bins.iter()
                        .any(|bin| bin[0].min != 0.0 || bin[0].max != 0.0)
                );
            }
        }
    }
    #[test]
    fn row_identity_limit_removal_and_cross_tab_operation_ownership() {
        let mut app = App::new(auditorylab::catalog().unwrap());
        app.open_workbench();
        let original = app.workbench.draft.components()[0].0;
        app.workbench
            .edit(original, Setting::Level, "bad", app.rate);
        app.workbench.remove(original, app.rate);
        assert!(app.workbench.parse_errors.is_empty());
        while app.workbench.draft.components().len() < 16 {
            app.workbench.draft.add(SourceKind::Tone).unwrap();
        }
        assert!(app.workbench.draft.add(SourceKind::Tone).is_err());
        assert!(app.start(Operation::Live));
        assert!(app.operation_editing);
        app.editing = false;
        assert!(!app.start(Operation::Play));
        let before = app.workbench.draft.components().len();
        app.open_workbench();
        assert_eq!(app.workbench.draft.components().len(), before);
        app.finish();
        assert!(app.workbench.control.is_none());
    }
    #[test]
    fn operation_snapshot_commits_pending_fields_and_rejects_pending_parser_errors() {
        let mut w = Workbench::new(Draft::new(), SampleRate::DEFAULT);
        let id = w.draft.add(SourceKind::Tone).unwrap();
        w.pending_metadata
            .insert("title".into(), "Saved without Enter".into());
        w.pending_metadata.insert("duration".into(), "0.05".into());
        w.pending_metadata.insert("fade".into(), "0".into());
        w.pending.insert((id, Setting::Frequency), "1234".into());
        assert_eq!(w.draft.title(), "Untitled sound");
        w.commit_pending(SampleRate::DEFAULT);
        assert_eq!(w.draft.title(), "Saved without Enter");
        assert_eq!(w.draft.duration_seconds(), 0.05);
        assert_eq!(w.draft.fade_seconds(), 0.0);
        assert_eq!(
            value(&w.draft.components()[0].1, Setting::Frequency),
            Some(1234.0)
        );
        assert!(w.can_save());
        w.pending.insert((id, Setting::Frequency), "1e-".into());
        w.commit_pending(SampleRate::DEFAULT);
        assert!(!w.can_save());
        assert_eq!(
            value(&w.draft.components()[0].1, Setting::Frequency),
            Some(1234.0)
        );
    }
    #[test]
    fn changing_kind_discards_hidden_pending_values_and_corrections_can_be_committed() {
        let mut w = Workbench::new(Draft::new(), SampleRate::DEFAULT);
        let id = w.draft.add(SourceKind::Tone).unwrap();
        w.pending.insert((id, Setting::Frequency), "bad".into());
        w.choice(id, 1, 1, SampleRate::DEFAULT);
        assert!(!w.has_pending());
        w.commit_pending(SampleRate::DEFAULT);
        assert!(w.can_save());
        w.edit(id, Setting::ClickRate, "bad", SampleRate::DEFAULT);
        assert!(!w.can_save());
        w.pending.insert((id, Setting::ClickRate), "40".into());
        assert!(w.has_pending());
        w.commit_pending(SampleRate::DEFAULT);
        assert!(w.can_save());
    }
    #[test]
    fn oversized_import_stays_in_library_without_replacing_the_workbench() {
        let mut app = App::new(auditorylab::catalog().unwrap());
        let text = format!(
            "schema_version=2\nduration_seconds=1\nfade_seconds=0\n{}",
            "[[component]]\nears=\"both\"\npeak_dbfs=-40\nkind=\"tone\"\nfrequency_hz=440\n"
                .repeat(17)
        );
        app.imported(auditorylab::load_reader(text.as_bytes(), "oversized"));
        let origin = app.preset().origin().clone();
        app.open_workbench();
        assert!(!app.editing);
        assert!(app.workbench.draft.components().is_empty());
        assert_eq!(app.preset().origin(), &origin);
        assert_eq!(app.preset().stimulus().components.len(), 17);
        assert!(app.status.contains("at most 16"));
    }
}
