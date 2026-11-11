"""Plan C: trace sprites into smooth 4x art for assets_hd/.

Each sprite is split into colour regions (median-cut palette, transparent = its own region).
Every region's mask is upscaled and blurred, and each 4x pixel goes to the region that is
strongest there, so region borders become smooth curves instead of pixel steps (a traced,
vector-like look). Inside a region the colour is interpolated only from that region's own
pixels, so shading blends smoothly without bleeding across outlines.

    python -I tools/trace.py assets assets_hd              # gameplay sprites (first pass)
    python -I tools/trace.py assets assets_hd "data/*.png" # any globs under assets/
"""
import os
import sys
from pathlib import Path

import numpy as np
from PIL import Image
from scipy.ndimage import distance_transform_edt, gaussian_filter, zoom

SCALE = 4
SIGMA = 1.6  # contour smoothing, in 4x pixels: higher = rounder shapes, but thin details fade
MAX_COLOURS = 32  # regions per sprite; more keeps detail, fewer gives cleaner shapes
GRADIENT = 2.0  # width in 4x pixels of the colour blend along region borders (0 = hard; 4+ softens outlines)

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


def shrink2(img: np.ndarray) -> np.ndarray:
    """Halve an RGBA image by averaging 2x2 blocks (alpha-weighted): anti-aliases traced edges."""
    h, w = img.shape[0] // 2, img.shape[1] // 2
    f = img.astype(np.float32).reshape(h, 2, w, 2, 4)
    a = f[..., 3:4]
    rgb = (f[..., :3] * a).sum(axis=(1, 3)) / np.maximum(a.sum(axis=(1, 3)), 1e-6)
    out = np.concatenate([rgb, a.mean(axis=(1, 3))], axis=-1)
    return np.clip(out + 0.5, 0, 255).astype(np.uint8)


def trace(rgba: np.ndarray, wrap: bool = False, max_colours: int = MAX_COLOURS, sigma: float = SIGMA,
          flat: bool = False, split: float | None = None, ink: bool = False, scale: int = SCALE,
          level: float = 0.5, light_bias: float = 1.0, gradient: float = 0.0) -> np.ndarray:
    """flat: fill each region with one solid colour instead of shading it from its own pixels.
    scale: output size multiple; sigma is in output pixels.
    level: where the shape's edge falls in the blurred mask (lower = thicker letters).
    light_bias: >1 favours the lightest region where it meets darker ones (thicker white fill).
    ink: dark text whose light pixels are only edge softening: darkness = how much ink a pixel
    holds, and the result is one dark colour (no pale halo)."""
    zmode, gmode = ("grid-wrap", "wrap") if wrap else ("nearest", "nearest")
    up = lambda a: gaussian_filter(zoom(a, scale, order=1, mode=zmode, grid_mode=True), sigma, mode=gmode)
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
    shape = up(cover) >= level
    lightest = lab.max()
    best = np.full((h * scale, w * scale), -1.0, np.float32)
    out = np.zeros((h * scale, w * scale, 4), np.float32)
    owner = np.zeros((h * scale, w * scale), np.int32)  # which region owns each output pixel (0 = none)
    for l in np.unique(lab):
        if l == 0:
            continue
        m = (lab == l).astype(np.float32)
        weight = up(m) * (light_bias if l == lightest and lightest > 1 else 1.0)
        win = (weight > best) & shape
        best[win] = weight[win]
        owner[win] = l
        if flat:
            out[..., :3][win] = np.median(rgb[lab == l], axis=0)
        else:
            den = np.maximum(weight, 1e-6)
            for c in range(3):
                out[..., c][win] = (up(rgb[..., c] * m) / den)[win]
        out[..., 3][win] = 255.0
    if gradient > 0:
        out = blend_borders(out, owner, gradient, wrap)
    return np.clip(out + 0.5, 0, 255).astype(np.uint8)


def blend_borders(out: np.ndarray, owner: np.ndarray, width: float, wrap: bool = False) -> np.ndarray:
    """In-between colours: near every border between two regions (or a region and transparency),
    fade toward a blurred mix of both sides; the further from the border, the less."""
    edge = np.zeros(owner.shape, bool)
    dx = owner[:, 1:] != owner[:, :-1]
    dy = owner[1:, :] != owner[:-1, :]
    edge[:, 1:] |= dx
    edge[:, :-1] |= dx
    edge[1:, :] |= dy
    edge[:-1, :] |= dy
    d = distance_transform_edt(~edge)
    t = np.clip(1.0 - d / width, 0.0, 1.0)
    t = (t * t * (3.0 - 2.0 * t))[..., None]  # smoothstep
    mode = "wrap" if wrap else "nearest"
    a = gaussian_filter(out[..., 3], width / 2, mode=mode)
    rgb = np.stack([gaussian_filter(out[..., c] * out[..., 3], width / 2, mode=mode) for c in range(3)], axis=-1)
    rgb = rgb / np.maximum(a, 1e-6)[..., None]
    mixed = np.concatenate([rgb, a[..., None]], axis=-1)
    blended = out * (1.0 - t) + mixed * t
    # keep the original colour where the blurred side is fully transparent
    blended[..., :3] = np.where(blended[..., 3:4] > 0, blended[..., :3], out[..., :3])
    return blended


