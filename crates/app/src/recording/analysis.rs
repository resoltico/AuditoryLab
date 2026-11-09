//! Validated recording requests and source composition over core measurements.
use auditorylab_core::measurement::{AnalysisWindow, SampleAnalysis};

/// Absolute selected start and at-most duration, in whole seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnalysisOptions {
    /// Absolute start, default zero; at most one day.
    pub start_seconds: u32,
    /// At-most duration, default 600; explicit range 2..=3600 seconds.
    pub seconds: Option<u32>,
}
impl AnalysisOptions {
    /// Parse shared strict unsigned decimal fields; blank start means zero.
    pub fn from_text(start: &str, duration: &str) -> Result<Self, super::RecordingError> {
        fn integer(text: &str) -> Result<Option<u32>, super::RecordingError> {
            if text.is_empty() {
                return Ok(None);
            }
            if !text.bytes().all(|b| b.is_ascii_digit()) {
                return Err(super::RecordingError::InvalidWindow);
            }
            text.parse()
                .map(Some)
                .map_err(|_| super::RecordingError::InvalidWindow)
        }
        Self {
            start_seconds: integer(start)?.unwrap_or(0),
            seconds: integer(duration)?,
        }
        .validate()
    }
    /// Validate before opening or decoding any recording.
    pub fn validate(self) -> Result<Self, super::RecordingError> {
        if self.start_seconds > 86400 || self.seconds.is_some_and(|s| !(2..=3600).contains(&s)) {
            return Err(super::RecordingError::InvalidWindow);
        }
        Ok(self)
    }
}
/// Completed recording facts composed with pure sample measurements.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordingFacts {
    pub codec: String,
    pub lossy: bool,
    /// Full source duration when trustworthy; absent for unknown/estimated durations.
    pub duration_seconds: Option<f64>,
    /// Trustworthy source frame count; absent for unknown/estimated durations.
    pub source_frames: Option<u64>,
    /// Inclusive selected frame in the admitted source timeline.
    pub start_frame: u64,
    /// Exclusive selected frame in the admitted source timeline.
    pub end_frame: u64,
    pub window: AnalysisWindow,
    pub envelope_window: AnalysisWindow,
    samples: SampleAnalysis,
}
/// Immutable completed recording; source facts cannot be changed after validation.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis(RecordingFacts);
impl std::ops::Deref for Analysis {
    type Target = RecordingFacts;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Analysis {
    /// Compose source facts and relative findings after checking their placement.
    pub fn complete(
        codec: String,
        lossy: bool,
        source_frames: Option<u64>,
        start_frame: u64,
        samples: SampleAnalysis,
    ) -> Result<Self, super::RecordingError> {
        let end_frame = start_frame
            .checked_add(samples.frames)
            .ok_or(super::RecordingError::InvalidWindow)?;
        if codec.is_empty() || source_frames.is_some_and(|total| end_frame > total) {
            return Err(super::RecordingError::InvalidWindow);
        }
        let rate = f64::from(samples.sample_rate);
        #[expect(
            clippy::cast_precision_loss,
            reason = "displayed source timeline; admission and endpoints use exact integer frame arithmetic"
        )]
        let seconds = |frame: u64| frame as f64 / rate;
        let duration_seconds = source_frames.map(seconds);
        Ok(Self(RecordingFacts {
            codec,
            lossy,
            duration_seconds,
            source_frames,
            start_frame,
            end_frame,
            window: AnalysisWindow {
                start_seconds: seconds(start_frame),
                end_seconds: seconds(end_frame),
            },
            envelope_window: AnalysisWindow {
                start_seconds: seconds(start_frame + samples.envelope_edge_frames),
                end_seconds: seconds(end_frame - samples.envelope_edge_frames),
            },
            samples,
        }))
    }
    /// Immutable sample findings, independent of file facts and placement.
    pub fn samples(&self) -> &SampleAnalysis {
        &self.samples
    }
}
impl std::ops::Deref for RecordingFacts {
    type Target = SampleAnalysis;
    fn deref(&self) -> &Self::Target {
        &self.samples
    }
}
/// Read-only identity and complete findings from the same admitted source.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletedRecordingFacts {
    /// File identity captured from the admitted handle.
    pub file: super::RecordingFile,
    /// Complete source facts and measurements from that handle.
    pub analysis: Analysis,
}
/// Immutable completed recording; only the admitted decoder composes this value.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzedRecording(CompletedRecordingFacts);
impl AnalyzedRecording {
    pub(crate) fn complete(file: super::RecordingFile, analysis: Analysis) -> Self {
        Self(CompletedRecordingFacts { file, analysis })
    }
}
impl std::ops::Deref for AnalyzedRecording {
    type Target = CompletedRecordingFacts;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Current device answers interpreted independently of retained sample findings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InterpretationContext {
    assessment: crate::device_check::Assessment,
    lowest_heard_hz: Option<f64>,
}
impl From<crate::device_check::Assessment> for InterpretationContext {
    fn from(assessment: crate::device_check::Assessment) -> Self {
        use crate::device_check::{DeviceTest, LowToneResult};
        let lowest_heard_hz = match assessment.low_tones {
            LowToneResult::AllHeard => Some(DeviceTest::Tone40Hz.frequency_hz()),
            LowToneResult::LowestHeard { test } => Some(test.frequency_hz()),
            LowToneResult::Incomplete | LowToneResult::NoneHeard => None,
        };
        Self {
            assessment,
            lowest_heard_hz,
        }
    }
}
impl InterpretationContext {
    /// Current assessment, without changing or recomputing samples.
    pub const fn assessment(self) -> crate::device_check::Assessment {
        self.assessment
    }
    /// Lowest low tone the listener reported hearing, when known.
    pub const fn lowest_heard_hz(self) -> Option<f64> {
        self.lowest_heard_hz
    }
}
