#!/usr/bin/env python3
"""Procedural native-4K Celaris space background.
Ringed violet planet (Celaris) + crescent moon + galaxy + nebula + starfield."""
import numpy as np
from PIL import Image, ImageFilter

W, H = 3840, 2160
rng = np.random.default_rng(7)

def smoothnoise(scale):
    base = rng.random((scale, max(2, scale * H // W))).astype(np.float32)
    img = Image.fromarray((base * 255).astype(np.uint8)).resize((W, H), Image.BICUBIC)
    return np.asarray(img, dtype=np.float32) / 255.0

def fractal(octaves, base_scale, persistence=0.5):
    total = np.zeros((H, W), np.float32); amp = 1.0; freq = base_scale; norm = 0.0
    for _ in range(octaves):
        total += amp * smoothnoise(max(2, int(freq)))
        norm += amp; amp *= persistence; freq *= 2
    return total / norm

def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)

yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)

# ---- Base deep-space gradient -------------------------------------------------
img = np.zeros((H, W, 3), np.float32)
gy = yy / H
img[..., 0] = 0.020 + 0.030 * (1 - gy)
img[..., 1] = 0.010 + 0.012 * (1 - gy)
img[..., 2] = 0.045 + 0.060 * (1 - gy)

# ---- Nebula -------------------------------------------------------------------
neb = fractal(6, 3)
neb = np.clip((neb - 0.40) / 0.60, 0, 1) ** 1.05
det = fractal(6, 9)
neb *= (0.55 + 0.55 * det)

# Two diagonal bands of cloud (upper-right + lower-left arc), like the brief.
nx = xx / W; ny = yy / H
band1 = np.exp(-((ny - 0.20) - 0.55 * (1 - nx)) ** 2 / 0.055)   # upper-right
band2 = np.exp(-((ny - 0.92) + 0.45 * (1 - nx)) ** 2 / 0.045)   # lower band
band3 = np.exp(-((nx - 0.93)) ** 2 / 0.06)                      # right edge wash
mask = np.clip(0.18 + band1 * 1.15 + band2 * 1.0 + band3 * 0.6, 0, 1)
neb = neb * mask

# Colour ramp: deep indigo -> violet -> magenta -> pink highlight.
def ramp(t):
    stops = [
        (0.00, (0.06, 0.02, 0.16)),
        (0.40, (0.28, 0.08, 0.45)),
        (0.70, (0.55, 0.18, 0.72)),
        (0.90, (0.78, 0.42, 0.92)),
        (1.00, (0.95, 0.80, 1.00)),
    ]
    t = np.clip(t, 0, 1)
    r = np.zeros_like(t); g = np.zeros_like(t); b = np.zeros_like(t)
    for (t0, c0), (t1, c1) in zip(stops, stops[1:]):
        m = (t >= t0) & (t <= t1)
        f = (t[m] - t0) / (t1 - t0)
        r[m] = c0[0] + (c1[0] - c0[0]) * f
        g[m] = c0[1] + (c1[1] - c0[1]) * f
        b[m] = c0[2] + (c1[2] - c0[2]) * f
    return np.stack([r, g, b], -1)

neb_col = ramp(neb) * neb[..., None] * 2.4
img += neb_col
del neb_col, det, band1, band2, mask

# ---- Starfield ----------------------------------------------------------------
def add_stars(img, n, bright, size_max):
    sx = rng.integers(0, W, n); sy = rng.integers(0, H, n)
    b = (rng.random(n).astype(np.float32) ** 3.2) * bright
    layer = np.zeros((H, W), np.float32)
    np.add.at(layer, (sy, sx), b)
    if size_max > 1:
        layer = np.asarray(Image.fromarray((np.clip(layer, 0, 1) * 255).astype(np.uint8))
                            .filter(ImageFilter.GaussianBlur(size_max)), np.float32) / 255.0
    tint = np.array([0.86, 0.84, 1.0], np.float32)
    img += layer[..., None] * tint

add_stars(img, 30000, 1.0, 1)
add_stars(img, 5000, 1.0, 1.4)
add_stars(img, 900, 1.0, 2.2)

# A handful of bright 4-point sparkle stars.
def sparkle(img, cx, cy, s, col):
    L = int(s * 7)
    for i in range(-L, L + 1):
        a = (1 - abs(i) / L) ** 2
        for (px, py) in ((cx + i, cy), (cx, cy + i)):
            if 0 <= px < W and 0 <= py < H:
                img[py, px] += np.array(col) * a
    # bright core
    for dx in range(-2, 3):
        for dy in range(-2, 3):
            px, py = cx + dx, cy + dy
            if 0 <= px < W and 0 <= py < H:
                img[py, px] += np.array(col) * (1 - (dx*dx+dy*dy)/12) * 0.6

for _ in range(26):
    sparkle(img, int(rng.integers(80, W-80)), int(rng.integers(80, H-80)),
            rng.uniform(1.0, 1.8), (0.9, 0.85, 1.0))

# ---- Distant galaxy (upper-right) --------------------------------------------
gcx, gcy = int(0.82 * W), int(0.23 * H)
gdx = xx - gcx; gdy = yy - gcy
th = np.radians(28); u = gdx*np.cos(th) - gdy*np.sin(th); v = gdx*np.sin(th) + gdy*np.cos(th)
gal = np.exp(-((u/260)**2 + (v/95)**2)) * 0.9 + np.exp(-((u/110)**2 + (v/60)**2)) * 0.7
gcol = np.stack([gal*0.75, gal*0.45, gal*0.95], -1)
img += gcol
del gcol, gdx, gdy, u, v

# ---- helper: additive glow ----------------------------------------------------
def glow(field, radius, col, strength):
    g = np.asarray(Image.fromarray((np.clip(field, 0, 1)*255).astype(np.uint8))
                   .filter(ImageFilter.GaussianBlur(radius)), np.float32)/255.0
    return g[..., None] * np.array(col, np.float32) * strength

# ---- Planet Celaris -----------------------------------------------------------
cx, cy, R = 0.36 * W, 0.55 * H, 0.215 * H
dx = xx - cx; dy = yy - cy
r = np.sqrt(dx*dx + dy*dy)
inside = r <= R
nz = np.sqrt(np.clip(1 - (r / R) ** 2, 0, 1))
pnx = dx / R; pny = dy / R
L = np.array([-0.55, -0.55, 0.63], np.float32); L /= np.linalg.norm(L)
lamb = np.clip(pnx * L[0] + pny * L[1] + nz * L[2], 0, 1)
shade = 0.16 + 1.05 * lamb ** 1.05

# Surface: fractal bands + mottling.
surf = fractal(6, 6)
surf2 = fractal(5, 14)
tex = 0.55 * surf + 0.45 * surf2
base_dark = np.array([0.22, 0.10, 0.34], np.float32)
base_lite = np.array([0.78, 0.52, 0.95], np.float32)
planet_rgb = base_dark + (base_lite - base_dark) * tex[..., None]
planet_rgb = planet_rgb * shade[..., None]
# Atmospheric limb brightening.
limb = smoothstep(0.55, 1.0, r / R) * (0.4 + 0.6 * lamb)
planet_rgb += limb[..., None] * np.array([0.5, 0.25, 0.85], np.float32)

# Edge anti-alias alpha.
palpha = np.clip((R - r) / 2.5, 0, 1) * inside

# ---- Rings --------------------------------------------------------------------
rth = np.radians(-18.0); k = 0.30  # tilt squash
ru = dx * np.cos(rth) - dy * np.sin(rth)
rv = dx * np.sin(rth) + dy * np.cos(rth)
rr = np.sqrt(ru * ru + (rv / k) ** 2)
ring_in, ring_out = 1.28 * R, 1.92 * R
rband = smoothstep(ring_in - 6, ring_in + 6, rr) * (1 - smoothstep(ring_out - 6, ring_out + 6, rr))
# gaps + brightness variation across the ring width
rnorm = np.clip((rr - ring_in) / (ring_out - ring_in), 0, 1)
ring_bright = (0.55 + 0.45 * np.sin(rnorm * 18) ** 2) * (0.7 + 0.3 * fractal(4, 40))
ralpha = np.clip(rband, 0, 1) * np.clip(ring_bright, 0, 1)
ring_col = (np.array([0.62, 0.40, 0.92], np.float32) * (0.6 + 0.6 * rnorm)[..., None])
ring_rgb = ring_col * (0.8 + 0.6 * ring_bright)[..., None]

# Composite: ring (outside disk) -> planet -> ring front arc (rv>0 over disk).
out_disk = ~inside
a_back = ralpha * out_disk
img = img * (1 - a_back[..., None]) + ring_rgb * a_back[..., None]
img += glow(ralpha * out_disk, 6, (0.5, 0.3, 0.9), 0.5)   # ring glow outside

# planet atmosphere halo (outside disk)
halo = np.exp(-np.clip((r - R) / (0.16 * R), 0, 8)) * out_disk
img += halo[..., None] * np.array([0.40, 0.20, 0.75], np.float32) * 0.9

img = img * (1 - palpha[..., None]) + planet_rgb * palpha[..., None]

a_front = ralpha * inside * smoothstep(0.0, 30.0, rv)     # lower/front arc
img = img * (1 - a_front[..., None]) + ring_rgb * a_front[..., None]
del dx, dy, r, nz, pnx, pny, lamb, shade, surf, surf2, tex, ru, rv, rr

# ---- Crescent moon (upper-left) ----------------------------------------------
mcx, mcy, mR = 0.165 * W, 0.165 * H, 0.052 * H
mdx = xx - mcx; mdy = yy - mcy; mr = np.sqrt(mdx*mdx + mdy*mdy)
m_in = mr <= mR
mnz = np.sqrt(np.clip(1 - (mr / mR) ** 2, 0, 1))
ml = np.clip((mdx/mR) * (-0.7) + (mdy/mR) * (-0.2) + mnz * 0.68, 0, 1)
m_alpha = np.clip((mR - mr) / 2.0, 0, 1) * m_in * smoothstep(0.18, 0.5, ml)
moon_rgb = (np.array([0.72, 0.60, 0.92], np.float32) * (0.4 + 0.7 * ml)[..., None])
img = img * (1 - m_alpha[..., None]) + moon_rgb * m_alpha[..., None]
img += glow(m_alpha, 14, (0.5, 0.35, 0.85), 0.35)
del mdx, mdy, mr

# ---- Bloom + vignette + tone -------------------------------------------------
lum = img.max(-1)
bright = np.clip(lum - 0.75, 0, None)
img += glow(bright, 10, (1, 1, 1), 0.6)

vdx = (xx / W - 0.5); vdy = (yy / H - 0.5)
vig = 1 - 0.55 * smoothstep(0.18, 0.95, np.sqrt(vdx*vdx + vdy*vdy * (W/H)**0 ))
img *= vig[..., None]

img = np.clip(img, 0, 1) ** (1/1.06)           # slight gamma lift
out = (np.clip(img, 0, 1) * 255).astype(np.uint8)
Image.fromarray(out, "RGB").save("/tmp/celaris-bg-4k.png")
print("saved /tmp/celaris-bg-4k.png", out.shape)
