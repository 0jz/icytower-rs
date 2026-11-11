"""Build assets_hd/ from assets/: every sprite upscaled 4x with Scale2x applied twice (AdvMAME4x).

Scale2x only copies existing pixels, so outlines get rounded corners without blur or new colours,
and transparent edges stay clean. The game draws HD sprites at their original size, so layout and
hit boxes don't change.

    python -I tools/upscale.py assets assets_hd
"""
import sys
from pathlib import Path

import numpy as np
from PIL import Image


def scale2x(a: np.ndarray) -> np.ndarray:
    """a: HxW uint32 (packed RGBA). Returns 2Hx2W."""
    p = np.pad(a, 1, mode="edge")
    b, d, e, f, h = p[:-2, 1:-1], p[1:-1, :-2], p[1:-1, 1:-1], p[1:-1, 2:], p[2:, 1:-1]
    e0 = np.where((d == b) & (b != f) & (d != h), d, e)
    e1 = np.where((b == f) & (b != d) & (f != h), f, e)
    e2 = np.where((d == h) & (d != b) & (h != f), d, e)
    e3 = np.where((h == f) & (d != h) & (b != f), f, e)
    out = np.empty((a.shape[0] * 2, a.shape[1] * 2), dtype=a.dtype)
    out[0::2, 0::2], out[0::2, 1::2], out[1::2, 0::2], out[1::2, 1::2] = e0, e1, e2, e3
    return out


def upscale(src: Path, dst: Path, times: int = 2) -> None:
    rgba = np.ascontiguousarray(np.asarray(Image.open(src).convert("RGBA")))
    packed = rgba.view(np.uint32)[..., 0]
    for _ in range(times):
        packed = scale2x(packed)
    out = packed[..., None].view(np.uint8).reshape(packed.shape + (4,))
    dst.parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(out, "RGBA").save(dst)


def main() -> None:
    src_root, dst_root = Path(sys.argv[1]), Path(sys.argv[2])
    files = sorted(src_root.glob("*/*.png"))
    for f in files:
        upscale(f, dst_root / f.relative_to(src_root))
    print(f"{len(files)} sprites -> {dst_root}")


def _check() -> None:
    # an outside corner gets its tip cut; a flat area just grows
    a = np.array([[1, 0], [0, 0]], dtype=np.uint32)
    assert (scale2x(a) == np.array([[1, 1, 0, 0], [1, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]])).all()
    a = np.array([[1, 1, 0], [1, 0, 0], [0, 0, 0]], dtype=np.uint32)
    assert scale2x(a)[2, 2] == 1 and scale2x(a)[3, 3] == 0  # the corner pixel at (1,1) rounds in


if __name__ == "__main__":
    _check()
    main()
