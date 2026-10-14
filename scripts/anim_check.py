"""Animation and textured-asset check (M2 exit proof): a moonlit, shadowed scene with
  (a) a Sepulcra creature exported to glTF with textures (JPEG base colour, PNG normal and
      metallic-roughness maps) and its clips, re-imported as a `model` entity (GPU-skinned),
  (b) procedural recipe models (tombstones, warding stones, braziers, a rune pillar),
  (c) Sepulcra creatures built from recipes playing walk / sprint / attack / death clips,
captured as filmstrips with sight_frame time_s.

What it checks:
  - model_build reports bones and clips for rigged recipes and for the glTF asset (source gltf,
    3 textures with mip chains),
  - every creature is a sight subject on screen,
  - the same time_s twice renders identical pixels (a pose is a function of render time),
  - every creature's screen box changes between filmstrip times (it moves),
  - a crossfade (world_set anim with crossfade) starts at the engine clock: mid-fade and
    after-fade captures differ from the pre-fade one,
  - anim survives world_save / world_load,
  - zero validation errors at shutdown.

Captures (PNG + packet JSON) land in OUT/captures/run-*/; the filmstrip list in OUT/anim_check.json.
LOOK AT THE PNGs before claiming anything about them.

Usage: cargo build -p specula && python scripts/anim_check.py [--port 7493] [--out DIR] [--adapter X] [--exe PATH]
Needs a GPU (Vulkan); runs headless.
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
from specula_mcp import ROOT, Engine  # noqa: E402

SEPULCRA = ROOT / "games" / "sepulcra" / "assets"

ELEV, AZ = math.radians(32), math.radians(60)
NIGHT = {
    "sun_direction": [math.cos(ELEV) * math.sin(AZ), math.sin(ELEV), math.cos(ELEV) * math.cos(AZ)],
    "sun_color": "#c9d6ff",
    "sun_intensity": 1.4,
    "ambient_color": "#2a3448",
    "ambient_intensity": 2.2,
    "exposure": 1.15,
    "tonemap": True,
    "background": "#0b1020",
    "shadow": {"enabled": True, "distance": 60, "resolution": 4096, "softness": 1.5},
    "bloom": {"enabled": True, "threshold": 1.0, "intensity": 0.6},
    "fxaa": True,
    "fog": {"enabled": True, "density": 0.012, "height": -1.0, "falloff": 0.3},
    "sky": {"enabled": True, "ground": "#06080e"},
}

# (entity name, model source, clip, speed, position, yaw): the small creatures in a row facing
# the camera, the 8 m colossus behind them.
CREATURES = [
    ("gltf_shambler", "imported/shambler_tex.glb", "walk", 1.0, [-3.6, 0, 0.5], 20),
    ("shambler", "recipes/shambler.json", "attack", 1.0, [-1.6, 0, 0.3], 10),
    ("runner", "recipes/skeleton_runner.json", "sprint", 1.0, [0.2, 0, 0.6], -10),
    ("wraith", "recipes/wraith.json", "swoop", 1.0, [2.0, 0, 0.2], -15),
    ("fallen", "recipes/shambler.json", "death", 1.0, [4.0, 0, 0.5], -30),
    ("colossus", "recipes/bone_colossus.json", "walk", 0.8, [6.5, 0, -6.5], -35),
]
PROPS = [
    ("recipes/tombstone.json", {}, [-4.5, 0, -4.0]),
    ("recipes/tombstone.json", {}, [-1.5, 0, -4.6]),
    ("recipes/warding_stone.json", {"tier": 2}, [2.0, 0, -4.2]),
    ("recipes/brazier_cairn.json", {}, [6.0, 0, -3.5]),
    ("recipes/rune_pillar.json", {"tier": 1}, [10.5, 0, -4.5]),
    ("recipes/graveyard_fence.json", {}, [-8.0, 0, -6.0]),
]
TIMES = [0.0, 0.35, 0.7, 1.05, 1.4]


def decode_png(path):
    """RGBA8 (or RGB8) non-interlaced PNG -> (w, h, bytes RGBA)."""
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


def screen_box(subject, grow=0.3):
    """A subject's `screen` rect [x, y, w, h] as [x0, y0, x1, y1], grown by `grow` of its size
    on each side (a pose reaches beyond the rest bounds the rect comes from)."""
    r = subject.get("screen")
    if not r:
        return None
    gx, gy = r[2] * grow, r[3] * grow
    return [r[0] - gx, r[1] - gy, r[0] + r[2] + gx, r[1] + r[3] + gy]


def box_diff(img_a, img_b, box):
    """Fraction of pixels inside `box` ([x0, y0, x1, y1]) whose colour differs by more than 8."""
    w, h, bpp, a = img_a
    _, _, _, b = img_b
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


def make_assets(out):
    """A copy of Sepulcra's style, recipes and anims, plus textured GLB exports."""
    assets = out / "assets"
    shutil.rmtree(assets, ignore_errors=True)
    assets.mkdir(parents=True)
    shutil.copy(SEPULCRA / "style.json", assets / "style.json")
    shutil.copytree(SEPULCRA / "recipes", assets / "recipes")
    shutil.copytree(SEPULCRA / "anims", assets / "anims")
    (assets / "imported").mkdir()
    for name in ("shambler",):
        cmd = ["cargo", "run", "-q", "-p", "specula-gltf", "--example", "textured_creature", "--",
               str(SEPULCRA), name, str(assets / "imported" / f"{name}_tex.glb")]
        r = subprocess.run(cmd, cwd=str(ROOT), capture_output=True, text=True)
        if r.returncode != 0:
            sys.exit(f"exporting {name}: {r.stderr}")
        print(r.stdout.strip())
    return assets


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--port", type=int, default=7493)
    ap.add_argument("--out", default=str(ROOT / "target" / "anim-check"))
    ap.add_argument("--exe", default=str(ROOT / "target" / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")))
    ap.add_argument("--adapter", default=None)
    a = ap.parse_args()
    if a.port == 7471:
        sys.exit("port 7471 belongs to the live engine; pick another")
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    assets = make_assets(out)
    shutil.rmtree(out / "worlds", ignore_errors=True)
    args = ["--headless", "--assets", str(assets), "--captures", str(out / "captures"), "--worlds", str(out / "worlds")]
    if a.adapter:
        args += ["--adapter", a.adapter]
    failures = []

    def check(ok, what):
        print(("ok    " if ok else "FAIL  ") + what)
        if not ok:
            failures.append(what)

    summary = {"filmstrip": [], "crossfade": [], "reports": {}}
    with Engine(a.exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as engine:
        mcp = engine.mcp("anim-check")
        mcp.call("agent_hello", name="anim_check", role="reviewer")
        mcp.call("world_env", **NIGHT)

        # 1. Reports: bones and clips.
        widths = {}
        for name, src, clip, *_ in CREATURES:
            if src in summary["reports"]:
                continue
            r = mcp.json("model_build", recipe=src)
            summary["reports"][src] = r
            clips = [c["name"] for c in r.get("clips", [])]
            lo, hi = r["bounds"]["min"], r["bounds"]["max"]
            widths[src] = (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2])
            print(f"{src:<34} {r.get('source')} bones {r.get('bones')} clips {clips} textures {len(r.get('textures', []))}"
                  f" size {widths[src][0]:.1f} x {widths[src][1]:.1f} x {widths[src][2]:.1f}")
            check(clip in clips and r.get("bones", 0) > 0, f"{src}: {r.get('bones')} bones, clips include '{clip}'")
        g = summary["reports"]["imported/shambler_tex.glb"]
        check(g.get("source") == "gltf" and len(g.get("textures", [])) == 3 and all(t["mips"] == 9 for t in g["textures"]),
              f"glTF asset: source {g.get('source')}, textures {g.get('textures')}")

        # 2. The scene: ground, props, creatures in a row (wide ones get room).
        ops = [{"op": "spawn", "shape": "group", "path": "/scene"},
               {"op": "spawn", "shape": "quad", "path": "/scene/ground", "position": [3, 0, -2], "scale": [60, 1, 40],
                "color": "#4a525e", "roughness": 0.9}]
        for i, (src, params, pos) in enumerate(PROPS):
            ops.append({"op": "spawn", "shape": "model", "path": f"/scene/prop_{i}", "recipe": src, "params": params,
                        "position": pos})
        for name, src, clip, speed, pos, yaw in CREATURES:
            ops.append({"op": "spawn", "shape": "model", "path": f"/scene/{name}", "recipe": src, "position": pos,
                        "rotation_deg": [0, yaw, 0], "anim": {"clip": clip, "speed": speed}})
        res = mcp.json("world_batch", ops=ops)
        check(res["applied"] == len(ops), f"world_batch spawned {res['applied']} of {len(ops)}")
        cam = dict(width=1600, height=900, eye=[1.2, 3.4, 10.5], target=[1.4, 2.2, -2.0], fov_y_deg=50, max_subjects=200)

        # 3. Filmstrip.
        frames = []
        for t in TIMES:
            p = mcp.frame(time_s=t, **cam)
            frames.append(p)
            summary["filmstrip"].append({"time_s": t, "png": p["image"]["path"], "packet": p["image"]["packet_path"]})
            print(f"t={t:<5} {p['image']['path']}  models {p['models']}  warnings {p['warnings']}")
        subjects = {s["name"]: s for s in frames[0]["subjects"]}
        for name, *_ in CREATURES:
            s = subjects.get(name)
            check(s is not None and s["on_screen"], f"{name} is a subject on screen")
        check(all(not f["warnings"] for f in frames), f"no capture warnings: {[f['warnings'] for f in frames if f['warnings']]}")
        check(all(f["models"]["missing"] == 0 for f in frames), "every model drawn")
        again = mcp.frame(time_s=TIMES[1], **cam)
        imgs = [decode_png(f["image"]["path"]) for f in frames]
        same = decode_png(again["image"]["path"])
        full = [0, 0, cam["width"] - 1, cam["height"] - 1]
        check(box_diff(imgs[1], same, full) == 0.0, "the same time_s renders identical pixels")
        for name, *_ in CREATURES:
            s = subjects.get(name)
            if not s or not screen_box(s):
                continue
            union = screen_box(s)
            moved = [box_diff(imgs[k], imgs[k + 1], union) for k in range(len(imgs) - 1)]
            print(f"  {name:<14} box {[round(v) for v in union]}  changed between frames {[round(m, 3) for m in moved]}")
            check(sum(m > 0.01 for m in moved) >= 2, f"{name} moves across the filmstrip")

        # Close-up of the imported, textured creature (base colour, normal and roughness maps).
        close = dict(width=1200, height=900, eye=[-2.7, 1.6, 3.1], target=[-3.6, 0.85, 0.5], fov_y_deg=40, max_subjects=20)
        for t in (0.2, 0.75):
            p = mcp.frame(time_s=t, **close)
            summary["filmstrip"].append({"time_s": t, "closeup": True, "png": p["image"]["path"], "packet": p["image"]["packet_path"]})
            print(f"closeup t={t}: {p['image']['path']}")

        # 4. Crossfade: the shambler switches from attack to walk now, fading over 0.4 s.
        mcp.call("world_set", path="/scene/shambler", anim={"clip": "walk", "crossfade": 0.4})
        anim = mcp.json("world_query", path_prefix="/scene/shambler", fields=["anim"])["items"][0]["anim"]
        start = anim["start"]
        check(anim.get("from", {}).get("clip") == "attack" and anim.get("crossfade") == 0.4,
              f"crossfade recorded: {anim}")
        shots = {}
        for tag, t in [("before", start - 0.5), ("mid_fade", start + 0.2), ("after", start + 0.9)]:
            p = mcp.frame(time_s=t, **cam)
            shots[tag] = p
            summary["crossfade"].append({"tag": tag, "time_s": t, "png": p["image"]["path"], "packet": p["image"]["packet_path"]})
            print(f"crossfade {tag} t={t:.2f}: {p['image']['path']}")
        box = screen_box(next(q for q in shots["before"]["subjects"] if q["name"] == "shambler"))
        cf = {k: decode_png(v["image"]["path"]) for k, v in shots.items()}
        d_mid = box_diff(cf["before"], cf["mid_fade"], box)
        d_after = box_diff(cf["mid_fade"], cf["after"], box)
        check(d_mid > 0.01 and d_after > 0.01, f"the crossfade moves the shambler (before->mid {d_mid:.3f}, mid->after {d_after:.3f})")

        # The imported creature deleted (its build and GPU textures go) and spawned again: its
        # texture pixels left the CPU with the first upload, so the asset is imported afresh
        # and draws exactly as before.
        first_close = next(f for f in summary["filmstrip"] if f.get("closeup") and f["time_s"] == 0.2)
        mcp.call("world_delete", path="/scene/gltf_shambler")
        mcp.frame(time_s=0.2, **close)
        g_name, g_src, g_clip, g_speed, g_pos, g_yaw = CREATURES[0]
        mcp.call("world_spawn", shape="model", path=f"/scene/{g_name}", recipe=g_src, position=g_pos,
                 rotation_deg=[0, g_yaw, 0], anim={"clip": g_clip, "speed": g_speed})
        back = mcp.frame(time_s=0.2, **close)
        d = box_diff(decode_png(first_close["png"]), decode_png(back["image"]["path"]), [0, 0, close["width"] - 1, close["height"] - 1])
        check(back["models"]["missing"] == 0 and d < 0.002,
              f"the textured asset spawned again draws as before ({d:.4f} of pixels differ): {back['image']['path']}")

        # 5. Save and load keep the anim component.
        mcp.call("world_save", name="anim_check")
        mcp.call("world_load", name="anim_check")
        after = mcp.json("world_query", path_prefix="/scene/shambler", fields=["anim"])["items"][0]["anim"]
        shift = after["start"] - anim["start"]
        same = {k: v for k, v in after.items() if k not in ("start", "from")} == {k: v for k, v in anim.items() if k not in ("start", "from")}
        from_shift = after["from"]["start"] - anim["from"]["start"]
        check(same and 0 <= shift < 30 and abs(from_shift - shift) < 1e-9,
              f"anim survives save/load, moved onto the clock by the time between save and load ({shift:.3f} s): {after}")
        # A clip the creature lacks is a warning in the reply and in captures (it draws its bind pose).
        reply = mcp.json("world_set", path="/scene/fallen", anim="moonwalk")
        warn = mcp.frame(time_s=1.0, **cam)["warnings"]
        check("moonwalk" in reply.get("anim_warning", "") and any("moonwalk" in w for w in warn),
              f"an unknown clip is reported: {reply.get('anim_warning')} / {warn}")
        mcp.call("world_set", path="/scene/fallen", anim={"clip": "death", "start": 0})
        perf = mcp.json("perf_frame")
        print(f"perf models: {perf['models']}")
        summary["models"] = perf["models"]
    errors = engine.validation_errors()
    check(errors == 0, f"validation errors at shutdown: {errors}")

    # 6. A restart: the new engine's clock starts at 0, the saved animations continue (the world
    # file records the clock they were on).
    with Engine(a.exe, a.port, out / "engine2.log", *args, cwd=str(ROOT)) as engine2:
        mcp = engine2.mcp("anim-check")
        mcp.call("world_load", name="anim_check")
        loaded = mcp.json("world_query", path_prefix="/scene/shambler", fields=["anim"])["items"][0]["anim"]
        shots = []
        for _ in range(3):
            shots.append(mcp.frame(**cam))  # default time: the new engine's clock
            time.sleep(0.4)
        box = screen_box(next(q for q in shots[0]["subjects"] if q["name"] == "shambler"))
        imgs = [decode_png(s["image"]["path"]) for s in shots]
        moved = [box_diff(imgs[k], imgs[k + 1], box) for k in range(2)]
        times = [s["time_s"] for s in shots]
        print(f"after restart: anim {loaded}; captures at clock {times}; shambler changed {[round(m, 3) for m in moved]}")
        check(all(m > 0.01 for m in moved), "after a restart and world_load the walk keeps moving (no freeze)")
        summary["restart"] = [s["image"]["path"] for s in shots]
    check(engine2.validation_errors() == 0, f"validation errors at the second shutdown: {engine2.validation_errors()}")
    (out / "anim_check.json").write_text(json.dumps(summary, indent=1))
    print(f"summary: {out / 'anim_check.json'}")
    if failures:
        sys.exit(f"{len(failures)} check(s) failed")
    print("anim check: all checks passed")


if __name__ == "__main__":
    main()
