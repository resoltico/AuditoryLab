# Native desktop integration

The workspace patches pinned i-slint-core and i-slint-backend-winit 1.18.1, Parley 0.11.1, Winit 0.30.13 and AccessKit Unix 0.22.1 sources for native typography, window scaling, modeless window parenting, keyboard input and accessibility. Their original manifests and licenses remain alongside the source. No font files are embedded.

- i-slint-core supplies the requested logical font size as the optical-size variation for shaping, font metrics and character measurements. Inline code uses its own scaled text size. Glyph drawing receives the shaped run’s normalized variation coordinates.
- Parley uses that logical optical-size request for Apple tracking, so changing the display backing scale does not change letter spacing. Callers without an explicit optical-size request retain Parley’s existing policy.
- Windows and Linux retain the toolkit’s existing variation policy.

The GUI’s action-wiring test compares real Slint font metrics and 100-glyph advances against AppKit at 13, 26 and 40 points with 1× and 2× scale factors. Run `cargo test -p auditorylab-gui corrected_pending_frequency --locked -- --nocapture` on macOS. These patches need to remain until the corresponding toolkit paths preserve logical optical sizing and tracking.

The pinned Winit 0.30.13 X11 backend uses GTK's positive resolved `Gdk/WindowScalingFactor` for window DPI. Font adapters apply independent text/font DPI scaling. KDE precedence, Wayland and explicit Winit/Slint scale overrides retain their existing paths. Missing, malformed or nonpositive resolved factors retain the toolkit's Xft/RandR fallback. The exact desktop-selection/factor policy source is included in xtask unit-test discovery; native qualification exercises real XSettings changes and final rendered scaling.

The Linux AccessKit bridge sends `org.a11y.atspi.Cache.AddAccessible` and `RemoveAccessible` with one structured D-Bus argument, as required by the [AT-SPI cache interface](https://github.com/GNOME/at-spi2-core/blob/main/xml/Cache.xml). These signals let assistive clients refresh controls that leave and re-enter the accessible tree, including scrolled fields and popup choices. Node identities, filtering and object lifetimes retain their existing policy.

On Linux, run `cargo test -p accesskit_unix --lib --locked` for the bridge tests. The cache-signal tests inspect the production signal argument builder's serialized header signatures and decode the single struct arguments.

The Winit Wayland extension exposes SCTK's typed modeless parent request. Details windows use their invoking window as parent and are reparented to the main window before a focused monitor is hidden. A weak binding reapplies the parent before the first frame when Slint recreates a hidden Details window. The request preserves normal compositor policy and does not make the dialog modal or globally topmost. The compositor controls keyboard activation.

The Winit backend requests a future redraw when showing a Wayland window. An initial redraw can arrive while a precreated window is hidden, and Wayland's native show operation does not request another frame. Drawing still waits for the initial configure; the backend does not pre-render Wayland surfaces.

On macOS, the Winit backend synchronizes Slint's logical keyboard modifiers from native aggregate flag reports, using the same Command/Control normalization as key events. Native aggregate flags remain separate from known physical sides, so flagged accessibility input does not require fabricated modifier-key presses. Ordinary input uses the latest native aggregate; actual modifier-key callbacks retain their immediate physical-family state. Losing window focus clears both sources. Other backends retain physical-key tracking and their existing AltGr handling.

The GUI's existing action-wiring test exercises real text selection from aggregate-only flags, both physical/canonical event orders, side-key release, focus reset and Windows AltGr clearing. It runs with `cargo test -p auditorylab-gui corrected_pending_frequency --locked`.
