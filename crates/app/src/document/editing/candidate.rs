//! Parsed candidates and sample-rate-specific analysis.
use super::{
    Arc, Draft, EditingField, InputProblem, SampleRate, Signal, Sound, SoundError, Target,
    TargetCheck,
};

/// A complete or best-effort candidate: committed values with every staged
/// field applied that parses.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub(super) draft: Draft,
    pub(super) problems: Vec<(EditingField, InputProblem)>,
}

impl Candidate {
    /// The candidate values. When [`Candidate::is_complete`] is false, fields
    /// with problems keep their committed values.
    #[must_use]
    pub const fn draft(&self) -> &Draft {
        &self.draft
    }

    /// Fields that do not parse and numeric values that are not finite,
    /// including inherited values that were never staged.
    #[must_use]
    pub fn problems(&self) -> &[(EditingField, InputProblem)] {
        &self.problems
    }

    /// Whether every staged field parses and every numeric value is finite.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.problems.is_empty()
    }
}

/// A candidate's evaluation at one explicit sample rate. Invalid results are
/// kept as results, so an unchanged invalid candidate is not evaluated again.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub(super) rate: SampleRate,
    pub(super) check: TargetCheck,
    pub(super) envelope: Result<(), SoundError>,
    pub(super) signal: Option<Arc<Signal>>,
}

impl Analysis {
    pub(super) fn new(draft: &Draft, rate: SampleRate) -> Self {
        let check = draft.check(rate);
        let envelope = Signal::check_envelope(rate, draft.duration_seconds(), draft.fade_seconds());
        let signal = match (&envelope, check.target()) {
            (Ok(()), Some(target)) => target
                .signal(draft.duration_seconds(), draft.fade_seconds())
                .ok()
                .map(Arc::new),
            _ => None,
        };
        Self {
            rate,
            check,
            envelope,
            signal,
        }
    }

    /// The rate of this evaluation.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    /// The components' one-pass check.
    #[must_use]
    pub const fn check(&self) -> &TargetCheck {
        &self.check
    }

    /// The live target, when the components form one.
    #[must_use]
    pub fn target(&self) -> Option<&Target> {
        self.check.target()
    }

    /// The finite duration and fade check.
    ///
    /// # Errors
    /// Returns the first duration or fade error.
    pub const fn envelope(&self) -> Result<(), &SoundError> {
        match &self.envelope {
            Ok(()) => Ok(()),
            Err(error) => Err(error),
        }
    }

    /// The first finite-signal error in the established order: envelope
    /// before components.
    #[must_use]
    pub fn finite_error(&self) -> Option<&SoundError> {
        self.envelope
            .as_ref()
            .err()
            .or_else(|| self.check.result().err())
    }

    /// The validated finite signal, when both components and envelope are valid.
    #[must_use]
    pub fn signal(&self) -> Option<&Arc<Signal>> {
        self.signal.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AnalysisKey {
    pub(super) timing: crate::core::FiniteTiming,
    pub(super) sound: Sound,
    pub(super) rate: SampleRate,
}
impl AnalysisKey {
    pub(super) fn new(draft: &Draft, rate: SampleRate) -> Self {
        Self {
            sound: draft.sound(),
            timing: draft.recipe.content.timing,
            rate,
        }
    }
}
