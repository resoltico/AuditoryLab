//! Native UI typography, read off the audio/UI hot path and applied as one value.
#[cfg(target_os = "linux")]
mod linux;
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Typography {
    pub(super) family: String,
    pub(super) size: f32,
    pub(super) origin: String,
}
fn system_family(fonts: &mut fontique::Collection) -> String {
    let id = fonts
        .generic_families(fontique::GenericFamily::SystemUi)
        .next();
    id.and_then(|id| fonts.family_name(id))
        .unwrap_or("system-ui")
        .to_owned()
}
pub(super) fn resolve() -> Typography {
    let mut fonts = fontique::Collection::new(fontique::CollectionOptions::default());
    #[cfg(target_os = "macos")]
    let (size, origin) = {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "AppKit systemFontSize returns the platform UI point size; rounding binary64 to the renderer f32 coordinate type is intentional"
        )]
        let size = objc2_app_kit::NSFont::systemFontSize() as f32;
        (size, "AppKit system UI / fontique SystemUi".to_owned())
    };
    #[cfg(target_os = "linux")]
    let (size, origin) = (13.0, "Toolkit DPI / fontique SystemUi".to_owned());
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let (size, origin) = (
        13.0,
        "SystemUi fallback while reading desktop preference".to_owned(),
    );
    Typography {
        family: system_family(&mut fonts),
        size,
        origin,
    }
}
/// One latest-value slot coalesces preference changes even while the UI is busy.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
#[derive(Clone)]
pub(super) struct TypographySink {
    pending: std::sync::Arc<std::sync::Mutex<Option<Typography>>>,
    wake: crate::preference_wake::PreferenceWake,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
#[cfg(any(target_os = "linux", target_os = "windows", test))]
impl TypographySink {
    fn send(&self, font: Typography) -> Result<(), ()> {
        if self.stop.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(());
        }
        *self.pending.lock().map_err(|_| ())? = Some(font);
        self.wake.notify();
        Ok(())
    }
}
/// One bounded listener, one coherent value. No audio callbacks or targets.
pub(super) struct TypographyService {
    pending: std::sync::Arc<std::sync::Mutex<Option<Typography>>>,
    wake: crate::preference_wake::PreferenceWake,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    #[cfg(target_os = "linux")]
    backend_ready: bool,
}
impl TypographyService {
    pub(super) fn start() -> Self {
        let pending = std::sync::Arc::new(std::sync::Mutex::new(None));
        let wake = crate::preference_wake::PreferenceWake::default();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        #[cfg(target_os = "windows")]
        let sender = TypographySink {
            pending: pending.clone(),
            wake: wake.clone(),
            stop: stop.clone(),
        };
        #[cfg(target_os = "windows")]
        let worker = {
            let stop = stop.clone();
            Some(std::thread::spawn(move || {
                #[cfg(target_os = "windows")]
                windows_listener(sender, stop);
            }))
        };
        #[cfg(not(target_os = "windows"))]
        let worker = None;
        Self {
            pending,
            wake,
            stop,
            worker,
            #[cfg(target_os = "linux")]
            backend_ready: false,
        }
    }
    #[cfg(target_os = "linux")]
    pub(super) fn bind_window(&mut self, window: &slint::Window, base: &Typography) {
        use slint::winit_030::{WinitWindowAccessor, winit::platform::wayland::WindowExtWayland};
        if self.backend_ready {
            return;
        }
        let Some(wayland) = window.with_winit_window(|window| window.xdg_toplevel().is_some())
        else {
            return;
        };
        self.backend_ready = true;
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let gnome = desktop.split(':').any(|s| s.eq_ignore_ascii_case("GNOME"));
        let kde = desktop.split(':').any(|s| s.eq_ignore_ascii_case("KDE"));
        if wayland && gnome && !kde {
            let sender = TypographySink {
                pending: self.pending.clone(),
                wake: self.wake.clone(),
                stop: self.stop.clone(),
            };
            let stop = self.stop.clone();
            let base = base.clone();
            self.worker = Some(std::thread::spawn(move || {
                linux::listen(sender, stop, base)
            }));
        }
    }
    pub(super) fn set_waker(&self, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
        self.wake.bind(wake);
    }
    pub(super) fn latest(&self) -> Option<Typography> {
        self.pending.try_lock().ok()?.take()
    }
}
impl TypographyService {
    pub(super) fn shutdown(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            // Never wait on a platform C call on the UI thread. Cooperative reads
            // have a stop flag/timeouts; process shutdown also terminates native listeners.
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}
impl Drop for TypographyService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(target_os = "windows")]
#[expect(
    unsafe_code,
    clippy::cast_possible_truncation,
    reason = "read-only Win32 metrics at 96 DPI use an initialized ABI structure with a fixed-size u32 length and UTF16 font name"
)]
fn windows_listener(sender: TypographySink, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use windows_sys::Win32::UI::{
        HiDpi::SystemParametersInfoForDpi,
        WindowsAndMessaging::{NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS},
    };
    let mut last = None;
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        // SAFETY: NONCLIENTMETRICSW is a plain initialized C record; GET fills exactly cbSize.
        let mut metrics: NONCLIENTMETRICSW = unsafe { std::mem::zeroed() };
        metrics.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
        // SAFETY: the DPI-aware GET writes the initialized NONCLIENTMETRICSW record above.
        if unsafe {
            SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS,
                metrics.cbSize,
                (&raw mut metrics).cast(),
                0,
                96,
            )
        } != 0
        {
            let face = &metrics.lfMessageFont.lfFaceName;
            let end = face.iter().position(|c| *c == 0).unwrap_or(face.len());
            let family = String::from_utf16_lossy(&face[..end]);

            // Native message metrics include the system text-size preference.
            // Request 96-DPI logical units; the window applies display scaling.
            if let Ok(height) = u16::try_from(metrics.lfMessageFont.lfHeight.unsigned_abs())
                && (1..=256).contains(&height)
                && !family.is_empty()
            {
                let size = f32::from(height);
                let next = Typography {
                    family,
                    size,
                    origin: "Win32 message font at 96 DPI".into(),
                };
                if last.as_ref() != Some(&next) {
                    last = Some(next.clone());
                    if sender.send(next).is_err() {
                        break;
                    }
                }
            }
        }
        for _ in 0..20 {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preference_updates_coalesce_and_shutdown_rejects_late_writes() {
        let pending = std::sync::Arc::new(std::sync::Mutex::new(None));
        let wake = crate::preference_wake::PreferenceWake::default();
        let notifications = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let count = notifications.clone();
        wake.bind(std::sync::Arc::new(move || {
            count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let service = TypographyService {
            pending: pending.clone(),
            wake: wake.clone(),
            stop: stop.clone(),
            worker: None,
            #[cfg(target_os = "linux")]
            backend_ready: false,
        };
        let sink = TypographySink {
            pending,
            stop,
            wake,
        };
        sink.send(Typography {
            family: "First".into(),
            size: 13.0,
            origin: "fixture1".into(),
        })
        .unwrap();
        sink.send(Typography {
            family: "Newest".into(),
            size: 20.0,
            origin: "fixture2".into(),
        })
        .unwrap();
        let latest = service.latest().unwrap();
        assert_eq!(latest.family, "Newest");
        assert_eq!(latest.size, 20.0);
        assert!(service.latest().is_none());
        assert_eq!(notifications.load(std::sync::atomic::Ordering::Relaxed), 2);
        drop(service);
        assert!(sink.send(latest).is_err());
        assert_eq!(notifications.load(std::sync::atomic::Ordering::Relaxed), 2);
    }
}
