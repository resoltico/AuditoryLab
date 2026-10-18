// Copyright 2026 Ervins Strauhmanis
// SPDX-License-Identifier: MPL-2.0
//! Bounded display state over application-owned signal facts.
use auditorylab::{ObservationError, SignalObservation, SignalProgress, SignalSnapshot};
use std::fmt;
use std::{sync::Arc, time::Duration};

const DISPLAY_BINS: usize = 512;

pub(crate) enum DisplayError {
    Observation(ObservationError),
    Rendering,
}
impl fmt::Display for DisplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Observation(error) => write!(
                f,
                "Signal observation unavailable: {error}. Playback result remains separate."
            ),
            Self::Rendering => fmt::Display::fmt(&crate::signal_geometry::CapacityError, f),
        }
    }
}

pub(crate) struct SignalDisplay {
    observation: Option<SignalObservation>,
    pub(crate) snapshot: Option<Arc<SignalSnapshot>>,
    pub(crate) progress: Option<SignalProgress>,
    pub(crate) frozen: bool,
    pub(crate) reduced_motion: bool,
    pub(crate) error: Option<DisplayError>,
    eligible: bool,
    suspended: bool,
    refresh_pending: bool,
    detail_window: Duration,
    ended: bool,
}
impl Default for SignalDisplay {
    fn default() -> Self {
        Self {
            observation: None,
            snapshot: None,
            progress: None,
            frozen: false,
            reduced_motion: false,
            error: None,
            eligible: true,
            suspended: false,
            refresh_pending: false,
            detail_window: Duration::from_millis(40),
            ended: false,
        }
    }
}
impl SignalDisplay {
    pub(crate) fn clear(&mut self) {
        let reduced_motion = self.reduced_motion;
        let detail_window = self.detail_window;
        let eligible = self.eligible;
        *self = Self {
            reduced_motion,
            detail_window,
            eligible,
            ..Self::default()
        };
    }
    pub(crate) fn attach(&mut self, observation: SignalObservation) {
        self.clear();
        self.observation = Some(observation);
        // Reduced motion captures one real snapshot, then remains static.
        self.refresh_pending = self.reduced_motion;
        self.adjust_suspension();
    }
    pub(crate) fn available(&self) -> bool {
        self.observation.is_some() && !self.ended
    }
    pub(crate) fn eligible(&self) -> bool {
        self.eligible
    }
    pub(crate) fn ended(&self) -> bool {
        self.ended
    }
    pub(crate) fn toggle_freeze(&mut self) {
        self.frozen = !self.frozen;
        self.refresh_pending = false;
        self.adjust_suspension();
    }
    pub(crate) fn set_reduced_motion(&mut self, reduced: bool) {
        self.reduced_motion = reduced;
        self.refresh_pending = reduced && self.snapshot.is_none();
        self.adjust_suspension();
    }
    pub(crate) fn refresh(&mut self) {
        if self.frozen || self.reduced_motion {
            self.refresh_pending = true;
            self.adjust_suspension();
        }
    }
    pub(crate) fn detail_window(&mut self, index: i32) {
        if let Some(milliseconds) =
            [2, 10, 40, 100].get(usize::try_from(index).unwrap_or(usize::MAX))
        {
            self.detail_window = Duration::from_millis(*milliseconds);
            // A frozen view changes only after an explicit refresh.
            if !self.frozen && !self.reduced_motion {
                self.capture();
            }
        }
    }
    fn adjust_suspension(&mut self) {
        let hold = self.error.is_some()
            || !self.eligible
            || ((self.frozen || self.reduced_motion) && !self.refresh_pending);
        if hold != self.suspended {
            if let Some(observation) = &mut self.observation {
                if hold {
                    observation.suspend();
                } else {
                    observation.resume();
                }
            }
            self.suspended = hold;
        }
    }
    pub(crate) fn poll(&mut self, eligible: bool) {
        self.eligible = eligible;
        self.adjust_suspension();
        let Some(observation) = &mut self.observation else {
            return;
        };
        let progress = observation.poll();
        let new_coverage = progress.observed_through().is_some()
            && self.snapshot.as_ref().is_none_or(|snapshot| {
                snapshot.progress().observed_through() != progress.observed_through()
            });
        self.progress = Some(progress);
        if !self.suspended && new_coverage {
            self.capture();
            if self.refresh_pending && self.snapshot.is_some() {
                self.refresh_pending = false;
                self.adjust_suspension();
            }
        }
    }
    pub(crate) fn finish(&mut self) {
        self.poll(self.eligible);
        self.ended = true;
    }
    pub(crate) fn rendering_failed(&mut self) {
        self.snapshot = None;
        self.error = Some(DisplayError::Rendering);
        self.adjust_suspension();
    }
    fn capture(&mut self) {
        if self.suspended || !self.eligible || self.ended {
            return;
        }
        let Some(observation) = &mut self.observation else {
            return;
        };
        match observation.snapshot(self.detail_window, DISPLAY_BINS) {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                self.error = None;
            }
            Err(error) => {
                self.snapshot = None;
                self.error = Some(DisplayError::Observation(error));
                self.adjust_suspension();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendering_failure_suspends_only_display_until_a_new_source() {
        let mut display = SignalDisplay::default();
        display.rendering_failed();
        assert!(display.error.is_some());
        assert!(display.snapshot.is_none());
        assert!(display.suspended);
        display.refresh();
        display.poll(true);
        assert!(display.suspended);
        display.clear();
        assert!(display.error.is_none());
        assert!(!display.suspended);
    }
    #[test]
    fn source_clear_preserves_independent_window_activity_and_preferences() {
        let mut display = SignalDisplay::default();
        display.set_reduced_motion(true);
        display.detail_window(3);
        display.poll(false);
        display.clear();
        assert!(!display.eligible());
        assert!(display.reduced_motion);
        assert_eq!(display.detail_window, Duration::from_millis(100));
        assert!(
            !display.suspended,
            "a new handle still needs its own suspend transition"
        );
        assert!(!display.ended());
        assert!(display.snapshot.is_none());
    }
}
