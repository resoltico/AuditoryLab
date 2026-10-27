// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! What a frontend renders: semantic rows, shared wording and action
//! availability. Building a view evaluates through the editor's caches; an
//! unchanged document is not evaluated again.
use super::{FileKind, OutputView, Refusal, View, Workspace};
use crate::audio::session::{EndReason, PlaybackReport};
use crate::document::editing::RowKey;
use crate::document::editing::{
    EditRefusal, EditingField, FieldHandle, InputProblem, MetadataField,
};
use crate::document::recipe::RecipeSnapshot;
use crate::presentation::vocabulary::NumberStyle;
use crate::presentation::waveform_view::{
    monitor_warning, output_monitor_state, output_underrun_warning, phase_text,
};
use crate::presentation::{AudioDetails, AudioErrorText, AudioSummary, ParameterText};
use crate::storage::wav::Exported;
use crate::{document::controls, presentation};
use auditorylab_core::{CarrierKind, ComponentId, Ears, ModulationKind, ParameterId, SampleRate};
use std::path::Path;
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
    /// A file task or prompt is active; command availability remains operation-specific.
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

pub(super) fn refusal_text(refusal: &Refusal) -> String {
    match refusal {
        Refusal::LibraryUnavailable(problem) => {
            format!("User Library unavailable: {}", problem.message())
        }
        Refusal::DevicePlaybackUnavailable => {
            "Device playback is disabled for this workspace.".into()
        }
        Refusal::PromptOpen => {
            "Finish or cancel the open dialog before closing or replacing this sound.".into()
        }
        Refusal::ActionPending => {
            "Another action is waiting for audio or file work to finish.".into()
        }
        Refusal::FileBusy => "Wait for the current file operation to finish.".into(),
        Refusal::AudioPlaying => "Stop audio before exporting a WAV file.".into(),
        Refusal::Exporting => "Wait for the export to finish or cancel it before playing.".into(),
        Refusal::NoAudio => "Nothing is playing.".into(),
        Refusal::NoSelection => "No recipe matches the search; clear it or choose a recipe.".into(),
        Refusal::NoExport => "No export is running.".into(),
        Refusal::Input(problems) => input_text(problems),
        Refusal::NotFinite => "The recipe's numbers must all be finite before it can play.".into(),
        Refusal::Document(error) => error.to_string(),
        Refusal::SelectionChanged => "Open file selection changed; activate Add again.".into(),
        Refusal::TemporaryRequired => "Select an Open file to add; use Edit a copy for Built-in content.".into(),
        Refusal::PurgeSelectionChanged => "Select the current trashed version before permanent deletion.".into(),
        Refusal::UnknownLibraryAction(action) => format!("Unknown library action: {action}"),
        Refusal::DetachedDraft(key) => format!("Detached draft from {key}. Edit the restored saved entry or Save independent copy; the old identity is not recreated."),
        Refusal::ExistingSavedIdentity(_) => "Detached draft belongs to an existing saved identity. Restore/edit that version or Save independent copy.".into(),
        Refusal::SavedEntryUnreadable => "Saved entry is unreadable; the retained snapshot is for inspection. Refresh or recover an independent copy before playback.".into(),
        Refusal::RestoreBeforePlayback => "Restore this saved recipe before playback.".into(),
        Refusal::Invalid { title, rate, error } => error.as_ref().map_or_else(
            || format!("Could not prepare “{title}” at {} Hz.", rate.hz()),
            |error| (error).at_rate(title, *rate),
        ),
        Refusal::Edit(error) => error.description(),
    }
}

fn field_label(field: EditingField) -> String {
    match field {
        EditingField::Parameter(_, parameter) => parameter.descriptor().label.to_owned(),
        EditingField::Metadata(MetadataField::Title) => "Title".into(),
        EditingField::Metadata(MetadataField::Summary) => "Notes".into(),
        EditingField::Metadata(MetadataField::Duration) => {
            ParameterId::Duration.descriptor().label.into()
        }
        EditingField::Metadata(MetadataField::Fade) => ParameterId::Fade.descriptor().label.into(),
    }
}

