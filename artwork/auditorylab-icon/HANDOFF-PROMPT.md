You are adding the new application icon to Auditory Lab (https://github.com/resoltico/AuditoryLab). Everything you need is in this package (`auditorylab-icon-<date>.tar.gz`). Read `README.md`, `ORIGINALITY.md` and `ARTWORK-LICENSE.md` in it first. The repository currently has no app icon on any platform.

## The icon

"Woven": two sine waves of slightly different frequency, one woven over the other, with a clear gap at each crossing. They stand for the two tones of a binaural beat. The design and colours are final; do not redraw, recolour or re-time it.

| Appearance | Background | Front wave | Back wave |
|---|---|---|---|
| Light (main) | `#F6F3EC` | `#15183F` | `#A8440B` |
| Dark (macOS 26+ only) | `#0A0C1F` | `#F6F3EC` | `#F1742C` |
| Tinted / clear (macOS 26+) | system-generated | — | back wave at 60% opacity |

- **Contrast:** both waves are at least 4.5:1 against the background, and the front wave is at least 2× brighter or darker than the back wave. This holds for normal vision and simulated protanopia, deuteranopia and tritanopia. `source/check_contrast.py` re-measures it.
- **Why there is no dark icon elsewhere:** only macOS 26+ switches app icons by appearance. Windows `.exe` icons and Linux icon themes have no standard light/dark switch, so they use the light icon.

## Package contents

- `source/make_icon.py` and `source/palette.json`: the single source of truth. `python3 source/make_icon.py OUT` regenerates every asset. It needs shapely 2, cairosvg, Pillow and numpy.
- `assets/macos/AppIcon.icon/`: Icon Composer package (light, dark and tinted). It passes the community icon.json schema check, but has **not** been compiled with actool yet.
- `assets/macos/AppIcon.icns`: flat light icon for macOS builds made without Xcode's actool.
- `assets/windows/auditorylab.ico`: 16, 20, 24, 32, 40, 48, 64 and 256 px.
- `assets/linux/hicolor/…/org.resoltico.auditorylab.{png,svg}`: the hicolor theme tree, including a scalable SVG.
- `assets/runtime/window-icon.png`: 512 px, for Slint `Window.icon` (title bar and taskbar on Windows and Linux).
- `assets/artwork/`: flat light and dark SVGs, for documentation only.
- `preview.png`: the light, dark and tinted looks at 320 down to 16 px.
- `SHA256SUMS`: checksums of every file.

## Integration tasks

1. **Licensing first.** The project is MPL 2.0, but the owner wants the artwork proprietary.
   - Add the artwork terms (start from `ARTWORK-LICENSE.md`; the owner should have it reviewed) and keep `LICENSE` for the code.
   - Say in `README.md` and `CONTRIBUTING.md` which paths are artwork.
   - Ship the artwork licence in every desktop archive that contains the icon.
   - Add `ORIGINALITY.md` as the provenance record (for example `docs/artwork-provenance.md`).
2. **Repository layout.** Commit the source and assets under one artwork folder (for example `crates/gui/icon/`, keeping the `source/` and `assets/` layout). `assets/` must stay exactly reproducible by `make_icon.py`.
3. **macOS** (`xtask/src/main.rs`, `fn bundle`). It currently writes `Info.plist` and codesigns, with no icon.
   - **Preferred, for dark/tinted support on macOS 26+:**
     - On the macOS release builders (full Xcode 26 or later is required), compile `AppIcon.icon` with:
       `xcrun actool AppIcon.icon --compile <Contents/Resources> --output-format human-readable-text --notices --warnings --errors --output-partial-info-plist <tmp>/icon-partial.plist --app-icon AppIcon --include-all-app-icons --enable-on-demand-resources NO --development-region en --target-device mac --minimum-deployment-target 15.0 --platform macosx`
     - Require `Assets.car` and `AppIcon.icns` in the output.
     - Merge the partial plist into `Info.plist`; it should yield `CFBundleIconFile = AppIcon` and `CFBundleIconName = AppIcon`.
     - Do all of this **before** `codesign`.
     - Xcode generates the older-macOS images from the same `.icon`, so macOS 15 is covered.
   - **Fallback, if Xcode is not available:** copy `assets/macos/AppIcon.icns` to `Contents/Resources/AppIcon.icns` and add `CFBundleIconFile = AppIcon`. There is no dark variant in this case.
   - Extend the extracted-package checks in `xtask/src/package.rs` to assert the icon keys and files. `codesign --verify --deep --strict` must still pass.
4. **Windows.** Embed `auditorylab.ico` as the executable's icon resource for `auditorylab-gui.exe` (MSVC), through `crates/gui/build.rs`. That needs a resource-embedding build dependency such as `winresource` or `embed-resource`. Pin it, and make sure it passes `deny.toml`, the lint policy and `cargo about` notices. Use it only on Windows targets.
5. **Runtime window icon (Windows and Linux).** Set `icon: @image-url("…/window-icon.png");` on `MainWindow` (and on the other top-level windows in `crates/gui/ui/` if they appear in the taskbar). `build.rs` already embeds resources.
6. **Linux.**
   - Add `Icon=org.resoltico.auditorylab` to the `.desktop` text written in `xtask/src/main.rs`.
   - Ship the `hicolor` tree in the Linux desktop archive.
   - Document installing it in `xtask/templates/archive-readme.txt`: copy it to `~/.local/share/icons/hicolor/`, or use `xdg-icon-resource` and `xdg-desktop-menu`.
7. **Checks and release.** Update any archive-content allowlists or expected-file sets (`collect-assets` and the dist inspections) for the new files. Add a `CHANGELOG.md` entry.

## Verify (required)

- Run `python3 source/make_icon.py /tmp/out`, then `diff -r /tmp/out assets`. The output must match exactly, or the differences must be explained by library versions; record which.
- Run `python3 source/check_contrast.py`. It must print PASS.
- On macOS with Xcode 26+:
  - Open `AppIcon.icon` in Icon Composer and preview Default, Dark and Mono for macOS.
  - Build the app with `cargo xtask dist` for both macOS targets.
  - Confirm the Finder and Dock icon in Light and Dark modes, and under System Settings → Appearance → icon style Dark/Clear/Tinted.
  - Confirm `codesign --verify --deep --strict` still passes.
- On Windows: check the icon in Explorer, the taskbar and Alt-Tab at 100% and 200% scaling.
- On Linux: check the launcher, dock and window icon after installing the hicolor tree.
- Run the project's full CI checks (`cargo xtask …`, clippy, cargo-deny, tests).

Do not alter the artwork. If anything about it must change, change `make_icon.py` or `palette.json`, regenerate `assets/`, re-run `check_contrast.py`, and update `ORIGINALITY.md`.
