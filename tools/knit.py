#!/usr/bin/env python3
"""Renders the knitted launcher icon: a ball of yarn with cat ears, embroidered in thick white yarn (chain stitch)
on bulging pink stockinette.

Writes
- android/res/drawable-nodpi/ic_launcher_knit.png: adaptive icon background (knitted fabric)
- android/res/drawable-nodpi/ic_launcher_yarn.png: adaptive icon foreground (the ball of yarn with its shadow)
- fastlane/metadata/android/en-US/images/icon.png: 512 px store icon, both layers with a larger ball

The monochrome themed icon stays the line logo from tools/logo.py.
Needs numpy, scipy, pillow and svgpathtools (e.g. in a venv): python3 tools/knit.py
"""

import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage
from scipy.spatial import cKDTree
from svgpathtools import parse_path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import logo  # noqa: E402

OUT = 432  # 108dp at xxxhdpi
SS = 3  # supersampling
COLUMNS = 9.5  # stitch columns across the 108dp icon
FABRIC = np.array([0xF0, 0xB9, 0xCE]) / 255
FABRIC_DARK = np.array([0xB2, 0x4B, 0x73]) / 255
YARN = np.array([0xFF, 0xF8, 0xF2]) / 255
YARN_DARK = np.array([0xE6, 0xC4, 0xD3]) / 255
CHAIN_LEN = 0.85  # length of a chain stitch link, relative to the yarn width
YARN_WIDTH = 1.5  # in logo units (24×24 grid)
BULGE = 0.45  # how strongly the fabric bows outwards
# The embroidered ball of yarn: a sphere filled with rows along the latitudes of a tilted axis, framed by a ring,
# with the logo's cat ears (filled as well) and tail.
BALL = logo.BALL
AXIS = (-0.55, -0.75, 0.35)
SCALE = 2.7  # in the 108dp icon; ears, ball and tail stay inside the 66dp safe zone
CENTER = (13.6, 12.6)
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


def latitudes(axis, degrees):
    """Path data for the front half of latitude circles of the ball around `axis` (x right, y down, z to the viewer)."""
    cx, cy, r = BALL
    a = np.array(axis, float)
    a /= np.linalg.norm(a)
    e1 = np.cross(a, [0, 0, 1])
    e1 /= np.linalg.norm(e1)
    e2 = np.cross(a, e1)
    t = np.linspace(0, 2 * math.pi, 721)
    parts = []
    for deg in degrees:
        phi = math.radians(deg)
        p = a * r * math.sin(phi) + r * math.cos(phi) * (np.outer(np.cos(t), e1) + np.outer(np.sin(t), e2))
        front = p[:, 2] > 0.4
        start = np.argmin(front)  # begin on the back, so every front run is contiguous
        p, front = np.roll(p, -start, 0), np.roll(front, -start)
        run = []
        for q, f in zip(p, front):
            if f:
                run.append(f"{cx + q[0]:.3f},{cy + q[1]:.3f}")
            elif run:
                parts.append("M" + " L".join(run))
                run = []
        if run:
            parts.append("M" + " L".join(run))
    return " ".join(parts)


def chain(along, across, radius, width):
    """Height of a chain stitch along a path: a row of V loops, each two teardrop legs tilted outwards."""
    L = width * CHAIN_LEN
    out = np.zeros_like(along)
    for dk in (-1, 0, 1):
        k = np.floor(along / L) + dk
        u = along - (k + 0.5) * L
        seed = np.sin(k * 12.9898 + width) * 43758.5453
        rnd = seed - np.floor(seed)
        for side in (-1, 1):
            ang = math.radians(35) * side
            pv = across - side * radius * 0.45
            a = u * math.cos(ang) + pv * math.sin(ang)
            b = -u * math.sin(ang) + pv * math.cos(ang)
            ra, rb = L * 0.78, radius * (0.55 + 0.06 * rnd)
            r = (a / ra) ** 2 + (b / rb) ** 2
            leg = np.clip(1 - r, 0, 1) ** 0.5
            fib = 0.5 + 0.5 * np.cos(2 * math.pi * (a / (L * 0.14) + side * b / (radius * 0.28)))
            out = np.maximum(out, leg * (0.85 + 0.15 * fib) * (0.9 + 0.1 * rnd))
    return 0.1 + 0.9 * out