FONT_SPLIT = 170  # letters are just dark (outline, inner lines) or light (fill), by brightness
FONT_SIGMA = 1.5  # outlined fonts
THIN_SIGMA = 1.2  # fonts under 20px tall have 1px strokes; round them less so they don't break
FONT_SOFTEN = 1.5  # gradient width at colour edges, in 4x pixels (0 = hard edges; 4 = one original pixel)
# thickness calibration, measured so HD letters cover the same area as the originals
# (tools/trace.py --check-fonts prints the ratios)
FONT_LEVEL = {"thin": 0.45, "outlined": 0.5}
FONT_LIGHT_BIAS = 1.75
PAD = 3


def soften(img: np.ndarray, sigma: float) -> np.ndarray:
    """Gaussian blur with premultiplied alpha, so transparent pixels don't darken the edges."""
    if sigma <= 0:
        return img
    f = img.astype(np.float32)
    a = gaussian_filter(f[..., 3], sigma)
    rgb = np.stack([gaussian_filter(f[..., c] * f[..., 3], sigma) for c in range(3)], axis=-1) / np.maximum(a, 1e-6)[..., None]
    return np.clip(np.concatenate([rgb, a[..., None]], axis=-1) + 0.5, 0, 255).astype(np.uint8)


def trace_font(rgba: np.ndarray, widths: list[int], ink: bool = False) -> np.ndarray:
    """A font strip, one glyph at a time so no letter bleeds into its neighbour's cell.
    ink: plain dark letters (FONT_MED_BLACK) rather than light letters with a dark outline.

    Most original glyphs touch their cell's edges, so each HD glyph keeps a transparent gutter of
    PAD original pixels on every side for its soft edge (main.rs FONT_GUTTER draws it):
    glyph i sits at x = 4*cell_x + 2*G*i, the strip is 4*height + 2*G tall, G = PAD*SCALE."""
    h = rgba.shape[0]
    g = PAD * SCALE
    sigma = THIN_SIGMA if h < 20 else FONT_SIGMA
    level = FONT_LEVEL["thin" if h < 20 else "outlined"]
    out = np.zeros((h * SCALE + 2 * g, rgba.shape[1] * SCALE + 2 * g * len(widths), 4), np.uint8)
    x = 0
    for i, w in enumerate(widths):
        cell = np.zeros((h + 2 * PAD, w + 2 * PAD, 4), np.uint8)  # transparent margin to round into
        cell[PAD:PAD + h, PAD:PAD + w] = rgba[:, x:x + w]
        if cell[..., 3].any():
            # solid fill/outline traced at 8x and halved (anti-aliased curves), then a slight blur
            # that brings back the original's soft step between fill, outline and background
            t = shrink2(trace(cell, sigma=2 * sigma, flat=True, split=FONT_SPLIT, ink=ink, scale=2 * SCALE,
                              level=level, light_bias=FONT_LIGHT_BIAS))
            t = soften(t, FONT_SOFTEN / 2 if h < 20 else FONT_SOFTEN)  # thin strokes blur away faster
            ox = x * SCALE + 2 * g * i
            out[:, ox:ox + t.shape[1]] = t
        x += w
    return out


def font_ratios(src: np.ndarray, hd: np.ndarray, widths: list[int], ink: bool) -> tuple[float, float | None]:
    """HD / original area of the whole letters and of their light fill (1.0 = same thickness)."""
    g = PAD * SCALE
    sd, t = src.astype(np.float32), hd.astype(np.float32)
    lum = lambda a: a[..., :3].mean(-1)
    on = sd[..., 3] >= 128
    if ink:
        a_sd = ((1 - lum(sd) / 255) * on).sum() * SCALE ** 2
        a_hd = ((1 - lum(t) / 255) * t[..., 3] / 255).sum()
        return a_hd / a_sd, None
    a_sd = on.sum() * SCALE ** 2
    l_sd = (on & (lum(sd) >= FONT_SPLIT)).sum() * SCALE ** 2
    a_hd = (t[..., 3] / 255).sum()
    l_hd = (t[..., 3] / 255 * (lum(t) >= FONT_SPLIT)).sum()
    return a_hd / a_sd, (l_hd / l_sd if l_sd else None)


def check_fonts(src_root: Path, dst_root: Path) -> None:
    for f in sorted(src_root.glob("data/FONT_*.png")):
        ws = [int(w) for w in f.with_suffix(".widths").read_text().split()]
        sd = np.asarray(Image.open(f).convert("RGBA"))
        hd = np.asarray(Image.open(dst_root / f.relative_to(src_root)).convert("RGBA"))
        total, fill = font_ratios(sd, hd, ws, "BLACK" in f.name)
        print(f"{f.name}: letters {total:.2f}" + (f", white fill {fill:.2f}" if fill else ""))


def main() -> None:
    src_root, dst_root = Path(sys.argv[1]), Path(sys.argv[2])
    if sys.argv[3:] == ["--check-fonts"]:
        return check_fonts(src_root, dst_root)
    files = sorted({f for g in (sys.argv[3:] or GAMEPLAY) for f in src_root.glob(g)})
    for f in files:
        rgba = np.asarray(Image.open(f).convert("RGBA"))
        widths = f.with_suffix(".widths")
        if widths.is_file():
            out = trace_font(rgba, [int(w) for w in widths.read_text().split()], ink="BLACK" in f.name)
        else:
            gradient = float(os.environ["TRACE_GRADIENT"]) if "TRACE_GRADIENT" in os.environ else GRADIENT
            out = trace(rgba, wrap=f.name.startswith(WRAPS), gradient=gradient)
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
