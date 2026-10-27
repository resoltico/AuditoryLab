# Desktop toolkit integration

The workspace patches the native toolkit for accessibility, focus, multiline editing, modeless windows and macOS application menus. Font shaping and X11 DPI use the toolkit's upstream policy. No font files are embedded.

[patches.toml](patches.toml) defines archive URLs, versions and SHA-256 digests. [patches/](patches/) contains the exact unified diffs. Vendored source must equal its authenticated archive with the corresponding patch applied. Each vendored package includes its authenticated archive’s `Cargo.lock`.

```sh
cargo xtask verify-vendor
```

The verifier authenticates archives before extraction, compares every source byte and checks Cargo resolution for both products on every target in `xtask/src/targets.rs`. Temporary extraction stays under `target/tmp`; authenticated archives stay under `target/vendor-review` until cleanup. Symlinks, unexpected additions, missing changes and registry resolutions that bypass a patch fail. Only Cargo metadata and compiler outputs are excluded. `curl`, Git and the pinned Rust toolchain are required.

## Patched behavior

| Patch | Behavior and retirement condition |
| --- | --- |
| AccessKit Unix | Cache add/remove signals encode one structured D-Bus argument, as specified by [AT-SPI Cache](https://github.com/GNOME/at-spi2-core/blob/main/xml/Cache.xml). An upstream replacement must pass the production signal-serialization tests and refresh removed/reinserted accessible controls. |
| Slint core | Preserve aggregate macOS modifiers, full multiline shaped extents, shaped cursor height, semantic focus reveal and clipped editor focus. An upstream replacement must pass keyboard, caret, scroll, reflow and removed-row focus tests. |
| Slint compiler | TextEdit reveals the shaped cursor rectangle. An upstream replacement must pass `text_edit_cursor_scrolls_into_view` at the supported text sizes. |
| Slint Winit backend | Wayland show requests a redraw. macOS aggregate modifiers, termination and application About route to their owning core/application handlers. An upstream replacement must preserve those native boundaries and the unsaved-change Save/Discard/Cancel flow. |
| Winit | Expose Wayland modeless parenting and deferred/coalesced macOS termination requests. An upstream replacement must preserve parent ownership, main-thread replies, stale-request rejection and one pending Quit request. |

Application buttons use [the app-owned component](../crates/gui/ui/button.slint). It retains toolkit rendering, sizing and focus, exposes one accessible button, and forwards its `triggered` application callback only while enabled. `accessibility_presses_respect_disabled_buttons` exercises this component through the production accessibility action.

The standard Slint application About item opens the toolkit's predefined native panel. The custom About event routes to the application's window containing its project and license actions. AppKit termination is intercepted through Winit's own application delegate; the delegate is not replaced. Repeated Quit joins one pending request; unavailable or abandoned handlers deny it.

The AccessKit Unix patch must satisfy the pinned `accesskit_winit` dependency range as well as the cache-signal contract. The exact patched version is recorded in [patches.toml](patches.toml) and resolved in [Cargo.lock](../Cargo.lock).

## Released APIs

| Package | Released behavior and compatibility | Upstream source |
| --- | --- | --- |
| `accesskit_unix` | 0.24.1 preserves Cache structure arguments. `accesskit_winit` 0.33.2 requires Unix ^0.22.1, whose compatible release still needs the backport; Unix 0.24.1 cannot replace it independently. | [0.24.1 release](https://github.com/AccessKit/accesskit/releases/tag/accesskit_unix-v0.24.1), [structure-argument fix](https://github.com/AccessKit/accesskit/commit/bffadacde348b0b73d3d7201bfb0b7f69a54b0bf), [0.33.2 dependency](https://docs.rs/crate/accesskit_winit/0.33.2/source/Cargo.toml) |
| `i-slint-core` | The published 1.18.1 source requires the aggregate-modifier, shaped-geometry and focus-lifecycle changes described above. | [1.18.1 release](https://github.com/slint-ui/slint/releases/tag/v1.18.1), [core source](https://github.com/slint-ui/slint/tree/372cf0ee5577c3dfec309a45e7b778ba4e81b734/internal/core) |
| `i-slint-compiler` | The 1.18.1 TextEdit scroll policy needs the shaped cursor height produced by the core patch; compiler and core remain paired. | [widget source](https://github.com/slint-ui/slint/blob/372cf0ee5577c3dfec309a45e7b778ba4e81b734/internal/compiler/widgets/common/textedit-base.slint) |
| `i-slint-backend-winit` | The 1.18.1 backend needs modifier forwarding, deferred Wayland redraw and custom macOS application-menu routing. | [backend source](https://github.com/slint-ui/slint/tree/372cf0ee5577c3dfec309a45e7b778ba4e81b734/internal/backends/winit) |
| `winit` | Slint 1.18.1 uses the 0.30 API. Stable 0.30.13 needs deferred/coalesced AppKit termination and Wayland modeless parenting; 0.31.0-beta.4 is outside that API line. | [0.30.13 source](https://github.com/rust-windowing/winit/tree/e9809ef54b18499bb4f2cac945719ecc2a61061b), [published versions](https://crates.io/crates/winit/versions) |

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

Linux X11 scaling follows Xft DPI through Winit. On GNOME Wayland, the application reads only `text-scaling-factor` using GLib's `gsettings` utility, because the toolkit's font-name preference does not include that factor. The reader runs on one worker with bounded process waits; no native font-family parsing or Pango/GTK loading is involved. Native backend identity determines whether the reader starts, so an X11 window does not apply the factor twice. macOS uses AppKit's system size, and Windows uses message-font metrics at 96 DPI.
