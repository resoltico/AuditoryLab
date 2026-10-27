//! What a frontend renders: semantic rows, shared wording and action
//! availability. Building a view evaluates through the editor's caches; an
//! unchanged document is not evaluated again.
// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use super::{FileKind, OutputView, Refusal, View, Workspace};
use crate::document::editing::FieldHandle;
use crate::document::editing::RowKey;
use crate::document::recipe::RecipeSnapshot;
use crate::presentation::workspace_text::refusal_text;
use crate::{presentation, workspace::controls};
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
    /// Domain quantity.
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

/// A rendered catalog item. Empty keys are headings/diagnostics, never sounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryRowView {
    /// Whether this is an immutable Built-in sound, rather than a heading or local record.
    pub built_in: bool,
    /// Immutable collection identity for selection and captured actions.
    pub key: String,
    /// Complete accessible description, generated from the same fields.
    pub text: String,
    /// Recipe title or nonselectable heading.
    pub title: String,
    /// Runtime ownership and read-only/last-observed status.
    pub collection: String,
    /// Nominal structural summary; no target compilation.
    pub summary: String,
    /// Independent calculated memberships, never an efficacy claim.
    pub bands: String,
    /// Requested duration in shared presentation units.
    pub duration: String,
    /// Sort-relevant date and any necessary unknown-history context.
    pub dates: String,
}

/// A navigation facet, including its disclosed interval and disjunctive count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryFacetView {
    /// Short shared name.
    pub label: String,
    /// Interval or qualification, independent of count.
    pub detail: String,
    /// Unique matching sounds; explicitly observed when inventory is incomplete.
    pub count: String,
    /// Full accessible/compact selector label.
    pub text: String,
}

/// The library and the selected recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryView {
    /// Structured rows; headers and diagnostic rows have empty keys.
    pub rows: Vec<LibraryRowView>,
    /// The selected entry's position among the rows.
    pub selected: Option<usize>,
    /// Selected title.
    pub title: String,
    /// Selected structure at the export rate.
    pub structure: String,
    /// Sample-rate-independent classification of the selected saved snapshot.
    pub classification: String,
    /// Runtime collection and read-only status of the selected record.
    pub ownership: String,
    /// What the source studied the sound for.
    pub studied: String,
    /// Duration and fade.
    pub timing: String,
    /// Plain-language summary.
    pub summary: String,
    /// Research and validated facts at the export rate.
    pub details: String,
    /// The study link, when it is a web address.
    pub study_url: Option<String>,
}

/// Which actions are available and why not.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionView {
    /// Play (or replace playing audio) is admitted.
    pub can_play: bool,
    /// Label for Library playback or replacement.
    pub library_play_label: &'static str,
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
    pub copy_reason: String,
    /// Bounded global refusal or task outcome beside persistent actions.
    pub operation_summary: String,
    /// File outcome retained alongside a current readiness refusal.
    pub operation_outcome: String,
    /// Complete selectable operation/refusal details.
    pub operation_details: String,
    /// A static preview may replace the last session display.
    pub can_preview: bool,
    /// Why Play is not admitted.
    pub play_reason: String,
    /// Save is admitted.
    pub can_save: bool,
    /// A complete independent copy can be created and saved.
    pub can_save_copy: bool,
    /// Current portable content can be captured for an external export.
    pub can_export_recipe: bool,
    /// Why Save is not admitted.
    pub save_reason: String,
    /// Export is admitted.
    pub can_export: bool,
    /// Why Export is not admitted.
    pub export_reason: String,
    /// Editing commands are accepted.
    pub can_edit: bool,
    /// Another component fits in the open sound.
    pub can_add_component: bool,
    /// Adding a pair requires room for both generator lanes.
    pub can_add_pair: bool,
    /// Why a two-tone pair cannot currently be added.
    pub pair_add_reason: String,
    /// Why no component can be added, when editing is otherwise allowed.
    pub add_component_reason: String,
    /// The most recently removed component, which Undo remove restores;
    /// empty when there is none.
    pub removed_component: String,
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
    pub transport_label: String,
    /// What the next Play would play.
    pub play_target: String,
    /// Export progress from 0 to 1.
    pub progress: f64,
    /// File and general status.
    pub file_status: String,
    /// Audio status.
    pub audio_status: String,
    /// What the running file task does.
    pub file_activity: String,
}

