//! What a frontend renders: semantic rows, shared wording and action
//! availability. Building a view evaluates through the editor's caches; an
//! unchanged document is not evaluated again.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::{FileKind, OutputView, Refusal, View, Workspace};
use crate::document::editing::FieldHandle;
use crate::document::editing::RowKey;
use crate::document::recipe::RecipeSnapshot;
use crate::workspace::controls;
use auditorylab_core::{CarrierKind, ComponentId, ParameterId, SampleRate};
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
    /// Play (or replace playing audio) is admitted.
    pub can_play: bool,
    /// Label for Library playback or replacement.
    pub playing: bool,
    /// A file import may be requested.
    pub can_import: bool,
    /// The library worker can accept an operation at this configured destination.
    pub can_manage_library: bool,
    /// The selected opened input can be explicitly imported.
    pub can_add_to_library: bool,
    /// Trash can start or queue the current managed draft decision.
    pub can_request_trash: bool,
    /// A document replacement/copy may be requested (possibly queued).
    pub can_new: bool,
    /// The current selection can be copied.
    pub can_copy: bool,
    /// Actionable reason a selected source cannot be derived.
    pub copy_reason: Option<Refusal>,
    /// Bounded global refusal or task outcome beside persistent actions.
    pub operation_refusal: Option<Refusal>,
    /// A static preview may replace the last session display.
    pub can_preview: bool,
    /// Why Play is not admitted.
    pub play_reason: Option<Refusal>,
    /// Save is admitted.
    pub can_save: bool,
    /// A complete independent copy can be created and saved.
    pub can_save_copy: bool,
    /// Current portable content can be captured for an external export.
    pub can_export_recipe: bool,
    /// Why Save is not admitted.
    pub save_reason: Option<Refusal>,
    /// Export is admitted.
    pub can_export: bool,
    /// Why Export is not admitted.
    pub export_reason: Option<Refusal>,
    /// Editing commands are accepted.
    pub can_edit: bool,
    /// Another component fits in the open sound.
    pub can_add_component: bool,
    /// Adding a pair requires room for both generator lanes.
    pub can_add_pair: bool,
    /// Why a two-tone pair cannot currently be added.
    pub pair_add_reason: Option<Refusal>,
    /// Why no component can be added, when editing is otherwise allowed.
    pub add_component_reason: Option<Refusal>,
    /// The most recently removed component, which Undo remove restores;
    /// empty when there is none.
    pub removed_component: Option<auditorylab_core::Component>,
    /// Undo remove is admitted.
    pub can_restore_component: bool,
    /// A file task occupies its joined slot; prompt admission is separate.
    pub file_busy: bool,
    /// An audio task occupies its slot.
    pub playback_busy: bool,
    /// An export runs.
    pub exporting: bool,
    /// The transport button is enabled.
    pub transport_enabled: bool,
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

#[derive(Debug, PartialEq, Eq)]
pub(super) struct LibraryViewKey {
    revision: u64,
    selected: Option<crate::document::recipe::SnapshotId>,
    rate: SampleRate,
}

