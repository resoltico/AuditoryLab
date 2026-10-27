//! Shared controls and publication intent for file and library jobs.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
/// Whether publication may replace an existing destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replace {
    /// Keep existing files untouched.
    Never,
    /// Atomically replace, after the person confirmed it.
    Allowed,
}

/// A file or library job's cancellation request; audio sessions have their own Stop.
#[derive(Clone, Default, Debug)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// Request cancellation.
    pub fn request(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Borrow the cancellation flag for the streaming encoder.
    pub(crate) fn flag(&self) -> &AtomicBool {
        &self.0
    }

    /// Whether cancellation was requested.
    #[must_use]
    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A file export's written frame count.
#[derive(Clone, Default, Debug)]
pub struct Progress(Arc<AtomicU64>);

impl Progress {
    /// Frames written so far.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }

    pub(crate) fn set(&self, n: u64) {
        self.0.store(n, Ordering::Relaxed);
    }
}
