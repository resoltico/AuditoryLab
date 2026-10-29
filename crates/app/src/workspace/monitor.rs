//! Coherent bounded display captures and target-model provenance, shared by renderers.
use crate::audio::session::{Meter, MonitorReader, SessionDescriptor};
use crate::core::{Observation, SampleRate, waveform::Extent};
use auditorylab_core::{Target, rhythm::Structure, timing::TimingModel};
/// A view cannot silently hide invalid samples or invent source coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureError {
    /// Invalid digital samples.
    NonFiniteSample,
    /// Source frame span does not match the copied samples.
    InvalidSpan,
}
/// Visual scaling only; samples and digital levels are never normalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveformScale {
    /// Digital full scale.
    Fixed,
    /// One symmetric extent shared by both route; silence uses ±1.
    Fit,
}
/// An owned immutable view. Freeze retains this entire value, not just graph paths.
#[derive(Debug, Clone)]
pub struct WaveformCapture {
    /// Source/session captured with these samples.
    session: Option<SessionDescriptor>,
    /// Source name retained across browsing and later targets.
    title: String,
    /// Sample rate that generated the copied window.
    rate: SampleRate,
    /// Actual source frame span [start,end).
    span: (u64, u64),
    /// Publication sequence (zero for static previews).
    sequence: u64,
    gaps: u64,
    underruns: u64,
    /// Last-generated-frame state captured at the same publication.
    observation: Option<Observation>,
    /// Per-column per-ear extrema over all copied samples; at most512 bins.
    bins: Vec<[Extent; 2]>,
    /// Measured levels of exactly the captured window, independent of visual scale.
    meters: [Meter; 2],
    /// Policy captured with the display.
    scale: WaveformScale,
    /// Exact common symmetric limit; positive even for silence.
    limit: f64,
}
impl WaveformCapture {
    /// All-sample per-column extrema.
    pub fn bins(&self) -> &[[Extent; 2]] {
        &self.bins
    }
    /// Fixed-scale geometry would clip; actual samples and meters remain unchanged.
    pub fn clipped(&self) -> bool {
        self.bins
            .iter()
            .flatten()
            .any(|e| e.min < -self.limit || e.max > self.limit)
    }
    /// Common stereo display limit.
    pub fn limit(&self) -> f64 {
        self.limit
    }
    /// Measured levels of this capture.
    pub fn meters(&self) -> [Meter; 2] {
        self.meters
    }
    /// Captured source title, independent of navigation.
    pub fn title(&self) -> &str {
        &self.title
    }
    /// Captured source frame coverage.
    pub fn span(&self) -> (u64, u64) {
        self.span
    }
    /// Captured sample rate.
    pub fn rate(&self) -> SampleRate {
        self.rate
    }
    /// Monitor publication identity, or zero for preview.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Captured session, if generated audio.
    pub fn session(&self) -> Option<&SessionDescriptor> {
        self.session.as_ref()
    }
    /// Captured dropped-publication count.
    pub fn gaps(&self) -> u64 {
        self.gaps
    }
    /// Captured backend underrun count.
    pub fn underruns(&self) -> u64 {
        self.underruns
    }
    /// Last-generated-frame facts at this capture publication.
    pub fn observation(&self) -> Option<&Observation> {
        self.observation.as_ref()
    }
    /// Captured visual scale policy.
    pub fn scale(&self) -> WaveformScale {
        self.scale
    }

    /// The same captured samples under another display scale. Bins and
    /// meters are reused; only the display limit changes.
    #[must_use]
    pub fn with_scale(&self, scale: WaveformScale) -> Self {
        let mut capture = self.clone();
        capture.scale = scale;
        capture.limit = limit(&self.bins, scale);
        capture
    }

