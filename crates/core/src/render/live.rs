//! Atomic component transitions with one modulation clock per source relationship.
//! Source lanes sum in authored order, then shared modulation scales the component.
//! Component order is preserved at settlement; linear gains retain peak bounds.
use crate::compiled::CompiledComponent;
use crate::synthesis::{self, Carrier, Modulator};
use crate::{Observation, SampleRate, Signal, SlotRole, Target};
use alloc::{boxed::Box, vec};
const SLOTS: usize = Engine::SLOTS;
#[derive(Debug, Clone, Copy)]
struct LaneState {
    gain: ([f64; 2], [f64; 2]),
    step: (u64, u64),
    phase: u64,
}
fn step(carrier: Carrier) -> u64 {
    match carrier {
        Carrier::Tone { step } => step,
        Carrier::Pulse(_) => 0,
    }
}
fn modulation_steps(modulation: Option<Modulator>) -> (u64, f64) {
    match modulation {
        Some(Modulator::Sinusoidal { step, depth }) => (step, depth),
        _ => (0, 0.0),
    }
}
fn compatible(a: &CompiledComponent, b: &CompiledComponent) -> bool {
    a.source_kind == b.source_kind
        && a.lanes
            .iter()
            .zip(b.lanes.iter())
            .all(|(a, b)| match (a, b) {
                (None, None) => true,
                (Some(a), Some(b)) => match (a.voice.carrier, b.voice.carrier) {
                    (Carrier::Tone { .. }, Carrier::Tone { .. }) => true,
                    (Carrier::Pulse(a), Carrier::Pulse(b)) => a == b,
                    _ => false,
                },
                _ => false,
            })
        && match (a.modulation, b.modulation) {
            (None, None)
            | (Some(Modulator::Sinusoidal { .. }), Some(Modulator::Sinusoidal { .. })) => true,
            (Some(Modulator::Pulsed(a)), Some(Modulator::Pulsed(b))) => a == b,
            _ => false,
        }
}
#[inline]
fn between(a: f64, b: f64, p: f64) -> f64 {
    if p >= 1.0 || a == b {
        b
    } else {
        a * (1.0 - p) + b * p
    }
}
#[inline]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "Validated nonnegative phase steps interpolate within their endpoint range; \
        conversion specifies glide rounding"
)]
fn glide(a: u64, b: u64, p: f64) -> u64 {
    if a == b || p >= 1.0 {
        b
    } else {
        between(a as f64, b as f64, p) as u64
    }
}
#[derive(Debug, Clone, Copy)]
struct Slot {
    component: CompiledComponent,
    // The carriers alone, so each frame reads a few words, not the whole definition.
    carriers: [Option<Carrier>; 2],
    lanes: [Option<LaneState>; 2],
    role: SlotRole,
    origin: u64,
    modulation_step: (u64, u64),
    modulation_phase: u64,
    depth: (f64, f64),
}
impl Slot {
    fn new(component: &CompiledComponent, origin: u64) -> Self {
        let (s, d) = modulation_steps(component.modulation);
        Self {
            component: *component,
            carriers: component
                .lanes
                .map(|lane| lane.map(|lane| lane.voice.carrier)),
            origin,
            role: SlotRole::Incoming,
            lanes: component.lanes.map(|lane| {
                lane.map(|compiled| LaneState {
                    gain: ([0.0; 2], compiled.voice.gains()),
                    step: (step(compiled.voice.carrier), step(compiled.voice.carrier)),
                    phase: 0,
                })
            }),
            modulation_step: (s, s),
            modulation_phase: 0,
            depth: (d, d),
        }
    }
    fn transition(&mut self, next: &CompiledComponent) {
        for (lane, next) in self.lanes.iter_mut().zip(next.lanes) {
            if let (Some(lane), Some(next)) = (lane, next) {
                lane.gain = (lane.gain.1, next.voice.gains());
                lane.step = (lane.step.1, step(next.voice.carrier));
            }
        }
        let (s, d) = modulation_steps(next.modulation);
        self.modulation_step = (self.modulation_step.1, s);
        self.depth = (self.depth.1, d);
        self.component = *next;
        self.carriers = next.lanes.map(|lane| lane.map(|lane| lane.voice.carrier));
        self.role = SlotRole::Continuing;
    }
    fn gains(&self, p: f64) -> [[f64; 2]; 2] {
        self.lanes.each_ref().map(|lane| {
            lane.map_or([0.0; 2], |lane| {
                [
                    between(lane.gain.0[0], lane.gain.1[0], p),
                    between(lane.gain.0[1], lane.gain.1[1], p),
                ]
            })
        })
    }
    #[inline]
    fn advance_modulation(&mut self, p: f64) {
        if let Some(Modulator::Sinusoidal { .. }) = self.component.modulation {
            self.modulation_phase = self.modulation_phase.wrapping_add(glide(
                self.modulation_step.0,
                self.modulation_step.1,
                p,
            ));
        }
    }
    fn frame(&mut self, frame: u64, p: f64) -> [f64; 2] {
        let mut samples = [0.0; 2];
        for (i, (state, carrier)) in self.lanes.iter_mut().zip(&self.carriers).enumerate() {
            if let (Some(state), Some(carrier)) = (state, carrier) {
                samples[i] = match *carrier {
                    Carrier::Tone { .. } => {
                        let sample = synthesis::sine(state.phase);
                        state.phase =
                            state
                                .phase
                                .wrapping_add(glide(state.step.0, state.step.1, p));
                        sample
                    }
                    Carrier::Pulse(pulse) => pulse.sample(frame - self.origin),
                };
            }
        }
        // Silent sources give +0.0 output whatever the shared gain, so a closed
        // pulse skips the gain; its modulation clock still advances.
        let silent = samples == [0.0; 2];
        let modulation = match self.component.modulation {
            None => 1.0,
            Some(_) if silent => {
                self.advance_modulation(p);
                return [0.0; 2];
            }
            Some(Modulator::Pulsed(pulse)) => pulse.sample(frame - self.origin),
            Some(Modulator::Sinusoidal { .. }) => {
                let sample = synthesis::am_gain(
                    self.modulation_phase,
                    between(self.depth.0, self.depth.1, p),
                );
                self.advance_modulation(p);
                sample
            }
        };
        synthesis::component_frame(samples, self.gains(p), modulation)
    }
    fn observe(
        &self,
        rate: SampleRate,
        p: f64,
        out: &mut [Option<crate::VoiceObservation>],
        cursor: &mut usize,
    ) {
        let pair_rate = match self.lanes {
            [Some(first), Some(second)] => Some(crate::facts::pair_rate(
                glide(first.step.0, first.step.1, p),
                glide(second.step.0, second.step.1, p),
                rate,
            )),
            _ => None,
        };
        for ((state, definition), gains) in self
            .lanes
            .iter()
            .zip(self.component.lanes.iter())
            .zip(self.gains(p))
        {
            if let (Some(lane), Some(definition)) = (state, definition) {
                let mut facts = definition.facts;
                if let Some(pair) = &mut facts.pair {
                    pair.realized_offset_hz = pair_rate;
                }
                if let crate::CarrierFacts::Tone(ref mut hz) = facts.carrier {
                    *hz = crate::PhaseRate::from_step(
                        hz.requested_hz,
                        glide(lane.step.0, lane.step.1, p),
                        rate,
                    );
                }
                if let Some(crate::ModulationFacts::Sinusoidal {
                    rate: ref mut hz,
                    ref mut depth,
                }) = facts.modulation
                {
                    *hz = crate::PhaseRate::from_step(
                        hz.requested_hz,
                        glide(self.modulation_step.0, self.modulation_step.1, p),
                        rate,
                    );
                    *depth = between(self.depth.0, self.depth.1, p);
                }
                out[*cursor] = Some(crate::VoiceObservation {
                    id: definition.id,
                    role: self.role,
                    facts,
                    gains,
                });
                *cursor += 1;
            }
        }
    }
}
/// An open-ended, allocation-free renderer of atomic components.
#[derive(Debug, Clone)]
pub struct Engine {
    rate: SampleRate,
    ramp_frames: u64,
    frame: u64,
    slots: Box<[Option<Slot>]>,
    // Both bounded buffers are allocated during preparation, never during a callback.
    spare: Box<[Option<Slot>]>,
    ramp: Option<u64>,
    destination: Option<(Target, u64)>,
    pending: Option<(Target, u64)>,
    requested_revision: u64,
    active_revision: u64,
    settled_revision: u64,
    last: Option<Observation>,
    stopping: bool,
    finished: bool,
}
impl Engine {
    /// Storage derives from the single authored lane budget; active work is at most 32 lanes.
    pub const SLOTS: usize = 2 * crate::Sound::MAX_COMPONENTS;
    /// Interactive startup/change/Stop duration; independent of authored fades and pulse edges.
    pub const RAMP_SECONDS: f64 = 0.03;
    /// Start silently at the selected sample rate.
    pub fn new(rate: SampleRate) -> Self {
        Self {
            rate,
            ramp_frames: Self::ramp_frames_at(rate),
            frame: 0,
            slots: vec![None; SLOTS].into_boxed_slice(),
            spare: vec![None; SLOTS].into_boxed_slice(),
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
    /// Resolved interactive transition duration, at least one frame.
    pub fn ramp_frames_at(rate: SampleRate) -> u64 {
        rate.frames_in(Self::RAMP_SECONDS).unwrap_or(1).max(1)
    }
    /// Rendering sample rate.
    pub const fn sample_rate(&self) -> SampleRate {
        self.rate
    }
    /// Next frame index.
    pub const fn frame(&self) -> u64 {
        self.frame
    }
    /// Frames in each interactive transition.
    pub const fn ramp_frames(&self) -> u64 {
        self.ramp_frames
    }
    /// Submit a target under the next revision.
    pub fn set(&mut self, target: &Target) -> bool {
        self.set_revision(target, self.requested_revision.saturating_add(1))
    }
    /// Submit a strictly newer target; same-audio updates preserve all generator state and retire
    /// waiting work.
    pub fn set_revision(&mut self, target: &Target, revision: u64) -> bool {
        if self.finished
            || self.stopping
            || target.sample_rate() != self.rate
            || revision <= self.requested_revision
        {
            return false;
        }
        self.requested_revision = revision;
        if self
            .destination
            .as_ref()
            .is_some_and(|(destination, _)| destination.same_audio(target))
        {
            self.destination = Some((*target, revision));
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
                if let Some(next) = target
                    .mix()
                    .components()
                    .find(|entry| entry.id == slot.component.id)
                {
                    slot.component = *next;
                }
            }
        } else {
            self.pending = Some((*target, revision));
        }
        true
    }
    /// Last actual generated frame's observation; never advances clocks.
    pub const fn observation(&self) -> Option<Observation> {
        self.last
    }
    /// Stop takes precedence over subsequent targets and fades after the active transition.
    pub fn stop(&mut self) {
        if !self.stopping {
            self.stopping = true;
            self.pending = Some((Target::silence(self.rate), self.active_revision));
        }
    }
    /// Stop has settled or the rendering horizon has been exhausted.
    pub const fn is_finished(&self) -> bool {
        self.finished
    }
    fn begin(&mut self, target: &Target) {
        self.spare.fill(None);
        let mut used = [false; SLOTS];
        let mut cursor = 0;
        for component in target.mix().components() {
            let found = self
                .slots
                .iter()
                .enumerate()
                .find(|(i, slot)| {
                    !used[*i]
                        && slot.as_ref().is_some_and(|slot| {
                            slot.component.id == component.id
                                && compatible(&slot.component, component)
                        })
                })
                .map(|(i, _)| i);
            let slot = if let Some(i) = found {
                used[i] = true;
                let mut slot = self.slots[i].unwrap_or_else(|| unreachable!("present slot"));
                slot.transition(component);
                slot
            } else {
                Slot::new(component, self.frame)
            };
            self.spare[cursor] = Some(slot);
            cursor += 1;
        }
        for (i, slot) in self.slots.iter().enumerate() {
            if !used[i]
                && let Some(mut slot) = *slot
            {
                slot.role = SlotRole::Outgoing;
                for lane in slot.lanes.iter_mut().flatten() {
                    lane.gain = (lane.gain.1, [0.0; 2]);
                }
                self.spare[cursor] = Some(slot);
                cursor += 1;
            }
        }
        core::mem::swap(&mut self.slots, &mut self.spare);
        self.ramp = Some(0);
    }
    fn end_ramp(&mut self) {
        for slot in self.slots.as_mut() {
            if let Some(value) = slot {
                if value.role == SlotRole::Outgoing {
                    *slot = None;
                } else {
                    value.role = SlotRole::Continuing;
                    for lane in value.lanes.iter_mut().flatten() {
                        lane.gain.0 = lane.gain.1;
                        lane.step.0 = lane.step.1;
                    }
                    value.modulation_step.0 = value.modulation_step.1;
                    value.depth.0 = value.depth.1;
                }
            }
        }
        self.ramp = None;
        self.settled_revision = self.active_revision;
        if self.stopping && self.pending.is_none() && self.slots.iter().all(Option::is_none) {
            self.finished = true;
        }
    }
    fn snapshot(&self, p: f64) -> Observation {
        let mut voices = [None; Observation::MAX_VOICES];
        let mut cursor = 0;
        for slot in self
            .slots
            .iter()
            .flatten()
            .filter(|slot| slot.role == SlotRole::Outgoing)
            .chain(
                self.slots
                    .iter()
                    .flatten()
                    .filter(|slot| slot.role != SlotRole::Outgoing),
            )
        {
            slot.observe(self.rate, p, &mut voices, &mut cursor);
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
    /// Render arbitrary buffer sizes without allocation, locks or I/O; zero the exhausted tail.
    #[expect(
        clippy::cast_precision_loss,
        reason = "Transition counters are at most 23040 and exactly representable"
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
            let p = self
                .ramp
                .map_or(1.0, |done| (done + 1) as f64 / self.ramp_frames as f64);
            let mut frame = [0.0; 2];
            for slot in self.slots.iter_mut().flatten() {
                let sample = slot.frame(self.frame, p);
                frame[0] += sample[0];
                frame[1] += sample[1];
            }
            out[index] = frame.map(|value| value.clamp(-1.0, 1.0));
            if index + 1 == out.len()
                || (self.ramp.is_some() && p == 1.0)
                || self.frame + 1 >= Signal::MAX_FRAMES
            {
                self.last = Some(self.snapshot(p));
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
    #[cfg(test)]
    fn live_slots(&self) -> usize {
        self.slots
            .iter()
            .flatten()
            .map(|slot| slot.lanes.iter().flatten().count())
            .sum()
    }
    #[cfg(test)]
    #[expect(
        clippy::cast_precision_loss,
        reason = "Small transition counters are exact"
    )]
    fn level_sums(&self) -> [f64; 2] {
        let p = self
            .ramp
            .map_or(1.0, |done| (done + 1) as f64 / self.ramp_frames as f64);
        let mut sums = [0.0; 2];
        for slot in self.slots.iter().flatten() {
            for gains in slot.gains(p) {
                sums[0] += gains[0];
                sums[1] += gains[1];
            }
        }
        sums
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Channel, ChannelRouting, ComponentForm, Modulation, Recipe, SoundError};
    use alloc::vec;
    use alloc::vec::Vec;
    use proptest::{prelude::*, test_runner::Config, test_runner::TestRunner};

    #[test]
    fn terminal_horizon_refuses_late_targets_without_rewriting_generated_state() {
        let mut engine = Engine::new(SampleRate::DEFAULT);
        engine.frame = Signal::MAX_FRAMES - 1;
        assert_eq!(engine.render(&mut [[0.0; 2]; 2]), 1);
        let last = engine.observation();
        assert!(engine.is_finished());
        assert!(!engine.set_revision(&Target::silence(SampleRate::DEFAULT), 1));
        assert_eq!(engine.observation(), last);
        assert!(engine.pending.is_none());
    }

    fn rate() -> SampleRate {
        SampleRate::new(48_000).unwrap()
    }

    fn tone(hz: f64, peak_dbfs: f64, route: ChannelRouting) -> ComponentForm {
        crate::ComponentForm {
            source: crate::SoundSource::Tone {
                route,
                peak_dbfs: crate::test_support::finite(peak_dbfs),
                frequency_hz: crate::test_support::finite(hz),
            },
            modulation: None,
        }
    }

    fn clicks(hz: f64, peak_dbfs: f64) -> ComponentForm {
        crate::ComponentForm {
            source: crate::SoundSource::PulseTrain {
                route: ChannelRouting::Both,
                peak_dbfs: crate::test_support::finite(peak_dbfs),
                pulse: crate::test_support::rectangular_pulse(hz, 0.001),
            },
            modulation: None,
        }
    }

    fn id(n: u32) -> crate::ComponentId {
        crate::ComponentId::new(n + 1).unwrap()
    }

    fn numbered(components: &[ComponentForm]) -> Vec<(crate::ComponentId, ComponentForm)> {
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
    fn closed_pulses_keep_modulation_phase_and_exact_reopening_samples() {
        for hz in [8_000, 44_100, 48_000, 768_000] {
            for depth in [0.0, 0.3, 1.0] {
                let voice = crate::ComponentForm {
                    modulation: Some(Modulation::Sinusoidal {
                        rate_hz: crate::test_support::finite(39.0625),
                        depth: crate::test_support::finite(depth),
                    }),
                    ..clicks(40.0, 0.0)
                };
                let rate = SampleRate::new(hz).unwrap();
                let target = crate::test_support::target(rate, &[(id(1), voice)]).unwrap();
                let mut engine = Engine::new(rate);
                engine.begin(&target);
                let mut slot = engine.slots.iter().flatten().next().copied().unwrap();
                let reference = target.mix().components().next().unwrap();
                let mut closed = false;
                let mut reopened = false;
                for frame in 0..u64::from(hz / 10) {
                    // The seekable generator computes phase from the absolute
                    // frame, independently of the live slot's advancing clock.
                    let expected = reference.frame(frame)[0];
                    assert_eq!(slot.frame(frame, 1.0)[0].to_bits(), expected.to_bits());
                    if expected == 0.0 {
                        closed = true;
                    } else if closed {
                        reopened = true;
                    }
                }
                assert!(
                    reopened,
                    "the oracle must cover a pulse after a closed span"
                );
            }
        }
    }

    #[test]
    fn shared_modulation_phase_keeps_exact_samples_and_independent_clocks() {
        for shared_depth in [true, false] {
            for hz in [8_000, 44_100, 48_000, 96_000, 768_000] {
                let components: Vec<_> = (0..16)
                    .map(|n| crate::ComponentForm {
                        modulation: Some(Modulation::Sinusoidal {
                            rate_hz: crate::test_support::finite(39.0625),
                            depth: if shared_depth {
                                crate::test_support::finite(1.0)
                            } else {
                                crate::test_support::finite(f64::from(n) / 15.0)
                            },
                        }),
                        ..tone(200.0 + f64::from(n) * 20.0, -30.0, ChannelRouting::Both)
                    })
                    .collect();
                let mut engine = Engine::new(SampleRate::new(hz).unwrap());
                engine.set(
                    &crate::test_support::target(engine.rate, &numbered(&components)).unwrap(),
                );
                engine.render(&mut [[0.0; 2]]);
                let mut slots: Vec<_> = engine.slots.iter().copied().flatten().collect();
                for progress in [None, Some(0.0), Some(0.25), Some(0.999), Some(1.0)] {
                    for frame in 0..64 {
                        for (n, slot) in slots.iter_mut().enumerate() {
                            if frame == 32 {
                                // Equal and unequal phases, a sign change and wrap,
                                // plus distinct glide steps must remain independent.
                                slot.modulation_phase = match n % 4 {
                                    0 => 0,
                                    1 => 1 << 63,
                                    2 => u64::MAX,
                                    _ => u64::try_from(n).unwrap(),
                                };
                                slot.modulation_step.1 += u64::try_from(n).unwrap();
                            }
                            let p = progress.unwrap_or(1.0);
                            // Independent sample and clock expectations.
                            let carrier = synthesis::sine(slot.lanes[0].unwrap().phase);
                            let depth = between(slot.depth.0, slot.depth.1, p);
                            let gain = (1.0 + depth * synthesis::sine(slot.modulation_phase))
                                / (1.0 + depth);
                            let expected = if slot.gains(p)[0][0] == 0.0 {
                                0.0
                            } else {
                                (0.0 + carrier * slot.gains(p)[0][0]) * gain
                            };
                            let carrier_phase = slot.lanes[0].unwrap().phase.wrapping_add(glide(
                                slot.lanes[0].unwrap().step.0,
                                slot.lanes[0].unwrap().step.1,
                                p,
                            ));
                            let modulation_phase = slot.modulation_phase.wrapping_add(glide(
                                slot.modulation_step.0,
                                slot.modulation_step.1,
                                p,
                            ));
                            assert_eq!(slot.frame(frame, p)[0].to_bits(), expected.to_bits());
                            assert_eq!(slot.lanes[0].unwrap().phase, carrier_phase);
                            assert_eq!(slot.modulation_phase, modulation_phase);
                        }
                    }
                }
            }
        }
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; \
            binary64 conversion is exact for these bounded fixtures"
    )]
    fn an_unedited_session_equals_the_signal_after_its_start_ramp() {
        let mut stimuli: Vec<crate::FiniteFixture> = crate::catalog::ENTRIES
            .iter()
            .map(|entry| crate::FiniteFixture::from_recipe(&Recipe::parse(&entry.text()).unwrap()))
            .collect();
        stimuli.push(
            crate::test_support::FiniteFixture::new(
                crate::test_support::finite(1.0),
                crate::test_support::finite(0.0),
                (vec![
                    tone(250.0, -20.0, ChannelRouting::Left),
                    tone(256.0, -20.0, ChannelRouting::Right),
                    clicks(40.0, -20.0),
                ])
                .into_iter()
                .enumerate()
                .map(|(index, voice)| crate::Component {
                    id: crate::ComponentId::numbered(index).unwrap(),
                    form: voice,
                })
                .collect(),
            )
            .unwrap_or_else(|error| panic!("invalid sound fixture: {error}")),
        );
        for mut sound in stimuli {
            sound.set_duration(crate::test_support::finite(1.0));
            sound.set_fade(crate::test_support::finite(0.0));
            let signal = sound.signal(rate()).unwrap();
            let target = sound.target(rate()).unwrap();
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
        let too_high = tone(30_000.0, -20.0, ChannelRouting::Both);
        let sound = crate::test_support::FiniteFixture::new(
            crate::test_support::finite(1.0),
            crate::test_support::finite(0.0),
            (vec![tone(440.0, -20.0, ChannelRouting::Both), too_high])
                .into_iter()
                .enumerate()
                .map(|(index, voice)| crate::Component {
                    id: crate::ComponentId::numbered(index).unwrap(),
                    form: voice,
                })
                .collect(),
        )
        .unwrap_or_else(|error| panic!("invalid sound fixture: {error}"));
        assert_eq!(
            sound.check(rate()).into_result().unwrap_err(),
            sound.signal(rate()).unwrap_err()
        );
        let loud = [
            tone(440.0, -3.0, ChannelRouting::Left),
            tone(441.0, -3.0, ChannelRouting::Left),
        ];
        assert!(matches!(
            crate::test_support::target(rate(), &numbered(&loud)),
            Err(SoundError::Peak {
                channel: Channel::Left,
                ..
            })
        ));
        let many = vec![tone(440.0, -40.0, ChannelRouting::Both); 17];
        assert_eq!(
            crate::test_support::sound(&numbered(&many)),
            Err(crate::StructureError::VoiceBudget { count: 17 })
        );
        let other = SampleRate::new(44_100).unwrap();
        let mut engine = Engine::new(rate());
        assert!(!engine.set(&Target::silence(other)));
    }

    #[test]
    #[expect(
        clippy::cast_precision_loss,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; \
            binary64 conversion is exact for these bounded fixtures"
    )]
    fn a_retuned_tone_keeps_its_phase_and_a_reset_would_not() {
        let mut engine = Engine::new(rate());
        engine.set(
            &crate::test_support::target(
                rate(),
                &[(id(7), tone(440.0, -6.0, ChannelRouting::Both))],
            )
            .unwrap(),
        );
        // 4,827 frames end near a crest of 440 Hz, where a phase reset would jump.
        let mut frames = render(&mut engine, 4_827);
        engine.set(
            &crate::test_support::target(
                rate(),
                &[(id(7), tone(880.0, -6.0, ChannelRouting::Both))],
            )
            .unwrap(),
        );
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
                amplitude * libm::sin(core::f64::consts::TAU * 880.0 * f64::from(n) / 48_000.0)
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
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; \
            binary64 conversion is exact for these bounded fixtures"
    )]
    fn a_level_change_moves_linearly_over_one_ramp() {
        // A pulse wider than the session holds the carrier at one.
        let held = crate::ComponentForm {
            source: crate::SoundSource::PulseTrain {
                route: ChannelRouting::Both,
                peak_dbfs: crate::test_support::finite(-20.0),
                pulse: crate::test_support::rectangular_pulse(0.1, 5.0),
            },
            modulation: None,
        };
        let mut engine = Engine::new(rate());
        engine.set(&crate::test_support::target(rate(), &[(id(1), held)]).unwrap());
        render(&mut engine, 2_000);
        let louder = {
            let mut value = held;
            assert!(value.set(
                crate::ParameterId::PeakLevel,
                crate::test_support::finite(-6.0)
            ));
            value
        };
        engine.set(&crate::test_support::target(rate(), &[(id(1), louder)]).unwrap());
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
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; \
            binary64 conversion is exact for these bounded fixtures"
    )]
    fn a_changed_pulse_crossfades_and_a_retuned_tone_does_not() {
        let mut engine = Engine::new(rate());
        engine.set(&crate::test_support::target(rate(), &[(id(1), clicks(40.0, -20.0))]).unwrap());
        render(&mut engine, 2_000);
        engine.set(&crate::test_support::target(rate(), &[(id(1), clicks(80.0, -20.0))]).unwrap());
        let ramp = engine.ramp_frames() as usize;
        render(&mut engine, 1);
        assert_eq!(engine.live_slots(), 2);
        render(&mut engine, ramp);
        assert_eq!(engine.live_slots(), 1);
        engine.set(
            &crate::test_support::target(
                rate(),
                &[(id(1), tone(300.0, -20.0, ChannelRouting::Left))],
            )
            .unwrap(),
        );
        render(&mut engine, 1);
        assert_eq!(engine.live_slots(), 2);
        render(&mut engine, ramp);
        engine.set(
            &crate::test_support::target(
                rate(),
                &[(id(1), tone(600.0, -10.0, ChannelRouting::Right))],
            )
            .unwrap(),
        );
        render(&mut engine, 1);
        assert_eq!(engine.live_slots(), 1);
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; \
            binary64 conversion is exact for these bounded fixtures"
    )]
    fn targets_during_a_ramp_wait_and_only_the_newest_applies() {
        let mut engine = Engine::new(rate());
        let quiet = crate::test_support::target(
            rate(),
            &[(id(1), tone(440.0, -40.0, ChannelRouting::Both))],
        )
        .unwrap();
        let skipped = crate::test_support::target(
            rate(),
            &[(id(2), tone(500.0, -10.0, ChannelRouting::Both))],
        )
        .unwrap();
        let newest = crate::test_support::target(
            rate(),
            &[(id(3), tone(600.0, -20.0, ChannelRouting::Left))],
        )
        .unwrap();
        engine.set(&quiet);
        render(&mut engine, 10);
        engine.set(&skipped);
        engine.set(&newest);
        let remaining = engine.ramp_frames() as usize - 10;
        render(&mut engine, remaining);
        assert_eq!(engine.live_slots(), 1);
        render(&mut engine, 1);
        assert!(
            engine
                .slots
                .iter()
                .flatten()
                .all(|s| s.component.id != id(2))
        );
        assert!(
            engine
                .slots
                .iter()
                .flatten()
                .any(|s| s.component.id == id(3))
        );
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; \
            binary64 conversion is exact for these bounded fixtures"
    )]
    fn stopping_fades_out_within_one_ramp_and_then_finishes() {
        let mut engine = Engine::new(rate());
        engine.set(
            &crate::test_support::target(
                rate(),
                &[(id(1), tone(440.0, -6.0, ChannelRouting::Both))],
            )
            .unwrap(),
        );
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
    fn sixteen_voices_changing_kind_use_every_slot_within_full_scale() {
        let tones: Vec<ComponentForm> = (0..16)
            .map(|i| tone(200.0 + 50.0 * f64::from(i), -24.1, ChannelRouting::Both))
            .collect();
        let pulses: Vec<ComponentForm> = (0..16).map(|_| clicks(40.0, -24.1)).collect();
        let mut engine = Engine::new(rate());
        engine.set(&crate::test_support::target(rate(), &numbered(&tones)).unwrap());
        render(&mut engine, 2_000);
        engine.set(&crate::test_support::target(rate(), &numbered(&pulses)).unwrap());
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
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; \
            binary64 conversion is exact for these bounded fixtures"
    )]
    fn random_edits_keep_every_ear_within_full_scale() {
        let component = (0_u8..3, 0_u8..4, 1.0_f64..20_000.0, 0.0_f64..1.0, 0_u32..4).prop_map(
            |(route, kind, hz, level, id)| {
                let route = [
                    ChannelRouting::Left,
                    ChannelRouting::Right,
                    ChannelRouting::Both,
                ][usize::from(route)];
                let source = if kind == 3 {
                    crate::SoundSource::PulseTrain {
                        route,
                        peak_dbfs: crate::test_support::finite(0.0),
                        pulse: crate::test_support::rectangular_pulse(hz.min(1_000.0), 0.0005),
                    }
                } else {
                    crate::SoundSource::Tone {
                        route,
                        peak_dbfs: crate::test_support::finite(0.0),
                        frequency_hz: crate::test_support::finite(hz),
                    }
                };
                let modulation = match kind {
                    1 => Some(Modulation::Sinusoidal {
                        rate_hz: crate::test_support::finite(40.0),
                        depth: crate::test_support::finite(level),
                    }),
                    2 => Some(Modulation::Pulsed {
                        pulse: crate::PulseSpec {
                            rate_hz: crate::test_support::finite(40.0),
                            width: crate::PulseWidth::Duration {
                                seconds: crate::test_support::finite(0.001),
                            },
                            shape: crate::PulseShape::Rectangular {},
                        },
                    }),
                    _ => None,
                };
                (id, crate::ComponentForm { source, modulation }, level)
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
                    let components: Vec<(crate::ComponentId, ComponentForm)> = parts
                        .iter()
                        .enumerate()
                        .map(|(i, (id, form, level))| {
                            let amplitude = (0.05 + 0.95 * level) / n;
                            (
                                crate::ComponentId::new(id * 16 + u32::try_from(i).unwrap() + 1)
                                    .unwrap(),
                                {
                                    let mut form = *form;
                                    assert!(form.set(
                                        crate::ParameterId::PeakLevel,
                                        crate::test_support::finite(20.0 * libm::log10(amplitude))
                                    ));
                                    form
                                },
                            )
                        })
                        .collect();
                    if let Ok(target) = crate::test_support::target(rate(), &components) {
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
        let a = crate::test_support::target(
            rate(),
            &numbered(&[
                tone(440.0, -20.0, ChannelRouting::Left),
                clicks(40.0, -20.0),
            ]),
        )
        .unwrap();
        let b = crate::test_support::target(
            rate(),
            &numbered(&[
                clicks(80.0, -20.0),
                tone(450.0, -26.0, ChannelRouting::Right),
            ]),
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

    fn gated(hz: f64, peak_dbfs: f64, width_seconds: f64) -> ComponentForm {
        crate::ComponentForm {
            modulation: Some(Modulation::Pulsed {
                pulse: crate::PulseSpec {
                    rate_hz: crate::test_support::finite(40.0),
                    width: crate::PulseWidth::Duration {
                        seconds: crate::test_support::finite(width_seconds),
                    },
                    shape: crate::PulseShape::Rectangular {},
                },
            }),
            ..tone(hz, peak_dbfs, ChannelRouting::Both)
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
            engine.set(&crate::test_support::target(rate(), &[(id(1), before)]).unwrap());
            render(&mut engine, 2_000);
            engine.set(&crate::test_support::target(rate(), &[(id(1), after)]).unwrap());
            render(&mut engine, 1);
            assert_eq!(engine.live_slots(), slots, "{before:?} -> {after:?}");
        }
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; \
            binary64 conversion is exact for these bounded fixtures"
    )]
    fn frequencies_glide_through_intermediate_steps_during_a_ramp() {
        let am = |hz: f64, rate_hz: f64, depth: f64| crate::ComponentForm {
            modulation: Some(Modulation::Sinusoidal {
                rate_hz: crate::test_support::finite(rate_hz),
                depth: crate::test_support::finite(depth),
            }),
            ..tone(hz, -20.0, ChannelRouting::Both)
        };
        let mut engine = Engine::new(rate());
        engine.set(&crate::test_support::target(rate(), &[(id(1), am(440.0, 10.0, 0.0))]).unwrap());
        render(&mut engine, 2_000);
        engine.set(&crate::test_support::target(rate(), &[(id(1), am(880.0, 40.0, 1.0))]).unwrap());
        let ramp = engine.ramp_frames() as usize;
        render(&mut engine, ramp / 2);
        let slot = engine.slots.iter().flatten().next().copied().unwrap();
        render(&mut engine, 1);
        let next = engine.slots.iter().flatten().next().copied().unwrap();
        let carrier = next.lanes[0]
            .unwrap()
            .phase
            .wrapping_sub(slot.lanes[0].unwrap().phase);
        let modulation = next.modulation_phase.wrapping_sub(slot.modulation_phase);
        assert!(slot.lanes[0].unwrap().step.0 < carrier && carrier < slot.lanes[0].unwrap().step.1);
        assert!(slot.modulation_step.0 < modulation && modulation < slot.modulation_step.1);
        // Depth is halfway too: recompute the frame from the slot's own phases.
        let p = (ramp / 2 + 1) as f64 / ramp as f64;
        let depth = slot.depth.0 * (1.0 - p) + slot.depth.1 * p;
        assert!(0.4 < depth && depth < 0.6);
        let mut copy = slot;
        let value = copy.frame(engine.frame() - 1, p)[0];
        let expected = (synthesis::sine(slot.lanes[0].unwrap().phase) * slot.gains(p)[0][0])
            * ((1.0 + depth * synthesis::sine(slot.modulation_phase)) / (1.0 + depth));
        assert_eq!(value.to_bits(), expected.to_bits());
        render(&mut engine, ramp);
        let settled = engine.slots.iter().flatten().next().copied().unwrap();
        assert_eq!(
            settled.lanes[0].unwrap().step,
            (slot.lanes[0].unwrap().step.1, slot.lanes[0].unwrap().step.1)
        );
        assert_eq!(settled.depth, (1.0, 1.0));
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; \
            binary64 conversion is exact for these bounded fixtures"
    )]
    fn pulses_started_mid_session_count_from_their_own_first_frame() {
        let reference = |n: u64| (n % 1_200) < 48;
        let mut engine = Engine::new(rate());
        render(&mut engine, 1_000);
        assert_eq!(engine.frame(), 1_000);
        engine.set(&crate::test_support::target(rate(), &[(id(1), clicks(40.0, -6.0))]).unwrap());
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
        engine.set(
            &crate::test_support::target(rate(), &[(id(1), gated(1_000.0, -6.0, 0.001))]).unwrap(),
        );
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
        engine.set(
            &crate::test_support::target(
                rate(),
                &[(id(1), tone(440.0, -30.0, ChannelRouting::Both))],
            )
            .unwrap(),
        );
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
        reason = "Test-only frame indices and ramp lengths fit the allocated buffers; \
            binary64 conversion is exact for these bounded fixtures"
    )]
    fn an_empty_target_plays_silence_until_stopped() {
        let mut engine = Engine::new(rate());
        assert!(engine.set(&crate::test_support::target(rate(), &[]).unwrap()));
        let mut out = vec![[1.0; 2]; 10_000];
        assert_eq!(engine.render(&mut out), 10_000);
        assert!(out.iter().all(|f| *f == [0.0; 2]));
        assert!(!engine.is_finished());
        engine.set(
            &crate::test_support::target(
                rate(),
                &[(id(1), tone(440.0, -20.0, ChannelRouting::Both))],
            )
            .unwrap(),
        );
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
