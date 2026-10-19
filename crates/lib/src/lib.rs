//! Recipe loading, WAV publication and playback for Auditory Lab frontends.
#![forbid(unsafe_code)]
mod draft;
mod error;
mod input_admission;
mod playback;
mod publish;
mod wav;
pub use auditorylab_core::waveform::trace;
pub use auditorylab_core::{
    About, Carrier, Component, Ear, Ears, Extent, Modulation, SampleRate, Signal, Stimulus,
};
pub use draft::{ComponentId, Draft};
pub use error::{Error, ErrorKind};
pub use playback::output::{OutputInfo, SampleFormat};
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
}
impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuiltIn(id) => write!(f, "{id}"),
            Self::File(path) => write!(f, "{path:?}"),
            Self::Stream(label) => write!(f, "{label}"),
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
    auditorylab_core::catalog::find(id).ok_or_else(|| {
        Error::new(
            ErrorKind::UnknownPreset,
            format!(
                "unknown preset {id:?}; use list to see built-ins, or --file PATH to read a recipe"
            ),
        )
    })
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
}
/// Discover the default output and validate the recipe at its rate.
pub fn prepare_playback(preset: &Preset) -> Result<Playback, Error> {
    let output = playback::Output::default_device().map_err(device_error)?;
    let sound = preset.at(output.description().sample_rate)?;
    Ok(Playback { output, sound })
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
            .play(self.sound.signal, &cancel.0, progress.clone())
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
/// Product version and optional source revision.
pub fn version_line(binary: &str) -> String {
    match option_env!("AUDITORYLAB_SOURCE_REVISION").filter(|s| !s.is_empty()) {
        Some(revision) => format!(
            "{binary} {} ({})",
            env!("CARGO_PKG_VERSION"),
            &revision[..revision.len().min(12)]
        ),
        None => format!("{binary} {}", env!("CARGO_PKG_VERSION")),
    }
}

/// A device-bound open-ended sound with a validated target mailbox.
pub struct Live {
    output: playback::Output,
    control: LiveControl,
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
    pub fn update(&self, draft: &Draft) -> Result<(), Error> {
        let target = draft.target(self.rate)?;
        *self
            .mailbox
            .lock()
            .map_err(|_| Error::new(ErrorKind::Device, "live control is unavailable"))? =
            Some(target);
        Ok(())
    }
}
/// Discover the default device and validate the initial draft at its rate.
pub fn prepare_live(draft: &Draft) -> Result<Live, Error> {
    let output = playback::Output::default_device().map_err(device_error)?;
    let rate = output.description().sample_rate;
    let mailbox = Arc::new(std::sync::Mutex::new(None));
    let control = LiveControl { rate, mailbox };
    // First lock and store happen before any device callback can run.
    control.update(draft)?;
    Ok(Live { output, control })
}
impl Live {
    /// Snapshot of the default output used for this stream.
    pub fn output(&self) -> &OutputInfo {
        self.output.description()
    }
    /// A handle for validated updates while playback runs.
    pub fn control(&self) -> LiveControl {
        self.control.clone()
    }
    /// Play until Stop, the frame limit, or an output failure.
    pub fn play(self, stop: &Cancel, progress: &Progress) -> Result<Played, Error> {
        let source = playback::source::Source::Live {
            engine: Box::new(auditorylab_core::live::Engine::new(self.control.rate)),
            mailbox: self.control.mailbox,
        };
        self.output
            .play(source, &stop.0, progress.clone())
            .map_err(device_error)
    }
}