    /// Capture the current copied contiguous monitor window; no callback or recorder changes.
    pub fn monitored(
        reader: &MonitorReader,
        scale: WaveformScale,
    ) -> Result<Option<Self>, CaptureError> {
        if !reader.has_current_window() {
            return Ok(None);
        }
        let status = reader.status();
        let mut capture = Self::samples(
            reader.descriptor().title.clone(),
            reader.descriptor().output.sample_rate,
            (status.window_start, status.window_end),
            reader.samples(),
            scale,
        )?;
        capture.session = Some(reader.descriptor().clone());
        capture.observation = status.observation;
        capture.sequence = status.sequence;
        capture.gaps = status.gaps;
        capture.underruns = status.underruns;
        Ok(Some(capture))
    }
    /// Build a static, clearly separate seekable preview of at most 100 ms,
    /// starting after the fade-in.
    #[must_use]
    pub fn preview(
        title: String,
        signal: &auditorylab_core::Signal,
        window_ms: u32,
        scale: WaveformScale,
    ) -> Self {
        let start = signal.fade_frames().min(signal.frames().saturating_sub(1));
        let count = (u64::from(signal.sample_rate().hz()) * u64::from(window_ms.clamp(1, 100)))
            .div_ceil(1000)
            .max(1)
            .min(signal.frames() - start);
        let mut samples = vec![[0.0; 2]; usize::try_from(count).unwrap_or(76_800)];
        signal.render(start, &mut samples);
        // Validated synthesis is finite; if that invariant changes, fail visibly rather than draw
        // stale geometry.
        Self::samples(
            title,
            signal.sample_rate(),
            (start, start + count),
            &samples,
            scale,
        )
        .unwrap_or_else(|_| unreachable!("validated signal samples are finite"))
    }
    /// Capture a finite sample slice at the application boundary. Invalid samples fail explicitly.
    pub fn samples(
        title: String,
        rate: SampleRate,
        span: (u64, u64),
        samples: &[[f64; 2]],
        scale: WaveformScale,
    ) -> Result<Self, CaptureError> {
        if span.1.checked_sub(span.0) != u64::try_from(samples.len()).ok() {
            return Err(CaptureError::InvalidSpan);
        }
        let mut bins = Vec::new();
        auditorylab_core::waveform::sample_trace(samples, 512, &mut bins)
            .map_err(|_| CaptureError::NonFiniteSample)?;
        let limit = limit(&bins, scale);
        Ok(Self {
            session: None,
            title,
            rate,
            span,
            sequence: 0,
            gaps: 0,
            underruns: 0,
            observation: None,
            bins,
            meters: crate::audio::session::sample_meters(samples),
            scale,
            limit,
        })
    }
    /// Exact source coverage in milliseconds, not a requested span or acoustic latency estimate.
    pub fn milliseconds(&self) -> f64 {
        self.rate
            .seconds_in(self.span.1.saturating_sub(self.span.0))
            * 1000.0
    }
}
fn limit(bins: &[[Extent; 2]], scale: WaveformScale) -> f64 {
    let peak = bins
        .iter()
        .flatten()
        .fold(0.0_f64, |p, e| p.max(e.min.abs()).max(e.max.abs()));
    match scale {
        WaveformScale::Fit if peak > 0.0 => peak,
        WaveformScale::Fixed | WaveformScale::Fit => 1.0,
    }
}

/// Provenance of a pure timing model; never an observed live pulse origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSource {
    /// Validated editing draft.
    Draft,
    /// Validated library recipe.
    Recipe,
    /// Accepted target, not necessarily generated yet.
    Accepted(u64),
}
/// Shared bounded projection, cached by target; frontends only choose geometry.
#[derive(Debug, Clone)]
pub struct ModelView {
    /// Immutable validated target.
    pub target: Target,
    /// Source identity.
    pub title: String,
    /// Model provenance.
    pub source: ModelSource,
    /// Origin-zero core projection.
    pub projection: TimingModel,
    /// Physical interpretation.
    pub structure: Structure,
}
impl ModelView {
    /// Construct off the callback; at most32lanes×512columns regardless of rates.
    pub fn new(target: &Target, title: String, source: ModelSource) -> Self {
        Self {
            target: *target,
            title,
            source,
            projection: target.timing_model(256),
            structure: crate::core::rhythm::realized(target),
        }
    }
}

