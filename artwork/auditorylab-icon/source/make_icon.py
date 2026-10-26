#!/usr/bin/env python3
"""Generate the Auditory Lab "Woven" platform assets from explicit geometry.

Two sine formulas and the palette define the drawing. The output covers assets/;
preview.png is separate. Exported bytes depend on the image-library versions.

Requires: Python 3.9+, shapely>=2, cairosvg, Pillow.
Usage:    python3 make_icon.py OUTPUT_DIR
"""
import io, json, math, os, sys
import cairosvg
from PIL import Image
from shapely.geometry import LineString
from shapely import affinity

HERE = os.path.dirname(os.path.abspath(__file__))
PALETTE = json.load(open(os.path.join(HERE, "palette.json")))

# ---------------------------------------------------------------------------
# Geometry (design units: 1024 canvas, glyph designed inside an 824-unit tile)
# ---------------------------------------------------------------------------
X0, X1 = 256, 768          # horizontal extent of both waves
AMPLITUDE = 180            # wave height from the centre line (y = 512)
FRONT_CYCLES = 1.5         # front tone
BACK_CYCLES = 2.0          # back tone: a slightly different frequency -> a beat
WIRE = 56                  # stroke thickness of each wave
GAP = 14                   # clear space cut around the front wave where it crosses
SAMPLES = 400

def front_y(t): return 512 - AMPLITUDE * math.sin(2 * math.pi * FRONT_CYCLES * t)
def back_y(t):  return 512 + AMPLITUDE * math.sin(2 * math.pi * BACK_CYCLES * t)

def wave(fn):
    return LineString([(X0 + (X1 - X0) * i / SAMPLES, fn(i / SAMPLES)) for i in range(SAMPLES + 1)])

def shapes(scale=1.0):
    """Filled outlines: the front wave, and the back wave with the crossing gaps removed."""
    f_line, b_line = wave(front_y), wave(back_y)
    front = f_line.buffer(WIRE / 2, cap_style=1, join_style=1, quad_segs=24)
    back = b_line.buffer(WIRE / 2, cap_style=1, join_style=1, quad_segs=24).difference(
        f_line.buffer(WIRE / 2 + GAP, cap_style=1, join_style=1, quad_segs=24))
    if scale != 1.0:
        front = affinity.scale(front, scale, scale, origin=(512, 512))
        back = affinity.scale(back, scale, scale, origin=(512, 512))
    return front, back

def path_d(geom):
    polys = [geom] if geom.geom_type == "Polygon" else list(geom.geoms)
    out = []
    for pg in polys:
        for ring in [pg.exterior, *pg.interiors]:
            out.append("M" + " L".join(f"{x:.2f} {y:.2f}" for x, y in ring.coords) + " Z")
    return " ".join(out)

TILE = '<rect x="100" y="100" width="824" height="824" rx="185" fill="{bg}"/>'
SHADOW = '<rect x="100" y="118" width="824" height="824" rx="185" fill="#000000" opacity="0.14"/>'

def flat_svg(mode, shadow=False):
    """Complete flat icon on the 824-unit rounded tile for reference and platform exports."""
    pal = PALETTE[mode]
    front, back = shapes()
    return ('<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">'
            + (SHADOW if shadow else "") + TILE.format(bg=pal["background"])
            + f'<path fill="{pal["back_wave"]}" fill-rule="evenodd" d="{path_d(back)}"/>'
            + f'<path fill="{pal["front_wave"]}" fill-rule="evenodd" d="{path_d(front)}"/></svg>\n')

def layer_svg(geom, color):
    return ('<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">'
            f'<path fill="{color}" fill-rule="evenodd" d="{path_d(geom)}"/></svg>\n')

def ext_srgb(hexcolor):
    r, g, b = (int(hexcolor[i:i + 2], 16) / 255 for i in (1, 3, 5))
    return f"extended-srgb:{r:.5f},{g:.5f},{b:.5f},1.00000"

def png(svg_text, size):
    return Image.open(io.BytesIO(cairosvg.svg2png(bytestring=svg_text.encode(),
                                                  output_width=size, output_height=size))).convert("RGBA")

