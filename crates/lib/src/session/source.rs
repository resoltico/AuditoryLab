//! Finite and live generators behind a stream callback.
use super::live::LiveGenerator;
use auditorylab_core::{Observation, SampleRate, Signal, live::Engine};
use std::sync::Arc;

/// A finite signal with a cursor and an optional Stop fade.
///
/// Generation has three states: generating, stop-fading and ended. Once
/// ended, the cursor, end boundary and last observation stay frozen; further
/// renders produce only silence.
#[derive(Debug)]
pub(crate) struct FiniteSource {
    signal: Arc<Signal>,
    cursor: u64,
    fade: Option<StopFade>,
    ended: bool,
    ramp_frames: u64,
}

/// A Stop fade of `frames` frames starting at `start`: frame `k` of the fade
/// has gain `1 - (k + 1) / frames`, so its last frame is silent.
#[derive(Debug, Clone, Copy)]
struct StopFade {
    start: u64,
    frames: u64,
}

impl StopFade {
    fn gain(self, frame: u64) -> f64 {
        let k = frame - self.start;
        let done = f64::from(u32::try_from(k + 1).unwrap_or(u32::MAX));
        // A fade is at most one engine ramp: 23,040 frames at 768 kHz.
        1.0 - done / f64::from(u32::try_from(self.frames).unwrap_or(u32::MAX))
    }
}

impl FiniteSource {
    pub(crate) fn new(signal: Arc<Signal>) -> Self {
        let ramp_frames = Engine::new(signal.sample_rate()).ramp_frames();
        Self {
            signal,
            cursor: 0,
            fade: None,
            ended: false,
            ramp_frames,
        }
    }

    /// Observe Stop at a buffer boundary. While frames remain, start a fade
    /// of at least one and at most one ramp's frames, bounded by the
    /// remaining source frames. After generation ended, nothing changes.
    fn begin_buffer(&mut self, stop: bool) {
        let total = self.signal.frames();
        if stop && self.fade.is_none() && !self.ended && self.cursor < total {
            self.fade = Some(StopFade {
                start: self.cursor,
                frames: self.ramp_frames.min(total - self.cursor),
            });
        }
    }

    fn render(&mut self, out: &mut [[f64; 2]]) -> usize {
        if self.ended {
            out.fill([0.0; 2]);
            return 0;
        }
        let count = if let Some(fade) = self.fade {
            let remaining = fade.frames - (self.cursor - fade.start);
            let count = usize::try_from(remaining)
                .unwrap_or(usize::MAX)
                .min(out.len());
            self.signal.render(self.cursor, &mut out[..count]);
            for (offset, frame) in (self.cursor..).zip(out[..count].iter_mut()) {
                let gain = fade.gain(offset);
                for value in frame {
                    *value *= gain;
                }
            }
            out[count..].fill([0.0; 2]);
            count
        } else {
            self.signal.render(self.cursor, out)
        };
        self.cursor += count as u64;
        self.ended = self.cursor == self.signal.frames()
            || self
                .fade
                .is_some_and(|fade| self.cursor - fade.start == fade.frames);
        count
    }

    fn observation(&self) -> Option<Observation> {
        let frame = self.cursor.checked_sub(1)?;
        let mut observation = self.signal.observation(frame)?;
        if let Some(fade) = self.fade.filter(|fade| frame >= fade.start) {
            let gain = fade.gain(frame);
            for voice in observation.voices.iter_mut().flatten() {
                voice.gains = voice.gains.map(|g| g * gain);
            }
        }
        Some(observation)
    }
}

/// The generator a stream callback owns.
#[derive(Debug)]
pub(crate) enum Source {
    Finite(FiniteSource),
    Live(Box<LiveGenerator>),
}

impl Source {
    pub(crate) fn rate(&self) -> SampleRate {
        match self {
            Self::Finite(source) => source.signal.sample_rate(),
            Self::Live(generator) => generator.sample_rate(),
        }
    }

    /// Frames generated so far: the generation cursor.
    pub(crate) fn generated(&self) -> u64 {
        match self {
            Self::Finite(source) => source.cursor,
            Self::Live(generator) => generator.frame(),
        }
    }

    pub(crate) fn is_live(&self) -> bool {
        matches!(self, Self::Live(_))
    }

    pub(crate) fn ended(&self) -> bool {
        match self {
            Self::Finite(source) => source.ended,
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

#[cfg(test)]
mod tests {
    use super::*;
    use auditorylab_core::{Carrier, Component, Ears, Stimulus};

    fn signal(frames: u32) -> Arc<Signal> {
        Arc::new(
            Stimulus {
                duration_seconds: f64::from(frames) / 8_000.0,
                fade_seconds: 0.0,
                components: vec![Component {
                    ears: Ears::Both,
                    peak_dbfs: -20.0,
                    carrier: Carrier::Tone {
                        frequency_hz: 100.0,
                    },
                    modulation: None,
                }],
            }
            .signal(SampleRate::new(8_000).unwrap())
            .unwrap(),
        )
    }

    #[test]
    fn a_stop_fade_is_positive_capped_and_ends_at_zero_gain() {
        // 8 kHz ramp: 240 frames. Remaining 1, 100 and 1000 frames.
        for (total, at, expected_fade) in [
            (1_000_u32, 999_u32, 1_u32),
            (1_000, 900, 100),
            (2_000, 1_000, 240),
        ] {
            let reference = signal(total);
            let mut source = FiniteSource::new(Arc::clone(&reference));
            source.render(&mut vec![[0.0; 2]; usize::try_from(at).unwrap()]);
            source.begin_buffer(true);
            let mut out = vec![[9.0; 2]; 400];
            let count = source.render(&mut out);
            assert_eq!(count, usize::try_from(expected_fade).unwrap());
            assert!(source.ended);
            for (k, frame) in (0_u32..).zip(&out[..count]) {
                let n = u64::from(at + k);
                // Independent oracle from the stated fade contract.
                let gain = 1.0 - f64::from(k + 1) / f64::from(expected_fade);
                assert_eq!(*frame, reference.frame(n).map(|v| v * gain), "frame {k}");
                assert!(frame.iter().all(|v| v.is_finite()));
            }
            assert_eq!(out[count - 1], [0.0; 2], "the last fade frame is silent");
            assert!(out[count..].iter().all(|f| *f == [0.0; 2]));
        }
    }

    #[test]
    fn stop_after_generation_ended_creates_no_fade_and_freezes_the_observation() {
        let mut source = FiniteSource::new(signal(100));
        source.render(&mut [[0.0; 2]; 128]);
        assert!(source.ended);
        let observation = source.observation();
        assert_eq!(observation.unwrap().frame, 99);
        source.begin_buffer(true);
        assert!(source.fade.is_none(), "no zero-length fade after the end");
        let mut out = [[9.0; 2]; 64];
        assert_eq!(source.render(&mut out), 0);
        assert_eq!(out, [[0.0; 2]; 64]);
        assert_eq!(source.cursor, 100);
        assert_eq!(source.observation(), observation);
        let gains: Vec<_> = observation
            .unwrap()
            .voices
            .iter()
            .flatten()
            .map(|v| v.gains)
            .collect();
        assert!(gains.iter().flatten().all(|g| g.is_finite()));
    }
}