/// The transport and monitor text.
#[derive(Debug, Clone, PartialEq)]
pub struct TransportView {
    /// Phase, source and kind of the session, or the preview.
    pub player_title: String,
    /// Estimated output elapsed time.
    pub elapsed: String,
    /// Generated structure.
    pub generated: String,
    /// Every generated voice.
    pub generated_details: String,
    /// Device or preview rate.
    pub device: String,
    /// Monitor state label.
    pub output_state: &'static str,
    /// Receipt age.
    pub receipt_age: String,
    /// Current monitor warning and whether it is announced.
    pub warning: (String, bool),
    /// Backend underrun warning.
    pub backend_warning: String,
    /// Monitor provenance.
    pub monitor_state: String,
    /// Meters of the displayed capture.
    pub meters: String,
    /// Caption of the displayed capture.
    pub caption: String,
    /// The model caption and structure.
    pub model_caption: String,
    /// Motion policy explanation.
    pub motion: String,
    /// Whether the person overrode the motion preference.
    pub motion_overridden: bool,
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
        presentation::workspace_text::library_view(
            rows,
            selected,
            self.selected(),
            self.editor.export_rate,
        )
    }

    /// Action availability, from the same admission rules the commands use.
    pub fn action_view(&mut self) -> ActionView {
        let play = self.play_readiness();
        let save = self.prepare_save();
        let save_copy = self.prepare_save_copy();
        let export_recipe = self.prepare_export_recipe();
        let export = self.prepare_export();
        let copy = self.copy_admission();
        let refusal = match self.view {
            View::Workbench => save
                .as_ref()
                .err()
                .filter(|r| matches!(r, Refusal::Input(_) | Refusal::Document(_)))
                .map(refusal_text),
            View::Library => copy
                .as_ref()
                .err()
                .filter(|r| matches!(r, Refusal::Document(_)))
                .map(refusal_text),
        };
        let outcome_details = [
            self.files.status.description(),
            self.editor.status.description(),
            self.audio.status.description(),
        ]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
        let operation_details = refusal
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(outcome_details.as_str()))
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let compact = |text: &str| {
            let line = text.lines().next().unwrap_or("");
            let mut short: String = line.chars().take(180).collect();
            if line.chars().count() > 180 {
                short.push('…');
            }
            short
        };
        let operation_summary = compact(refusal.as_deref().unwrap_or(&outcome_details));
        let operation_outcome = if refusal.is_some() {
            compact(&outcome_details)
        } else {
            String::new()
        };
        let row = self.editor.session.document_key();
        let component_add = self.component_add_admission(row, CarrierKind::Tone);
        let pair_add = self.pair_add_admission(row, crate::core::PairKind::Binaural);
        let restore_component = self.restore_component_admission(row);
        let playing = self.audio.slot.is_some();
        let exporting = self.exporting();
        let progress = self.export_progress();
        ActionView {
            can_play: play.is_ok(),
            library_play_label: crate::presentation::vocabulary::library_play_labels()
                [usize::from(playing)],
            can_import: self.import_admission().is_ok(),
            can_manage_library: self.library_job_admission().is_ok(),
            can_add_to_library: self.prepare_add_to_library().is_ok(),
            can_request_trash: self
                .library
                .selected_key()
                .is_some_and(|key| self.trash_request_admission(key).is_ok()),
            can_new: self.start_allowed().is_ok(),
            can_copy: copy.is_ok(),
            copy_reason: copy.err().as_ref().map(refusal_text).unwrap_or_default(),
            operation_summary,
            operation_outcome,
            operation_details,
            can_preview: self.preview_admission().is_ok(),
            play_reason: play.as_ref().err().map(refusal_text).unwrap_or_default(),
            can_save: save.is_ok(),
            can_save_copy: save_copy.is_ok(),
            can_export_recipe: export_recipe.is_ok(),
            save_reason: save.err().as_ref().map(refusal_text).unwrap_or_default(),
            can_export: export.is_ok(),
            export_reason: export.err().as_ref().map(refusal_text).unwrap_or_default(),
            can_edit: self.can_edit(),
            can_add_component: component_add.is_ok(),
            can_add_pair: pair_add.is_ok(),
            pair_add_reason: pair_add
                .err()
                .as_ref()
                .map(refusal_text)
                .unwrap_or_default(),
            add_component_reason: component_add
                .err()
                .as_ref()
                .map(refusal_text)
                .unwrap_or_default(),
            removed_component: self
                .editor
                .session
                .removed()
                .map(|(_, component)| {
                    presentation::workspace_text::removed_component(component.form, component.id)
                })
                .unwrap_or_default(),
            can_restore_component: restore_component.is_ok(),
            file_busy: self.file_busy(),
            playback_busy: playing,
            exporting,
            transport_enabled: self.audio.stop_admission().is_ok() || play.is_ok(),
            transport_label: if playing {
                crate::core::vocabulary::Action::StopAudio.label()
            } else if self.view == View::Workbench {
                crate::core::vocabulary::Action::PlayLive.label()
            } else {
                crate::core::vocabulary::Action::PlaySelectedSound.label()
            }
            .into(),
            play_target: match self.view {
                View::Workbench => self.editor.session.draft().title().to_owned(),
                View::Library => self
                    .selected()
                    .map(RecipeSnapshot::title)
                    .unwrap_or_default(),
            },
            progress,
            file_status: [
                self.files.status.description(),
                self.editor.status.description(),
            ]
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
            audio_status: self.audio.status.description(),
            file_activity: presentation::workspace_text::file_activity(
                self.files.slot.as_ref().map(|task| &task.kind),
            ),
        }
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
        presentation::workspace_text::transport_view(
            &self.audio.output,
            self.editor.export_rate,
            self.audio.slot.as_ref().map(|task| task.title.as_str()),
            self.view,
        )
    }

    /// The selected inspection and export rate's export choices.
    #[must_use]
    pub const fn export_rates() -> [SampleRate; 5] {
        controls::EXPORT_RATES
    }
}
