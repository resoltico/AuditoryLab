// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Shared, backend-opaque application capabilities for auditory frontends.
//!
//! ```
//! use auditory_app::{load_bytes, Origin, SampleRate};
//! let preset = load_bytes(b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0", Origin::Reader("example".into()))?;
//! assert_eq!(preset.inspect(SampleRate::DEFAULT)?.frames(), 480);
//! # Ok::<(), auditory_app::AppError>(())
//! ```
//! ```compile_fail
//! let preset = auditory_app::load_catalog("orozco-perez-2020-binaural-7-hz").unwrap();
//! let session = preset.preset.stimulus.session(auditory_app::SampleRate::DEFAULT);
//! ```
//! ```compile_fail
//! fn leak(prepared: auditory_app::PreparedExport) { let _ = prepared.session; }
//! ```
//! ```compile_fail
//! fn leak(prepared: auditory_app::PreparedPlayback) { let _ = prepared.state; }
//! ```
//! ```compile_fail
//! fn raw(_: auditory_app::Session) {}
//! ```
//! Owned views and reports can cross a channel while execution stays on its worker:
//! ```no_run
//! use auditory_app::{AppError, Cancellation, ExportReport, ExportView, SampleRate};
//! use std::{path::PathBuf, sync::mpsc::{self, Receiver}};
//! fn export_on_worker(path: PathBuf) -> Receiver<Result<(ExportView, ExportReport), AppError>> {
//!     let (sender, receiver) = mpsc::channel();
//!     std::thread::spawn(move || {
//!         let result = (|| {
//!             let loaded = auditory_app::load_catalog("orozco-perez-2020-binaural-7-hz")?;
//!             let _inspection = loaded.inspect(SampleRate::DEFAULT)?;
//!             let prepared = loaded.prepare_export(SampleRate::DEFAULT, &path)?;
//!             let view = prepared.view().clone();
//!             let report = prepared.execute(&Cancellation::default())?;
//!             Ok((view, report))
//!         })();
//!         let _ = sender.send(result);
//!     });
//!     receiver
//! }
//! ```
//! Native playback similarly prepares and executes on the same worker; send its
//! cloned [`PlaybackView`] and owned [`PlaybackReport`], rather than the native capability.
//! Optional generated-signal observation is available before the start permit:
//! ```no_run
//! # fn prepare() -> Result<(), auditory_app::AppError> {
//! let source = auditory_app::load_catalog("chan-2022-clicks-40-hz")?;
//! let (prepared, mut observation) = source.prepare_observed_playback(0)?;
//! let view = prepared.view().clone();
//! // This example allocates no renderer buffers. A frontend supplies its own
//! // verified reserve instead of zero, then transfers `view` and `observation`
//! // to the UI while keeping `prepared` on its worker.
//! let progress = observation.poll();
//! let snapshot = observation.snapshot(std::time::Duration::from_millis(40), 512);
//! # let _ = (view, progress, snapshot); Ok(()) }
//! ```
//! ```compile_fail
//! fn raw(observation: auditory_app::SignalObservation) { let _ = observation.consumer; }
//! ```
//! ```compile_fail
//! fn relabel(snapshot: &mut auditory_app::SignalSnapshot) { snapshot.identity = auditory_app::SignalIdentity(7); }
//! ```
//!
//! Public snapshot context can be read but cannot be substituted or mutated:
//! ```compile_fail
//! fn replace(value: &mut auditory_app::DeclaredDescription) { value.origin = auditory_app::Origin::Reader("other".into()); }
//! ```
//! ```compile_fail
//! fn replace(value: &mut auditory_app::ExportView) { value.destination = "other.wav".into(); }
//! ```
//! ```compile_fail
//! fn replace(value: &mut auditory_app::PlaybackView, other: auditory_app::Facts) { value.facts = other; }
//! ```
//! ```compile_fail
//! fn replace(value: &mut auditory_app::SaveReport) { value.destination = "other.toml".into(); }
//! ```
//! ```compile_fail
//! fn replace(value: &mut auditory_app::ExportReport) { value.origin = auditory_app::Origin::Reader("other".into()); }
//! ```
//! ```compile_fail
//! fn replace(value: &mut auditory_app::PlaybackReport) { value.origin = auditory_app::Origin::Reader("other".into()); }
//! ```
//! Error classification and context are app-created and immutable:
//! ```compile_fail
//! fn relabel(error: &mut auditory_app::AppError) { error.reason = auditory_app::Reason::Cancelled; }
//! ```
//! ```compile_fail
//! fn replace(error: &mut auditory_app::AppError) { error.origin = None; }
//! ```
//! ```compile_fail
//! fn erase(error: &mut auditory_app::AppError) { error.cleanup = None; }
//! ```
mod diagnostic;
mod identity;
mod input_admission;
mod observation;
mod publication;
pub use auditory_core::{
    About, Component, ComponentError, Ear, Ears, Modulation, Parameter, SampleRate,
    SampleRateError, StimulusError,
};
use auditory_core::{Preset, Session};
use auditory_device::{Output, PlaybackError};
pub use auditory_device::{OutputDescription, Played, SampleRepresentation};
pub use auditory_wav::{Container, Exported};
pub use diagnostic::{
    AppError, CleanupFailure, DeviceDiagnostic, Diagnostic, ErrorClass, FilesystemDiagnostic,
    FilesystemPhase, FormatDiagnostic, InputDiagnostic, Reason,
};
pub use identity::{BuildIdentity, BuildKind, Product};
pub use observation::{
    ObservationError, SIGNAL_MAX_BINS, SIGNAL_MEMORY_LIMIT, SignalBin, SignalExtrema,
    SignalFreshness, SignalIdentity, SignalObservation, SignalProgress, SignalRange, SignalSeries,
    SignalSnapshot, analyze_samples,
};
use std::{
    ffi::OsStr,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Identity of the input document, without presentation quoting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Built-in catalog name.
    BuiltIn(String),
    /// Native filesystem path.
    File(PathBuf),
    /// Caller-provided stream label.
    Reader(String),
}
/// Cooperative cancellation belongs to one operation and may be shared with its caller.
#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    /// Request cancellation. Returns whether it was already requested.
    pub fn request(&self) -> bool {
        self.0.swap(true, Ordering::Relaxed)
    }
    /// Whether cancellation has been requested.
    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
