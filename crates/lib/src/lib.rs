//! Shared application capabilities for Auditory Lab frontends.
//!
//! The library loads immutable [`RecipeSnapshot`]s, edits documents through
//! [`EditorSession`], publishes completed files, exports WAV, and runs
//! portable playback [`session`]s whose device output is the optional
//! `device-playback` feature. [`Workspace`] combines them for interactive
//! frontends; command-line use calls the same functions directly.
#![forbid(unsafe_code)]
mod controls;
#[cfg(feature = "device-playback")]
pub mod device;
mod editing;
mod error;
mod identity;
mod input_admission;
mod presentation;
mod publish;
mod recipe;
pub mod session;
mod wav;
mod waveform_view;
pub mod workspace;

pub use auditorylab_core::interpretation::{Rhythm, RhythmKind, Structure, ToneRelationship};
pub use auditorylab_core::timing::{ModelKind, ModelLane, TimingModel};
pub use auditorylab_core::{
    About, Carrier, CarrierFacts, CarrierKind, Component, ComponentError, ComponentFacts,
    ComponentId, ComponentRole, Ear, Ears, Extent, Modulation, ModulationFacts, ModulationKind,
    Observation, ParameterDescriptor, ParameterId, PhaseRate, PulseFacts, Recipe, Research,
    SampleRate, Signal, SlotRole, Source, Stimulus, StimulusError, Target, TargetCheck, Unit,
    VoiceObservation,
};
pub use controls::{
    DEFAULT_MONITOR_SCALE, DEFAULT_MONITOR_SPAN_MS, EXPORT_RATES, MONITOR_SCALES, MONITOR_SPANS_MS,
    Range, Scale, default_carrier, default_component, default_modulation, quantize, range,
    slider_position, slider_value,
};
pub use editing::{
    Analysis, Candidate, CommitRefusal, DocumentId, Draft, EditRefusal, EditingField,
    EditorSession, FieldHandle, InputProblem, MetadataField, SaveRefusal, save_content,
};
pub use error::{Error, ErrorKind};
pub use identity::{BUILD_IDENTITY, BuildIdentity, SOURCE_URL, version_line};
pub use presentation::{
    LIBRARY_PLAY_LABELS, Notice, TRANSPORT_LABELS, axis_limit, capture_error_text, catalog_listing,
    clock, compact_number, component_description, describe, editable_number, format_dbfs,
    generated_description, generated_details, human_duration, inspection, invalid_at_rate, labeled,
    library_label, notices, playback_elapsed, playback_preamble, playback_result, playback_status,
    recipe_details, research, sound_brief, sound_description, structure_description,
    window_cycle_hint,
};
pub use recipe::{
    Origin, RecipeSnapshot, SnapshotId, Sound, builtin, builtin_source, catalog, load_file,
    load_reader,
};
pub use session::{
    EndReason, GeneratedStatus, LiveControl, LiveGenerator, LiveUpdateError, Meter, MonitorDetail,
    MonitorReader, ObservationFreshness, OutputInfo, PlaybackControl, PlaybackMonitor,
    PlaybackPhase, PlaybackReport, Preparation, SampleFormat, Session, SessionDescriptor,
    SessionId, StopRequest, sample_meters,
};
pub use wav::{Container, Exported};
pub use waveform_view::{
    CaptureError, ModelSource, ModelView, MonitorWarning, OutputMonitorState, WaveformCapture,
    WaveformScale, monitor_warning, output_monitor_state, output_underrun_warning, phase_text,
};
pub use workspace::Workspace;

use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// Whether publication may replace an existing destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replace {
    /// Keep existing files untouched.
    Never,
    /// Atomically replace, after the person confirmed it.
    Allowed,
}

/// A file export's cancellation request; audio sessions have their own Stop.
#[derive(Clone, Default, Debug)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// Request cancellation.
    pub fn request(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether cancellation was requested.
    #[must_use]
    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A file export's written frame count.
#[derive(Clone, Default, Debug)]
pub struct Progress(Arc<AtomicU64>);

impl Progress {
    /// Frames written so far.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    pub(crate) fn set(&self, n: u64) {
        self.0.store(n, Ordering::Relaxed);
    }
}

/// Export stereo float WAV, sync it and publish the completed file.
///
/// Confirmed replacement is atomic. No-overwrite publication preserves an
/// existing destination but is not guaranteed atomic on every platform.
///
/// # Errors
/// Returns [`ErrorKind::DestinationExists`], [`ErrorKind::Cancelled`] or
/// [`ErrorKind::Write`]. A cancellation requested after the file was
/// published does not unpublish it.
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

/// Save a built-in recipe's exact text, comments included.
///
/// # Errors
/// Returns [`ErrorKind::UnknownRecipe`], [`ErrorKind::DestinationExists`]
/// under [`Replace::Never`], or [`ErrorKind::Write`].
pub fn save_builtin(id: &str, path: &Path, replace: Replace) -> Result<(), Error> {
    use std::io::Write;
    let text = builtin_source(id)?;
    publish::publish(path, replace, &Cancel::default(), |file| {
        file.write_all(text.as_bytes()).map_err(|e| {
            Error::caused(ErrorKind::Write, format!("cannot save to {path:?}: {e}"), e)
        })
    })
}
