// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use {crate::audio::session::Outcome, crate::audio::session::SessionState};

/// Why device playback did not start or did not finish normally.
#[derive(Debug, Clone)]
pub enum DeviceError {
    /// No default output device is available.
    NoDefaultOutput,
    /// The audio backend refused an operation or reported that the stream
    /// cannot continue.
    Backend(cpal::Error),
    /// The device's stream uses a sample representation playback cannot fill.
    UnsupportedSampleFormat(cpal::SampleFormat),
    /// The device runs at a sample rate signals cannot have.
    UnsupportedSampleRate {
        /// The device's sample rate in frames per second.
        hz: u32,
    },
    /// The default configuration cannot present separate ear channels.
    TooFewChannels {
        /// Available channels in each device frame.
        channels: u16,
    },
    /// The backend's clocks ran backwards or beyond their range.
    InvalidTiming,
    /// The backend supplied a buffer holding a partial frame.
    MisalignedBuffer,
    /// The stream stopped making progress or outlasted its time limit.
    TimedOut,
    /// The session ended before playback could start.
    Abandoned,
}

impl Error for DeviceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Backend(error) => Some(error),
            _ => None,
        }
    }
}

impl From<cpal::Error> for DeviceError {
    fn from(error: cpal::Error) -> Self {
        Self::Backend(error)
    }
}

/// Asynchronous backend reports for one stream. The first stream-ending
/// report claims the single diagnostic slot, stores its payload and only then
/// publishes the fixed Backend outcome; later reports never overwrite it.
pub(super) struct Diagnostics {
    state: Arc<SessionState>,
    claimed: AtomicBool,
    backend: OnceLock<cpal::Error>,
}

impl Diagnostics {
    pub(super) fn new(state: Arc<SessionState>) -> Self {
        Self {
            state,
            claimed: AtomicBool::new(false),
            backend: OnceLock::new(),
        }
    }

    /// Underruns and refused realtime priority never claim the slot.
    pub(super) fn report(&self, error: cpal::Error) {
        match error.kind() {
            cpal::ErrorKind::Xrun => self.state.count_underrun(),
            cpal::ErrorKind::RealtimeDenied => {}
            _ => {
                if self.claim() {
                    self.publish(error);
                }
            }
        }
    }

    fn claim(&self) -> bool {
        self.claimed
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    fn publish(&self, error: cpal::Error) {
        // The elected writer is the only initializer, so set never contends.
        // The terminal Release publication follows the stored payload.
        let _ = self.backend.set(error);
        self.state.finish(Outcome::Backend);
    }

    /// Only called after observing Backend with Acquire ordering.
    pub(super) fn backend_error(&self) -> cpal::Error {
        match self.backend.get() {
            Some(error) => error.clone(),
            None => unreachable!("Backend publishes its payload first"),
        }
    }

    #[cfg(test)]
    pub(super) fn claim_for_test(&self) -> bool {
        self.claim()
    }

    #[cfg(test)]
    pub(super) fn publish_for_test(&self, error: cpal::Error) {
        self.publish(error);
    }
}
