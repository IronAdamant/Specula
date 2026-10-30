"""The Fidelity track's T3 contact sheet (M4): the five benchmark subjects at T0, T1 and T3, side by
side, from the asset studio's fixed shots, with an iGPU budget line per subject.

T3 is "detailed stylised: SDF sculpting, normal maps, baked GI, decals, particles". The recipes
(`docs/fidelity/assets/recipes/*_t3.json`, texture graphs `tex_*_t3.json`) carry the sculpts and the
normal-mapped materials; decals, particles, the lantern's point light and baked GI are world
features, so the asset studio's isolated stage (which shows builds only) can't show them. This
script therefore rebuilds the studio's stage in a headless engine's world and renders every tier
there through the studio's own shot set:

  - the stage: the studio's ground plane and backdrop colour per light rig, its three rigs
    (`neutral` for the close-up, `game`, `worst`: read from `studio_stage`), the key light turned
    with the camera exactly as the studio turns it; tone mapping, shadows, bloom and FXAA as the
    studio sets them;
  - the shots: `close_up` (three-quarter, az 35 el 20, framed to fit, fov 30, LOD 0) and the game
    camera (`[shots.gameplay]` of the set's style.toml) for `gameplay` and `worst_light`, rendered
    as the studio does: a crop around the subject at true pixel size (a crop class), the field of
    view narrowed to it, magnified on the sheet by nearest neighbour;
  - T0 and T1 are rendered there with GI off and no dressing; T3 gets its dressing from
    `docs/fidelity/assets/stage_t3.json` (decals, effects, the lantern's light, set pieces) and a
    small GI volume baked around it for each rig (`gi_bake`, the runtime tier the adapter picks:
    low on the iGPU). Set pieces (the lantern's two walls) stand at every tier.

The world clock is paused (`--paused`) and every capture is taken at a fixed `time_s`, so the
particles, the skeleton's idle pose and the flame repeat exactly.

Also, per T3 subject: `style_check` and `studio_check` (the studio's own measures, on its stage),
the budget line (triangles per LOD and texture KB from `studio_check`'s facts; GPU ms from
`perf_frame` after 1280x720 captures of the gameplay view and of the close-up, with and without
the subject, medians of several), the GI tier in use, and for the lantern the bounce its light
adds (`gi_check` points on the walls, baked with the light and again without it).

Output: `docs/fidelity/benchmark_t3_m4.png` (the sheet), `docs/fidelity/benchmark_t3_m4.json`
(the numbers, relative paths only), captures and packets under OUT/captures. Called by
`python scripts/studio_check.py --fidelity` (after the T0/T1 sheet) or on its own:
`python scripts/fidelity_t3.py [--exe PATH] [--port 7850] [--adapter integrated] [--out DIR]`.
Needs a GPU (Vulkan). Standard library only; output ASCII.
"""
import argparse
import base64
import json
import math
import os
import re
import shutil
import struct
import sys
import time
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

FIDELITY = ROOT / "docs" / "fidelity"
ASSETS = FIDELITY / "assets"
SUBJECTS = ["runestone", "barrel", "lantern", "tree", "skeleton_warrior"]
TIERS = ["t0", "t1", "t3"]
SHOTS = ["close_up", "gameplay", "worst_light"]
TIME_S = 6.0
TILE = 184
CLOSE_PX = 368
FIT_FOV, FIT_AZ, FIT_EL, FIT_MARGIN = 30.0, 35.0, 20.0, 1.08
CROP_CLASSES = [32, 64, 96, 128, 192, 256, 384, 512]
GPU_REPEATS = 3
GPU_ROUNDS = 8


def rel(p):
    """A path for committed output: relative to the repository, or a placeholder (no home folders)."""
    try:
        return str(Path(p).resolve().relative_to(ROOT.resolve())).replace("\\", "/")
    except ValueError:
        return "<out>/" + Path(p).name


# --- PNG -------------------------------------------------------------------------------------------
def decode_png(path):
    """8-bit RGB/RGBA non-interlaced PNG -> (w, h, RGB bytes)."""
    data = Path(path).read_bytes()
    pos, idat, w, h, bpp = 8, b"", 0, 0, 3
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if kind == b"IHDR":
            w, h, _, ctype, _, _, _ = struct.unpack(">IIBBBBB", body)
            bpp = 3 if ctype == 2 else 4
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    raw = zlib.decompress(idat)
    stride = w * bpp
    out, prev, i = bytearray(h * stride), bytearray(stride), 0
    for _ in range(h):
        f, line = raw[i], bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        if f == 1:
            for x in range(bpp, stride):
                line[x] = (line[x] + line[x - bpp]) & 255
        elif f == 2:
            for x in range(stride):
                line[x] = (line[x] + prev[x]) & 255
        elif f == 3:
            for x in range(stride):
                a = line[x - bpp] if x >= bpp else 0
                line[x] = (line[x] + ((a + prev[x]) >> 1)) & 255
        elif f == 4:
            for x in range(stride):
                a = line[x - bpp] if x >= bpp else 0
                b = prev[x]
                c = prev[x - bpp] if x >= bpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else (b if pb <= pc else c))) & 255
        out[_ * stride:(_ + 1) * stride] = line
        prev = line
    if bpp == 4:
        rgb = bytearray(w * h * 3)
        rgb[0::3], rgb[1::3], rgb[2::3] = out[0::4], out[1::4], out[2::4]
        return w, h, bytes(rgb)
    return w, h, bytes(out)


def encode_png(path, w, h, pixels, channels=3):
    """Writes 8-bit RGB (or RGBA) pixels as a PNG (filter 'up' per row, deterministic)."""
    stride = w * channels
    rows, prev = bytearray(), bytes(stride)
    for y in range(h):
        line = pixels[y * stride:(y + 1) * stride]
        rows.append(2)
        rows += bytes((line[i] - prev[i]) & 255 for i in range(stride))
        prev = line
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    ctype = 2 if channels == 3 else 6
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, ctype, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b"")
    Path(path).write_bytes(png)


