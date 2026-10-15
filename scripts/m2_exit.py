"""M2 exit proof: the three exit criteria of ROADMAP.md's M2, as one repeatable check.

  1. A lit, shadowed scene with imported and procedural assets and an animated character.
     A moonlit Sepulcra graveyard vignette built over MCP: procedural recipe props (tombstones,
     warding stone, brazier cairn with its fire light, rune pillar, fence), imported glTF assets
     (a textured, skinned creature GLB exported by `specula-gltf`'s textured_creature example and
     a static GLB prop), recipe creatures playing clips through the world `anim` component, under
     a moon with shadows, height fog, bloom, colour grading and SSAO. Captures a hero shot and a
     6-frame filmstrip at fixed time_s. Checks:
       - every model entity is `ready` (world_query model_state) and drawn (packet models.missing 0),
         the GLB imports report source gltf, a skin, clips and textures,
       - each animated character's pixels change across the filmstrip (its pose moves), and the
         same time_s renders identical pixels,
       - shadows: ground points the engine's own camera maths puts inside the shadows of the
         warding stone and the brazier are at least 25% darker with shadows on than with them off,
         while open ground beside them changes by under 10% (measured from the PNGs),
       - zero validation errors at shutdown.
  2. Sight reports 0 visible pixels for a fully hidden object: a box behind a wall has
     visible_px 0 and occluded true, a control box in view has visible_px > 0. Needs the ID
     buffer (the Sight lane); SKIPPED with the reason when subjects carry no visible_px.
  3. A 10k-instance world holds its budget on the Intel integrated GPU: a windowed engine at
     1280x720 with the full Sepulcra night effect stack and `perf.example.toml`'s budgets,
     10,000 entities spawned in 1,000-op world_batch chunks (8,000 stone grave markers sharing
     the cube mesh + 2,000 tombstone models sharing one build, so they instance), seen through
     the window camera; `perf_capture` records 120 window frames and checks the budgets on the
     p95 over them. Required (no errors on the integrated adapter): the frame's GPU ms, frame
     pacing, draw calls, triangles, GPU memory, pipelines built after warm-up. Per-pass GPU
     budgets (Sepulcra's split of its own frame) and CPU-side budgets (cpu_frame, zones,
     allocations, heap) are printed and saved as notes. The discrete adapter is measured too,
     against its tighter budgets, for reference. Other engines running on the machine share the
     GPU and skew timings: the script warns when it sees any.

Output (PNGs, packets, logs, summary JSON) goes to target/m2-exit/ (--out). Prints PASS / FAIL /
SKIPPED per criterion and exits non-zero on any FAIL. LOOK AT THE PNGs before claiming anything.

Usage:
    cargo build --release -p specula
    python scripts/m2_exit.py [--adapter integrated|discrete|both] [--port 7484] [--out DIR] [--exe PATH]

Needs a GPU (Vulkan) and, for criterion 3, a desktop session (it opens a window for a few seconds).
"""
import argparse
import json
import math
import shutil
import struct
import subprocess
import sys
import time
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine, Mcp, url_for  # noqa: E402

SEPULCRA = ROOT / "games" / "sepulcra" / "assets"
PERF_TOML = ROOT / "perf.example.toml"
EXE_NAME = "specula.exe" if sys.platform == "win32" else "specula"

# --- Lighting -----------------------------------------------------------------------------------
# A low moon behind and to the left of the camera's view, so shadows fall forward and to the
# right where the camera sees them; values otherwise from Sepulcra's night() (STYLE.md).
MOON_ELEV = math.radians(22)
MOON_XZ = (-0.92, -0.39)  # towards the moon, on the ground plane (unit length)
MOON_DIR = [math.cos(MOON_ELEV) * MOON_XZ[0], math.sin(MOON_ELEV), math.cos(MOON_ELEV) * MOON_XZ[1]]


def night_env(shadows=True):
    return {
        "sun_direction": MOON_DIR,
        "sun_color": "#c9d6ff",
        "sun_intensity": 1.5,
        "ambient_color": "#2a3448",
        "ambient_intensity": 1.6,
        "exposure": 1.15,
        "tonemap": True,
        "background": "#0b1020",
        "shadow": {"enabled": shadows, "distance": 40, "resolution": 4096, "softness": 1.5},
        "bloom": {"enabled": True, "threshold": 1.0, "intensity": 0.7},
        "fxaa": True,
        "sky": {"enabled": True, "ground": "#06080e"},
        "fog": {"enabled": True, "color": "#1c2334", "density": 0.09, "falloff": 0.3, "height": -1.0,
                "sun_scatter": 0.35},
        "grading": {"enabled": True, "vibrance": -1.0, "lift": [-0.01, -0.004, 0.012], "gain": [1.03, 1.0, 0.97],
                    "vignette": 0.3, "vignette_radius": 0.55},
        "ssao": {"enabled": True, "radius": 0.6, "intensity": 1.0, "samples": 8, "resolution": 0.5},
    }