def inside(px, py, poly):
    """Even-odd point in polygon test."""
    res = np.zeros(px.shape, bool)
    x0, y0 = poly.real, poly.imag
    x1, y1 = np.roll(x0, -1), np.roll(y0, -1)
    for a, b, c, d in zip(x0, y0, x1, y1):
        cross = ((b > py) != (d > py)) & (px < (c - a) * (py - b) / (d - b + 1e-12) + a)
        res ^= cross
    return res


def ear_rows(spacing):
    """Rows of stitches filling both ears, running from the base up to the tip."""
    parts = []
    for sub in parse_path(logo.ears()).continuous_subpaths():
        poly = np.array([sub.point(t) for t in np.linspace(0, 1, 200)])
        tip = poly[np.argmin(poly.imag)]
        base = (poly[0] + poly[-1]) / 2
        d = (tip - base) / abs(tip - base)
        n = d * 1j
        span = np.abs(((poly - base) * n.conjugate()).real).max()
        for o in np.arange(-span, span + 1e-6, spacing) + (span % spacing) / 2:
            t = np.linspace(-1, abs(tip - base) + 1, 400)
            line = base + n * o + d * t
            m = inside(line.real, line.imag, poly)
            run = []
            for q, f in zip(line, m):
                if f:
                    run.append(f"{q.real:.3f},{q.imag:.3f}")
                elif run:
                    if len(run) > 3:
                        parts.append("M" + " L".join(run))
                    run = []
            if len(run) > 3:
                parts.append("M" + " L".join(run))
    return " ".join(parts)


def strands():
    """The stitches in drawing order (back to front), each as (path data, width in logo units)."""
    cx, cy, r = BALL
    return [
        (ear_rows(YARN_WIDTH * 0.95), YARN_WIDTH * 1.1),
        (logo.ears(), YARN_WIDTH * 1.25),
        (latitudes(AXIS, range(-80, 81, 16)), YARN_WIDTH * 1.3),
        (logo.circle(cx, cy, r), YARN_WIDTH * 1.25),
        (logo.TAIL, YARN_WIDTH * 1.25),
    ]


def samples(data, step=0.01):
    """Points along each continuous subpath with their tangent and arc length."""
    for sub in parse_path(data).continuous_subpaths():
        n = max(2, int(sub.length() / step))
        ts = np.linspace(0, 1, n)
        pts = np.array([sub.point(t) for t in ts])
        tangent = np.gradient(pts)
        tangent /= np.abs(tangent) + 1e-12
        arc = np.concatenate([[0], np.cumsum(np.abs(np.diff(pts)))])
        yield pts, tangent, arc


