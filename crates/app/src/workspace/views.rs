//! What a frontend renders: semantic rows, shared wording and action
//! availability. Building a view evaluates through the editor's caches; an
//! unchanged document is not evaluated again.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::{Refusal, View};
use crate::document::editing::FieldHandle;
use crate::document::editing::RowKey;
use crate::document::recipe::RecipeSnapshot;
use auditorylab_core::{ComponentId, ParameterId, SampleRate};
use std::sync::Arc;

/// A field diagnostic selected against the current candidate.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldDiagnostic {
    /// Input remains staged and correctable.
    Input(
        crate::document::editing::EditingField,
        crate::document::editing::InputProblem,
    ),
    /// One generator lane is invalid at the diagnostic rate.
    Voice(auditorylab_core::VoiceError),
    /// An envelope or coupled constraint is invalid.
    Sound(auditorylab_core::SoundError),
}
/// One applicable component quantity and its current input state.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterView {
    /// Captured document and component lifetime.
    pub handle: FieldHandle,
    pub parameter: ParameterId,
    /// Staged input or committed input grammar.
    pub text: String,
    /// Whether input has been staged.
    pub staged: bool,
    /// Display-only slider position.
    pub slider: Option<f64>,
    /// Selected cause, without wording.
    pub diagnostic: Option<FieldDiagnostic>,
}
/// Authored component and applicable editable quantities.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentView {
    /// Captured component lifetime.
    pub key: RowKey,
    /// Authored owner.
    pub id: ComponentId,
    /// Committed structure, which owns the applicable fields.
    pub committed: auditorylab_core::ComponentForm,
    /// Candidate quantities where input can be parsed.
    pub shown: auditorylab_core::ComponentForm,
    /// Evaluated primary lane.
    pub facts: Option<auditorylab_core::VoiceFacts>,
    /// Component causes, before presentation deduplication.
    pub diagnostics: Vec<FieldDiagnostic>,
    /// Applicable fields in domain order.
    pub parameters: Vec<ParameterView>,
}
/// Recipe-wide input and its selected cause.
#[derive(Debug, Clone, PartialEq)]
pub struct MetadataView {
    /// Captured document lifetime.
    pub handle: FieldHandle,
    /// Domain metadata field.
    pub field: crate::document::editing::MetadataField,
    /// Staged input or committed content.
    pub text: String,
    /// Whether input has been staged.
    pub staged: bool,
    /// Selected cause, without wording.
    pub diagnostic: Option<FieldDiagnostic>,
}
/// Current document inputs and cached analysis, without rendered wording.
#[derive(Debug, Clone)]
pub struct EditorView {
    pub status: crate::vocabulary::EditorStatus,
    /// Captured document lifetime.
    pub key: RowKey,
    /// Title, notes, duration and fade.
    pub metadata: [MetadataView; 4],
    /// Authored components in order.
    pub components: Vec<ComponentView>,
    /// Whether content differs from the saved baseline.
    pub dirty: bool,
    /// Whether any input remains staged.
    pub pending: bool,
    /// Diagnostic sample rate.
    pub rate: SampleRate,
    /// Whether diagnostics use a bound live device.
    pub live: bool,
    /// Candidate target result.
    pub target: Result<auditorylab_core::Target, auditorylab_core::SoundError>,
    /// Committed generated structure.
    pub committed: Option<auditorylab_core::rhythm::Structure>,
    /// Current committed nominal classification.
    pub nominal: Arc<auditorylab_core::rhythm::Analysis>,
    /// Saved nominal classification, when a baseline exists.
    pub saved: Option<auditorylab_core::rhythm::Analysis>,
    /// Copied source retained as authored context.
    pub based_on: Option<RecipeSnapshot>,
    /// An unrelated currently playing source.
    pub unrelated_title: Option<String>,
}