# --- Criterion 1: the scene ---------------------------------------------------------------------
GROUND = "#3d434d"
PATH = "#2c3038"
# (name, model source, params, position, yaw deg, extra spawn fields)
PROPS = [
    ("ward", "recipes/warding_stone.json", {"tier": 2}, [-2.4, 0, -1.2], 12,
     {"light": {"intensity": 1.6, "range": 5.0, "color": "#3fe0d0", "offset": [0, 1.3, 0.9]}}),
    ("brazier", "recipes/brazier_cairn.json", {"tier": 2}, [2.3, 0, -1.4], -20,
     {"light": {"intensity": 3.0, "range": 8.0, "color": "#ff9a4a", "offset": [0, 2.1, 0]}}),
    ("ward_gltf", "imported/warding_stone_t3.glb", {}, [5.6, 0, -8.5], -25, {}),
    ("pillar", "recipes/rune_pillar.json", {"tier": 1}, [-7.0, 0, -9.5], 30, {}),
]
TOMBS = [  # (position, yaw, seed, tilt)
    ([-5.2, 0, -3.6], 8, 1, 0.6), ([-3.6, 0, -4.4], -6, 2, -0.4), ([-1.6, 0, -4.9], 4, 3, 1.0),
    ([0.6, 0, -5.4], -10, 4, 0.2), ([3.4, 0, -4.6], 12, 5, -0.8), ([-6.0, 0, -6.8], -4, 6, -1.0),
    ([-4.0, 0, -7.4], 10, 7, 0.5), ([-1.0, 0, -8.0], -8, 8, -0.3), ([1.8, 0, -7.8], 6, 9, 0.9),
    ([0.2, 0, -2.6], 18, 10, -0.6), ([-2.9, 0, 3.0], 28, 11, 1.0), ([-3.9, 0, 1.4], -14, 12, -0.7),
]
FENCE_Z = -12.0
FENCE_X = [-11.0, -9.0, -7.0, -5.0, -3.0, 3.0, 5.0, 7.0, 9.0, 11.0]  # a gap for the path
# (name, source, clip, speed, position, yaw): the imported GLB creature is the hero.
CHARACTERS = [
    ("shambler_gltf", "imported/shambler_tex.glb", "walk", 1.0, [-0.2, 0, 1.6], 18),
    ("runner", "recipes/skeleton_runner.json", "sprint", 1.0, [3.4, 0, 0.6], -35),
    ("wraith", "recipes/wraith.json", "float", 1.0, [-4.4, 0.3, -2.4], 25),
    ("shambler", "recipes/shambler.json", "attack", 1.0, [0.9, 0, -3.6], -10),
    ("colossus", "recipes/bone_colossus.json", "walk", 0.7, [-18.0, 0, -16.0], 50),
]
HERO = dict(width=1600, height=900, eye=[1.8, 1.7, 7.0], target=[-0.4, 1.25, -3.0], fov_y_deg=46)
FILM_TIMES = [0.0, 0.2, 0.4, 0.6, 0.8, 1.0]
HERO_TIME = 0.45
# Shadow check: props (name, height m) whose shadows are probed, and ground no shadow reaches.
SHADOW_CASTERS = [("ward", 2.4), ("brazier", 2.27)]
OPEN_GROUND = [[-2.0, 0.0, 1.0], [2.0, 0.0, 2.0]]


def scene_ops():
    ops = [{"op": "spawn", "shape": "group", "path": "/vignette"},
           {"op": "spawn", "shape": "quad", "path": "/vignette/ground", "position": [0, 0, -8], "scale": [120, 1, 120],
            "color": GROUND, "roughness": 0.95}]
    for name, src, params, pos, yaw, extra in PROPS:
        ops.append({"op": "spawn", "shape": "model", "path": f"/vignette/{name}", "recipe": src, "params": params,
                    "position": pos, "rotation_deg": [0, yaw, 0], **extra})
    for i, (pos, yaw, seed, tilt) in enumerate(TOMBS):
        ops.append({"op": "spawn", "shape": "model", "path": f"/vignette/tomb_{i}", "recipe": "recipes/tombstone.json",
                    "params": {"tilt": tilt}, "seed": seed, "position": pos, "rotation_deg": [0, yaw, 0]})
    for i, x in enumerate(FENCE_X):
        ops.append({"op": "spawn", "shape": "model", "path": f"/vignette/fence_{i}", "recipe": "recipes/graveyard_fence.json",
                    "position": [x, 0, FENCE_Z]})
    for name, src, clip, speed, pos, yaw in CHARACTERS:
        ops.append({"op": "spawn", "shape": "model", "path": f"/vignette/{name}", "recipe": src, "position": pos,
                    "rotation_deg": [0, yaw, 0], "anim": {"clip": clip, "speed": speed}})
    return ops


