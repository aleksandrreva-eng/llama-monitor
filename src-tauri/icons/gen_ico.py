"""Generate a proper multi-size .ico file for Tauri's Windows resource file.

The classic Windows resource compiler (rc.exe) does not accept PNG-in-ICO,
so we render the icon pixels ourselves and emit DIB (BI_RGB, 32bpp) entries.
"""
import struct
import os

OUT = os.path.dirname(os.path.abspath(__file__))

BG = (30, 40, 60, 255)
FG = (96, 205, 255)
ACCENT = (16, 124, 16)

SIZES = [16, 32, 48, 128, 256]


def render(size):
    """Return (width, height, rgba_bytes) for the icon at the given size."""
    w = h = size
    r = size * 0.18
    bar_w = size * 0.14
    x1 = size * 0.28
    x2 = size * 0.56
    top = size * 0.28
    bot = size * 0.72

    def in_corner(cx, cy, x, y):
        dx = x - cx
        dy = y - cy
        return dx * dx + dy * dy <= r * r

    def rounded_inside(x, y):
        if x < r and y < r and not in_corner(r, r, x, y):
            return False
        if x > w - r and y < r and not in_corner(w - r, r, x, y):
            return False
        if x < r and y > h - r and not in_corner(r, h - r, x, y):
            return False
        if x > w - r and y > h - r and not in_corner(w - r, h - r, x, y):
            return False
        return True

    def is_bar(x, y):
        if x1 <= x <= x1 + bar_w and top <= y <= bot:
            return FG
        if x2 <= x <= x2 + bar_w and top <= y <= bot:
            return ACCENT
        return None

    rgba = bytearray()
    for y in range(h):
        row = bytearray()
        for x in range(w):
            if not rounded_inside(x, y):
                row += bytes((0, 0, 0, 0))
                continue
            c = is_bar(x, y)
            row += bytes(c if c else BG)
        # Pad row to a 4-byte boundary for DIB alignment.
        pad = (4 - (len(row) % 4)) % 4
        row += bytes(pad)
        rgba += row
    return w, h, bytes(rgba)


def dib_entry(width, height, rgba):
    """Build one ICONDIR image entry + DIB data."""
    row_stride = ((width * 32 + 31) // 32) * 4
    pixel_size = row_stride * height
    and_size = row_stride * height

    header = struct.pack(
        "<IiiHHIIiiII",
        40,
        width,
        height * 2,
        1,
        32,
        0,
        pixel_size,
        0, 0,
        0, 0,
    )

    bgra = bytearray()
    for i in range(0, len(rgba), 4):
        r, g, b, a = rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]
        bgra += bytes((b, g, r, a))

    and_mask = bytes(and_size)
    return header + bytes(bgra) + and_mask


def main():
    entries = []
    data = bytearray()
    offset = 6 + 16 * len(SIZES)
    for size in SIZES:
        w, h, rgba = render(size)
        dib = dib_entry(w, h, rgba)
        entries.append(struct.pack(
            "<BBBBHHII",
            w & 0xFF,
            h & 0xFF,
            0, 0, 1, 32,
            len(dib),
            offset,
        ))
        data += dib
        offset += len(dib)

    header = struct.pack("<HHH", 0, 1, len(SIZES))
    ico = header + b"".join(entries) + bytes(data)
    out_path = os.path.join(OUT, "icon.ico")
    with open(out_path, "wb") as f:
        f.write(ico)
    print(f"wrote {out_path} ({len(ico)} bytes, {len(SIZES)} images)")


if __name__ == "__main__":
    main()