fn problem_text(field: EditingField, problem: InputProblem) -> String {
    match problem {
        InputProblem::Unparseable => format!("{}: enter a finite number", field_label(field)),
        InputProblem::Nonfinite => format!(
            "{}: the value is not finite; enter a finite number",
            field_label(field)
        ),
    }
}

fn input_text(problems: &[(EditingField, InputProblem)]) -> String {
    let mut text = String::from("Correct these fields to continue: ");
    text.push_str(
        &problems
            .iter()
            .map(|(field, problem)| problem_text(*field, *problem))
            .collect::<Vec<_>>()
            .join("; "),
    );
    text
}

/// A path as people read it, quoted, without escapes.
fn shown(path: &Path) -> String {
    format!("“{}”", path.display())
}

pub(super) fn exported_text(path: &Path, exported: &Exported) -> String {
    let mut text = format!(
        "Exported {} frames at {} Hz to {} ({}).",
        exported.frames,
        exported.sample_rate.hz(),
        shown(path),
        exported.container
    );
    for warning in &exported.warnings {
        text.push('\n');
        text.push_str(warning);
    }
    text
}

/// How a session ended, for the status line. With `shown`, the transport
/// already names an ordinary ending, so only what it does not show remains.
pub(super) fn playback_text(report: &PlaybackReport, shown: bool) -> String {
    let mut text = String::from(match report.end {
        EndReason::Completed if shown => "",
        EndReason::Completed => "Playback complete.",
        EndReason::Stopped if report.drained && shown => "",
        EndReason::Stopped if report.drained => "Stopped.",
        EndReason::Stopped => "Stopped; queued output was cut off after the stop limit.",
        EndReason::FrameLimit => "Live playback reached the longest supported session.",
    });
    if report.underruns > 0 {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&presentation::Notice::Underruns(report.underruns).to_string());
    }
    text
}

