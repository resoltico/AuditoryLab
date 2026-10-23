# Desktop toolkit integration

The workspace patches pinned i-slint-core, i-slint-compiler and i-slint-backend-winit 1.18.1, Parley 0.11.1, Winit 0.30.13 and AccessKit Unix 0.22.1 sources for native typography, window scaling, modeless window parenting, keyboard input, accessibility, multiline text scrolling, and macOS application termination and About routing. Their original manifests and licenses remain alongside the source. No font files are embedded.

## Sources

Each directory is the published crates.io archive with the listed files changed. The SHA-256 values are those of the archives and match the crates.io index.

| Directory | Upstream archive | SHA-256 | Changed files |
| --- | --- | --- | --- |
| `accesskit_unix` | [accesskit_unix 0.22.1](https://static.crates.io/crates/accesskit_unix/accesskit_unix-0.22.1.crate) | `03e156ed3802e35eefe894ef2671bc6c889303d8a7e110b5e1b48f504b91362f` | `src/atspi/bus.rs`; adds `LICENSE-APACHE`, `LICENSE-MIT` |
| `i-slint-core` | [i-slint-core 1.18.1](https://static.crates.io/crates/i-slint-core/i-slint-core-1.18.1.crate) | `ed2ea15059d70bc31b1a8b1eb4c34f2c5df04cb7e53f36179b43c0261231d646` | `input.rs`, `item_tree.rs`, `items/flickable.rs`, `items/text.rs`, `textlayout/sharedparley.rs`, `textlayout/sharedparley/shaping.rs`, `window.rs` |
| `i-slint-compiler` | [i-slint-compiler 1.18.1](https://static.crates.io/crates/i-slint-compiler/i-slint-compiler-1.18.1.crate) | `f55d48ebef05e2bf09ba5a1fc67bcb11033b9e12a2a50ed3105c7d25e2b539d0` | `builtin_elements.rs`, `widgets/common/textedit-base.slint`, `widgets/{cosmic,cupertino,fluent,material,qt}/button.slint` |
| `i-slint-backend-winit` | [i-slint-backend-winit 1.18.1](https://static.crates.io/crates/i-slint-backend-winit/i-slint-backend-winit-1.18.1.crate) | `dae4fcf1b19e9cb109984d2af87d2b9485388c619265bff71dd523133888bee2` | `event_loop.rs`, `lib.rs`, `muda.rs`, `winitwindowadapter.rs`; omits the archive's `Cargo.lock` |
| `parley` | [parley 0.11.1](https://static.crates.io/crates/parley/parley-0.11.1.crate) | `22d2ff88bd3f7d68d1d9b09c7e6209f9a8e8c05088295140a2bcf2e9b17038c5` | `src/shape/mod.rs` |
| `winit` | [winit 0.30.13](https://static.crates.io/crates/winit/winit-0.30.13.crate) | `a6755fa58a9f8350bd1e472d4c3fcc25f824ec358933bba33306d0b63df5978d` | `src/platform/macos.rs`, `src/platform/wayland.rs`, `src/platform_impl/linux/wayland/window/mod.rs`, `src/platform_impl/linux/x11/util/randr.rs`, `src/platform_impl/linux/x11/xsettings.rs` and the new `xsettings/` directory, `src/platform_impl/macos/app_state.rs`, `src/platform_impl/macos/event_loop.rs`, `src/platform_impl/macos/mod.rs` |

The directories here are the only maintained copy of the changes. To review one, compare it with its verified archive; every archive also contains `.cargo_vcs_info.json`, which is not kept:

```sh
mkdir -p target/vendor-review
curl -sSfL -o target/vendor-review/winit-0.30.13.crate https://static.crates.io/crates/winit/winit-0.30.13.crate
shasum -a 256 target/vendor-review/winit-0.30.13.crate
tar -xzf target/vendor-review/winit-0.30.13.crate -C target/vendor-review
diff -ruN target/vendor-review/winit-0.30.13 third_party/winit
```

The root manifest excludes `third_party` from the workspace so that a crate whose tests need its own development dependencies runs them from its manifest and lockfile.

## Changes

- i-slint-core supplies the requested logical font size as the optical-size variation for shaping, font metrics and character measurements. Inline code uses its own scaled text size. Glyph drawing receives the shaped run’s normalized variation coordinates.
- Parley uses that logical optical-size request for Apple tracking, so changing the display backing scale does not change letter spacing. Callers without an explicit optical-size request retain Parley’s existing policy.
- Windows and Linux retain the toolkit’s existing variation policy.

The GUI test `slint_font_metrics_match_appkit` compares real Slint font metrics and 100-glyph advances against AppKit at 13, 26 and 40 points with 1× and 2× scale factors. Run `cargo test -p auditorylab-gui slint_font_metrics_match_appkit --locked -- --nocapture` on macOS. These patches need to remain until the corresponding toolkit paths preserve logical optical sizing and tracking.

The pinned Winit 0.30.13 X11 backend uses GTK's positive resolved `Gdk/WindowScalingFactor` for window DPI. Font adapters apply independent text/font DPI scaling. KDE precedence, Wayland and explicit Winit/Slint scale overrides retain their existing paths. Missing, malformed or nonpositive resolved factors retain the toolkit's Xft/RandR fallback. The exact desktop-selection/factor policy source is included in xtask unit-test discovery; native checks must exercise actual XSettings changes and rendered scaling.

The Linux AccessKit bridge sends `org.a11y.atspi.Cache.AddAccessible` and `RemoveAccessible` with one structured D-Bus argument, as required by the [AT-SPI cache interface](https://github.com/GNOME/at-spi2-core/blob/main/xml/Cache.xml). These signals let assistive clients refresh controls that leave and re-enter the accessible tree, including scrolled fields and popup choices. Node identities, filtering and object lifetimes retain their existing policy.

On Linux, run `cargo test -p accesskit_unix --lib --locked` for the bridge tests. The cache-signal tests inspect the production signal argument builder's serialized header signatures and decode the single struct arguments.

The Winit Wayland extension exposes SCTK's typed modeless parent request. Details windows use their invoking window as parent and are reparented to the main window before a focused monitor is hidden. A weak binding reapplies the parent before the first frame when Slint recreates a hidden Details window. The request preserves normal compositor policy and does not make the dialog modal or globally topmost. The compositor controls keyboard activation.

The Winit backend requests a future redraw when showing a Wayland window. An initial redraw can arrive while a precreated window is hidden, and Wayland's native show operation does not request another frame. Drawing still waits for the initial configure; the backend does not pre-render Wayland surfaces.

On macOS, the Winit backend synchronizes Slint's logical keyboard modifiers from native aggregate flag reports, using the same Command/Control normalization as key events. Native aggregate flags remain separate from known physical sides, so flagged accessibility input does not require fabricated modifier-key presses. Ordinary input uses the latest native aggregate; actual modifier-key callbacks retain their immediate physical-family state. Losing window focus clears both sources. Other backends retain physical-key tracking and their existing AltGr handling.

The GUI's action-wiring test exercises real text selection from aggregate-only flags, both physical/canonical event orders, side-key release, focus reset and Windows AltGr clearing. It runs with `cargo test -p auditorylab-gui corrected_pending_frequency --locked`.

The shared TextEdit widget scrolls the complete shaped cursor rectangle into view. The core publishes its actual height before the existing cursor-position callback; the widget uses that height for both viewport edges and short viewports instead of a fixed 20px margin. Font leading and fallback shaping remain the renderer’s responsibility.

The GUI test `text_edit_cursor_scrolls_into_view` exercises it.

Multiline TextInput preferred height includes every shaped line’s full vertical extent, so fallback fonts that extend beyond the nominal line advance remain reachable by scrolling. Other text measurement retains its existing policy.

Keyboard focus reveals the owning semantic control when its internal focus scope has zero width or height. This keeps the whole button, checkbox or other leaf control reachable without enlarging its pointer target. Owner selection stops at popup, scroll-container and window boundaries; ordinary positive-sized focus retains its existing geometry. On an axis where the owner exceeds the visible viewport, scrolling retains the original focus extent instead of chasing an edge that cannot fit. The GUI test `control_focus_scrolls_into_view` exercises it.

Input focus survives clipping by a scroll container, including pointer movement after typography reflow. Each ancestor clip is evaluated in its own geometry. Uncaptured nonmodifier key presses reveal a clipped focused control before delivery; pointer movement and captured shortcuts preserve the viewport. Hidden or detached controls lose focus. Dynamic components must match their parent’s active subtree, and a component with no surviving parent must be its existing window’s canonical root. These checks use the existing component and window identities without retaining removed delegates. The GUI test `pending_entry_survives_font_reflow_clipping_and_row_changes` exercises these paths through the production Workbench bindings.

On macOS, Winit’s own application delegate implements `applicationShouldTerminate:`. With an interceptor registered through `EventLoopExtMacOS::set_termination_interceptor`, it answers “terminate later”, owns the pending request under an epoch that is never reused, and hands the interceptor an opaque `TerminationRequest`. Repeated Quit joins the pending request. A request the interceptor cannot take is denied, and so is one still pending when the event loop exits. `reply_to_termination` answers only on the main thread, returns the request unchanged when called from another thread, and refuses a request that is no longer pending. Without an interceptor, termination proceeds as AppKit’s default. The delegate object itself is not replaced. Run `CARGO_TARGET_DIR=target/vendor-tests cargo test --manifest-path third_party/winit/Cargo.toml --locked --lib termination` on macOS for the request-state tests.

The Slint Winit backend registers that interceptor when its custom application handler asks for it, before the event loop runs, and delivers each request to the handler through its event-loop proxy; without a handler, it denies the request. When the handler supplies an About title, the default and declarative application menus both build the About item with the reserved menu identifier `slint-application-about`, and the backend routes that identifier to the handler before parsing window menu identifiers. Otherwise the predefined About item remains. The default menu bar installs the same once-installed menu-event bridge as declarative menus.

The standard Button's accessibility default action — the press an assistive technology sends — reaches `clicked` only while the button is enabled, in every widget style, as pointer and keyboard presses already do. The GUI test `accessibility_presses_respect_disabled_buttons` exercises it.
