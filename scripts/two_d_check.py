#!/usr/bin/env python3
"""2D check: builds a pixel-art 2D scene and a 3D scene with 2D elements over MCP, captures them on
every adapter, and checks pixels, subjects, persistence and validation.

    cargo build -p specula
    python scripts/two_d_check.py                 # every adapter, port 7492
    python scripts/two_d_check.py --adapters 1

Scene 1 (2D, orthographic `view_2d`): a 32 x 18 tile map with three layers (ground with an
animated pond, details, a canopy over the characters), animated sprites from a sheet, and a
vector-shape HUD panel in the world. Scene 2 (3D, perspective): lit, shadow-casting billboard
trees, blended spherical sparks, a tile path and a vector decal lying on the ground; plus an
orthographic three-quarter view of it. Then tilemap_set / tilemap_get, world_save + world_load
(same world_hash) and zero validation errors.

Generated art goes to target/two-d-check/assets, captures and logs to target/two-d-check/.
Open the PNGs and look at them. Standard library only; keep output ASCII.
"""
import argparse
import json
import os
import struct
import sys
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

DEFAULT_PORT = 7492


# ---------------------------------------------------------------------------------------------
# PNG in and out
# ---------------------------------------------------------------------------------------------

def png_bytes(w, h, rgba, level=9):
    raw = b"".join(b"\0" + bytes(rgba[y * w * 4:(y + 1) * w * 4]) for y in range(h))

    def chunk(kind, data):
        c = struct.pack(">I", len(data)) + kind + data
        return c + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, level)) + chunk(b"IEND", b""))


def read_png(path):
    """RGBA8 (colour type 6 or 2, 8 bits, no interlace) -> (w, h, bytearray)."""
    data = Path(path).read_bytes()
    pos, idat, w, h, bpp = 8, b"", 0, 0, 4
    while pos < len(data):
        n = struct.unpack(">I", data[pos:pos + 4])[0]
        kind, body = data[pos + 4:pos + 8], data[pos + 8:pos + 8 + n]
        if kind == b"IHDR":
            w, h, depth, ctype = struct.unpack(">IIBB", body[:10])
            assert depth == 8 and ctype in (2, 6), (depth, ctype)
            bpp = 4 if ctype == 6 else 3
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    raw = zlib.decompress(idat)
    stride = w * bpp
    out = bytearray(w * h * 4)
    prev = bytearray(stride)
    for y in range(h):
        f = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if f == 1:
                line[i] = (line[i] + a) & 255
            elif f == 2:
                line[i] = (line[i] + b) & 255
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        for x in range(w):
            px = line[x * bpp:x * bpp + bpp]
            out[(y * w + x) * 4:(y * w + x) * 4 + 4] = bytes(px) + (b"" if bpp == 4 else b"\xff")
        prev = line
    return w, h, out


def pixel(img, x, y):
    w, _, px = img
    i = (int(y) * w + int(x)) * 4
    return tuple(px[i:i + 4])


# ---------------------------------------------------------------------------------------------
# Art (deterministic pixel art, generated)
# ---------------------------------------------------------------------------------------------

def noise(x, y, s):
    v = (x * 374761393 + y * 668265263 + s * 2147483647) & 0xFFFFFFFF
    v = ((v ^ (v >> 13)) * 1274126177) & 0xFFFFFFFF
    return (v ^ (v >> 16)) & 255


