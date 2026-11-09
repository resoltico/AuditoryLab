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
        duration_seconds: Option<f64>,
        start: f64,
        samples: SampleAnalysis,
    ) -> Result<Self, super::RecordingError> {
        if codec.is_empty()
            || !start.is_finite()
            || start < 0.0
            || !(start + samples.window.end_seconds).is_finite()
            || duration_seconds.is_some_and(|duration| {
                !duration.is_finite()
                    || duration <= 0.0
                    || start + samples.window.end_seconds > duration
            })
        {
            return Err(super::RecordingError::InvalidWindow);
        }
        let absolute = |relative: &AnalysisWindow| AnalysisWindow {
            start_seconds: start + relative.start_seconds,
            end_seconds: start + relative.end_seconds,
        };
        Ok(Self(RecordingFacts {
            codec,
            lossy,
            duration_seconds,
            window: absolute(&samples.window),
            envelope_window: absolute(&samples.envelope_window),
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
/// Identity and analysis from the same opened file.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzedRecording {
    pub file: super::RecordingFile,
    pub analysis: Analysis,
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
