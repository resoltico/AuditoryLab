//! Wording for monitor captures and model views.
use crate::vocabulary::NumberStyle;
use auditorylab_app::workspace::monitor::ModelSource;
use auditorylab_app::workspace::monitor::ModelView;
use auditorylab_app::workspace::monitor::MonitorWarning;
use auditorylab_app::workspace::monitor::OutputMonitorState;
use auditorylab_app::workspace::monitor::WaveformCapture;
use auditorylab_app::workspace::monitor::WaveformScale;
/// Wording for the captured WaveformCapture data.
pub trait WaveformCaptureText {
    /// Concise label derived only from this captured window, never current navigation.
    fn label(&self, frozen: bool, ended: bool) -> String;
    /// Full frozen/retained context; current generated status is presented independently.
    fn caption(&self, frozen: bool, ended: bool) -> String;
}
impl WaveformCaptureText for WaveformCapture {
    /// Concise label derived only from this captured window, never current navigation.
    fn label(&self, frozen: bool, ended: bool) -> String {
        format!(
            "{} · {} · {:.2} ms · {} ±{}",
            if frozen {
                "Held capture"
            } else if ended {
                "Last capture"
            } else if self.session().is_some() {
                "Generated waveform"
            } else {
                "Preview · not playing"
            },
            self.title(),
            self.milliseconds(),
            if self.scale() == WaveformScale::Fit {
                "Fit"
            } else {
                "Fixed"
            },
            NumberStyle::Compact.format(self.limit())
        )
    }
    /// Full frozen/retained context; current generated status is presented independently.
    fn caption(&self, frozen: bool, ended: bool) -> String {
        format!(
            "{} · session {} · sample rate {} Hz · frames {}..{} · snapshot {} · observed frame {} · captured gaps {} / underruns {} · exact scale ±{}",
            self.label(frozen, ended),
            self.session()
                .map_or_else(|| "preview".into(), |session| session.id.to_string()),
            self.rate().hz(),
            self.span().0,
            self.span().1,
            self.sequence(),
            self.observation()
                .map_or_else(|| "none yet".to_owned(), |o| o.frame.to_string()),
            self.gaps(),
            self.underruns(),
            self.limit()
        )
    }
}

/// Wording for the captured ModelView data.
pub trait ModelViewText {
    /// Explicit model rate/origin/provenance, with insufficient-cycle warning when capped.
    fn caption(&self) -> String;
    /// Shared labels for physical lanes; a frontend only renders their geometry.
    fn lane_label(lane: &auditorylab_core::timing::ModelLane) -> String;
    /// Relate a model's accepted revision to actual generation without replaying live phases.
    fn generation_note(
        &self,
        status: auditorylab_app::services::playback::GeneratedStatus,
    ) -> String;
}
impl ModelViewText for ModelView {
    /// Explicit model rate/origin/provenance, with insufficient-cycle warning when capped.
    fn caption(&self) -> String {
        format!(
            "Timing model · {} · {} · origin 0 · {} Hz · {:.1} ms · projection, not recorded output{}",
            self.title,
            match self.source {
                ModelSource::Draft => "committed draft".into(),
                ModelSource::Recipe => "validated recipe".into(),
                ModelSource::Accepted(r) =>
                    format!("accepted target revision {r}; not phase history"),
            },
            self.projection.rate.hz(),
            self.projection.rate.seconds_in(self.projection.frames) * 1000.0,
            if self.projection.insufficient_cycle {
                " · less than one cycle at a slow rate"
            } else {
                ""
            }
        )
    }
    /// Shared labels for physical lanes; a frontend only renders their geometry.
    fn lane_label(lane: &auditorylab_core::timing::ModelLane) -> String {
        use auditorylab_core::timing::ModelKind;
        let quantity = match lane.kind {
            ModelKind::Steady => match lane.facts.carrier {
                auditorylab_core::CarrierFacts::Tone(rate) => {
                    format!(
                        "{} Hz steady tone",
                        NumberStyle::Compact.format(rate.realized_hz)
                    )
                }
                _ => "Steady carrier".into(),
            },
            ModelKind::Clicks => match lane.facts.carrier {
                auditorylab_core::CarrierFacts::Clicks(p) => {
                    format!("{} Hz clicks", NumberStyle::Compact.format(p.rate_hz))
                }
                _ => unreachable!("click lane carrier"),
            },
            ModelKind::Gate => match lane.facts.modulation {
                Some(auditorylab_core::ModulationFacts::Gate(p)) => {
                    format!("{} Hz gate", NumberStyle::Compact.format(p.rate_hz))
                }
                _ => unreachable!("gate lane modulation"),
            },
            ModelKind::Envelope => match lane.facts.modulation {
                Some(auditorylab_core::ModulationFacts::Am { rate, depth }) => format!(
                    "{} Hz AM · {}%",
                    NumberStyle::Compact.format(rate.realized_hz),
                    NumberStyle::Compact.format(depth * 100.0)
                ),
                _ => unreachable!("envelope lane modulation"),
            },
        };
        format!(
            "{} · {quantity}",
            super::acoustic_errors::voice_reference(lane.id)
        )
    }
    /// Relate a model's accepted revision to actual generation without replaying live phases.
    fn generation_note(
        &self,
        status: auditorylab_app::services::playback::GeneratedStatus,
    ) -> String {
        let ModelSource::Accepted(revision) = self.source else {
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
}

/// Wording for the captured OutputMonitorState data.
pub trait OutputMonitorStateText {
    /// Concise status for any frontend.
    fn label(self) -> &'static str;
}
impl OutputMonitorStateText for OutputMonitorState {
    /// Concise status for any frontend.
    fn label(self) -> &'static str {
        match self {
            Self::Preview => "Preview",
            Self::Live => "Live",
            Self::Held => "Held",
            Self::Stale => "Stale",
            Self::Waiting => "Waiting",
            Self::Unavailable => "Unavailable",
            Self::LastCapture => "Last capture",
        }
    }
}

/// Wording for the captured MonitorWarning data.
pub trait MonitorWarningText {
    /// Visible and accessible text for this current condition.
    fn message(self) -> &'static str;
}
impl MonitorWarningText for MonitorWarning {
    /// Visible and accessible text for this current condition.
    fn message(self) -> &'static str {
        match self {
            Self::Stale => "Monitoring is stale; retained samples are not current output.",
            Self::IncompleteCoverage => {
                "Incomplete current waveform coverage; waiting for a contiguous window."
            }
            Self::FixedRangeExceeded => {
                "Waveform exceeds the fixed display range; exact levels are retained. Use Fit to view the full extent."
            }
        }
    }
}

/// Wording for the captured WaveformScale data.
pub trait WaveformScaleText {
    /// Display vocabulary for this scale.
    fn terms(self) -> (&'static str, &'static str);
    /// Domain name for a heading or choice.
    fn label(self) -> &'static str;
    /// Domain name within a sentence.
    fn phrase(self) -> &'static str;
}
impl WaveformScaleText for WaveformScale {
    fn terms(self) -> (&'static str, &'static str) {
        match self {
            Self::Fixed => ("Fixed ±1", "fixed ±1"),
            Self::Fit => ("Fit signal", "fit signal"),
        }
    }
    /// Domain name for a heading or choice.
    fn label(self) -> &'static str {
        self.terms().0
    }
    /// Domain name within a sentence.
    fn phrase(self) -> &'static str {
        self.terms().1
    }
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
