# Auditory Lab application icon: "Woven"

Two sine waves at slightly different frequencies, one woven over the other. They stand for the two tones of a binaural beat.

| Folder | Contents |
|---|---|
| `source/` | `make_icon.py` (generates everything), `palette.json` (colours), `check_contrast.py` (accessibility check) |
| `assets/macos/` | `AppIcon.icon` (Icon Composer: light, dark and tinted), `AppIcon.icns` (flat fallback) |
| `assets/windows/` | `auditorylab.ico` (16–256 px) |
| `assets/linux/` | hicolor icon theme tree, `org.resoltico.auditorylab` (PNG 16–512 px plus SVG) |
| `assets/runtime/` | `window-icon.png` (512 px) for the Slint window icon |
| `assets/artwork/` | flat light and dark SVGs, for documentation |
| `preview.png` | light, dark and tinted looks at 320 down to 16 px |

## Light and dark

The light palette is the main icon on every platform. The dark palette is used only on macOS 26 and later, which switch icons automatically from the single `AppIcon.icon`. Windows and Linux have no standard light/dark switch for application icons.

## Regenerate and check

```sh
pip install "shapely>=2" cairosvg pillow numpy
python3 source/make_icon.py assets      # rebuilds every asset
python3 source/check_contrast.py        # prints PASS when the palette meets the targets
```

`HANDOFF-PROMPT.md` covers integration into the repository, `ORIGINALITY.md` covers provenance and `ARTWORK-LICENSE.md` covers licensing.
