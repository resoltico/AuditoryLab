// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

/// Fixed outcomes shared with the sample callback. Rich errors stay off that path.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Terminal {
    #[default]
    Running,
    Complete,
    Cancelled,
    TimedOut,
    InvalidTiming,
    MisalignedBuffer,
    Backend,
    StreamSetupFailed,
}

/// The first terminal publication wins. Sample processing only touches atomics;
/// a backend reporter may still be preparing an unused diagnostic afterwards.
#[derive(Default)]
pub(crate) struct Shared {
    terminal: AtomicU8,
    backend_claimed: AtomicBool,
    backend_error: OnceLock<cpal::Error>,
    progress: AtomicU64,
    underruns: AtomicU64,
}

impl Shared {
    pub(crate) fn finish(&self, terminal: Terminal) {
        debug_assert_ne!(terminal, Terminal::Running);
        let _ = self.terminal.compare_exchange(
            Terminal::Running as u8,
            terminal as u8,
            Ordering::Release,
            Ordering::Relaxed,
        );
    }

    pub(crate) fn terminal(&self) -> Terminal {
        match self.terminal.load(Ordering::Acquire) {
            0 => Terminal::Running,
            1 => Terminal::Complete,
            2 => Terminal::Cancelled,
            3 => Terminal::TimedOut,
            4 => Terminal::InvalidTiming,
            5 => Terminal::MisalignedBuffer,
            6 => Terminal::Backend,
            7 => Terminal::StreamSetupFailed,
            _ => unreachable!("only fixed terminal codes are published"),
        }
    }

    /// Only called by the worker after observing Backend with Acquire ordering.
    pub(crate) fn backend_error(&self) -> &cpal::Error {
        let Some(error) = self.backend_error.get() else {
            unreachable!("Backend publishes its payload first");
        };
        error
    }

    /// Underruns and refused realtime priority never claim diagnostic storage.
    pub(crate) fn report(&self, error: cpal::Error) {
        match error.kind() {
            cpal::ErrorKind::Xrun => {
                self.underruns.fetch_add(1, Ordering::Relaxed);
            }
            cpal::ErrorKind::RealtimeDenied => {}
            _ => {
                if self.claim_backend() {
                    self.publish_backend(error);
                }
            }
        }
    }

