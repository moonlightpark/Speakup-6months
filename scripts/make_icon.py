#!/usr/bin/env python3
"""Draws the SpeakUp 6M app icon (the mascot on a gradient tile) and writes
assets/icon/icon.png (1024 px), icon.ico (Windows) and, on macOS, icon.icns."""
import math, os, subprocess, sys, tempfile
from PIL import Image, ImageDraw, ImageFilter

S = 2048  # drawn at 2x, then downscaled for smooth edges
OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "icon")


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(len(a)))


def gradient(size, c1, c2):
    """Diagonal gradient, c1 top-left → c2 bottom-right."""
    w, h = size
    g = Image.new("RGBA", size)
    px = g.load()
    for y in range(h):
        for x in range(w):
            px[x, y] = lerp(c1, c2, (x + y) / (w + h - 2))
    return g


def rounded_mask(size, radius):
    m = Image.new("L", size, 0)
    ImageDraw.Draw(m).rounded_rectangle([0, 0, size[0] - 1, size[1] - 1], radius=radius, fill=255)
    return m


def glow(img, center, radius, color):
    layer = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    x, y = center
    d.ellipse([x - radius, y - radius, x + radius, y + radius], fill=color)
    layer = layer.filter(ImageFilter.GaussianBlur(radius * 0.45))
    return Image.alpha_composite(img, layer)


def draw():
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    # Tile (macOS-style margins: the artwork sits inside ~80% of the canvas).
    m = int(S * 0.09)
    tile = (S - 2 * m, S - 2 * m)
    bg = gradient((256, 256), (61, 123, 255, 255), (139, 92, 246, 255)).resize(tile, Image.BICUBIC)
    bg = glow(bg, (int(tile[0] * 0.82), int(tile[1] * 0.15)), int(tile[0] * 0.35), (236, 72, 153, 150))
    bg = glow(bg, (int(tile[0] * 0.12), int(tile[1] * 0.9)), int(tile[0] * 0.3), (34, 211, 238, 120))
    # Soft drop shadow under the tile
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle([m, m + S * 0.02, S - m, S - m + S * 0.02], radius=int(tile[0] * 0.225), fill=(0, 0, 0, 110))
    img = Image.alpha_composite(img, shadow.filter(ImageFilter.GaussianBlur(S * 0.02)))
    img.paste(bg, (m, m), rounded_mask(tile, int(tile[0] * 0.225)))

    d = ImageDraw.Draw(img)
    cx, cy, r = S * 0.46, S * 0.5, S * 0.25
    ink = (25, 26, 60, 255)
    # Sound waves (right side)
    waves = Image.new("RGBA", img.size, (0, 0, 0, 0))
    wd = ImageDraw.Draw(waves)
    for i in range(3):
        rr = r * (1.28 + i * 0.22)
        w = int(r * 0.085)
        alpha = 255 - i * 55
        wd.arc([cx - rr, cy - rr, cx + rr, cy + rr], start=-32, end=32, fill=(34, 211, 238, alpha), width=w)
    img = Image.alpha_composite(img, waves)
    d = ImageDraw.Draw(img)
    # Bubble tail
    d.polygon([(cx - r * 0.55, cy + r * 0.55), (cx - r * 1.0, cy + r * 1.02), (cx - r * 0.1, cy + r * 0.85)], fill=(98, 112, 255, 255))
    # Body: white-ish radial gradient circle
    body = gradient((256, 256), (150, 185, 255, 255), (150, 105, 250, 255)).resize((int(2 * r), int(2 * r)), Image.BICUBIC)
    bm = Image.new("L", body.size, 0)
    ImageDraw.Draw(bm).ellipse([0, 0, body.size[0] - 1, body.size[1] - 1], fill=255)
    img.paste(body, (int(cx - r), int(cy - r)), bm)
    # Translucent details go on their own layer and are blended in (ImageDraw would
    # otherwise overwrite the alpha channel and punch holes).
    over = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(over)
    # Rim light
    d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=(255, 255, 255, 90), width=int(r * 0.035))
    # Shine
    d.ellipse([cx - r * 0.66, cy - r * 0.66, cx - r * 0.12, cy - r * 0.36], fill=(255, 255, 255, 95))
    for dx in (-0.52, 0.52):
        ex, ey = cx + r * dx, cy + r * 0.16
        d.ellipse([ex - r * 0.12, ey - r * 0.065, ex + r * 0.12, ey + r * 0.065], fill=(236, 72, 153, 150))
    img = Image.alpha_composite(img, over)
    d = ImageDraw.Draw(img)
    # Eyes with highlights
    for dx in (-0.3, 0.3):
        ex, ey = cx + r * dx, cy - r * 0.08
        d.ellipse([ex - r * 0.085, ey - r * 0.12, ex + r * 0.085, ey + r * 0.12], fill=ink)
        d.ellipse([ex - r * 0.005, ey - r * 0.085, ex + r * 0.055, ey - r * 0.025], fill=(255, 255, 255, 255))
    # Open, talking smile
    mx, my = cx, cy + r * 0.2
    d.chord([mx - r * 0.2, my - r * 0.14, mx + r * 0.2, my + r * 0.22], start=0, end=180, fill=ink)
    d.chord([mx - r * 0.1, my + r * 0.06, mx + r * 0.1, my + r * 0.2], start=0, end=180, fill=(236, 92, 120, 255))
    # Sparkles
    def sparkle(x, y, s, col):
        pts = []
        for k in range(8):
            a = math.pi * 2 * k / 8 - math.pi / 2
            rad = s if k % 2 == 0 else s * 0.32
            pts.append((x + math.cos(a) * rad, y + math.sin(a) * rad))
        d.polygon(pts, fill=col)
    sparkle(S * 0.24, S * 0.24, S * 0.045, (255, 214, 120, 255))
    sparkle(S * 0.77, S * 0.27, S * 0.03, (255, 255, 255, 230))
    sparkle(S * 0.8, S * 0.76, S * 0.022, (255, 255, 255, 200))
    return img.resize((1024, 1024), Image.LANCZOS)


def main():
    os.makedirs(OUT, exist_ok=True)
    icon = draw()
    png = os.path.join(OUT, "icon.png")
    icon.save(png)
    # Small copy embedded in the executable for the window / taskbar icon.
    icon.resize((256, 256), Image.LANCZOS).save(os.path.join(OUT, "icon_256.png"))
    icon.save(os.path.join(OUT, "icon.ico"), sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    if sys.platform == "darwin":
        with tempfile.TemporaryDirectory() as t:
            iconset = os.path.join(t, "icon.iconset")
            os.makedirs(iconset)
            for s in (16, 32, 128, 256, 512):
                icon.resize((s, s), Image.LANCZOS).save(os.path.join(iconset, f"icon_{s}x{s}.png"))
                icon.resize((s * 2, s * 2), Image.LANCZOS).save(os.path.join(iconset, f"icon_{s}x{s}@2x.png"))
            subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(OUT, "icon.icns")], check=True)
    print("wrote", os.path.abspath(OUT))


if __name__ == "__main__":
    main()
