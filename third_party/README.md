# Desktop toolkit integration

The workspace patches the native toolkit for accessibility, focus, multiline editing, modeless windows and macOS application menus. Font shaping and X11 DPI use the toolkit's upstream policy. No font files are embedded.

[patches.toml](patches.toml) defines archive URLs, versions and SHA-256 digests. [patches/](patches/) contains the exact unified diffs. Vendored source must equal its authenticated archive with the corresponding patch applied.

```sh
cargo xtask verify-vendor
```

The verifier authenticates archives before extraction, compares every source byte and checks Cargo resolution for both products on every target in `xtask/src/targets.rs`. Temporary extraction stays under `target/tmp`; authenticated archives stay under `target/vendor-review` until cleanup. Symlinks, unexpected additions, missing changes and registry resolutions that bypass a patch fail. Only Cargo metadata and compiler outputs are excluded. `curl`, Git and the pinned Rust toolchain are required.

## Retained behavior

| Patch | Behavior and retirement condition |
| --- | --- |
| AccessKit Unix | Cache add/remove signals encode one structured D-Bus argument, as specified by [AT-SPI Cache](https://github.com/GNOME/at-spi2-core/blob/main/xml/Cache.xml). An upstream replacement must pass the production signal-serialization tests and refresh removed/reinserted accessible controls. |
| Slint core | Preserve aggregate macOS modifiers, full multiline shaped extents, shaped cursor height, semantic focus reveal and clipped editor focus. An upstream replacement must pass keyboard, caret, scroll, reflow and removed-row focus tests. |
| Slint compiler/widgets | Disabled accessibility presses cannot invoke buttons; TextEdit reveals the shaped cursor rectangle. An upstream replacement must pass `accessibility_presses_respect_disabled_buttons` and `text_edit_cursor_scrolls_into_view`. |
| Slint Winit backend | Wayland show requests a redraw; modeless parent requests survive window recreation. macOS termination and application About route to the application handler. An upstream replacement must preserve those native boundaries and the unsaved-change Save/Discard/Cancel flow. |
| Winit | Expose Wayland modeless parenting and deferred/coalesced macOS termination requests. An upstream replacement must preserve parent ownership, main-thread replies, stale-request rejection and one pending Quit request. |

The standard Slint application About item opens the toolkit's predefined native panel. The custom About event routes to the application's window containing its project and license actions. AppKit termination is intercepted through Winit's own application delegate; the delegate is not replaced. Repeated Quit joins one pending request; unavailable or abandoned handlers deny it.

The published AccessKit version accepted by the pinned `accesskit_winit` dependency is used with its cache-signal correction. Its dependency range does not accept the newer incompatible AccessKit Unix release.

## Checks

The workspace GUI suite exercises the production controls and retained focus/input behavior. On Linux, the bridge tests exercise the real serialized cache signals:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path third_party/accesskit_unix/Cargo.toml --lib --locked
```

On macOS, termination state tests exercise the request implementation:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path third_party/winit/Cargo.toml --lib --locked termination
```

Linux X11 scaling follows Xft DPI through Winit. On GNOME Wayland, the application reads only `text-scaling-factor` using GLib's `gsettings` utility, because the toolkit's font-name preference does not include that factor. The reader runs on one worker with bounded process waits; no native font-family parsing or Pango/GTK loading is involved. Native backend identity determines whether the reader starts, so an X11 window does not apply the factor twice. macOS uses AppKit's system size, and Windows uses message-font metrics at 96 DPI.
