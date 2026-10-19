//! Coherent bounded display captures and target-model provenance, shared by renderers.
use crate::{
    Extent, Meter, MonitorReader, Observation, SampleRate, SessionDescriptor, SoundSummary,
};
use auditorylab_core::{live::Target, timing::TimingModel};
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
    /// One symmetric extent shared by both ears; silence uses ±1.
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
    /// Last-generated-frame facts at this capture publication.
    pub fn observation(&self) -> Option<&Observation> {
        self.observation.as_ref()
    }
    /// Captured visual scale policy.
    pub fn scale(&self) -> WaveformScale {
        self.scale
    }

    /// Capture the current copied contiguous monitor window; no callback or recorder changes.
    pub fn monitored(
        reader: &MonitorReader,
        scale: WaveformScale,
    ) -> Result<Option<Self>, CaptureError> {
        if reader.samples().is_empty() {
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
    /// Build a static, clearly separate seekable preview, bounded to100ms.
    pub fn preview(sound: &crate::Sound, window_ms: u32, scale: WaveformScale) -> Self {
        let signal = sound.signal();
        let start = signal.fade_frames().min(signal.frames().saturating_sub(1));
        let count = (u64::from(signal.sample_rate().hz()) * u64::from(window_ms.clamp(1, 100)))
            .div_ceil(1000)
            .max(1)
            .min(signal.frames() - start);
        let mut samples = vec![[0.0; 2]; usize::try_from(count).unwrap_or(76_800)];
        signal.render(start, &mut samples);
        // Validated synthesis is finite; if that invariant changes, fail visibly rather than draw stale geometry.
        Self::samples(
            sound.preset().title(),
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
        crate::sample_trace(samples, 512, &mut bins).map_err(|_| CaptureError::NonFiniteSample)?;
        let peak = bins
            .iter()
            .flatten()
            .fold(0.0_f64, |p, e| p.max(e.min.abs()).max(e.max.abs()));
        let limit = match scale {
            WaveformScale::Fixed => 1.0,
            WaveformScale::Fit if peak > 0.0 => peak,
            WaveformScale::Fit => 1.0,
        };
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
            meters: crate::sample_meters(samples),
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
    /// Concise label derived only from this captured window, never current navigation.
    pub fn label(&self, frozen: bool, ended: bool) -> String {
        format!(
            "{} · {} · {:.2} ms · {} ±{}",
            if frozen {
                "Frozen capture"
            } else if ended {
                "Last waveform"
            } else if self.session.is_some() {
                "Generated waveform"
            } else {
                "Preview · not playing"
            },
            self.title,
            self.milliseconds(),
            if self.scale == WaveformScale::Fit {
                "Fit"
            } else {
                "Fixed"
            },
            crate::compact_number(self.limit)
        )
    }
    /// Full frozen/retained context; current generated status is presented independently.
    pub fn caption(&self, frozen: bool, ended: bool) -> String {
        format!(
            "{} · sample rate {} Hz · frames {}..{} · snapshot {} · observed frame {:?} · captured gaps {} / underruns {} · exact scale ±{}",
            self.label(frozen, ended),
            self.rate.hz(),
            self.span.0,
            self.span.1,
            self.sequence,
            self.observation.map(|o| o.frame),
            self.gaps,
            self.underruns,
            self.limit
        )
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
    pub summary: SoundSummary,
}
impl ModelView {
    /// Construct off the callback; at most32lanes×512columns regardless of rates.
    pub fn new(target: Target, title: String, source: ModelSource) -> Self {
        Self {
            target,
            title,
            source,
            projection: target.timing_model(256),
            summary: SoundSummary::target(&target),
        }
    }
    /// Explicit model rate/origin/provenance, with insufficient-cycle warning when capped.
    pub fn caption(&self) -> String {
        format!(
            "Timing model · {} · {} · origin 0 · {} Hz · {:.1} ms · projection, not recorded output{}",
            self.title,
            match self.source {
                ModelSource::Draft => "last valid draft".into(),
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
}
/// Shared lifecycle wording; terminal silence never relabels a session's recipe.
pub const fn phase_text(phase: crate::PlaybackPhase) -> &'static str {
    use crate::PlaybackPhase::*;
    match phase {
        Preparing => "Preparing output…",
        Running => "Playing",
        Stopping => "Stopping…",
        Draining => "Finishing output…",
        Completed => "Completed",
        Stopped => "Stopped",
        Failed => "Failed",
    }
}

impl ModelView {
    /// Shared labels for physical lanes; a frontend only renders their geometry.
    pub fn lane_label(lane: &auditorylab_core::timing::ModelLane) -> String {
        use auditorylab_core::timing::ModelKind;
        let quantity = match lane.kind {
            ModelKind::Steady => match lane.facts.carrier {
                crate::CarrierFacts::Tone(rate) => {
                    format!("{} Hz steady tone", crate::compact_number(rate.realized_hz))
                }
                _ => "Steady carrier".into(),
            },
            ModelKind::Clicks => match lane.facts.carrier {
                crate::CarrierFacts::Clicks(p) => {
                    format!("{} Hz clicks", crate::compact_number(p.rate_hz))
                }
                _ => unreachable!("click lane carrier"),
            },
            ModelKind::Gate => match lane.facts.modulation {
                Some(crate::ModulationFacts::Gate(p)) => {
                    format!("{} Hz gate", crate::compact_number(p.rate_hz))
                }
                _ => unreachable!("gate lane modulation"),
            },
            ModelKind::Envelope => match lane.facts.modulation {
                Some(crate::ModulationFacts::Sinusoidal { rate, depth }) => format!(
                    "{} Hz AM · {}%",
                    crate::compact_number(rate.realized_hz),
                    depth * 100.0
                ),
                _ => unreachable!("envelope lane modulation"),
            },
        };
        format!("Component {} · {quantity}", lane.id)
    }
}

impl ModelView {
    /// Relate a model's accepted revision to actual generation without replaying live phases.
    pub fn generation_note(&self, status: crate::GeneratedStatus) -> String {
        let ModelSource::Accepted(revision) = self.source else {
            return String::new();
        };
        let ended = matches!(
            status.phase,
            crate::PlaybackPhase::Stopped
                | crate::PlaybackPhase::Completed
                | crate::PlaybackPhase::Failed
        );
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
