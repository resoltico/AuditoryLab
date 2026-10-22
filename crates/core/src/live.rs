//! Live rendering for interactive editing.
//!
//! An [`Engine`] renders an open-ended stereo stream and moves between
//! validated [`Target`]s without discontinuities. Each change takes one ramp
//! of [`Engine::RAMP_SECONDS`]:
//!
//! - levels, ear routes and sinusoidal depth move linearly;
//! - tone and sinusoidal-modulation frequencies glide with continuous phase;
//! - a component whose carrier or modulation kind changes, or whose pulse
//!   rate or width changes, crossfades from the old generator to the new one.
//!
//! Every ramp runs from one validated state to another with one shared
//! progress, so each ear's summed level stays within full scale throughout.
//! A target that arrives during a ramp waits for it to finish; only the
//! newest waiting target is kept. A target that generates the same audio as
//! the engine's current destination changes only its facts and revision: it
//! starts no ramp, resets no phase and discards an older waiting target.
//! Rendering allocates nothing, takes no lock and performs no I/O.
use crate::synthesis::{self, Carrier, Modulator, Voice};
use crate::{ComponentId, Observation, SampleRate, Signal, SlotRole, Target};

const SLOTS: usize = Observation::MAX_VOICES;

#[derive(Debug, Clone, Copy)]
struct Slot {
    id: ComponentId,
    facts: crate::ComponentFacts,
    role: SlotRole,
    voice: Voice,
    gain: (f64, f64, f64, f64),
    carrier_step: (u64, u64),
    modulation_step: (u64, u64),
    depth: (f64, f64),
    carrier_phase: u64,
    modulation_phase: u64,
    origin: u64,
}

fn steps(voice: &Voice) -> (u64, u64, f64) {
    let carrier = match voice.carrier {
        Carrier::Tone { step } => step,
        Carrier::Pulse(_) => 0,
    };
    let (modulation, depth) = match voice.modulation {
        Some(Modulator::Sinusoidal { step, depth }) => (step, depth),
        _ => (0, 0.0),
    };
    (carrier, modulation, depth)
}

/// Whether `next` can continue `current`'s generator: same kinds, same pulses.
fn continues(current: &Voice, next: &Voice) -> bool {
    let carrier = match (current.carrier, next.carrier) {
        (Carrier::Tone { .. }, Carrier::Tone { .. }) => true,
        (Carrier::Pulse(a), Carrier::Pulse(b)) => a == b,
        _ => false,
    };
    let modulation = match (current.modulation, next.modulation) {
        (None, None) | (Some(Modulator::Sinusoidal { .. }), Some(Modulator::Sinusoidal { .. })) => {
            true
        }
        (Some(Modulator::Rectangular(a)), Some(Modulator::Rectangular(b))) => a == b,
        _ => false,
    };
    carrier && modulation
}

#[inline]
fn between(from: f64, to: f64, progress: f64) -> f64 {
    from * (1.0 - progress) + to * progress
}

#[inline]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "Validated nonnegative phase steps below 2^63 interpolate within their endpoint range; rounding to binary64 and truncating back is the specified glide"
)]
fn glide(from: u64, to: u64, progress: f64) -> u64 {
    if from == to || progress >= 1.0 {
        to
    } else {
        between(from as f64, to as f64, progress) as u64
    }
}

impl Slot {
    fn level(&self, ear: usize, progress: Option<f64>) -> f64 {
        let (from_left, from_right, to_left, to_right) = self.gain;
        let (from, to) = if ear == 0 {
            (from_left, to_left)
        } else {
            (from_right, to_right)
        };
        progress.map_or(to, |p| between(from, to, p))
    }

    fn depth_at(&self, p: f64) -> f64 {
        if p >= 1.0 {
            self.depth.1
        } else {
            between(self.depth.0, self.depth.1, p)
        }
    }

    #[inline]
    fn sample(&mut self, frame: u64, progress: Option<f64>) -> f64 {
        let p = progress.unwrap_or(1.0);
        let carrier = match self.voice.carrier {
            Carrier::Tone { .. } => {
                let value = synthesis::sine(self.carrier_phase);
                let step = glide(self.carrier_step.0, self.carrier_step.1, p);
                self.carrier_phase = self.carrier_phase.wrapping_add(step);
                value
            }
            Carrier::Pulse(pulse) => pulse.sample(frame - self.origin),
        };
        let gain = match self.voice.modulation {
            None => 1.0,
            Some(Modulator::Sinusoidal { .. }) => {
                let depth = self.depth_at(p);
                let value = synthesis::sine(self.modulation_phase);
                let step = glide(self.modulation_step.0, self.modulation_step.1, p);
                self.modulation_phase = self.modulation_phase.wrapping_add(step);
                (1.0 + depth * value) / (1.0 + depth)
            }
            Some(Modulator::Rectangular(pulse)) => pulse.sample(frame - self.origin),
        };
        carrier * gain
    }
}

