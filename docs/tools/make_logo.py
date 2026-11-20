"""Draw the Nookly logo: assets/nookly.png (512 px) and assets/nookly.ico.

A bold lowercase "n" in warm amber on a dark ink tile; its arch is the nook,
and a small teal light sits tucked inside it. The tray icon in
src/gfx.rs (make_tray_icon) draws the same shapes on the same 32-unit grid,
so change both together.

    python docs/tools/make_logo.py
"""

from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent.parent

# Geometry on a 32 x 32 grid (same as the tray icon).
TILE = (1.0, 1.0, 31.0, 31.0, 8.0)  # left, top, right, bottom, radius
LEGS = (10.0, 22.0)  # x of the n's two legs
ARCH_Y, FOOT_Y, ARCH_R = 15.0, 25.0, 6.0  # arch centre y, leg bottom, radius
STROKE = 4.8
DOT = (16.0, 22.4, 2.5)  # x, y, radius

INK_TOP, INK_BOTTOM = np.array([42, 38, 62]), np.array([17, 16, 25])
AMBER_TOP, AMBER_BOTTOM = np.array([255, 196, 74]), np.array([255, 128, 56])
TEAL = np.array([92, 225, 210])


def n_distance(x, y):
    """Distance from a point to the n's centre line."""
    cx = (LEGS[0] + LEGS[1]) / 2
    arc = np.abs(np.hypot(x - cx, y - ARCH_Y) - ARCH_R)
    arc = np.where(y <= ARCH_Y, arc, np.inf)
    d = arc
    for lx in LEGS:
        leg = np.where((y >= ARCH_Y) & (y <= FOOT_Y), np.abs(x - lx), np.hypot(x - lx, y - np.clip(y, ARCH_Y, FOOT_Y)))
        d = np.minimum(d, leg)
    return d


def tile_inside(x, y):
    l, t, r, b, rad = TILE
    cx, cy = np.clip(x, l + rad, r - rad), np.clip(y, t + rad, b - rad)
    return (x >= l) & (x <= r) & (y >= t) & (y <= b) & (np.hypot(x - cx, y - cy) <= rad)


def render(size, ss=4):
    """RGBA image of the logo at `size` px, supersampled ss x ss."""
    n = size * ss
    g = (np.arange(n) + 0.5) / n * 32.0
    x, y = np.meshgrid(g, g)
    t = (y / 32.0)[..., None]
    rgb = INK_TOP * (1 - t) + INK_BOTTOM * t
    alpha = tile_inside(x, y).astype(np.float32)
    # A soft teal glow around the light.
    glow = np.exp(-(np.hypot(x - DOT[0], y - DOT[1]) / 4.5) ** 2)[..., None] * 0.35
    rgb = rgb * (1 - glow) + TEAL * glow
    on_n = (n_distance(x, y) <= STROKE / 2)[..., None]
    ta = np.clip((y - (ARCH_Y - ARCH_R)) / (FOOT_Y - ARCH_Y + ARCH_R), 0, 1)[..., None]
    rgb = np.where(on_n, AMBER_TOP * (1 - ta) + AMBER_BOTTOM * ta, rgb)
    on_dot = (np.hypot(x - DOT[0], y - DOT[1]) <= DOT[2])[..., None]
    rgb = np.where(on_dot, TEAL, rgb)
    img = np.concatenate([rgb, alpha[..., None] * 255], -1)
    # Average the samples (colour weighted by coverage).
    img = img.reshape(size, ss, size, ss, 4)
    a = img[..., 3:4]
    col = (img[..., :3] * a).sum((1, 3)) / np.maximum(a.sum((1, 3)), 1e-6)
    out = np.concatenate([col, a.mean((1, 3))], -1)
    return Image.fromarray(out.clip(0, 255).astype(np.uint8), "RGBA")


def main():
    render(512).save(ROOT / "assets" / "nookly.png")
    render(256).save(ROOT / "docs" / "images" / "logo.png")  # the website
    sizes = [16, 24, 32, 48, 64, 128, 256]
    # Each size drawn on its own, so the small ones stay sharp.
    imgs = [render(s, 8 if s <= 48 else 4) for s in sizes]
    imgs[-1].save(ROOT / "assets" / "nookly.ico", sizes=[(s, s) for s in sizes], append_images=imgs[:-1])
    print("assets/nookly.png, assets/nookly.ico")


if __name__ == "__main__":
    main()
