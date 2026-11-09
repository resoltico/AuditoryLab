//! Shared controls and publication intent for file and library jobs.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
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

/// Cooperative work stage; preparing/completing do not imply a known fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressPhase {
    /// Opening resources and establishing sample facts.
    Preparing,
    /// Sequentially reaching the selected first frame.
    Skipping,
    /// Processing the selected samples.
    Processing,
    /// Completing numerical findings before publication.
    Completing,
}
impl ProgressPhase {
    /// Whether processed frame counts describe this stage's progress.
    pub const fn determinate(self) -> bool {
        matches!(self, Self::Skipping | Self::Processing)
    }
}
/// A file job's processed frame count and optional known total.
#[derive(Clone, Default)]
pub struct Progress {
    frames: Arc<AtomicU64>,
    phase: Arc<AtomicU8>,
    total: Arc<AtomicU64>,
    notification: Option<Arc<ProgressNotification>>,
}
struct ProgressNotification {
    wake: Arc<dyn Fn() + Send + Sync>,
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
    /// Current cooperative work stage.
    pub fn phase(&self) -> ProgressPhase {
        match self.phase.load(Ordering::Relaxed) {
            1 => ProgressPhase::Skipping,
            2 => ProgressPhase::Processing,
            3 => ProgressPhase::Completing,
            _ => ProgressPhase::Preparing,
        }
    }
    pub(crate) fn set_phase(&self, phase: ProgressPhase) {
        self.phase.store(
            match phase {
                ProgressPhase::Preparing => 0,
                ProgressPhase::Skipping => 1,
                ProgressPhase::Processing => 2,
                ProgressPhase::Completing => 3,
            },
            Ordering::Relaxed,
        );
        if let Some(notification) = &self.notification {
            (notification.wake)();
        }
    }
    /// Frames processed so far.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }
    /// Total frames when the decoder or exporter has established them.
    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }
    pub(crate) fn set_total(&self, total: u64) {
        self.total.store(total, Ordering::Relaxed);
    }
    pub(crate) fn with_waker(&self, wake: Arc<dyn Fn() + Send + Sync>, total: u64) -> Self {
        self.set_total(total);
        Self {
            frames: self.frames.clone(),
            phase: self.phase.clone(),
            total: self.total.clone(),
            notification: Some(Arc::new(ProgressNotification {
                wake,
                percent: AtomicU64::new(0),
            })),
        }
    }
    pub(crate) fn set(&self, n: u64) {
        self.frames.store(n, Ordering::Relaxed);
        if let Some(notification) = &self.notification {
            let percent = n
                .saturating_mul(100)
                .checked_div(self.total())
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

#[cfg(test)]
mod phase_tests {
    use super::*;
    #[test]
    fn phase_changes_reach_shared_progress_without_fabricating_counts() {
        let progress = Progress::default();
        let shared = progress.clone();
        assert_eq!(progress.phase(), ProgressPhase::Preparing);
        assert_eq!(progress.total(), 0);
        for phase in [
            ProgressPhase::Skipping,
            ProgressPhase::Processing,
            ProgressPhase::Completing,
        ] {
            progress.set_phase(phase);
            assert_eq!(shared.phase(), phase);
            assert_eq!(shared.frames(), 0);
        }
    }
}