class Canvas:
    def __init__(self, w, h):
        self.w, self.h, self.px = w, h, bytearray(w * h * 4)

    def set(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            self.px[(y * self.w + x) * 4:(y * self.w + x) * 4 + 4] = bytes(c)

    def save(self, path):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(png_bytes(self.w, self.h, self.px))


def shade(c, k):
    return tuple(max(0, min(255, int(v * k))) for v in c[:3]) + (c[3] if len(c) > 3 else 255,)


GRASS, DIRT, WATER, STONE, SAND = (86, 150, 66), (150, 112, 72), (52, 110, 190), (128, 132, 140), (214, 196, 140)


def tileset(path):
    """8 x 2 tiles of 16 px: 1 grass, 2 dirt, 3-4 water (animated), 5 stone wall, 6 flowers,
    7 bush, 8 canopy, 9 sand."""
    t = Canvas(128, 32)

    def tile(n, f):
        ox, oy = ((n - 1) % 8) * 16, ((n - 1) // 8) * 16
        for y in range(16):
            for x in range(16):
                c = f(x, y)
                if c:
                    t.set(ox + x, oy + y, c)

    tile(1, lambda x, y: shade(GRASS, 0.85 + noise(x, y, 1) / 900))
    tile(2, lambda x, y: shade(DIRT, 0.85 + noise(x, y, 2) / 900))
    for k, n in enumerate((3, 4)):
        tile(n, lambda x, y, k=k: shade(WATER, 1.25 if (x + 2 * y + 6 * k) % 12 < 2 else 0.95 + noise(x, y, 3) / 2000))
    tile(5, lambda x, y: shade(STONE, 0.6 if (y % 8 == 7 or (x + (y // 8) * 8) % 16 == 0) else 0.95 + noise(x, y, 5) / 1500))

    def flowers(x, y):
        h = noise(x // 3, y // 3, 6)
        if h < 40 and x % 3 == 1 and y % 3 == 1:
            return (240, 220, 80, 255) if h < 20 else (230, 90, 120, 255)
        return None

    tile(6, flowers)
    tile(7, lambda x, y: shade((48, 112, 52), 0.8 + noise(x, y, 7) / 800) if (x - 7.5) ** 2 + (y - 9) ** 2 < 42 else None)
    tile(8, lambda x, y: shade((34, 92, 46), 0.75 + noise(x, y, 8) / 700) if (x - 7.5) ** 2 + (y - 7.5) ** 2 < 60 else None)
    tile(9, lambda x, y: shade(SAND, 0.92 + noise(x, y, 9) / 1500))
    t.save(path)


def hero(path):
    """A 4-frame walk sheet, 16 x 16 each: a cloaked figure, legs and arms swinging."""
    c = Canvas(64, 16)
    for f in range(4):
        ox = f * 16
        swing = [-1, 0, 1, 0][f]
        for y in range(16):
            for x in range(16):
                col = None
                if 3 <= y <= 5 and 6 <= x <= 9:
                    col = (236, 200, 160, 255)  # face
                elif 1 <= y <= 2 and 5 <= x <= 10:
                    col = (120, 60, 30, 255)  # hair
                elif 6 <= y <= 11 and 5 <= x <= 10:
                    col = (60, 90, 170, 255)  # cloak
                elif 7 <= y <= 10 and (x == 4 + (swing < 0)) or 7 <= y <= 10 and x == 11 - (swing > 0):
                    col = (236, 200, 160, 255)  # hands
                elif 12 <= y <= 15 and x in (6 + swing, 9 - swing):
                    col = (60, 40, 30, 255)  # legs
                if col:
                    c.set(ox + x, y, col)
    c.save(path)


def tree(path):
    c = Canvas(32, 48)
    for y in range(48):
        for x in range(32):
            if y >= 30 and 13 <= x <= 18:
                c.set(x, y, shade((96, 64, 40), 0.85 + noise(x, y, 11) / 1000))
            elif (x - 15.5) ** 2 / 15 ** 2 + (y - 17) ** 2 / 16 ** 2 < 1 and noise(x, y, 12) > 20:
                c.set(x, y, shade((46, 120, 58), 0.7 + noise(x // 2, y // 2, 13) / 600))
    c.save(path)


def spark(path):
    c = Canvas(16, 16)
    for y in range(16):
        for x in range(16):
            d = ((x - 7.5) ** 2 + (y - 7.5) ** 2) ** 0.5 / 8
            a = max(0.0, 1 - d) ** 2
            c.set(x, y, (255, 220, 140, int(a * 255)))
    c.save(path)


# ---------------------------------------------------------------------------------------------
# Scenes
# ---------------------------------------------------------------------------------------------

def rows_2d():
    """Three layers of a 32 x 18 map: ground (grass, a dirt path, a sand-lined pond), details
    (flowers, bushes), canopy (trees over row 3..6)."""
    ground, details, canopy = [], [], []
    for y in range(18):
        g, d, c = [], [], []
        for x in range(32):
            pond = 20 <= x <= 27 and 10 <= y <= 15
            shore = 19 <= x <= 28 and 9 <= y <= 16
            g.append("3" if pond else "9" if shore else "2" if y == 8 or x == 12 else "1")
            dv = noise(x, y, 21)
            d.append("6" if not shore and y != 8 and x != 12 and dv < 30 else "7" if not shore and y != 8 and x != 12 and dv > 245 else "0")
            c.append("8" if 3 <= y <= 6 and x in (3, 4, 7, 8, 9) else "0")
        ground.append(",".join(g))
        details.append(",".join(d))
        canopy.append(",".join(c))
    return ground, details, canopy


def build_2d(m):
    m.call("world_spawn", shape="group", name="two_d")
    ground, details, canopy = rows_2d()
    m.call(
        "world_spawn",
        shape="tilemap",
        path="/two_d/map",
        position=[-16, 9, 0],
        color=[1, 1, 1],
        tilemap={
            "tileset": "tiles/tileset.png",
            "tile_px": [16, 16],
            "size": [32, 18],
            "pixel_snap": True,
            "animations": [{"tile": 3, "frames": 2, "frame_ms": 400}],
            "layers": [
                {"name": "ground", "z_layer": 0, "rows": ground},
                {"name": "details", "z_layer": 1, "rows": details},
                {"name": "canopy", "z_layer": 3, "rows": canopy},
            ],
        },
    )
    common = {
        "texture": "sprites/hero.png",
        "size": [1.5, 1.5],
        "pivot": [0.5, 0.0],
        "sheet": {"frame_px": [16, 16]},
        "filter": "nearest",
        "blend": "cutout",
        "pixel_snap": True,
        "z_layer": 2,
    }
    m.call("world_spawn", shape="sprite", path="/two_d/hero", position=[0, 0, 0], color=[1, 1, 1],
           sprite=dict(common, anim={"count": 4, "fps": 6}))
    # A walker under the canopy (layer 2 < 3: the trees hide its top), and a mirrored one.
    m.call("world_spawn", shape="sprite", path="/two_d/villager", position=[-7.5, 2.4, 0], color=[1, 0.8, 0.8],
           sprite=dict(common, frame=1))
    m.call("world_spawn", shape="sprite", path="/two_d/twin", position=[6, -4, 0], color=[0.8, 1, 0.8],
           sprite=dict(common, frame=2, flip=[True, False]))
    # HUD in the world, top left: a rounded panel with a health ring and a star.
    m.call("world_spawn", shape="shape2d", path="/two_d/hud", position=[-15.5, 8.5, 0], color=[1, 1, 1],
           shape2d={
               "z_layer": 10,
               "items": [
                   {"rect": [0, -3, 7, 3], "radius": 0.4, "fill": "#101820d0", "stroke": {"width": 0.08, "color": "#e8d8a0"}},
                   {"circle": [1.4, -1.5, 0.9], "stroke": {"width": 0.25, "color": "#d04040"}},
                   {"path": [["A", 1.4, -1.5, 0.9, 0.9, 90, -250]], "stroke": {"width": 0.25, "color": "#60e070", "cap": "round"}},
                   {"polygon": star(4.6, -1.5, 1.1, 0.45), "fill": "#f0c040", "stroke": {"width": 0.05, "color": "#805010", "join": "round"}},
               ],
           })


def star(cx, cy, r1, r2):
    import math
    return [[round(cx + (r1 if k % 2 == 0 else r2) * math.cos(math.pi / 2 + k * math.pi / 5), 4),
             round(cy + (r1 if k % 2 == 0 else r2) * math.sin(math.pi / 2 + k * math.pi / 5), 4)] for k in range(10)]


def build_3d(m):
    m.call("world_spawn", shape="group", name="three_d", position=[200, 0, 0])
    m.call("world_spawn", shape="cube", path="/three_d/ground", position=[0, -0.1, 0], scale=[30, 0.2, 30], color="#5d7a4a")
    for k in range(12):
        x, z = (k % 4) * 5 - 7.5, (k // 4) * 5 - 6
        m.call("world_spawn", shape="sprite", path=f"/three_d/tree_{k:02d}", position=[x + (k % 3) * 0.7, 0, z], color=[1, 1, 1],
               sprite={"texture": "sprites/tree.png", "size": [3, 4.5], "pivot": [0.5, 0], "billboard": "cylindrical",
                       "blend": "cutout", "lit": True, "cast_shadows": True, "filter": "nearest"})
    for k in range(20):
        m.call("world_spawn", shape="sprite", path=f"/three_d/spark_{k:02d}", position=[(k % 5) - 2, 1 + (k // 5) * 0.6, 4 + (k % 3) * 0.5],
               color=[1, 0.9, 0.6], opacity=0.9,
               sprite={"texture": "sprites/spark.png", "size": [0.6, 0.6], "billboard": "spherical"})
    m.call("world_spawn", shape="tilemap", path="/three_d/path", position=[-6, 0.01, 7], rotation_deg=[-90, 0, 0], color=[1, 1, 1],
           tilemap={"tileset": "tiles/tileset.png", "tile_px": [16, 16], "size": [12, 3], "lit": True,
                    "layers": [{"name": "ground", "rows": ["2*12", "9,2*10,9", "2*12"]}]})
    m.call("world_spawn", shape="shape2d", path="/three_d/decal", position=[4, 0.02, 7], rotation_deg=[-90, 0, 0], color=[1, 1, 1],
           shape2d={"items": [
               {"circle": [0, 0, 1.6], "stroke": {"width": 0.15, "color": "#f0e0a0"}},
               {"path": [["M", -1, -0.3], ["L", 0.3, -0.3], ["L", 0.3, -0.8], ["L", 1.2, 0], ["L", 0.3, 0.8], ["L", 0.3, 0.3], ["L", -1, 0.3], ["Z"]],
                "fill": "#f0e0a0c0"}]})


def probes(m, assets, worlds, problems, engine):
    """The 2D review's live probes: textures appear and reload, hostile input is refused (the
    engine survives), big tile edits are quick and undo exactly."""
    import time

    def raw(tool, **a):
        r = m.request("tools/call", {"name": tool, "arguments": a})
        return bool(r.get("isError")), r["content"][0].get("text", "")

    def centre(c=(500, 0)):
        p = m.frame(width=64, height=64, view_2d={"center": list(c), "height_m": 2}, max_subjects=3)
        w, _, px = read_png(p["image"]["path"])
        i = (32 * w + 32) * 4
        return tuple(px[i:i + 3]), p.get("warnings", [])

    m.call("world_set", path="/two_d", visible=False)
    m.call("world_set", path="/three_d", visible=False)
    m.call("world_env", tonemap=False, sky={"enabled": False}, background=[0, 0, 0])
    probe = assets / "probe"
    probe.mkdir(parents=True, exist_ok=True)
    tex = probe / "a.png"
    if tex.exists():
        tex.unlink()
    m.call("world_spawn", shape="sprite", name="probe_sprite", position=[500, 0, 0], color=[1, 1, 1],
           sprite={"texture": "probe/a.png", "size": [2, 2]})
    px, warn = centre()
    check(px == (0, 0, 0) and any("probe/a.png" in w for w in warn), f"a missing texture draws nothing and says so {px} {warn}", problems)
    # A capture checks the textures in view when asked: no waiting for the periodic poll.
    tex.write_bytes(png_bytes(2, 2, [255, 0, 0, 255] * 4))
    px, warn = centre()
    check(px == (255, 0, 0) and not warn, f"once written it is drawn {px} {warn}", problems)
    seen = []
    for colour in [(0, 0, 255), (0, 255, 0), (255, 255, 0), (0, 0, 255)]:
        tex.write_bytes(png_bytes(2, 2, [*colour, 255] * 4))
        seen.append(centre()[0] == colour)
    check(all(seen), f"every edit shows in the next capture {seen}", problems)
    # Round 5: a rewrite of the same length within one timestamp tick (here: the timestamp put
    # back, as mtime-preserving tools do) still shows: recent files are compared by content.
    # Stored (uncompressed) PNGs: the length depends only on the size, not the colour.
    tex.write_bytes(png_bytes(2, 2, [255, 255, 255, 255] * 4, level=0))
    centre()
    st = tex.stat()
    tex.write_bytes(png_bytes(2, 2, [255, 0, 255, 255] * 4, level=0))
    os.utime(tex, ns=(st.st_atime_ns, st.st_mtime_ns))
    same_stamp = tex.stat().st_size == st.st_size and tex.stat().st_mtime_ns == st.st_mtime_ns
    px, _ = centre()
    check(same_stamp and px == (255, 0, 255), f"a same-length rewrite with the same timestamp shows {px} (stamp kept: {same_stamp})", problems)
    # Round 5: a file edited while another agent's capture is still loading it: the next capture
    # shows the edit, not what that load read before it.
    import threading
    slow = probe / "slow.png"
    n = 2048
    slow.write_bytes(png_bytes(n, n, [0, 0, 255, 255] * (n * n)))
    m.call("world_spawn", shape="sprite", name="probe_slow", position=[700, 0, 0], color=[1, 1, 1],
           sprite={"texture": "probe/slow.png", "size": [2, 2]})
    other = {}
    with engine.mcp("two_d_check_other") as m2:
        def first():
            p = m2.frame(width=64, height=64, view_2d={"center": [700, 0], "height_m": 2}, max_subjects=3)
            w, _, buf = read_png(p["image"]["path"])
            i = (32 * w + 32) * 4
            other["px"] = tuple(buf[i:i + 3])
        t = threading.Thread(target=first)
        t.start()
        time.sleep(0.05)  # the other capture's load is under way
        slow.write_bytes(png_bytes(2, 2, [0, 255, 0, 255] * 4))
        px, warn = centre((700, 0))
        t.join()
    check(px == (0, 255, 0) and not warn, f"an edit during another capture's load shows next: {px} (that capture saw {other.get('px')})", problems)
    # Round 2: an off-screen sprite whose texture is huge (refused from its header) or missing
    # neither holds a capture up nor shows in its warnings; on screen it is reported.
    head = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR" + (16384).to_bytes(4, "big") * 2 + bytes([8, 6, 0, 0, 0, 0, 0, 0, 0])
    (probe / "huge.png").write_bytes(head)
    m.call("world_spawn", shape="sprite", name="probe_far", position=[10000, 0, 0], sprite={"texture": "probe/huge.png"})
    m.call("world_spawn", shape="sprite", name="probe_far2", position=[-10000, 0, 0], sprite={"texture": "probe/nowhere.png"})
    t = time.time()
    px, warn = centre()
    took = time.time() - t
    check(took < 2 and not any("probe/huge" in w or "nowhere" in w for w in warn), f"off-screen textures don't hold up or warn ({took:.2f} s) {warn}", problems)
    _, warn = centre((10000, 0))
    check(any("16384x16384" in w for w in warn), f"on screen, a huge texture is refused from its header: {warn}", problems)
    bad, text = raw("world_spawn", shape="shape2d", shape2d={"items": [{"rect": [0, 0, 1, 1], "fill": "#12345\u00e9a"}]})
    check(bad and "fill" in text, "a non-ASCII colour is an argument error", problems)
    m.call("world_spawn", shape="tilemap", name="probe_map", tilemap={"tileset": "probe/a.png", "tile_px": [1, 1], "size": [1024, 1024], "layers": [{"name": "g"}]})
    bad, _ = raw("tilemap_get", name="probe_map", layer="g", region=[0, 0, 4294967296, 4294967296])
    check(bad, "tilemap_get refuses an overflowing region", problems)
    before = m.json("tilemap_get", name="probe_map", layer="g", region=[0, 0, 64, 64])["rows"]
    cells = [[x, y, 1 + (x + y) % 3] for y in range(64) for x in range(1024)]
    t = time.time()
    m.call("tilemap_set", name="probe_map", layer="g", cells=cells)
    set_s = time.time() - t
    t = time.time()
    centre((512, -32))
    frame_s = time.time() - t
    check(set_s < 5 and frame_s < 5, f"65,536-cell tilemap_set {set_s:.2f} s, next frame {frame_s:.2f} s", problems)
    m.text("world_undo")
    after = m.json("tilemap_get", name="probe_map", layer="g", region=[0, 0, 64, 64])["rows"]
    check(after == before, "world_undo restores the cells exactly", problems)
    # A world file with a hostile colour is refused on load; the engine keeps running.
    m.call("world_spawn", shape="shape2d", name="probe_shape", shape2d={"items": [{"rect": [0, 0, 1, 1], "fill": "#123456"}]})
    m.json("world_save", name="probe_world", overwrite=True)
    for f in (worlds / "probe_world").glob("*.json"):
        t = f.read_text(encoding="utf-8")
        if "#123456" in t:
            f.write_text(t.replace("#123456", "#12345\u00e9a"), encoding="utf-8")
    bad, text = raw("world_load", name="probe_world", force=True)
    check(bad, f"loading the edited world fails cleanly: {text[:120]}", problems)
    check("specula" in m.text("engine_info").lower(), "the engine is still running", problems)
    # 100 entities sharing one heavy shape: one tessellation; save and load stay quick (shapes
    # loaded from files are tessellated off the engine thread when first drawn).
    shape = {"items": [{"circle": [0, 0, 100], "fill": "#ffffff"}] * 90, "tolerance": 0.001}
    ops = [{"op": "spawn", "shape": "shape2d", "name": f"probe_heavy_{i}", "position": [i * 300.0, 2000, 0], "shape2d": shape} for i in range(100)]
    t = time.time()
    m.call("world_batch", ops=ops)
    batch_s = time.time() - t
    t = time.time()
    m.json("world_save", name="probe_heavy", overwrite=True)
    save_s = time.time() - t
    t = time.time()
    m.json("world_load", name="probe_heavy", force=True)
    load_s = time.time() - t
    check(batch_s < 3 and save_s < 3 and load_s < 3, f"100 shared heavy shapes: batch {batch_s:.2f} s, save {save_s:.2f} s, load {load_s:.2f} s", problems)
    t = time.time()
    p = m.frame(width=64, height=64, view_2d={"center": [0, 2000], "height_m": 300}, max_subjects=3)
    check(time.time() - t < 5 and not p.get("warnings"), f"after loading, they draw (tessellated on the loader) {p.get('warnings')}", problems)
    # Round 3: the shape budget is world state, charged from each description when it enters the
    # world. Distinct heavy shapes fill it and the next is refused with a structured error; a
    # reloaded world keeps its charge (later spawns can't break it); bounds never depend on what
    # has been drawn.
    def heavy(tag):
        return {"items": [{"circle": [0, 0, 100], "fill": "#ffffff"}] * 90, "tolerance": 0.001, "z_layer": tag}

    def err(text):
        try:
            return json.loads(text)
        except ValueError:
            return {}

    refused = None
    for k in range(1, 40):
        bad, text = raw("world_spawn", shape="shape2d", name=f"probe_d{k}", position=[k * 1000.0, 5000, 0], shape2d=heavy(k))
        if bad:
            refused = (k, err(text))
            break
    e = refused[1] if refused else {}
    check(refused is not None and e.get("error") == "limit" and e.get("field") == "shape2d" and e.get("limit") == 2000000 and e.get("value", 0) > 2000000,
          f"distinct heavy shapes fill the world budget, then a structured refusal: {refused and (refused[0], e.get('error'), e.get('value'))}", problems)
    def q():
        page = m.json("world_query", shape="shape2d", fields=["path", "bounds"], limit=200)
        return sorted((i["path"], i["bounds"]) for i in page["items"] if i["path"].startswith("/probe_d"))
    bounds_before = q()
    m.json("world_save", name="probe_budget", overwrite=True)
    m.json("world_load", name="probe_budget", force=True)
    bounds_loaded = q()
    bad, text = raw("world_spawn", shape="shape2d", name="probe_late", shape2d=heavy(999))
    check(bad and err(text).get("error") == "limit", f"after a reload, the loaded shapes keep their budget: {text[:80]!r}", problems)
    m.frame(width=64, height=64, view_2d={"center": [1000, 5000], "height_m": 300}, max_subjects=3)
    check(bounds_before == bounds_loaded == q(), "shape bounds are the same saved, loaded, and drawn", problems)
    bad, text = raw("world_delete", path="/probe_d1")
    m.call("world_spawn", shape="shape2d", name="probe_late", shape2d=heavy(999))
    check(not bad, "deleting a shape frees its budget for another", problems)


def check(cond, what, problems):
    print(("  ok    " if cond else "  FAIL  ") + what)
    if not cond:
        problems.append(what)


def run_adapter(exe, adapter, out, port, assets, windowed=False):
    problems = []
    log = out / f"engine_{adapter}.log"
    worlds = out / "worlds"
    mode = ["--size", "960x540"] if windowed else ["--headless"]
    with Engine(exe, port, log, *mode, "--adapter", adapter, "--assets", str(assets), "--captures", str(out / "captures"),
                "--worlds", str(worlds), cwd=str(out)) as engine:
        with engine.mcp("two_d_check") as m:
            info = m.json("engine_info")
            gpu = info["gpu"]["name"]
            print(f"adapter {adapter}: {gpu} (validation {'on' if info.get('validation') else 'OFF'})")
            build_2d(m)
            build_3d(m)
            m.call("world_set", path="/three_d", visible=False)
            shots = {}

            def shot(name, **args):
                f = m.frame(width=1024, height=576, max_subjects=100, **args)
                src = Path(f["image"]["path"])
                dst = out / f"{adapter}_{name}.png"
                dst.write_bytes(src.read_bytes())
                (out / f"{adapter}_{name}.json").write_text(json.dumps(f, indent=1))
                for w in f.get("warnings", []):
                    print(f"  warning: {w}")
                    if "vulkan Error" in w:
                        problems.append(f"{name}: {w}")
                shots[name] = (read_png(dst), f)
                return shots[name]

            # 2D: 32 px per metre (2 screen px per art px); world (x, y) -> ((x + 16) * 32, (9 - y) * 32).
            img, f = shot("2d_t0", view_2d={"center": [0, 0], "height_m": 18}, time_s=0.0)
            px = lambda x, y: pixel(img, (x + 16) * 32, (9 - y) * 32)  # noqa: E731
            g = px(-5.3, -5.3)
            check(g[1] > g[0] + 30 and g[1] > g[2] + 30, f"grass is green {g}", problems)
            wa = px(8.5, -3.5)
            check(wa[2] > wa[0] + 60, f"pond is blue {wa}", problems)
            d = px(-3.97, 0.5)
            check(d[0] > d[2] + 30, f"the dirt column x=-4 is brown {d}", problems)
            panel = px(-9.5, 6.0)
            check(panel[0] < 60 and panel[2] < 70, f"the HUD panel darkens what's under it {panel}", problems)
            starc = px(-10.9, 7.0)
            check(starc[0] > 200 and starc[1] > 150 and starc[2] < 120, f"the star is gold {starc}", problems)
            names = {s["name"]: s for s in f["subjects"]}
            check(all(names.get(n, {}).get("on_screen") for n in ("map", "hero", "villager", "twin", "hud")), "2D subjects are on screen", problems)
            hero_rect = names.get("hero", {}).get("screen")
            check(hero_rect is not None and abs(hero_rect[2] - 48) <= 2 and abs(hero_rect[3] - 48) <= 2, f"hero's rect is 1.5 m = 48 px: {hero_rect}", problems)
            img2, _ = shot("2d_t45", view_2d={"center": [0, 0], "height_m": 18}, time_s=0.45)
            moved = sum(1 for y in range(9 * 32 - 48, 9 * 32) for x in range(512 - 24, 512 + 24) if pixel(img, x, y) != pixel(img2, x, y))
            check(moved > 20, f"the hero's walk frame changes ({moved} px)", problems)
            pond_changed = pixel(img, (8.5 + 16) * 32, (9 + 3.5) * 32) != pixel(img2, (8.5 + 16) * 32, (9 + 3.5) * 32) or any(
                pixel(img, (x + 16) * 32 + 3, (9 + 3.5) * 32 + 5) != pixel(img2, (x + 16) * 32 + 3, (9 + 3.5) * 32 + 5) for x in range(4, 12))
            check(pond_changed, "the pond animates", problems)
            # Edit: a stone wall across row 12 (y = -3.5), then read it back.
            r = m.json("tilemap_set", path="/two_d/map", layer="ground", cells=[[x, 12, 5] for x in range(0, 12)])
            check(r.get("version", 0) >= 2, f"tilemap_set made version {r.get('version')}", problems)
            got = m.json("tilemap_get", path="/two_d/map", layer="ground", region=[0, 12, 14, 1])
            check(got["rows"] == ["5*12,2,1"], f"tilemap_get reads the wall back: {got['rows']}", problems)
            img3, _ = shot("2d_edited", view_2d={"center": [0, 0], "height_m": 18}, time_s=0.0)
            wall = pixel(img3, (-10 + 16) * 32 + 8, (9 + 3.5) * 32 + 4)
            check(abs(wall[0] - wall[2]) < 30 and wall[0] > 60, f"the wall is grey {wall}", problems)
            listing = m.json("world_query", path_prefix="/two_d/map", fields=["tilemap"])
            check("rows" not in json.dumps(listing) and len(json.dumps(listing)) < 2000, "listings stay bounded (no cells)", problems)

            # 3D.
            m.call("world_set", path="/two_d", visible=False)
            m.call("world_set", path="/three_d", visible=True)
            m.call("world_env", sun_direction=[-0.5, 0.8, 0.4], tonemap=True, sky={"enabled": True},
                   shadow={"enabled": True}, fog={"enabled": True, "density": 0.01})
            img4, f4 = shot("3d_persp", eye=[212, 8, 16], target=[200, 1.5, 0], time_s=0.0)
            subs = {s["name"]: s for s in f4["subjects"]}
            trees = [s for n, s in subs.items() if n.startswith("tree_") and s.get("on_screen")]
            check(len(trees) >= 8, f"{len(trees)} trees on screen", problems)
            check(all(s.get("two_d", {}).get("billboard") == "cylindrical" for s in trees), "tree subjects say cylindrical", problems)
            _, f5 = shot("3d_ortho", eye=[215, 12, 15], target=[200, 0, 0], ortho={"height_m": 22}, time_s=0.0)
            # A 3 m card at 576 / 22 px per metre: about 79 px wide, whichever way it turned.
            widths = [s["screen"][2] for s in f5["subjects"] if s["name"].startswith("tree_") and s.get("screen")]
            check(widths and all(70 <= w_ <= 90 for w_ in widths), f"billboard subject rects are the drawn card: widths {sorted(set(widths))}", problems)
            perf = m.json("perf_frame")
            (out / f"{adapter}_perf.json").write_text(json.dumps(perf, indent=1))
            # Persistence through the engine: the 2D scene, saved, loaded, renders the same. The
            # environment is world-wide: the 3D scene's height fog (density by world Y) would
            # shade the XY-plane map by row, so it goes off for the 2D view.
            m.call("world_env", fog={"enabled": False}, shadow={"enabled": False})
            m.call("world_set", path="/two_d", visible=True)
            m.call("world_set", path="/three_d", visible=False)
            img_saved, _ = shot("2d_saved", view_2d={"center": [0, 0], "height_m": 18}, time_s=0.0)
            before = m.json("world_hash")
            m.json("world_save", name="two_d_check", overwrite=True)
            m.json("world_load", name="two_d_check", force=True)
            after = m.json("world_hash")
            hb, ha = before.get("state_hash"), after.get("state_hash")
            check(hb is not None and hb == ha, f"the state hash survives save + load ({hb} / {ha})", problems)
            img5, _ = shot("2d_reloaded", view_2d={"center": [0, 0], "height_m": 18}, time_s=0.0)
            same = sum(1 for k in range(0, len(img5[2]), 4 * 97) if img_saved[2][k:k + 3] != img5[2][k:k + 3])
            check(same == 0, f"the reloaded 2D world renders the same ({same} sampled pixels differ)", problems)
            probes(m, assets, worlds, problems, engine)
            if windowed:
                # The human's window shows the 2D view for a while (frames in flight, 2D uploads
                # and edits between presents), then the 3D scene.
                import time
                m.call("window_view", view_2d={"center": [0, 0], "height_m": 18})
                for k in range(20):
                    m.json("tilemap_set", path="/two_d/map", layer="details", cells=[[k, 16, 6], [k + 1, 15, 7]])
                    time.sleep(0.05)
                m.call("world_set", path="/three_d", visible=True)
                m.call("window_view", eye=[212, 8, 16], target=[200, 1.5, 0])
                time.sleep(1.0)
                perf = m.json("perf_frame")
                (out / f"{adapter}_perf_window.json").write_text(json.dumps(perf, indent=1))
    errors = engine.validation_errors()
    check(errors == 0, f"shutdown with {errors} validation errors", problems)
    return problems


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--exe", type=Path, default=ROOT / "target" / "debug" / "specula.exe")
    p.add_argument("--adapters", default="discrete,integrated", help="comma-separated: discrete, integrated or indices")
    p.add_argument("--out", type=Path, default=ROOT / "target" / "two-d-check")
    p.add_argument("--port", type=int, default=DEFAULT_PORT)
    p.add_argument("--windowed", action="store_true", help="also present to a window (needs a desktop)")
    args = p.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    assets = args.out / "assets"
    tileset(assets / "tiles" / "tileset.png")
    hero(assets / "sprites" / "hero.png")
    tree(assets / "sprites" / "tree.png")
    spark(assets / "sprites" / "spark.png")
    failed = {}
    for a in args.adapters.split(","):
        failed[a] = run_adapter(args.exe, a, args.out, args.port, assets, args.windowed)
    print()
    for a, probs in failed.items():
        print(f"adapter {a}: {'clean' if not probs else str(len(probs)) + ' problem(s)'}")
    print(f"captures in {args.out}")
    sys.exit(1 if any(failed.values()) else 0)


if __name__ == "__main__":
    main()
