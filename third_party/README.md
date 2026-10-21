# Desktop toolkit integration

The workspace patches pinned i-slint-core, i-slint-compiler and i-slint-backend-winit 1.18.1, Parley 0.11.1, Winit 0.30.13 and AccessKit Unix 0.22.1 sources for native typography, window scaling, modeless window parenting, keyboard input, accessibility and multiline text scrolling. Their original manifests and licenses remain alongside the source. No font files are embedded.

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

The shared TextEdit widget scrolls the complete shaped cursor rectangle into view. The core publishes its actual height before the existing cursor-position callback; the widget uses that height for both viewport edges and short viewports instead of a fixed 20px margin. Font leading and fallback shaping remain the renderer’s responsibility.

Multiline TextInput preferred height includes every shaped line’s full vertical extent, so fallback fonts that extend beyond the nominal line advance remain reachable by scrolling. Other text measurement retains its existing policy.

Keyboard focus reveals the owning semantic control when its internal focus scope has zero width or height. This keeps the whole button, checkbox or other leaf control reachable without enlarging its pointer target. Owner selection stops at popup, scroll-container and window boundaries; ordinary positive-sized focus retains its existing geometry. On an axis where the owner exceeds the visible viewport, scrolling retains the original focus extent instead of chasing an edge that cannot fit.

Input focus survives clipping by a scroll container, including pointer movement after typography reflow. Each ancestor clip is evaluated in its own geometry. Uncaptured nonmodifier key presses reveal a clipped focused control before delivery; pointer movement and captured shortcuts preserve the viewport. Hidden or detached controls lose focus. Dynamic components must match their parent’s active subtree, and a component with no surviving parent must be its existing window’s canonical root. These checks use the existing component and window identities without retaining removed delegates.
