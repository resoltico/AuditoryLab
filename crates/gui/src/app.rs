//! Desktop selection, editing, validation and operation transitions.
use auditorylab::{
    ComponentId, Draft, Ears, LiveControl, Origin, Preset, SampleRate, Setting, SourceKind,
    format_dbfs, human_duration, set_setting, setting_value,
};
use std::collections::HashMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Operation {
    Import,
    Play,
    Live,
    Export,
    Save,
}
impl Operation {
    pub(crate) fn is_audio(self) -> bool {
        matches!(self, Self::Play | Self::Live)
    }
}
trait TargetControl {
    fn sample_rate(&self) -> SampleRate;
    fn update(&self, draft: &Draft) -> Result<u64, auditorylab::Error>;
}
impl TargetControl for LiveControl {
    fn sample_rate(&self) -> SampleRate {
        self.sample_rate()
    }
    fn update(&self, draft: &Draft) -> Result<u64, auditorylab::Error> {
        self.update(draft)
    }
}
pub(crate) struct Workbench {
    pub(crate) draft: Draft,
    pub(crate) saved: Draft,
    pub(crate) identity: std::sync::Arc<()>,
    control: Option<Box<dyn TargetControl>>,
    pub(crate) last_target: Option<auditorylab::Target>,
    pub(crate) messages: HashMap<ComponentId, String>,
    pub(crate) parse_errors: HashMap<(ComponentId, Setting), String>,
    pub(crate) pending: HashMap<(ComponentId, Setting), String>,
    pub(crate) pending_metadata: HashMap<String, String>,
    pub(crate) metadata_errors: HashMap<&'static str, String>,
    pub(crate) peak: String,
    pub(crate) application_error: Option<String>,
}
impl Workbench {
    pub(crate) fn new(draft: Draft, rate: SampleRate) -> Self {
        let mut this = Self {
            draft,
            saved: Draft::new(),
            identity: std::sync::Arc::new(()),
            control: None,
            last_target: None,
            messages: HashMap::new(),
            parse_errors: HashMap::new(),
            metadata_errors: HashMap::new(),
            pending: HashMap::new(),
            pending_metadata: HashMap::new(),
            peak: String::new(),
            application_error: None,
        };
        this.refresh(rate);
        this
    }
    pub(crate) fn dirty(&self) -> bool {
        !self.draft.same_content(&self.saved)
            || self.has_pending()
            || !self.parse_errors.is_empty()
            || !self.metadata_errors.is_empty()
    }
    pub(crate) fn rate(&self, export: SampleRate) -> SampleRate {
        self.control
            .as_ref()
            .map_or(export, |control| control.sample_rate())
    }
    pub(crate) fn ready(
        &mut self,
        owner: &std::sync::Arc<()>,
        control: LiveControl,
        export: SampleRate,
    ) -> bool {
        if !std::sync::Arc::ptr_eq(owner, &self.identity) {
            return false;
        }
        self.attach(Box::new(control), export);
        true
    }
    fn attach(&mut self, control: Box<dyn TargetControl>, export: SampleRate) {
        self.control = Some(control);
        // Edits made during device preparation must reach the first available control.
        self.refresh(export);
    }
    pub(crate) fn refresh(&mut self, export: SampleRate) {
        let rate = self.rate(export);
        self.application_error = None;
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
                self.last_target = self.draft.target(rate).ok();
                if let Some(control) = &self.control {
                    // Validated above at this exact rate; a poisoned mailbox is still actionable.
                    if let Err(error) = control.update(&self.draft) {
                        self.application_error = Some(error.to_string());
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
        self.apply_text(id, setting, text);
        self.refresh(rate);
    }
    fn apply_text(&mut self, id: ComponentId, setting: Setting, text: &str) {
        if !self
            .draft
            .components()
            .iter()
            .any(|(key, c)| *key == id && setting_value(c, setting).is_some())
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
                    set_setting(&mut component, setting, value);
                    self.draft.replace(id, component);
                }
            }
            _ => {
                self.pending.insert((id, setting), text.to_owned());
                self.parse_errors
                    .insert((id, setting), format!("{setting:?}: enter a finite number"));
            }
        }
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
                    1 => Some(auditorylab::default_modulation(
                        auditorylab::ModulationKind::Sinusoidal,
                        rate,
                    )),
                    2 => Some(auditorylab::default_modulation(
                        auditorylab::ModulationKind::Gate,
                        rate,
                    )),
                    _ => None,
                }
            }
            _ => return,
        }
        // Discard parser errors only for controls that the kind change removes.
        self.parse_errors
            .retain(|(key, setting), _| *key != id || setting_value(&c, *setting).is_some());
        self.pending
            .retain(|(key, setting), _| *key != id || setting_value(&c, *setting).is_some());
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
                        self.pending_metadata
                            .insert(key.to_owned(), text.to_owned());
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
            self.apply_text(id, setting, &text);
        }
        for (key, text) in std::mem::take(&mut self.pending_metadata) {
            self.metadata(&key, &text);
        }
        self.refresh(rate);
    }
    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty() || !self.pending_metadata.is_empty()
    }
    /// Evaluate the same complete candidate as an explicit action, without sending it to audio.
    pub(crate) fn action_draft(&self, rate: SampleRate) -> Option<Draft> {
        if !self.has_pending() {
            return self.input_valid().then(|| self.draft.clone());
        }
        let mut candidate = Self::new(self.draft.clone(), rate);
        candidate.pending = self.pending.clone();
        candidate.parse_errors = self.parse_errors.clone();
        candidate.pending_metadata = self.pending_metadata.clone();
        candidate.metadata_errors = self.metadata_errors.clone();
        candidate.commit_pending(rate);
        candidate.input_valid().then_some(candidate.draft)
    }
    pub(crate) fn field_error(
        &self,
        id: ComponentId,
        setting: Setting,
        rate: SampleRate,
    ) -> String {
        if let Some(error) = self.parse_errors.get(&(id, setting)) {
            return error.clone();
        }
        self.draft
            .components()
            .iter()
            .find(|(key, _)| *key == id)
            .and_then(|(_, c)| {
                auditorylab::setting_diagnostics(c, rate)
                    .into_iter()
                    .find(|(key, _)| *key == setting)
            })
            .map_or_else(String::new, |(_, message)| message)
    }
    pub(crate) fn record_save(&mut self, owner: &std::sync::Arc<()>, draft: Draft) -> bool {
        if !std::sync::Arc::ptr_eq(owner, &self.identity) {
            return false;
        }
        self.saved = draft;
        true
    }
    pub(crate) fn input_valid(&self) -> bool {
        self.parse_errors.is_empty() && self.metadata_errors.is_empty()
    }
    pub(crate) fn can_save(&self) -> bool {
        self.input_valid() && self.draft.to_toml().is_ok()
    }
}
pub(crate) struct App {
    pub(crate) presets: Vec<Preset>,
    pub(crate) selected: usize,
    pub(crate) rate: SampleRate,
    pub(crate) operation: Option<Operation>,
    pub(crate) file_operation: Option<Operation>,
    pub(crate) status: String,
    pub(crate) audio_status: String,
    pub(crate) workbench: Workbench,
    pub(crate) editing: bool,
}
impl App {
    pub(crate) fn new(presets: Vec<Preset>) -> Self {
        Self {
            presets,
            selected: 0,
            rate: SampleRate::DEFAULT,
            operation: None,
            file_operation: None,
            status: String::new(),
            audio_status: String::new(),
            workbench: Workbench::new(Draft::new(), SampleRate::DEFAULT),
            editing: false,
        }
    }
    pub(crate) fn preset(&self) -> &Preset {
        &self.presets[self.selected]
    }
    pub(crate) fn select(&mut self, index: usize) {
        if index < self.presets.len() {
            self.selected = index;
        }
    }
    pub(crate) fn start(&mut self, op: Operation) -> bool {
        if op == Operation::Export && self.operation.is_some()
            || op.is_audio() && self.file_operation == Some(Operation::Export)
        {
            return false;
        }
        let slot = if op.is_audio() {
            &mut self.operation
        } else {
            &mut self.file_operation
        };
        if slot.is_some() {
            return false;
        }
        *slot = Some(op);
        if op.is_audio() {
            self.audio_status.clear();
        } else {
            self.status.clear();
        }
        true
    }
    pub(crate) fn finish(&mut self, op: Operation) {
        if op.is_audio() {
            self.operation = None;
            self.workbench.control = None;
            self.workbench.refresh(self.rate);
        } else {
            self.file_operation = None;
        }
    }
    pub(crate) fn open_workbench(&mut self, preset: &Preset) {
        if preset.stimulus().components.len() > 16 {
            self.status="Workbench supports at most 16 components; edit the recipe file to reduce its component count.".into();
            return;
        }
        self.workbench = Workbench::new(Draft::from_preset(preset), self.rate);
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
}
pub(crate) fn label(p: &Preset, rate: SampleRate) -> String {
    let structure = p.at(rate).map_or_else(
        |_| "Recipe needs validation".into(),
        |sound| auditorylab::sound_brief(&sound),
    );
    format!(
        "{}{}\n{}\n{} · {}",
        if matches!(p.origin(), Origin::File(_)) {
            "Imported · "
        } else {
            ""
        },
        p.title(),
        structure,
        p.about()
            .and_then(|a| a.studied_for.as_deref())
            .unwrap_or("Custom recipe"),
        human_duration(p.stimulus().duration_seconds)
    )
}
pub(crate) fn details(p: &Preset, rate: SampleRate) -> String {
    let mut text = format!("Source: {}\n\n", p.origin());
    for (name, value) in p.research() {
        text.push_str(&format!("{name}: {value}\n\n"));
    }
    for (i, c) in p.stimulus().components.iter().enumerate() {
        text.push_str(&format!("Component {}: {c}\n", i + 1));
    }
    match p.at(rate) {
        Ok(sound) => {
            let s = sound.signal();
            text.push_str(&auditorylab::sound_description(&sound, true));
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
    fn browsing_preserves_audio_ownership_and_export_waits_for_idle_audio() {
        let mut app = App::new(auditorylab::catalog().unwrap());
        app.select(3);
        assert_eq!(app.selected, 3);
        assert!(app.start(Operation::Play));
        assert!(!app.start(Operation::Export));
        app.select(4);
        assert_eq!(app.selected, 4);
        assert!(app.operation.is_some());
        app.finish(Operation::Play);
        assert!(app.start(Operation::Play));
        assert_eq!(app.selected, 4);
    }
    #[test]
    fn imports_replace_the_same_origin_without_duplicating_library_rows() {
        let mut app = App::new(auditorylab::catalog().unwrap());
        let preset = auditorylab::load_reader(
            b"schema_version=2\nduration_seconds=1\nfade_seconds=0\n".as_slice(),
            "import",
        )
        .unwrap();
        app.imported(Ok(preset.clone()));
        app.imported(Ok(preset));
        assert_eq!(app.presets.len(), 20);
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
        fn update(&self, draft: &Draft) -> Result<u64, auditorylab::Error> {
            draft.target(self.rate)?;
            self.sent.borrow_mut().push(draft.clone());
            Ok(u64::try_from(self.sent.borrow().len()).unwrap())
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
            setting_value(&sent.borrow()[0].components()[0].1, Setting::Frequency),
            Some(1000.0)
        );
        w.edit(id, Setting::Frequency, "23000", SampleRate::DEFAULT);
        assert!(w.messages[&id].contains("frequency"));
        assert_eq!(sent.borrow().len(), 1);
        w.edit(id, Setting::Frequency, "1e-", SampleRate::DEFAULT);
        assert!(w.messages[&id].contains("finite number"));
        assert_eq!(sent.borrow().len(), 1);
        w.edit(id, Setting::Frequency, "10", SampleRate::DEFAULT);
        assert!(w.messages.is_empty());
        assert_eq!(sent.borrow().len(), 2);
        assert_eq!(
            setting_value(&w.draft.components()[0].1, Setting::Frequency),
            Some(10.0)
        );
        assert_eq!(
            auditorylab::slider_position(Setting::Frequency, rate, 10.0),
            0.0
        );
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
    fn row_identity_limit_removal_and_cross_tab_operation_ownership() {
        let mut app = App::new(auditorylab::catalog().unwrap());
        app.open_workbench(&app.preset().clone());
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
        app.editing = false;
        assert!(!app.start(Operation::Play));
        let playing_owner = app.workbench.identity.clone();
        app.open_workbench(&app.preset().clone());
        assert!(!std::sync::Arc::ptr_eq(
            &playing_owner,
            &app.workbench.identity
        ));
        assert_eq!(app.operation, Some(Operation::Live));
        assert!(app.workbench.control.is_none());
        app.finish(Operation::Live);
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
            setting_value(&w.draft.components()[0].1, Setting::Frequency),
            Some(1234.0)
        );
        assert!(w.can_save());
        w.pending.insert((id, Setting::Frequency), "1e-".into());
        w.commit_pending(SampleRate::DEFAULT);
        assert!(!w.can_save());
        assert_eq!(
            setting_value(&w.draft.components()[0].1, Setting::Frequency),
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
        app.open_workbench(&app.preset().clone());
        assert!(!app.editing);
        assert!(app.workbench.draft.components().is_empty());
        assert_eq!(app.preset().origin(), &origin);
        assert_eq!(app.preset().stimulus().components.len(), 17);
        assert!(app.status.contains("at most 16"));
    }
    #[test]
    fn dirty_state_uses_the_saved_snapshot_and_keeps_later_edits() {
        let mut w = Workbench::new(Draft::new(), SampleRate::DEFAULT);
        assert!(!w.dirty());
        w.metadata("title", "saved title");
        assert!(w.dirty());
        let snapshot = w.draft.clone();
        w.metadata("title", "later title");
        w.saved = snapshot.clone();
        assert!(w.dirty());
        w.draft = snapshot;
        assert!(!w.dirty());
        w.pending_metadata
            .insert("title".into(), "pending title".into());
        assert!(w.dirty());
        w.commit_pending(SampleRate::DEFAULT);
        assert_eq!(w.draft.title(), "pending title");
    }
    #[test]
    fn a_pending_batch_with_a_parser_error_never_sends_a_partial_target() {
        let mut w = Workbench::new(Draft::new(), SampleRate::DEFAULT);
        let a = w.draft.add(SourceKind::Tone).unwrap();
        let b = w.draft.add(SourceKind::Tone).unwrap();
        let sent = Sent::default();
        w.attach(
            Box::new(Recorder {
                rate: SampleRate::DEFAULT,
                sent: sent.clone(),
            }),
            SampleRate::DEFAULT,
        );
        assert_eq!(sent.borrow().len(), 1);
        w.pending.insert((a, Setting::Frequency), "1000".into());
        w.pending.insert((b, Setting::Level), "bad".into());
        w.commit_pending(SampleRate::DEFAULT);
        assert_eq!(sent.borrow().len(), 1);
        assert!(!w.parse_errors.is_empty());
    }

    #[test]
    fn saving_has_an_independent_slot_and_never_detaches_the_audio_control() {
        let mut app = App::new(auditorylab::catalog().unwrap());
        let sent = Sent::default();
        app.workbench.attach(
            Box::new(Recorder {
                rate: app.rate,
                sent: sent.clone(),
            }),
            app.rate,
        );
        assert!(app.start(Operation::Live));
        assert!(app.start(Operation::Save));
        assert!(!app.start(Operation::Save));
        assert!(!app.start(Operation::Import));
        assert!(!app.start(Operation::Play));
        app.finish(Operation::Save);
        assert_eq!(app.operation, Some(Operation::Live));
        assert!(app.workbench.control.is_some());
        let count = sent.borrow().len();
        app.workbench.refresh(app.rate);
        assert!(sent.borrow().len() > count);
        assert!(app.start(Operation::Save));
        app.finish(Operation::Live);
        assert_eq!(app.file_operation, Some(Operation::Save));
        assert!(app.workbench.control.is_none());
        app.finish(Operation::Save);
        assert!(app.start(Operation::Export));
        assert!(!app.start(Operation::Live));
    }
    #[test]
    fn late_saved_content_and_prepared_control_cannot_rebind_another_draft() {
        let rate = SampleRate::DEFAULT;
        let mut w = Workbench::new(Draft::new(), rate);
        w.draft.set_title("Snapshot N".into());
        let snapshot = w.draft.clone();
        let owner = w.identity.clone();
        w.draft.set_title("Newer N+1".into());
        assert!(w.record_save(&owner, snapshot.clone()));
        assert!(w.dirty());
        assert_eq!(w.saved.title(), "Snapshot N");
        assert_eq!(w.draft.title(), "Newer N+1");
        let mut other = Workbench::new(Draft::new(), rate);
        assert!(!other.record_save(&owner, snapshot));
        assert!(!other.dirty());
        let generator = auditorylab::LiveGenerator::new(&Draft::new(), rate).unwrap();
        assert!(!other.ready(&owner, generator.control(), rate));
        assert!(other.control.is_none());
    }
    #[test]
    fn pending_metadata_cannot_hide_an_unrelated_parser_error_and_diagnostics_are_specific() {
        let rate = SampleRate::DEFAULT;
        let mut w = Workbench::new(Draft::new(), rate);
        let id = w.draft.add(SourceKind::Tone).unwrap();
        w.edit(id, Setting::Frequency, "bad", rate);
        w.pending_metadata
            .insert("title".into(), "A pending title".into());
        assert!(w.action_draft(rate).is_none());
        assert!(
            w.field_error(id, Setting::Frequency, rate)
                .contains("finite")
        );
        assert!(w.field_error(id, Setting::Level, rate).is_empty());
        w.pending.insert((id, Setting::Frequency), "3750".into());
        let draft = w.action_draft(rate).unwrap();
        assert_eq!(draft.title(), "A pending title");
        assert!(draft.target(rate).is_ok());
        assert!(
            !w.parse_errors.is_empty(),
            "eligibility must not apply pending text to audio"
        );
    }
}
