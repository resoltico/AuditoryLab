# Desktop toolkit integration

The workspace patches the native toolkit for accessibility, focus, multiline editing, modeless windows and macOS application menus. Font shaping and X11 DPI use the toolkit's upstream policy. No font files are embedded.

[patches.toml](patches.toml) defines archive URLs, versions and SHA-256 digests. [patches/](patches/) contains the exact unified diffs. Vendored source must equal its authenticated archive with the corresponding patch applied. Each vendored package includes its authenticated archive’s `Cargo.lock`.

```sh
cargo xtask verify-vendor
```

The verifier authenticates archives before extraction, compares every source byte and checks Cargo resolution for both products on every target in `xtask/src/targets.rs`. Temporary extraction stays under `target/tmp`; authenticated archives stay under `target/vendor-review` until cleanup. Symlinks, unexpected additions, missing changes and registry resolutions that bypass a patch fail. Only Cargo metadata and compiler outputs are excluded. `curl`, Git and the pinned Rust toolchain are required.

## Patched behavior

| Patch | Retirement condition | Proof required on the replacement |
| --- | --- | --- |
| AccessKit Unix | A compatible Slint/AccessKit graph resolves `accesskit_unix` 0.24.1 or newer and emits each cache payload as one structured D-Bus argument. | Linux `cache_add_carries_one_complete_struct_argument` and `cache_remove_carries_one_object_reference_struct` in [bus.rs](accesskit_unix/src/atspi/bus.rs); removed/reinserted accessible controls refresh correctly. |
| Slint core | Upstream provides equivalent aggregate modifier synchronization, complete shaped multiline extents/cursor height, semantic focus reveal, key focus for scroll-clipped editors and active-row focus validation. Retire individual hunks only when their behavior is covered. | `platform_modifiers_drive_text_shortcuts`, `text_edit_cursor_scrolls_into_view`, `control_focus_scrolls_into_view` and `partly_scrolled_editor_keeps_focus_through_keys` in [window tests](../crates/gui/src/window_tests.rs), plus both [focus/reflow tests](../crates/gui/src/focus_reflow_tests.rs) and retired-row callback tests in [adapter tests](../crates/gui/src/adapter_tests.rs). |
| Slint compiler | TextEdit reveals the shaped cursor rectangle and its height; the paired core property exists in the same resolved Slint version. | `text_edit_cursor_scrolls_into_view` at supported text sizes, with the [compiler binding](i-slint-compiler/widgets/common/textedit-base.slint) and [core geometry](i-slint-core/items/text.rs) exercised together. |
| Slint Winit backend | Upstream supplies first-show redraw, normalized aggregate macOS modifiers, deferred termination routing and application-owned About actions through supported public APIs. | Wayland first-show/details recreation; macOS modifier test, native About action and Quit Save/Discard/Cancel smoke; [adapter workflows](../crates/gui/src/adapter_tests.rs) must retain captures and one pending prompt. |
| Winit | Public APIs support remappable Wayland modeless parents and application-owned deferred/coalesced AppKit termination without replacing a toolkit-owned delegate. | Linux parent recreation and invalid-parent checks through [Details binding](../crates/gui/src/details_parent.rs); macOS `requests_coalesce_and_each_epoch_is_answered_once` and `undeliverable_and_abandoned_requests_are_settled` in [termination state](winit/src/platform_impl/macos/app_state.rs), plus native Quit/Cancel. |

Application buttons use [the app-owned component](../crates/gui/ui/button.slint). It retains toolkit rendering and focus, gives every button one height that follows the window's text size, exposes one accessible button, and forwards its `triggered` application callback only while enabled. `accessibility_presses_respect_disabled_buttons` exercises this component through the production accessibility action.

The macOS application menu's About item opens the application's window containing its project and license actions. AppKit termination is intercepted through Winit's own application delegate; the delegate is not replaced. Repeated Quit joins one pending request; unavailable or abandoned handlers deny it.

The AccessKit Unix patch must satisfy the pinned `accesskit_winit` dependency range and cache-signal contract. The exact patched version is recorded in [patches.toml](patches.toml) and resolved in [Cargo.lock](../Cargo.lock). A replacement requires a compatible toolkit graph; substituting a package outside its declared range is insufficient.

## Dependency boundaries

[Cargo.toml](../Cargo.toml), [Cargo.lock](../Cargo.lock) and [patches.toml](patches.toml) define the current dependency graph. Slint core, compiler and backend share one version; the compiler's cursor-height input is paired with the core's shaped geometry. The backend uses Slint's Winit 0.30 API. AccessKit Unix must satisfy `accesskit_winit`'s dependency range and use compatible AccessKit data types throughout that graph.

Modeless parent-request retention belongs to the [Details binding](../crates/gui/src/details_parent.rs): it reacquires native handles after recreation and never retains a retired Wayland surface. Winit exposes the parent request; the backend's deferred-show redraw does not own this application binding.

## Checks

The workspace GUI suite exercises the production controls and retained focus/input behavior. On Linux, the bridge tests exercise the real serialized cache signals:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path third_party/accesskit_unix/Cargo.toml --lib --locked
```

On macOS, termination state tests exercise the request implementation:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path third_party/winit/Cargo.toml --lib --locked termination
```

Linux X11 scaling follows Xft DPI through Winit. The GNOME Wayland text-scaling reader obtains `text-scaling-factor` through GLib's `gsettings` utility because the toolkit's font-name preference does not include that factor. The reader runs on one worker with bounded process waits; no native font-family parsing or Pango/GTK loading is involved. Native backend identity determines whether the reader starts, so an X11 window does not apply the factor twice. macOS reads AppKit’s `systemFontSize`; its supported-app preferred-reading-size control is not an input to this adapter. Windows uses message-font metrics at 96 DPI. The [desktop guide](../docs/desktop.md#appearance-typography-and-accessibility) describes user-facing behavior.
