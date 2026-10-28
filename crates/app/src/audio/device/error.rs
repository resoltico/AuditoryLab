// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use {crate::audio::session::Outcome, crate::audio::session::SessionState};

/// An opaque native backend failure with its original typed cause.
#[derive(Debug, Clone)]
pub struct BackendError(Arc<dyn Error + Send + Sync>);
impl BackendError {
    /// Retain a backend cause without exposing its native type in app signatures.
    #[must_use]
    pub fn new(cause: impl Error + Send + Sync + 'static) -> Self {
        Self(Arc::new(cause))
    }
}
impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.0.as_ref(), f)
    }
}
impl Error for BackendError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

/// Why device playback did not start or did not finish normally.
#[derive(Debug, Clone)]
pub enum DeviceError {
    /// No default output device is available.
    NoDefaultOutput,
    /// The audio backend refused an operation or reported that the stream
    /// cannot continue.
    Backend(BackendError),
    /// The native output route changed and the stream cannot continue.
    RouteChanged(BackendError),
    /// The device's stream uses a sample representation playback cannot fill.
    UnsupportedSampleFormat(String),
    /// The device runs at a sample rate signals cannot have.
    UnsupportedSampleRate(crate::core::SampleRateError),
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
            Self::Backend(error) | Self::RouteChanged(error) => Some(error),
            Self::UnsupportedSampleRate(error) => Some(error),
            _ => None,
        }
    }
}

impl DeviceError {
    pub(super) fn from_backend(error: cpal::Error) -> Self {
        let changed = error.kind() == cpal::ErrorKind::DeviceChanged;
        let cause = BackendError::new(error);
        if changed {
            Self::RouteChanged(cause)
        } else {
            Self::Backend(cause)
        }
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

impl std::fmt::Display for DeviceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::core::diagnostic::Code::code(self))
    }
}

crate::core::diagnostic_codes! {
    pub(crate) const CODES;
    BackendError {
        codes {
            Self(..) => "device.backend",
        }
        delegates {}
        params(this) {
            crate::core::diagnostic_params! {"cause"=>this.to_string()}
        }
    }
    DeviceError {
        codes {
            Self::NoDefaultOutput => "device.no_default_output",
            Self::RouteChanged(..) => "device.route_changed",
            Self::UnsupportedSampleFormat(..) => "device.unsupported_sample_format",
            Self::TooFewChannels { .. } => "device.too_few_channels",
            Self::InvalidTiming => "device.invalid_timing",
            Self::MisalignedBuffer => "device.misaligned_buffer",
            Self::TimedOut => "device.timed_out",
            Self::Abandoned => "device.abandoned",
        }
        delegates {
            Self::Backend(error) => error,
            Self::UnsupportedSampleRate(error) => error,
        }
        params(this) {
            match this {
                Self::RouteChanged(error) => {
                    crate::core::diagnostic_params! {"cause"=>error.to_string()}
                }
                Self::UnsupportedSampleFormat(format) => {
                    crate::core::diagnostic_params! {"sample_format"=>format.clone()}
                }
                Self::TooFewChannels { channels } => {
                    crate::core::diagnostic_params! {"channels"=>*channels}
                }
                _ => crate::core::diagnostic::Params::new(),
            }
        }
    }
}
