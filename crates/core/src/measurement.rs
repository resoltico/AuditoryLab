//! Pure digital sample measurements and findings; no device, file or job access.
use crate::rhythm::Band;

/// Window actually measured, in the decoded sample timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisWindow {
    /// Inclusive relative start in seconds.
    pub start_seconds: f64,
    /// Exclusive relative end in seconds.
    pub end_seconds: f64,
}
/// A detected local spectral maximum in one channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Tone {
    /// Zero-based channel index.
    pub channel: usize,
    /// Measured frequency in hertz.
    pub hz: f64,
    /// Integrated peak power in dB relative to digital full scale.
    pub dbfs: f64,
    /// Power in dB relative to the strongest detected peak.
    pub relative_db: f64,
}
/// A pair of channel-exclusive pitches consistent with a binaural difference.
#[derive(Debug, Clone, PartialEq)]
pub struct EarDifference {
    /// Left-channel pitch in hertz.
    pub left_hz: f64,
    /// Right-channel pitch in hertz.
    pub right_hz: f64,
    /// Absolute pitch difference in hertz.
    pub difference_hz: f64,
    /// Research band containing the measured frequency, when any.
    pub band: Option<Band>,
}
/// Evidence for an envelope rate; a frequency match does not establish intent.
#[derive(Debug, Clone, PartialEq)]
pub enum PulsingCause {
    /// Pattern consistent with beating between detected pitches.
    Beating {
        /// First beating pitch in hertz.
        first_hz: f64,
        /// Second beating pitch in hertz.
        second_hz: f64,
        /// Positive harmonic index relative to the pitch difference.
        harmonic: u32,
    },
    /// Pattern consistent with amplitude modulation.
    Modulation,
}
/// Frequency range whose analytic amplitude envelope was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeBand {
    /// Full source frequency range.
    Whole,
    /// Frequencies at or below 100 Hz.
    Below100,
    /// Frequencies above 100 Hz.
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
#[derive(Debug, Clone, PartialEq)]
pub struct Pulsing {
    /// Zero-based channel index.
    pub channel: usize,
    /// Frequency region used for the analytic envelope.
    pub envelope_band: EnvelopeBand,
    /// Measured frequency in hertz.
    pub hz: f64,
    /// Research band containing the measured frequency, when any.
    pub band: Option<Band>,
    /// Spectral pattern attribution; does not establish authored intent.
    pub cause: PulsingCause,
}
/// Cumulative fraction of measured PSD energy below a threshold.
#[derive(Debug, Clone, PartialEq)]
pub struct EnergyShare {
    /// Zero-based channel index.
    pub channel: usize,
    /// Exclusive upper energy threshold in hertz.
    pub below_hz: f64,
    /// None for digital silence, whose total energy is zero.
    pub fraction: Option<f64>,
}
/// Channel levels and stereo relationships; undefined values remain absent.
#[derive(Debug, Clone, PartialEq)]
pub struct Stereo {
    /// Per-channel peak levels; silence is absent.
    pub peak_dbfs: Vec<Option<f64>>,
    /// Per-channel root-mean-square levels; silence is absent.
    pub rms_dbfs: Vec<Option<f64>>,
    /// Stereo correlation; absent for mono or zero variance.
    pub correlation: Option<f64>,
    /// Stereo side-to-mid power ratio in dB when defined.
    pub side_to_mid_db: Option<f64>,
}
/// A chart point; PSD is power per Hz in digital full-scale units.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectrumPoint {
    /// Measured frequency in hertz.
    pub hz: f64,
    /// Power spectral density in dB full scale per hertz.
    pub dbfs_per_hz: f64,
}
/// Interpretive limits carried in machine reports independently of human wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementLimit {
    /// Samples do not establish device or acoustic output.
    DigitalSamplesOnly,
    /// Quiet channel-exclusive pairs may escape detection.
    QuietPairsMayBeMissed,
    /// Spectral patterns cannot identify creator intent.
    PatternAttributionNotIntent,
    /// Peaks below the relative reporting floor are omitted.
    PeaksBelowRelativeFloorOmitted,
}
/// Complete measurements of one captured window, with their interpretation limits.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleFacts {
    /// Decoded source frames per second, independent of synthesis admission.
    pub sample_rate: u32,
    /// Number of channels, one or two.
    pub channels: usize,
    /// Actual measured extent relative to the first selected frame.
    pub window: AnalysisWindow,
    /// Actual usable envelope extent after FFT edge exclusion.
    pub envelope_window: AnalysisWindow,
    /// Tone spectral bin spacing in hertz.
    pub resolution_hz: f64,
    /// Envelope spectral bin spacing in hertz.
    pub envelope_resolution_hz: f64,
    /// Rates below this frequency are unresolved, not absent.
    pub minimum_resolved_envelope_hz: f64,
    /// Minimum envelope peak power relative to squared mean amplitude.
    pub minimum_envelope_relative_power: f64,
    /// Detected channel-local spectral maxima.
    pub tones: Vec<Tone>,
    /// Detected channel-exclusive pitch differences.
    pub ear_differences: Vec<EarDifference>,
    /// True only for stereo with detected peaks matching in both directions.
    pub coincident_tones: bool,
    /// Detected analytic-envelope rates.
    pub pulsing: Vec<Pulsing>,
    /// Energy shares at documented threshold frequencies.
    pub low_energy: Vec<EnergyShare>,
    /// Measured channel levels and relationships.
    pub stereo: Stereo,
    /// Per-channel chart points of power spectral density.
    pub spectrum: Vec<Vec<SpectrumPoint>>,
    /// Limits of interpretation applying to these findings.
    pub limits: Vec<MeasurementLimit>,
}

/// Immutable completed sample findings, constructed only by measurement completion.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleAnalysis(SampleFacts);
impl std::ops::Deref for SampleAnalysis {
    type Target = SampleFacts;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

mod accumulate;
pub use accumulate::{MIN_ENVELOPE_RELATIVE_POWER, Measurement};

/// Refusal or incomplete pure-sample measurement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeasurementError {
    /// Unsupported mono/stereo channel layout.
    UnsupportedChannels {
        /// Rejected channel count.
        channels: usize,
    },
    /// Fewer than two seconds or invalid source rate.
    TooShort,
    /// Channel mismatch, nonfinite sample, or excess frames.
    InvalidFrame,
    /// Cooperative cancellation.
    Cancelled,
    /// Owned work budget exceeded before allocation or processing.
    BudgetExceeded,
}

impl EnvelopeBand {
    /// Stable semantic key; chart captions and wire shape are separate owners.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::Below100 => "below100",
            Self::Above100 => "above100",
        }
    }
}
impl MeasurementLimit {
    /// Stable semantic key for the interpretation limit.
    pub const fn key(self) -> &'static str {
        match self {
            Self::DigitalSamplesOnly => "digital_samples_only",
            Self::QuietPairsMayBeMissed => "quiet_pairs_may_be_missed",
            Self::PatternAttributionNotIntent => "pattern_attribution_not_intent",
            Self::PeaksBelowRelativeFloorOmitted => "peaks_below_relative_floor_omitted",
        }
    }
}
