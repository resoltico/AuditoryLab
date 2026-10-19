//! Recipe loading, WAV publication and playback for Auditory Lab frontends.
#![forbid(unsafe_code)]
mod draft;
mod identity;
pub use identity::{BUILD_IDENTITY, BuildIdentity, SOURCE_URL, version_line};
mod error;
mod input_admission;
mod playback;
mod presentation;
mod publish;
mod wav;
pub use auditorylab_core::waveform::trace;
pub use auditorylab_core::{
    About, Carrier, Component, Ear, Ears, Extent, Modulation, SampleRate, Signal, Stimulus,
};
pub use auditorylab_core::{
    CarrierFacts, ComponentFacts, ModulationFacts, Observation, PhaseRate, PulseFacts, SlotRole,
    VoiceObservation,
};
pub use draft::{
    ComponentId, Draft, DraftCheck, ModulationKind, Range, Scale, Setting, SourceKind,
    default_component, default_modulation, range, set_setting, setting_value, slider_position,
    slider_value,
};
pub use error::{Error, ErrorKind};
pub use playback::monitor::{
    GeneratedStatus, Meter, MonitorDetail, MonitorReader, ObservationFreshness, PlaybackMonitor,
    PlaybackPhase, SessionDescriptor, sample_meters, sample_trace,
};
pub use playback::output::{OutputInfo, SampleFormat};
pub use presentation::{
    InterauralPair, catalog_matches, component_description, format_dbfs, format_hz,
    generated_description, generated_details, human_duration, interaural_pair, playback_elapsed,
    sound_brief, sound_description, window_cycle_hint,
};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
pub use wav::{Container, Exported};
/// Where a recipe was loaded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Catalog ID.
    BuiltIn(&'static str),
    /// Document path.
    File(PathBuf),
    /// Label for a stream.
    Stream(String),
    /// An editable sound title.
    Draft(String),
}
impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuiltIn(id) => write!(f, "{id}"),
            Self::File(path) => write!(f, "{path:?}"),
            Self::Stream(label) | Self::Draft(label) => write!(f, "{label}"),
        }
    }
}
/// A loaded recipe and its origin; values are checked at the chosen rate.
#[derive(Debug, Clone)]
pub struct Preset {
    origin: Origin,
    recipe: Arc<auditorylab_core::Preset>,
}
impl Preset {
    /// Recipe origin.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }
    /// Research and listening description.
    pub fn about(&self) -> Option<&About> {
        self.recipe.about.as_ref()
    }
    /// Declared sound parameters.
    pub fn stimulus(&self) -> &Stimulus {
        &self.recipe.stimulus
    }
    /// Sound title, falling back to its origin.
    pub fn title(&self) -> String {
        self.about()
            .map_or_else(|| self.origin.to_string(), |a| a.title.clone())
    }
    /// Available research text in its shared display order.
    pub fn research(&self) -> Vec<(&'static str, &str)> {
        self.about().map_or_else(Vec::new, |a| {
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
    /// Guidance shown before playback.
    pub fn notices(&self) -> Vec<Notice> {
        let mut notices = vec![Notice::LowStartingVolume];
        if self.stimulus().uses_separate_ears() {
            notices.push(Notice::HeadphonesForSeparateEars);
        }
        notices
    }
    /// Validate the recipe and realize it at a sample rate.
    pub fn at(&self, rate: SampleRate) -> Result<Sound, Error> {
        let signal = self.stimulus().signal(rate).map_err(|e| {
            Error::caused(ErrorKind::InvalidSound, format!("{}: {e}", self.origin), e)
        })?;
        Ok(Sound {
            preset: self.clone(),
            signal: Arc::new(signal),
        })
    }
}
/// An immutable, validated sound.
#[derive(Debug, Clone)]
pub struct Sound {
    preset: Preset,
    signal: Arc<Signal>,
}
impl Sound {
    /// Loaded recipe.
    pub fn preset(&self) -> &Preset {
        &self.preset
    }
    /// Seekable stereo signal.
    pub fn signal(&self) -> &Signal {
        &self.signal
    }
}
/// Load every catalog recipe in natural order.
pub fn catalog() -> Result<Vec<Preset>, Error> {
    auditorylab_core::catalog::ENTRIES
        .iter()
        .map(|e| builtin(e.id))
        .collect()
}
fn entry(id: &str) -> Result<&'static auditorylab_core::catalog::Entry, Error> {
    auditorylab_core::catalog::find(id)
        .ok_or_else(|| Error::new(ErrorKind::UnknownPreset, format!("unknown preset {id:?}")))
}
/// Load a catalog recipe by exact ID.
pub fn builtin(id: &str) -> Result<Preset, Error> {
    let e = entry(id)?;
    parse(e.text.as_bytes(), Origin::BuiltIn(e.id))
}
/// Return the byte-exact catalog recipe.
pub fn builtin_source(id: &str) -> Result<&'static str, Error> {
    Ok(entry(id)?.text)
}
fn parse(bytes: &[u8], origin: Origin) -> Result<Preset, Error> {
    let recipe = auditorylab_core::Preset::from_bytes(bytes)
        .map_err(|e| Error::caused(ErrorKind::InvalidPreset, format!("{origin}: {e}"), e))?;
    Ok(Preset {
        origin,
        recipe: Arc::new(recipe),
    })
}
fn read(reader: impl Read, origin: Origin) -> Result<Preset, Error> {
    let mut bytes = Vec::new();
    reader
        .take((auditorylab_core::Preset::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            Error::caused(
                ErrorKind::Unreadable,
                format!("cannot read {origin}: {e}"),
                e,
            )
        })?;
    parse(&bytes, origin)
}
/// Read a bounded recipe stream.
pub fn load_reader(reader: impl Read, label: &str) -> Result<Preset, Error> {
    read(reader, Origin::Stream(label.into()))
}
/// Open and read a regular recipe file without blocking on special files.
pub fn load_file(path: &Path) -> Result<Preset, Error> {
    let file = input_admission::open(path).map_err(|e| match e {
        input_admission::Failure::Io(e) => Error::caused(
            ErrorKind::Unreadable,
            format!("cannot open {path:?}: {e}"),
            e,
        ),
        input_admission::Failure::NotRegular => Error::new(
            ErrorKind::Unreadable,
            format!("cannot read {path:?}: expected a regular file"),
        ),
    })?;
    read(file, Origin::File(path.to_owned()))
}
/// Whether publication may replace an existing destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replace {
    /// Keep existing files untouched.
    Never,
    /// Atomically replace after native Save confirmation.
    Allowed,
}
/// Shared cancellation request.
#[derive(Clone, Default, Debug)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    /// Request cancellation.
    pub fn request(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    /// Whether cancellation was requested.
    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
/// Shared audible playback position or exported frame count.
#[derive(Clone, Default, Debug)]
pub struct Progress(Arc<AtomicU64>);
impl Progress {
    /// Current position in frames.
    pub fn frames(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
    pub(crate) fn set(&self, n: u64) {
        self.0.store(n, Ordering::Relaxed);
    }
}
/// Export stereo float WAV and publish the completed file atomically.
pub fn export(
    sound: &Sound,
    path: &Path,
    replace: Replace,
    cancel: &Cancel,
    progress: &Progress,
) -> Result<Exported, Error> {
    publish::publish(path, replace, cancel, |file| {
        wav::encode(sound.signal(), file, &cancel.0, progress).map_err(|e| match e {
            wav::ExportError::Cancelled => Error::new(ErrorKind::Cancelled, "export cancelled"),
            wav::ExportError::Io(e) => Error::caused(
                ErrorKind::Write,
                format!("cannot export to {path:?}: {e}"),
                e,
            ),
        })
    })
}
/// Save the original catalog TOML text.
pub fn save_builtin(id: &str, path: &Path, replace: Replace) -> Result<(), Error> {
    use std::io::Write;
    let text = builtin_source(id)?;
    publish::publish(path, replace, &Cancel::default(), |file| {
        file.write_all(text.as_bytes()).map_err(|e| {
            Error::caused(ErrorKind::Write, format!("cannot save to {path:?}: {e}"), e)
        })
    })
}
/// A device snapshot bound to a sound at that device's rate.
pub struct Playback {
    output: playback::Output,
    sound: Sound,
    descriptor: SessionDescriptor,
    monitor: Option<PlaybackMonitor>,
}
/// Discover the default output and validate the recipe at its rate.
pub fn prepare_playback(preset: &Preset) -> Result<Playback, Error> {
    let output = playback::Output::default_device().map_err(device_error)?;
    let sound = preset.at(output.description().sample_rate)?;
    let descriptor = SessionDescriptor::new(
        preset.title(),
        preset.origin().clone(),
        output.description().clone(),
        Some(sound.signal().frames()),
    );
    Ok(Playback {
        output,
        sound,
        descriptor,
        monitor: None,
    })
}
/// Completed playback statistics.
#[derive(Debug, Clone, Copy)]
pub struct Played {
    /// Signal frames played.
    pub frames: u64,
    /// Reported buffer underruns.
    pub underruns: u64,
}
fn device_error(e: playback::PlaybackError) -> Error {
    Error::caused(
        if matches!(e, playback::PlaybackError::Cancelled) {
            ErrorKind::Cancelled
        } else {
            ErrorKind::Device
        },
        e.to_string(),
        e,
    )
}
impl Playback {
    /// Captured identity independent of frontend navigation.
    pub fn descriptor(&self) -> &SessionDescriptor {
        &self.descriptor
    }
    /// Enable bounded observation before playback. The first call fixes the detail mode; later calls reuse that same monitor.
    pub fn monitor(&mut self, detail: MonitorDetail) -> PlaybackMonitor {
        self.monitor
            .get_or_insert_with(|| PlaybackMonitor::new(self.descriptor.clone(), detail, None))
            .clone()
    }

    /// Default device snapshot.
    pub fn output(&self) -> &OutputInfo {
        self.output.description()
    }
    /// Sound at the device's sample rate.
    pub fn sound(&self) -> &Sound {
        &self.sound
    }
    /// Run playback on this thread until completion or cancellation.
    pub fn play(self, cancel: &Cancel, progress: &Progress) -> Result<Played, Error> {
        self.output
            .play(self.sound.signal, &cancel.0, progress.clone(), self.monitor)
            .map_err(device_error)
    }
}
/// Shared listening guidance.
#[derive(Debug, Clone, Copy)]
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
/// Prepared live generation at a caller-supplied rate, also used by device playback.
/// Owns the validated latest-wins control boundary without requiring an audio device.
pub struct LiveGenerator {
    engine: auditorylab_core::live::Engine,
    mailbox: playback::source::Mailbox,
}
impl LiveGenerator {
    /// Prepare a live generator and its initial whole target. Allocation happens here.
    pub fn new(draft: &Draft, rate: SampleRate) -> Result<Self, Error> {
        let mailbox = Arc::new(std::sync::Mutex::new(playback::source::Updates::default()));
        let control = LiveControl {
            rate,
            mailbox: Arc::clone(&mailbox),
        };
        control.update(draft)?;
        Ok(Self {
            engine: auditorylab_core::live::Engine::new(rate),
            mailbox,
        })
    }
    /// Handle for accepted, monotonically revisioned whole-target changes.
    pub fn control(&self) -> LiveControl {
        LiveControl {
            rate: self.engine.sample_rate(),
            mailbox: Arc::clone(&self.mailbox),
        }
    }
    /// Poll once for a new target and generate a caller-owned mixed stereo buffer.
    /// Takes one nonblocking mailbox lock attempt and allocates nothing.
    pub fn render(&mut self, out: &mut [[f64; 2]]) -> usize {
        self.begin_buffer(false);
        self.engine.render(out)
    }
    /// Last-generated-frame state, without advancing the generator.
    pub fn observation(&self) -> Option<Observation> {
        self.engine.observation()
    }
    /// Finish the current transition and fade to silence; subsequent updates cannot win over Stop.
    pub fn stop(&mut self) {
        self.engine.stop();
    }
    /// Whether the generator's Stop fade or frame lifetime has ended.
    pub fn is_finished(&self) -> bool {
        self.engine.is_finished()
    }
    fn begin_buffer(&mut self, stop: bool) {
        if stop {
            self.engine.stop();
        } else if let Some((revision, target)) = self
            .mailbox
            .try_lock()
            .ok()
            .and_then(|mut state| state.pending.take())
        {
            self.engine.set_revision(&target, revision);
        }
    }
}

/// A device-bound open-ended sound with a validated target mailbox.
pub struct Live {
    output: playback::Output,
    generator: Box<LiveGenerator>,
    descriptor: SessionDescriptor,
    monitor: Option<PlaybackMonitor>,
}
/// Producer-side live updates at the device's fixed sample rate.
#[derive(Clone)]
pub struct LiveControl {
    rate: SampleRate,
    mailbox: playback::source::Mailbox,
}
impl LiveControl {
    /// Rate used to validate every update.
    pub const fn sample_rate(&self) -> SampleRate {
        self.rate
    }
    /// Validate and replace the waiting target; invalid edits leave it untouched.
    pub fn update(&self, draft: &Draft) -> Result<u64, Error> {
        let target = draft.target(self.rate)?;
        let mut updates = self
            .mailbox
            .lock()
            .map_err(|_| Error::new(ErrorKind::Device, "live control is unavailable"))?;
        if updates.last.as_ref() == Some(&target) {
            return Ok(updates.revision);
        }
        updates.revision = updates.revision.checked_add(1).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidSound,
                "live target revision limit reached",
            )
        })?;
        updates.last = Some(target);
        updates.pending = Some((updates.revision, target));
        Ok(updates.revision)
    }
}
/// Discover the default device and validate the initial draft at its rate.
pub fn prepare_live(draft: &Draft) -> Result<Live, Error> {
    let output = playback::Output::default_device().map_err(device_error)?;
    let rate = output.description().sample_rate;
    let generator = Box::new(LiveGenerator::new(draft, rate)?);
    let descriptor = SessionDescriptor::new(
        draft.title().into(),
        Origin::Draft(draft.title().into()),
        output.description().clone(),
        None,
    );
    Ok(Live {
        output,
        generator,
        descriptor,
        monitor: None,
    })
}
impl Live {
    /// Captured source and device identity.
    pub fn descriptor(&self) -> &SessionDescriptor {
        &self.descriptor
    }
    /// Enable bounded generated observation before playback. The first call fixes its detail mode.
    pub fn monitor(&mut self, detail: MonitorDetail) -> PlaybackMonitor {
        self.monitor
            .get_or_insert_with(|| {
                PlaybackMonitor::new(
                    self.descriptor.clone(),
                    detail,
                    Some(Arc::clone(&self.generator.mailbox)),
                )
            })
            .clone()
    }

    /// Snapshot of the default output used for this stream.
    pub fn output(&self) -> &OutputInfo {
        self.output.description()
    }
    /// A handle for validated updates while playback runs.
    pub fn control(&self) -> LiveControl {
        self.generator.control()
    }
    /// Play until Stop, the frame limit, or an output failure.
    pub fn play(self, stop: &Cancel, progress: &Progress) -> Result<Played, Error> {
        let source = playback::source::Source::Live(self.generator);
        self.output
            .play(source, &stop.0, progress.clone(), self.monitor)
            .map_err(device_error)
    }
}
