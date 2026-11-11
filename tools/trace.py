"""Plan C: trace sprites into smooth 4x art for assets_hd/.

Each sprite is split into colour regions (median-cut palette, transparent = its own region).
Every region's mask is upscaled and blurred, and each 4x pixel goes to the region that is
strongest there, so region borders become smooth curves instead of pixel steps (a traced,
vector-like look). Inside a region the colour is interpolated only from that region's own
pixels, so shading blends smoothly without bleeding across outlines.

    python -I tools/trace.py assets assets_hd              # gameplay sprites (first pass)
    python -I tools/trace.py assets assets_hd "data/*.png" # any globs under assets/
"""
import sys
from pathlib import Path

import numpy as np
from PIL import Image
from scipy.ndimage import gaussian_filter, zoom

SCALE = 4
SIGMA = 1.6  # contour smoothing, in 4x pixels: higher = rounder shapes, but thin details fade
MAX_COLOURS = 32  # regions per sprite; more keeps detail, fewer gives cleaner shapes

GAMEPLAY = [
    "data/BGTILE*.png", "data/SIDEBLOCK.png", "data/FLOOR*.png", "data/SIGN*.png",
    "data/CLOCK*.png", "data/COMBO_*.png", "data/STAR*.png", "data/REWARD*.png",
    "data/HURRYUP.png", "harold/*.png", "data/FONT_*.png",
]
# textures the game repeats edge to edge: smooth across the wrap so tiles stay seamless
WRAPS = ("BGTILE", "SIDEBLOCK")


def labels(rgba: np.ndarray, max_colours: int = MAX_COLOURS, split: float | None = None) -> np.ndarray:
    """0 = transparent, 1.. = colour regions. `split`: just dark (1) / light (2) by brightness."""
    opaque = rgba[..., 3] >= 128
    rgb = rgba[..., :3]
    if split is not None:
        lab = 1 + (rgb.astype(np.float32).mean(axis=2) >= split).astype(np.int32)
        lab[~opaque] = 0
        return lab
    n = min(max_colours, max(1, len(np.unique(rgb[opaque].reshape(-1, 3), axis=0))))
    pal = Image.fromarray(np.ascontiguousarray(rgb)).quantize(n, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
    lab = np.asarray(pal).astype(np.int32) + 1
    lab[~opaque] = 0
    return lab


def trace(rgba: np.ndarray, wrap: bool = False, max_colours: int = MAX_COLOURS, sigma: float = SIGMA,
          flat: bool = False, split: float | None = None, ink: bool = False) -> np.ndarray:
    """flat: fill each region with one solid colour (crisp text) instead of blended shading.
    ink: dark text whose light pixels are only edge softening: darkness = how much ink a pixel
    holds, and the result is one dark colour (no pale halo)."""
    zmode, gmode = ("grid-wrap", "wrap") if wrap else ("nearest", "nearest")
    up = lambda a: gaussian_filter(zoom(a, SCALE, order=1, mode=zmode, grid_mode=True), sigma, mode=gmode)
    lab = labels(rgba, max_colours, split)
    rgb = rgba[..., :3].astype(np.float32)
    h, w = lab.shape
    if ink:
        darkness = (1.0 - rgb.mean(axis=2) / 255.0) * (lab > 0)
        cover = darkness / max(float(darkness.max()), 1e-6)
        lab = np.where(lab > 0, 1, 0)  # one ink region, coloured like its darkest pixels
        ink_colour = np.median(rgb[darkness >= 0.5 * darkness.max()], axis=0)
        rgb = np.broadcast_to(ink_colour, rgb.shape).copy()
    else:
        cover = (lab > 0).astype(np.float32)
    # shape first: all visible pixels together vs transparency, so a stroke made of several
    # shades isn't split into pieces that each lose to the background
    shape = up(cover) >= 0.5
    best = np.full((h * SCALE, w * SCALE), -1.0, np.float32)
    out = np.zeros((h * SCALE, w * SCALE, 4), np.float32)
    for l in np.unique(lab):
        if l == 0:
            continue
        m = (lab == l).astype(np.float32)
        weight = up(m)
        win = (weight > best) & shape
        best[win] = weight[win]
        if flat:
            out[..., :3][win] = np.median(rgb[lab == l], axis=0)
        else:
            den = np.maximum(weight, 1e-6)
            for c in range(3):
                out[..., c][win] = (up(rgb[..., c] * m) / den)[win]
        out[..., 3][win] = 255.0
    return np.clip(out + 0.5, 0, 255).astype(np.uint8)


FONT_SPLIT = 170  # letters are just dark (outline, inner lines) or light (fill), by brightness
FONT_SIGMA = 1.5  # outlined fonts
THIN_SIGMA = 1.2  # fonts under 20px tall have 1px strokes; round them less so they don't break
PAD = 3


def trace_font(rgba: np.ndarray, widths: list[int], ink: bool = False) -> np.ndarray:
    """A font strip, one glyph at a time so no letter bleeds into its neighbour's cell.
    ink: plain dark letters (FONT_MED_BLACK) rather than light letters with a dark outline."""
    h = rgba.shape[0]
    sigma = THIN_SIGMA if h < 20 else FONT_SIGMA
    out = np.zeros((h * SCALE, rgba.shape[1] * SCALE, 4), np.uint8)
    x = 0
    for w in widths:
        cell = np.zeros((h + 2 * PAD, w + 2 * PAD, 4), np.uint8)  # transparent margin to round into
        cell[PAD:PAD + h, PAD:PAD + w] = rgba[:, x:x + w]
        if cell[..., 3].any():
            t = trace(cell, sigma=sigma, flat=True, split=FONT_SPLIT, ink=ink)
            p = PAD * SCALE
            out[:, x * SCALE:(x + w) * SCALE] = t[p:p + h * SCALE, p:p + w * SCALE]
        x += w
    return out


def main() -> None:
    src_root, dst_root = Path(sys.argv[1]), Path(sys.argv[2])
    files = sorted({f for g in (sys.argv[3:] or GAMEPLAY) for f in src_root.glob(g)})
    for f in files:
        rgba = np.asarray(Image.open(f).convert("RGBA"))
        widths = f.with_suffix(".widths")
        if widths.is_file():
            out = trace_font(rgba, [int(w) for w in widths.read_text().split()], ink="BLACK" in f.name)
        else:
            out = trace(rgba, wrap=f.name.startswith(WRAPS))
        dst = dst_root / f.relative_to(src_root)
        dst.parent.mkdir(parents=True, exist_ok=True)
        Image.fromarray(out, "RGBA").save(dst)
    print(f"{len(files)} sprites traced -> {dst_root}")


def _check() -> None:
    # a solid disc on transparency: 4x size, still a disc (centre filled, corner clear),
    # colour unchanged inside, and its outline is smoother than nearest-neighbour steps
    y, x = np.mgrid[:16, :16]
    disc = (x - 7.5) ** 2 + (y - 7.5) ** 2 < 36
    rgba = np.zeros((16, 16, 4), np.uint8)
    rgba[disc] = (200, 40, 40, 255)
    out = trace(rgba)
    assert out.shape == (64, 64, 4)
    assert tuple(out[32, 32]) == (200, 40, 40, 255) and out[0, 0, 3] == 0
    steps = lambda a: np.abs(np.diff((a[..., 3] > 0).sum(1))).sum()
    nearest = np.repeat(np.repeat(rgba, 4, 0), 4, 1)
    assert steps(out) <= steps(nearest)


if __name__ == "__main__":
    _check()
    main()
