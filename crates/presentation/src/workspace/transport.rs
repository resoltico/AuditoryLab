//! workspace transport presentation.
use crate::numbers::NumberStyle;
use crate::*;
use auditorylab_core::Sound;
/// Shared listening guidance.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// Start quietly.
    LowStartingVolume,
    /// Use separate ear channels.
    HeadphonesForSeparateEars,
    /// Reported buffer underruns.
    Underruns(u64),
}

impl std::fmt::Display for Notice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LowStartingVolume => f.write_str("Turn the volume down before playing."),
            Self::HeadphonesForSeparateEars => {
                f.write_str("Use headphones for sounds that differ between the ears.")
            }
            Self::Underruns(n) => write!(f, "The audio backend reported {n} buffer underruns."),
        }
    }
}

/// A save completed for an older snapshot while newer edits remain.

pub(crate) const SAVED_SNAPSHOT_STILL_DIRTY: &str =
    "Saved snapshot; newer edits remain unsaved. The sound remains open.";

/// A worker thread panicked.

pub(crate) const WORKER_FAILED: &str = "The operation thread failed.";

/// A worker thread ended without reporting a result.

/// An export ended at the person's request.

pub(crate) const EXPORT_CANCELLED: &str = "Export cancelled; no file was written.";

impl Notice {
    /// Persistent desktop listening guidance, independent of the current selection.
    #[must_use]
    pub fn general_guidance() -> String {
        format!(
            "{} {}",
            Self::LowStartingVolume,
            Self::HeadphonesForSeparateEars
        )
    }

    /// Guidance shown before playing a sound.
    #[must_use]
    pub fn for_sound(sound: &Sound) -> Vec<Notice> {
        let mut notices = vec![Notice::LowStartingVolume];
        if sound.uses_separate_ears() {
            notices.push(Notice::HeadphonesForSeparateEars);
        }
        notices
    }
}

/// A monitored window that cannot be drawn truthfully.

pub fn capture_error_description(
    value: auditorylab_app::workspace::monitor::CaptureError,
) -> String {
    let error = value;
    match error {
        auditorylab_app::workspace::monitor::CaptureError::NonFiniteSample => {
            "Invalid waveform capture (non-finite samples); retaining the last valid window".into()
        }
        auditorylab_app::workspace::monitor::CaptureError::InvalidSpan => {
            "Invalid waveform capture (inconsistent span); retaining the last valid window".into()
        }
    }
}

/// The result of a playback, for any frontend.

pub fn playback_report_description(
    value: &auditorylab_app::services::playback::PlaybackReport,
) -> String {
    let report = value;
    let mut text = format!(
        "{} {} frames generated; about {} output.",
        match report.end {
            auditorylab_app::services::playback::EndReason::Completed => "Played to the end.",
            auditorylab_app::services::playback::EndReason::Stopped if report.drained => "Stopped.",
            auditorylab_app::services::playback::EndReason::Stopped =>
                "Stopped; queued output was cut off after the stop limit.",
            auditorylab_app::services::playback::EndReason::FrameLimit =>
                "Reached the longest supported session.",
        },
        report.generated_frames,
        report.estimated_output_frames
    );
    if report.underruns > 0 {
        text.push('\n');
        text.push_str(&Notice::Underruns(report.underruns).to_string());
    }
    text
}

/// One status line for a playing session.

pub fn monitor_reader_status_text(
    value: &auditorylab_app::services::playback::MonitorReader,
) -> String {
    let reader = value;
    let status = reader.status();
    format!(
        "{} · {} · {} · {}",
        crate::numbers::format(
            NumberStyle::Clock,
            (reader.descriptor().output.sample_rate).seconds_in(status.estimated_output_frames)
        ),
        crate::workspace::monitor::phase_text(status.phase),
        match reader.freshness() {
            auditorylab_app::services::playback::ObservationFreshness::Waiting =>
                "waiting for generated audio",
            auditorylab_app::services::playback::ObservationFreshness::Current => "current",
            auditorylab_app::services::playback::ObservationFreshness::Stale => "stale",
        },
        crate::optional_observation_summary(&(status.observation.as_ref()))
    )
}

/// Estimated output frame count as a transport clock at this output's rate.

pub fn output_info_elapsed(
    value: &auditorylab_app::services::playback::OutputInfo,
    frames: u64,
) -> String {
    clock(value.sample_rate.seconds_in(frames))
}

/// Explain why an editor command was refused without confusing authored components and voices.

pub fn edit_refusal_description(value: auditorylab_app::document::editing::EditRefusal) -> String {
    use auditorylab_app::document::editing::EditRefusal;
    match value {
        EditRefusal::BudgetExceeded {
            components,
            voices,
            additional_voices,
        } => {
            let limit = match (
                components >= Sound::MAX_COMPONENTS,
                voices + additional_voices > Sound::MAX_VOICES,
            ) {
                (true, true) => format!(
                    "the {}-component and {}-voice limits",
                    Sound::MAX_COMPONENTS,
                    Sound::MAX_VOICES
                ),
                (true, false) => format!("the {}-component limit", Sound::MAX_COMPONENTS),
                _ => format!("the {}-voice limit", Sound::MAX_VOICES),
            };
            format!(
                "This sound has {components} authored components and {voices} expanded voices. Adding this component requires {additional_voices} more {}; it would exceed {limit}. Each pair uses two voices.",
                if additional_voices == 1 {
                    "voice"
                } else {
                    "voices"
                }
            )
        }
        EditRefusal::NothingRemoved => "No removed component is waiting to be restored.".into(),
        EditRefusal::IdentitiesExhausted => {
            "This sound has exhausted an editing identity; start a new sound to continue.".into()
        }
        EditRefusal::StaleDocument
        | EditRefusal::StaleRow
        | EditRefusal::UnknownComponent
        | EditRefusal::Inapplicable => "That field no longer belongs to the open sound.".into(),
    }
}

/// Human names for the actual device sample representation.

pub fn sample_format_label(
    value: auditorylab_app::services::playback::SampleFormat,
) -> &'static str {
    match value {
        auditorylab_app::services::playback::SampleFormat::F64 => "64-bit float",
        auditorylab_app::services::playback::SampleFormat::F32 => "32-bit float",
        auditorylab_app::services::playback::SampleFormat::I32 => "32-bit integer",
        auditorylab_app::services::playback::SampleFormat::I24 => "24-bit integer",
        auditorylab_app::services::playback::SampleFormat::I16 => "16-bit integer",
    }
}