    fn claim_backend(&self) -> bool {
        self.backend_claimed
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    fn publish_backend(&self, error: cpal::Error) {
        // The elected asynchronous writer is the only initializer, so set never
        // contends with another initializer. The terminal Release follows storage.
        assert!(
            self.backend_error.set(error).is_ok(),
            "only one backend writer"
        );
        self.finish(Terminal::Backend);
    }

    pub(crate) fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    pub(crate) fn mark_progress(&self) {
        self.progress.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn progress(&self) -> u64 {
        self.progress.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::ErrorKind;
    use std::sync::{Arc, Barrier};

    const OUTCOMES: [Terminal; 7] = [
        Terminal::Complete,
        Terminal::Cancelled,
        Terminal::TimedOut,
        Terminal::InvalidTiming,
        Terminal::MisalignedBuffer,
        Terminal::Backend,
        Terminal::StreamSetupFailed,
    ];

    #[test]
    fn every_pair_preserves_the_first_publication() {
        for first in OUTCOMES {
            for second in OUTCOMES {
                let shared = Shared::default();
                assert_eq!(shared.terminal(), Terminal::Running);
                shared.finish(first);
                shared.finish(second);
                assert_eq!(shared.terminal(), first);
            }
        }
    }

    #[test]
    fn concurrent_fixed_publications_agree_on_one_winner() {
        for first in OUTCOMES {
            for second in OUTCOMES {
                let shared = Arc::new(Shared::default());
                let barrier = Arc::new(Barrier::new(3));
                let workers: Vec<_> = [first, second]
                    .into_iter()
                    .map(|outcome| {
                        let shared = Arc::clone(&shared);
                        let barrier = Arc::clone(&barrier);
                        std::thread::spawn(move || {
                            barrier.wait();
                            shared.finish(outcome);
                        })
                    })
                    .collect();
                barrier.wait();
                for worker in workers {
                    worker.join().unwrap();
                }
                let winner = shared.terminal();
                assert!(winner == first || winner == second);
                shared.finish(Terminal::Cancelled);
                assert_eq!(shared.terminal(), winner);
            }
        }
    }

    #[test]
    fn paused_backend_writer_cannot_block_fixed_publication_or_other_reporters() {
        for outcome in [
            Terminal::Complete,
            Terminal::Cancelled,
            Terminal::StreamSetupFailed,
        ] {
            let shared = Arc::new(Shared::default());
            let claimed = Arc::new(Barrier::new(2));
            let resume = Arc::new(Barrier::new(2));
            let writer = {
                let shared = Arc::clone(&shared);
                let claimed = Arc::clone(&claimed);
                let resume = Arc::clone(&resume);
                std::thread::spawn(move || {
                    assert!(shared.claim_backend());
                    claimed.wait();
                    resume.wait();
                    shared.publish_backend(cpal::Error::new(ErrorKind::DeviceChanged));
                })
            };
            claimed.wait();
            shared.report(cpal::Error::new(ErrorKind::DeviceBusy));
            if outcome == Terminal::Complete {
                let session = auditorylab_core::Stimulus {
                    duration_seconds: 1.0 / 8_000.0,
                    fade_seconds: 0.0,
                    components: vec![],
                }
                .session(auditorylab_core::SampleRate::new(8_000).unwrap())
                .unwrap();
                let mut callback =
                    crate::playback::callback::Callback::new(session.into_renderer(), 2, Arc::clone(&shared));
                let mut buffer = [1.0; 2];
                for millis in [0, 1] {
                    let now = cpal::StreamInstant::from_millis(millis);
                    callback.fill(
                        &mut buffer,
                        cpal::OutputStreamTimestamp {
                            callback: now,
                            playback: now,
                        },
                        &|sample| sample,
                    );
                }
                assert_eq!(buffer, [0.0; 2]);
            } else {
                shared.finish(outcome);
            }
            assert_eq!(shared.terminal(), outcome);
            resume.wait();
            writer.join().unwrap();
            assert_eq!(shared.terminal(), outcome);
        }
    }

    #[test]
    fn setup_failure_returns_its_local_diagnostic_while_async_writer_is_paused() {
        use crate::playback::output::{resolve, setup_failed};
        // Both synchronous build and play errors must bypass async filtering,
        // even if their error kind normally would be ignored asynchronously.
        for kind in [
            ErrorKind::Xrun,
            ErrorKind::RealtimeDenied,
            ErrorKind::DeviceBusy,
        ] {
            let shared = Arc::new(Shared::default());
            let claimed = Arc::new(Barrier::new(2));
            let resume = Arc::new(Barrier::new(2));
            let writer = {
                let shared = Arc::clone(&shared);
                let claimed = Arc::clone(&claimed);
                let resume = Arc::clone(&resume);
                std::thread::spawn(move || {
                    assert!(shared.claim_backend());
                    claimed.wait();
                    resume.wait();
                    shared.publish_backend(cpal::Error::new(ErrorKind::DeviceChanged));
                })
            };
            claimed.wait();
            let mut local = None;
            let winner = setup_failed(
                &shared,
                cpal::Error::with_message(kind, "original setup failure"),
                &mut local,
            );
            assert_eq!(winner, Terminal::StreamSetupFailed);
            let result = resolve(&shared, winner, local);
            assert!(
                matches!(result, Err(crate::playback::PlaybackError::Backend(error)) if error.kind() == kind && error.to_string() == "original setup failure")
            );
            // Resolution already returned before the async initializer resumes.
            resume.wait();
            writer.join().unwrap();
            assert_eq!(shared.terminal(), Terminal::StreamSetupFailed);
        }
    }

    #[test]
    fn setup_failure_resolves_every_previously_published_winner() {
        use crate::playback::output::{resolve, setup_failed};
        for outcome in OUTCOMES {
            if outcome == Terminal::StreamSetupFailed {
                continue;
            }
            let shared = Shared::default();
            if outcome == Terminal::Backend {
                shared.report(cpal::Error::with_message(
                    ErrorKind::DeviceChanged,
                    "asynchronous winner",
                ));
            } else {
                shared.finish(outcome);
            }
            let mut local = None;
            let winner = setup_failed(
                &shared,
                cpal::Error::with_message(ErrorKind::DeviceBusy, "losing setup failure"),
                &mut local,
            );
            assert_eq!(winner, outcome);
            let result = resolve(&shared, winner, local);
            match outcome {
                Terminal::Complete => assert!(result.is_ok()),
                Terminal::Backend => assert!(
                    matches!(result, Err(crate::playback::PlaybackError::Backend(error)) if error.to_string() == "asynchronous winner")
                ),
                Terminal::Cancelled => {
                    assert!(matches!(result, Err(crate::playback::PlaybackError::Cancelled)))
                }
                Terminal::TimedOut => {
                    assert!(matches!(result, Err(crate::playback::PlaybackError::TimedOut)))
                }
                Terminal::InvalidTiming => {
                    assert!(matches!(result, Err(crate::playback::PlaybackError::InvalidTiming)))
                }
                Terminal::MisalignedBuffer => assert!(matches!(
                    result,
                    Err(crate::playback::PlaybackError::MisalignedBuffer)
                )),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn backend_publication_makes_the_payload_visible_without_joining() {
        let shared = Arc::new(Shared::default());
        let writer = Arc::clone(&shared);
        let worker = std::thread::spawn(move || {
            writer.report(cpal::Error::with_message(
                ErrorKind::DeviceNotAvailable,
                "unplugged",
            ));
        });
        while shared.terminal() == Terminal::Running {
            std::thread::yield_now();
        }
        assert_eq!(shared.terminal(), Terminal::Backend);
        assert_eq!(shared.backend_error().to_string(), "unplugged");
        worker.join().unwrap();
    }

    #[test]
    fn underruns_and_priority_refusals_leave_the_slot_available_for_route_change() {
        let shared = Shared::default();
        shared.report(cpal::Error::new(ErrorKind::Xrun));
        shared.report(cpal::Error::new(ErrorKind::RealtimeDenied));
        shared.report(cpal::Error::new(ErrorKind::Xrun));
        assert_eq!(shared.underruns(), 2);
        assert_eq!(shared.terminal(), Terminal::Running);
        assert!(!shared.backend_claimed.load(Ordering::Relaxed));
        shared.report(cpal::Error::new(ErrorKind::DeviceChanged));
        assert_eq!(shared.terminal(), Terminal::Backend);
        assert_eq!(shared.backend_error().kind(), ErrorKind::DeviceChanged);
        shared.report(cpal::Error::new(ErrorKind::DeviceBusy));
        assert_eq!(shared.backend_error().kind(), ErrorKind::DeviceChanged);
    }

    #[test]
    fn progress_is_readable_from_another_thread() {
        let shared = Arc::new(Shared::default());
        let writer = Arc::clone(&shared);
        std::thread::spawn(move || {
            writer.mark_progress();
            writer.mark_progress();
        })
        .join()
        .unwrap();
        assert_eq!(shared.progress(), 2);
    }
}
