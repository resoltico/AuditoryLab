//! A native preference listener's late-bound event notification.
use std::sync::{Arc, Mutex};
type Callback = Arc<dyn Fn() + Send + Sync>;
#[derive(Clone, Default)]
pub(crate) struct PreferenceWake(Arc<Mutex<Option<Callback>>>);
impl PreferenceWake {
    pub(crate) fn bind(&self, wake: Arc<dyn Fn() + Send + Sync>) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(wake);
    }
    pub(crate) fn notify(&self) {
        let wake = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(wake) = wake {
            wake();
        }
    }
}
