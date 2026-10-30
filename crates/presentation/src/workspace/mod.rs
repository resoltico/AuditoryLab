//! Shared wording derived from interactive application values.
use crate as presentation;

use crate::Explain;

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
        Refusal::LearningVisible => {
            "Return to Library or Workbench before using this action.".into()
        }
        Refusal::LibraryUnavailable(problem) => {
            format!(
                "User Library unavailable: {}",
                crate::errors::library_diagnostic_message(problem)
            )
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
        Refusal::TemporaryRequired => "Select an Open file to add; use Edit a copy for Built-in \
            content."
            .into(),
        Refusal::PurgeSelectionChanged => "Select the current trashed version before permanent \
            deletion."
            .into(),
        Refusal::DetachedDraft(key) => format!(
            "Detached draft from {key}. Edit the restored \
            saved entry or Save independent copy; the old identity is not \
            recreated."
        ),
        Refusal::ExistingSavedIdentity(_) => "Detached draft belongs to an existing saved \
            identity. Restore/edit that version or Save independent copy."
            .into(),
        Refusal::SavedEntryUnreadable => "Saved entry is unreadable; the retained snapshot is for \
            inspection. Refresh or recover an independent copy before playback."
            .into(),
        Refusal::RestoreBeforePlayback => "Restore this saved recipe before playback.".into(),
        Refusal::Invalid { title, rate, error } => error.as_ref().map_or_else(
            || format!("Could not prepare “{title}” at {} Hz.", rate.hz()),
            |error| crate::sound_error_at_rate(error, title, *rate),
        ),
        Refusal::Edit(error) => crate::edit_refusal_description(*(error)),
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
        text.push_str(&crate::errors::publication::warning(warning));
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
            crate::errors::acoustic::voice_error(error)
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
            crate::errors::acoustic::sound_error(error)
        ),
    }
}

