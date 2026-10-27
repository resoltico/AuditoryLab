//! Shared wording derived from interactive application values.
use crate as presentation;
use crate::AudioErrorText;
use crate::Explain;
use crate::recipe_text::SnapshotText;
use crate::waveform_view::*;
use crate::*;
use auditorylab_app::document::editing::EditingField;
use auditorylab_app::document::editing::InputProblem;
use auditorylab_app::document::editing::MetadataField;
use auditorylab_app::services::Exported;
use auditorylab_app::services::playback::EndReason;
use auditorylab_app::services::playback::PlaybackReport;
use auditorylab_app::workspace::Refusal;
use auditorylab_core::ParameterId;
use std::path::Path;
pub(crate) fn refusal_text(refusal: &Refusal) -> String {
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
        Refusal::Document(error) => error.explain(),
        Refusal::SelectionChanged => "Library selection changed; activate the action again.".into(),
        Refusal::TemporaryRequired => "Select an Open file to add; use Edit a copy for Built-in content.".into(),
        Refusal::PurgeSelectionChanged => "Select the current trashed version before permanent deletion.".into(),
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

pub(crate) fn field_label(field: EditingField) -> String {
    match field {
        EditingField::Parameter(_, parameter) => parameter.descriptor().label.to_owned(),
        EditingField::Metadata(MetadataField::Title) => {
            auditorylab_core::vocabulary::TITLE.label.into()
        }
        EditingField::Metadata(MetadataField::Summary) => "Notes".into(),
        EditingField::Metadata(MetadataField::Duration) => {
            ParameterId::Duration.descriptor().label.into()
        }
        EditingField::Metadata(MetadataField::Fade) => ParameterId::Fade.descriptor().label.into(),
    }
}

pub(crate) fn problem_text(field: EditingField, problem: InputProblem) -> String {
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

pub(crate) fn exported_text(path: &Path, exported: &Exported) -> String {
    let mut text = format!(
        "Exported {} frames at {} Hz to {} ({}).",
        exported.frames,
        exported.sample_rate.hz(),
        shown(path),
        exported.container
    );
    for warning in &exported.warnings {
        text.push('\n');
        text.push_str(&super::publication_text::warning(warning));
    }
    text
}

/// How a session ended, for the status line. With `shown`, the transport
/// already names an ordinary ending, so only what it does not show remains.
pub(crate) fn playback_text(report: &PlaybackReport, shown: bool) -> String {
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
pub(crate) fn received_text(age: std::time::Duration, held: bool) -> String {
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

pub(crate) fn component_diagnostic(error: &auditorylab_core::VoiceError, context: &str) -> String {
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

pub(crate) fn sound_diagnostic(error: &auditorylab_core::SoundError, context: &str) -> String {
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

/// One editable quantity of a component.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterView {
    /// The field this row edits, bound to its document.
    pub handle: auditorylab_app::document::editing::FieldHandle,
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
    pub key: auditorylab_app::document::editing::RowKey,
    /// The authored component identity.
    pub id: auditorylab_core::ComponentId,
    pub ears: auditorylab_core::Ears,
    /// False for separate-ear pairs whose routes are part of their construction.
    pub routing_editable: bool,
    pub carrier: Option<auditorylab_core::CarrierKind>,
    /// Modulation kind, if any.
    pub modulation: Option<auditorylab_core::ModulationKind>,
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
    pub handle: auditorylab_app::document::editing::FieldHandle,
    /// Label, with unit for quantities.
    pub label: String,
    /// Staged text, or the committed value.
    pub text: String,
    /// Whether the text is staged and not committed.
    pub staged: bool,
    /// What is wrong with the field, if anything.
    pub diagnostic: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct EditorView {
    /// Opaque identity of the rendered document lifetime.
    pub key: auditorylab_app::document::editing::RowKey,
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

/// Render a typed editor projection without changing its admission or applicability.
#[must_use]
pub fn editor_view(view: &auditorylab_app::workspace::EditorView) -> EditorView {
    use crate::AudioSummary;
    use crate::ParameterText;
    use crate::vocabulary::NumberStyle;
    use auditorylab_app::workspace::FieldDiagnostic;
    let rate = view.rate;
    let context = if view.live {
        format!("at the live device rate of {} Hz", rate.hz())
    } else {
        format!("at the {} Hz export rate", rate.hz())
    };
    let diagnostic = |cause: &FieldDiagnostic| match cause {
        FieldDiagnostic::Input(field, problem) => problem_text(*field, *problem),
        FieldDiagnostic::Voice(error) => component_diagnostic(error, &context),
        FieldDiagnostic::Sound(error) => sound_diagnostic(error, &context),
    };
    let metadata = view.metadata.clone().map(|field| MetadataView {
        handle: field.handle,
        label: field.field.parameter().map_or_else(
            || field_label(EditingField::Metadata(field.field)),
            ParameterText::input_label,
        ),
        text: field.text,
        staged: field.staged,
        diagnostic: field
            .diagnostic
            .as_ref()
            .map(|cause| match cause {
                FieldDiagnostic::Sound(error) => presentation::acoustic_errors::sound_error(error),
                _ => diagnostic(cause),
            })
            .unwrap_or_default(),
    });
    let components = view.components.iter().map(|component| {
        let parameters = component.parameters.iter().map(|field| ParameterView {
            handle:field.handle,parameter:field.parameter,label:field.parameter.input_label(),help:field.parameter.help(),
            text:field.text.clone(), staged:field.staged,slider:field.slider,diagnostic:field.diagnostic.as_ref().map(diagnostic).unwrap_or_default(),
        }).collect::<Vec<_>>();
        let mut errors:Vec<_> = component.diagnostics.iter().map(diagnostic).collect();
        errors.retain(|e|!parameters.iter().any(|p|p.diagnostic==*e));errors.dedup();
        ComponentView {
            key:component.key,id:component.id,ears:component.committed.route().unwrap_or(auditorylab_core::Ears::Both),
            routing_editable:component.committed.route().is_some(),carrier:component.committed.primitive_kind(),modulation:component.committed.modulation_kind(),
            heading:component.shown.heading(),name:format!("{} · Component {}",component.shown.heading(),component.id),
            context:component.shown.pair().map_or_else(||component.facts.as_ref().map(AudioSummary::summary).unwrap_or_default(),|pair| format!("Requested second carrier: {} Hz · nominal difference: {} Hz · levels {} / {} dBFS",pair.first_hz.get()+pair.offset_hz.get(),pair.offset_hz.get().abs(),pair.first_peak_dbfs,pair.second_peak_dbfs)),
            error:errors.join("; "),parameters,
        }
    }).collect();
    let peak = match &view.target {
        Ok(target) => format!(
            "{}Peak bounds: {} / {} dBFS {context}",
            if view.pending {
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
    let mut deduped: Vec<String> = Vec::new();
    for field in &metadata {
        if !field.diagnostic.is_empty() && !deduped.contains(&field.diagnostic) {
            deduped.push(field.diagnostic.clone());
        }
    }
    let generated_brief = view.committed.as_ref().map_or_else(
        || "No valid sound model; correct the fields".into(),
        AudioSummary::summary,
    );
    let brief = format!(
        "{} classification · {}\nSaved version · {}\n{}",
        if view.pending {
            "Last committed"
        } else {
            "Draft"
        },
        view.nominal.summary(),
        view.saved
            .as_ref()
            .map_or_else(|| "Not saved".into(), |a| a.summary()),
        generated_brief
    );
    let based_on = view.based_on.as_ref();
    EditorView {
        key: view.key,
        metadata,
        components,
        dirty: view.dirty,
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
            let about = p.about();
            format!(
                "Based on “{}” · {}\n{}\nResearch findings describe the source recipe.",
                p.title(),
                about
                    .as_ref()
                    .and_then(|a| a.research.as_ref())
                    .and_then(|a| a.citation.as_deref())
                    .unwrap_or("source recipe"),
                about
                    .as_ref()
                    .and_then(|a| a
                        .summary
                        .as_deref()
                        .or_else(|| a.research.as_ref().and_then(|r| r.summary.as_deref())))
                    .unwrap_or("")
            )
        }),
        retained_warning: retained_warning(
            view.pending,
            view.target.is_err(),
            view.live,
            view.unrelated_title.as_deref(),
        ),
    }
}

pub(crate) fn retained_warning(
    pending: bool,
    invalid: bool,
    live: bool,
    unrelated_title: Option<&str>,
) -> String {
    let retained = if pending {
        "Pending edits — not yet applied"
    } else if invalid && live {
        "Edits not applied — playing the last valid settings."
    } else {
        ""
    };
    unrelated_title.map_or_else(|| retained.to_owned(), |title| format!("Editing a separate sound; current audio remains “{title}”. Stop, then Play this sound to replace it.{}{retained}", if retained.is_empty() { "" } else { "\n" }))
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
    /// Time since changed generated data was received.
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

/// Render a coherent transport projection; all lifecycle decisions are captured by workspace.
#[must_use]
pub fn transport_view(data: &auditorylab_app::workspace::TransportView) -> TransportView {
    use crate::AudioDetails;
    use crate::AudioSummary;
    use crate::vocabulary::NumberStyle;
    use crate::waveform_view::output_underrun_warning;
    use crate::waveform_view::phase_text;
    use auditorylab_app::workspace::MotionPreference;
    use auditorylab_app::workspace::View;
    let output = &data.output;
    let reader = data.monitor.as_ref();
    let audio = data.task_title.is_some();
    let held = output.held;
    let ended = reader.is_some_and(|r| r.status.phase.is_terminal());
    let export_rate = data.export_rate;
    let task_title = data.task_title.as_deref();
    let view_kind = data.view;
    let mut view = TransportView {
        player_title: String::new(),
        elapsed: String::new(),
        generated: String::new(),
        generated_details: String::new(),
        device: format!("Preview/export sample rate: {} Hz", export_rate.hz()),
        output_state: data.state.label(),
        receipt_age: String::new(),
        warning: (String::new(), false),
        backend_warning: String::new(),
        monitor_state: String::new(),
        meters: String::new(),
        caption: String::new(),
        model_caption: String::new(),
        motion: format!(
            "{}{}",
            match data.motion {
                MotionPreference::Reduced =>
                    "System Reduce Motion is enabled: display held by default.",
                MotionPreference::Enabled => "System motion preference permits live drawing.",
                MotionPreference::Unknown => "System motion preference is unknown; drawing live.",
            },
            match data.hold_override {
                Some(true) => " User chose Hold display.",
                Some(false) => " User chose Return to live.",
                None => "",
            }
        ),
        motion_overridden: data.hold_override.is_some(),
    };
    if let Some(reader) = reader {
        let status = reader.status;
        let descriptor = &reader.descriptor;
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
                auditorylab_core::rhythm::realized(&reader.accepted).summary()
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
            descriptor.output.format.label()
        );
        let warning = reader.warning;
        view.warning = (
            warning.map_or_else(String::new, |w| w.message().into()),
            warning.is_some_and(
                auditorylab_app::workspace::monitor::MonitorWarning::announces_changes,
            ),
        );
        view.backend_warning = output_underrun_warning(status.underruns);
        view.receipt_age = reader.age.map_or_else(
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
        view.player_title = if let Some(title) = data.failed_title.as_deref() {
            format!("Failed · {title}")
        } else if let Some(title) = task_title {
            format!("Preparing · {title}")
        } else if view_kind == View::Workbench {
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
    view.model_caption = data.model.as_ref().map_or_else(
        || "No valid timing model available".to_owned(),
        |model| {
            format!(
                "{}{}\n{}",
                model.caption(),
                reader.map_or_else(String::new, |r| model.generation_note(r.status)),
                model.structure.summary()
            )
        },
    );
    if let Some(reason) = &output.unavailable {
        view.warning = (reason.description(), true);
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
        view.monitor_state = format!("{}\n{}", capture.caption(held, ended), view.monitor_state);
        if capture.clipped() {
            let advisory = auditorylab_app::workspace::monitor::MonitorWarning::FixedRangeExceeded;
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

/// Render an immutable library projection without querying or loading sources.
#[must_use]
pub fn library_view(data: &auditorylab_app::workspace::LibraryView) -> LibraryView {
    use crate::AudioSummary;
    use crate::vocabulary::NumberStyle;
    let rows = data
        .rows
        .iter()
        .map(|row| super::library_text::row(row, data.sort))
        .collect::<Vec<_>>();
    let selected = data.selected;
    let snapshot = data.snapshot.as_ref();
    let rate = data.rate;
    let Some(snapshot) = snapshot else {
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
            auditorylab_core::rhythm::CONVENTION
        ),
        ownership,
        structure: snapshot
            .at(rate)
            .map_or_else(|e| e.explain(), |s| s.brief()),
        studied: about
            .as_ref()
            .and_then(|a| a.research.as_ref())
            .and_then(|a| a.studied_for.clone())
            .unwrap_or_default(),
        timing: format!(
            "Duration {} · fade {} at each edge",
            NumberStyle::Duration.format(snapshot.sound().duration_seconds().get()),
            NumberStyle::Duration.format(snapshot.sound().fade_seconds().get())
        ),
        summary: about
            .as_ref()
            .and_then(|a| {
                a.summary
                    .clone()
                    .or_else(|| a.research.as_ref().and_then(|r| r.summary.clone()))
            })
            .unwrap_or_default(),
        details: (snapshot).details_at(rate),
        study_url: about
            .as_ref()
            .and_then(|a| a.research.as_ref())
            .and_then(|a| a.source_url.clone())
            .filter(|url| url.starts_with("https://") || url.starts_with("http://")),
    }
}
pub(crate) fn removed_component(
    form: auditorylab_core::ComponentForm,
    id: auditorylab_core::ComponentId,
) -> String {
    format!("Removed {} · Component {id}.", form.heading())
}
pub(crate) fn file_activity(
    kind: Option<auditorylab_app::workspace::status::FileAction>,
) -> String {
    use auditorylab_app::workspace::status::FileAction;
    let Some(kind) = kind else {
        return String::new();
    };
    let context = match kind {
        FileAction::RecipeExport => "Captured portable content keeps its identity and history.",
        FileAction::Library(_) => {
            "Later edits remain independent of the captured library operation."
        }
        FileAction::AudioExport => "Cancel export is separate from Stop audio.",
        FileAction::Import => "The playing source remains unchanged.",
    };
    format!("{} · {context}", kind.operation().label())
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
    pub audio_status: String,
    /// What the running file task does.
    pub file_activity: String,
}

/// Render action availability and retained status without making admission decisions.
#[must_use]
pub fn action_view(view: &auditorylab_app::workspace::ActionView) -> ActionView {
    let refusal = view.operation_refusal.as_ref().map(refusal_text);
    let outcome_details = [
        view.file_status.description(),
        view.edit_status.description(),
        view.audio_status.description(),
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
    ActionView {
        can_play: view.can_play,
        library_play_label: crate::vocabulary::library_play_labels()[usize::from(view.playing)],
        can_import: view.can_import,
        can_manage_library: view.can_manage_library,
        can_add_to_library: view.can_add_to_library,
        can_request_trash: view.can_request_trash,
        can_new: view.can_new,
        can_copy: view.can_copy,
        copy_reason: view
            .copy_reason
            .as_ref()
            .map(refusal_text)
            .unwrap_or_default(),
        operation_summary,
        operation_outcome,
        operation_details,
        can_preview: view.can_preview,
        play_reason: view
            .play_reason
            .as_ref()
            .map(refusal_text)
            .unwrap_or_default(),
        can_save: view.can_save,
        can_save_copy: view.can_save_copy,
        can_export_recipe: view.can_export_recipe,
        save_reason: view
            .save_reason
            .as_ref()
            .map(refusal_text)
            .unwrap_or_default(),
        can_export: view.can_export,
        export_reason: view
            .export_reason
            .as_ref()
            .map(refusal_text)
            .unwrap_or_default(),
        can_edit: view.can_edit,
        can_add_component: view.can_add_component,
        can_add_pair: view.can_add_pair,
        pair_add_reason: view
            .pair_add_reason
            .as_ref()
            .map(refusal_text)
            .unwrap_or_default(),
        add_component_reason: view
            .add_component_reason
            .as_ref()
            .map(refusal_text)
            .unwrap_or_default(),
        removed_component: view
            .removed_component
            .map(|c| removed_component(c.form, c.id))
            .unwrap_or_default(),
        can_restore_component: view.can_restore_component,
        file_busy: view.file_busy,
        playback_busy: view.playback_busy,
        exporting: view.exporting,
        transport_enabled: view.transport_enabled,
        transport_label: view.transport_action.label().into(),
        play_target: view.play_target.clone(),
        progress: view.progress,
        file_status: [
            view.file_status.description(),
            view.edit_status.description(),
        ]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n"),
        audio_status: view.audio_status.description(),
        file_activity: file_activity(view.file_activity.as_ref().map(|activity| activity.kind)),
    }
}
