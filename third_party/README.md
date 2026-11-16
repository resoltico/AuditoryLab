# Dependency integration

The workspace patches the native toolkit for accessibility, focus, multiline editing, modeless windows and macOS application menus, including typed Windows UI Automation interface arrays, the RIFF reader for bounded RF64/BW64 lengths, and the AAC decoder to refuse unsupported profiles and SBR rather than report an incomplete LC-only decode. Font-family resolution, fallback shaping and X11 DPI use the toolkit's upstream policy. The application embeds no fonts and uses system fonts; on Linux, Winit's Wayland window decorations embed the Cantarell title font, which packaging credits under its SIL Open Font License.

[patches.toml](patches.toml) defines archive URLs, versions and SHA-256 digests. [patches/](patches/) contains the exact unified diffs. Vendored source must equal its authenticated archive with the corresponding patch applied. Archive `Cargo.lock` files are retained when present; `spin_on` ships none and uses the workspace lockfile.

```sh
cargo xtask verify-vendor
```

The verifier authenticates archives before extraction, compares every source byte and checks Cargo resolution for both products on every target in `xtask/src/targets.rs`. Temporary extraction stays under `target/tmp`; authenticated archives stay under `target/vendor-review` until cleanup. Symlinks, unexpected additions, missing changes and registry resolutions that bypass a patch fail. Only Cargo metadata and compiler outputs are excluded. `curl`, Git and the pinned Rust toolchain are required.

## Patched behavior

| Component | Maintained behavior | Qualification |
| --- | --- | --- |
| spin_on | The executor uses `core::pin::pin!`, keeping `pin-utils` out of Slint's build-tool graph (RUSTSEC-2026-0441). | Ready/pending executor tests, the workspace GUI build and all target dependency graphs. |
| Symphonia RIFF | RF64/BW64 ds64 lengths use checked bounds and bounded table metadata. | RF64/BW64 export-sample equivalence, invalid-size refusals and sparse seeking beyond 4 GiB in [recording decoder tests](../crates/app/src/storage/recording/decode/tests.rs). |
| Symphonia AAC | Non-LC ADTS profiles and in-band SBR are rejected before incomplete samples can be returned. | AAC-profile and in-band SBR refusals in [recording decoder tests](../crates/app/src/storage/recording/decode/tests.rs), with AAC-LC fixture positive controls. |
| AccessKit Windows | UIA SAFEARRAYs preserve typed provider interface pointers, their IID metadata and COM reference ownership; unsupported text searches return failure HRESULTs. | Native [selection tests](accesskit_windows/src/selection_tests.rs) and actual selected multiline text through the [Windows UI Automation harness](../crates/gui/native/windows.ps1). |
| Slint core | Aggregate modifier synchronization, complete shaped multiline extents, maximal viewport coverage for oversized reveal targets, accumulated clipping and active-row focus validation. | Modifier, cursor, control-focus and clipped-editor [window tests](../crates/gui/src/window_tests.rs), [focus/reflow tests](../crates/gui/src/focus_reflow_tests.rs) and retired-row callback [adapter tests](../crates/gui/src/adapter_tests.rs). |
| Slint Winit backend | Aggregate macOS modifiers and deferred OS termination routing use the shared native backend. | Wayland first-show/details recreation, macOS modifiers and About/Quit Save/Discard/Cancel through the [native harnesses](../crates/gui/native/README.md). |
| Winit | Modeless Wayland parents can be reacquired, and AppKit termination is deferred/coalesced without replacing the toolkit delegate. | Parent recreation/invalid-parent checks through [Details binding](../crates/gui/src/details_parent.rs), [termination state tests](winit/src/platform_impl/macos/app_state.rs) and native Quit/Cancel. |

Application widgets use `ensure-visible(ScrollBehavior.instant)` on their own rectangles when focused, and Library rows use that same public API. Slint owns caret scrolling and native smooth-scrolling requests. Focus validity intersects clipping rectangles through the ancestor chain; active-row checks reject retained controls from retired rows.

Application buttons use [the app-owned component](../crates/gui/ui/button.slint). It retains toolkit rendering and focus, gives every button one height that follows the window's text size, exposes one accessible button, and forwards toolkit activation to its `triggered` application callback. The toolkit owns disabled-input admission, including accessibility actions. `accessibility_presses_respect_disabled_buttons` exercises this component through the production accessibility action.

The macOS application menu's About item opens the application's window containing its project and license actions. AppKit termination is intercepted through Winit's own application delegate; the delegate is not replaced. Repeated Quit joins one pending request; unavailable or abandoned handlers deny it.

## Dependency boundaries

[Cargo.toml](../Cargo.toml), [Cargo.lock](../Cargo.lock) and [patches.toml](patches.toml) define the current dependency graph. Slint core, the registry compiler and the backend share one version. TextInput owns shaped-caret scrolling; the application uses declared menu roles for native About and Quit placement. The backend uses Slint's Winit 0.30 API. AccessKit Unix uses the registry's current compatible adapter, whose structured cache signals are exercised through the native AT-SPI client.

Modeless parent-request retention belongs to the [Details binding](../crates/gui/src/details_parent.rs): it reacquires native handles after recreation and never retains a retired Wayland surface. Winit exposes the parent request; Slint's first-show redraw remains separate from this application binding.

## Checks

The workspace GUI suite exercises the production controls and retained focus/input behavior. The core's `reveal_bounds_tests` specify literal minimal-move, oversized-coverage, scroll-limit and nonfinite cases; CI runs only that named vendor module with the current `std` feature before full production-feature GUI integration; renderer unit fixtures are outside this geometry test. On Linux, the [native graphical harness](../crates/gui/native/README.md) exercises the actual registry adapter through AT-SPI cache discovery and control updates.

On Windows, provider-array tests exercise the native SAFEARRAY ABI and reference ownership:

```sh
CARGO_TARGET_DIR="$PWD/target/vendor-tests" cargo test --manifest-path third_party/accesskit_windows/Cargo.toml --lib --locked selection_tests
```

The Windows patch preserves `ITextRangeProvider` and `IRawElementProviderSimple` pointers where UIA contracts require those interfaces. Canonical `IUnknown` identity is a different pointer/vtable for the resolved `windows-implement`; returning it in a typed selection array can crash clients reading selected text. `SafeArrayPutElement` retains the supplied interface and AddRefs it; `SafeArrayCreateVectorEx` retains the corresponding typed IID in the descriptor across COM/UIA boundaries. In-process pointer/lifetime checks and the actual out-of-process UIA text/selection harness prove different boundaries; neither substitutes for the other. Ordinary `GetPatternProvider` identity handling is unchanged. Unsupported optional text-range searches return `E_NOTIMPL`; returning an empty Rust error would encode `S_OK` without initializing the output interface. The adapter's original licenses and archive lockfile remain intact.

On macOS, termination state tests exercise the request implementation:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path third_party/winit/Cargo.toml --lib --locked termination
```

Linux X11 scaling follows Xft DPI through Winit. The GNOME Wayland text-scaling reader obtains `text-scaling-factor` through GLib's `gsettings` utility because the toolkit's font-name preference does not include that factor. The reader runs on one worker with bounded process waits; no native font-family parsing or Pango/GTK loading is involved. Native backend identity determines whether the reader starts, so an X11 window does not apply the factor twice. macOS reads AppKit’s `systemFontSize`; its supported-app preferred-reading-size control is not an input to this adapter. Windows uses message-font metrics at 96 DPI. The [desktop guide](../docs/desktop.md#appearance-typography-and-accessibility) describes user-facing behavior.