impl Workspace {
    /// The open document's rows, values and diagnostics. Diagnostics use
    /// the bound live session's rate when there is one, otherwise the
    /// selected export rate; each is labelled.
    pub fn editor_view(&mut self) -> EditorView {
        let rate = self.diagnostic_rate();
        let candidate = self.editor.session.candidate();
        let analysis = self.editor.session.candidate_analysis(rate);
        let committed_analysis = self.editor.session.committed_analysis(rate);
        let nominal = self.editor.session.nominal();
        let input = |field| {
            candidate
                .problems()
                .iter()
                .find(|(f, _)| *f == field)
                .map(|(f, p)| FieldDiagnostic::Input(*f, *p))
        };
        let metadata = crate::document::editing::MetadataField::ALL.map(|field| {
            let editing = crate::document::editing::EditingField::Metadata(field);
            MetadataView {
                handle: self
                    .editor
                    .session
                    .handle(editing)
                    .unwrap_or_else(|_| unreachable!("existing metadata")),
                field,
                text: self.editor.session.text(editing),
                staged: self.editor.session.staged(editing).is_some(),
                diagnostic: input(editing).or_else(|| {
                    field.parameter().and_then(|p| {
                        analysis
                            .envelope()
                            .err()
                            .filter(|e| e.parameters().contains(&p))
                            .cloned()
                            .map(FieldDiagnostic::Sound)
                    })
                }),
            }
        });
        let components =
            self.editor
                .session
                .draft()
                .components()
                .iter()
                .map(|component| {
                    use crate::document::editing::EditingField;
                    let id = component.id;
                    let committed = component.form;
                    let shown = candidate
                        .draft()
                        .component(id)
                        .copied()
                        .unwrap_or(committed);
                    let own = analysis.check().voice(id.into());
                    let acoustic = own.and_then(|r| r.as_ref().err());
                    let second = analysis
                        .check()
                        .voice(id.with_lane(auditorylab_core::Lane::Secondary))
                        .and_then(|r| r.as_ref().err());
                    let coupled = analysis.check().result().err().filter(|error| match error {
                        auditorylab_core::SoundError::Pair(auditorylab_core::PairError {
                            id: owner,
                            ..
                        }) => *owner == id,
                        auditorylab_core::SoundError::PairCarrier { id: owner, .. } => {
                            owner.component == id
                        }
                        _ => false,
                    });
                    let parameters =
                        ParameterId::COMPONENT
                            .into_iter()
                            .filter(|p| committed.get(*p).is_some())
                            .map(|parameter| {
                                let field = EditingField::Parameter(id, parameter);
                                let diagnostic = input(field)
                                    .or_else(|| {
                                        coupled
                                            .filter(|e| e.parameters().contains(&parameter))
                                            .cloned()
                                            .map(FieldDiagnostic::Sound)
                                    })
                                    .or_else(|| {
                                        let error = if parameter == ParameterId::PairSecondLevel {
                                            second
                                        } else {
                                            acoustic
                                        };
                                        error
                                    .filter(|e| {
                                        e.parameters().contains(&parameter)
                                            || matches!(
                                                parameter,
                                                ParameterId::PairFirstLevel
                                                    | ParameterId::PairSecondLevel
                                            ) && matches!(
                                                e,
                                                auditorylab_core::VoiceError::PeakDbfs { .. }
                                            )
                                    })
                                    .cloned()
                                    .map(FieldDiagnostic::Voice)
                                    });
                                ParameterView {
                                    handle: self.editor.session.handle(field).unwrap_or_else(
                                        |_| unreachable!("existing component field"),
                                    ),
                                    parameter,
                                    text: self.editor.session.text(field),
                                    staged: self.editor.session.staged(field).is_some(),
                                    slider: committed.get(parameter).and_then(|v| {
                                        controls::slider_position(parameter, rate, v.get())
                                    }),
                                    diagnostic,
                                }
                            })
                            .collect();
                    let diagnostics = candidate
                        .problems()
                        .iter()
                        .filter(|(f, _)| matches!(f,EditingField::Parameter(owner,_) if *owner==id))
                        .map(|(f, p)| FieldDiagnostic::Input(*f, *p))
                        .chain(
                            [acoustic, second]
                                .into_iter()
                                .flatten()
                                .cloned()
                                .map(FieldDiagnostic::Voice),
                        )
                        .chain(coupled.cloned().map(FieldDiagnostic::Sound))
                        .collect();
                    ComponentView {
                        key: self
                            .editor
                            .session
                            .component_key(id)
                            .unwrap_or_else(|| unreachable!("existing component lifetime")),
                        id,
                        committed,
                        shown,
                        facts: own.and_then(|r| r.as_ref().ok()).copied(),
                        diagnostics,
                        parameters,
                    }
                })
                .collect();
        let unrelated = self.audio.slot.as_ref().filter(|task| self.view == View::Workbench
            && !matches!(task.kind, super::AudioKind::Live(d) if d == self.editor.session.document()));
        EditorView {
            status: if self.editor.session.dirty() {
                crate::vocabulary::EditorStatus::UnsavedChanges
            } else {
                crate::vocabulary::EditorStatus::UnmodifiedDraft
            },
            key: self.editor.session.document_key(),
            metadata,
            components,
            dirty: self.editor.session.dirty(),
            pending: self.editor.session.has_staged(),
            rate,
            live: self.live_rate().is_some(),
            target: analysis.check().result().copied().map_err(Clone::clone),
            committed: committed_analysis
                .target()
                .map(auditorylab_core::rhythm::realized),
            nominal,
            saved: self.editor.session.saved_nominal().cloned(),
            based_on: self.editor.session.draft().based_on().cloned(),
            unrelated_title: unrelated.map(|t| t.title.clone()),
        }
    }

