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
#[derive(Clone, Default)]
pub struct Progress {
    frames: Arc<AtomicU64>,
    notification: Option<Arc<ProgressNotification>>,
}
struct ProgressNotification {
    wake: Arc<dyn Fn() + Send + Sync>,
    total: u64,
    percent: AtomicU64,
}
impl std::fmt::Debug for Progress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Progress")
            .field("frames", &self.frames())
            .finish_non_exhaustive()
    }
}

impl Progress {
    /// Frames written so far.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }
    pub(crate) fn with_waker(&self, wake: Arc<dyn Fn() + Send + Sync>, total: u64) -> Self {
        Self {
            frames: self.frames.clone(),
            notification: Some(Arc::new(ProgressNotification {
                wake,
                total,
                percent: AtomicU64::new(0),
            })),
        }
    }
    pub(crate) fn set(&self, n: u64) {
        self.frames.store(n, Ordering::Relaxed);
        if let Some(notification) = &self.notification {
            let percent = n
                .saturating_mul(100)
                .checked_div(notification.total)
                .unwrap_or(100)
                .min(100);
            if notification.percent.fetch_max(percent, Ordering::Relaxed) < percent {
                (notification.wake)();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_progress_notifies_bounded_percentage_changes_and_keeps_shared_frames() {
        let original = Progress::default();
        let wakes = Arc::new(AtomicU64::new(0));
        let count = wakes.clone();
        let watched = original.with_waker(
            Arc::new(move || {
                count.fetch_add(1, Ordering::Relaxed);
            }),
            1000,
        );
        for frames in 0..=1000 {
            watched.set(frames);
        }
        assert_eq!(original.frames(), 1000);
        assert_eq!(wakes.load(Ordering::Relaxed), 100);
        watched.set(1000);
        assert_eq!(wakes.load(Ordering::Relaxed), 100);
    }
}