/// An open-ended live renderer at one sample rate.
#[derive(Debug, Clone)]
pub struct Engine {
    rate: SampleRate,
    ramp_frames: u64,
    frame: u64,
    slots: [Option<Slot>; SLOTS],
    ramp: Option<u64>,
    /// The target the current or last ramp moved to, with its revision.
    destination: Option<(Target, u64)>,
    /// The newest target waiting for the current ramp to end.
    pending: Option<(Target, u64)>,
    requested_revision: u64,
    active_revision: u64,
    settled_revision: u64,
    last: Option<crate::Observation>,
    stopping: bool,
    finished: bool,
}

impl Engine {
    /// How long every change takes, in seconds.
    pub const RAMP_SECONDS: f64 = 0.03;

    /// A silent engine; the first target fades in.
    #[must_use]
    pub fn new(rate: SampleRate) -> Self {
        let ramp_frames = rate.frames_in(Self::RAMP_SECONDS).unwrap_or(1).max(1);
        Self {
            rate,
            ramp_frames,
            frame: 0,
            slots: [None; SLOTS],
            ramp: None,
            destination: None,
            pending: None,
            requested_revision: 0,
            active_revision: 0,
            settled_revision: 0,
            last: None,
            stopping: false,
            finished: false,
        }
    }

    /// The engine's sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    /// Frames rendered so far.
    #[must_use]
    pub const fn frame(&self) -> u64 {
        self.frame
    }

    /// Frames in one ramp at this rate.
    #[must_use]
    pub const fn ramp_frames(&self) -> u64 {
        self.ramp_frames
    }

    /// Move to `target` with the next ramp. Returns `false`, changing nothing,
    /// for a target validated at another rate or after [`Engine::stop`].
    pub fn set(&mut self, target: &Target) -> bool {
        self.set_revision(target, self.requested_revision.saturating_add(1))
    }

    /// Submit a target under a strictly increasing caller revision; invalid
    /// revisions, another rate and targets after [`Engine::stop`] leave the
    /// state unchanged and return `false`.
    ///
    /// A target that generates the same audio as the current destination is
    /// a facts-only revision: it discards any waiting target and takes effect
    /// at once without a ramp, keeping phases and an active ramp's progress.
    /// Outgoing generators keep the facts of the target they belong to, and
    /// observations of earlier frames are not rewritten. Any other target
    /// waits for the current ramp to end, replacing an older waiting target.
    pub fn set_revision(&mut self, target: &Target, revision: u64) -> bool {
        if self.stopping || target.sample_rate() != self.rate || revision <= self.requested_revision
        {
            return false;
        }
        self.requested_revision = revision;
        match &mut self.destination {
            Some((destination, destination_revision)) if destination.same_audio(target) => {
                *destination = *target;
                *destination_revision = revision;
                self.pending = None;
                self.active_revision = revision;
                if self.ramp.is_none() {
                    self.settled_revision = revision;
                }
                for slot in self
                    .slots
                    .iter_mut()
                    .flatten()
                    .filter(|slot| slot.role != SlotRole::Outgoing)
                {
                    if let Some((_, facts)) = target.components().find(|(id, _)| *id == slot.id) {
                        slot.facts = *facts;
                    }
                }
            }
            _ => self.pending = Some((*target, revision)),
        }
        true
    }

    /// Last generated frame, or None before any nonempty render. Does not advance phase.
    #[must_use]
    pub const fn observation(&self) -> Option<Observation> {
        self.last
    }

    /// Fade to silence with the next ramp, then finish.
    pub fn stop(&mut self) {
        if !self.stopping {
            self.stopping = true;
            self.pending = Some((Target::silence(self.rate), self.active_revision));
        }
    }

