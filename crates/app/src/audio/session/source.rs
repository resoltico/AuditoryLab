//! Finite and live generators behind a stream callback.
use super::live::LiveGenerator;
pub(crate) use auditorylab_core::finite_source::FiniteSource;
use auditorylab_core::{Observation, SampleRate};

/// The generator a stream callback owns.
#[derive(Debug)]
pub(crate) enum Source {
    Finite(FiniteSource),
    Live(Box<LiveGenerator>),
}

impl Source {
    pub(crate) fn rate(&self) -> SampleRate {
        match self {
            Self::Finite(source) => source.sample_rate(),
            Self::Live(generator) => generator.sample_rate(),
        }
    }

    /// Frames generated so far: the generation cursor.
    pub(crate) fn generated(&self) -> u64 {
        match self {
            Self::Finite(source) => source.generated(),
            Self::Live(generator) => generator.frame(),
        }
    }

    pub(crate) fn is_live(&self) -> bool {
        matches!(self, Self::Live(_))
    }

    pub(crate) fn ended(&self) -> bool {
        match self {
            Self::Finite(source) => source.is_finished(),
            Self::Live(generator) => generator.is_finished(),
        }
    }

    /// Observe Stop or take a waiting target, once per valid buffer.
    pub(crate) fn begin_buffer(&mut self, stop: bool) {
        match self {
            Self::Finite(source) => source.begin_buffer(stop),
            Self::Live(generator) => generator.begin_buffer(stop),
        }
    }

    /// Render the next frames; return how many precede the end.
    pub(crate) fn render(&mut self, out: &mut [[f64; 2]]) -> usize {
        match self {
            Self::Finite(source) => source.render(out),
            Self::Live(generator) => generator.render_buffer(out),
        }
    }

    pub(crate) fn observation(&self) -> Option<Observation> {
        match self {
            Self::Finite(source) => source.observation(),
            Self::Live(generator) => generator.observation(),
        }
    }
}