/// A semantic library row, including nonselectable inventory observations.
#[derive(Debug, Clone)]
pub enum LibraryRowView {
    /// One unique indexed recipe at its last observed version.
    Entry {
        /// Captured listing identity and observed membership/version.
        handle: super::EntryHandle,
        /// Correlated indexed content and membership.
        entry: Box<crate::discovery::collection::EntrySummary>,
        /// The row requires a fresh read before an action can use it.
        requires_read: bool,
    },
    /// A group heading may repeat recipe keys across memberships.
    Group(crate::discovery::collection::ResultGroup),
    /// Heading introducing unreadable inventory records.
    Unreadable,
    /// Retained inventory cause, excluded from sound counts.
    Diagnostic(crate::services::library::LibraryDiagnostic),
}
/// Query results and selected immutable source at one export rate.
#[derive(Debug, Clone)]
pub struct LibraryView {
    /// Captured query semantics, independent of frontend option ordering.
    pub query: crate::discovery::collection::CatalogQuery,
    /// Semantic rows in result order.
    pub rows: Vec<LibraryRowView>,
    /// Selected row position, when visible.
    pub selected: Option<usize>,
    /// Selected immutable source, if readable.
    pub snapshot: Option<RecipeSnapshot>,
    /// Rate used for inspection and export.
    pub rate: SampleRate,
    /// Effective ordering used by the indexed rows.
    pub sort: crate::discovery::query::SortBy,
}
/// Disjunctive navigation membership whose count is independent of wording.
#[derive(Debug, Clone, Copy)]
pub enum FacetKind {
    /// Every sound after non-rhythm restrictions.
    AllSounds,
    /// Membership in a disclosed reference interval.
    Band(auditorylab_core::rhythm::Band),
    /// Completeness or multiplicity of declared rates.
    State(crate::discovery::query::RhythmState),
    /// Every method after the other restrictions.
    AllMethods,
    /// One declared structural method.
    Method(auditorylab_core::rhythm::Method),
}
impl FacetKind {
    /// Semantic option key, independent of count presentation and row order.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::AllSounds | Self::AllMethods => "any",
            Self::Band(value) => value.key(),
            Self::State(value) => value.key(),
            Self::Method(value) => value.key(),
        }
    }
}
/// Unique count for one navigation membership.
#[derive(Debug, Clone, Copy)]
pub struct LibraryFacetView {
    /// Membership used by the query.
    pub kind: FacetKind,
    /// Unique indexed sounds matching this membership.
    pub count: usize,
    /// Whether the inventory established a complete count.
    pub complete: bool,
}
/// Rhythm and method navigation counts in shared vocabulary order.
#[derive(Debug, Clone)]
pub struct FacetCounts {
    /// Band and rhythm-state memberships.
    pub bands: Vec<LibraryFacetView>,
    /// Structural method memberships.
    pub methods: Vec<LibraryFacetView>,
}

/// Which actions are available and why not.
#[derive(Debug, Clone)]
pub struct ActionView {
    pub operation: crate::vocabulary::Operation,
    pub enabled: bool,
}
/// Workspace feedback and the offered operations at one projection.
#[derive(Debug, Clone)]
pub struct WorkspaceActions {
    pub operations: Vec<ActionView>,
    pub import: Result<(), Refusal>,
    pub library: Result<(), Refusal>,
    pub add_to_library: Result<(), Refusal>,
    pub trash: Result<(), Refusal>,
    pub start: Result<(), Refusal>,
    pub preview: Result<(), Refusal>,
    pub save_copy: Result<(), Refusal>,
    pub export_recipe: Result<(), Refusal>,
    pub editing: Result<(), Refusal>,
    pub restore_component: Result<(), Refusal>,
    /// Label for Library playback or replacement.
    pub playing: bool,
    /// Actionable reason a selected source cannot be derived.
    pub copy_reason: Option<Refusal>,
    /// Bounded global refusal or task outcome beside persistent actions.
    pub operation_refusal: Option<Refusal>,
    /// Why Play is not admitted.
    pub play_reason: Option<Refusal>,
    /// Why Save is not admitted.
    pub save_reason: Option<Refusal>,
    /// Why Export is not admitted.
    pub export_reason: Option<Refusal>,
    /// Why a two-tone pair cannot currently be added.
    pub pair_add_reason: Option<Refusal>,
    /// Why no component can be added, when editing is otherwise allowed.
    pub add_component_reason: Option<Refusal>,
    /// The most recently removed component, which Undo remove restores;
    /// empty when there is none.
    pub removed_component: Option<auditorylab_core::Component>,
    /// A file task occupies its joined slot; prompt admission is separate.
    pub file_busy: bool,
    /// An export runs.
    pub exporting: bool,
    /// The transport button's label.
    pub transport_action: crate::vocabulary::Operation,
    /// What the next Play would play.
    pub play_target: String,
    /// Export progress from 0 to 1.
    pub progress: f64,
    /// File and general status.
    pub file_status: super::status::FileStatus,
    pub audio_status: super::status::AudioStatus,
    /// What the running file task does.
    pub file_activity: Option<super::status::FileActivity>,
    /// Most recent editor admission result, independent of file worker activity.
    pub edit_status: super::status::EditStatus,
}