/// Wording for one typed parameter; identity, text and state remain in app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterFieldText {
    pub label: String,
    pub help: &'static str,
    pub diagnostic: String,
}
/// Wording for one typed metadata field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataFieldText {
    pub label: String,
    pub diagnostic: String,
}
/// Wording derived from one component, without child rows or copied state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentText {
    pub heading: &'static str,
    pub name: String,
    pub context: String,
    pub error: String,
}
/// Document wording; the app owns the only editor view tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorText {
    pub brief: String,
    pub peak: String,
    pub metadata_error: String,
    pub based_on_title: String,
    pub based_on: String,
    pub retained_warning: String,
}
fn editor_context(view: &auditorylab_app::workspace::EditorView) -> String {
    if view.live {
        format!("at the live device rate of {} Hz", view.rate.hz())
    } else {
        format!("at the {} Hz export rate", view.rate.hz())
    }
}
fn field_diagnostic(
    cause: &auditorylab_app::workspace::FieldDiagnostic,
    view: &auditorylab_app::workspace::EditorView,
) -> String {
    use auditorylab_app::workspace::FieldDiagnostic;
    match cause {
        FieldDiagnostic::Input(field, problem) => problem_text(*field, *problem),
        FieldDiagnostic::Voice(error) => component_diagnostic(error, &editor_context(view)),
        FieldDiagnostic::Sound(error) => sound_diagnostic(error, &editor_context(view)),
    }
}
/// Render one parameter directly from its typed view.
#[must_use]
pub fn parameter_text(
    field: &auditorylab_app::workspace::ParameterView,
    editor: &auditorylab_app::workspace::EditorView,
) -> ParameterFieldText {
    ParameterFieldText {
        label: crate::parameter_input_label(field.parameter),
        help: crate::parameter_help(field.parameter),
        diagnostic: field
            .diagnostic
            .as_ref()
            .map(|cause| field_diagnostic(cause, editor))
            .unwrap_or_default(),
    }
}
/// Render one metadata field directly from its typed view.
#[must_use]
pub fn metadata_text(
    field: &auditorylab_app::workspace::MetadataView,
    editor: &auditorylab_app::workspace::EditorView,
) -> MetadataFieldText {
    use auditorylab_app::workspace::FieldDiagnostic;
    MetadataFieldText {
        label: field.field.parameter().map_or_else(
            || field_label(EditingField::Metadata(field.field)),
            crate::parameter_input_label,
        ),
        diagnostic: field
            .diagnostic
            .as_ref()
            .map(|cause| match cause {
                FieldDiagnostic::Sound(error) => crate::errors::acoustic::sound_error(error),
                _ => field_diagnostic(cause, editor),
            })
            .unwrap_or_default(),
    }
}
/// Render one component directly; child fields retain their app-owned structure.
#[must_use]
pub fn component_text(
    component: &auditorylab_app::workspace::ComponentView,
    editor: &auditorylab_app::workspace::EditorView,
) -> ComponentText {
    let parameters = component
        .parameters
        .iter()
        .map(|field| parameter_text(field, editor).diagnostic)
        .collect::<Vec<_>>();
    let mut errors = component
        .diagnostics
        .iter()
        .map(|cause| field_diagnostic(cause, editor))
        .collect::<Vec<_>>();
    errors.retain(|error| !parameters.contains(error));
    errors.dedup();
    ComponentText {
        heading: component.shown.heading(),
        name: format!("{} · Component {}", component.shown.heading(), component.id),
        context: component.shown.pair().map_or_else(
            || {
                component
                    .facts
                    .as_ref()
                    .map(crate::voice_summary)
                    .unwrap_or_default()
            },
            |pair| {
                format!(
                    "Requested \
        second carrier: {} Hz · nominal difference: {} Hz · levels {} / {} \
        dBFS",
                    pair.first_hz.get() + pair.offset_hz.get(),
                    pair.offset_hz.get().abs(),
                    pair.first_peak_dbfs,
                    pair.second_peak_dbfs
                )
            },
        ),
        error: errors.join("; "),
    }
}
/// Render document wording without copying its structure or admission state.
#[must_use]
pub fn editor_view(view: &auditorylab_app::workspace::EditorView) -> EditorText {
    let context = editor_context(view);
    let peak = match &view.target {
        Ok(target) => format!(
            "{}{} {context}",
            if view.pending {
                "Pending validation · "
            } else {
                ""
            },
            crate::numbers::peak_level(target.peak_dbfs())
        ),
        Err(error) if error.parameters().is_empty() => format!(
            "{} ({context})",
            crate::errors::acoustic::sound_error(error)
        ),
        Err(_) => format!("Correct the highlighted fields to apply this sound {context}."),
    };
    let mut deduped = Vec::new();
    for field in &view.metadata {
        let diagnostic = metadata_text(field, view).diagnostic;
        if !diagnostic.is_empty() && !deduped.contains(&diagnostic) {
            deduped.push(diagnostic);
        }
    }
    let generated_brief = view.committed.as_ref().map_or_else(
        || "No valid sound model; correct the fields".into(),
        crate::structure_summary,
    );
    let brief = format!(
        // One label in every state: the brief always describes applied settings, and a
        // label that changed with pending text would reflow the editor under the caret.
        "Applied rhythm · {}\nSaved version: {}\n{}",
        crate::analysis_summary(&(view.nominal)),
        view.saved
            .as_ref()
            .map_or_else(|| "Not saved".into(), crate::analysis_summary),
        generated_brief
    );
    let based_on = view.based_on.as_ref();
    EditorText {
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
            let about = crate::recipe::snapshot_about(p);
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
    unrelated_title.map_or_else(
        || retained.to_owned(),
        |title| {
            format!(
                "Editing a separate \
        sound; current audio remains “{title}”. Stop, then Play this sound to \
        replace it.{}{retained}",
                if retained.is_empty() { "" } else { "\n" }
            )
        },
    )
}

/// The transport and monitor text.
#[derive(Debug, Clone, PartialEq)]
pub struct TransportText {
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
    pub warning: String,
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
}

/// Render a coherent transport projection; all lifecycle decisions are captured by workspace.
#[must_use]
pub fn transport_view(data: &auditorylab_app::workspace::TransportView) -> TransportText {
    use crate::numbers::NumberStyle;
    use auditorylab_app::workspace::MotionPreference;
    let output = &data.output;
    let reader = data.monitor.as_ref();
    let audio = data.task_title.is_some();
    let held = output.held;
    let ended = reader.is_some_and(|r| r.status.phase.is_terminal());
    let export_rate = data.export_rate;
    let mut view = TransportText {
        player_title: String::new(),
        elapsed: String::new(),
        generated: String::new(),
        generated_details: String::new(),
        device: format!("Preview/export sample rate: {} Hz", export_rate.hz()),
        output_state: crate::workspace::monitor::output_monitor_state_label(data.state),
        receipt_age: String::new(),
        warning: String::new(),
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
    };
    render_transport_session(&mut view, data);
    view.model_caption = data.model.as_ref().map_or_else(
        || "No valid timing model available".to_owned(),
        |model| {
            format!(
                "{}{}\n{}",
                crate::workspace::monitor::model_view_caption(model),
                reader.map_or_else(String::new, |r| {
                    crate::workspace::monitor::model_view_generation_note(model, &r.status)
                }),
                crate::structure_summary(&(model.structure))
            )
        },
    );
    if let Some(reason) = &output.unavailable {
        view.warning = crate::workspace::status::output_issue_description(reason);
    }
    if let Some(capture) = &output.capture {
        view.meters = format!(
            "Captured sample peak L {} / R {} dBFS · RMS L {} / R {} dBFS",
            crate::numbers::format(NumberStyle::Decibels, capture.meters()[0].peak_dbfs),
            crate::numbers::format(NumberStyle::Decibels, capture.meters()[1].peak_dbfs),
            crate::numbers::format(NumberStyle::Decibels, capture.meters()[0].rms_dbfs),
            crate::numbers::format(NumberStyle::Decibels, capture.meters()[1].rms_dbfs)
        );
        view.caption = crate::workspace::monitor::waveform_capture_label(capture, held, ended);
        view.monitor_state = format!(
            "{}\n{}",
            crate::workspace::monitor::waveform_capture_caption(capture, held, ended),
            view.monitor_state
        );
        if capture.clipped() {
            let advisory = auditorylab_app::workspace::monitor::MonitorWarning::FixedRangeExceeded;
            if !view.warning.is_empty() {
                view.warning.push('\n');
            }
            view.warning
                .push_str(crate::workspace::monitor::monitor_warning_message(advisory));
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
pub struct LibraryRowText {
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
    /// What the current reference condition was studied for.
    pub studied_for: String,
    /// Requested duration in shared presentation units.
    pub duration: String,
    /// Sort-relevant date and any necessary unknown-history context.
    pub dates: String,
}

/// A navigation facet, including its disclosed interval and disjunctive count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryFacetText {
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
pub struct LibraryText {
    /// Current scope or rhythm heading, independent of control indexes.
    pub heading: String,
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
    pub study_url: String,
}

/// Render an immutable library projection without querying or loading sources.
#[must_use]
pub fn library_view(data: &auditorylab_app::workspace::LibraryView) -> LibraryText {
    use crate::numbers::NumberStyle;
    let heading = data
        .query
        .rhythm
        .bands
        .first()
        .map(ToString::to_string)
        .or_else(|| {
            data.query
                .rhythm
                .state
                .map(|state| state.label().to_owned())
        })
        .unwrap_or_else(|| {
            if data.query.scope == auditorylab_app::discovery::query::Scope::All {
                auditorylab_app::vocabulary::ALL_SOUNDS.label.to_owned()
            } else {
                data.query.scope.label().to_owned()
            }
        });
    let snapshot = data.snapshot.as_ref();
    let rate = data.rate;
    let Some(snapshot) = snapshot else {
        return LibraryText {
            heading,
            title: String::new(),
            structure: String::new(),
            classification: String::new(),
            ownership: String::new(),
            studied: String::new(),
            timing: String::new(),
            summary: String::new(),
            details: String::new(),
            study_url: String::new(),
        };
    };
    let about = crate::recipe::snapshot_about(snapshot);
    let ownership = data
        .selected
        .and_then(|index| data.rows.get(index))
        .map(|row| crate::library::row(row, data.sort).collection)
        .unwrap_or_default();
    LibraryText {
        heading,
        title: snapshot.title(),
        classification: format!(
            "{}\nBands follow {}. This describes the sound, not measured brain activity.",
            crate::analysis_summary(snapshot.nominal()),
            auditorylab_core::rhythm::CONVENTION_LABEL
        ),
        ownership,
        structure: snapshot
            .at(rate)
            .map_or_else(|e| e.explain(), |s| crate::rendition_brief(&(s))),
        studied: about
            .as_ref()
            .and_then(|a| a.research.as_ref())
            .and_then(|a| a.studied_for.clone())
            .unwrap_or_default(),
        timing: format!(
            "Duration {} · fade {} at each edge",
            crate::numbers::format(
                NumberStyle::Duration,
                snapshot.recipe().timing().duration_seconds.get()
            ),
            crate::numbers::format(
                NumberStyle::Duration,
                snapshot.recipe().timing().fade_seconds.get()
            )
        ),
        summary: about
            .as_ref()
            .and_then(|a| {
                a.summary
                    .clone()
                    .or_else(|| a.research.as_ref().and_then(|r| r.summary.clone()))
            })
            .unwrap_or_default(),
        details: crate::snapshot_details_at(snapshot, rate),
        study_url: about
            .as_ref()
            .and_then(|a| a.research.as_ref())
            .and_then(|a| a.source_url.clone())
            .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
            .unwrap_or_default(),
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

/// Which actions are available and why not.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionText {
    /// Label for Library playback or replacement.
    pub library_play_label: &'static str,
    /// Actionable reason a selected source cannot be derived.
    pub copy_reason: String,
    /// Bounded global refusal or task outcome beside persistent actions.
    pub operation_summary: String,
    /// File outcome retained alongside a current readiness refusal.
    pub operation_outcome: String,
    /// Complete selectable operation/refusal details.
    pub operation_details: String,
    /// Why Play is not admitted.
    pub play_reason: String,
    /// Why Save is not admitted.
    pub save_reason: String,
    /// Why Export is not admitted.
    pub export_reason: String,
    /// Why a two-tone pair cannot currently be added.
    pub pair_add_reason: String,
    /// Why no component can be added, when editing is otherwise allowed.
    pub add_component_reason: String,
    /// The most recently removed component, which Undo remove restores;
    /// empty when there is none.
    pub removed_component: String,
    /// The transport button's label.
    pub transport_label: String,
    /// What the next Play would play.
    pub play_target: String,
    /// File and general status.
    pub file_status: String,
    pub audio_status: String,
    /// What the running file task does.
    pub file_activity: String,
}

/// Render action availability and retained status without making admission decisions.
#[must_use]
pub fn action_view(view: &auditorylab_app::workspace::WorkspaceActions) -> ActionText {
    let refusal = view.operation_refusal.as_ref().map(refusal_text);
    let outcome_details = [
        crate::workspace::status::file_status_description(&(view.file_status)),
        crate::workspace::status::edit_status_description(&(view.edit_status)),
        crate::workspace::status::audio_status_description(&(view.audio_status)),
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
    ActionText {
        library_play_label: (if view.playing {
            auditorylab_app::vocabulary::Operation::StopCurrentAndPlayRecipe
        } else {
            auditorylab_app::vocabulary::Operation::PlaySelectedSound
        })
        .label(),
        copy_reason: view
            .copy_reason
            .as_ref()
            .map(refusal_text)
            .unwrap_or_default(),
        operation_summary,
        operation_outcome,
        operation_details,
        play_reason: view
            .play_reason
            .as_ref()
            .map(refusal_text)
            .unwrap_or_default(),
        save_reason: view
            .save_reason
            .as_ref()
            .map(refusal_text)
            .unwrap_or_default(),
        export_reason: view
            .export_reason
            .as_ref()
            .map(refusal_text)
            .unwrap_or_default(),
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
        transport_label: view.transport_action.label().into(),
        play_target: view.play_target.clone(),
        file_status: [
            crate::workspace::status::file_status_description(&(view.file_status)),
            crate::workspace::status::edit_status_description(&(view.edit_status)),
        ]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n"),
        audio_status: crate::workspace::status::audio_status_description(&(view.audio_status)),
        file_activity: file_activity(view.file_activity.as_ref().map(|activity| activity.kind)),
    }
}

/// Render one app-issued control option, preserving domain terms and number styles.
#[must_use]
pub fn control_choice(
    option: &auditorylab_app::workspace::ChoiceOption<auditorylab_app::workspace::ControlValue>,
) -> String {
    use auditorylab_app::workspace::ControlValue;
    match option.key {
        ControlValue::MonitorSpan(ms) => {
            crate::numbers::format(crate::numbers::NumberStyle::Milliseconds, f64::from(ms))
        }
        ControlValue::ExportRate(rate) => {
            crate::numbers::format(crate::numbers::NumberStyle::Hertz, f64::from(rate.hz()))
        }
        ControlValue::Creation(kind) => format!("Add {}", kind.term().phrase),
        ControlValue::PulseWidth { duty, .. } => if duty {
            "Duty cycle (%)"
        } else {
            "Fixed duration (ms)"
        }
        .into(),
        ControlValue::PulseShape { shaped, .. } => if shaped {
            "Raised cosine"
        } else {
            "Rectangular"
        }
        .into(),
        ControlValue::WaveformScale(scale) => {
            crate::workspace::monitor::waveform_scale_label(scale).into()
        }
        ControlValue::ChannelRouting(_)
        | ControlValue::Carrier(_)
        | ControlValue::Modulation(_) => option.label.into(),
    }
}

/// Heading or control label of an offered application operation.
#[must_use]
pub const fn operation_label(operation: auditorylab_app::vocabulary::Operation) -> &'static str {
    operation.label()
}

/// Shared editor status at this projection.
#[must_use]
pub const fn editor_status(status: auditorylab_app::vocabulary::EditorStatus) -> &'static str {
    status.label()
}
/// Caption owned by the application or its sound/research domain.
#[must_use]
pub const fn view_caption(caption: auditorylab_app::vocabulary::ViewCaption) -> &'static str {
    caption.label()
}

mod controls;
pub mod monitor;
mod publication;
pub mod status;
mod transport;
pub use controls::*;
pub use publication::*;
pub use transport::*;

fn render_transport_session(
    view: &mut TransportText,
    data: &auditorylab_app::workspace::TransportView,
) {
    use crate::numbers::NumberStyle;
    use crate::workspace::monitor::phase_text;
    let reader = data.monitor.as_ref();
    let held = data.output.held;
    let task_title = data.task_title.as_deref();
    let audio = task_title.is_some();
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
        let elapsed = crate::numbers::format(
            NumberStyle::Clock,
            (descriptor.output.sample_rate).seconds_in(status.estimated_output_frames),
        );
        view.elapsed = descriptor.frames.map_or_else(
            || elapsed.clone(),
            |frames| {
                format!(
                    "{elapsed} / {}",
                    crate::numbers::format(
                        NumberStyle::Clock,
                        descriptor.output.sample_rate.seconds_in(frames)
                    )
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
                crate::structure_summary(&(auditorylab_core::rhythm::realized(&reader.accepted)))
            )
        } else {
            status.observation.as_ref().map_or_else(
                || crate::optional_observation_summary(None),
                |o| {
                    format!(
                        "Generated audio · {}",
                        crate::optional_observation_summary(Some(o))
                    )
                },
            )
        };
        view.generated_details = status.observation.map_or_else(String::new, |o| {
            format!(
                "Observed generated frame {} · accepted revision {} · active revision \
                    {} · settled revision {}\n{}",
                o.frame,
                status.accepted_revision,
                o.active_revision,
                o.settled_revision,
                crate::observation_details(&(o))
            )
        });
        render_transport_output(view, reader, held);
    } else {
        view.player_title = if let Some(title) = data.failed_title.as_deref() {
            format!("Failed · {title}")
        } else if let Some(title) = task_title {
            format!("Preparing · {title}")
        } else {
            "Not playing".into()
        };
        view.receipt_age = "No generated publication received".into();
        view.monitor_state = if audio {
            "Waiting for generated samples".into()
        } else {
            "Seekable preview; not generated playback".into()
        };
    }
}

fn render_transport_output(
    view: &mut TransportText,
    reader: &auditorylab_app::workspace::MonitorView,
    held: bool,
) {
    let status = reader.status;
    let descriptor = &reader.descriptor;
    view.device = format!(
        "{} · {} Hz · {} channels · {} · output elapsed is estimated",
        descriptor
            .output
            .name
            .as_deref()
            .unwrap_or("Default output"),
        descriptor.output.sample_rate.hz(),
        descriptor.output.channels,
        crate::sample_format_label(descriptor.output.format)
    );
    let warning = reader.warning;
    view.warning = warning.map_or_else(String::new, |warning| {
        crate::workspace::monitor::monitor_warning_message(warning).into()
    });
    view.backend_warning = crate::workspace::monitor::output_underrun_warning(status.underruns);
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
            "Frequency 22050.5 Hz must be finite, above 0 and below 22050.5 Hz (at \
                the 44101 Hz export rate)."
        );
    }
}
