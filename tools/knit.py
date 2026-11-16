#!/usr/bin/env python3
"""Renders the knitted launcher icon: the wordmark "Flausch" (Montez, ui/fonts) embroidered in white yarn on bulging
pink stockinette; the lettering bows with the fabric.

Writes
- android/res/drawable-nodpi/ic_launcher_knit.png: adaptive icon background (knitted fabric)
- android/res/drawable-nodpi/ic_launcher_text.png: adaptive icon foreground (the lettering with its shadow)
- fastlane/metadata/android/en-US/images/icon.png: 512 px store icon, both layers with larger lettering

The monochrome themed icon is the vector wordmark from tools/logo.py.
Needs numpy, scipy and pillow (e.g. in a venv): python3 tools/knit.py
"""

import math
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont
from scipy import ndimage

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "ui/fonts/Montez-Regular.ttf"
TEXT = "Flausch"
OUT = 432  # 108dp at xxxhdpi
SS = 3  # supersampling
COLUMNS = 9.5  # stitch columns across the 108dp icon
FABRIC = np.array([0xF0, 0xB9, 0xCE]) / 255
FABRIC_DARK = np.array([0xB2, 0x4B, 0x73]) / 255
YARN = np.array([0xFF, 0xF8, 0xF2]) / 255
YARN_DARK = np.array([0xE6, 0xC4, 0xD3]) / 255
BULGE = 0.45  # how strongly the fabric bows outwards
WIDTH = 0.62  # width of the lettering in the 108dp icon (tools/logo.py uses the same)
BOLD = 0.007  # the yarn is a little thicker than the font's strokes, relative to the icon size
STORE_ZOOM = 1.4  # the store icon has no safe zone, so the lettering can be larger
LIGHT = np.array([-0.45, -0.6, 0.66])
LIGHT = LIGHT / np.linalg.norm(LIGHT)

rng = np.random.default_rng(7)


def noise(shape, sigma):
    n = ndimage.gaussian_filter(rng.standard_normal(shape), sigma, mode="wrap")
    return n / (n.std() + 1e-9)


def shade(height, scale, base, dark, light=None):
    """Lambert shading of a height map, mixed between `dark` and `base` (and a little white for highlights)."""
    gy, gx = np.gradient(height * scale)
    normal = np.dstack([-gx, -gy, np.ones_like(height)])
    normal /= np.linalg.norm(normal, axis=2, keepdims=True)
    lit = np.clip(normal @ LIGHT, 0, 1)
    t = np.clip(0.3 + 0.85 * lit, 0, 1.15)[..., None]
    color = dark + (base - dark) * np.minimum(t, 1)
    return color + (1 - color) * np.clip(t - 1, 0, 1) * 2


def knit(size, columns):
    """Stockinette: columns of V stitches, each V two plump, twisted legs."""
    w = size / columns
    h = w * 0.66
    y, x = np.mgrid[0:size, 0:size].astype(float)
    # Bulge like a cushion: sample the flat fabric further out towards the edges, so columns and rows bow outwards.
    c = size / 2
    dx, dy = (x - c) / c, (y - c) / c
    r2 = dx**2 + dy**2
    warp = 1 + BULGE * r2
    x = c + (x - c) * warp + w * 0.25
    y = c + (y - c) * warp
    height = np.zeros((size, size))
    jitter = noise((size, size), w * 1.5) * 0.05 * w
    for dc in (-1, 0, 1):
        col = np.floor(x / w) + dc
        cx = (col + 0.5) * w
        for dr in (-1, 0, 1):
            row = np.floor(y / h) + dr
            cy = (row + 0.5) * h
            seed = np.sin(col * 12.9898 + row * 78.233) * 43758.5453
            rand = seed - np.floor(seed)
            for side in (-1, 1):
                # A leg runs from the bottom center of the stitch up and out.
                angle = math.atan2(w * 0.36, h * 1.2) * side
                ux, uy = math.sin(angle), -math.cos(angle)
                px = x - (cx + side * w * 0.22) + jitter
                py = y - cy
                u = px * ux + py * uy  # along the leg
                v = -px * uy + py * ux  # across
                a = h * 0.8
                bw = w * (0.235 + 0.025 * rand)
                # Teardrop: a little wider at the top.
                bw = bw * (1 + 0.12 * np.clip(u / a, -1, 1))
                r = (u / a) ** 2 + (v / bw) ** 2
                leg = np.clip(1 - r, 0, 1) ** 0.45
                twist = 0.5 + 0.5 * np.cos(2 * math.pi * (u / (w * 0.11) + side * v / (w * 0.16)))
                leg = leg * (0.9 + 0.1 * twist) * (0.9 + 0.1 * rand)
                height = np.maximum(height, leg)
    height += 0.03 * noise((size, size), 0.7 * size / 1296) + 0.04 * noise((size, size), 4 * size / 1296)
    color = shade(height, size / 22, FABRIC, FABRIC_DARK)
    color *= (0.55 + 0.45 * np.clip(height, 0, 1) ** 0.5)[..., None]  # the gaps between stitches stay dark
    # Light the dome as a whole: brighter towards the light, darker at the edges.
    dome = np.dstack([dx * 0.6, dy * 0.6, np.ones_like(dx)])
    dome /= np.linalg.norm(dome, axis=2, keepdims=True)
    lit = np.clip(dome @ LIGHT, 0, 1)
    color *= (0.8 + 0.35 * lit)[..., None]
    color *= (1 - 0.25 * np.clip(np.sqrt(r2) - 0.45, 0, 1) ** 1.5)[..., None]
    return np.clip(color, 0, 1)


