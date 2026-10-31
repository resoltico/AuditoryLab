//! Sample-rate resolution and requested/resolved pulse facts.
use crate::PulseSpec;
/// A pulse cannot be realized at the selected sample rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PulseError {
    /// Repetition must be positive and below Nyquist.
    Rate,
    /// Width must resolve to 1..=2^36 frames.
    Width,
    /// Duty must be strictly between zero and one.
    Duty,
    /// The exact closed-frame condition fails.
    ClosedFrame,
    /// Positive edges must resolve and fit, including both zero endpoints.
    Edges,
}
impl PulseError {
    /// Stable machine reason within a pulse-resolution diagnostic.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Rate => "rate",
            Self::Width => "width",
            Self::Duty => "duty",
            Self::ClosedFrame => "closed_frame",
            Self::Edges => "edges",
        }
    }
}
impl core::fmt::Display for PulseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Rate => "pulse repetition must be positive and below Nyquist",
            Self::Width => "pulse width must resolve to 1..=2^36 frames",
            Self::Duty => "pulse duty must be strictly between zero and one",
            Self::ClosedFrame => "pulse width must leave a closed frame",
            Self::Edges => "positive pulse edges must resolve to frames and fit the window",
        })
    }
}
impl core::error::Error for PulseError {}

/// A sample-rate-specific pulse, with requested intent and generated facts together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedPulse {
    requested: PulseSpec,
    rate: crate::SampleRate,
    generator: crate::synthesis::Pulse,
}
impl PulseSpec {
    /// Resolve the exact repetition clock, width and edge frames.
    pub fn resolve(self, rate: crate::SampleRate) -> Result<ResolvedPulse, PulseError> {
        Ok(ResolvedPulse {
            requested: self,
            rate,
            generator: crate::synthesis::pulse_spec(self, rate.hz())?,
        })
    }
}
impl ResolvedPulse {
    /// The original request, including its width mode.
    pub const fn requested(self) -> PulseSpec {
        self.requested
    }
    /// Total window length, including zero-valued shaped endpoints.
    pub const fn width_frames(self) -> u64 {
        self.generator.frames()
    }
    /// Resolved rise and fall lengths; both zero for rectangular pulses.
    pub const fn edge_frames(self) -> [u64; 2] {
        self.generator.edges()
    }
    /// Generated window duration in seconds.
    pub fn width_seconds(self) -> f64 {
        self.rate.seconds_in(self.width_frames())
    }
    /// Derived nominal duty; fractional-period intervals can differ.
    pub fn nominal_duty(self) -> f64 {
        self.width_seconds() * self.requested.rate_hz.get()
    }
    /// Exact extrema over a nonempty discrete frame interval within the horizon.
    pub fn extent(self, start: u64, end: u64) -> Option<crate::waveform::Extent> {
        (start < end && end <= crate::Signal::MAX_FRAMES).then(|| self.generator.extent(start, end))
    }
    /// Gain at an origin-zero frame, or zero beyond the rendering horizon.
    pub fn sample(self, frame: u64) -> f64 {
        if frame >= crate::Signal::MAX_FRAMES {
            0.0
        } else {
            self.generator.sample(frame)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::finite;

    #[test]
    fn errors_have_distinct_stable_keys_and_explanations() {
        let errors = [
            (PulseError::Rate, "rate"),
            (PulseError::Width, "width"),
            (PulseError::Duty, "duty"),
            (PulseError::ClosedFrame, "closed_frame"),
            (PulseError::Edges, "edges"),
        ];
        let mut messages = alloc::vec::Vec::new();
        for (error, key) in errors {
            assert_eq!(error.key(), key);
            let message = alloc::string::ToString::to_string(&error);
            assert!(message.starts_with("pulse") || message.starts_with("positive pulse"));
            assert!(
                !messages.contains(&message),
                "{key} repeats another message"
            );
            messages.push(message);
        }
    }

    #[test]
    fn resolved_duty_and_width_come_from_the_generated_frames() {
        let rate = crate::SampleRate::DEFAULT;
        let resolved = PulseSpec {
            rate_hz: finite(4.0),
            width: crate::PulseWidth::DutyCycle {
                fraction: finite(0.25),
            },
            shape: crate::PulseShape::Rectangular {},
        }
        .resolve(rate)
        .unwrap();
        assert_eq!(resolved.width_frames(), 3_000);
        assert_eq!(resolved.width_seconds(), 0.0625);
        assert_eq!(resolved.nominal_duty(), 0.25);
    }
}
