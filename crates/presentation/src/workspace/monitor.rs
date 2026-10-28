//! Wording for monitor captures and model views.
use crate::numbers::NumberStyle;
use auditorylab_app::workspace::monitor::ModelSource;
use auditorylab_app::workspace::monitor::ModelView;
use auditorylab_app::workspace::monitor::MonitorWarning;
use auditorylab_app::workspace::monitor::OutputMonitorState;
use auditorylab_app::workspace::monitor::WaveformCapture;
use auditorylab_app::workspace::monitor::WaveformScale;
/// Wording for the captured WaveformCapture data.

/// Concise label derived only from this captured window, never current navigation.

pub fn waveform_capture_label(value: &WaveformCapture, frozen: bool, ended: bool) -> String {
    format!(
        "{} · {} · {:.2} ms · {} ±{}",
        if frozen {
            "Held capture"
        } else if ended {
            "Last capture"
        } else if value.session().is_some() {
            "Generated waveform"
        } else {
            "Preview · not playing"
        },
        value.title(),
        value.milliseconds(),
        if value.scale() == WaveformScale::Fit {
            "Fit"
        } else {
            "Fixed"
        },
        crate::numbers::format(NumberStyle::Compact, value.limit())
    )
}
/// Full frozen/retained context; current generated status is presented independently.

pub fn waveform_capture_caption(value: &WaveformCapture, frozen: bool, ended: bool) -> String {
    format!(
        "{} · session {} · sample rate {} Hz · frames {}..{} · snapshot {} · observed frame {} · captured gaps {} / underruns {} · exact scale ±{}",
        crate::workspace::monitor::waveform_capture_label(&(value), frozen, ended),
        value
            .session()
            .map_or_else(|| "preview".into(), |session| session.id.to_string()),
        value.rate().hz(),
        value.span().0,
        value.span().1,
        value.sequence(),
        value
            .observation()
            .map_or_else(|| "none yet".to_owned(), |o| o.frame.to_string()),
        value.gaps(),
        value.underruns(),
        value.limit()
    )
}

/// Wording for the captured ModelView data.

/// Explicit model rate/origin/provenance, with insufficient-cycle warning when capped.

pub fn model_view_caption(value: &ModelView) -> String {
    format!(
        "Timing model · {} · {} · origin 0 · {} Hz · {:.1} ms · projection, not recorded output{}",
        value.title,
        match value.source {
            ModelSource::Draft => "committed draft".into(),
            ModelSource::Recipe => "validated recipe".into(),
            ModelSource::Accepted(r) => format!("accepted target revision {r}; not phase history"),
        },
        value.projection.rate.hz(),
        value.projection.rate.seconds_in(value.projection.frames) * 1000.0,
        if value.projection.insufficient_cycle {
            " · less than one cycle at a slow rate"
        } else {
            ""
        }
    )
}
/// Shared labels for physical lanes; a frontend only renders their geometry.

pub fn model_view_lane_label(lane: &auditorylab_core::timing::ModelLane) -> String {
    use auditorylab_core::timing::ModelKind;
    let quantity = match lane.kind {
        ModelKind::Steady => match lane.facts.carrier {
            auditorylab_core::CarrierFacts::Tone(rate) => {
                format!(
                    "{} Hz steady tone",
                    crate::numbers::format(NumberStyle::Compact, rate.realized_hz)
                )
            }
            _ => "Steady carrier".into(),
        },
        ModelKind::Clicks => match lane.facts.carrier {
            auditorylab_core::CarrierFacts::Clicks(p) => {
                format!(
                    "{} Hz clicks",
                    crate::numbers::format(NumberStyle::Compact, p.rate_hz)
                )
            }
            _ => unreachable!("click lane carrier"),
        },
        ModelKind::Gate => match lane.facts.modulation {
            Some(auditorylab_core::ModulationFacts::Gate(p)) => {
                format!(
                    "{} Hz gate",
                    crate::numbers::format(NumberStyle::Compact, p.rate_hz)
                )
            }
            _ => unreachable!("gate lane modulation"),
        },
        ModelKind::Envelope => match lane.facts.modulation {
            Some(auditorylab_core::ModulationFacts::Am { rate, depth }) => format!(
                "{} Hz AM · {}%",
                crate::numbers::format(NumberStyle::Compact, rate.realized_hz),
                crate::numbers::format(NumberStyle::Compact, depth * 100.0)
            ),
            _ => unreachable!("envelope lane modulation"),
        },
    };
    format!(
        "{} · {quantity}",
        crate::errors::acoustic::voice_reference(lane.id)
    )
}
/// Relate a model's accepted revision to actual generation without replaying live phases.