# --- A 5x7 bitmap font for the sheet's labels --------------------------------------------------------
FONT = {
    "A": "01110 10001 10001 11111 10001 10001 10001", "B": "11110 10001 10001 11110 10001 10001 11110",
    "C": "01110 10001 10000 10000 10000 10001 01110", "D": "11110 10001 10001 10001 10001 10001 11110",
    "E": "11111 10000 10000 11110 10000 10000 11111", "F": "11111 10000 10000 11110 10000 10000 10000",
    "G": "01110 10001 10000 10111 10001 10001 01111", "H": "10001 10001 10001 11111 10001 10001 10001",
    "I": "01110 00100 00100 00100 00100 00100 01110", "J": "00111 00010 00010 00010 00010 10010 01100",
    "K": "10001 10010 10100 11000 10100 10010 10001", "L": "10000 10000 10000 10000 10000 10000 11111",
    "M": "10001 11011 10101 10101 10001 10001 10001", "N": "10001 11001 10101 10011 10001 10001 10001",
    "O": "01110 10001 10001 10001 10001 10001 01110", "P": "11110 10001 10001 11110 10000 10000 10000",
    "Q": "01110 10001 10001 10001 10101 10010 01101", "R": "11110 10001 10001 11110 10100 10010 10001",
    "S": "01111 10000 10000 01110 00001 00001 11110", "T": "11111 00100 00100 00100 00100 00100 00100",
    "U": "10001 10001 10001 10001 10001 10001 01110", "V": "10001 10001 10001 10001 10001 01010 00100",
    "W": "10001 10001 10001 10101 10101 10101 01010", "X": "10001 10001 01010 00100 01010 10001 10001",
    "Y": "10001 10001 01010 00100 00100 00100 00100", "Z": "11111 00001 00010 00100 01000 10000 11111",
    "0": "01110 10001 10011 10101 11001 10001 01110", "1": "00100 01100 00100 00100 00100 00100 01110",
    "2": "01110 10001 00001 00010 00100 01000 11111", "3": "11111 00010 00100 00010 00001 10001 01110",
    "4": "00010 00110 01010 10010 11111 00010 00010", "5": "11111 10000 11110 00001 00001 10001 01110",
    "6": "00110 01000 10000 11110 10001 10001 01110", "7": "11111 00001 00010 00100 01000 01000 01000",
    "8": "01110 10001 10001 01110 10001 10001 01110", "9": "01110 10001 10001 01111 00001 00010 01100",
    ".": "00000 00000 00000 00000 00000 01100 01100", ",": "00000 00000 00000 00000 01100 00100 01000",
    ":": "00000 01100 01100 00000 01100 01100 00000", "/": "00001 00010 00010 00100 01000 01000 10000",
    "(": "00010 00100 01000 01000 01000 00100 00010", ")": "01000 00100 00010 00010 00010 00100 01000",
    "+": "00000 00100 00100 11111 00100 00100 00000", "-": "00000 00000 00000 11111 00000 00000 00000",
    "%": "11000 11001 00010 00100 01000 10011 00011", "_": "00000 00000 00000 00000 00000 00000 11111",
    "=": "00000 00000 11111 00000 11111 00000 00000", "'": "00100 00100 01000 00000 00000 00000 00000",
    "~": "00000 00000 01000 10101 00010 00000 00000", ";": "00000 01100 01100 00000 01100 00100 01000",
    " ": "00000 00000 00000 00000 00000 00000 00000", "x": "00000 00000 10001 01010 00100 01010 10001",
}


def draw_text(img, w, x0, y0, text, color, scale=1):
    """Draws `text` (upper-cased; unknown characters as spaces) into an RGB buffer."""
    x = x0
    for ch in text:
        g = FONT.get(ch if ch == "x" else ch.upper(), FONT[" "]).split()
        for gy, row in enumerate(g):
            for gx, bit in enumerate(row):
                if bit == "1":
                    for sy in range(scale):
                        for sx in range(scale):
                            px, py = x + gx * scale + sx, y0 + gy * scale + sy
                            if 0 <= px < w and 0 <= py * w < len(img) // 3:
                                k = (py * w + px) * 3
                                img[k:k + 3] = bytes(color)
        x += 6 * scale
    return x


def fill(img, w, x0, y0, bw, bh, color):
    row = bytes(color) * bw
    for y in range(y0, y0 + bh):
        k = (y * w + x0) * 3
        img[k:k + bw * 3] = row