    /// The library rows and the selected recipe at the export rate.
    #[must_use]
    pub fn library_view(&self) -> Arc<LibraryView> {
        let rate = self.editor.export_rate;
        let key = LibraryViewKey {
            revision: self.library.revision,
            selected: self.selected().map(RecipeSnapshot::id),
            rate,
        };
        if let Some((cached, view)) = &*self.library.cache.borrow()
            && *cached == key
        {
            return view.clone();
        }
        #[cfg(test)]
        self.library
            .evaluations
            .set(self.library.evaluations.get() + 1);
        let view = Arc::new(self.build_library_view());
        *self.library.cache.borrow_mut() = Some((key, view.clone()));
        view
    }

    fn build_library_view(&self) -> LibraryView {
        let (rows, selected) = self.library.rows();
        LibraryView {
            query: self.library.query.clone(),
            rows,
            selected,
            snapshot: self.selected().cloned(),
            rate: self.editor.export_rate,
            sort: self.library.query.effective_sort().0,
        }
    }

    /// Action availability, from the same admission rules the commands use.
    pub fn action_view(&mut self) -> WorkspaceActions {
        let play = self.play_readiness();
        let save = self.prepare_save();
        let save_copy = self.prepare_save_copy();
        let export_recipe = self.prepare_export_recipe();
        let export = self.prepare_export();
        let copy = self.copy_admission();
        let operation_refusal = match self.view {
            View::Workbench => save
                .as_ref()
                .err()
                .filter(|r| matches!(r, Refusal::Input(_) | Refusal::Document(_)))
                .cloned(),
            View::Library => copy
                .as_ref()
                .err()
                .filter(|r| matches!(r, Refusal::Document(_)))
                .cloned(),
        };
        let row = self.editor.session.document_key();
        let component_add = self.component_add_admission(row, CarrierKind::Tone);
        let pair_add = self.pair_add_admission(row, crate::core::PairKind::Binaural);
        let restore_component = self.restore_component_admission(row);
        let playing = self.audio.slot.is_some();
        let exporting = self.exporting();
        let progress = self.export_progress();
        let mut view = WorkspaceActions {
            operations: Vec::new(),
            can_play: play.is_ok(),
            playing,
            can_import: self.import_admission().is_ok(),
            can_manage_library: self.library_job_admission().is_ok(),
            can_add_to_library: self.prepare_add_to_library().is_ok(),
            can_request_trash: self
                .library
                .selected_key()
                .is_some_and(|key| self.trash_request_admission(key).is_ok()),
            can_new: self.start_allowed().is_ok(),
            can_copy: copy.is_ok(),
            copy_reason: copy.err(),
            operation_refusal,
            can_preview: self.preview_admission().is_ok(),
            play_reason: play.as_ref().err().cloned(),
            can_save: save.is_ok(),
            can_save_copy: save_copy.is_ok(),
            can_export_recipe: export_recipe.is_ok(),
            save_reason: save.err(),
            can_export: export.is_ok(),
            export_reason: export.err(),
            can_edit: self.can_edit(),
            can_add_component: component_add.is_ok(),
            can_add_pair: pair_add.is_ok(),
            pair_add_reason: pair_add.err(),
            add_component_reason: component_add.err(),
            removed_component: self
                .editor
                .session
                .removed()
                .map(|(_, component)| *component),
            can_restore_component: restore_component.is_ok(),
            file_busy: self.file_busy(),
            playback_busy: playing,
            exporting,
            transport_enabled: self.audio.stop_admission().is_ok() || play.is_ok(),
            transport_action: if playing {
                crate::vocabulary::Operation::Stop
            } else if self.view == View::Workbench {
                crate::vocabulary::Operation::PlayLive
            } else {
                crate::vocabulary::Operation::PlaySelectedSound
            },
            play_target: match self.view {
                View::Workbench => self.editor.session.draft().title().to_owned(),
                View::Library => self
                    .selected()
                    .map(RecipeSnapshot::title)
                    .unwrap_or_default(),
            },
            progress,
            file_status: self.files.status.clone(),
            edit_status: self.editor.status.clone(),
            audio_status: self.audio.status.clone(),
            file_activity: self
                .files
                .slot
                .as_ref()
                .map(|task| super::status::FileActivity::from_kind(&task.kind)),
        };
        view.operations = crate::vocabulary::Operation::ALL
            .into_iter()
            .map(|operation| {
                use crate::vocabulary::Operation as O;
                let enabled = match operation {
                    O::Save => self.view == View::Workbench && view.can_save,
                    O::SaveCopy => self.view == View::Workbench && view.can_save_copy,
                    O::NewSound => view.can_new,
                    O::EditCopy => view.can_copy,
                    O::OpenRecipe => view.can_import,
                    O::ExportRecipe => view.can_export_recipe,
                    O::Export => view.can_export,
                    O::Play | O::PlaySelectedSound | O::StopCurrentAndPlayRecipe => {
                        self.view == View::Library && view.can_play
                    }
                    O::PlayLive => self.view == View::Workbench && view.can_play,
                    O::Stop => view.playback_busy,
                    O::CancelExport => view.exporting,
                    O::AddComponent | O::RemoveComponent => view.can_edit,
                    O::UndoRemove => view.can_restore_component,
                    O::EditSavedRecipe | O::EditExternalFile => view.can_new,
                    O::LibraryAdd => view.can_add_to_library,
                    O::LibraryTrash => view.can_request_trash,
                    O::LibraryInit
                    | O::LibraryUpdate
                    | O::LibraryRename
                    | O::LibraryRestore
                    | O::LibraryPurge => view.can_manage_library,
                    O::List
                    | O::ListTrash
                    | O::Describe
                    | O::Inspect
                    | O::Recipe
                    | O::LibraryPath
                    | O::LibraryCheck
                    | O::LibraryRefresh
                    | O::LibraryRead
                    | O::CloseOpenFile
                    | O::ShowSaved => true,
                };
                ActionView { operation, enabled }
            })
            .collect();
        view
    }

