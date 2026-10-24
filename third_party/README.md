# Desktop toolkit integration

The workspace patches pinned i-slint-core, i-slint-compiler and i-slint-backend-winit 1.18.1, Parley 0.11.1, Winit 0.30.13 and AccessKit Unix 0.22.1 sources for native typography, window scaling, modeless window parenting, keyboard input, accessibility, multiline text scrolling, and macOS application termination and About routing. Their original manifests and licenses remain alongside the source. No font files are embedded.

## Sources

[patches.toml](patches.toml) is the authoritative archive URL/version/SHA-256 record. [patches/](patches/) contains upstream-ready unified diffs, including license additions and the backend lockfile omission. Vendored source must equal the authenticated archive with exactly its reviewed patch applied. Changed-file inventories are derived from these diffs, rather than maintained separately.

Run from the repository root:

```sh
cargo xtask verify-vendor
```

This command downloads missing archives to `target/vendor-review`, verifies SHA-256 before extraction, applies each patch in an automatically removed temporary directory under `target/tmp`, compares every source byte, and checks Cargo metadata plus normal/build graphs for both products at every target defined in `xtask/src/targets.rs`. Only `.cargo_vcs_info.json` and Cargo's top-level `target/` compiler outputs are excluded. Symlinks, unreviewed additions, missing edits and registry resolutions that bypass a patch fail. The SHA implementation uses the maintained RustCrypto `sha2` crate; no platform-specific hashing command is required. `curl`, Git and the pinned Rust toolchain are prerequisites. The command does not build native targets or qualify their behavior.

For an upgrade, authenticate the new archive first, inspect upstream changes and the existing diff, reapply only required fixes, then update source, patch and archive record together. Run this verifier and the boundary tests below. A matching package version or patch filename is insufficient evidence. Negative controls in `xtask/src/vendor.rs` exercise corrupt bytes, missing files and unexpected source additions.

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

The GUI test `platform_modifiers_drive_text_shortcuts` exercises real text selection from aggregate-only flags, both physical/canonical event orders, side-key release, focus reset and Windows AltGr clearing.

The shared TextEdit widget scrolls the complete shaped cursor rectangle into view. The core publishes its actual height before the existing cursor-position callback; the widget uses that height for both viewport edges and short viewports instead of a fixed 20px margin. Font leading and fallback shaping remain the renderer’s responsibility.

The GUI test `text_edit_cursor_scrolls_into_view` exercises it.

Multiline TextInput preferred height includes every shaped line’s full vertical extent, so fallback fonts that extend beyond the nominal line advance remain reachable by scrolling. Other text measurement retains its existing policy.

Keyboard focus reveals the owning semantic control when its internal focus scope has zero width or height. This keeps the whole button, checkbox or other leaf control reachable without enlarging its pointer target. Owner selection stops at popup, scroll-container and window boundaries; ordinary positive-sized focus retains its existing geometry. On an axis where the owner exceeds the visible viewport, scrolling retains the original focus extent instead of chasing an edge that cannot fit. The GUI test `control_focus_scrolls_into_view` exercises it.

Input focus survives clipping by a scroll container, including pointer movement after typography reflow. Each ancestor clip is evaluated in its own geometry. Uncaptured nonmodifier key presses reveal a clipped focused control before delivery; pointer movement and captured shortcuts preserve the viewport. Hidden or detached controls lose focus. Dynamic components must match their parent’s active subtree, and a component with no surviving parent must be its existing window’s canonical root. These checks use the existing component and window identities without retaining removed delegates. The GUI test `pending_entry_survives_font_reflow_clipping_and_row_changes` exercises these paths through the production Workbench bindings.

On macOS, Winit’s own application delegate implements `applicationShouldTerminate:`. With an interceptor registered through `EventLoopExtMacOS::set_termination_interceptor`, it answers “terminate later”, owns the pending request under an epoch that is never reused, and hands the interceptor an opaque `TerminationRequest`. Repeated Quit joins the pending request. A request the interceptor cannot take is denied, and so is one still pending when the event loop exits. `reply_to_termination` answers only on the main thread, returns the request unchanged when called from another thread, and refuses a request that is no longer pending. Without an interceptor, termination proceeds as AppKit’s default. The delegate object itself is not replaced. Run `CARGO_TARGET_DIR=target/vendor-tests cargo test --manifest-path third_party/winit/Cargo.toml --locked --lib termination` on macOS for the request-state tests.

The Slint Winit backend registers that interceptor when its custom application handler asks for it, before the event loop runs, and delivers each request to the handler through its event-loop proxy; without a handler, it denies the request. When the handler supplies an About title, the default and declarative application menus both build the About item with the reserved menu identifier `slint-application-about`, and the backend routes that identifier to the handler before parsing window menu identifiers. Otherwise the predefined About item remains. The default menu bar installs the same once-installed menu-event bridge as declarative menus.

The standard Button's accessibility default action — the press an assistive technology sends — reaches `clicked` only while the button is enabled, in every widget style, as pointer and keyboard presses already do. The GUI test `accessibility_presses_respect_disabled_buttons` exercises it.

## Patch retirement gates

Remove a patch only when an authenticated candidate upstream implementation passes its corresponding production-boundary checks, followed by native qualification on affected platforms:

| Patch | Required behavior before retirement |
| --- | --- |
| AccessKit Unix | AT-SPI cache add/remove signals encode one structured argument and real assistive clients refresh removed/reinserted controls. |
| Slint core | Native optical sizing/tracking, multiline shaped extents, semantic focus recovery, clipped editor focus and modifier normalization pass the font, reflow, scroll and keyboard suites. |
| Slint compiler/widgets | TextEdit reveals the shaped cursor rectangle and disabled accessibility presses cannot invoke buttons in any style. |
| Slint Winit backend | Modeless parenting/show redraw work on Wayland; native macOS termination and About route through the production handler with stale epochs rejected. |
| Parley | Shaping preserves logical optical sizing/tracking; bidi membership uses public property identities without deprecated numeric encodings. |
| Winit | XSettings scaling preserves explicit override/KDE precedence; Wayland parent requests and macOS deferred/coalesced termination pass their native boundaries. |

Parley's bidi masks now assign local distinct membership bits to the public `BidiClass` constants; their numeric values never escape the internal sets. This removes ICU 2.3's deprecated ICU4C accessor without suppressing warnings. Existing bidirectional and GUI text tests remain required.

The published Parley archive omits the `parley_dev` test helper, so its full standalone `cargo test --lib` does not compile from these published sources. The changed membership function lives in `src/bidi_mask.rs`; xtask includes that exact production file and checks nonzero, pairwise-disjoint bits for every public class used by the resolver. It compiles against the workspace's resolved ICU version under the authored warnings policy. GUI shaping/keyboard/font tests exercise the integrated renderer. These focused checks do not claim execution of upstream's full font-fixture suite.