def blit(img, w, x0, y0, src, sw, sh, dw, dh):
    """`src` (RGB, sw x sh) scaled to dw x dh by nearest neighbour into img at (x0, y0)."""
    for y in range(dh):
        sy = y * sh // dh
        srow = sy * sw
        k = ((y0 + y) * w + x0) * 3
        out = bytearray(dw * 3)
        for x in range(dw):
            s = (srow + x * sw // dw) * 3
            out[x * 3:x * 3 + 3] = src[s:s + 3]
        img[k:k + dw * 3] = out


# --- Decal and particle textures (deterministic, generated as data for the set) ----------------------
def _hash(ix, iy, seed):
    h = (ix * 374761393 + iy * 668265263 + seed * 2147483647) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((h ^ (h >> 16)) & 0xFFFFFF) / float(0x1000000)


def _vnoise(x, y, seed):
    ix, iy = math.floor(x), math.floor(y)
    fx, fy = x - ix, y - iy
    ux, uy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    a, b = _hash(ix, iy, seed), _hash(ix + 1, iy, seed)
    c, d = _hash(ix, iy + 1, seed), _hash(ix + 1, iy + 1, seed)
    return (a + (b - a) * ux) + ((c + (d - c) * ux) - (a + (b - a) * ux)) * uy


def _fbm(x, y, seed, octaves=4):
    v, amp, tot = 0.0, 1.0, 0.0
    for o in range(octaves):
        v += _vnoise(x, y, seed + o * 17) * amp
        tot += amp
        x, y, amp = x * 2.0, y * 2.0, amp * 0.5
    return v / tot


def _smooth(e0, e1, v):
    t = min(max((v - e0) / (e1 - e0), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def make_fx(dest):
    """The set's decal and particle textures as RGBA PNGs under `dest` (fx/...): moss patches,
    lichen crusts, grime and a ground stain (multiply), wear scuffs, a leaf. Greyscale where the
    decal's palette colour tints them."""
    dest.mkdir(parents=True, exist_ok=True)
    n = 128

    def write(name, fn, size=n):
        px = bytearray()
        for y in range(size):
            for x in range(size):
                px += bytes(max(0, min(255, int(round(c * 255)))) for c in fn(x / size, y / size))
        encode_png(dest / name, size, size, bytes(px), channels=4)

    def moss(u, v):
        r = math.hypot(u - 0.5, v - 0.5) * 2
        f = _fbm(u * 6, v * 6, 7)
        lump = _fbm(u * 18, v * 18, 9, 3)
        a = _smooth(0.42, 0.52, f * 0.75 + (1 - r) * 0.45 - 0.05) * (0.75 + 0.25 * lump)
        g = 0.72 + 0.28 * lump
        return (g, g, g, a)

    def lichen(u, v):
        a = 0.0
        for i in range(9):
            cx, cy = 0.15 + 0.7 * _hash(i, 1, 3), 0.15 + 0.7 * _hash(i, 2, 3)
            rad = 0.06 + 0.08 * _hash(i, 3, 3)
            d = math.hypot(u - cx, v - cy) / rad
            ring = _smooth(1.0, 0.85, d) * (0.55 + 0.45 * _smooth(0.4, 0.9, d))
            a = max(a, ring * (0.7 + 0.3 * _fbm(u * 30, v * 30, 5, 2)))
        g = 0.85 + 0.15 * _fbm(u * 20, v * 20, 4, 2)
        return (g, g, g * 0.92, a)

    def grime(u, v):
        # Darkest at the bottom of the box's image (+Z down the image = the ground), streaking up.
        streak = _fbm(u * 14, v * 2.5, 11, 3)
        h = v + (streak - 0.5) * 0.35
        a = _smooth(0.35, 1.0, h) * (0.55 + 0.45 * _fbm(u * 8, v * 8, 12, 3))
        return (0.22, 0.19, 0.16, min(a * 1.2, 0.95))

    def stain(u, v):
        r = math.hypot(u - 0.5, v - 0.5) * 2
        f = _fbm(u * 5, v * 5, 21)
        a = _smooth(0.0, 0.45, (1 - r) * 0.7 + f * 0.3 - 0.15) * 0.75
        return (0.34, 0.32, 0.28, a)

    def scuffs(u, v):
        a = 0.0
        for i in range(14):
            y0, x0 = _hash(i, 7, 5), _hash(i, 8, 5)
            ln = 0.15 + 0.3 * _hash(i, 9, 5)
            slope = (_hash(i, 10, 5) - 0.5) * 0.3
            if x0 <= u <= x0 + ln:
                d = abs(v - (y0 + (u - x0) * slope))
                a = max(a, _smooth(0.012, 0.0, d) * _smooth(0.0, 0.05, min(u - x0, x0 + ln - u)) * 0.9)
        return (1.0, 1.0, 1.0, a)

    def leaf(u, v):
        # Pointed at both ends, along the image's vertical, with a midrib.
        x, y = (u - 0.5) * 2, (v - 0.5) * 2
        half = 0.55 * (1 - y * y) ** 0.8 if abs(y) < 1 else 0
        a = _smooth(half, half - 0.06, abs(x)) if half > 0 else 0.0
        rib = 0.8 if abs(x) < 0.04 else 1.0
        return (rib, rib, rib, a)

    write("moss_patch.png", moss)
    write("lichen.png", lichen)
    write("grime.png", grime)
    write("stain.png", stain)
    write("scuffs.png", scuffs)
    write("leaf.png", leaf, 64)


# --- The studio's cameras (ported from crates/specula/src/studio.rs, so tiles match its shots) ------
def v_add(a, b):
    return [a[i] + b[i] for i in range(3)]


def v_sub(a, b):
    return [a[i] - b[i] for i in range(3)]


def v_mul(a, s):
    return [x * s for x in a]


def dot(a, b):
    return sum(a[i] * b[i] for i in range(3))


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def norm(a):
    n = math.sqrt(dot(a, a))
    return [x / n for x in a]


def dir_of(az, el):
    a, e = math.radians(az), math.radians(el)
    return [math.cos(e) * math.sin(a), math.sin(e), math.cos(e) * math.cos(a)]


def corners(lo, hi):
    return [[hi[0] if i & 1 else lo[0], hi[1] if i & 2 else lo[1], hi[2] if i & 4 else lo[2]] for i in range(8)]


def fit(lo, hi, az, el, fov, aspect, margin):
    """studio::fit: a camera from az/el that fits the box in a view of `aspect`."""
    centre = v_mul(v_add(lo, hi), 0.5)
    d = dir_of(az, el)
    fwd = v_mul(d, -1)
    c = cross(fwd, [0, 1, 0])
    right = [1, 0, 0] if math.sqrt(dot(c, c)) < 1e-3 else norm(c)
    up = norm(cross(right, fwd))
    ex = ey = ez = 0.0
    for p in corners(lo, hi):
        q = v_sub(p, centre)
        ex, ey, ez = max(ex, abs(dot(q, right))), max(ey, abs(dot(q, up))), max(ez, dot(q, d))
    tv = math.tan(math.radians(fov) * 0.5)
    th = tv * max(aspect, 0.05)
    dist = max(ex / th, ey / tv, 0.05) * margin + ez
    return {"eye": v_add(centre, v_mul(d, dist)), "target": centre, "fov_y_deg": fov}


def game_shot(lo, hi, cam, viewport_h):
    """studio::game_cam: the game camera on the subject, cropped around it at true pixel size.
    Returns the camera (fov narrowed to the crop), the crop's side and its viewport scale."""
    centre = v_mul(v_add(lo, hi), 0.5)
    radius = max(math.sqrt(dot(v_sub(hi, lo), v_sub(hi, lo))) * 0.5, 0.05)
    eye = v_add(centre, v_mul(dir_of(cam["yaw"], cam["pitch"]), cam["distance"]))
    px = min(radius / (cam["distance"] * math.tan(math.radians(cam["fov"]) * 0.5)) * viewport_h, viewport_h * 8.0)
    want = math.ceil(px * 1.3) + 10
    if want >= viewport_h:
        crop = min(viewport_h, CROP_CLASSES[-1])
        return {"eye": eye, "target": centre, "fov_y_deg": cam["fov"]}, crop, viewport_h / crop
    crop = min(next((c for c in CROP_CLASSES if c >= want), CROP_CLASSES[-1]), viewport_h)
    fov = 2.0 * math.degrees(math.atan(math.tan(math.radians(cam["fov"]) * 0.5) * crop / viewport_h))
    return {"eye": eye, "target": centre, "fov_y_deg": fov}, crop, 1.0


def rig_env(rig, cam):
    """studio::environment for a rig and camera, as world_env arguments."""
    d = v_sub(cam["eye"], cam["target"])
    yaw = math.degrees(math.atan2(d[0], d[2]))
    az, el = math.radians(yaw + rig["key_azimuth_deg"]), math.radians(rig["key_elevation_deg"])
    reach = math.sqrt(dot(d, d)) * 2.0 + 8.0
    return {
        "sun_direction": [math.cos(el) * math.sin(az), math.sin(el), math.cos(el) * math.cos(az)],
        "sun_color": rig["key_color"], "sun_intensity": rig["key_intensity"],
        "ambient_color": rig["ambient_color"], "ambient_intensity": rig["ambient_intensity"],
        "exposure": rig["exposure"], "tonemap": True, "background": rig["background"],
        "shadow": {"enabled": rig["shadows"], "resolution": 2048, "cascades": 1, "distance": reach, "softness": 1.5, "bias": 1.0},
        "bloom": {"enabled": rig["bloom"], "threshold": 1.0, "intensity": 0.6}, "fxaa": True,
    }


# --- The run -----------------------------------------------------------------------------------------
class Run:
    def __init__(self, m, out, check):
        self.m, self.out, self.check = m, out, check
        self.caps = out / "t3_captures"
        self.caps.mkdir(parents=True, exist_ok=True)
        self.rig_now = None

    def frame(self, label, cam, w, h, **extra):
        """A capture of `w` x `h` through `cam`. sight_frame's narrowest field of view is 10
        degrees: a crop narrower than that (a small subject at true pixel size) renders a larger
        square through exactly 10+ degrees at the same pixels per degree, and keeps its centre."""
        fov, size = cam["fov_y_deg"], (w, h)
        if fov < 10.0:
            t = math.tan(math.radians(fov) * 0.5) / h  # tan per pixel at the crop's scale
            side = math.ceil(math.tan(math.radians(5.0)) / t)
            side += (side - w) % 2  # the crop sits exactly in the middle
            fov = 2.0 * math.degrees(math.atan(t * side))
            w = h = side
        r = self.m.call("sight_frame", width=w, height=h, eye=cam["eye"], target=cam["target"], fov_y_deg=fov,
                        time_s=TIME_S, max_subjects=8, **extra)
        png = next(c for c in r["content"] if c["type"] == "image")["data"]
        packet = json.loads(next(c for c in r["content"] if c["type"] == "text")["text"])
        path = self.caps / f"{label}.png"
        path.write_bytes(base64.b64decode(png))
        if (w, h) != size:
            sw, sh, px = decode_png(path)
            x0, y0 = (sw - size[0]) // 2, (sh - size[1]) // 2
            crop = b"".join(px[((y0 + y) * sw + x0) * 3:((y0 + y) * sw + x0 + size[0]) * 3] for y in range(size[1]))
            path.with_name(f"{label}_full.png").write_bytes(path.read_bytes())
            encode_png(path, size[0], size[1], crop)
            packet["t3_crop"] = {"rendered": [sw, sh], "fov_y_deg": round(fov, 4), "kept_centre": list(size)}
        (self.caps / f"{label}.json").write_text(json.dumps(packet, indent=1))
        return path, packet

    def wait_bake(self, timeout=600):
        start = time.time()
        while True:
            info = self.m.json("gi_info")
            if info.get("job") is None:
                vol = next((v for v in info["volumes"] if v["name"] == "stage"), None)
                if vol and vol.get("status") == "loading" and time.time() - start < 60:
                    time.sleep(0.2)
                    continue
                return info, time.time() - start
            if time.time() - start > timeout:
                raise RuntimeError(f"bake did not finish: {info['job']}")
            time.sleep(0.2)

    def wait_pipelines(self, timeout=600):
        start = time.time()
        while not self.m.json("gi_info")["runtime"].get("pipelines_ready"):
            if time.time() - start > timeout:
                raise RuntimeError("GI pipelines never became ready")
            time.sleep(0.5)

    def gpu_frames(self, cam, w, h, n, into):
        """`n` offscreen captures (no ID buffer) after a warm-up one; GPU ms (total, per pass) into `into`."""
        for i in range(n + 1):
            self.m.call("sight_frame", width=w, height=h, eye=cam["eye"], target=cam["target"], fov_y_deg=cam["fov_y_deg"],
                        time_s=TIME_S, ids=False, max_subjects=1)
            g = ((self.m.json("perf_frame", response_format="detailed").get("offscreen") or {}).get("gpu")) or {}
            if i == 0 or not g:
                continue  # the first frame after a change carries uploads and pipeline waits
            into.setdefault("total_ms", []).append(g.get("total_ms", 0.0))
            for k, v in (g.get("passes") or {}).items():
                into.setdefault(k, []).append(v)

    def gpu_ab(self, cam, w, h, homes):
        """The subject's GPU cost on a GPU other work shares: captures with the subject (and its
        dressing) in place and moved 1 km away, alternated GPU_ROUNDS times, so contention hits
        both alike. Reports the lowest (closest to the real cost) and the median of each."""
        on, off = {}, {}
        for _ in range(GPU_ROUNDS):
            for side, into in ((0, on), (1, off)):
                for p, pos in homes.items():
                    self.m.json("world_set", path=p, position=[pos[0] + 1000.0 * side, pos[1], pos[2] + 1000.0 * side])
                self.gpu_frames(cam, w, h, GPU_REPEATS, into)
        for p, pos in homes.items():
            self.m.json("world_set", path=p, position=pos)
        lo =lambda xs: round(min(xs), 3) if xs else None
        med = lambda xs: round(sorted(xs)[len(xs) // 2], 3) if xs else None
        pack = lambda d: {"lowest_ms": lo(d.get("total_ms", [])), "median_ms": med(d.get("total_ms", [])), "frames": len(d.get("total_ms", [])),
                          "passes_lowest": {k: lo(v) for k, v in d.items() if k != "total_ms"}}
        a, b = pack(on), pack(off)
        delta = lambda k: round(a[k] - b[k], 3) if a[k] is not None and b[k] is not None else None
        return {"with": a, "without": b, "added_lowest_ms": delta("lowest_ms"), "added_median_ms": delta("median_ms"), "other_engines": other_engines()}


def run(exe, port, adapter, out, check, failures):
    out = Path(out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    make_fx(ASSETS / "fx")
    assets = out / "t3_assets"
    shutil.rmtree(assets, ignore_errors=True)
    shutil.copytree(ASSETS, assets)
    stage = json.loads((ASSETS / "stage_t3.json").read_text())
    summary = {"adapter": adapter, "time_s": TIME_S, "subjects": {}}
    cache = out / "t3_cache"
    shutil.rmtree(cache, ignore_errors=True)
    args = ["--headless", "--paused", "--assets", str(assets), "--captures", str(out / "captures"), "--cache", str(cache),
            "--adapter", adapter, "--lead", "fidelity-lead"]
    with Engine(exe, port, out / "t3_engine.log", *args, cwd=str(out)) as engine:
        m = engine.mcp("fidelity-t3", detailed=True)
        m.json("agent_hello", name="fidelity-lead", role="lead")
        R = Run(m, out, check)
        desc = m.json("studio_stage")
        rigs = desc["rigs"]
        g = re.search(r"pitch ([\d.]+) deg, yaw ([\d.-]+) deg, ([\d.]+) m, fov ([\d.]+) deg, (\d+)x(\d+)", desc["shots"]["gameplay"])
        cam = {"pitch": float(g[1]), "yaw": float(g[2]), "distance": float(g[3]), "fov": float(g[4]), "viewport": [int(g[5]), int(g[6])]}
        summary["stage"] = {"rigs": rigs, "game_camera": cam, "shots": desc["shots"]}

        # Taste and rules on the studio's own stage first.
        sc = m.json("style_check", folder="recipes", response_format="detailed")
        bad = {r["subject"]: [e["check"] for e in r["errors"]] for r in sc["results"] if not r["passed"]}
        check(sc["passed"], f"style_check: {sc['subjects']} benchmark recipes keep the set's style.toml {bad or ''}")
        summary["style_check"] = {"subjects": sc["subjects"], "failed": sc["failed"]}
        facts = {}
        for name in SUBJECTS:
            for tier in TIERS:
                r = m.json("studio_check", recipe=f"recipes/{name}_{tier}.json", siblings=[], image=tier == "t3", response_format="detailed")
                facts[(name, tier)] = r["facts"]
                if tier == "t3":
                    summary["subjects"].setdefault(name, {})["studio_check"] = {
                        "passed": r["passed"], "errors": [(e["check"], e.get("value")) for e in r["errors"]],
                        "marginal": [c["check"] for c in r["report"]["all"] if c.get("marginal")],
                        "px_gameplay": r["measures"]["gameplay"]["px_height"], "contrast": r["measures"]["gameplay"]["contrast"],
                        "contrast_worst": r["measures"]["worst_light"]["contrast"], "spread": r["measures"]["game_light"]["spread"],
                        "solidity": r["measures"]["thumbnails"]["solidity"], "sheet": rel(r["image"]["path"])}
                    check(r["passed"], f"{name} T3 studio_check: {r['failed']} errors {[e['check'] for e in r['errors']]}")
        # The library effects (journaled world data, like any agent's).
        for ename, file in stage["effects"].items():
            m.json("vfx_set", name=ename, effect=json.loads((ASSETS / file).read_text()))
            # The effect's colours against its style family (fire for the lantern, leaves for the tree).
            pal = json.loads((ASSETS / "style.json").read_text())["palette"]
            family = ["flame", "flame_core"] if ename.startswith("lantern") else ["leaf", "leaf_dark", "leaf_light"]
            chk = m.json("vfx_check", effect=ename, palette=[pal[k] for k in family])
            failed = chk.get("failed") or []
            summary.setdefault("vfx_check", {})[ename] = {"ok": chk.get("ok"), "failed": failed, "passed": [c.get("check") for c in chk.get("passed") or []]}
            check(chk.get("ok") is True and not failed, f"vfx_check {ename} (palette {family}): {[(e.get('check'), e.get('value')) for e in failed] or 'all checks pass'}")
        gi_settings = m.json("gi_settings")
        summary["gi_settings"] = {k: gi_settings.get(k) for k in ("quality", "default", "adapter_default", "tier") if k in gi_settings}

        m.json("world_spawn", shape="group", path="/stage")
        m.json("world_spawn", shape="quad", path="/stage/ground", position=[0, 0, 0], scale=[200, 1, 200], color=rigs["game"]["ground"], roughness=0.9)
        tiles = {}
        for name in SUBJECTS:
            dress = stage["subjects"][name]
            row = summary["subjects"].setdefault(name, {})
            set_paths = []
            for piece in dress.get("set", []):
                p = f"/stage/set_{piece['name']}"
                m.json("world_spawn", path=p, **{k: v for k, v in piece.items() if k != "name"})
                set_paths.append(p)
            for tier in TIERS:
                model = m.json("model_build", recipe=f"recipes/{name}_{tier}.json")
                lo, hi = model["bounds"]["min"], model["bounds"]["max"]
                subject = f"/stage/{name}_{tier}"
                spawn = dict(shape="model", path=subject, recipe=f"recipes/{name}_{tier}.json", position=[0, 0, 0])
                if tier == "t3" and dress.get("light"):
                    spawn["light"] = dress["light"]
                if tier == "t3" and dress.get("anim"):
                    spawn["anim"] = dress["anim"]
                m.json("world_spawn", **spawn)
                dressing = []
                if tier == "t3":
                    for d in dress.get("decals", []):
                        p = f"{subject}_decal_{d['name']}"
                        m.json("world_spawn", shape="group", path=p, position=d["position"], rotation_deg=d["rotation_deg"], decal=d["decal"])
                        dressing.append(p)
                    for e in dress.get("effects", []):
                        p = f"{subject}_fx_{e['name']}"
                        m.json("world_spawn", shape="group", path=p, position=e["position"], vfx=e["vfx"])
                        dressing.append(p)
                close = fit(lo, hi, FIT_AZ, FIT_EL, FIT_FOV, 1.0, FIT_MARGIN)
                vh = cam["viewport"][1]
                game, crop, scale = game_shot(lo, hi, cam, vh)
                shots = {"close_up": (close, CLOSE_PX, "neutral"), "gameplay": (game, crop, "game"), "worst_light": (game, crop, "worst")}
                for shot in SHOTS:
                    c, px, rig_name = shots[shot]
                    rig = rigs[rig_name]
                    m.json("world_env", **rig_env(rig, c))
                    m.json("world_set", path="/stage/ground", color=rig["ground"])
                    extra = {"gi": "off"} if tier != "t3" else {}
                    if tier == "t3":
                        gb = dress["gi"]
                        m.json("gi_bake", volume="stage", min=gb["min"], max=gb["max"], spacing=gb["spacing"], rays=256, bounces=2)
                        info, wall = R.wait_bake()
                        R.wait_pipelines()
                        vol = next(v for v in info["volumes"] if v["name"] == "stage")
                        row.setdefault("gi", {})[rig_name] = {"status": vol.get("status"), "probes": (vol.get("baked") or {}).get("probes"),
                                                             "hash": (vol.get("baked") or {}).get("hash"), "bake_wall_s": round(wall, 2),
                                                             "tier": info["runtime"].get("quality") or info["runtime"].get("tier")}
                    path, packet = R.frame(f"{name}_{tier}_{shot}", c, px, px, **extra)
                    tiles[(name, tier, shot)] = (path, scale if shot != "close_up" else 1.0, packet)
                    if tier == "t3":
                        fx = packet.get("effects") or {}
                        row.setdefault("packets", {})[shot] = {
                            "gi": packet.get("gi"), "effects": len(fx.get("effects", [])), "decals": len(fx.get("decals", [])), "decals_skipped": (fx.get("frame") or {}).get("decals_skipped"), "decal_fill_screens": (fx.get("frame") or {}).get("decal_fill_screens"),
                            "particles": sum(e.get("live", e.get("particles", 0)) or 0 for e in fx.get("effects", [])),
                            "warnings": packet.get("warnings"), "packet": rel(Path(str(path)[:-4] + ".json"))}
                    if tier == "t3" and shot == "gameplay":
                        # The indirect light alone, and the same view without GI (what the bake adds).
                        # The irradiance view draws decals too (they're re-lit by it): moved away for it,
                        # so what it shows is the bake's light alone.
                        decals = {f"{subject}_decal_{d['name']}": d["position"] for d in dress.get("decals", [])}
                        for p, pos in decals.items():
                            m.json("world_set", path=p, position=[pos[0] + 1000.0, pos[1], pos[2] + 1000.0])
                        ipath, _ = R.frame(f"{name}_t3_gameplay_irradiance", c, px, px, gi="irradiance")
                        for p, pos in decals.items():
                            m.json("world_set", path=p, position=pos)
                        tiles[(name, "t3", "irradiance")] = (ipath, scale, None)
                        opath, _ = R.frame(f"{name}_t3_gameplay_gi_off", c, px, px, gi="off")
                        row["gi_off_capture"] = rel(opath)
                        row["gi_effect"] = image_delta(path, opath)
                if tier == "t3":
                    # iGPU cost: the game camera's whole 1280x720 view and the close-up filling it,
                    # with the subject and its dressing, then without (the stage alone).
                    rig = rigs["game"]
                    full_game = {"eye": game["eye"], "target": game["target"], "fov_y_deg": cam["fov"]}
                    m.json("world_env", **rig_env(rig, full_game))
                    m.json("world_set", path="/stage/ground", color=rig["ground"])
                    gb = dress["gi"]
                    m.json("gi_bake", volume="stage", min=gb["min"], max=gb["max"], spacing=gb["spacing"], rays=256, bounces=2)
                    R.wait_bake()
                    R.wait_pipelines()
                    close16 = fit(lo, hi, FIT_AZ, FIT_EL, FIT_FOV, 16 / 9, FIT_MARGIN)
                    homes = {subject: [0.0, 0.0, 0.0]}
                    homes.update({f"{subject}_decal_{d['name']}": d["position"] for d in dress.get("decals", [])})
                    homes.update({f"{subject}_fx_{e['name']}": e["position"] for e in dress.get("effects", [])})
                    R.gpu_frames(full_game, 1280, 720, 1, {})
                    perf = m.json("perf_frame", response_format="detailed")
                    row["gpu"] = {"gameplay_720p": R.gpu_ab(full_game, 1280, 720, homes), "close_up_720p": R.gpu_ab(close16, 1280, 720, homes),
                                  "draws": (perf.get("offscreen") or {}).get("draws"), "method": f"{GPU_ROUNDS} rounds alternating with and without the subject, {GPU_REPEATS} frames each"}
                    if name == "lantern":
                        row["bounce"] = lantern_bounce(m, R, dress, subject)
                    f3 = facts[(name, "t3")]
                    row["budget"] = {"triangles": f3.get("triangles"), "texture_kb": f3.get("texture_kb"), "materials": len(f3.get("materials", [])),
                                     "size_m": f3.get("size_m")}
                for p in ([subject] if subject else []) + dressing:
                    m.json("world_delete", path=p)
            for p in set_paths:
                m.json("world_delete", path=p)
            row["tiers"] = {t: {"triangles": facts[(name, t)].get("triangles"), "texture_kb": facts[(name, t)].get("texture_kb")} for t in TIERS}
        summary["gi_info"] = m.json("gi_info").get("runtime")
        m.close()
    v = engine.validation_errors()
    check(v == 0, f"zero validation errors at shutdown ({v})")
    summary["validation_errors"] = v
    sheet = compose(tiles, summary, cam)
    summary["sheet"] = rel(sheet)
    (FIDELITY / "benchmark_t3_m4.json").write_text(json.dumps(scrub(summary), indent=1) + "\n")
    (out / "t3_summary.json").write_text(json.dumps(summary, indent=1, default=str))
    return summary


def other_engines():
    """How many other specula/sepulcra processes are running: other sessions' engines share the
    iGPU and skew GPU timings (recorded with each measurement; 0 where it can't be told)."""
    if sys.platform != "win32":
        return 0
    import subprocess
    n = 0
    for image in ("specula.exe", "sepulcra.exe"):
        r = subprocess.run(["tasklist", "/FI", f"IMAGENAME eq {image}", "/FO", "CSV", "/NH"], capture_output=True, text=True)
        n += sum(1 for line in r.stdout.splitlines() if line.startswith('"'))
    return max(n - 1, 0)  # this run's own engine


def image_delta(a, b):
    """How much two captures of one view differ: the share of pixels whose luma moved more than
    4/255, the mean luma change (b to a, so positive = a brighter) and its largest value."""
    wa, ha, pa = decode_png(a)
    wb, hb, pb = decode_png(b)
    if (wa, ha) != (wb, hb):
        return None
    n, changed, total, peak = wa * ha, 0, 0.0, 0.0
    for i in range(0, n * 3, 3):
        la = 0.2126 * pa[i] + 0.7152 * pa[i + 1] + 0.0722 * pa[i + 2]
        lb = 0.2126 * pb[i] + 0.7152 * pb[i + 1] + 0.0722 * pb[i + 2]
        d = la - lb
        total += d
        peak = max(peak, abs(d))
        if abs(d) > 4:
            changed += 1
    return {"changed_share": round(changed / n, 4), "mean_luma_delta": round(total / n, 2), "max_luma_delta": round(peak, 1), "pixels": n}


def scrub(v):
    """Committed numbers carry no absolute paths."""
    if isinstance(v, dict):
        return {k: scrub(x) for k, x in v.items()}
    if isinstance(v, list):
        return [scrub(x) for x in v]
    if isinstance(v, str) and (":\\" in v or ":/" in v or v.startswith("/")) and not v.startswith("/stage"):
        return "<out>/" + v.replace("\\", "/").rsplit("/", 1)[-1]
    return v


def lantern_bounce(m, R, dress, subject):
    """The lantern light's bounce on its walls under the game rig: gi_check at points on the walls
    and the ground, baked with the light and again with it off (the difference is the bounce)."""
    pts = [{"p": [0.0, 0.35, -0.385], "n": [0, 0, 1]}, {"p": [-0.435, 0.35, 0.0], "n": [1, 0, 0]},
           {"p": [-0.25, 0.0, -0.2], "n": [0, 1, 0]}, {"p": [0.5, 0.0, 0.5], "n": [0, 1, 0]}]
    lit = m.json("gi_check", volume="stage", points=pts)["points"]
    m.json("world_set", path=subject, light=dict(dress["light"], intensity=0.0))
    m.json("gi_bake", volume="stage")
    R.wait_bake()
    dark = m.json("gi_check", volume="stage", points=pts)["points"]
    m.json("world_set", path=subject, light=dress["light"])
    m.json("gi_bake", volume="stage")
    R.wait_bake()
    names = ["back_wall", "side_wall", "ground_corner", "ground_open"]
    return [{"at": n, "with_light": a.get("luminance"), "without": b.get("luminance"),
             "bounce": round((a.get("luminance") or 0) - (b.get("luminance") or 0), 4)} for n, a, b in zip(names, lit, dark)]


def plural(n, word):
    return f"{n} {word}" + ("" if n == 1 else "S")


def compose(tiles, summary, cam):
    """The sheet: per subject a row of close-up, gameplay and worst-light groups (T0, T1, T3 each)
    and T3's indirect light, with the budget line under it."""
    pad, head, label_h = 6, 54, 40
    groups = [("CLOSE-UP (NEUTRAL RIG)", "close_up"), ("GAMEPLAY (GAME RIG)", "gameplay"), ("WORST LIGHT (BACKLIT)", "worst_light")]
    cols = len(groups) * len(TIERS) + 1
    gap = 10
    W = pad * 2 + cols * TILE + (len(groups)) * gap
    H = head + len(SUBJECTS) * (TILE + label_h + pad) + pad
    img = bytearray(bytes((14, 16, 20)) * (W * H))
    title = "FIDELITY T3 (M4): DETAILED STYLISED - SDF SCULPTS, NORMAL MAPS, BAKED GI, DECALS, PARTICLES. T0 / T1 / T3 IN THE STAGE WORLD, STUDIO SHOTS"
    draw_text(img, W, pad, 6, title, (230, 232, 236))
    gm = summary["stage"]["game_camera"]
    sub = (f"GAME CAMERA {gm['pitch']:.0f} DEG DOWN, {gm['distance']:.0f} M, FOV {gm['fov']:.0f}, {gm['viewport'][0]}x{gm['viewport'][1]}: "
           f"CROPS AT TRUE PIXEL SIZE, MAGNIFIED. ADAPTER {summary['adapter'].upper()}, GI TIER {str((summary.get('gi_info') or {}).get('quality', '?')).upper()}. T={TIME_S:.0f} S")
    draw_text(img, W, pad, 18, sub, (150, 156, 166))
    x = pad
    for gname, _ in groups:
        draw_text(img, W, x + 2, 34, gname, (200, 205, 214))
        for i, t in enumerate(TIERS):
            draw_text(img, W, x + i * TILE + 4, 44, t.upper(), (120, 200, 230) if t == "t3" else (150, 156, 166))
        x += len(TIERS) * TILE + gap
    draw_text(img, W, x + 2, 34, "T3 GI: INDIRECT", (200, 205, 214))
    draw_text(img, W, x + 4, 44, "GAMEPLAY", (150, 156, 166))
    y = head
    for name in SUBJECTS:
        x = pad
        for _, shot in groups:
            for t in TIERS:
                path, scale, _ = tiles[(name, t, shot)]
                w, h, px = decode_png(path)
                blit(img, W, x, y, px, w, h, TILE, TILE)
                if scale != 1.0:
                    draw_text(img, W, x + 3, y + TILE - 10, f"x{scale:.1f} VIEW", (240, 240, 240))
                x += TILE
            x += gap
        path, scale, _ = tiles[(name, "t3", "irradiance")]
        w, h, px = decode_png(path)
        blit(img, W, x, y, px, w, h, TILE, TILE)
        row = summary["subjects"][name]
        b, gp = row.get("budget", {}), row.get("gpu", {})
        tri = b.get("triangles") or [0]
        t01 = row.get("tiers", {})
        line1 = (f"{name.replace('_', ' ').upper()}  T0 {t01.get('t0', {}).get('triangles', [0])[0]:,} / T1 {t01.get('t1', {}).get('triangles', [0])[0]:,} / "
                 f"T3 {tri[0]:,} TRIS (T3 LODS {' '.join(f'{v:,}' for v in tri[1:])}), TEXTURES {b.get('texture_kb', 0):.0f} KB, {b.get('materials', 0)} MATERIALS")
        pk = (row.get("packets") or {}).get("gameplay") or {}
        fmt = lambda v: "?" if v is None else f"{v:.2f}"
        sgn = lambda v: "?" if v is None else f"{v:+.2f}"
        gpl, gcl = gp.get("gameplay_720p") or {}, gp.get("close_up_720p") or {}
        line2 = (f"IGPU 1280x720 GPU (LOWEST OF {(gpl.get('with') or {}).get('frames', 0)}): GAMEPLAY VIEW {fmt((gpl.get('with') or {}).get('lowest_ms'))} MS, "
                 f"{sgn(gpl.get('added_lowest_ms'))} OVER THE BARE STAGE; CLOSE-UP FILLING IT {fmt((gcl.get('with') or {}).get('lowest_ms'))} MS, {sgn(gcl.get('added_lowest_ms'))} ({gpl.get('other_engines', 0)} OTHER ENGINES ON THE GPU); "
                 f"GI {str((summary.get('gi_info') or {}).get('quality', '?')).upper()}; {plural(pk.get('decals', 0), 'DECAL')}, {plural(pk.get('effects', 0), 'EFFECT')} IN VIEW")
        ge = row.get("gi_effect") or {}
        parts = [f"T3 BAKED GI (GAMEPLAY, LIT AGAINST GI OFF): {100 * ge.get('changed_share', 0):.0f}% OF PIXELS CHANGED, MEAN {ge.get('mean_luma_delta', 0):+.1f} LUMA"]
        if row.get("bounce"):
            walls = [b["bounce"] for b in row["bounce"] if "wall" in b["at"]]
            parts.append(f"THE LANTERN'S LIGHT BOUNCES +{min(walls):.2f} TO +{max(walls):.2f} ONTO THE WALLS (PROBE LUMINANCE)")
        live = sum(e.get("particles", 0) for e in (row.get("packets") or {}).values()) // max(len(row.get("packets") or {}), 1)
        if live:
            parts.append(f"ABOUT {live} PARTICLES LIVE")
        draw_text(img, W, pad, y + TILE + 5, line1, (226, 228, 232))
        draw_text(img, W, pad, y + TILE + 16, line2, (160, 200, 170))
        draw_text(img, W, pad, y + TILE + 27, "; ".join(parts), (170, 190, 215))
        y += TILE + label_h + pad
    path = FIDELITY / "benchmark_t3_m4.png"
    encode_png(path, W, H, bytes(img))
    return path


def default_exe():
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    return str(target / "release" / ("specula.exe" if sys.platform == "win32" else "specula"))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--exe", default=default_exe())
    ap.add_argument("--port", type=int, default=7850)
    ap.add_argument("--out", default=str(ROOT / "target" / "fidelity-t3"))
    ap.add_argument("--adapter", default="integrated")
    a = ap.parse_args()
    failures = []

    def check(ok, what):
        print(("  ok   " if ok else "  FAIL ") + what)
        if not ok:
            failures.append(what)

    run(a.exe, a.port, a.adapter, a.out, check, failures)
    print(f"\nsheet: {FIDELITY / 'benchmark_t3_m4.png'}")
    print("FAILED: " + "; ".join(failures) if failures else "fidelity T3 checked")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
