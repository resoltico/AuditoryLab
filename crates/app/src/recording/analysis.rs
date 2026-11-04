//! Typed signal findings, independent of presentation and media containers.
use crate::core::rhythm::Band;
use serde::Serialize;

/// Captured analysis request. The offset is 10% of the recording's duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnalysisOptions {
    /// Requested duration in seconds, or the default of at most ten minutes.
    pub seconds: Option<u32>,
}
impl AnalysisOptions {
    /// Interpret an optional integer field. Invalid input is retained as an invalid
    /// request so the normal worker completion reports a typed window failure.
    pub fn from_seconds_text(text: &str) -> Self {
        let text = text.trim();
        Self {
            seconds: if text.is_empty() {
                None
            } else {
                Some(text.parse().unwrap_or(0))
            },
        }
    }

    /// Validate a requested duration before opening or decoding a recording.
    pub fn validate(self) -> Result<Self, super::RecordingError> {
        if self.seconds.is_some_and(|seconds| seconds < 2) {
            return Err(super::RecordingError::InvalidWindow);
        }
        Ok(self)
    }
}
/// Window actually measured, in the decoded sample timeline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnalysisWindow {
    pub start_seconds: f64,
    pub end_seconds: f64,
}
/// A detected local spectral maximum in one channel.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tone {
    pub channel: usize,
    pub hz: f64,
    pub dbfs: f64,
    pub relative_db: f64,
}
/// A pair of channel-exclusive pitches consistent with a binaural difference.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EarDifference {
    pub left_hz: f64,
    pub right_hz: f64,
    pub difference_hz: f64,
    pub band: Option<Band>,
}
/// Evidence for an envelope rate; a frequency match does not establish intent.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PulsingCause {
    Beating {
        first_hz: f64,
        second_hz: f64,
        harmonic: u32,
    },
    Modulation,
}
/// Frequency range whose analytic amplitude envelope was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeBand {
    Whole,
    Below100,
    Above100,
}
impl EnvelopeBand {
    /// Measurement and presentation order for every envelope band.
    pub const ALL: [Self; 3] = [Self::Whole, Self::Below100, Self::Above100];
    pub(super) fn includes(self, hz: f64) -> bool {
        match self {
            Self::Whole => true,
            Self::Below100 => hz <= 100.0,
            Self::Above100 => hz > 100.0,
        }
    }
}
/// A significant analytic-envelope rate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pulsing {
    pub channel: usize,
    pub envelope_band: EnvelopeBand,
    pub hz: f64,
    pub band: Option<Band>,
    pub cause: PulsingCause,
}
/// Cumulative fraction of measured PSD energy below a threshold.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EnergyShare {
    pub channel: usize,
    pub below_hz: f64,
    /// None for digital silence, whose total energy is zero.
    pub fraction: Option<f64>,
}
/// Channel levels and stereo relationships; undefined values remain absent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stereo {
    pub peak_dbfs: Vec<Option<f64>>,
    pub rms_dbfs: Vec<Option<f64>>,
    pub correlation: Option<f64>,
    pub side_to_mid_db: Option<f64>,
}
/// A chart point; PSD is power per Hz in digital full-scale units.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpectrumPoint {
    pub hz: f64,
    pub dbfs_per_hz: f64,
}
/// Interpretive limits carried in machine reports independently of human wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementLimit {
    DigitalSamplesOnly,
    QuietPairsMayBeMissed,
    PatternAttributionNotIntent,
    PeaksBelowRelativeFloorOmitted,
}
/// Complete measurements of one captured window, with their interpretation limits.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Analysis {
    pub codec: String,
    pub lossy: bool,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: usize,
    pub window: AnalysisWindow,
    pub envelope_window: AnalysisWindow,
    pub resolution_hz: f64,
    pub envelope_resolution_hz: f64,
    /// Rates below this frequency are unresolved, not absent.
    pub minimum_resolved_envelope_hz: f64,
    pub minimum_envelope_relative_power: f64,
    pub tones: Vec<Tone>,
    pub ear_differences: Vec<EarDifference>,
    /// True only for stereo with detected peaks matching in both directions.
    pub coincident_tones: bool,
    pub pulsing: Vec<Pulsing>,
    pub low_energy: Vec<EnergyShare>,
    pub stereo: Stereo,
    pub spectrum: Vec<Vec<SpectrumPoint>>,
    pub limits: Vec<MeasurementLimit>,
}
/// Identity and analysis from the same opened file.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzedRecording {
    pub file: super::RecordingFile,
    pub analysis: Analysis,
}