/// Immutable generated-session facts used by the transport projection.
#[derive(Debug, Clone)]
pub struct MonitorView {
    /// Lifecycle, counters and last published observation.
    pub status: crate::audio::session::GeneratedStatus,
    /// Source identity and actual output configuration.
    pub descriptor: crate::audio::session::SessionDescriptor,
    /// Last accepted target, including after a session ends.
    pub accepted: auditorylab_core::Target,
    /// Age of the last changed publication.
    pub age: Option<std::time::Duration>,
    /// Current monitoring impairment.
    pub warning: Option<super::monitor::MonitorWarning>,
}
/// Transport, capture and model facts captured in one coordinator projection.
#[derive(Debug, Clone)]
pub struct TransportView {
    /// Currently displayed capture and any boundary failure.
    pub output: super::OutputView,
    /// Bound session facts, when preparation has completed.
    pub monitor: Option<MonitorView>,
    /// Current output display lifecycle.
    pub state: super::monitor::OutputMonitorState,
    /// Selected rate for static previews and exports.
    pub export_rate: SampleRate,
    /// Source title while the audio task occupies its slot.
    pub task_title: Option<String>,
    /// Title retained after failed playback preparation.
    pub failed_title: Option<String>,
    /// Selected editing surface, independent of the playing source.
    pub view: View,
    /// Effective system motion preference.
    pub motion: super::MotionPreference,
    /// Explicit user hold choice, if present.
    pub hold_override: Option<bool>,
    /// Validated timing model, independent of observed generation.
    pub model: Option<Arc<super::monitor::ModelView>>,
}

impl std::fmt::Display for FieldDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}
impl std::error::Error for FieldDiagnostic {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Input(_, cause) => cause,
            Self::Voice(cause) => cause,
            Self::Sound(cause) => cause,
        })
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    FieldDiagnostic {
        codes {}
        delegates {
            Self::Input(_, cause) => cause,
            Self::Voice(cause) => cause,
            Self::Sound(cause) => cause,
        }
    }
}

impl WorkspaceActions {
    pub fn can_play(&self) -> bool {
        self.play_reason.is_none()
    }
    pub fn can_import(&self) -> bool {
        self.import.is_ok()
    }
    pub fn can_manage_library(&self) -> bool {
        self.library.is_ok()
    }
    pub fn can_add_to_library(&self) -> bool {
        self.add_to_library.is_ok()
    }
    pub fn can_request_trash(&self) -> bool {
        self.trash.is_ok()
    }
    pub fn can_new(&self) -> bool {
        self.start.is_ok()
    }
    pub fn can_copy(&self) -> bool {
        self.copy_reason.is_none()
    }
    pub fn can_preview(&self) -> bool {
        self.preview.is_ok()
    }
    pub fn can_save(&self) -> bool {
        self.save_reason.is_none()
    }
    pub fn can_save_copy(&self) -> bool {
        self.save_copy.is_ok()
    }
    pub fn can_export_recipe(&self) -> bool {
        self.export_recipe.is_ok()
    }
    pub fn can_export(&self) -> bool {
        self.export_reason.is_none()
    }
    pub fn can_edit(&self) -> bool {
        self.editing.is_ok()
    }
    pub fn can_add_component(&self) -> bool {
        self.add_component_reason.is_none()
    }
    pub fn can_add_pair(&self) -> bool {
        self.pair_add_reason.is_none()
    }
    pub fn can_restore_component(&self) -> bool {
        self.restore_component.is_ok()
    }
    pub fn playback_busy(&self) -> bool {
        self.playing
    }
    pub fn transport_enabled(&self) -> bool {
        self.playing || self.can_play()
    }
}
