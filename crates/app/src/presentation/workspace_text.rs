//! Shared wording derived from interactive application values.
use crate::audio::session::{EndReason, PlaybackReport};
use crate::core::ParameterId;
use crate::document::editing::{EditingField, InputProblem, MetadataField};
use crate::presentation::{self, AudioErrorText};
use crate::storage::wav::Exported;
use crate::workspace::Refusal;
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

pub(crate) fn field_label(field: EditingField) -> String {
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
        text.push_str(warning);
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

/// Rendering context captured from the coordinator's current rates and playback binding.
pub(crate) struct EditorViewContext {
    pub(crate) rate: crate::core::SampleRate,
    pub(crate) live: bool,
    pub(crate) retained_warning: String,
}
pub(crate) fn editor_view(
    editor: &crate::document::editing::EditorSession,
    candidate: &crate::document::editing::Candidate,
    analysis: &crate::document::editing::Analysis,
    committed_analysis: &crate::document::editing::Analysis,
    nominal: &crate::core::rhythm::Analysis,
    rendering: EditorViewContext,
) -> crate::workspace::EditorView {
    use crate::document::controls;
    use crate::presentation::vocabulary::NumberStyle;
    use crate::presentation::{AudioSummary, ParameterText};
    use crate::workspace::{ComponentView, EditorView, MetadataView, ParameterView};
    let rate = rendering.rate;
    let problem = |field: EditingField| {
        candidate
            .problems()
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(f, p)| problem_text(*f, *p))
    };
    let context = if rendering.live {
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
        .map(|component| {
            let (id,committed)=(&component.id,&component.form);
            let shown = candidate
                .draft()
                .component(*id)
                .copied()
                .unwrap_or(*committed);
            let own = analysis.check().voice((*id).into());
            let acoustic = own.and_then(|r| r.as_ref().err());
            let second=analysis.check().voice(id.with_lane(auditorylab_core::Lane::Secondary)).and_then(|r|r.as_ref().err());
            let coupled=analysis.check().result().err().filter(|error|match error {auditorylab_core::SoundError::Pair(auditorylab_core::PairError {id:owner,..})=>owner.get()==id.get(),auditorylab_core::SoundError::PairCarrier {id:owner,..}=>owner.component==*id,_=>false});
            let parameters = ParameterId::COMPONENT
                .into_iter()
                .filter(|p| committed.get(*p).is_some())
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
                            _ => parameter.help(),
                        },
                        text: editor.text(field),
                        staged: editor.staged(field).is_some(),
                        slider: committed
                            .get(parameter)
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
                ears: committed.route().unwrap_or(crate::core::Ears::Both),
                routing_editable: committed.route().is_some(),
                carrier: committed.primitive_kind(),
                modulation: committed.modulation_kind(),
                heading: shown.heading(),
                name: format!("{} · Component {id}", shown.heading()),
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
        retained_warning: rendering.retained_warning,
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

pub(crate) fn transport_view(
    output_state: &crate::workspace::output::OutputState,
    export_rate: crate::core::SampleRate,
    task_title: Option<&str>,
    view_kind: crate::workspace::View,
) -> crate::workspace::TransportView {
    use crate::presentation::vocabulary::NumberStyle;
    use crate::presentation::waveform_view::{
        monitor_warning, output_monitor_state, output_underrun_warning, phase_text,
    };
    use crate::presentation::{AudioDetails, AudioSummary};
    use crate::workspace::{MotionPreference, TransportView, View};

    let output = output_state.view();
    let reader = output_state.reader();
    let audio = task_title.is_some();
    let span = output_state.span_ms();
    let held = output.held;
    let ended = reader.is_some_and(|r| r.status().phase.is_terminal());
    let mut view = TransportView {
        player_title: String::new(),
        elapsed: String::new(),
        generated: String::new(),
        generated_details: String::new(),
        device: format!("Preview/export sample rate: {} Hz", export_rate.hz()),
        output_state: output_monitor_state(reader, output.capture.as_deref(), held, audio).label(),
        receipt_age: String::new(),
        warning: (String::new(), false),
        backend_warning: String::new(),
        monitor_state: String::new(),
        meters: String::new(),
        caption: String::new(),
        model_caption: String::new(),
        motion: format!(
            "{}{}",
            match output_state.motion() {
                MotionPreference::Reduced =>
                    "System Reduce Motion is enabled: display held by default.",
                MotionPreference::Enabled => "System motion preference permits live drawing.",
                MotionPreference::Unknown => "System motion preference is unknown; drawing live.",
            },
            match output_state.hold_overridden() {
                Some(true) => " User chose Hold display.",
                Some(false) => " User chose Return to live.",
                None => "",
            }
        ),
        motion_overridden: output_state.hold_overridden().is_some(),
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
            warning
                .is_some_and(crate::presentation::waveform_view::MonitorWarning::announces_changes),
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
        view.player_title = if let Some(title) = output_state.failed_title() {
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
    view.model_caption = output_state.model().map_or_else(
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
        view.monitor_state = format!("{}\n{}", capture.caption(held, ended), view.monitor_state);
        if capture.clipped() {
            let advisory = crate::presentation::waveform_view::MonitorWarning::FixedRangeExceeded;
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

pub(crate) fn library_view(
    rows: Vec<crate::workspace::LibraryRowView>,
    selected: Option<usize>,
    snapshot: Option<&crate::document::recipe::RecipeSnapshot>,
    rate: crate::core::SampleRate,
) -> crate::workspace::LibraryView {
    use crate::presentation::{AudioSummary, vocabulary::NumberStyle};
    use crate::workspace::LibraryView;
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
pub(crate) fn removed_component(
    form: crate::core::ComponentForm,
    id: crate::core::ComponentId,
) -> String {
    format!("Removed {} · Component {id}.", form.heading())
}
pub(crate) fn file_activity(kind: Option<&crate::workspace::tasks::FileKind>) -> String {
    use crate::workspace::tasks::FileKind;
    match kind {
        Some(FileKind::ExportRecipe { .. }) => {
            "Exporting a captured portable recipe; identity and history remain unchanged.".into()
        }
        Some(FileKind::Managed(_)) => {
            "Reading or publishing the persistent library; later edits remain independent.".into()
        }
        Some(FileKind::Export { .. }) => {
            "Exporting WAV. Cancel export is separate from Stop audio.".into()
        }
        Some(FileKind::Import(_)) => "Opening sound; the playing source remains unchanged.".into(),
        None => String::new(),
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