# --- Images -------------------------------------------------------------------------------------
def decode_png(path):
    """8-bit RGB/RGBA non-interlaced PNG -> (w, h, bytes per pixel, pixel bytes)."""
    data = Path(path).read_bytes()
    pos, idat = 8, b""
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
    for y in range(h):
        f, line = raw[i], bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + ((a + b) >> 1)) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else (b if pb <= pc else c))) & 255
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return w, h, bpp, bytes(out)


def box_diff(img_a, img_b, box):
    """Fraction of pixels inside box [x0, y0, x1, y1] whose colour differs by more than 8/255."""
    w, h, bpp, a = img_a
    b = img_b[3]
    x0, y0 = max(0, int(box[0])), max(0, int(box[1]))
    x1, y1 = min(w, int(box[2]) + 1), min(h, int(box[3]) + 1)
    n = changed = 0
    for y in range(y0, y1):
        row = y * w * bpp
        for x in range(x0, x1):
            k = row + x * bpp
            n += 1
            if max(abs(a[k + c] - b[k + c]) for c in range(3)) > 8:
                changed += 1
    return changed / max(n, 1)


def luma(img, px, py, r=5):
    """Mean Rec.709 luma (0..255, of the sRGB bytes) in a (2r+1)^2 patch around (px, py)."""
    w, h, bpp, d = img
    total = n = 0
    for y in range(max(0, int(py) - r), min(h, int(py) + r + 1)):
        for x in range(max(0, int(px) - r), min(w, int(px) + r + 1)):
            k = (y * w + x) * bpp
            total += 0.2126 * d[k] + 0.7152 * d[k + 1] + 0.0722 * d[k + 2]
            n += 1
    return total / max(n, 1)


