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
        #[allow(
            clippy::cast_possible_truncation,
            reason = "native UI point size fits f32"
        )]
        let size = objc2_app_kit::NSFont::systemFontSize() as f32;
        (size, "AppKit system UI / fontique SystemUi".to_owned())
    };
    #[cfg(not(target_os = "macos"))]
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
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
#[cfg(any(target_os = "linux", target_os = "windows", test))]
impl TypographySink {
    fn send(&self, font: Typography) -> Result<(), ()> {
        if self.stop.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(());
        }
        *self.pending.lock().map_err(|_| ())? = Some(font);
        Ok(())
    }
}
/// One bounded listener, one coherent value. No audio callbacks or targets.
pub(super) struct TypographyService {
    pending: std::sync::Arc<std::sync::Mutex<Option<Typography>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl TypographyService {
    pub(super) fn start() -> Self {
        let pending = std::sync::Arc::new(std::sync::Mutex::new(None));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        let sender = TypographySink {
            pending: pending.clone(),
            stop: stop.clone(),
        };
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        let worker = {
            let stop = stop.clone();
            Some(std::thread::spawn(move || {
                #[cfg(target_os = "linux")]
                linux::listen(sender, stop);
                #[cfg(target_os = "windows")]
                windows_listener(sender, stop);
            }))
        };
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        let worker = None;
        Self {
            pending,
            stop,
            worker,
        }
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
#[allow(
    unsafe_code,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "bounded read-only Win32 metrics and WinRT text scale; initialized ABI structure and UTF16 font name"
)]
fn windows_listener(sender: TypographySink, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
    use windows_sys::Win32::UI::{
        HiDpi::SystemParametersInfoForDpi,
        WindowsAndMessaging::{NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS},
    };
    // SAFETY: this worker exclusively initializes and releases its own WinRT apartment.
    let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok();
    let settings = windows::UI::ViewManagement::UISettings::new().ok();
    let mut last = None;
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        // SAFETY: NONCLIENTMETRICSW is a plain initialized C record; GET fills exactly cbSize.
        let mut metrics: NONCLIENTMETRICSW = unsafe { std::mem::zeroed() };
        metrics.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
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

            let scaling = settings
                .as_ref()
                .and_then(|s| s.TextScaleFactor().ok())
                .filter(|s| s.is_finite() && *s > 0.0);
            let scale = scaling.unwrap_or(1.0);
            let size = (f64::from(metrics.lfMessageFont.lfHeight.unsigned_abs()) * scale) as f32;
            if !family.is_empty() && size.is_finite() && (1.0..=256.0).contains(&size) {
                let next = Typography {
                    family,
                    size,
                    origin: if scaling.is_some() {
                        "Win32 message font / WinRT text scaling"
                    } else {
                        "Win32 message font / fallback text scale 1: WinRT unavailable"
                    }
                    .into(),
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
    drop(settings);
    if initialized {
        unsafe { RoUninitialize() };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preference_updates_coalesce_and_shutdown_rejects_late_writes() {
        let pending = std::sync::Arc::new(std::sync::Mutex::new(None));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let service = TypographyService {
            pending: pending.clone(),
            stop: stop.clone(),
            worker: None,
        };
        let sink = TypographySink { pending, stop };
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
        drop(service);
        assert!(sink.send(latest).is_err());
    }
}