    /// The running export's completed fraction, from 0 to 1; 0 otherwise.
    #[must_use]
    pub fn export_progress(&self) -> f64 {
        match &self.files.slot {
            Some(super::FileTask {
                kind: FileKind::Export {
                    frames, progress, ..
                },
                ..
            }) if *frames > 0 => {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "Nonnegative frame counts form a display-only ratio"
                )]
                let ratio = progress.frames() as f64 / *frames as f64;
                ratio.clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    /// The displayed capture.
    #[must_use]
    pub fn output_view(&self) -> OutputView {
        self.audio.output.view()
    }

    /// The transport and monitor text for the bound session or preview.
    #[must_use]
    pub fn transport_view(&self) -> TransportView {
        use super::monitor::{monitor_warning, output_monitor_state};
        let state = &self.audio.output;
        let reader = state.reader();
        let output = state.view();
        let monitor = reader.map(|reader| MonitorView {
            status: reader.status(),
            descriptor: reader.descriptor().clone(),
            accepted: reader.accepted().1,
            age: reader.age(),
            warning: monitor_warning(reader, state.span_ms()),
        });
        TransportView {
            state: output_monitor_state(
                reader,
                output.capture.as_deref(),
                output.held,
                self.audio.slot.is_some(),
            ),
            output,
            monitor,
            export_rate: self.editor.export_rate,
            task_title: self.audio.slot.as_ref().map(|task| task.title.clone()),
            failed_title: state.failed_title().map(str::to_owned),
            view: self.view,
            motion: state.motion(),
            hold_override: state.hold_overridden(),
            model: state.model().cloned(),
        }
    }

    /// The selected inspection and export rate's export choices.
    #[must_use]
    pub const fn export_rates() -> [SampleRate; 5] {
        controls::EXPORT_RATES
    }
}

impl std::fmt::Display for FieldDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::Code::code(self))
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
