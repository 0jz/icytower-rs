"""Extract sprites (PNG) and sounds (WAV) from Icy Tower 1.5 Allegro 4 datafiles.

usage: python -I tools/extract.py <game dir> <out dir>
"""
import struct, sys, wave
from pathlib import Path
from PIL import Image

KEY = b"CHEESE"  # recovered from known-plaintext of the "ALL." header; applies from byte 4 on
PACK, NOPACK = b"slh!", b"slh."


def decrypt(raw):
    if raw[:4] in (PACK, NOPACK):
        return raw
    head = PACK if raw[3] ^ 0x3A == PACK[3] else NOPACK
    return head + bytes(b ^ KEY[(i + 4) % 6] for i, b in enumerate(raw[4:]))


def lzss(src):
    N, F, T = 4096, 18, 2
    text, r, out, flags, i = bytearray(N), N - F, bytearray(), 0, 0
    while i < len(src):
        flags >>= 1
        if not flags & 0x100:
            flags = src[i] | 0xFF00
            i += 1
            if i >= len(src):
                break
        if flags & 1:
            c = src[i]; i += 1
            out.append(c); text[r] = c; r = (r + 1) & (N - 1)
        else:
            if i + 1 >= len(src):
                break
            a, b = src[i], src[i + 1]; i += 2
            pos, ln = a | ((b & 0xF0) << 4), (b & 0x0F) + T
            for k in range(ln + 1):
                c = text[(pos + k) & (N - 1)]
                out.append(c); text[r] = c; r = (r + 1) & (N - 1)
    return bytes(out)


class Reader:
    def __init__(self, data):
        self.d, self.p = data, 0

    def take(self, n):
        b = self.d[self.p:self.p + n]; self.p += n; return b

    def l(self): return struct.unpack(">i", self.take(4))[0]
    def w(self): return struct.unpack(">h", self.take(2))[0]


def parse(r, out, prefix, pal):
    count = r.l()
    for _ in range(count):
        props = {}
        while True:
            typ = r.take(4)
            if typ != b"prop":
                break
            ptype = r.take(4).decode("latin1"); size = r.l()
            props[ptype] = r.take(size).decode("latin1", "replace")
        fsize, dsize = r.l(), r.l()
        data = lzss(r.take(-fsize))[:dsize] if fsize < 0 else r.take(fsize)
        name = props.get("NAME", "unnamed").replace("/", "_")
        kind = typ.decode("latin1").strip()
        print(f"{prefix}{name:32} {kind:5} {dsize:8}  {props.get('ORIG', '')}")
        if kind == "FILE":
            sub = data[4:] if data[:4] == b"ALL." else data
            (out / name).mkdir(parents=True, exist_ok=True)
            parse(Reader(sub), out / name, prefix + "  ", pal)
        elif kind == "PAL":
            # 256 entries of r, g, b, pad, each 0..63
            pal[:] = [min(255, data[i] * 4) for i in range(1024) if i % 4 != 3]
        elif kind in ("BMP", "RLE"):
            img = bitmap(Reader(data), kind, pal)
            if img: img.save(out / f"{name}.png")
        elif kind == "FONT":
            font(Reader(data), out / name, pal)
        elif kind == "SAMP":
            sample(Reader(data), out / f"{name}.wav")
        else:
            (out / f"{name}.{kind.lower() or 'bin'}").write_bytes(data)


