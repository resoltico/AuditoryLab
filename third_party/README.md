# Native typography and window scaling

The workspace patches pinned i-slint-core 1.18.1, Parley 0.11.1 and Winit 0.30.13 sources for native font and window scaling. Their original manifests and licenses remain alongside the source. No font files are embedded.

- i-slint-core supplies the requested logical font size as the optical-size variation for shaping, font metrics and character measurements. Inline code uses its own scaled text size. Glyph drawing receives the shaped run’s normalized variation coordinates.
- Parley uses that logical optical-size request for Apple tracking, so changing the display backing scale does not change letter spacing. Callers without an explicit optical-size request retain Parley’s existing policy.
- Windows and Linux retain the toolkit’s existing variation policy.

The GUI’s action-wiring test compares real Slint font metrics and 100-glyph advances against AppKit at 13, 26 and 40 points with 1× and 2× scale factors. Run `cargo test -p auditorylab-gui corrected_pending_frequency --locked -- --nocapture` on macOS. These patches need to remain until the corresponding toolkit paths preserve logical optical sizing and tracking.

The pinned Winit 0.30.13 X11 backend uses GTK's positive resolved `Gdk/WindowScalingFactor` for window DPI. Font adapters apply independent text/font DPI scaling. KDE precedence, Wayland and explicit Winit/Slint scale overrides retain their existing paths. Missing, malformed or nonpositive resolved factors retain the toolkit's Xft/RandR fallback. The exact desktop-selection/factor policy source is included in xtask unit-test discovery; native qualification exercises real XSettings changes and final rendered scaling.
