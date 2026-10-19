//! Desktop selection, validation and operation transitions.
use auditorylab::{Origin, Preset, SampleRate, Sound};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Operation {
    Import,
    Play,
    Export,
}
pub(crate) struct App {
    pub(crate) presets: Vec<Preset>,
    pub(crate) selected: usize,
    pub(crate) rate: SampleRate,
    pub(crate) operation: Option<Operation>,
    pub(crate) closing: bool,
    pub(crate) playback: Option<Sound>,
    pub(crate) status: String,
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
        self.status.clear();
        true
    }
    pub(crate) fn finish(&mut self) {
        self.operation = None;
        self.playback = None;
    }
    pub(crate) fn imported(&mut self, result: Result<Preset, auditorylab::Error>) -> bool {
        let preset = match result {
            Ok(preset) => preset,
            Err(error) => {
                self.status = error.to_string();
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
pub(crate) fn title(preset: &Preset) -> String {
    preset
        .about()
        .map_or_else(|| preset.origin().to_string(), |a| a.title.clone())
}
pub(crate) fn label(preset: &Preset) -> String {
    format!(
        "{}{}\n{} · {} s",
        if matches!(preset.origin(), Origin::File(_)) {
            "Imported: "
        } else {
            ""
        },
        title(preset),
        preset
            .about()
            .and_then(|a| a.studied_for.as_deref())
            .unwrap_or("Custom recipe"),
        preset.stimulus().duration_seconds
    )
}
pub(crate) fn scope_start(sound: &Sound, playing: bool, position: u64, frames: u64) -> u64 {
    if playing {
        position.saturating_sub(frames)
    } else {
        sound.signal().fade_frames()
    }
}
pub(crate) fn details(preset: &Preset, rate: SampleRate) -> String {
    let mut text = String::new();
    if let Some(a) = preset.about() {
        for (name, value) in [
            ("Citation", &a.citation),
            ("Source", &a.source_url),
            ("Study", &a.study),
            ("Adaptation", &a.adaptation),
        ] {
            if let Some(value) = value {
                text.push_str(&format!("{name}: {value}\n\n"));
            }
        }
    }
    for (i, component) in preset.stimulus().components.iter().enumerate() {
        text.push_str(&format!("Component {}: {component}\n", i + 1));
    }
    match preset.at(rate) {
        Ok(sound) => {
            let s = sound.signal();
            text.push_str(&format!(
                "\n{} Hz · {} frames · {} s\nFade: {} frames\nPeak bounds: {} / {} dBFS",
                rate.hz(),
                s.frames(),
                s.duration_seconds(),
                s.fade_frames(),
                s.peak_dbfs()[0],
                s.peak_dbfs()[1]
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
}