def bitmap(r, kind, pal):
    bits, w, h = r.w(), r.w(), r.w()
    if kind == "RLE":
        r.l()
        return rle(r, abs(bits), w, h, pal)
    if bits == -32:  # only -32 carries alpha; plain 32-bit is stored as 24-bit RGB
        return Image.frombytes("RGBA", (w, h), r.take(w * h * 4))
    bits = abs(bits)
    if bits == 8:
        img = Image.frombytes("P", (w, h), r.take(w * h)); img.putpalette(pal); img.info["transparency"] = 0
        return img.convert("RGBA")
    if bits in (15, 16):
        px = struct.unpack(f"<{w*h}H", r.take(w * h * 2))
        rgb = bytearray()
        for c in px:
            rgb += bytes(((c >> 11 & 31) * 255 // 31, (c >> 5 & 63) * 255 // 63, (c & 31) * 255 // 31))
        return magic_pink(Image.frombytes("RGB", (w, h), bytes(rgb)))
    if bits in (24, 32):
        return magic_pink(Image.frombytes("RGB", (w, h), r.take(w * h * 3)))
    print("   unsupported bitmap depth", bits)


def rle(r, bits, w, h, pal):
    # 8-bit RLE: signed count bytes; >0 literal run, <0 transparent skip, 0 end of line
    if bits != 8:
        print("   unsupported RLE depth", bits); return None
    img = Image.new("RGBA", (w, h))
    for y in range(h):
        x = 0
        while True:
            n = struct.unpack("b", r.take(1))[0]
            if n == 0: break
            if n < 0: x -= n; continue
            for c in r.take(n):
                img.putpixel((x, y), (pal[c*3], pal[c*3+1], pal[c*3+2], 255)); x += 1
    return img


def font(r, path, pal):
    # Allegro 4.2 font: 0, ranges, then per range: mono flag, first, last, glyphs (w, h, pixels)
    r.w()
    glyphs = []
    for _ in range(r.w()):
        mono = r.take(1)[0]
        first, last = r.l(), r.l()
        for _ in range(first, last + 1):
            w, h = r.w(), r.w()
            g = Image.new("RGBA", (w, h))
            if mono:
                rows = r.take((w + 7) // 8 * h)
                for y in range(h):
                    for x in range(w):
                        if rows[y * ((w + 7) // 8) + x // 8] & (0x80 >> (x % 8)):
                            g.putpixel((x, y), (255, 255, 255, 255))
            else:
                px = r.take(w * h)
                for i, c in enumerate(px):
                    if c:  # colour 0 is transparent
                        g.putpixel((i % w, i // w), (pal[c*3], pal[c*3+1], pal[c*3+2], 255))
            glyphs.append(g)
    # one strip of glyphs for chars 32..127 plus their widths, read by the game
    strip = Image.new("RGBA", (sum(g.width for g in glyphs), max(g.height for g in glyphs)))
    x = 0
    for g in glyphs:
        strip.alpha_composite(g, (x, 0)); x += g.width
    strip.save(f"{path}.png")
    Path(f"{path}.widths").write_text(" ".join(str(g.width) for g in glyphs))


def magic_pink(img):
    # Allegro's transparent colour is magenta (255,0,255)
    img = img.convert("RGBA")
    px = img.load()
    for y in range(img.height):
        for x in range(img.width):
            if px[x, y][:3] == (255, 0, 255):
                px[x, y] = (0, 0, 0, 0)
    return img


def sample(r, path):
    bits, freq, length = r.w(), r.w() & 0xFFFF, r.l()
    ch = 2 if bits < 0 else 1
    bits = abs(bits)
    with wave.open(str(path), "wb") as wv:
        wv.setnchannels(ch); wv.setsampwidth(bits // 8); wv.setframerate(freq)
        data = r.take(length * ch * bits // 8)
        if bits == 16:  # Allegro stores unsigned 16-bit; WAV wants signed
            data = b"".join(struct.pack("<h", (v ^ 0x8000) - 0x10000 if v ^ 0x8000 > 0x7FFF else v ^ 0x8000)
                            for (v,) in struct.iter_unpack("<H", data))
        wv.writeframes(data)


def extract(path, out):
    raw = decrypt(path.read_bytes())
    body = lzss(raw[4:]) if raw[:4] == PACK else raw[4:]
    if body[:4] != b"ALL.":
        print(f"== {path}: unknown encryption, skipped"); return
    out.mkdir(parents=True, exist_ok=True)
    print(f"== {path}")
    parse(Reader(body[4:]), out, "  ", [i % 256 for i in range(768)])


if __name__ == "__main__":
    game, out = Path(sys.argv[1]), Path(sys.argv[2])
    for f in sorted(game.glob("data/*.dat")) + sorted(game.glob("characters/*/*.dat")):
        extract(f, out / f.stem)
