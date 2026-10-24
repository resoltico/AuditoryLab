// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! What a frontend renders: semantic rows, shared wording and action
//! availability. Building a view evaluates through the editor's caches; an
//! unchanged document is not evaluated again.
use super::{FileKind, OutputView, Refusal, View, Workspace};
use crate::editing::{EditRefusal, EditingField, FieldHandle, InputProblem, MetadataField};
use crate::session::{EndReason, PlaybackReport};
use crate::{
    DocumentId, Exported, RecipeSnapshot, controls, monitor_warning, output_monitor_state,
    output_underrun_warning, phase_text, presentation,
};
use auditorylab_core::{
    CarrierKind, ComponentId, Ears, ModulationKind, ParameterId, Recipe, SampleRate,
    interpretation::Structure,
};
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
    /// The component's identity.
    pub id: ComponentId,
    /// Ear routing.
    pub ears: Ears,
    /// Carrier kind.
    pub carrier: CarrierKind,
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
    /// What is wrong with this field, if anything.
    pub diagnostic: String,
}

/// The open document.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorView {
    /// The document these rows belong to.
    pub document: DocumentId,
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

/// The library and the selected recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryView {
    /// One label per matching entry.
    pub rows: Vec<String>,
    /// The selected entry's position among the rows.
    pub selected: Option<usize>,
    /// Selected title.
    pub title: String,
    /// Selected structure at the export rate.
    pub structure: String,
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
        Refusal::DevicePlaybackUnavailable => {
            "Device playback is not available in this build.".into()
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
        Refusal::SourceNotFinite {
            component,
            parameter,
        } => format!(
            "Cannot edit a copy: original source {}{} is not finite. Correct the source document before deriving a sound.",
            component.map_or_else(String::new, |index| format!("component {} ", index + 1)),
            parameter.descriptor().label,
        ),
        Refusal::NotFinite => "The recipe's numbers must all be finite before it can play.".into(),
        Refusal::Document(message) => message.clone(),
        Refusal::Invalid { title, rate, error } => error.as_ref().map_or_else(
            || format!("Could not prepare “{title}” at {} Hz.", rate.hz()),
            |error| presentation::invalid_at_rate(title, *rate, error),
        ),
        Refusal::Edit(EditRefusal::ComponentLimit) => format!(
            "A sound holds at most {} components; remove one to add another.",
            auditorylab_core::Stimulus::MAX_COMPONENTS
        ),
        Refusal::Edit(EditRefusal::NothingRemoved) => {
            "No removed component is waiting to be restored.".into()
        }
        Refusal::Edit(EditRefusal::IdentitiesExhausted) => {
            "This sound has used every component identity; start a new sound to add more.".into()
        }
        Refusal::Edit(
            EditRefusal::StaleDocument | EditRefusal::UnknownComponent | EditRefusal::Inapplicable,
        ) => "That field no longer belongs to the open sound.".into(),
        Refusal::TooManyComponents(count) => format!(
            "The workbench supports at most {} components; this recipe has {count}. Edit the recipe file to reduce them.",
            auditorylab_core::Stimulus::MAX_COMPONENTS
        ),
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

pub(super) fn file_started_text(kind: &FileKind) -> String {
    match kind {
        FileKind::Import(path) => format!("Opening {}.", shown(path)),
        FileKind::Save { content, path, .. } => format!(
            "Saving {} to {}.",
            content.about.as_ref().map_or("sound", |a| a.title.as_str()),
            shown(path)
        ),
        FileKind::Export { title, path, .. } => format!("Exporting {title} to {}.", shown(path)),
    }
}

pub(super) fn imported_text(snapshot: &RecipeSnapshot) -> String {
    format!("Opened {}.", snapshot.title())
}

pub(super) fn saved_text(
    path: &Path,
    content: &Recipe,
    same_document: bool,
    rate: SampleRate,
) -> String {
    let valid = content.stimulus.signal(rate).is_ok();
    format!(
        "{} to {}.{}",
        if valid {
            "Saved sound"
        } else {
            "Saved with validation errors"
        },
        shown(path),
        if same_document {
            ""
        } else {
            " The current sound is a different document; its saved state is unchanged."
        }
    )
}

pub(super) fn exported_text(path: &Path, exported: &Exported) -> String {
    format!(
        "Exported {} frames at {} Hz to {} ({}).",
        exported.frames,
        exported.sample_rate.hz(),
        shown(path),
        exported.container
    )
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

#[derive(PartialEq, Eq)]
pub(super) struct LibraryKey {
    snapshots: Vec<crate::SnapshotId>,
    selected: Option<crate::SnapshotId>,
    rate: SampleRate,
}

impl Workspace {
    /// The open document's rows, values and diagnostics. Diagnostics use
    /// the bound live session's rate when there is one, otherwise the
    /// selected export rate; each is labelled.
    pub fn editor_view(&mut self) -> EditorView {
        let rate = self.diagnostic_rate();
        let candidate = self.editor.candidate();
        let analysis = self.editor.candidate_analysis(rate);
        let editor = &self.editor;
        let document = editor.document();
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
                            .map(ToString::to_string)
                    })
                    .unwrap_or_default()
            });
            MetadataView {
                handle: editor.handle(editing),
                label: match field.parameter() {
                    Some(p) => presentation::labeled(p),
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
            .map(|(index, (id, committed))| {
                let shown = candidate
                    .draft()
                    .component(*id)
                    .copied()
                    .unwrap_or(*committed);
                let own = analysis.check().component(*id);
                let acoustic = own.and_then(|r| r.as_ref().err());
                let parameters = ParameterId::COMPONENT
                    .into_iter()
                    .filter(|p| p.read(committed).is_some())
                    .map(|parameter| {
                        let field = EditingField::Parameter(*id, parameter);
                        let diagnostic = problem(field).unwrap_or_else(|| {
                            acoustic
                                .filter(|e| e.parameters().contains(&parameter))
                                .map(ToString::to_string)
                                .unwrap_or_default()
                        });
                        ParameterView {
                            handle: editor.handle(field),
                            parameter,
                            label: presentation::labeled(parameter),
                            help: match parameter {
                                ParameterId::Duration => "The length of saved recipes and exported audio; live playback continues until stopped.",
                                ParameterId::Fade => "The raised-cosine fade at each edge of saved recipes and exported audio.",
                                _ => parameter.descriptor().help,
                            },
                            text: editor.text(field),
                            staged: editor.staged(field).is_some(),
                            slider: parameter
                                .read(committed)
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
                if let Some(error) = acoustic {
                    errors.push(format!("{error} ({context})"));
                }
                ComponentView {
                    id: *id,
                    ears: committed.ears,
                    carrier: committed.carrier.kind(),
                    modulation: committed.modulation.map(|m| m.kind()),
                    heading: shown.role().label(),
                    name: format!("{} · component {}", shown.role().label(), index + 1),
                    context: match own {
                        Some(Ok(facts)) => presentation::component_description(facts, false),
                        // The error line states it, with its rate.
                        Some(Err(_)) | None => String::new(),
                    },
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
                presentation::format_dbfs(target.peak_dbfs()[0]),
                presentation::format_dbfs(target.peak_dbfs()[1])
            ),
            Err(error) if error.parameters().is_empty() => format!("{error} ({context})"),
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
        let brief = analysis.target().map_or_else(
            || "No valid sound model; correct the fields".into(),
            |t| presentation::structure_description(&Structure::of_target(t)),
        );
        let based_on = editor.draft().based_on();
        let retained_warning = self.retained_warning(pending, analysis.target().is_none());
        EditorView {
            document,
            metadata,
            components,
            dirty: editor.dirty(),
            brief,
            peak,
            metadata_error: deduped.join("; "),
            based_on_title: based_on.map_or_else(String::new, |p| {
                format!(
                    "Based on “{}” · Retained source in Sound settings",
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
        let unrelated = self.audio.as_ref().is_some_and(|t| {
            self.view == View::Workbench
                && !matches!(t.kind, super::AudioKind::Live(d) if d == self.editor.document())
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
            let title = self.audio.as_ref().map_or("", |t| t.title.as_str());
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
        let rate = self.export_rate;
        let key = LibraryKey {
            snapshots: self
                .filtered
                .iter()
                .map(|&i| self.catalog[i].id())
                .collect(),
            selected: self.selected().map(RecipeSnapshot::id),
            rate,
        };
        if let Some((cached, view)) = &*self.library_cache.borrow()
            && *cached == key
        {
            return view.clone();
        }
        #[cfg(test)]
        self.library_evaluations
            .set(self.library_evaluations.get() + 1);
        let view = Arc::new(self.build_library_view());
        *self.library_cache.borrow_mut() = Some((key, view.clone()));
        view
    }

    fn build_library_view(&self) -> LibraryView {
        let rate = self.export_rate;
        let rows = self
            .filtered
            .iter()
            .map(|&i| presentation::library_label(&self.catalog[i], rate))
            .collect();
        let selected = self.filtered.iter().position(|&i| i == self.selected);
        let Some(snapshot) = self.selected() else {
            return LibraryView {
                rows,
                selected,
                title: String::new(),
                structure: String::new(),
                studied: String::new(),
                timing: String::new(),
                summary: String::new(),
                details: String::new(),
                study_url: None,
            };
        };
        let about = snapshot.about();
        LibraryView {
            rows,
            selected,
            title: snapshot.title(),
            structure: snapshot
                .at(rate)
                .map_or_else(|e| e.to_string(), |s| presentation::sound_brief(&s)),
            studied: about
                .and_then(|a| a.research.as_ref())
                .and_then(|a| a.studied_for.clone())
                .unwrap_or_default(),
            timing: format!(
                "Duration {} · fade {} at each edge",
                presentation::human_duration(snapshot.stimulus().duration_seconds),
                presentation::human_duration(snapshot.stimulus().fade_seconds)
            ),
            summary: about
                .and_then(|a| {
                    a.summary
                        .clone()
                        .or_else(|| a.research.as_ref().and_then(|r| r.summary.clone()))
                })
                .unwrap_or_default(),
            details: presentation::recipe_details(snapshot, rate),
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
                .filter(|r| {
                    matches!(
                        r,
                        Refusal::SourceNotFinite { .. } | Refusal::TooManyComponents(_)
                    )
                })
                .map(refusal_text),
        };
        let outcome_details = [
            self.file_status.as_str(),
            self.edit_status.as_str(),
            self.audio_status.as_str(),
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
        let full =
            self.editor.draft().components().len() >= auditorylab_core::Stimulus::MAX_COMPONENTS;
        let playing = self.audio.is_some();
        let exporting = self.exporting();
        let progress = self.export_progress();
        ActionView {
            can_play: play.is_ok(),
            library_play_label: presentation::LIBRARY_PLAY_LABELS[usize::from(playing)],
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
            can_add_component: self.can_edit() && !full,
            add_component_reason: if full {
                refusal_text(&Refusal::Edit(EditRefusal::ComponentLimit))
            } else {
                String::new()
            },
            removed_component: self
                .editor
                .removed()
                .map(|(index, component)| {
                    format!(
                        "Removed {} · component {}.",
                        component.role().label(),
                        index + 1
                    )
                })
                .unwrap_or_default(),
            can_restore_component: self.can_edit() && !full && self.editor.removed().is_some(),
            file_busy: self.file.is_some() || self.prompt.is_some(),
            playback_busy: playing,
            exporting,
            transport_enabled: playing || play.is_ok(),
            transport_label: if playing {
                presentation::STOP_AUDIO
            } else if self.view == View::Workbench {
                presentation::PLAY_LIVE
            } else {
                presentation::PLAY_SELECTED
            }
            .into(),
            play_target: match self.view {
                View::Workbench => self.editor.draft().title().to_owned(),
                View::Library => self
                    .selected()
                    .map(RecipeSnapshot::title)
                    .unwrap_or_default(),
            },
            progress,
            file_status: [self.file_status.as_str(), self.edit_status.as_str()]
                .into_iter()
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
            audio_status: self.audio_status.clone(),
            file_activity: match self.file.as_ref().map(|t| &t.kind) {
                Some(FileKind::Save { .. }) => {
                    "Saving an immutable sound snapshot; audio and later edits remain independent."
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
        match &self.file {
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
        self.output.view()
    }

    /// The transport and monitor text for the bound session or preview.
    #[must_use]
    pub fn transport_view(&self) -> TransportView {
        let output = self.output.view();
        let reader = self.output.reader();
        let audio = self.audio.is_some();
        let span = self.output.span_ms();
        let held = output.held;
        let ended = reader.is_some_and(|r| r.status().phase.is_terminal());
        let mut view = TransportView {
            player_title: String::new(),
            elapsed: String::new(),
            generated: String::new(),
            generated_details: String::new(),
            device: format!("Preview/export sample rate: {} Hz", self.export_rate.hz()),
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
                match self.output.motion() {
                    super::MotionPreference::Reduced =>
                        "System Reduce Motion is enabled: display held by default.",
                    super::MotionPreference::Enabled =>
                        "System motion preference permits live drawing.",
                    super::MotionPreference::Unknown =>
                        "System motion preference is unknown; drawing live.",
                },
                match self.output.hold_overridden() {
                    Some(true) => " User chose Hold display.",
                    Some(false) => " User chose Return to live.",
                    None => "",
                }
            ),
            motion_overridden: self.output.hold_overridden().is_some(),
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
            let elapsed = presentation::playback_elapsed(
                descriptor.output.sample_rate,
                status.estimated_output_frames,
            );
            view.elapsed = descriptor.frames.map_or_else(
                || elapsed.clone(),
                |frames| {
                    format!(
                        "{elapsed} / {}",
                        presentation::clock(descriptor.output.sample_rate.seconds_in(frames))
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
                    presentation::structure_description(&Structure::of_target(
                        &reader.accepted().1
                    ))
                )
            } else {
                status.observation.as_ref().map_or_else(
                    || presentation::generated_description(None),
                    |o| {
                        format!(
                            "Generated audio · {}",
                            presentation::generated_description(Some(o))
                        )
                    },
                )
            };
            view.generated_details = status.observation.map_or_else(String::new, |o| {
                format!(
                    "Observed generated frame {} · accepted revision {} · active revision {} · settled revision {}\n{}",
                    o.frame,
                    status.accepted_revision,
                    o.active_revision,
                    o.settled_revision,
                    presentation::generated_details(&o)
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
                warning.is_some_and(crate::MonitorWarning::announces_changes),
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
            view.player_title = if let Some(title) = self.output.failed_title() {
                format!("Failed · {title}")
            } else if let Some(task) = &self.audio {
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
        view.model_caption = self.output.model().map_or_else(
            || "No valid timing model available".to_owned(),
            |model| {
                format!(
                    "{}{}\n{}",
                    model.caption(),
                    reader.map_or_else(String::new, |r| model.generation_note(r.status())),
                    presentation::structure_description(&model.structure)
                )
            },
        );
        if let Some(reason) = &output.unavailable {
            view.warning = (reason.clone(), true);
        }
        if let Some(capture) = &output.capture {
            view.meters = format!(
                "Captured sample peak L {} / R {} dBFS · RMS L {} / R {} dBFS",
                presentation::format_dbfs(capture.meters()[0].peak_dbfs),
                presentation::format_dbfs(capture.meters()[1].peak_dbfs),
                presentation::format_dbfs(capture.meters()[0].rms_dbfs),
                presentation::format_dbfs(capture.meters()[1].rms_dbfs)
            );
            view.caption = capture.label(held, ended);
            view.monitor_state =
                format!("{}\n{}", capture.caption(held, ended), view.monitor_state);
            if capture.clipped() {
                let advisory = crate::MonitorWarning::FixedRangeExceeded;
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