/// Operation whose requested effect failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// Load and decode a recipe.
    Load,
    /// Validate a recipe at a rate.
    Inspect,
    /// Save exact catalog text.
    Save,
    /// Encode and publish WAV.
    Export,
    /// Discover or use native output.
    Playback,
}
/// Immutable declarations, usable without sample-rate validation or a native device.
/// Route categories describe component assignments, not measured ear signals.
#[derive(Debug, Clone)]
pub struct DeclaredDescription {
    origin: Origin,
    preset: Arc<Preset>,
    routes: Vec<Ears>,
    notices: Vec<Notice>,
}
impl DeclaredDescription {
    fn new(origin: Origin, preset: Preset) -> Self {
        let routes = [Ears::Both, Ears::Left, Ears::Right]
            .into_iter()
            .filter(|ears| {
                preset
                    .stimulus
                    .components
                    .iter()
                    .any(|component| component.ears() == *ears)
            })
            .collect();
        let notices = playback_notices(&preset);
        Self {
            origin,
            preset: Arc::new(preset),
            routes,
            notices,
        }
    }
    /// Original typed input identity.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }
    /// Descriptive metadata, without certification against numerical parameters.
    pub fn about(&self) -> Option<&About> {
        self.preset.about.as_ref()
    }
    /// Declared duration, even when invalid for execution.
    pub fn duration_seconds(&self) -> f64 {
        self.preset.stimulus.duration_seconds
    }
    /// Declared fade at each edge, before frame rounding.
    pub fn fade_seconds(&self) -> f64 {
        self.preset.stimulus.fade_seconds
    }
    /// Component declarations in source order.
    pub fn components(&self) -> &[Component] {
        &self.preset.stimulus.components
    }
    /// Unique assignments in canonical Both, Left, Right order.
    pub fn routes(&self) -> &[Ears] {
        &self.routes
    }
    /// Application-selected pre-listening reminders, computed without a device.
    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }
}
/// Parsed recipe with no raw execution capability exposed.
#[derive(Debug, Clone)]
pub struct LoadedPreset {
    description: DeclaredDescription,
}
impl LoadedPreset {
    /// Immutable input declarations; inspecting them does not validate or start audio.
    pub fn description(&self) -> &DeclaredDescription {
        &self.description
    }
    fn session(&self, rate: SampleRate, operation: Operation) -> Result<Session, AppError> {
        self.description
            .preset
            .stimulus
            .session(rate)
            .map_err(|e| AppError::validation(operation, self.description.origin.clone(), e))
    }
    /// Resolve immutable facts through the authoritative validation path.
    pub fn inspect(&self, rate: SampleRate) -> Result<Facts, AppError> {
        Ok(Facts::new(&self.session(rate, Operation::Inspect)?, self))
    }
    /// Bind a validated session and the exact requested native destination spelling.
    /// Preparation creates no output and does not canonicalize the path.
    pub fn prepare_export(
        &self,
        rate: SampleRate,
        path: &Path,
    ) -> Result<PreparedExport, AppError> {
        let session = self.session(rate, Operation::Export)?;
        Ok(PreparedExport {
            view: ExportView {
                facts: Facts::new(&session, self),
                destination: path.to_owned(),
            },
            session,
        })
    }
    /// Acquire one native snapshot and validate for that exact configuration.
    /// Create and consume the native capability on the same worker thread.
    pub fn prepare_playback(&self) -> Result<PreparedPlayback, AppError> {
        prepare_playback_with(self, || {
            let output = Output::default_device()?;
            let description = output.description().clone();
            Ok((output, description))
        })
        .map(|state| PreparedPlayback {
            state,
            observation: None,
        })
    }
    /// Prepare optional bounded generated-signal observation before playback.
    /// Send the handle and cloned preparation view to the UI before permitting
    /// execution. The handle has no native stream or playback control.
    /// `presentation_reserve_bytes` is the caller's independently bounded buffer
    /// reservation, composed with app-owned capacities under the product cap.
    /// The caller must verify its actual allocations and peak live copies.
    pub fn prepare_observed_playback(
        &self,
        presentation_reserve_bytes: usize,
    ) -> Result<(PreparedPlayback, SignalObservation), AppError> {
        // Reject an already excessive reservation before native discovery.
        observation::combined_capacity(0, presentation_reserve_bytes)
            .map_err(|error| AppError::observation(self.description.origin.clone(), error))?;
        let mut prepared = self.prepare_playback()?;
        let (producer, observation) = SignalObservation::new(
            self.description.clone(),
            prepared.view().device().sample_rate,
            presentation_reserve_bytes,
        )
        .map_err(|error| AppError::observation(self.description.origin.clone(), error))?;
        prepared.observation = Some(producer);
        Ok((prepared, observation))
    }
}
/// Immutable declarations and resolved facts copied from the validated session.
#[derive(Debug, Clone)]
pub struct Facts {
    description: DeclaredDescription,
    frames: u64,
    rate: SampleRate,
    fade_frames: u64,
    peaks: [f64; 2],
}
impl Facts {
    fn new(session: &Session, preset: &LoadedPreset) -> Self {
        Self {
            description: preset.description.clone(),
            frames: session.frames(),
            rate: session.sample_rate(),
            fade_frames: session.fade_frames(),
            peaks: session.peak_dbfs(),
        }
    }
    /// Input declarations belonging to these resolved facts.
    pub fn description(&self) -> &DeclaredDescription {
        &self.description
    }
    /// Frames in the validated session.
    pub fn frames(&self) -> u64 {
        self.frames
    }
    /// Actual validation rate.
    pub fn sample_rate(&self) -> SampleRate {
        self.rate
    }
    /// Fade frames per edge.
    pub fn fade_frames(&self) -> u64 {
        self.fade_frames
    }
    /// Resolved duration in seconds.
    pub fn duration_seconds(&self) -> f64 {
        seconds_in(self.frames, self.rate)
    }
    /// Resolved fade in seconds.
    pub fn fade_seconds(&self) -> f64 {
        seconds_in(self.fade_frames, self.rate)
    }
    /// Conservative per-ear peak bounds.
    pub fn peak_dbfs(&self) -> [f64; 2] {
        self.peaks
    }
}
// Facts are constructed only from validated sessions. Every frame/fade count is
// at most Session::MAX_FRAMES (2^36), so binary64 represents the integer exactly.
// Seconds remain a presentation quotient; frame counts and the rate are authoritative.
#[expect(
    clippy::cast_precision_loss,
    reason = "validated session frame counts are at most 2^36, exactly representable in binary64"
)]
fn seconds_in(frames: u64, rate: SampleRate) -> f64 {
    frames as f64 / f64::from(rate.hz())
}

