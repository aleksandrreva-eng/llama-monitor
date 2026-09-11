"""Generate simple placeholder PNG icons for Tauri without external deps."""
import struct
import zlib
import os

OUT = os.path.dirname(os.path.abspath(__file__))


def make_png(size, path):
    # Build a rounded-square icon with a "ll" monogram-ish shape (bars).
    w = h = size
    # background color (dark slate)
    bg = (30, 40, 60, 255)
    fg = (96, 205, 255)  # accent blue
    # accent bar color
    accent = (16, 124, 16)

    def px(x, y, c):
        return c

    # We'll construct raw RGBA pixels.
    raw = bytearray()
    for y in range(h):
        raw.append(0)  # filter type 0
        for x in range(w):
            # rounded rect background
            r = size * 0.18
            inside = True
            # corners
            def in_corner(cx, cy):
                dx = x - cx
                dy = y - cy
                return dx * dx + dy * dy <= r * r
            tl_x, tl_y = r, r
            tr_x, tr_y = w - r, r
            bl_x, bl_y = r, h - r
            br_x, br_y = w - r, h - r
            if x < r and y < r and not in_corner(tl_x, tl_y):
                inside = False
            if x > w - r and y < r and not in_corner(tr_x, tr_y):
                inside = False
            if x < r and y > h - r and not in_corner(bl_x, bl_y):
                inside = False
            if x > w - r and y > h - r and not in_corner(br_x, br_y):
                inside = False

            if not inside:
                raw.extend((0, 0, 0, 0))
                continue

            # draw two vertical bars "II"
            bar_w = size * 0.14
            gap = size * 0.10
            x1 = size * 0.28
            x2 = size * 0.56
            top = size * 0.28
            bot = size * 0.72
            is_bar = False
            if x1 <= x <= x1 + bar_w and top <= y <= bot:
                is_bar = True
                c = fg
            elif x2 <= x <= x2 + bar_w and top <= y <= bot:
                is_bar = True
                c = accent
            if is_bar:
                # anti-alias-ish edge
                raw.extend(c)
            else:
                raw.extend(bg)

    # Build PNG
    def chunk(typ, data):
        c = struct.pack(">I", len(data)) + typ + data
        c += struct.pack(">I", zlib.crc32(typ + data) & 0xffffffff)
        return c

    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)
    idat = zlib.compress(bytes(raw), 9)
    png = sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


sizes = {
    "32x32.png": 32,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "icon.icns": None,  # platform-specific, skip
}

for name, size in sizes.items():
    if size:
        make_png(size, os.path.join(OUT, name))
        print(f"wrote {name} ({size}x{size})")

# Create a minimal placeholder icns stub (Tauri will warn but continue).
with open(os.path.join(OUT, "icon.icns"), "wb") as f:
    f.write(b"")
print("wrote empty icon.icns (placeholder)")
