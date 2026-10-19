//! Finite and live generators behind the device callback.
use auditorylab_core::{
    SampleRate, Signal,
    live::{Engine, Target},
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(crate) struct Updates {
    pub(crate) pending: Option<(u64, Target)>,
    pub(crate) last: Option<Target>,
    pub(crate) revision: u64,
}
pub(crate) type Mailbox = Arc<Mutex<Updates>>;

pub(crate) enum Source {
    Finite {
        signal: Arc<Signal>,
        cursor: u64,
        fade: Option<u64>,
        ramp_frames: u64,
    },
    Live(Box<crate::LiveGenerator>),
}
impl From<Signal> for Source {
    fn from(signal: Signal) -> Self {
        Self::from(Arc::new(signal))
    }
}
impl From<Arc<Signal>> for Source {
    fn from(signal: Arc<Signal>) -> Self {
        let ramp_frames = Engine::new(signal.sample_rate()).ramp_frames();
        Self::Finite {
            signal,
            cursor: 0,
            fade: None,
            ramp_frames,
        }
    }
}
impl Source {
    pub(crate) fn observation(&self) -> Option<auditorylab_core::Observation> {
        match self {
            Self::Live(generator) => generator.engine.observation(),
            Self::Finite {
                signal,
                cursor,
                fade,
                ramp_frames,
            } => {
                let frame = cursor.checked_sub(1)?;
                let mut observation = signal.observation(frame)?;
                if let Some(start) = fade {
                    let ramp = Self::fade_frames(signal, *start, *ramp_frames);
                    let gain = 1.0
                        - f64::from(u32::try_from(cursor - start).unwrap_or(u32::MAX))
                            / f64::from(u32::try_from(ramp).unwrap_or(u32::MAX));
                    for v in observation.voices.iter_mut().flatten() {
                        v.gains = v.gains.map(|g| g * gain);
                    }
                }
                Some(observation)
            }
        }
    }
    pub(crate) fn rate(&self) -> SampleRate {
        match self {
            Self::Finite { signal, .. } => signal.sample_rate(),
            Self::Live(generator) => generator.engine.sample_rate(),
        }
    }
    pub(crate) fn frame(&self) -> u64 {
        match self {
            Self::Finite { cursor, .. } => *cursor,
            Self::Live(generator) => generator.engine.frame(),
        }
    }
    pub(crate) fn frames(&self) -> Option<u64> {
        match self {
            Self::Finite { signal, .. } => Some(signal.frames()),
            Self::Live(_) => None,
        }
    }
    pub(crate) fn cancelled(&self) -> bool {
        matches!(self, Self::Finite { fade: Some(_), .. })
    }
    pub(crate) fn finished(&self) -> bool {
        match self {
            Self::Finite {
                signal,
                cursor,
                fade,
                ramp_frames,
            } => {
                *cursor == signal.frames()
                    || fade.is_some_and(|start| {
                        *cursor - start >= Self::fade_frames(signal, start, *ramp_frames)
                    })
            }
            Self::Live(generator) => generator.engine.is_finished(),
        }
    }
    fn fade_frames(signal: &Signal, start: u64, ramp_frames: u64) -> u64 {
        ramp_frames.min(signal.frames() - start)
    }
    /// Poll once per valid device buffer, before any scratch chunks are rendered.
    pub(crate) fn begin_buffer(&mut self, stop: bool) {
        match self {
            Self::Finite { cursor, fade, .. } => {
                if stop && fade.is_none() {
                    *fade = Some(*cursor);
                }
            }
            Self::Live(generator) => generator.begin_buffer(stop),
        }
    }
    pub(crate) fn render(&mut self, out: &mut [[f64; 2]]) -> usize {
        match self {
            Self::Live(generator) => generator.engine.render(out),
            Self::Finite {
                signal,
                cursor,
                fade,
                ramp_frames,
            } => {
                let count = if let Some(start) = *fade {
                    let ramp = Self::fade_frames(signal, start, *ramp_frames);
                    let remaining = ramp - (*cursor - start).min(ramp);
                    let count = usize::try_from(remaining)
                        .unwrap_or(usize::MAX)
                        .min(out.len());
                    signal.render(*cursor, &mut out[..count]);
                    for (i, frame) in out[..count].iter_mut().enumerate() {
                        let offset =
                            u32::try_from(*cursor - start + i as u64 + 1).unwrap_or(u32::MAX);
                        let gain = 1.0
                            - f64::from(offset)
                                / f64::from(u32::try_from(ramp).unwrap_or(u32::MAX));
                        for value in frame {
                            *value *= gain;
                        }
                    }
                    out[count..].fill([0.0; 2]);
                    count
                } else {
                    signal.render(*cursor, out)
                };
                *cursor += count as u64;
                count
            }
        }
    }
}