def lettering(size, width):
    """Mask of TEXT, centered and `width` of the size wide, bowed like the fabric (see knit())."""
    probe = ImageFont.truetype(str(FONT), 400)
    left, _, right, _ = probe.getbbox(TEXT)
    font = ImageFont.truetype(str(FONT), int(400 * width * size / (right - left)))
    left, top, right, bottom = font.getbbox(TEXT)
    im = Image.new("L", (size, size), 0)
    pos = ((size - (right - left)) / 2 - left, (size - (bottom - top)) / 2 - top)
    ImageDraw.Draw(im).text(pos, TEXT, font=font, fill=255)
    flat = np.asarray(im, float) / 255
    flat = (ndimage.distance_transform_edt(flat < 0.5) < BOLD * size).astype(float)
    flat = ndimage.gaussian_filter(flat, 1.0)
    y, x = np.mgrid[0:size, 0:size].astype(float)
    c = size / 2
    warp = 1 + BULGE * (((x + 0.5 - c) / c) ** 2 + ((y + 0.5 - c) / c) ** 2)
    return ndimage.map_coordinates(flat, [c + (y + 0.5 - c) * warp, c + (x + 0.5 - c) * warp], order=1)


def foreground(size, width):
    """The lettering in white yarn: a rounded cord, lit like the dome of the fabric, with a soft drop shadow."""
    mask = lettering(size, width)
    filled = mask > 0.5
    dist = ndimage.distance_transform_edt(filled)
    stroke = max(2.0, np.percentile(dist[filled], 95))
    height = ndimage.gaussian_filter(np.clip(dist / stroke, 0, 1) ** 0.5, 0.6)
    alpha = np.clip(ndimage.gaussian_filter(mask, 0.8), 0, 1)
    color = shade(height * stroke, 1.0, YARN, YARN_DARK)
    color *= (0.7 + 0.3 * np.clip(height, 0, 1) ** 0.6)[..., None]
    y, x = np.mgrid[0:size, 0:size].astype(float)
    c = size / 2
    wx, wy = (x + 0.5 - c) / c, (y + 0.5 - c) / c
    dome = np.dstack([wx * 0.6, wy * 0.6, np.ones_like(wx)])
    dome /= np.linalg.norm(dome, axis=2, keepdims=True)
    color *= ((0.8 + 0.5 * np.clip(dome @ LIGHT, 0, 1)) / (0.8 + 0.5 * LIGHT[2]))[..., None]
    color *= (1 - 0.25 * np.clip(np.sqrt(wx**2 + wy**2) - 0.45, 0, 1) ** 1.5)[..., None]
    shadow = ndimage.shift(ndimage.gaussian_filter(alpha, stroke * 0.5), (stroke * 0.35, stroke * 0.12), order=1)
    shadow_alpha = 0.55 * shadow * (1 - alpha)
    out_alpha = alpha + shadow_alpha
    rgb = color * alpha[..., None] / np.maximum(out_alpha, 1e-6)[..., None]
    return to_image(np.clip(rgb, 0, 1), out_alpha)


def to_image(rgb, alpha=None):
    data = np.dstack([rgb, alpha if alpha is not None else np.ones(rgb.shape[:2])])
    return Image.fromarray((np.clip(data, 0, 1) * 255 + 0.5).astype(np.uint8), "RGBA")


def render(size, width, columns=COLUMNS):
    big = size * SS
    fabric = to_image(knit(big, columns)).resize((size, size), Image.LANCZOS)
    text = foreground(big, width).resize((size, size), Image.LANCZOS)
    return fabric, text


if __name__ == "__main__":
    res = ROOT / "android/res/drawable-nodpi"
    res.mkdir(parents=True, exist_ok=True)
    fabric, text = render(OUT, WIDTH)
    fabric.convert("RGB").save(res / "ic_launcher_knit.png", optimize=True)
    text.save(res / "ic_launcher_text.png", optimize=True)
    # The store shows the whole square, so it gets the visible part of the launcher icon, a bit larger.
    fabric, text = render(512, WIDTH * STORE_ZOOM, COLUMNS / STORE_ZOOM)
    icon = ROOT / "fastlane/metadata/android/en-US/images/icon.png"
    Image.alpha_composite(fabric, text).convert("RGB").save(icon, optimize=True)
