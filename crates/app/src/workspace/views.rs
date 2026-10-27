// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! What a frontend renders: semantic rows, shared wording and action
//! availability. Building a view evaluates through the editor's caches; an
//! unchanged document is not evaluated again.
use super::{FileKind, OutputView, Refusal, View, Workspace};
use crate::document::editing::FieldHandle;
use crate::document::editing::RowKey;
use crate::document::recipe::RecipeSnapshot;
use crate::presentation::workspace_text::refusal_text;
use crate::{document::controls, presentation};
use auditorylab_core::{CarrierKind, ComponentId, Ears, ModulationKind, ParameterId, SampleRate};
use std::sync::Arc;

/// One editable quantity of a component.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterView {
    /// The field this row edits, bound to its document.
    pub handle: FieldHandle,
    /// The quantity.
    pub parameter: ParameterId,
    /// Label with unit.
    pub label: String,
    /// One plain sentence about the quantity.
    pub help: &'static str,
    /// Staged text, or the committed value in the displayed unit.
    pub text: String,
    /// Whether the text is staged and not committed.
    pub staged: bool,
    /// The slider position, for quantities with a slider.
    pub slider: Option<f64>,
    /// What is wrong with this field, if anything.
    pub diagnostic: String,
}

/// One component of the open document.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentView {
    /// Opaque identity of this rendered component lifetime.
    pub key: RowKey,
    /// The authored component identity.
    pub id: ComponentId,
    /// Ear routing.
    pub ears: Ears,
    /// False for separate-ear pairs whose routes are part of their construction.
    pub routing_editable: bool,
    /// Carrier kind.
    pub carrier: Option<CarrierKind>,
    /// Modulation kind, if any.
    pub modulation: Option<ModulationKind>,
    /// The component's role.
    pub heading: &'static str,
    /// Accessible name: role and position.
    pub name: String,
    /// The component's generated quantities at the diagnostic rate, or its error.
    pub context: String,
    /// Every problem of this component.
    pub error: String,
    /// The quantities this component has, in display order.
    pub parameters: Vec<ParameterView>,
}

/// A recipe-wide field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataView {
    /// The field, bound to its document.
    pub handle: FieldHandle,
    /// Label, with unit for quantities.
    pub label: String,
    /// Staged text, or the committed value.
    pub text: String,
    /// Whether the text is staged and not committed.
    pub staged: bool,
    /// What is wrong with the field, if anything.
    pub diagnostic: String,
}

/// The open document.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorView {
    /// Opaque identity of the rendered document lifetime.
    pub key: RowKey,
    /// Title, notes, duration and fade.
    pub metadata: [MetadataView; 4],
    /// Components in order.
    pub components: Vec<ComponentView>,
    /// Whether the document differs from what was saved.
    pub dirty: bool,
    /// The candidate's structure in one line, or why it has none.
    pub brief: String,
    /// The candidate's peak bounds or global error, with its rate context.
    pub peak: String,
    /// Recipe-wide problems: duration, fade and unparseable metadata.
    pub metadata_error: String,
    /// Short attribution to the copied source.
    pub based_on_title: String,
    /// Attribution with the source's citation and summary.
    pub based_on: String,
    /// Edits not applied to the playing sound, and why.
    pub retained_warning: String,
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
        let context = presentation::workspace_text::EditorViewContext {
            rate,
            live: self.live_rate().is_some(),
            retained_warning: self.retained_warning(
                self.editor.session.has_staged(),
                analysis.target().is_none(),
            ),
        };
        presentation::workspace_text::editor_view(
            &self.editor.session,
            &candidate,
            &analysis,
            &committed_analysis,
            &nominal,
            context,
        )
    }

    fn retained_warning(&self, pending: bool, invalid: bool) -> String {
        let unrelated = self.audio.slot.as_ref().is_some_and(|t| {
            self.view == View::Workbench
                && !matches!(t.kind, super::AudioKind::Live(d) if d == self.editor.session.document())
        });
        presentation::workspace_text::retained_warning(
            pending,
            invalid,
            self.live_rate().is_some(),
            unrelated.then(|| {
                self.audio
                    .slot
                    .as_ref()
                    .map_or("", |task| task.title.as_str())
            }),
        )
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
            library_play_label: crate::presentation::vocabulary::Action::library_play_labels()
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
                crate::presentation::vocabulary::Action::StopAudio.label()
            } else if self.view == View::Workbench {
                crate::presentation::vocabulary::Action::PlayLive.label()
            } else {
                crate::presentation::vocabulary::Action::PlaySelectedSound.label()
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