def project(cam, p):
    """World point -> pixel (x, y), the engine's camera: right-handed, +Y up, Vulkan clip with y
    down, pixel origin top-left. None when behind the camera."""
    eye, target = cam["eye"], cam["target"]
    sub = lambda a, b: [a[i] - b[i] for i in range(3)]
    dot = lambda a, b: sum(a[i] * b[i] for i in range(3))
    cross = lambda a, b: [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    norm = lambda a: [v / math.sqrt(dot(a, a)) for v in a]
    f = norm(sub(target, eye))
    r = norm(cross(f, [0, 1, 0]))
    u = cross(r, f)
    d = sub(p, eye)
    z = dot(d, f)
    if z <= 0:
        return None
    t = math.tan(math.radians(cam["fov_y_deg"]) / 2)
    aspect = cam["width"] / cam["height"]
    nx, ny = dot(d, r) / (z * t * aspect), dot(d, u) / (z * t)
    return (nx + 1) / 2 * cam["width"], (1 - ny) / 2 * cam["height"]


def shadow_probe(cam, prop_pos, height, boxes):
    """A ground point inside a prop's shadow, as (world point, pixel): along the shadow (away
    from the moon) from 30% to 90% of its length, the first whose pixel lies in no subject's
    screen box (so nothing stands between the camera and that patch of ground)."""
    sx, sz = -MOON_XZ[0], -MOON_XZ[1]
    length = height / math.tan(MOON_ELEV)
    for k in range(3, 10):
        d = length * k / 10
        p = [prop_pos[0] + sx * d, 0.0, prop_pos[2] + sz * d]
        px = project(cam, p)
        if px and 8 <= px[0] < cam["width"] - 8 and 8 <= px[1] < cam["height"] - 8 and not in_boxes(px, boxes):
            return p, px
    return None, None


def in_boxes(px, boxes, margin=8):
    """True if pixel px lies within `margin` of any [x, y, w, h] box."""
    return any(b[0] - margin <= px[0] <= b[0] + b[2] + margin and b[1] - margin <= px[1] <= b[1] + b[3] + margin
               for b in boxes)


# --- Helpers ------------------------------------------------------------------------------------
class Checks:
    def __init__(self):
        self.failures = []

    def __call__(self, ok, what):
        print(("  ok    " if ok else "  FAIL  ") + what)
        if not ok:
            self.failures.append(what)
        return ok


def make_assets(out):
    """Sepulcra's style, recipes and anims plus imported GLBs, in a scratch assets root
    (model paths ending .glb resolve against it like recipes do)."""
    assets = out / "assets"
    shutil.rmtree(assets, ignore_errors=True)
    assets.mkdir(parents=True)
    shutil.copy(SEPULCRA / "style.json", assets / "style.json")
    shutil.copytree(SEPULCRA / "recipes", assets / "recipes")
    shutil.copytree(SEPULCRA / "anims", assets / "anims")
    (assets / "imported").mkdir()
    for example, args in [("textured_creature", ["shambler", "imported/shambler_tex.glb"]),
                          ("export_recipe", ["warding_stone", "imported/warding_stone_t3.glb", "tier=3"])]:
        args = [str(SEPULCRA), args[0], str(assets / args[1]), *args[2:]]
        r = subprocess.run(["cargo", "run", "-q", "-p", "specula-gltf", "--example", example, "--", *args],
                           cwd=str(ROOT), capture_output=True, text=True)
        if r.returncode != 0:
            sys.exit(f"{example}: {r.stderr}")
        print("  " + r.stdout.strip().replace(str(assets) + "\\", "").replace(str(assets) + "/", ""))
    return assets


def full_subjects(packet):
    """Every subject of a capture (the reply holds the biggest max_subjects; the packet all)."""
    path = packet["image"].get("packet_path")
    if path and Path(path).exists():
        return json.loads(Path(path).read_text())["subjects"]
    return packet["subjects"]


# --- Criterion 1 --------------------------------------------------------------------------------
def criterion_scene(mcp, out, check, tag):
    print(f"[1] lit, shadowed scene with imported + procedural assets and animated characters ({tag})")
    result = {"captures": {}}
    mcp.call("world_env", **night_env())
    for src in ("imported/shambler_tex.glb", "imported/warding_stone_t3.glb"):
        r = mcp.json("model_build", recipe=src)
        clips = [c["name"] for c in r.get("clips", [])]
        print(f"    {src}: source {r.get('source')}, bones {r.get('bones')}, clips {clips}, "
              f"textures {len(r.get('textures', []))}, triangles {[l['triangles'] for l in r['lods']]}")
        check(r.get("source") == "gltf", f"{src} imported from glTF (source {r.get('source')})")
        if "shambler" in src:
            check(r.get("bones", 0) > 0 and "walk" in clips and len(r.get("textures", [])) == 3,
                  "the creature GLB has a skin, a walk clip and 3 textures")
    ops = scene_ops()
    try:
        mcp.call("world_delete", path="/vignette")  # an attached engine that ran this before
    except RuntimeError as e:
        if "not_found" not in str(e):
            raise
    res = mcp.json("world_batch", ops=ops)
    check(res["applied"] == len(ops), f"world_batch built the vignette ({res['applied']} of {len(ops)} ops)")

    # Hero shot and filmstrip (the capture waits for builds in view).
    hero = mcp.frame(time_s=HERO_TIME, max_subjects=100, **HERO)
    result["captures"]["hero"] = {"png": hero["image"]["path"], "packet": hero["image"]["packet_path"]}
    print(f"    hero: {hero['image']['path']}  models {hero['models']}")
    small = mcp.frame(time_s=HERO_TIME, max_subjects=1, **{**HERO, "width": 1280, "height": 720})
    result["captures"]["hero_1280"] = {"png": small["image"]["path"], "packet": small["image"]["packet_path"]}
    print(f"    hero at 1280x720 (for docs/fidelity/m2_exit_hero.png): {small['image']['path']}")
    states = mcp.json("world_query", path_prefix="/vignette", shape="model", fields=["path", "model_state"], limit=500)["items"]
    not_ready = [(s["path"], s["model_state"]) for s in states if s["model_state"] != "ready"]
    check(len(states) == len(ops) - 2 and not not_ready,
          f"all {len(states)} model entities ready (not ready: {not_ready})")
    check(hero["models"]["missing"] == 0 and hero["models"]["pipeline_failed"] == 0,
          f"every model drawn (missing {hero['models']['missing']}, pipeline_failed {hero['models']['pipeline_failed']})")
    check(not hero["warnings"] and not hero.get("problems"), f"no capture warnings/problems {hero['warnings']} {hero.get('problems')}")

    film = []
    for t in FILM_TIMES:
        p = mcp.frame(time_s=t, max_subjects=100, **HERO)
        film.append(p)
    result["captures"]["filmstrip"] = [{"time_s": t, "png": p["image"]["path"], "packet": p["image"]["packet_path"]}
                                       for t, p in zip(FILM_TIMES, film)]
    print(f"    filmstrip: {', '.join(Path(p['image']['path']).name for p in film)}")
    imgs = [decode_png(p["image"]["path"]) for p in film]
    again = decode_png(mcp.frame(time_s=FILM_TIMES[2], max_subjects=1, **HERO)["image"]["path"])
    check(box_diff(imgs[2], again, [0, 0, HERO["width"], HERO["height"]]) == 0.0, "the same time_s renders identical pixels")
    subjects = {s["name"]: s for s in full_subjects(film[0])}
    motion = {}
    for name, *_ in CHARACTERS:
        s = subjects.get(name)
        if not check(s is not None and s["on_screen"], f"{name} is on screen"):
            continue
        x, y, w, h = s["screen"]
        box = [x - 0.2 * w, y - 0.2 * h, x + 1.2 * w, y + 1.2 * h]
        moved = [box_diff(imgs[k], imgs[k + 1], box) for k in range(len(imgs) - 1)]
        motion[name] = {"box": [round(v) for v in box], "changed": [round(m, 4) for m in moved]}
        print(f"    {name:<14} box {[round(v) for v in box]} changed between frames {[round(m, 3) for m in moved]}")
        check(sum(m > 0.01 for m in moved) >= len(moved) - 1, f"{name}'s pose moves across the filmstrip")
    result["motion"] = motion

    # Shadows: the same frame with the moon's shadow map off; probes from the camera maths,
    # checked against the packet's own screen boxes first.
    ward = subjects.get("ward")
    centre = project(HERO, PROPS[0][3][:1] + [1.2] + PROPS[0][3][2:])
    if ward and ward["screen"]:
        x, y, w, h = ward["screen"]
        check(x <= centre[0] <= x + w and y <= centre[1] <= y + h,
              f"camera maths agrees with the packet (ward centre at {[round(c) for c in centre]}, box {ward['screen']})")
    mcp.call("world_env", shadow={"enabled": False})
    flat = mcp.frame(time_s=HERO_TIME, max_subjects=1, **HERO)
    mcp.call("world_env", shadow={"enabled": True})
    result["captures"]["no_shadows"] = {"png": flat["image"]["path"], "packet": flat["image"]["packet_path"]}
    lit, unlit = decode_png(hero["image"]["path"]), decode_png(flat["image"]["path"])
    boxes = [s["screen"] for s in full_subjects(hero) if s["on_screen"] and s["name"] != "ground"]
    probes = []
    for name, height in SHADOW_CASTERS:
        pos = next(p[3] for p in PROPS if p[0] == name)
        world, px = shadow_probe(HERO, pos, height, boxes)
        if not check(px is not None, f"{name}: a patch of its shadow is in view, clear of every subject box"):
            continue
        on, off = luma(lit, *px), luma(unlit, *px)
        probes.append({"kind": "shadow", "of": name, "world": [round(v, 2) for v in world], "px": [round(v) for v in px],
                       "luma_shadows_on": round(on, 1), "luma_shadows_off": round(off, 1), "ratio": round(on / max(off, 1e-3), 3)})
        print(f"    {name}'s shadow at {probes[-1]['world']} (px {probes[-1]['px']}): luma {on:.1f} with shadows, "
              f"{off:.1f} without (x{on / max(off, 1e-3):.2f})")
        check(on < 0.75 * off, f"{name} casts a shadow on the ground: x{on / max(off, 1e-3):.2f} (needs < 0.75)")
    for world in OPEN_GROUND:
        px = project(HERO, world)
        if not check(px and 0 <= px[0] < HERO["width"] and 0 <= px[1] < HERO["height"] and not in_boxes(px, boxes),
                     f"open ground {world} is in view, clear of every subject box"):
            continue
        on, off = luma(lit, *px), luma(unlit, *px)
        probes.append({"kind": "open", "world": world, "px": [round(v) for v in px], "luma_shadows_on": round(on, 1),
                       "luma_shadows_off": round(off, 1), "ratio": round(on / max(off, 1e-3), 3)})
        print(f"    open ground at {world} (px {probes[-1]['px']}): luma {on:.1f} with shadows, {off:.1f} without")
        check(abs(on / max(off, 1e-3) - 1) < 0.1, f"open ground keeps its light: x{on / max(off, 1e-3):.2f} (needs 0.9..1.1)")
    result["shadow_probes"] = probes
    return result


# --- Criterion 2 --------------------------------------------------------------------------------
def criterion_hidden(mcp, check):
    """A box fully behind a wall, a control box in view, well away from the vignette."""
    print("[2] sight reports 0 visible pixels for a fully hidden object")
    base = [200.0, 0.0, 0.0]
    at = lambda dx, dy, dz: [base[0] + dx, base[1] + dy, base[2] + dz]
    ops = [{"op": "spawn", "shape": "group", "path": "/hidden_test"},
           {"op": "spawn", "shape": "quad", "path": "/hidden_test/floor", "position": at(0, 0, 0), "scale": [30, 1, 30],
            "color": GROUND},
           {"op": "spawn", "shape": "cube", "path": "/hidden_test/wall", "position": at(0, 1.5, 0), "scale": [6, 3, 0.3],
            "color": "#8c96a3"},
           {"op": "spawn", "shape": "cube", "path": "/hidden_test/hidden_box", "position": at(0, 0.6, -2.0),
            "scale": [1, 1, 1], "color": "#ff3030"},
           {"op": "spawn", "shape": "cube", "path": "/hidden_test/control_box", "position": at(4.5, 0.5, 1.0),
            "scale": [1, 1, 1], "color": "#30ff60"}]
    mcp.json("world_batch", ops=ops)
    cam = dict(width=960, height=540, eye=at(0.5, 1.6, 9.0), target=at(0.5, 1.2, 0), fov_y_deg=50)
    p = mcp.frame(time_s=0.0, max_subjects=500, **cam)
    subs = {s["name"]: s for s in full_subjects(p)}
    hidden, control = subs.get("hidden_box"), subs.get("control_box")
    info = {"png": p["image"]["path"], "packet": p["image"]["packet_path"],
            "hidden": hidden, "control": control}
    mcp.call("world_delete", path="/hidden_test")
    if hidden is None or control is None:
        check(False, f"both boxes are subjects (hidden {hidden is not None}, control {control is not None})")
        return "FAIL", info
    if "visible_px" not in hidden:
        print(f"    {p['image']['path']}: subjects carry no visible_px (the ID buffer is on the unmerged Sight lane)")
        return "SKIPPED", info
    print(f"    {p['image']['path']}: hidden_box visible_px {hidden.get('visible_px')} occluded {hidden.get('occluded')} "
          f"(projected box {hidden.get('screen')}); control_box visible_px {control.get('visible_px')}")
    ok = check(hidden.get("on_screen") and hidden.get("visible_px") == 0 and hidden.get("occluded") is True,
               "the box behind the wall: in view, visible_px 0, occluded true")
    ok &= check((control.get("visible_px") or 0) > 0, f"the control box: visible_px {control.get('visible_px')} > 0")
    return ("PASS" if ok else "FAIL"), info


# --- Criterion 3 --------------------------------------------------------------------------------
GRID = 100          # 100 x 100 plots
PLOT = 1.6          # metres between plots
TOMB_EVERY = 5      # every 5th plot holds a tombstone model; the rest a stone marker (cube)


def field_batches():
    """10,000 entities in 10 world_batch calls of 1,000 ops."""
    for b in range(GRID * GRID // 1000):
        ops = []
        for k in range(1000):
            i = b * 1000 + k
            gx, gz = i % GRID, i // GRID
            # Deterministic jitter so the field doesn't read as a grid.
            h = (i * 2654435761) & 0xFFFFFFFF
            jx, jz = ((h & 0xFF) / 255 - 0.5) * 0.5, (((h >> 8) & 0xFF) / 255 - 0.5) * 0.5
            pos = [(gx - GRID / 2) * PLOT + jx, 0, -(gz * PLOT) + jz]
            yaw = ((h >> 16) & 0xFF) / 255 * 24 - 12
            if i % TOMB_EVERY == 0:
                ops.append({"op": "spawn", "shape": "model", "path": f"/field/t{i:05d}", "recipe": "recipes/tombstone.json",
                            "position": pos, "rotation_deg": [0, yaw, 0]})
            else:
                shade = 0x80 + ((h >> 24) & 0x1F)
                ops.append({"op": "spawn", "shape": "cube", "path": f"/field/m{i:05d}",
                            "position": [pos[0], 0.3, pos[2]], "rotation_deg": [0, yaw, 0], "scale": [0.45, 0.6, 0.14],
                            "color": f"#{shade:02x}{shade - 4:02x}{shade - 12:02x}", "roughness": 0.85})
        yield ops


FRAME_METRICS = ("gpu_ms", "frame_pacing_p95_ms", "draw_calls", "triangles", "gpu_memory_mb", "pipelines_after_warm")


def is_frame_metric(metric):
    """The budgets criterion 3 requires: the frame's GPU time, frame pacing, draw calls,
    triangles, GPU memory and pipelines built after warm-up. Per-pass GPU budgets (Sepulcra's
    split of its own frame; a 10k-entity field is a different mix, with far more shadow casters)
    and CPU-side budgets (cpu_frame, zones, allocations, heap) are reported beside them as notes."""
    return metric in FRAME_METRICS


def other_engines(own_pid):
    """How many other specula/sepulcra processes are running (other sessions' engines share the
    GPU and skew timings); 0 where it can't be told."""
    if sys.platform != "win32":
        return 0
    n = 0
    for image in ("specula.exe", "sepulcra.exe"):
        r = subprocess.run(["tasklist", "/FI", f"IMAGENAME eq {image}", "/FO", "CSV", "/NH"],
                           capture_output=True, text=True)
        n += sum(1 for line in r.stdout.splitlines() if line.startswith('"') and f'"{own_pid}"' not in line)
    return n


def criterion_instances(exe, adapter, port, out, check):
    print(f"[3] 10k-instance world within budget, window path ({adapter})")
    log = out / f"engine_window_{adapter}.log"
    args = ["--adapter", adapter, "--size", "1280x720", "--perf", str(PERF_TOML), "--assets", str(out / "assets"),
            "--captures", str(out / "captures"), "--worlds", str(out / "worlds")]
    result = {}
    with Engine(exe, port, log, *args, cwd=str(ROOT)) as engine:
        mcp = engine.mcp("m2-exit-instances")
        mcp.call("agent_hello", name="m2_exit", role="reviewer")
        mcp.call("world_env", **night_env())
        mcp.call("world_spawn", shape="group", path="/field")
        mcp.call("world_spawn", shape="quad", path="/field/ground", position=[0, 0, -GRID * PLOT / 2],
                 scale=[GRID * PLOT + 40, 1, GRID * PLOT + 40], color=GROUND, roughness=0.95)
        spawned = 0
        for ops in field_batches():
            spawned += mcp.json("world_batch", ops=ops)["applied"]
        check(spawned == GRID * GRID, f"spawned {spawned} entities in 1,000-op batches")
        # The window camera: from the field's near edge, looking over it.
        mcp.call("window_view", eye=[0, 9, 14], target=[0, 0, -45], fov_y_deg=50)
        # Let the tombstone build land and LODs settle, then record 120 window frames.
        deadline = time.time() + 30
        while time.time() < deadline:
            q = mcp.json("world_query", path_prefix="/field/t00000", fields=["model_state"])["items"]
            if q and q[0]["model_state"] == "ready":
                break
            time.sleep(0.2)
        time.sleep(3.0)
        cap = mcp.json("perf_capture", frames=120, trace=False, label=f"m2exit_{adapter}")
        frame = mcp.json("perf_frame")
        win = frame.get("window") or {}
        draws = win.get("draws", {})
        gpu = cap.get("gpu_ms") or {}
        shot = mcp.frame(width=1280, height=720, eye=[0, 9, 14], target=[0, 0, -45], fov_y_deg=50, time_s=1.0, max_subjects=1)
        result = {
            "adapter_kind": cap.get("adapter"),
            "gpu": frame.get("budgets", {}).get("adapter"),
            "frames": cap.get("frames"),
            "frame_ms": cap.get("frame_ms"),
            "gpu_ms": gpu,
            "gpu_passes_ms": cap.get("gpu_passes_ms"),
            "draw_calls": cap.get("draw_calls"),
            "triangles": cap.get("triangles"),
            "window_last_frame": {k: draws.get(k) for k in ("submitted", "culled", "draw_calls", "instances", "triangles")},
            "budget_errors": cap.get("budget_errors"),
            "budgets_checked": frame.get("budgets", {}).get("checked"),
            "budgets_checked_on": cap.get("budgets_checked_on"),
            "entities": frame.get("entities"),
            "models": frame.get("models", {}).get("window"),
            "capture_png": shot["image"]["path"],
        }
        print(f"    window frames {cap.get('frames')}: frame ms {cap.get('frame_ms')}")
        print(f"    GPU ms {gpu}")
        print(f"    passes p95 { {k: v.get('p95') for k, v in (cap.get('gpu_passes_ms') or {}).items()} }")
        result["window_size"] = win.get("size")
        print(f"    last window frame ({win.get('size')} px): {result['window_last_frame']}; entities {frame.get('entities')}")
        print(f"    view through the capture camera: {shot['image']['path']}")
        if cap.get("frames", 0) < 60 or not win:
            check(False, f"the window produced frames (captured {cap.get('frames')}, stopped early: {cap.get('stopped_early')})")
        check(draws.get("instances", 0) >= 5000 and draws.get("draw_calls", 10**9) <= 10,
              f"instanced: {draws.get('instances')} instances in {draws.get('draw_calls')} draw calls")
        errors = cap.get("budget_errors") or []
        required = [e for e in errors if is_frame_metric(e.get("metric", ""))]
        cpu_side = [e for e in errors if not is_frame_metric(e.get("metric", ""))]
        result["other_budget_misses"] = cpu_side
        for e in required:
            print(f"    budget miss: {e}")
        for e in cpu_side:
            print(f"    note (per-pass or CPU-side budget: reported, doesn't decide the criterion): {e}")
        result["other_engines_running"] = other_engines(engine.proc.pid)
        if result["other_engines_running"]:
            print(f"    warning: {result['other_engines_running']} other engine process(es) were running and may share the GPU")
        result["passed"] = check(not required, f"no frame/GPU budget errors over {cap.get('frames')} window frames "
                                               f"({result['adapter_kind']}; GPU p95 {gpu.get('p95')} ms, "
                                               f"frame p95 {(cap.get('frame_ms') or {}).get('p95')} ms)")
    errs = engine.validation_errors()
    check(errs == 0, f"validation errors at shutdown: {errs}")
    result["validation_errors"] = errs
    return result


# --- Main ---------------------------------------------------------------------------------------
def run_scene(exe, adapter, port, out, attach, run):
    """Criteria 1 and/or 2 (as `run` asks) on one headless engine, or an attached one. Returns
    the summary and criterion 1's checks; the summary's hidden_status is criterion 2's verdict."""
    check, check2 = Checks(), Checks()
    summary = {"adapter": adapter}

    def body(mcp):
        if 1 in run:
            summary["scene"] = criterion_scene(mcp, out, check, adapter)
        if 2 in run:
            summary["hidden_status"], summary["hidden"] = criterion_hidden(mcp, check2)

    if attach:
        body(Mcp(url_for(attach), "m2-exit"))
        return summary, check
    args = ["--headless", "--adapter", adapter, "--assets", str(out / "assets"), "--captures", str(out / "captures"),
            "--worlds", str(out / "worlds")]
    with Engine(exe, port, out / f"engine_scene_{adapter}.log", *args, cwd=str(ROOT)) as engine:
        mcp = engine.mcp("m2-exit")
        mcp.call("agent_hello", name="m2_exit", role="reviewer")
        summary["gpu"] = mcp.json("engine_info").get("gpu")
        body(mcp)
    errs = engine.validation_errors()
    summary["validation_errors"] = errs
    if not (check if 1 in run else check2)(errs == 0, f"validation errors at shutdown: {errs}") and 1 not in run:
        summary["hidden_status"] = "FAIL"
    return summary, check


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--adapter", default="both", choices=["integrated", "discrete", "both"])
    ap.add_argument("--port", type=int, default=7484)
    ap.add_argument("--out", default=str(ROOT / "target" / "m2-exit"))
    ap.add_argument("--exe", default=str(ROOT / "target" / "release" / EXE_NAME))
    ap.add_argument("--attach", type=int, default=None,
                    help="build criteria 1-2 in an already running engine on this port (for iterating on the scene)")
    ap.add_argument("--criteria", default="1,2,3", help="which criteria to run, e.g. 3 or 1,2 (default all)")
    a = ap.parse_args()
    run = {int(c) for c in a.criteria.split(",")}
    if a.port == 7471 or a.attach == 7471:
        sys.exit("port 7471 belongs to the live engine; pick another")
    if not a.attach and not Path(a.exe).exists():
        sys.exit(f"{a.exe} not found: cargo build --release -p specula")
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    adapters = ["integrated", "discrete"] if a.adapter == "both" else [a.adapter]
    print("assets:")
    if not a.attach:
        make_assets(out)

    status = {1: [], 2: [], 3: []}
    summary = {}
    for adapter in adapters:
        print(f"== {adapter}")
        s = summary[adapter] = {}
        if run & {1, 2}:
            s, check = run_scene(Path(a.exe), adapter, a.port, out, a.attach, run)
            summary[adapter] = s
            if 1 in run:
                status[1].append(("FAIL" if check.failures else "PASS", adapter, check.failures))
            if 2 in run:
                status[2].append((s["hidden_status"], adapter, []))
        if a.attach or 3 not in run:
            continue
        check3 = Checks()
        s["instances"] = criterion_instances(Path(a.exe), adapter, a.port + 1, out, check3)
        status[3].append(("FAIL" if check3.failures else "PASS", adapter, check3.failures))
    (out / "m2_exit.json").write_text(json.dumps(summary, indent=1))
    print(f"summary: {out / 'm2_exit.json'}")

    names = {1: "lit, shadowed scene: imported + procedural assets, animated character",
             2: "0 visible pixels for a fully hidden object",
             3: "10k-instance world holds its budget on the Intel iGPU"}
    any_fail = False
    for c in (1, 2, 3):
        results = status[c]
        if not results:
            print(f"SKIPPED  criterion {c}: {names[c]} (not run: --criteria / --attach)")
            continue
        verdicts = {v for v, _, _ in results}
        if c == 3 and "integrated" not in [ad for _, ad, _ in results]:
            # The criterion is about the iGPU: without it, the discrete result is only a reference.
            verdicts = {"SKIPPED" if v == "PASS" else v for v in verdicts}
        verdict = "FAIL" if "FAIL" in verdicts else ("SKIPPED" if "SKIPPED" in verdicts else "PASS")
        detail = ", ".join(f"{ad} {v}" for v, ad, _ in results)
        why = ""
        if c == 2 and verdict == "SKIPPED":
            why = " - subjects carry no visible_px (needs the ID buffer from the Sight lane)"
        print(f"{verdict:<8} criterion {c}: {names[c]} ({detail}){why}")
        any_fail |= verdict == "FAIL"
    sys.exit(1 if any_fail else 0)


if __name__ == "__main__":
    main()