    /// Whether the engine has faded out after [`Engine::stop`], or reached
    /// [`Signal::MAX_FRAMES`].
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Fill `out` with the next frames and return how many precede the end
    /// of the stream; frames after the end are silence.
    #[expect(
        clippy::cast_precision_loss,
        reason = "The 30 ms ramp is at most 23040 frames at the supported maximum sample rate; its counts fit binary64 exactly"
    )]
    pub fn render(&mut self, out: &mut [[f64; 2]]) -> usize {
        for index in 0..out.len() {
            if self.finished {
                out[index..].fill([0.0; 2]);
                return index;
            }
            if self.ramp.is_none()
                && let Some((target, revision)) = self.pending.take()
            {
                self.active_revision = revision;
                self.destination = Some((target, revision));
                self.begin(&target);
            }
            let progress = self
                .ramp
                .map(|done| (done + 1) as f64 / self.ramp_frames as f64);
            let mut frame = [0.0; 2];
            for slot in self.slots.iter_mut().flatten() {
                let value = slot.sample(self.frame, progress);
                for (ear, sample) in frame.iter_mut().enumerate() {
                    let level = slot.level(ear, progress);
                    if level != 0.0 {
                        *sample += level * value;
                    }
                }
            }
            // Ramps between validated states keep each ear within full scale;
            // the clamp removes only rounding-level excess.
            out[index] = frame.map(|sample| sample.clamp(-1.0, 1.0));
            if index + 1 == out.len()
                || progress == Some(1.0)
                || self.frame + 1 >= Signal::MAX_FRAMES
            {
                self.last = Some(self.snapshot(progress));
            }
            self.frame += 1;
            if let Some(done) = self.ramp {
                if done + 1 == self.ramp_frames {
                    self.end_ramp();
                } else {
                    self.ramp = Some(done + 1);
                }
            }
            if self.frame >= Signal::MAX_FRAMES {
                self.finished = true;
            }
        }
        out.len()
    }

    fn begin(&mut self, target: &Target) {
        let mut continued = [false; SLOTS];
        for slot in self.slots.iter_mut().flatten() {
            let (_, _, left, right) = slot.gain;
            slot.gain = (left, right, 0.0, 0.0);
            slot.role = SlotRole::Outgoing;
        }
        for entry in target.mix().entries() {
            let (id, voice, facts) = (entry.id, &entry.voice, &entry.facts);
            let found = (0..SLOTS).find(|&k| {
                !continued[k]
                    && self.slots[k]
                        .as_ref()
                        .is_some_and(|slot| slot.id == id && continues(&slot.voice, voice))
            });
            let [left, right] = voice.gains();
            let (carrier, modulation, depth) = steps(voice);
            if let Some(k) = found {
                continued[k] = true;
                if let Some(slot) = self.slots[k].as_mut() {
                    slot.gain = (slot.gain.0, slot.gain.1, left, right);
                    slot.carrier_step = (slot.carrier_step.1, carrier);
                    slot.modulation_step = (slot.modulation_step.1, modulation);
                    slot.depth = (slot.depth.1, depth);
                    slot.voice = *voice;
                    slot.facts = *facts;
                    slot.role = SlotRole::Continuing;
                }
            } else if let Some(k) = (0..SLOTS).find(|&k| self.slots[k].is_none()) {
                continued[k] = true;
                self.slots[k] = Some(Slot {
                    id,
                    facts: *facts,
                    role: SlotRole::Incoming,
                    voice: *voice,
                    gain: (0.0, 0.0, left, right),
                    carrier_step: (carrier, carrier),
                    modulation_step: (modulation, modulation),
                    depth: (depth, depth),
                    carrier_phase: 0,
                    modulation_phase: 0,
                    origin: self.frame,
                });
            }
        }
        self.ramp = Some(0);
    }

    fn end_ramp(&mut self) {
        for entry in &mut self.slots {
            if let Some(slot) = entry {
                let (_, _, left, right) = slot.gain;
                if left == 0.0 && right == 0.0 {
                    *entry = None;
                } else {
                    slot.gain = (left, right, left, right);
                    slot.role = SlotRole::Continuing;
                    slot.carrier_step.0 = slot.carrier_step.1;
                    slot.modulation_step.0 = slot.modulation_step.1;
                    slot.depth.0 = slot.depth.1;
                }
            }
        }
        self.ramp = None;
        self.settled_revision = self.active_revision;
        if self.stopping && self.pending.is_none() && self.slots.iter().all(Option::is_none) {
            self.finished = true;
        }
    }

    fn snapshot(&self, progress: Option<f64>) -> Observation {
        let p = progress.unwrap_or(1.0);
        let mut voices = [None; SLOTS];
        for (index, slot) in self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|s| (i, s)))
        {
            let mut facts = slot.facts;
            if let crate::CarrierFacts::Tone(ref mut rate) = facts.carrier {
                *rate = crate::PhaseRate::from_step(
                    rate.requested_hz,
                    glide(slot.carrier_step.0, slot.carrier_step.1, p),
                    self.rate,
                );
            }
            if let Some(crate::ModulationFacts::Sinusoidal {
                ref mut rate,
                ref mut depth,
            }) = facts.modulation
            {
                *rate = crate::PhaseRate::from_step(
                    rate.requested_hz,
                    glide(slot.modulation_step.0, slot.modulation_step.1, p),
                    self.rate,
                );
                *depth = slot.depth_at(p);
            }
            voices[index] = Some(crate::VoiceObservation {
                id: slot.id,
                role: slot.role,
                facts,
                gains: [slot.level(0, progress), slot.level(1, progress)],
            });
        }
        Observation {
            frame: self.frame,
            active_revision: self.active_revision,
            settled_revision: if p >= 1.0 {
                self.active_revision
            } else {
                self.settled_revision
            },
            transitioning: p < 1.0,
            voices,
        }
    }

    #[cfg(test)]
    #[expect(
        clippy::cast_precision_loss,
        reason = "The ramp is at most 23040 frames; conversion to binary64 is exact in this test oracle"
    )]
    fn level_sums(&self) -> [f64; 2] {
        let progress = self
            .ramp
            .map(|done| (done + 1) as f64 / self.ramp_frames as f64);
        let mut sums = [0.0; 2];
        for slot in self.slots.iter().flatten() {
            for (ear, sum) in sums.iter_mut().enumerate() {
                *sum += slot.level(ear, progress);
            }
        }
        sums
    }

    #[cfg(test)]
    fn live_slots(&self) -> usize {
        self.slots.iter().flatten().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Carrier as Source, Component, Ear, Ears, Modulation, Recipe, Stimulus, StimulusError,
    };
    use alloc::vec;
    use alloc::vec::Vec;
    use proptest::{prelude::*, test_runner::Config, test_runner::TestRunner};

    fn rate() -> SampleRate {
        SampleRate::new(48_000).unwrap()
    }

    fn tone(hz: f64, peak_dbfs: f64, ears: Ears) -> Component {
        Component {
            ears,
            peak_dbfs,
            carrier: Source::Tone { frequency_hz: hz },
            modulation: None,
        }
    }

    fn clicks(hz: f64, peak_dbfs: f64) -> Component {
        Component {
            ears: Ears::Both,
            peak_dbfs,
            carrier: Source::Click {
                rate_hz: hz,
                width_seconds: 0.001,
            },
            modulation: None,
        }
    }

    fn id(n: u32) -> ComponentId {
        ComponentId::new(n)
    }

    fn numbered(components: &[Component]) -> Vec<(ComponentId, Component)> {
        components
            .iter()
            .enumerate()
            .map(|(i, c)| (id(u32::try_from(i).unwrap()), *c))
            .collect()
    }

    fn render(engine: &mut Engine, frames: usize) -> Vec<[f64; 2]> {
        let mut out = vec![[9.0; 2]; frames];
        let mut at = 0;
        for size in [1, 63, 512, 7, 1024].into_iter().cycle() {
            if at >= frames {
                break;
            }
            let end = frames.min(at + size);
            engine.render(&mut out[at..end]);
            at = end;
        }
        out
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; binary64 conversion is exact for these bounded fixtures"
    )]
    fn an_unedited_session_equals_the_signal_after_its_start_ramp() {
        let mut stimuli: Vec<Stimulus> = crate::catalog::ENTRIES
            .iter()
            .map(|entry| Recipe::parse(entry.text).unwrap().stimulus)
            .collect();
        stimuli.push(Stimulus {
            duration_seconds: 1.0,
            fade_seconds: 0.0,
            components: vec![
                tone(250.0, -20.0, Ears::Left),
                tone(256.0, -20.0, Ears::Right),
                clicks(40.0, -20.0),
            ],
        });
        for mut stimulus in stimuli {
            stimulus.duration_seconds = 1.0;
            stimulus.fade_seconds = 0.0;
            let signal = stimulus.signal(rate()).unwrap();
            let target = Target::new(rate(), &numbered(&stimulus.components)).unwrap();
            let mut engine = Engine::new(rate());
            assert!(engine.set(&target));
            let frames = render(&mut engine, 48_000);
            for (n, frame) in frames
                .iter()
                .enumerate()
                .skip(engine.ramp_frames() as usize)
            {
                let expected = signal.frame(n as u64);
                assert_eq!(
                    frame.map(f64::to_bits),
                    expected.map(f64::to_bits),
                    "frame {n}"
                );
            }
            assert_eq!(target.peak_dbfs(), signal.peak_dbfs());
        }
    }

    #[test]
    fn targets_reject_what_signals_reject_with_the_same_errors() {
        let too_high = tone(30_000.0, -20.0, Ears::Both);
        let stimulus = Stimulus {
            duration_seconds: 1.0,
            fade_seconds: 0.0,
            components: vec![tone(440.0, -20.0, Ears::Both), too_high],
        };
        assert_eq!(
            Target::new(rate(), &numbered(&stimulus.components)).unwrap_err(),
            stimulus.signal(rate()).unwrap_err()
        );
        let loud = [tone(440.0, -3.0, Ears::Left), tone(441.0, -3.0, Ears::Left)];
        assert!(matches!(
            Target::new(rate(), &numbered(&loud)),
            Err(StimulusError::Peak { ear: Ear::Left, .. })
        ));
        let many = vec![tone(440.0, -40.0, Ears::Both); 17];
        assert!(matches!(
            Target::new(rate(), &numbered(&many)),
            Err(StimulusError::TooManyComponents { count: 17 })
        ));
        let other = SampleRate::new(44_100).unwrap();
        let mut engine = Engine::new(rate());
        assert!(!engine.set(&Target::silence(other)));
    }

    #[test]
    #[expect(
        clippy::cast_precision_loss,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; binary64 conversion is exact for these bounded fixtures"
    )]
    fn a_retuned_tone_keeps_its_phase_and_a_reset_would_not() {
        let mut engine = Engine::new(rate());
        engine.set(&Target::new(rate(), &[(id(7), tone(440.0, -6.0, Ears::Both))]).unwrap());
        // 4,827 frames end near a crest of 440 Hz, where a phase reset would jump.
        let mut frames = render(&mut engine, 4_827);
        engine.set(&Target::new(rate(), &[(id(7), tone(880.0, -6.0, Ears::Both))]).unwrap());
        frames.extend(render(&mut engine, 9_600));
        let amplitude = libm::pow(10.0, -6.0 / 20.0);
        // A sine changes by at most 2π f / fs per frame at its amplitude.
        let bound = amplitude * core::f64::consts::TAU * 880.0 / 48_000.0 * 1.000_001;
        let largest = frames
            .windows(2)
            .map(|pair| (pair[1][0] - pair[0][0]).abs())
            .fold(0.0, f64::max);
        assert!(largest <= bound, "{largest} > {bound}");
        // Negative control: restarting the phase at the edit jumps far further.
        let reset: Vec<f64> = (0..4_827_u64)
            .map(|n| amplitude * libm::sin(core::f64::consts::TAU * 440.0 * n as f64 / 48_000.0))
            .chain((0..100).map(|n| {
                amplitude * libm::sin(core::f64::consts::TAU * 880.0 * n as f64 / 48_000.0)
            }))
            .collect();
        let jump = reset
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0, f64::max);
        assert!(jump > bound, "{jump} <= {bound}");
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; binary64 conversion is exact for these bounded fixtures"
    )]
    fn a_level_change_moves_linearly_over_one_ramp() {
        // A pulse wider than the session holds the carrier at one.
        let held = Component {
            ears: Ears::Both,
            peak_dbfs: -20.0,
            carrier: Source::Click {
                rate_hz: 0.1,
                width_seconds: 5.0,
            },
            modulation: None,
        };
        let mut engine = Engine::new(rate());
        engine.set(&Target::new(rate(), &[(id(1), held)]).unwrap());
        render(&mut engine, 2_000);
        let louder = Component {
            peak_dbfs: -6.0,
            ..held
        };
        engine.set(&Target::new(rate(), &[(id(1), louder)]).unwrap());
        let ramp = engine.ramp_frames() as usize;
        let frames = render(&mut engine, ramp + 10);
        let from = libm::pow(10.0, -20.0 / 20.0);
        let to = libm::pow(10.0, -6.0 / 20.0);
        for (t, frame) in frames.iter().take(ramp).enumerate() {
            let p = (t + 1) as f64 / ramp as f64;
            assert!((frame[0] - (from * (1.0 - p) + to * p)).abs() < 1e-15);
        }
        assert!(frames[ramp..].iter().all(|f| f[0] == to && f[1] == to));
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; binary64 conversion is exact for these bounded fixtures"
    )]
    fn a_changed_pulse_crossfades_and_a_retuned_tone_does_not() {
        let mut engine = Engine::new(rate());
        engine.set(&Target::new(rate(), &[(id(1), clicks(40.0, -20.0))]).unwrap());
        render(&mut engine, 2_000);
        engine.set(&Target::new(rate(), &[(id(1), clicks(80.0, -20.0))]).unwrap());
        let ramp = engine.ramp_frames() as usize;
        render(&mut engine, 1);
        assert_eq!(engine.live_slots(), 2);
        render(&mut engine, ramp);
        assert_eq!(engine.live_slots(), 1);
        engine.set(&Target::new(rate(), &[(id(1), tone(300.0, -20.0, Ears::Left))]).unwrap());
        render(&mut engine, 1);
        assert_eq!(engine.live_slots(), 2);
        render(&mut engine, ramp);
        engine.set(&Target::new(rate(), &[(id(1), tone(600.0, -10.0, Ears::Right))]).unwrap());
        render(&mut engine, 1);
        assert_eq!(engine.live_slots(), 1);
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; binary64 conversion is exact for these bounded fixtures"
    )]
    fn targets_during_a_ramp_wait_and_only_the_newest_applies() {
        let mut engine = Engine::new(rate());
        let quiet = Target::new(rate(), &[(id(1), tone(440.0, -40.0, Ears::Both))]).unwrap();
        let skipped = Target::new(rate(), &[(id(2), tone(500.0, -10.0, Ears::Both))]).unwrap();
        let newest = Target::new(rate(), &[(id(3), tone(600.0, -20.0, Ears::Left))]).unwrap();
        engine.set(&quiet);
        render(&mut engine, 10);
        engine.set(&skipped);
        engine.set(&newest);
        let remaining = engine.ramp_frames() as usize - 10;
        render(&mut engine, remaining);
        assert_eq!(engine.live_slots(), 1);
        render(&mut engine, 1);
        assert!(engine.slots.iter().flatten().all(|s| s.id != id(2)));
        assert!(engine.slots.iter().flatten().any(|s| s.id == id(3)));
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; binary64 conversion is exact for these bounded fixtures"
    )]
    fn stopping_fades_out_within_one_ramp_and_then_finishes() {
        let mut engine = Engine::new(rate());
        engine.set(&Target::new(rate(), &[(id(1), tone(440.0, -6.0, Ears::Both))]).unwrap());
        render(&mut engine, 4_800);
        assert!(!engine.is_finished());
        engine.stop();
        assert!(!engine.set(&Target::silence(rate())));
        let ramp = engine.ramp_frames() as usize;
        let mut out = vec![[1.0; 2]; ramp + 100];
        assert_eq!(engine.render(&mut out), ramp);
        assert!(engine.is_finished());
        assert!(out[ramp - 1].iter().all(|s| s.abs() < 1e-12));
        assert!(out[ramp..].iter().all(|f| *f == [0.0; 2]));
        assert_eq!(engine.render(&mut out), 0);
    }

    #[test]
    fn sixteen_components_changing_kind_use_every_slot_within_full_scale() {
        let tones: Vec<Component> = (0..16)
            .map(|i| tone(200.0 + 50.0 * f64::from(i), -24.1, Ears::Both))
            .collect();
        let pulses: Vec<Component> = (0..16).map(|_| clicks(40.0, -24.1)).collect();
        let mut engine = Engine::new(rate());
        engine.set(&Target::new(rate(), &numbered(&tones)).unwrap());
        render(&mut engine, 2_000);
        engine.set(&Target::new(rate(), &numbered(&pulses)).unwrap());
        render(&mut engine, 1);
        assert_eq!(engine.live_slots(), 32);
        for frame in render(&mut engine, 2_000) {
            assert!(frame.iter().all(|s| s.abs() <= 1.0));
        }
        assert_eq!(engine.live_slots(), 16);
    }

    #[test]
    #[expect(
        clippy::cast_precision_loss,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; binary64 conversion is exact for these bounded fixtures"
    )]
    fn random_edits_keep_every_ear_within_full_scale() {
        let component = (0_u8..3, 0_u8..4, 1.0_f64..20_000.0, 0.0_f64..1.0, 0_u32..4).prop_map(
            |(ears, kind, hz, level, id)| {
                let ears = [Ears::Left, Ears::Right, Ears::Both][usize::from(ears)];
                let carrier = if kind == 3 {
                    Source::Click {
                        rate_hz: hz.min(1_000.0),
                        width_seconds: 0.0005,
                    }
                } else {
                    Source::Tone { frequency_hz: hz }
                };
                let modulation = match kind {
                    1 => Some(Modulation::Sinusoidal {
                        rate_hz: 40.0,
                        depth: level,
                    }),
                    2 => Some(Modulation::Rectangular {
                        rate_hz: 40.0,
                        width_seconds: 0.001,
                    }),
                    _ => None,
                };
                (id, ears, carrier, modulation, level)
            },
        );
        let strategy = prop::collection::vec(
            (prop::collection::vec(component, 0..=6), 1_usize..3_000),
            1..8,
        );
        let config = Config {
            cases: 64,
            failure_persistence: None,
            max_shrink_iters: 256,
            ..Config::default()
        };
        TestRunner::new(config)
            .run(&strategy, |edits| {
                let mut engine = Engine::new(rate());
                for (parts, frames) in edits {
                    // Scale levels so that each ear's sum is exactly at most one.
                    let n = parts.len().max(1) as f64;
                    let components: Vec<(ComponentId, Component)> = parts
                        .iter()
                        .enumerate()
                        .map(|(i, (id, ears, carrier, modulation, level))| {
                            let amplitude = (0.05 + 0.95 * level) / n;
                            (
                                ComponentId::new(id * 16 + u32::try_from(i).unwrap()),
                                Component {
                                    ears: *ears,
                                    peak_dbfs: 20.0 * libm::log10(amplitude),
                                    carrier: *carrier,
                                    modulation: *modulation,
                                },
                            )
                        })
                        .collect();
                    if let Ok(target) = Target::new(rate(), &components) {
                        engine.set(&target);
                    }
                    for _ in 0..frames {
                        let mut out = [[0.0; 2]];
                        engine.render(&mut out);
                        let sums = engine.level_sums();
                        prop_assert!(sums[0] <= 1.0 + 1e-12 && sums[1] <= 1.0 + 1e-12);
                        prop_assert!(out[0].iter().all(|s| s.is_finite() && s.abs() <= 1.0));
                    }
                }
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn rendering_and_editing_allocate_nothing() {
        let a = Target::new(
            rate(),
            &numbered(&[tone(440.0, -20.0, Ears::Left), clicks(40.0, -20.0)]),
        )
        .unwrap();
        let b = Target::new(
            rate(),
            &numbered(&[clicks(80.0, -20.0), tone(450.0, -26.0, Ears::Right)]),
        )
        .unwrap();
        let mut engine = Engine::new(rate());
        let mut out = [[0.0; 2]; 1_024];
        let count = allocation_counter::measure(|| {
            engine.set(&a);
            for _ in 0..10 {
                engine.render(&mut out);
            }
            engine.set(&b);
            for _ in 0..10 {
                engine.render(&mut out);
            }
            engine.stop();
            for _ in 0..10 {
                engine.render(&mut out);
            }
        });
        assert_eq!((count.count_total, count.bytes_total), (0, 0));
        assert!(engine.is_finished());
    }

    fn gated(hz: f64, peak_dbfs: f64, width_seconds: f64) -> Component {
        Component {
            modulation: Some(Modulation::Rectangular {
                rate_hz: 40.0,
                width_seconds,
            }),
            ..tone(hz, peak_dbfs, Ears::Both)
        }
    }

    #[test]
    fn unchanged_pulses_and_gates_keep_their_slot_and_changed_gates_crossfade() {
        for (before, after, slots) in [
            (clicks(40.0, -20.0), clicks(40.0, -10.0), 1),
            (gated(500.0, -20.0, 0.001), gated(500.0, -10.0, 0.001), 1),
            (gated(500.0, -20.0, 0.001), gated(700.0, -20.0, 0.001), 1),
            (gated(500.0, -20.0, 0.001), gated(500.0, -20.0, 0.002), 2),
        ] {
            let mut engine = Engine::new(rate());
            engine.set(&Target::new(rate(), &[(id(1), before)]).unwrap());
            render(&mut engine, 2_000);
            engine.set(&Target::new(rate(), &[(id(1), after)]).unwrap());
            render(&mut engine, 1);
            assert_eq!(engine.live_slots(), slots, "{before:?} -> {after:?}");
        }
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; binary64 conversion is exact for these bounded fixtures"
    )]
    fn frequencies_glide_through_intermediate_steps_during_a_ramp() {
        let am = |hz: f64, rate_hz: f64, depth: f64| Component {
            modulation: Some(Modulation::Sinusoidal { rate_hz, depth }),
            ..tone(hz, -20.0, Ears::Both)
        };
        let mut engine = Engine::new(rate());
        engine.set(&Target::new(rate(), &[(id(1), am(440.0, 10.0, 0.0))]).unwrap());
        render(&mut engine, 2_000);
        engine.set(&Target::new(rate(), &[(id(1), am(880.0, 40.0, 1.0))]).unwrap());
        let ramp = engine.ramp_frames() as usize;
        render(&mut engine, ramp / 2);
        let slot = engine.slots.iter().flatten().next().copied().unwrap();
        render(&mut engine, 1);
        let next = engine.slots.iter().flatten().next().copied().unwrap();
        let carrier = next.carrier_phase.wrapping_sub(slot.carrier_phase);
        let modulation = next.modulation_phase.wrapping_sub(slot.modulation_phase);
        assert!(slot.carrier_step.0 < carrier && carrier < slot.carrier_step.1);
        assert!(slot.modulation_step.0 < modulation && modulation < slot.modulation_step.1);
        // Depth is halfway too: recompute the frame from the slot's own phases.
        let p = (ramp / 2 + 1) as f64 / ramp as f64;
        let depth = slot.depth.0 * (1.0 - p) + slot.depth.1 * p;
        assert!(0.4 < depth && depth < 0.6);
        let mut copy = slot;
        let value = copy.sample(engine.frame() - 1, Some(p));
        let expected = synthesis::sine(slot.carrier_phase)
            * ((1.0 + depth * synthesis::sine(slot.modulation_phase)) / (1.0 + depth));
        assert_eq!(value.to_bits(), expected.to_bits());
        render(&mut engine, ramp);
        let settled = engine.slots.iter().flatten().next().copied().unwrap();
        assert_eq!(
            settled.carrier_step,
            (slot.carrier_step.1, slot.carrier_step.1)
        );
        assert_eq!(settled.depth, (1.0, 1.0));
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; binary64 conversion is exact for these bounded fixtures"
    )]
    fn pulses_started_mid_session_count_from_their_own_first_frame() {
        let reference = |n: u64| (n % 1_200) < 48;
        let mut engine = Engine::new(rate());
        render(&mut engine, 1_000);
        assert_eq!(engine.frame(), 1_000);
        engine.set(&Target::new(rate(), &[(id(1), clicks(40.0, -6.0))]).unwrap());
        let frames = render(&mut engine, 6_000);
        assert_eq!(engine.frame(), 7_000);
        let level = libm::pow(10.0, -6.0 / 20.0);
        for (t, frame) in frames
            .iter()
            .enumerate()
            .skip(engine.ramp_frames() as usize)
        {
            let expected = if reference(t as u64) { level } else { 0.0 };
            assert_eq!(frame[0], expected, "frame {t} after the target");
        }
        let mut engine = Engine::new(rate());
        render(&mut engine, 777);
        engine.set(&Target::new(rate(), &[(id(1), gated(1_000.0, -6.0, 0.001))]).unwrap());
        let frames = render(&mut engine, 6_000);
        for (t, frame) in frames
            .iter()
            .enumerate()
            .skip(engine.ramp_frames() as usize)
        {
            if !reference(t as u64) {
                assert_eq!(frame[0], 0.0, "closed gate at frame {t} after the target");
            }
        }
        assert!(frames.iter().skip(2_400).take(48).any(|f| f[0] != 0.0));
    }

    #[test]
    fn stop_waits_for_the_active_ramp_then_fades_for_one_ramp() {
        let mut engine = Engine::new(rate());
        engine.set(&Target::new(rate(), &[(id(1), tone(440.0, -30.0, Ears::Both))]).unwrap());
        render(&mut engine, 1);
        engine.stop();
        let expected = 2 * engine.ramp_frames() - 1;
        let mut out = vec![[1.0; 2]; usize::try_from(expected + 1).unwrap()];
        assert_eq!(engine.render(&mut out), usize::try_from(expected).unwrap());
        assert!(engine.is_finished());
        let observation = engine.observation().unwrap();
        assert_eq!(observation.frame, expected);
        assert!(!observation.transitioning);
        assert!(
            observation
                .voices
                .iter()
                .flatten()
                .all(|voice| voice.gains == [0.0; 2])
        );
        assert!(
            out[usize::try_from(expected - 1).unwrap()..]
                .iter()
                .all(|frame| *frame == [0.0; 2])
        );
    }

    #[test]
    fn the_frame_limit_ends_and_zeros_the_remainder() {
        let mut engine = Engine::new(rate());
        engine.frame = Signal::MAX_FRAMES - 2;
        let mut out = [[1.0; 2]; 5];
        assert_eq!(engine.render(&mut out), 2);
        assert_eq!(engine.frame(), Signal::MAX_FRAMES);
        assert!(engine.is_finished());
        let observation = engine.observation().unwrap();
        assert_eq!(observation.frame, Signal::MAX_FRAMES - 1);
        assert_eq!(out, [[0.0; 2]; 5]);
        assert_eq!(engine.render(&mut out), 0);
        assert_eq!(engine.frame(), Signal::MAX_FRAMES);
        assert_eq!(engine.observation(), Some(observation));
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; binary64 conversion is exact for these bounded fixtures"
    )]
    fn an_empty_target_plays_silence_until_stopped() {
        let mut engine = Engine::new(rate());
        assert!(engine.set(&Target::new(rate(), &[]).unwrap()));
        let mut out = vec![[1.0; 2]; 10_000];
        assert_eq!(engine.render(&mut out), 10_000);
        assert!(out.iter().all(|f| *f == [0.0; 2]));
        assert!(!engine.is_finished());
        engine.set(&Target::new(rate(), &[(id(1), tone(440.0, -20.0, Ears::Both))]).unwrap());
        engine.render(&mut out);
        engine.set(&Target::silence(rate()));
        engine.render(&mut out);
        assert_eq!(engine.live_slots(), 0);
        assert!(!engine.is_finished());
        assert_eq!(engine.render(&mut out), 10_000);
        engine.stop();
        assert_eq!(engine.render(&mut out), engine.ramp_frames() as usize);
        assert!(engine.is_finished());
    }
}
