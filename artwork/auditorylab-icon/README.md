# Woven icon assets

Two sine waves at different frequencies weave across one another, representing the two tones of a binaural beat. [Software and artwork terms](../../LICENSE) govern use; [attribution and geometry](ORIGINALITY.md) identify the drawing source. The [application artwork guide](../README.md) documents actual runtime consumers.

| Path | Contents |
| --- | --- |
| `source/make_icon.py`, `source/palette.json` | Drawing geometry and palettes. |
| `source/check_contrast.py` | Palette contrast calculation with color-vision simulations. |
| `assets/macos/AppIcon.icon` | Editable Icon Composer package and SVG layers, with light/dark/tinted settings. |
| `assets/macos/AppIcon.icns` | Flat reference export; app packaging compiles the editable package instead. |
| `assets/windows/auditorylab.ico` | Windows executable icon. |
| `assets/linux/hicolor` | PNG sizes and scalable SVG for `org.resoltico.auditorylab`. |
| `assets/runtime/window-icon.png` | Embedded 512px Slint icon. |
| `assets/artwork` | Flat light/dark SVG references. |
| `preview.png` | Appearance and size reference. |

The light palette supplies Windows, Linux and the runtime PNG. The macOS icon package also carries dark and tinted appearance settings for system rendering.

## Inspect construction output

Application builds use the tracked assets and do not need Python image libraries. To inspect the drawing generator, use an isolated Python environment with Shapely 2 or later, CairoSVG and Pillow, plus NumPy for the contrast script. Run from this directory, writing to a separate output directory:

```sh
python3 source/make_icon.py ../../target/icon-review
python3 source/check_contrast.py
diff -ru assets ../../target/icon-review
shasum -a 256 -c SHA256SUMS
```

The generator emits `assets/` contents; it does not regenerate the separate preview or this documentation. Image-library versions can affect exported bytes, so inspect differences rather than treating unpinned construction tools as a reproducible application build. The contrast script calculates reference-palette ratios, not whole-application accessibility or native rendering qualification. The manifest covers the files it lists; it is not a complete directory inventory. Changes to proprietary artwork require the owner's permission.
