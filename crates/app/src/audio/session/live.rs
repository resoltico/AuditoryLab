//! Live generation from accepted, revisioned targets.
#[cfg(test)]
use super::StopRequest;
use super::{SessionId, SessionState};
use auditorylab_core::{Observation, SampleRate, Target, live::Engine};
use std::sync::{Arc, Mutex};

/// The producer side of the live mailbox: the newest accepted target, the
/// one waiting for the generator, and the last accepted revision.
#[derive(Debug, Default)]
pub(crate) struct Updates {
    pub(crate) pending: Option<(u64, Target)>,
    pub(crate) accepted: Option<(u64, Target)>,
}

pub(crate) type Mailbox = Arc<Mutex<Updates>>;

/// Why a live update was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveUpdateError {
    /// Stop was accepted, generation ended or the generator is gone.
    SessionClosed,
    /// The target was validated at another rate than the session's.
    RateMismatch {
        /// The target's rate in frames per second.
        target_hz: u32,
        /// The session's rate in frames per second.
        session_hz: u32,
    },
    /// The revision counter cannot advance further.
    RevisionLimit,
    /// The control mailbox was poisoned by a panic.
    Unavailable,
}

impl std::error::Error for LiveUpdateError {}

/// Producer handle for accepted whole-target changes at a fixed rate.
#[derive(Debug, Clone)]
pub struct LiveControl {
    state: Arc<SessionState>,
    mailbox: Mailbox,
    rate: SampleRate,
}

impl LiveControl {
    /// The rate every update must be validated at.
    #[must_use]
    pub const fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    /// The session this control addresses.
    #[must_use]
    pub fn session(&self) -> SessionId {
        self.state.id()
    }

    /// Accept an already compiled target as the newest waiting target.
    ///
    /// An exact duplicate of the latest accepted definition keeps its
    /// revision; any other definition gets the next revision and replaces
    /// the waiting target. Admission is decided under the mailbox lock; an
    /// update accepted just before Stop may never be generated.
    ///
    /// # Errors
    /// Returns [`LiveUpdateError`] after Stop or generation end, for another
    /// rate, at the revision limit, or for a poisoned mailbox.
    pub fn update(&self, target: &Target) -> Result<u64, LiveUpdateError> {
        let mut updates = self
            .mailbox
            .lock()
            .map_err(|_| LiveUpdateError::Unavailable)?;
        if !self.state.admits_updates() {
            return Err(LiveUpdateError::SessionClosed);
        }
        if target.sample_rate() != self.rate {
            return Err(LiveUpdateError::RateMismatch {
                target_hz: target.sample_rate().hz(),
                session_hz: self.rate.hz(),
            });
        }
        match updates.accepted {
            Some((revision, accepted)) if accepted == *target => Ok(revision),
            last => {
                let revision = last
                    .map_or(0, |(revision, _)| revision)
                    .checked_add(1)
                    .ok_or(LiveUpdateError::RevisionLimit)?;
                updates.accepted = Some((revision, *target));
                updates.pending = Some((revision, *target));
                Ok(revision)
            }
        }
    }

    /// The latest accepted target and revision, if the mailbox is free.
    /// Accepted is not a claim that the target was generated.
    #[must_use]
    pub fn accepted(&self) -> Option<(u64, Target)> {
        self.mailbox
            .try_lock()
            .ok()
            .and_then(|updates| updates.accepted)
    }
}

/// An open-ended live generator for a validated target, usable without an
/// audio device. Dropping it closes every surviving control.
#[derive(Debug)]
pub(crate) struct LiveGenerator {
    engine: Engine,
    mailbox: Mailbox,
    state: Arc<SessionState>,
}

