//! Read motion preferences without changing system settings.
/// Unknown preferences default to a frozen trace; rates/status remain available.
pub(crate) fn reduced() -> bool {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("defaults")
        .args(["read", "com.apple.universalaccess", "reduceMotion"])
        .output()
        .ok()
        .filter(|r| r.status.success())
        .and_then(|r| String::from_utf8(r.stdout).ok())
        .map(|s| s.trim() == "1");
    #[cfg(target_os = "windows")]
    let result = windows_reduced_motion();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .output()
        .ok()
        .filter(|r| r.status.success())
        .and_then(|r| String::from_utf8(r.stdout).ok())
        .map(|s| s.trim() == "false");
    result.unwrap_or(true)
}

#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "read-only Win32 preference query writes a single live, initialized BOOL"
)]
fn windows_reduced_motion() -> Option<bool> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };
    let mut enabled: windows_sys::core::BOOL = 1;
    // SAFETY: this GET action accepts a BOOL pointer, which is correctly aligned,
    // initialized, and exclusively borrowed for the duration of the synchronous call.
    let result = unsafe {
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut enabled).cast(), 0)
    };
    (result != 0).then_some(enabled == 0)
}