def write(path, data, mode="w"):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, mode) as fh: fh.write(data)

def build(out):
    light, dark = PALETTE["light"], PALETTE["dark"]
    # --- macOS 26+: Icon Composer package (full-bleed canvas; system applies the mask) ----
    front, back = shapes(scale=1024 / 824)
    icon_dir = os.path.join(out, "macos", "AppIcon.icon")
    write(os.path.join(icon_dir, "Assets", "1-back-wave.svg"), layer_svg(back, light["back_wave"]))
    write(os.path.join(icon_dir, "Assets", "2-front-wave.svg"), layer_svg(front, light["front_wave"]))
    icon_json = {
        "fill-specializations": [
            {"value": {"solid": ext_srgb(light["background"])}},
            {"appearance": "dark", "value": {"solid": ext_srgb(dark["background"])}},
        ],
        "groups": [{
            "name": "Woven waves",
            "lighting": "individual",
            "specular": True,
            "shadow": {"kind": "neutral", "opacity": 0.5},
            "translucency": {"enabled": False, "value": 0.5},
            "layers": [
                {"name": "2-front-wave", "image-name": "2-front-wave.svg", "glass": True,
                 "fill-specializations": [
                     {"value": {"solid": ext_srgb(light["front_wave"])}},
                     {"appearance": "dark", "value": {"solid": ext_srgb(dark["front_wave"])}}]},
                {"name": "1-back-wave", "image-name": "1-back-wave.svg", "glass": True,
                 "fill-specializations": [
                     {"value": {"solid": ext_srgb(light["back_wave"])}},
                     {"appearance": "dark", "value": {"solid": ext_srgb(dark["back_wave"])}}],
                 "opacity-specializations": [
                     {"value": 1.0},
                     {"appearance": "tinted", "value": PALETTE["tinted_back_wave_opacity"]}]},
            ],
        }],
        "supported-platforms": {"squares": "shared"},
    }
    write(os.path.join(icon_dir, "icon.json"), json.dumps(icon_json, indent=2) + "\n")

    # --- Flat macOS reference export; app packaging compiles AppIcon.icon with actool ---
    light_mac = flat_svg("light", shadow=True)
    icns = os.path.join(out, "macos", "AppIcon.icns")
    png(light_mac, 1024).save(icns, sizes=[(16, 16), (32, 32), (64, 64), (128, 128),
                                           (256, 256), (512, 512), (1024, 1024)])

    # --- Windows (.ico, light only: Windows does not switch app icons by theme) ----------
    light_flat = flat_svg("light")
    big = png(light_flat, 256)
    sizes = [16, 20, 24, 32, 40, 48, 64, 256]
    frames = [png(light_flat, s) for s in sizes]
    os.makedirs(os.path.join(out, "windows"), exist_ok=True)
    big.save(os.path.join(out, "windows", "auditorylab.ico"), sizes=[(s, s) for s in sizes],
             append_images=frames)

    # --- Linux (hicolor theme, light only) ------------------------------------------------
    name = "org.resoltico.auditorylab"
    write(os.path.join(out, "linux", "hicolor", "scalable", "apps", f"{name}.svg"), light_flat)
    for s in (16, 24, 32, 48, 64, 128, 256, 512):
        d = os.path.join(out, "linux", "hicolor", f"{s}x{s}", "apps")
        os.makedirs(d, exist_ok=True)
        png(light_flat, s).save(os.path.join(d, f"{name}.png"))

    # --- Runtime window icon (Slint Window.icon, Windows/Linux title bar and taskbar) -----
    os.makedirs(os.path.join(out, "runtime"), exist_ok=True)
    png(light_flat, 512).save(os.path.join(out, "runtime", "window-icon.png"))

    # --- Flat reference artwork --------------------------------------------------------------
    write(os.path.join(out, "artwork", "auditorylab-icon-light.svg"), light_flat)
    write(os.path.join(out, "artwork", "auditorylab-icon-dark.svg"), flat_svg("dark"))

if __name__ == "__main__":
    build(sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "..", "build"))
