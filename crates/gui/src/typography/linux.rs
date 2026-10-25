//! GNOME text scaling on Wayland; X11 font scaling belongs to the toolkit's DPI path.
use super::{Typography, TypographySink};
use std::{
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn parse_scale(text: &str) -> Option<f32> {
    text.trim()
        .parse::<f32>()
        .ok()
        .filter(|scale| scale.is_finite() && (0.5..=4.0).contains(scale))
}
/// Read the scale only; no native font-family or font-description parsing.
pub(super) fn requested_scale() -> Option<f32> {
    let mut child = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "text-scaling-factor"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child.wait_with_output().ok()?;
                return output
                    .status
                    .success()
                    .then(|| parse_scale(&String::from_utf8_lossy(&output.stdout)))
                    .flatten();
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}
pub(super) fn listen(sender: TypographySink, stop: Arc<AtomicBool>, base: Typography) {
    let mut last = None;
    while !stop.load(Ordering::Relaxed) {
        let scale = requested_scale();
        if last != Some(scale) {
            let mut font = base.clone();
            font.size *= scale.unwrap_or(1.0);
            font.origin = if scale.is_some() {
                "GNOME Wayland text scale / toolkit SystemUi"
            } else {
                "Toolkit SystemUi: GNOME text scale unavailable; install GLib gsettings"
            }
            .into();
            if sender.send(font).is_err() {
                return;
            }
            last = Some(scale);
        }
        for _ in 0..20 {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_scale_requires_one_bounded_finite_number() {
        for value in ["0.5", "1.0", "1.5", "4.0"] {
            assert!(parse_scale(value).is_some(), "{value}");
        }
        for value in [
            "",
            "NaN",
            "inf",
            "0",
            "0.499",
            "4.01",
            "1.5 2",
            "'DejaVu Sans 12'",
        ] {
            assert!(parse_scale(value).is_none(), "{value}");
        }
    }
}