/// How long ago the monitor last received changed samples.
pub(super) fn received_text(age: std::time::Duration, held: bool) -> String {
    let when = match age.as_secs() {
        0 => "just now".to_owned(),
        seconds @ 1..60 => format!("{seconds} s ago"),
        seconds @ 60..3600 => format!("{} min ago", seconds / 60),
        seconds => format!("{} h ago", seconds / 3600),
    };
    format!(
        "{} {when}",
        if held {
            "Current session received samples"
        } else {
            "Received samples"
        }
    )
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
        let editor = &self.editor.session;
        let problem = |field: EditingField| {
            candidate
                .problems()
                .iter()
                .find(|(f, _)| *f == field)
                .map(|(f, p)| problem_text(*f, *p))
        };
        let context = if self.live_rate().is_some() {
            format!("at the live device rate of {} Hz", rate.hz())
        } else {
            format!("at the {} Hz export rate", rate.hz())
        };
        let metadata = MetadataField::ALL.map(|field| {
            let editing = EditingField::Metadata(field);
            let diagnostic = problem(editing).unwrap_or_else(|| {
                field
                    .parameter()
                    .and_then(|p| {
                        analysis
                            .envelope()
                            .err()
                            .filter(|e| e.parameters().contains(&p))
                            .map(presentation::acoustic_errors::sound_error)
                    })
                    .unwrap_or_default()
            });
            MetadataView {
                handle: editor
                    .handle(editing)
                    .unwrap_or_else(|_| unreachable!("metadata views use existing fields")),
                label: match field.parameter() {
                    Some(p) => (p).input_label(),
                    None => field_label(editing),
                },
                text: editor.text(editing),
                staged: editor.staged(editing).is_some(),
                diagnostic,
            }
        });
        let components = editor
            .draft()
            .components()
            .iter()
            .enumerate()
            .map(|(index, component)| {
                let (id,committed)=(&component.id,&component.form);
                let shown = candidate
                    .draft()
                    .component(*id)
                    .copied()
                    .unwrap_or(*committed);
                let own = analysis.check().component((*id).into());
                let acoustic = own.and_then(|r| r.as_ref().err());
                let second=analysis.check().component(id.with_lane(auditorylab_core::Lane::Secondary)).and_then(|r|r.as_ref().err());
                let coupled=analysis.check().result().err().filter(|error|match error {auditorylab_core::SoundError::Pair(auditorylab_core::PairError {id:owner,..})=>owner.get()==id.get(),auditorylab_core::SoundError::PairCarrier {id:owner,..}=>owner.component==*id,_=>false});
                let parameters = ParameterId::COMPONENT
                    .into_iter()
                    .filter(|p| committed.read(*p).is_some())
                    .map(|parameter| {
                        let field = EditingField::Parameter(*id, parameter);
                        let diagnostic = problem(field).unwrap_or_else(|| {
                            coupled.filter(|e|e.parameters().contains(&parameter)).map(|e| sound_diagnostic(e, &context)).unwrap_or_else(|| {
                            let error=match parameter {ParameterId::PairSecondLevel=>second,_=>acoustic};
                            error
                                .filter(|e| e.parameters().contains(&parameter) || matches!(parameter,ParameterId::PairFirstLevel|ParameterId::PairSecondLevel) && matches!(e,auditorylab_core::VoiceError::PeakDbfs {..}))
                                .map(|error| component_diagnostic(error, &context))
                                .unwrap_or_default()})
                        });
                        ParameterView {
                            handle: editor.handle(field).unwrap_or_else(|_| unreachable!("parameter views use existing fields")),
                            parameter,
                            label: (parameter).input_label(),
                            help: match parameter {
                                ParameterId::Duration => "The length of saved recipes and exported audio; live playback continues until stopped.",
                                ParameterId::Fade => "The raised-cosine fade at each edge of saved recipes and exported audio.",
                                _ => parameter.descriptor().help,
                            },
                            text: editor.text(field),
                            staged: editor.staged(field).is_some(),
                            slider: committed
                                .read(parameter)
                                .and_then(|v| controls::slider_position(parameter, rate, v)),
                            diagnostic,
                        }
                    })
                    .collect::<Vec<_>>();
                let mut errors: Vec<String> = candidate
                    .problems()
                    .iter()
                    .filter(|(f, _)| matches!(f, EditingField::Parameter(owner, _) if owner == id))
                    .map(|(f, p)| problem_text(*f, *p))
                    .collect();
                for error in [acoustic,second].into_iter().flatten() {errors.push(component_diagnostic(error, &context));}
                if let Some(error)=coupled {errors.push(sound_diagnostic(error, &context));}
                errors.retain(|error| !parameters.iter().any(|parameter| parameter.diagnostic == *error));
                errors.dedup();
                ComponentView {
                    key: editor.component_key(*id).unwrap_or_else(|| unreachable!("an active component has a rendered lifetime")),
                    id: *id,
                    ears: committed.ears(),
                    routing_editable: committed.routing_editable(),
                    carrier: committed.primitive_kind(),
                    modulation: committed.modulation_kind(),
                    heading: shown.heading(),
                    name: format!("{} · component {}", shown.heading(), index + 1),
                    context: if let Some(pair)=shown.pair() {
                        format!("Requested second carrier: {} Hz · nominal difference: {} Hz · levels {} / {} dBFS",pair.first_hz+pair.offset_hz,pair.offset_hz.abs(),pair.first_peak_dbfs,pair.second_peak_dbfs)
                    } else {match own {
                        Some(Ok(facts)) => (facts).summary(),
                        // The error line states it, with its rate.
                        Some(Err(_)) | None => String::new(),
                    }},
                    error: errors.join("; "),
                    parameters,
                }
            })
            .collect();
        let pending = editor.has_staged();
        let peak = match analysis.check().result() {
            Ok(target) => format!(
                "{}Peak bounds: {} / {} dBFS {context}",
                if pending {
                    "Pending validation · "
                } else {
                    ""
                },
                NumberStyle::Decibels.format(target.peak_dbfs()[0]),
                NumberStyle::Decibels.format(target.peak_dbfs()[1])
            ),
            Err(error) if error.parameters().is_empty() => format!(
                "{} ({context})",
                presentation::acoustic_errors::sound_error(error)
            ),
            Err(_) => format!("Correct the highlighted fields to apply this sound {context}."),
        };
        let metadata_error = metadata
            .iter()
            .map(|m| m.diagnostic.as_str())
            .filter(|d| !d.is_empty())
            .collect::<Vec<_>>();
        let mut deduped: Vec<String> = Vec::new();
        for item in metadata_error {
            if !deduped.iter().any(|d| d == item) {
                deduped.push(item.to_owned());
            }
        }
        let generated_brief = committed_analysis.target().map_or_else(
            || "No valid sound model; correct the fields".into(),
            |t| crate::core::rhythm::realized(t).summary(),
        );
        let brief = format!(
            "{} classification · {}\nSaved version · {}\n{}",
            if pending { "Last committed" } else { "Draft" },
            nominal.summary(),
            editor
                .saved_nominal()
                .map_or_else(|| "Not saved".into(), AudioSummary::summary),
            generated_brief
        );
        let based_on = editor.draft().based_on();
        let retained_warning = self.retained_warning(pending, analysis.target().is_none());
        EditorView {
            key: editor.document_key(),
            metadata,
            components,
            dirty: editor.dirty(),
            brief,
            peak,
            metadata_error: deduped.join("; "),
            based_on_title: based_on.map_or_else(String::new, |p| {
                format!(
                    "Copied from “{}” · Session snapshot and ancestry in Sound settings",
                    p.title()
                )
            }),
            based_on: based_on.map_or_else(String::new, |p| {
                format!(
                    "Based on “{}” · {}\n{}\nResearch findings describe the source recipe.",
                    p.title(),
                    p.about()
                        .and_then(|a| a.research.as_ref())
                        .and_then(|a| a.citation.as_deref())
                        .unwrap_or("source recipe"),
                    p.about()
                        .and_then(|a| a
                            .summary
                            .as_deref()
                            .or_else(|| a.research.as_ref().and_then(|r| r.summary.as_deref())))
                        .unwrap_or("")
                )
            }),
            retained_warning,
        }
    }

    fn retained_warning(&self, pending: bool, invalid: bool) -> String {
        let unrelated = self.audio.slot.as_ref().is_some_and(|t| {
            self.view == View::Workbench
                && !matches!(t.kind, super::AudioKind::Live(d) if d == self.editor.session.document())
        });
        let retained = if pending {
            "Pending edits — not yet applied".to_owned()
        } else if invalid {
            if self.live_rate().is_some() {
                "Edits not applied — playing the last valid settings.".to_owned()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        if unrelated {
            let title = self.audio.slot.as_ref().map_or("", |t| t.title.as_str());
            format!(
                "Editing a separate sound; current audio remains “{title}”. Stop, then Play this sound to replace it.{}{retained}",
                if retained.is_empty() { "" } else { "\n" }
            )
        } else {
            retained
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
        let rate = self.editor.export_rate;
        let (rows, selected) = self.library.rows();
        let Some(snapshot) = self.selected() else {
            return LibraryView {
                rows,
                selected,
                title: String::new(),
                structure: String::new(),
                classification: String::new(),
                ownership: String::new(),
                studied: String::new(),
                timing: String::new(),
                summary: String::new(),
                details: String::new(),
                study_url: None,
            };
        };
        let about = snapshot.about();
        let ownership = selected
            .and_then(|index| rows.get(index))
            .map(|row| row.collection.clone())
            .unwrap_or_default();
        LibraryView {
            rows,
            selected,
            title: snapshot.title(),
            classification: format!(
                "{}\nReference: {}. This describes the sound, not measured brain activity.",
                snapshot.nominal().summary(),
                crate::core::rhythm::CONVENTION
            ),
            ownership,
            structure: snapshot
                .at(rate)
                .map_or_else(|e| e.to_string(), |s| s.brief()),
            studied: about
                .and_then(|a| a.research.as_ref())
                .and_then(|a| a.studied_for.clone())
                .unwrap_or_default(),
            timing: format!(
                "Duration {} · fade {} at each edge",
                NumberStyle::Duration.format(snapshot.sound().duration_seconds),
                NumberStyle::Duration.format(snapshot.sound().fade_seconds)
            ),
            summary: about
                .and_then(|a| {
                    a.summary
                        .clone()
                        .or_else(|| a.research.as_ref().and_then(|r| r.summary.clone()))
                })
                .unwrap_or_default(),
            details: (snapshot).details_at(rate),
            study_url: about
                .and_then(|a| a.research.as_ref())
                .and_then(|a| a.source_url.clone())
                .filter(|url| url.starts_with("https://") || url.starts_with("http://")),
        }
    }

    /// Action availability, from the same admission rules the commands use.
    pub fn action_view(&mut self) -> ActionView {
        let play = self.play_readiness();
        let save = self.prepare_save();
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
        let draft = self.editor.session.draft();
        let component_add = draft.check_addition(auditorylab_core::ComponentForm::Voice(
            controls::default_component(CarrierKind::Tone),
        ));
        let pair_add =
            draft.check_addition(controls::default_pair(auditorylab_core::PairKind::Binaural));
        let playing = self.audio.slot.is_some();
        let exporting = self.exporting();
        let progress = self.export_progress();
        ActionView {
            can_play: play.is_ok(),
            library_play_label: crate::presentation::vocabulary::Action::library_play_labels()
                [usize::from(playing)],
            can_import: self.import_admission().is_ok(),
            can_new: self.start_allowed().is_ok(),
            can_copy: copy.is_ok(),
            copy_reason: copy.err().as_ref().map(refusal_text).unwrap_or_default(),
            operation_summary,
            operation_outcome,
            operation_details,
            can_preview: !playing,
            play_reason: play.as_ref().err().map(refusal_text).unwrap_or_default(),
            can_save: save.is_ok(),
            save_reason: save.err().as_ref().map(refusal_text).unwrap_or_default(),
            can_export: export.is_ok(),
            export_reason: export.err().as_ref().map(refusal_text).unwrap_or_default(),
            can_edit: self.can_edit(),
            can_add_component: self.can_edit() && component_add.is_ok(),
            can_add_pair: self.can_edit() && pair_add.is_ok(),
            pair_add_reason: pair_add
                .err()
                .map(EditRefusal::description)
                .unwrap_or_default(),
            add_component_reason: component_add
                .err()
                .map(EditRefusal::description)
                .unwrap_or_default(),
            removed_component: self
                .editor
                .session
                .removed()
                .map(|(index, component)| {
                    format!(
                        "Removed {} · component {}.",
                        component.form.heading(),
                        index + 1
                    )
                })
                .unwrap_or_default(),
            can_restore_component: self.can_edit()
                && self
                    .editor
                    .session
                    .removed()
                    .is_some_and(|(_, component)| draft.check_addition(component.form).is_ok()),
            file_busy: self.files.slot.is_some() || self.prompt.is_some(),
            playback_busy: playing,
            exporting,
            transport_enabled: playing || play.is_ok(),
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
            file_activity: match self.files.slot.as_ref().map(|t| &t.kind) {
                Some(FileKind::ExportRecipe { .. }) => {
                    "Exporting a captured portable recipe; identity and history remain unchanged."
                        .into()
                }
                Some(FileKind::Managed(_)) => {
                    "Reading or publishing the persistent library; later edits remain independent."
                        .into()
                }
                Some(FileKind::Export { .. }) => {
                    "Exporting WAV. Cancel export is separate from Stop audio.".into()
                }
                Some(FileKind::Import(_)) => {
                    "Opening sound; the playing source remains unchanged.".into()
                }
                None => String::new(),
            },
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
        let output = self.audio.output.view();
        let reader = self.audio.output.reader();
        let audio = self.audio.slot.is_some();
        let span = self.audio.output.span_ms();
        let held = output.held;
        let ended = reader.is_some_and(|r| r.status().phase.is_terminal());
        let mut view = TransportView {
            player_title: String::new(),
            elapsed: String::new(),
            generated: String::new(),
            generated_details: String::new(),
            device: format!(
                "Preview/export sample rate: {} Hz",
                self.editor.export_rate.hz()
            ),
            output_state: output_monitor_state(reader, output.capture.as_deref(), held, audio)
                .label(),
            receipt_age: String::new(),
            warning: (String::new(), false),
            backend_warning: String::new(),
            monitor_state: String::new(),
            meters: String::new(),
            caption: String::new(),
            model_caption: String::new(),
            motion: format!(
                "{}{}",
                match self.audio.output.motion() {
                    super::MotionPreference::Reduced =>
                        "System Reduce Motion is enabled: display held by default.",
                    super::MotionPreference::Enabled =>
                        "System motion preference permits live drawing.",
                    super::MotionPreference::Unknown =>
                        "System motion preference is unknown; drawing live.",
                },
                match self.audio.output.hold_overridden() {
                    Some(true) => " User chose Hold display.",
                    Some(false) => " User chose Return to live.",
                    None => "",
                }
            ),
            motion_overridden: self.audio.output.hold_overridden().is_some(),
        };
        if let Some(reader) = reader {
            let status = reader.status();
            let descriptor = reader.descriptor();
            view.player_title = format!(
                "{} · {} · {}",
                phase_text(status.phase),
                descriptor.title,
                match (descriptor.frames, status.phase.is_terminal()) {
                    (None, false) => "Live · until stopped",
                    (None, true) => "Last live session",
                    (Some(_), _) => "Finite recipe",
                }
            );
            let elapsed = NumberStyle::Clock
                .format((descriptor.output.sample_rate).seconds_in(status.estimated_output_frames));
            view.elapsed = descriptor.frames.map_or_else(
                || elapsed.clone(),
                |frames| {
                    format!(
                        "{elapsed} / {}",
                        NumberStyle::Clock.format(descriptor.output.sample_rate.seconds_in(frames))
                    )
                },
            );
            view.generated = if status.phase.is_terminal() {
                format!(
                    "{} · {}",
                    if descriptor.frames.is_some() {
                        "Captured recipe"
                    } else {
                        "Last accepted target"
                    },
                    crate::core::rhythm::realized(&reader.accepted().1).summary()
                )
            } else {
                status.observation.as_ref().map_or_else(
                    || (None).summary(),
                    |o| format!("Generated audio · {}", (Some(o)).summary()),
                )
            };
            view.generated_details = status.observation.map_or_else(String::new, |o| {
                format!(
                    "Observed generated frame {} · accepted revision {} · active revision {} · settled revision {}\n{}",
                    o.frame,
                    status.accepted_revision,
                    o.active_revision,
                    o.settled_revision,
                    o.details()
                )
            });
            view.device = format!(
                "{} · {} Hz · {} channels · {} · output elapsed is estimated",
                descriptor
                    .output
                    .name
                    .as_deref()
                    .unwrap_or("Default output"),
                descriptor.output.sample_rate.hz(),
                descriptor.output.channels,
                descriptor.output.format
            );
            let warning = monitor_warning(reader, span);
            view.warning = (
                warning.map_or_else(String::new, |w| w.message().into()),
                warning.is_some_and(
                    crate::presentation::waveform_view::MonitorWarning::announces_changes,
                ),
            );
            view.backend_warning = output_underrun_warning(status.underruns);
            view.receipt_age = reader.age().map_or_else(
                || "No generated publication received".into(),
                |age| received_text(age, held),
            );
            view.monitor_state = format!(
                "Current session {}: observed frame {} · gaps {} · output underruns {}",
                status.session_id,
                status
                    .observation
                    .map_or_else(|| "none yet".to_owned(), |o| o.frame.to_string()),
                status.gaps,
                status.underruns
            );
        } else {
            view.player_title = if let Some(title) = self.audio.output.failed_title() {
                format!("Failed · {title}")
            } else if let Some(task) = &self.audio.slot {
                format!("Preparing · {}", task.title)
            } else if self.view == View::Workbench {
                "Draft preview · not playing".into()
            } else {
                "Recipe preview · not playing".into()
            };
            view.receipt_age = "No generated publication received".into();
            view.monitor_state = if audio {
                "Waiting for generated samples".into()
            } else {
                "Seekable preview; not generated playback".into()
            };
        }
        view.model_caption = self.audio.output.model().map_or_else(
            || "No valid timing model available".to_owned(),
            |model| {
                format!(
                    "{}{}\n{}",
                    model.caption(),
                    reader.map_or_else(String::new, |r| model.generation_note(r.status())),
                    model.structure.summary()
                )
            },
        );
        if let Some(reason) = &output.unavailable {
            view.warning = (reason.clone(), true);
        }
        if let Some(capture) = &output.capture {
            view.meters = format!(
                "Captured sample peak L {} / R {} dBFS · RMS L {} / R {} dBFS",
                NumberStyle::Decibels.format(capture.meters()[0].peak_dbfs),
                NumberStyle::Decibels.format(capture.meters()[1].peak_dbfs),
                NumberStyle::Decibels.format(capture.meters()[0].rms_dbfs),
                NumberStyle::Decibels.format(capture.meters()[1].rms_dbfs)
            );
            view.caption = capture.label(held, ended);
            view.monitor_state =
                format!("{}\n{}", capture.caption(held, ended), view.monitor_state);
            if capture.clipped() {
                let advisory =
                    crate::presentation::waveform_view::MonitorWarning::FixedRangeExceeded;
                if !view.warning.0.is_empty() {
                    view.warning.0.push('\n');
                }
                view.warning.0.push_str(advisory.message());
            }
        } else {
            view.caption = if audio {
                "Waiting for generated waveform samples".into()
            } else {
                "No waveform capture available. Preview a valid draft or recipe.".into()
            };
        }
        view
    }

    /// The selected inspection and export rate's export choices.
    #[must_use]
    pub const fn export_rates() -> [SampleRate; 5] {
        controls::EXPORT_RATES
    }
}

fn component_diagnostic(error: &auditorylab_core::VoiceError, context: &str) -> String {
    match error {
        auditorylab_core::VoiceError::Frequency {
            hz, sample_rate_hz, ..
        } => {
            format!(
                "Frequency {hz} Hz must be finite, above 0 and below {} Hz ({context}).",
                f64::from(*sample_rate_hz) / 2.0
            )
        }
        _ => format!(
            "{} ({context})",
            presentation::acoustic_errors::voice_error(error)
        ),
    }
}

fn sound_diagnostic(error: &auditorylab_core::SoundError, context: &str) -> String {
    match error {
        auditorylab_core::SoundError::PairCarrier { error, .. } => {
            component_diagnostic(error, context)
        }
        _ => format!(
            "{} ({context})",
            presentation::acoustic_errors::sound_error(error)
        ),
    }
}

#[cfg(test)]
mod diagnostic_tests {
    #[test]
    fn frequency_limits_retain_the_half_hertz_boundary_at_odd_sample_rates() {
        let error = auditorylab_core::VoiceError::Frequency {
            parameter: auditorylab_core::ParameterId::ToneFrequency,
            hz: 22050.5,
            sample_rate_hz: 44101,
        };
        let message = super::component_diagnostic(&error, "at the 44101 Hz export rate");
        assert_eq!(
            message,
            "Frequency 22050.5 Hz must be finite, above 0 and below 22050.5 Hz (at the 44101 Hz export rate)."
        );
    }
}
