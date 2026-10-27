//! Coalesced worker/preference notification of the owning Slint event loop.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
pub(super) struct Wake {
    main: slint::Weak<MainWindow>,
    pending: AtomicBool,
    active: AtomicBool,
}
impl Wake {
    pub(super) fn new(main: &MainWindow) -> Arc<Self> {
        Arc::new(Self {
            main: main.as_weak(),
            pending: AtomicBool::new(false),
            active: AtomicBool::new(true),
        })
    }
    pub(super) fn callback(self: &Arc<Self>) -> Arc<dyn Fn() + Send + Sync> {
        let wake = self.clone();
        Arc::new(move || wake.notify())
    }
    pub(super) fn notify(self: &Arc<Self>) {
        if !self.active.load(Ordering::Acquire) || self.pending.swap(true, Ordering::AcqRel) {
            return;
        }
        let wake = self.clone();
        if slint::invoke_from_event_loop(move || {
            // Clear before polling: a completion racing this poll must post a successor.
            wake.pending.store(false, Ordering::Release);
            if wake.active.load(Ordering::Acquire)
                && let Some(main) = wake.main.upgrade()
            {
                main.invoke_worker_ready();
            }
        })
        .is_err()
        {
            self.pending.store(false, Ordering::Release);
        }
    }
    pub(super) fn stop(&self) {
        self.active.store(false, Ordering::Release);
    }
}
