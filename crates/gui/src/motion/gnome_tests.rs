//! Native motion qualification for an explicitly isolated GNOME session.
use super::*;
use std::process::Command;
fn set(value: &str) -> bool {
    Command::new("gsettings")
        .args([
            "set",
            "org.gnome.desktop.interface",
            "enable-animations",
            value,
        ])
        .status()
        .is_ok_and(|status| status.success())
}
struct Restore(String);
impl Drop for Restore {
    fn drop(&mut self) {
        if !set(&self.0) {
            eprintln!("Failed to restore isolated GNOME enable-animations");
        }
    }
}
#[test]
#[ignore = "requires an explicitly isolated GNOME D-Bus session; changes its motion setting"]
fn native_motion_changes_reach_the_running_service() {
    assert_eq!(
        std::env::var("AUDITORYLAB_MOTION_TEST_SESSION").as_deref(),
        Ok("isolated"),
        "run only in a disposable desktop session"
    );
    assert!(desktop_has("GNOME") && !desktop_has("KDE"));
    let original = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .output()
        .unwrap();
    assert!(original.status.success());
    let original = String::from_utf8(original.stdout)
        .unwrap()
        .trim()
        .to_owned();
    assert!(matches!(original.as_str(), "true" | "false"));
    let restore = Restore(original);
    let mut service = Service::start();
    for (setting, expected) in [
        ("true", Preference::Full),
        ("false", Preference::Reduced),
        ("true", Preference::Full),
    ] {
        assert!(set(setting));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if service.latest() == Some(expected) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "native motion change to {expected:?} was not delivered"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        eprintln!("Native GNOME enable-animations={setting} delivered {expected:?}");
    }
    let start = std::time::Instant::now();
    service.shutdown();
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    assert!(set(&restore.0));
}
