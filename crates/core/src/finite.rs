//! Sequential finite playback with a bounded Stop fade and frozen end state.
use crate::{Observation, SampleRate, Signal, live::Engine};
use alloc::sync::Arc;

/// A finite signal with a cursor and an optional Stop fade.
///
/// Generation has three states: generating, stop-fading and ended. Once
/// ended, the cursor, end boundary and last observation stay frozen; further
/// renders produce only silence.
#[derive(Debug)]
pub struct FiniteSource {
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
    /// Begin finite generation at frame zero with the shared live ramp length.
    #[must_use]
    pub fn new(signal: Arc<Signal>) -> Self {
        let ramp_frames = Engine::ramp_frames_at(signal.sample_rate());
        Self {
            signal,
            cursor: 0,
            fade: None,
            ended: false,
            ramp_frames,
        }
    }

    /// The signal's sample rate.
    #[must_use]
    pub fn sample_rate(&self) -> SampleRate {
        self.signal.sample_rate()
    }
    /// Number of generated source frames, frozen at the end.
    #[must_use]
    pub const fn generated(&self) -> u64 {
        self.cursor
    }
    /// Whether natural completion or a Stop fade has ended generation.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.ended
    }

    /// Observe Stop at a buffer boundary. While frames remain, start a fade
    /// of at least one and at most one ramp's frames, bounded by the
    /// remaining source frames. After generation ended, nothing changes.
    pub fn begin_buffer(&mut self, stop: bool) {
        let total = self.signal.frames();
        if stop && self.fade.is_none() && !self.ended && self.cursor < total {
            self.fade = Some(StopFade {
                start: self.cursor,
                frames: self.ramp_frames.min(total - self.cursor),
            });
        }
    }

    /// Render sequential frames and silence the remainder after the frozen end.
    pub fn render(&mut self, out: &mut [[f64; 2]]) -> usize {
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

    /// Facts of the last generated frame, frozen after the end.
    #[must_use]
    pub fn observation(&self) -> Option<Observation> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Carrier, Ears, Sound, Voice};
    use alloc::{vec, vec::Vec};

    fn signal(frames: u32) -> Arc<Signal> {
        Arc::new(
            Sound {
                duration_seconds: f64::from(frames) / 8_000.0,
                fade_seconds: 0.0,
                components: (vec![Voice {
                    ears: Ears::Both,
                    peak_dbfs: -20.0,
                    carrier: Carrier::Tone {
                        frequency_hz: 100.0,
                    },
                    modulation: None,
                }])
                .into_iter()
                .enumerate()
                .map(|(index, voice)| crate::Component {
                    id: crate::ComponentId::numbered(index).unwrap(),
                    form: crate::test_support::form(voice),
                })
                .collect(),
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