pub fn model_view_generation_note(
    value: &ModelView,
    status: auditorylab_app::services::playback::GeneratedStatus,
) -> String {
    let ModelSource::Accepted(revision) = value.source else {
        return String::new();
    };
    let ended = status.phase.is_terminal();
    match status.observation {
        None => if ended {
            " · not generated before the session ended"
        } else {
            " · waiting for generation"
        }
        .into(),
        Some(o) if o.active_revision < revision => if ended {
            " · accepted model was not generated before the session ended"
        } else {
            " · queued; not generated yet"
        }
        .into(),
        Some(o) if o.active_revision > revision => {
            " · older model snapshot; generation has advanced".into()
        }
        Some(o) if o.transitioning || o.settled_revision < revision => {
            " · generation is changing toward this model".into()
        }
        _ => String::new(),
    }
}

/// Wording for the captured OutputMonitorState data.

/// Concise status for any frontend.

pub fn output_monitor_state_label(value: OutputMonitorState) -> &'static str {
    match value {
        OutputMonitorState::Preview => "Preview",
        OutputMonitorState::Live => "Live",
        OutputMonitorState::Held => "Held",
        OutputMonitorState::Stale => "Stale",
        OutputMonitorState::Waiting => "Waiting",
        OutputMonitorState::Unavailable => "Unavailable",
        OutputMonitorState::LastCapture => "Last capture",
    }
}

/// Wording for the captured MonitorWarning data.

/// Visible and accessible text for this current condition.

pub fn monitor_warning_message(value: MonitorWarning) -> &'static str {
    match value {
        MonitorWarning::Stale => "Monitoring is stale; retained samples are not current output.",
        MonitorWarning::IncompleteCoverage => {
            "Incomplete current waveform coverage; waiting for a contiguous window."
        }
        MonitorWarning::FixedRangeExceeded => {
            "Waveform exceeds the fixed display range; exact levels are retained. Use Fit to view the full extent."
        }
    }
}

/// Wording for the captured WaveformScale data.

pub fn waveform_scale_terms(value: WaveformScale) -> (&'static str, &'static str) {
    match value {
        WaveformScale::Fixed => ("Fixed ±1", "fixed ±1"),
        WaveformScale::Fit => ("Fit signal", "fit signal"),
    }
}
/// Domain name for a heading or choice.

pub fn waveform_scale_label(value: WaveformScale) -> &'static str {
    crate::workspace::monitor::waveform_scale_terms(value).0
}
/// Domain name within a sentence.

pub fn waveform_scale_phrase(value: WaveformScale) -> &'static str {
    crate::workspace::monitor::waveform_scale_terms(value).1
}

/// Shared lifecycle wording; terminal silence never relabels a session's recipe.
#[must_use]
pub const fn phase_text(phase: auditorylab_app::services::playback::PlaybackPhase) -> &'static str {
    use auditorylab_app::services::playback::PlaybackPhase::Completed;
    use auditorylab_app::services::playback::PlaybackPhase::Draining;
    use auditorylab_app::services::playback::PlaybackPhase::Failed;
    use auditorylab_app::services::playback::PlaybackPhase::FrameLimit;
    use auditorylab_app::services::playback::PlaybackPhase::Preparing;
    use auditorylab_app::services::playback::PlaybackPhase::Running;
    use auditorylab_app::services::playback::PlaybackPhase::Stopped;
    use auditorylab_app::services::playback::PlaybackPhase::Stopping;
    match phase {
        Preparing => "Preparing output…",
        Running => "Playing",
        Stopping => "Stopping…",
        Draining => "Finishing output…",
        Completed => "Completed",
        Stopped => "Stopped",
        FrameLimit => "Reached the longest session length",
        Failed => "Failed",
    }
}
/// Independent backend warning; partial monitoring must not hide reported output underruns.
pub fn output_underrun_warning(underruns: u64) -> String {
    if underruns == 0 {
        String::new()
    } else {
        format!("Backend-reported output underruns: {underruns} · see details.")
    }
}