impl LiveGenerator {
    /// Prepare a generator for an initial target at the target's rate.
    /// Allocation happens here, never while rendering.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn new(target: &Target) -> Self {
        Self::with_state(target, SessionState::new())
    }

    pub(crate) fn with_state(target: &Target, state: Arc<SessionState>) -> Self {
        let updates = Updates {
            pending: Some((1, *target)),
            accepted: Some((1, *target)),
        };
        let mailbox = Arc::new(Mutex::new(updates));
        // Darwin's std mutex lazily allocates its OS storage. Initialize it
        // before the first callback's try_lock.
        drop(
            mailbox
                .lock()
                .unwrap_or_else(|_| unreachable!("fresh live mailbox")),
        );
        Self {
            engine: Engine::new(target.sample_rate()),
            mailbox,
            state,
        }
    }

    /// A handle for accepted changes.
    #[must_use]
    pub(crate) fn control(&self) -> LiveControl {
        LiveControl {
            state: Arc::clone(&self.state),
            mailbox: Arc::clone(&self.mailbox),
            rate: self.engine.sample_rate(),
        }
    }

    pub(crate) fn mailbox(&self) -> &Mailbox {
        &self.mailbox
    }

    /// The generator's sample rate.
    #[must_use]
    pub(crate) const fn sample_rate(&self) -> SampleRate {
        self.engine.sample_rate()
    }

    /// Frames generated so far.
    #[must_use]
    pub(crate) const fn frame(&self) -> u64 {
        self.engine.frame()
    }

    /// Generate the next frames: observe Stop or take the newest waiting
    /// target once, then render. Takes one nonblocking mailbox lock attempt
    /// and allocates nothing. Returns the active frames before the end.
    #[cfg(test)]
    pub(crate) fn render(&mut self, out: &mut [[f64; 2]]) -> usize {
        let stop = self.state.outcome() == super::Outcome::StopRequested;
        self.begin_buffer(stop);
        self.render_buffer(out)
    }

    /// Observe Stop, or take the newest waiting target, at a buffer boundary.
    pub(crate) fn begin_buffer(&mut self, stop: bool) {
        if stop {
            self.engine.stop();
        } else if let Some((revision, target)) = self
            .mailbox
            .try_lock()
            .ok()
            .and_then(|mut updates| updates.pending.take())
        {
            self.engine.set_revision(&target, revision);
        }
    }

    /// Render frames already admitted at the buffer boundary, publishing
    /// generation end at once.
    pub(crate) fn render_buffer(&mut self, out: &mut [[f64; 2]]) -> usize {
        let active = self.engine.render(out);
        if self.engine.is_finished() {
            self.state.end_source();
        }
        active
    }

    /// Last-generated-frame state, without advancing the generator.
    #[must_use]
    pub(crate) const fn observation(&self) -> Option<Observation> {
        self.engine.observation()
    }

    /// Request Stop: the generator finishes its current transition, fades
    /// to silence and ends. Updates are closed from now on. Stopping after
    /// generation ended is harmless.
    #[cfg(test)]
    pub(crate) fn stop(&mut self) -> StopRequest {
        self.state.request_stop()
    }

    /// Whether generation has ended: after Stop's fade, or at the frame limit.
    #[must_use]
    pub(crate) const fn is_finished(&self) -> bool {
        self.engine.is_finished()
    }
}

impl Drop for LiveGenerator {
    fn drop(&mut self) {
        self.state.end_source();
    }
}

impl std::fmt::Display for LiveUpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    LiveUpdateError {
        codes {
            Self::SessionClosed => "live_update.session_closed",
            Self::RateMismatch { .. } => "live_update.rate_mismatch",
            Self::RevisionLimit => "live_update.revision_limit",
            Self::Unavailable => "live_update.unavailable",
        }
        delegates {}
        params(this) {
            match this {
                Self::RateMismatch {
                    target_hz,
                    session_hz,
                } => {
                    crate::core::diagnostic_params! {
                        "target_hz"=>*target_hz,
                        "session_hz"=>*session_hz,
                    }
                }
                _ => crate::core::diagnostic::Params::new(),
            }
        }
    }
}
