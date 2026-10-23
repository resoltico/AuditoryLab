#!/usr/bin/env python3
"""Re-measures the icon palette: WCAG 2.x contrast of each wave against the background and
between the two waves, for normal vision and simulated protanopia, deuteranopia and
tritanopia (Machado, Oliveira & Fernandes 2009, severity 1.0). Requires numpy."""
import json, os, numpy as np
HERE = os.path.dirname(os.path.abspath(__file__))
PAL = json.load(open(os.path.join(HERE, "palette.json")))
M = {"normal": np.eye(3),
     "protanopia": np.array([[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]]),
     "deuteranopia": np.array([[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]]),
     "tritanopia": np.array([[1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602], [0.004733, 0.691367, 0.303900]])}
def lin(h):
    c = np.array([int(h[i:i + 2], 16) / 255 for i in (1, 3, 5)])
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)
def lum(rgb): return float(0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2])
def ratio(a, b): a, b = max(a, b), min(a, b); return (a + 0.05) / (b + 0.05)
ok = True
for mode in ("light", "dark"):
    p = PAL[mode]; print(f"{mode}: background {p['background']}, front {p['front_wave']}, back {p['back_wave']}")
    for name, m in M.items():
        bg, f, b = (lum(np.clip(m @ lin(p[k]), 0, 1)) for k in ("background", "front_wave", "back_wave"))
        fb, bb, ff = ratio(f, bg), ratio(b, bg), ratio(f, b)
        ok &= min(fb, bb) >= 4.5 and ff >= 2.0
        print(f"  {name:13} front/bg {fb:5.2f}  back/bg {bb:5.2f}  front/back {ff:4.2f}")
print("PASS" if ok else "FAIL", "(waves >= 4.5:1 against the background; front/back >= 2.0 in every simulation)")
raise SystemExit(0 if ok else 1)