def yarn(size, scale, center):
    """The stitches in white yarn: returns color, alpha and the shadow alpha."""
    unit = size / 108 * scale  # pixels per logo unit
    y, x = np.mgrid[0:size, 0:size].astype(float)
    # Follow the fabric's bulge (see knit()), so the embroidery bows with it.
    c = size / 2
    wx, wy = (x + 0.5 - c) / c, (y + 0.5 - c) / c
    warp = 1 + BULGE * (wx**2 + wy**2)
    lx = (x + 0.5 - c) * warp / unit + center[0]
    ly = (y + 0.5 - c) * warp / unit + center[1]
    color = np.zeros((size, size, 3))
    alpha = np.zeros((size, size))
    cx, cy, r = BALL
    fuzz = noise((size, size), 0.6 * SS)
    for data, width in strands():
        radius = width / 2
        height = np.zeros((size, size))
        cover = np.zeros((size, size))
        for pts, tangent, arc in samples(data):
            tree = cKDTree(np.column_stack([pts.real, pts.imag]))
            box = (lx > pts.real.min() - width) & (lx < pts.real.max() + width) & \
                  (ly > pts.imag.min() - width) & (ly < pts.imag.max() + width)
            qx, qy = lx[box], ly[box]
            dist, idx = tree.query(np.column_stack([qx, qy]), distance_upper_bound=radius * 1.3)
            hit = np.isfinite(dist)
            idx = np.where(hit, idx, 0)
            dx = qx - pts.real[idx]
            dy = qy - pts.imag[idx]
            across = tangent.real[idx] * dy - tangent.imag[idx] * dx
            along = arc[idx] + tangent.real[idx] * dx + tangent.imag[idx] * dy
            d = np.where(hit, dist, np.inf) / radius
            h = np.where(hit, chain(along, across, radius, width), 0) * np.clip(1.2 - d, 0, 1) ** 0.3
            a = np.clip((1 - d) * radius * unit / 1.2 + 0.25 * fuzz[box], 0, 1)
            hb = height[box]
            ab = cover[box]
            height[box] = np.where(h > hb, h, hb)
            cover[box] = np.maximum(ab, a)
        height += 0.03 * fuzz
        layer = shade(height * width, size / 110 * scale, YARN, YARN_DARK)
        layer *= (0.55 + 0.45 * np.clip(height, 0, 1) ** 0.6)[..., None]
        color = color * (1 - cover[..., None]) + layer * cover[..., None]
        alpha = alpha + cover * (1 - alpha)
    # The ball is round: its strands get darker away from the light.
    bx, by = (lx - cx) / r, (ly - cy) / r
    nz = np.sqrt(np.clip(1 - bx**2 - by**2, 0, 1))
    ball = np.clip(bx * LIGHT[0] + by * LIGHT[1] + nz * LIGHT[2], 0, 1)
    color *= np.where(nz > 0, 0.9 + 0.12 * ball, 1)[..., None]
    dome = np.dstack([wx * 0.6, wy * 0.6, np.ones_like(wx)])
    dome /= np.linalg.norm(dome, axis=2, keepdims=True)
    color *= ((0.8 + 0.35 * np.clip(dome @ LIGHT, 0, 1)) / (0.8 + 0.35 * LIGHT[2]))[..., None]
    shadow = ndimage.gaussian_filter(alpha, 0.9 * unit)
    shadow = ndimage.shift(shadow, (0.55 * unit, 0.2 * unit), order=1)
    return np.clip(color, 0, 1), alpha, shadow


def foreground(size, scale, center):
    color, alpha, shadow = yarn(size, scale, center)
    shadow_alpha = 0.6 * shadow * (1 - alpha)
    out_alpha = alpha + shadow_alpha
    rgb = color * alpha[..., None] / np.maximum(out_alpha, 1e-6)[..., None]
    return to_image(rgb, out_alpha)


def to_image(rgb, alpha=None):
    data = np.dstack([rgb, alpha if alpha is not None else np.ones(rgb.shape[:2])])
    return Image.fromarray((np.clip(data, 0, 1) * 255 + 0.5).astype(np.uint8), "RGBA")


def render(size, scale, center, columns=COLUMNS):
    big = size * SS
    fabric = to_image(knit(big, columns)).resize((size, size), Image.LANCZOS)
    logo_layer = foreground(big, scale, center).resize((size, size), Image.LANCZOS)
    return fabric, logo_layer


if __name__ == "__main__":
    root = Path(__file__).resolve().parent.parent
    res = root / "android/res/drawable-nodpi"
    res.mkdir(parents=True, exist_ok=True)
    fabric, yarn_layer = render(OUT, SCALE, CENTER)
    fabric.convert("RGB").save(res / "ic_launcher_knit.png", optimize=True)
    yarn_layer.save(res / "ic_launcher_yarn.png", optimize=True)
    # The store shows the whole square, so it gets the visible part of the launcher icon, a bit larger.
    zoom = logo.STORE_ZOOM
    fabric, yarn_layer = render(512, SCALE * zoom, CENTER, COLUMNS / zoom)
    icon = root / "fastlane/metadata/android/en-US/images/icon.png"
    Image.alpha_composite(fabric, yarn_layer).convert("RGB").save(icon, optimize=True)
