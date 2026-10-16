Auditory Lab desktop {version}

Open Auditory Lab.app on macOS, auditorylab-gui.exe on Windows, or
./auditorylab-gui on Linux. Selecting a sound never starts playback. Keep
output volume low before pressing Play. Stop requests cancellation; close
waits for the actual operation and worker to finish. Existing files are never
replaced. Imported recipes remain snapshots until explicitly reopened.

Requirements: macOS {macos_minimum} or later; Windows 10 x64 or Windows 11 ARM64;
Ubuntu 22.04-class Linux with a working X11 or Wayland display, system fonts,
D-Bus/accessibility services and a desktop portal or Zenity for native pickers.
Manual native-path entry remains available when a picker returns no selection.
For a Linux launcher entry, place auditorylab-gui in a directory already on
PATH (for example ~/.local/bin), then copy auditorylab-gui.desktop into
~/.local/share/applications. No installer or PATH changes are performed.
The preferred drawing route is Slint FemtoVG/WGPU with a compatible adapter;
software rendering remains available. Only early renderer construction and
no-adapter preflight have automatic software fallback. Later surface/device
loss does not promise a cross-renderer switch. For per-process recovery launch
with SLINT_BACKEND=winit-software. The sound engine remains deterministic CPU
code; drawing does not use all installed GPUs or offload audio.

Graphics loaders/drivers may load dynamically and are not proved by an import
list. macOS uses Metal. Linux needs security-updated FreeType and Fontconfig
(libfreetype6 and libfontconfig1), libxkbcommon0, and libxkbcommon-x11-0 for X11
keyboard handling. The Linux system-font integration is based in part on the
work of the FreeType Team; complete reference terms are in THIRD-PARTY-NOTICES.txt.
GPU drawing also needs an installed Vulkan loader/driver.
Software rendering still needs the display and keyboard libraries for window
interaction. Windows uses compatible Direct3D support; WGPU may use an existing
dxcompiler.dll and falls back to OS FXC when the optional library is absent.
No graphics driver or DXC implementation is bundled or statically linked.
Accessibility, picker and actual first-frame renderer behavior need separate
native qualification. Compilation alone is not proof that GPU drawing occurs.

Help -> About shows the product/build identity, MPL-2.0 information and the
standard AboutSlint attribution. This desktop integration selects Slint's
Royalty-free Desktop, Mobile, and Web Applications License 2.0; its retained
terms and dependency notices are in THIRD-PARTY-NOTICES.txt. Auditory Lab's own
source remains MPL-2.0 (LICENSE).

A local build is unqualified and claims no immutable source SHA. Candidate
artifacts include their full source revision below; that identity alone does
not prove native interaction, accessibility, audio timing or minimum-OS tests.

{source_reference}
Auditory Lab source is available under MPL-2.0; see LICENSE and the source
reference above for access to the exact build source.

Linux source builds must run from the checkout root with its Cargo configuration.
The links=freetype overrides select the system library and prevent the producer's
bundled native fallback from compiling. Keep distribution font packages updated;
do not bypass this policy with outside-root --manifest-path or alternate configs.
