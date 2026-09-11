"""Generate the app icons with Pillow (valid PNG + multi-size ICO).

Design: a rounded-square "squircle" with a subtle vertical gradient, and a
monogram of three ascending bars (a monitoring/graph motif) in the Windows 11
accent blue. Renders crisply at 16-256 px.
"""

import os

from PIL import Image, ImageDraw

OUT = os.path.dirname(os.path.abspath(__file__))

SS = 8  # supersampling factor for smooth edges


def rounded_mask(size, radius_ratio=0.22):
    s = size * SS
    mask = Image.new("L", (s, s), 0)
    d = ImageDraw.Draw(mask)
    r = int(s * radius_ratio)
    d.rounded_rectangle((0, 0, s - 1, s - 1), radius=r, fill=255)
    return mask.resize((size, size), Image.LANCZOS)


def gradient(size, top, bottom):
    s = size * SS
    img = Image.new("RGB", (s, s))
    d = ImageDraw.Draw(img)
    for y in range(s):
        t = y / max(1, s - 1)
        c = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
        d.line([(0, y), (s, y)], fill=c)
    return img.resize((size, size), Image.LANCZOS)


def bar_icon(size):
    """Rounded square + three ascending bars (monitoring motif)."""
    base = gradient(size, (37, 99, 165), (24, 62, 110))  # Win11-ish blues
    img = base.convert("RGBA")

    # Bars drawn at supersampled scale for crisp edges.
    s = size * SS
    overlay = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(overlay)

    accent = (96, 205, 255, 255)      # light accent
    accent_dim = (96, 205, 255, 170)

    # Three bars, ascending, with rounded caps.
    bar_w = s * 0.15
    gap = s * 0.085
    total = bar_w * 3 + gap * 2
    x0 = (s - total) / 2
    bottoms = s * 0.74
    heights = [0.24, 0.40, 0.56]
    colors = [accent_dim, accent, accent]
    for i, (hh, col) in enumerate(zip(heights, colors)):
        x = x0 + i * (bar_w + gap)
        top = bottoms - s * hh
        d.rounded_rectangle(
            (x, top, x + bar_w, bottoms),
            radius=bar_w / 2,
            fill=col,
        )
    # Baseline.
    d.rounded_rectangle(
        (s * 0.18, bottoms + s * 0.04, s * 0.82, bottoms + s * 0.07),
        radius=s * 0.015,
        fill=(255, 255, 255, 90),
    )

    overlay = overlay.resize((size, size), Image.LANCZOS)
    img.alpha_composite(overlay)

    mask = rounded_mask(size)
    img.putalpha(mask)
    return img


def main():
    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = {sz: bar_icon(sz) for sz in sizes}

    # Individual PNGs Tauri expects.
    images[32].save(os.path.join(OUT, "32x32.png"))
    images[128].save(os.path.join(OUT, "128x128.png"))
    images[256].save(os.path.join(OUT, "128x128@2x.png"))

    # Multi-resolution ICO (Pillow writes proper DIB/PNG entries).
    images[256].save(
        os.path.join(OUT, "icon.ico"),
        format="ICO",
        sizes=[(sz, sz) for sz in sizes],
    )

    # A 256px PNG handy for docs/store listing.
    images[256].save(os.path.join(OUT, "icon.png"))

    print("icons written:", ", ".join(f"{sz}px" for sz in sizes))


if __name__ == "__main__":
    main()
