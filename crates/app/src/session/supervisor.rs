// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

//! Waiting for a stream's terminal outcome, with a stall watchdog, an
//! overall allowance for finite sources and a bounded wait after Stop.
use super::transport::{Rounding, frame_duration};
use super::{Outcome, SessionState};
use auditorylab_core::SampleRate;
use std::time::Duration;

#[cfg(feature = "device-playback")]
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// How long a stream may go without progress, and how long playback may take
/// beyond the signal's length to start up and drain.
pub(crate) const STALL_LIMIT: Duration = Duration::from_secs(30);
/// How long an accepted Stop may wait for drain before forcing silence.
pub(crate) const STOP_LIMIT: Duration = Duration::from_secs(2);

/// The signal's length, rounded up to a nanosecond, plus the stall limit.
pub(crate) fn time_limit(frames: u64, rate: SampleRate) -> Duration {
    frame_duration(frames, rate.hz(), Rounding::Up) + STALL_LIMIT
}

/// Decides from the elapsed time and the progress count when a stream has
/// stalled or outlasted its limit. Rendering frames, then a strictly
/// advancing drain clock, are the only progress evidence.
struct Watchdog {
    time_limit: Option<Duration>,
    last_progress: u64,
    last_progress_at: Duration,
}

impl Watchdog {
    fn new(time_limit: Option<Duration>) -> Self {
        Self {
            time_limit,
            last_progress: 0,
            last_progress_at: Duration::ZERO,
        }
    }

    fn expired(&mut self, elapsed: Duration, progress: u64) -> bool {
        if progress != self.last_progress {
            self.last_progress = progress;
            self.last_progress_at = elapsed;
        }
        self.time_limit.is_some_and(|limit| elapsed >= limit)
            || elapsed.saturating_sub(self.last_progress_at) >= STALL_LIMIT
    }
}

/// Supervision of one running stream.
pub(crate) struct Supervision {
    watchdog: Watchdog,
    stop_seen_at: Option<Duration>,
}

impl Supervision {
    /// `time_limit` is the overall allowance for finite sources; live
    /// sources have none beyond the core frame limit.
    pub(crate) fn new(time_limit: Option<Duration>) -> Self {
        Self {
            watchdog: Watchdog::new(time_limit),
            stop_seen_at: None,
        }
    }

    /// Check the shared state once; return the terminal outcome when there is one.
    ///
    /// The forced-Stop cutoff runs from this supervisor's first observation
    /// of an accepted Stop. It cannot interrupt an operating-system call that
    /// blocks inside the backend.
    pub(crate) fn poll(&mut self, state: &SessionState, elapsed: Duration) -> Option<Outcome> {
        match state.outcome() {
            Outcome::Running => {
                if self.watchdog.expired(elapsed, state.progress()) {
                    state.finish(Outcome::TimedOut);
                }
            }
            Outcome::StopRequested => {
                let since = *self.stop_seen_at.get_or_insert(elapsed);
                if elapsed.saturating_sub(since) >= STOP_LIMIT {
                    state.finish_stop_undrained();
                }
            }
            terminal => return Some(terminal),
        }
        let outcome = state.outcome();
        (!outcome.is_active()).then_some(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn rate(hz: u32) -> SampleRate {
        SampleRate::new(hz).unwrap()
    }

    #[test]
    fn the_time_limit_is_the_signal_length_rounded_up_plus_the_stall_limit() {
        assert_eq!(time_limit(1, rate(8_001)), Duration::new(30, 124_985));
        assert_eq!(time_limit(480_000, rate(48_000)), Duration::from_secs(40));
        assert_eq!(
            time_limit(1 << 36, rate(1)),
            Duration::from_secs((1 << 36) + 30)
        );
    }

    #[test]
    fn the_watchdog_requires_progress_and_enforces_the_time_limit() {
        let mut stalled = Watchdog::new(Some(Duration::from_secs(3_630)));
        assert!(!stalled.expired(Duration::from_secs(29), 0));
        assert!(stalled.expired(Duration::from_secs(30), 0));
        let mut progressing = Watchdog::new(Some(Duration::from_secs(60)));
        assert!(!progressing.expired(Duration::from_secs(29), 1));
        assert!(!progressing.expired(Duration::from_secs(58), 2));
        // Progress does not extend the overall limit.
        assert!(progressing.expired(Duration::from_secs(60), 3));
        let mut resumed = Watchdog::new(Some(Duration::from_secs(3_630)));
        assert!(!resumed.expired(Duration::from_secs(20), 1));
        // Unchanged progress keeps the earlier time as the last progress.
        assert!(!resumed.expired(Duration::from_secs(49), 1));
        assert!(resumed.expired(Duration::from_secs(50), 1));
        let limit = time_limit(1, rate(8_001));
        let mut exact = Watchdog::new(Some(limit));
        assert!(!exact.expired(limit - Duration::from_nanos(1), 1));
        assert!(exact.expired(limit, 2));
        let mut live = Watchdog::new(None);
        for i in 0..200_u64 {
            assert!(!live.expired(Duration::from_secs(i * 29), i + 1));
        }
        assert!(live.expired(Duration::from_secs(199 * 29 + 30), 200));
    }

    #[test]
    fn an_undrained_stop_is_forced_only_after_the_cutoff_and_reports_undrained() {
        let state = SessionState::new();
        state.set_generated(800);
        state.set_estimated_output(400);
        let mut supervision = Supervision::new(Some(Duration::from_secs(3_600)));
        assert_eq!(supervision.poll(&state, Duration::from_secs(5)), None);
        assert_eq!(state.request_stop(), crate::session::StopRequest::Accepted);
        // The cutoff runs from the supervisor's first observation of Stop.
        assert_eq!(supervision.poll(&state, Duration::from_secs(7)), None);
        assert_eq!(supervision.poll(&state, Duration::from_millis(8_999)), None);
        assert_eq!(
            supervision.poll(&state, Duration::from_secs(9)),
            Some(Outcome::StoppedUndrained)
        );
        let report = state.report().unwrap();
        assert!(!report.drained);
        assert_eq!(report.end, crate::session::EndReason::Stopped);
        assert_eq!(
            report.estimated_output_frames, 400,
            "no forced final progress"
        );
        // The watchdog does not run while a Stop is draining.
        let stopping = SessionState::new();
        stopping.request_stop();
        let mut supervision = Supervision::new(Some(Duration::ZERO));
        assert_eq!(supervision.poll(&stopping, Duration::ZERO), None);
    }

    #[test]
    fn a_published_terminal_outcome_is_never_replaced_by_a_timeout() {
        for winner in [
            Outcome::Completed,
            Outcome::Backend,
            Outcome::StoppedDrained,
        ] {
            let state = SessionState::new();
            state.finish(winner);
            let mut supervision = Supervision::new(Some(Duration::ZERO));
            assert_eq!(
                supervision.poll(&state, Duration::from_secs(99)),
                Some(winner)
            );
            assert_eq!(state.outcome(), winner);
        }
        // A stalled stream times out and closes live admission.
        let state = Arc::new(SessionState::new());
        let mut supervision = Supervision::new(None);
        assert_eq!(supervision.poll(&state, Duration::ZERO), None);
        assert_eq!(
            supervision.poll(&state, STALL_LIMIT),
            Some(Outcome::TimedOut)
        );
        assert!(!state.admits_updates());
        assert_eq!(
            state.request_stop(),
            crate::session::StopRequest::AlreadyFinished
        );
    }
}