/// Transferable preparation snapshot bound to one export capability.
#[derive(Debug, Clone)]
pub struct ExportView {
    facts: Facts,
    destination: PathBuf,
}
impl ExportView {
    /// Session facts belonging to the export.
    pub fn facts(&self) -> &Facts {
        &self.facts
    }
    /// Exact native destination requested at preparation.
    pub fn destination(&self) -> &Path {
        &self.destination
    }
}
/// Opaque validated export capability.
pub struct PreparedExport {
    session: Session,
    view: ExportView,
}
impl PreparedExport {
    /// Immutable source, session and destination context for presentation.
    pub fn view(&self) -> &ExportView {
        &self.view
    }
    /// Encode, sync and publish at the bound destination without overwriting.
    pub fn execute(self, cancel: &Cancellation) -> Result<ExportReport, AppError> {
        let origin = self.view.facts.description.origin.clone();
        let destination = self.view.destination;
        let exported = publication::publish(
            &destination,
            Operation::Export,
            Some(origin.clone()),
            cancel,
            |file| {
                auditory_wav::encode(self.session, file, &cancel.0).map_err(|e| match e {
                    auditory_wav::ExportError::Cancelled => publication::Failure::Cancelled,
                    auditory_wav::ExportError::Io(e) => publication::Failure::Io(e),
                })
            },
        )?;
        Ok(ExportReport {
            origin,
            destination,
            exported,
        })
    }
}
/// Listener policy supplied before stream start or after completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// Start with low output volume.
    LowStartingVolume,
    /// Preserve separate-ear routing with headphones.
    HeadphonesForSeparateEars,
    /// Backend reported gaps.
    BufferUnderruns {
        /// Number reported by the backend.
        count: u64,
    },
}
fn playback_notices(preset: &Preset) -> Vec<Notice> {
    let mut notices = vec![Notice::LowStartingVolume];
    if preset.stimulus.uses_separate_ear_routing() {
        notices.push(Notice::HeadphonesForSeparateEars);
    }
    notices
}
fn completion_notices(played: Played) -> Vec<Notice> {
    if played.underruns == 0 {
        vec![]
    } else {
        vec![Notice::BufferUnderruns {
            count: played.underruns,
        }]
    }
}
fn playback_error(origin: Origin, error: PlaybackError) -> AppError {
    AppError::playback(origin, error)
}
/// Transferable snapshot of the exact device and session bound for playback.
#[derive(Debug, Clone)]
pub struct PlaybackView {
    facts: Facts,
    device: OutputDescription,
}
impl PlaybackView {
    /// Session facts belonging to this device snapshot.
    pub fn facts(&self) -> &Facts {
        &self.facts
    }
    /// Exact configuration snapshot used by execution.
    pub fn device(&self) -> &OutputDescription {
        &self.device
    }
    /// Pre-listening notices from the same declared description.
    pub fn notices(&self) -> &[Notice] {
        self.facts.description.notices()
    }
}
/// Opaque native operation. Create and consume it on the same worker thread.
pub struct PreparedPlayback {
    state: PlaybackState<Output>,
    observation: Option<auditory_device::ObservationProducer>,
}
struct PlaybackState<O> {
    output: O,
    session: Session,
    view: PlaybackView,
}
fn prepare_playback_with<O>(
    preset: &LoadedPreset,
    discover: impl FnOnce() -> Result<(O, OutputDescription), PlaybackError>,
) -> Result<PlaybackState<O>, AppError> {
    let (output, device) =
        discover().map_err(|e| playback_error(preset.description.origin.clone(), e))?;
    let session = preset.session(device.sample_rate, Operation::Playback)?;
    Ok(PlaybackState {
        view: PlaybackView {
            facts: Facts::new(&session, preset),
            device,
        },
        output,
        session,
    })
}
impl<O> PlaybackState<O> {
    fn execute_with(
        self,
        cancel: &Cancellation,
        play: impl FnOnce(O, Session, &AtomicBool) -> Result<Played, PlaybackError>,
    ) -> Result<PlaybackReport, AppError> {
        let origin = self.view.facts.description.origin;
        let played = play(self.output, self.session, &cancel.0)
            .map_err(|e| playback_error(origin.clone(), e))?;
        Ok(PlaybackReport {
            origin,
            played,
            notices: completion_notices(played),
        })
    }
}
impl PreparedPlayback {
    /// Immutable preparation context that can be cloned and sent to a UI thread.
    pub fn view(&self) -> &PlaybackView {
        &self.state.view
    }
    /// Consume the same snapshot and return owned completion context.
    pub fn execute(self, cancel: &Cancellation) -> Result<PlaybackReport, AppError> {
        let observation = self.observation;
        self.state.execute_with(cancel, |output, session, flag| {
            output.play(session, flag, observation)
        })
    }
}
/// Immutable completion context for a saved exact catalog recipe.
#[derive(Debug, Clone)]
pub struct SaveReport {
    origin: Origin,
    destination: PathBuf,
}
impl SaveReport {
    /// Built-in source of the saved bytes.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }
    /// Requested destination that was published.
    pub fn destination(&self) -> &Path {
        &self.destination
    }
}
/// Immutable completion context and encoder facts for a published WAV.
#[derive(Debug, Clone)]
pub struct ExportReport {
    origin: Origin,
    destination: PathBuf,
    exported: Exported,
}
impl ExportReport {
    /// Source of the encoded session.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }
    /// Requested destination that was published.
    pub fn destination(&self) -> &Path {
        &self.destination
    }
    /// Encoder result, without another copy of the resolved session facts.
    pub fn exported(&self) -> Exported {
        self.exported
    }
}
/// Immutable playback completion context, backend facts and typed notices.
#[derive(Debug, Clone)]
pub struct PlaybackReport {
    origin: Origin,
    played: Played,
    notices: Vec<Notice>,
}
impl PlaybackReport {
    /// Source of the played session.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }
    /// Backend result, without another copy of the resolved session facts.
    pub fn played(&self) -> Played {
        self.played
    }
    /// Application-selected completion notices.
    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }
}
/// Enumerate immutable declared descriptions in natural catalog order.
pub fn catalog() -> Result<Vec<DeclaredDescription>, AppError> {
    auditory_core::catalog::ENTRIES
        .iter()
        .map(|entry| load_catalog(entry.name).map(|loaded| loaded.description))
        .collect()
}
/// Exact UTF-8 catalog source.
pub fn catalog_source(name: impl AsRef<OsStr>) -> Result<&'static str, AppError> {
    let name = name.as_ref();
    name.to_str()
        .and_then(auditory_core::catalog::find)
        .map(|e| e.text)
        .ok_or_else(|| AppError::unknown_catalog(name))
}
/// Load a catalog recipe.
pub fn load_catalog(name: impl AsRef<OsStr>) -> Result<LoadedPreset, AppError> {
    let name = name.as_ref();
    load_bytes(
        catalog_source(name)?.as_bytes(),
        Origin::BuiltIn(name.to_string_lossy().into_owned()),
    )
}
/// Load a regular file, following regular-file symlinks and retaining its exact identity.
///
/// Special files are rejected before reading. This is not an I/O deadline for
/// regular files on stalled filesystems; use [`load_reader`] for intentional streams.
pub fn load_file(path: impl AsRef<Path>) -> Result<LoadedPreset, AppError> {
    let path = path.as_ref();
    let origin = Origin::File(path.to_owned());
    let file = input_admission::open(path).map_err(|error| match error {
        input_admission::Failure::NotRegular => AppError::not_regular_file(origin.clone()),
        input_admission::Failure::Io(error) => AppError::filesystem(
            Operation::Load,
            Some(origin.clone()),
            None,
            FilesystemPhase::OpenInput,
            Some(path),
            None,
            error,
        ),
    })?;
    load_reader(file, origin)
}
/// Read at most the strict input limit plus one byte before decoding.
pub fn load_reader(reader: impl Read, origin: Origin) -> Result<LoadedPreset, AppError> {
    let mut bytes = Vec::new();
    reader
        .take(Preset::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            AppError::filesystem(
                Operation::Load,
                Some(origin.clone()),
                None,
                FilesystemPhase::ReadInput,
                match &origin {
                    Origin::File(path) => Some(path.as_path()),
                    _ => None,
                },
                None,
                error,
            )
        })?;
    load_bytes(&bytes, origin)
}
/// Decode bytes using the same strict limit and schema contract as stream input.
pub fn load_bytes(bytes: &[u8], origin: Origin) -> Result<LoadedPreset, AppError> {
    let preset =
        Preset::from_bytes(bytes).map_err(|error| AppError::input(origin.clone(), bytes, error))?;
    Ok(LoadedPreset {
        description: DeclaredDescription::new(origin, preset),
    })
}
/// Publish exact catalog text through the shared staged publisher.
pub fn save_recipe(
    name: impl AsRef<OsStr>,
    path: &Path,
    cancel: &Cancellation,
) -> Result<SaveReport, AppError> {
    let name = name.as_ref();
    let text = catalog_source(name)?;
    let origin = Origin::BuiltIn(name.to_string_lossy().into_owned());
    publication::publish(
        path,
        Operation::Save,
        Some(origin.clone()),
        cancel,
        |file| {
            file.write_all(text.as_bytes())
                .map_err(publication::Failure::Io)
        },
    )?;
    Ok(SaveReport {
        origin,
        destination: path.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notices_are_application_policy_for_silence_both_and_separate_routing() {
        for (ears, headphones) in [
            (None, false),
            (Some("both"), false),
            (Some("left"), true),
            (Some("right"), true),
        ] {
            let mut text = "schema_version=2\nduration_seconds=0.01\nfade_seconds=0\n".to_owned();
            if let Some(ears) = ears {
                text.push_str(&format!(
                    "[[component]]\nkind='tone'\nears='{ears}'\nfrequency_hz=100\npeak_dbfs=-20\n"
                ));
            }
            let loaded = load_bytes(text.as_bytes(), Origin::Reader("policy".into())).unwrap();
            assert_eq!(
                loaded.description().notices(),
                if headphones {
                    vec![Notice::LowStartingVolume, Notice::HeadphonesForSeparateEars]
                } else {
                    vec![Notice::LowStartingVolume]
                }
            );
        }
        let played = Played {
            frames: 1,
            sample_rate: SampleRate::DEFAULT,
            representation: SampleRepresentation::F32,
            underruns: 0,
        };
        assert!(completion_notices(played).is_empty());
        assert_eq!(
            completion_notices(Played {
                underruns: 7,
                ..played
            }),
            vec![Notice::BufferUnderruns { count: 7 }]
        );
    }
    #[test]
    fn playback_preparation_binds_one_snapshot_and_validation_precedes_start() {
        use std::cell::Cell;
        let discovered = Cell::new(0);
        let started = Cell::new(0);
        let discover = || {
            discovered.set(discovered.get() + 1);
            Ok((
                42,
                OutputDescription {
                    name: Some("snapshot".into()),
                    channels: 2,
                    sample_rate: SampleRate::new(8000).unwrap(),
                    representation: SampleRepresentation::F32,
                },
            ))
        };
        let preset = load_bytes(
            b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0",
            Origin::Reader("test".into()),
        )
        .unwrap();
        let state = prepare_playback_with(&preset, discover).unwrap();
        assert_eq!(state.view.facts().frames(), 80);
        assert_eq!(state.view.device.name.as_deref(), Some("snapshot"));
        let report = state
            .execute_with(&Cancellation::default(), |snapshot, session, _| {
                assert_eq!(snapshot, 42);
                started.set(started.get() + 1);
                assert_eq!(session.sample_rate().hz(), 8000);
                Ok(Played {
                    frames: session.frames(),
                    sample_rate: session.sample_rate(),
                    representation: SampleRepresentation::F32,
                    underruns: 2,
                })
            })
            .unwrap();
        assert_eq!(discovered.get(), 1);
        assert_eq!(started.get(), 1);
        assert_eq!(report.notices(), vec![Notice::BufferUnderruns { count: 2 }]);
        let invalid = load_bytes(
            b"schema_version=2\nduration_seconds=-1\nfade_seconds=0",
            Origin::Reader("test".into()),
        )
        .unwrap();
        assert_eq!(
            prepare_playback_with(&invalid, discover)
                .err()
                .unwrap()
                .class(),
            ErrorClass::InvalidInput
        );
        assert_eq!(discovered.get(), 2);
        assert_eq!(started.get(), 1);
    }
    #[test]
    fn playback_view_preserves_missing_and_hostile_device_names_and_extra_channels() {
        let loaded = load_bytes(
            b"schema_version=2\nduration_seconds=0.01\nfade_seconds=0",
            Origin::Reader("source".into()),
        )
        .unwrap();
        for name in [None, Some("hostile\n\u{1b}[2J".into())] {
            let state = prepare_playback_with(&loaded, || {
                Ok((
                    (),
                    OutputDescription {
                        name: name.clone(),
                        channels: 5,
                        sample_rate: SampleRate::DEFAULT,
                        representation: SampleRepresentation::F32,
                    },
                ))
            })
            .unwrap();
            let view = state.view.clone();
            assert_eq!(view.device().name, name);
            assert_eq!(view.device().channels, 5);
            assert_eq!(
                view.facts().description().origin(),
                loaded.description().origin()
            );
            assert_eq!(view.notices(), loaded.description().notices());
            let report = state
                .execute_with(&Cancellation::default(), |(), session, _| {
                    Ok(Played {
                        frames: session.frames(),
                        sample_rate: session.sample_rate(),
                        representation: SampleRepresentation::F32,
                        underruns: 0,
                    })
                })
                .unwrap();
            assert_eq!(report.origin(), view.facts().description().origin());
            assert!(report.notices().is_empty());
            assert_eq!(report.played().frames, view.facts().frames());
        }
    }
    #[test]
    fn derived_fact_seconds_match_session_getters_at_frame_and_rate_boundaries() {
        for hz in [SampleRate::MIN_HZ, 8000, 44100, 48000, SampleRate::MAX_HZ] {
            let rate = SampleRate::new(hz).unwrap();
            for frames in [1, 2, 3, 4097, Session::MAX_FRAMES] {
                let duration = seconds_in(frames, rate);
                let fade = seconds_in((frames - 1) / 3, rate);
                let text =
                    format!("schema_version=2\nduration_seconds={duration}\nfade_seconds={fade}");
                let loaded =
                    load_bytes(text.as_bytes(), Origin::Reader("boundary".into())).unwrap();
                let session = loaded.session(rate, Operation::Inspect).unwrap();
                let facts = loaded.inspect(rate).unwrap();
                assert_eq!(facts.frames(), session.frames());
                assert_eq!(facts.fade_frames(), session.fade_frames());
                assert_eq!(
                    facts.duration_seconds().to_bits(),
                    session.duration_seconds().to_bits(),
                    "frames={frames}, hz={hz}"
                );
                assert_eq!(
                    facts.fade_seconds().to_bits(),
                    session.fade_seconds().to_bits(),
                    "frames={frames}, hz={hz}"
                );
            }
        }
    }
}
