//! Read-only native motion preferences, coalesced off UI and audio processing.
#[cfg(target_os = "linux")]
mod gnome;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Preference {
    Reduced,
    Full,
    Unknown,
}
impl Preference {
    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Reduced => "System Reduce Motion is enabled; display held by default.",
            Self::Full => "System motion preference permits live drawing.",
            Self::Unknown => "System motion preference unavailable; using live drawing fallback.",
        }
    }
}
pub(crate) struct Service {
    pending: Arc<Mutex<Option<Preference>>>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl Service {
    pub(crate) fn start() -> Self {
        let pending = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let target = pending.clone();
        let stopped = stop.clone();
        let handle = std::thread::spawn(move || {
            #[cfg(target_os = "linux")]
            if desktop_has("GNOME") && !desktop_has("KDE") && gnome::listen(&target, &stopped) {
                return;
            }
            let mut last = None;
            while !stopped.load(Ordering::Relaxed) {
                let next = query();
                if last != Some(next) {
                    *target
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(next);
                    last = Some(next);
                }
                for _ in 0..10 {
                    if stopped.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        });
        Self {
            pending,
            stop,
            handle: Some(handle),
        }
    }
    pub(crate) fn latest(&self) -> Option<Preference> {
        self.pending.try_lock().ok()?.take()
    }
    pub(crate) fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn from_reduced(reduced: Option<bool>) -> Preference {
    match reduced {
        Some(true) => Preference::Reduced,
        Some(false) => Preference::Full,
        None => Preference::Unknown,
    }
}
#[cfg(target_os = "macos")]
fn query() -> Preference {
    from_reduced(Some(
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion(),
    ))
}
#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "read-only Win32 GET fills one initialized, exclusively borrowed BOOL"
)]
fn query() -> Preference {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };
    let mut enabled: windows_sys::core::BOOL = 1;
    // SAFETY: the documented GET expects a writable BOOL, live for this call.
    let result = unsafe {
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut enabled).cast(), 0)
    };
    from_reduced((result != 0).then_some(enabled == 0))
}
#[cfg(target_os = "linux")]
fn query() -> Preference {
    if desktop_has("KDE") {
        let reduced = async_io::block_on(futures_lite::future::race(
            async {
                let connection = zbus::connection::Builder::session()
                    .ok()?
                    .method_timeout(std::time::Duration::from_secs(2))
                    .build()
                    .await
                    .ok()?;
                let proxy = zbus::Proxy::new(
                    &connection,
                    "org.freedesktop.portal.Desktop",
                    "/org/freedesktop/portal/desktop",
                    "org.freedesktop.portal.Settings",
                )
                .await
                .ok()?;
                let result: zbus::Result<zbus::zvariant::OwnedValue> = proxy
                    .call(
                        "ReadOne",
                        &("org.kde.kdeglobals.KDE", "AnimationDurationFactor"),
                    )
                    .await;
                let value = match result {
                    Ok(v) => v,
                    Err(_) => proxy
                        .call::<_, _, zbus::zvariant::OwnedValue>(
                            "Read",
                            &("org.kde.kdeglobals.KDE", "AnimationDurationFactor"),
                        )
                        .await
                        .ok()?,
                };
                let duration = f64::try_from(value.try_clone().ok()?)
                    .ok()
                    .or_else(|| String::try_from(value).ok()?.parse().ok())?;
                (duration.is_finite() && duration >= 0.0).then_some(duration == 0.0)
            },
            async {
                async_io::Timer::after(std::time::Duration::from_secs(2)).await;
                None
            },
        ));
        return from_reduced(reduced);
    }
    Preference::Unknown
}
#[cfg(target_os = "linux")]
fn desktop_has(name: &str) -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .any(|value| value.eq_ignore_ascii_case(name))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_is_not_a_confirmed_reduced_motion_preference() {
        assert_eq!(from_reduced(None), Preference::Unknown);
        assert_eq!(from_reduced(Some(false)), Preference::Full);
        assert_eq!(from_reduced(Some(true)), Preference::Reduced);
    }
}
