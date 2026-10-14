// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0

use crate::PlaybackError;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// What the audio callbacks and the thread that waits for them share.
///
/// The result is recorded once: the first of completion, failure,
/// cancellation and timeout stands, and whatever follows is ignored. Reading
/// it is nonblocking. Concurrent OnceLock::set attempts can wait while a
/// terminal result is being initialized. Callbacks return silence when they
/// observe that result; sound already queued to the backend cannot be recalled.
#[derive(Default)]
pub(crate) struct Shared {
    result: OnceLock<Result<(), PlaybackError>>,
    progress: AtomicU64,
    underruns: AtomicU64,
}

impl Shared {
    /// Record how playback ended, unless that is already recorded.
    pub(crate) fn finish(&self, result: Result<(), PlaybackError>) {
        // A result that arrives second is deliberately dropped.
        let _ = self.result.set(result);
    }

    pub(crate) fn result(&self) -> Option<&Result<(), PlaybackError>> {
        self.result.get()
    }

    /// Take a report from the backend's error callback: underruns are
    /// counted, a refused real-time priority is ignored, and anything else
    /// ends playback.
    pub(crate) fn report(&self, error: cpal::Error) {
        match error.kind() {
            cpal::ErrorKind::Xrun => {
                self.underruns.fetch_add(1, Ordering::Relaxed);
            }
            cpal::ErrorKind::RealtimeDenied => {}
            _ => self.finish(Err(PlaybackError::Backend(error))),
        }
    }

    pub(crate) fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }

    /// Note that the stream rendered session frames or drained further.
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

    fn recorded(shared: &Shared) -> String {
        match shared.result() {
            None => "running".into(),
            Some(Ok(())) => "complete".into(),
            Some(Err(error)) => error.to_string(),
        }
    }

    #[test]
    fn the_first_result_stands_whatever_follows() {
        let results = || {
            [
                Ok(()),
                Err(PlaybackError::Cancelled),
                Err(PlaybackError::MisalignedBuffer),
                Err(PlaybackError::InvalidTiming),
                Err(PlaybackError::TimedOut),
                Err(PlaybackError::Backend(cpal::Error::new(
                    ErrorKind::DeviceNotAvailable,
                ))),
            ]
        };
        for first in results() {
            let shared = Shared::default();
            assert_eq!(recorded(&shared), "running");
            let expected = match &first {
                Ok(()) => "complete".to_owned(),
                Err(error) => error.to_string(),
            };
            shared.finish(first);
            for later in results() {
                shared.finish(later);
                shared.report(cpal::Error::new(ErrorKind::StreamInvalidated));
                assert_eq!(recorded(&shared), expected);
            }
        }
    }

    #[test]
    fn underruns_are_counted_a_refused_priority_is_ignored_and_other_reports_end_playback() {
        let shared = Shared::default();
        shared.report(cpal::Error::new(ErrorKind::Xrun));
        shared.report(cpal::Error::new(ErrorKind::RealtimeDenied));
        shared.report(cpal::Error::new(ErrorKind::Xrun));
        assert_eq!(shared.underruns(), 2);
        assert_eq!(recorded(&shared), "running");
        shared.report(cpal::Error::with_message(
            ErrorKind::DeviceNotAvailable,
            "unplugged",
        ));
        assert_eq!(recorded(&shared), "audio backend: unplugged");
        assert_eq!(shared.underruns(), 2);
    }

    #[test]
    fn a_route_change_ends_playback() {
        let shared = Shared::default();
        shared.report(cpal::Error::new(ErrorKind::DeviceChanged));
        assert!(matches!(
            shared.result(),
            Some(Err(PlaybackError::Backend(error))) if error.kind() == ErrorKind::DeviceChanged
        ));
    }

    #[test]
    fn progress_is_readable_from_another_thread() {
        let shared = std::sync::Arc::new(Shared::default());
        let writer = std::sync::Arc::clone(&shared);
        std::thread::spawn(move || {
            writer.mark_progress();
            writer.mark_progress();
            writer.finish(Ok(()));
        })
        .join()
        .unwrap();
        assert_eq!(shared.progress(), 2);
        assert_eq!(recorded(&shared), "complete");
    }
}