/// Display lifecycle, independent of the audio transport lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMonitorState {
    /// A seekable sound preview, not generated playback.
    Preview,
    /// Genuinely changed generated data received recently.
    Live,
    /// Immutable user-held capture, while current transport remains independent.
    Held,
    /// The last changed publication is too old to describe current output.
    Stale,
    /// Audio preparation has not delivered samples yet.
    Waiting,
    /// Monitoring cannot currently provide valid samples.
    Unavailable,
    /// Historical samples from an ended session.
    LastCapture,
}

/// Shared output-view state; never infer liveness from waveform shape or redraws.
pub fn output_monitor_state(
    reader: Option<&MonitorReader>,
    capture: Option<&WaveformCapture>,
    held: bool,
    preparing: bool,
) -> OutputMonitorState {
    if held && capture.is_some() {
        return OutputMonitorState::Held;
    }
    let Some(reader) = reader else {
        return if preparing {
            OutputMonitorState::Waiting
        } else if capture.is_some() {
            OutputMonitorState::Preview
        } else {
            OutputMonitorState::Unavailable
        };
    };
    if reader.status().phase.is_terminal() {
        return if capture.is_some() {
            OutputMonitorState::LastCapture
        } else {
            OutputMonitorState::Unavailable
        };
    }
    match reader.freshness() {
        crate::audio::session::ObservationFreshness::Stale => OutputMonitorState::Stale,
        crate::audio::session::ObservationFreshness::Current
            if capture.is_some() && reader.has_current_window() =>
        {
            OutputMonitorState::Live
        }
        crate::audio::session::ObservationFreshness::Waiting
        | crate::audio::session::ObservationFreshness::Current => OutputMonitorState::Waiting,
    }
}
/// A monitor-data or display warning, independent of backend-reported output underruns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorWarning {
    /// Retained observations are no longer current.
    Stale,
    /// Current samples do not yet cover the requested contiguous window.
    IncompleteCoverage,
    /// Samples exceed the selected display scale, without changing their exact levels.
    FixedRangeExceeded,
}
impl MonitorWarning {
    /// Stale and unavailable monitoring are announced automatically.
    /// Coverage and display-scale advice remain readable without repeated automatic speech.
    pub const fn announces_changes(self) -> bool {
        matches!(self, Self::Stale)
    }
}
/// Current impairment only. Cumulative skipped monitor publications remain in diagnostics.
pub fn monitor_warning(reader: &MonitorReader, window_ms: u32) -> Option<MonitorWarning> {
    let status = reader.status();
    if matches!(
        status.phase,
        crate::audio::session::PlaybackPhase::Running
            | crate::audio::session::PlaybackPhase::Stopping
            | crate::audio::session::PlaybackPhase::Draining
    ) {
        if reader.freshness() == crate::audio::session::ObservationFreshness::Stale {
            return Some(MonitorWarning::Stale);
        }
        let wanted = (u64::from(reader.descriptor().output.sample_rate.hz())
            * u64::from(window_ms.clamp(1, 100)))
        .div_ceil(1000);
        if !reader.has_current_window()
            || status.window_end.saturating_sub(status.window_start) < wanted
        {
            return Some(MonitorWarning::IncompleteCoverage);
        }
    }
    None
}

impl WaveformScale {
    /// Every visual scaling choice, in display order.
    pub const ALL: [Self; 2] = [Self::Fixed, Self::Fit];
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}

impl std::error::Error for CaptureError {}

impl std::fmt::Display for MonitorWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    CaptureError {
        codes {
            Self::NonFiniteSample => "capture.non_finite_sample",
            Self::InvalidSpan => "capture.invalid_span",
        }
        delegates {}
    }
    MonitorWarning {
        codes {
            Self::Stale => "monitor.stale",
            Self::IncompleteCoverage => "monitor.incomplete_coverage",
            Self::FixedRangeExceeded => "monitor.fixed_range_exceeded",
        }
        delegates {}
    }
}
