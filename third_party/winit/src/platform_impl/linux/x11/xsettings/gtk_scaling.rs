// SPDX-License-Identifier: Apache-2.0 OR MIT
//! GTK font DPI is independent of its resolved window factor.

pub(super) fn applies(desktop: &str) -> bool {
    !desktop
        .split(':')
        .any(|name| name.eq_ignore_ascii_case("KDE"))
}

pub(super) fn window_scale(integer: Option<i32>) -> Option<f64> {
    integer.filter(|scale| *scale > 0).map(f64::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_selection_preserves_kde_precedence() {
        for desktop in [
            "GNOME",
            "GNOME:GNOME-Flashback",
            "ubuntu:GNOME",
            "gnome",
            "XFCE",
            "LXQt",
            "",
        ] {
            assert!(applies(desktop), "{desktop}");
        }
        for desktop in ["KDE", "GNOME:KDE", "kde:gnome"] {
            assert!(!applies(desktop), "{desktop}");
        }
    }
    #[test]
    fn resolved_factors_keep_text_dpi_out_of_window_geometry() {
        assert_eq!(window_scale(Some(1)), Some(1.0));
        assert_eq!(window_scale(Some(2)), Some(2.0));
        // Auto/unavailable/wrong-type properties cannot supply a resolved factor.
        assert_eq!(window_scale(None), None);
        assert_eq!(window_scale(Some(0)), None);
        assert_eq!(window_scale(Some(-1)), None);
    }
}
