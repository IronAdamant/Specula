"""Procedural modelling tiers 2-4, live: booleans, SDF sculpts, a scattered graveyard and a
building with procedural materials, built and captured through MCP.

What it does and checks:
  - model_build reports for the Sepulcra procgen recipes (headstone variants cut with bevelled
    booleans, an SDF rock, a ruined wall with SDF rubble, a textured crypt from the building
    generator, the graveyard ground and a whole graveyard plot): triangles per LOD within budget,
    build time, the procgen steps each part took;
  - procgen_texture bakes the crypt stone (tiling PBR set) and returns its strip;
  - procgen_terrain puts the graveyard ground's heightfield collider on its entity (world_set);
  - procgen_scatter places carved headstones and rocks on that ground and spawns them as model
    entities (world_batch); physics_raycast lands on the heightfield where the ground mesh is;
  - captures (PNG + packet) of each piece and of the scattered graveyard, perf_frame GPU time;
  - zero validation errors at shutdown.

Captures land in OUT/captures/run-*/; the summary in OUT/procgen.json. Look at the PNGs before
claiming anything about them.

Usage: cargo build -p specula && python scripts/procgen_check.py [--port 7620] [--exe PATH] [--out DIR]
       [--adapter integrated]
Needs a GPU (Vulkan); runs headless.
"""
import argparse
import json
import math
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

ASSETS = ROOT / "games" / "sepulcra" / "assets"

ELEV, AZ = math.radians(32), math.radians(55)
NIGHT = {
    "sun_direction": [math.cos(ELEV) * math.sin(AZ), math.sin(ELEV), math.cos(ELEV) * math.cos(AZ)],
    "sun_color": "#c9d6ff",
    "sun_intensity": 1.6,
    "ambient_color": "#2a3448",
    "ambient_intensity": 2.4,
    "exposure": 1.2,
    "tonemap": True,
    "background": "#0b1020",
    "shadow": {"enabled": True, "distance": 80, "resolution": 4096, "softness": 1.5},
    "bloom": {"enabled": True, "threshold": 1.0, "intensity": 0.5},
    "fxaa": True,
    "sky": {"enabled": True, "ground": "#06080e"},
}

# (label, recipe, params, budget)
RECIPES = [
    ("headstone_arched", "recipes/headstone_carved.json", {"variant": 1}, 900),
    ("headstone_cross", "recipes/headstone_carved.json", {"variant": 2}, 900),
    ("headstone_obelisk", "recipes/headstone_carved.json", {"variant": 3}, 900),
    ("grave_rock", "recipes/grave_rock.json", {}, 1600),
    ("ruined_wall", "recipes/ruined_wall.json", {}, 4000),
    ("crypt", "recipes/crypt.json", {}, 2500),
    ("graveyard_ground", "recipes/graveyard_ground.json", {}, 12000),
    ("graveyard_plot", "recipes/graveyard_plot.json", {}, 30000),
]


def default_exe():
    target = os.environ.get("CARGO_TARGET_DIR") or str(ROOT / "target")
    return str(Path(target) / "debug" / ("specula.exe" if sys.platform == "win32" else "specula"))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--port", type=int, default=7620)
    ap.add_argument("--out", default=str(ROOT / "target" / "procgen-check"))
    ap.add_argument("--exe", default=default_exe())
    ap.add_argument("--adapter", default=None)
    a = ap.parse_args()
    if a.port == 7471:
        sys.exit("port 7471 belongs to the live engine; pick another")
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    args = ["--headless", "--assets", str(ASSETS), "--captures", str(out / "captures"), "--worlds", str(out / "worlds")]
    if a.adapter:
        args += ["--adapter", a.adapter]
    failures = []

    def check(ok, what):
        print(("ok    " if ok else "FAIL  ") + what)
        if not ok:
            failures.append(what)

    summary = {"reports": {}, "captures": {}, "packets": {}}
    with Engine(a.exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as engine:
        mcp = engine.mcp("procgen-check")
        mcp.call("agent_hello", name="procgen", role="builder")
        mcp.call("world_env", **NIGHT)
        tools = mcp.tools()
        check(all(t in tools for t in ("procgen_texture", "procgen_terrain", "procgen_scatter")), "procgen tools are listed")

        # 1. Reports.
        print(f"{'model':<20} {'tris per LOD':<20} {'size (m)':<20} {'build ms':>9}")
        for label, rel, params, budget in RECIPES:
            r = mcp.json("model_build", recipe=rel, params=params)
            lo, hi = r["bounds"]["min"], r["bounds"]["max"]
            size = [hi[i] - lo[i] for i in range(3)]
            tris = [int(l["triangles"]) for l in r["lods"]]
            print(f"{label:<20} {'/'.join(map(str, tris)):<20} {'%.2f x %.2f x %.2f' % tuple(size):<20} {r['build_ms']:>9.1f}")
            check(tris[0] <= budget, f"{label}: {tris[0]} triangles within {budget}")
            check(len(tris) >= 2, f"{label}: {len(tris)} LODs")
            check("bevel_failed" not in json.dumps(r), f"{label}: every boolean's bevel went through")
            summary["reports"][label] = r
        crypt = summary["reports"]["crypt"]
        check(any("+" in m for m in crypt["materials"]), f"the crypt draws a textured material: {crypt['materials']}")

        # 2. The crypt stone, baked and seen.
        t = mcp.call("procgen_texture", file="recipes/tex_crypt_stone.json", save=True, name="crypt_stone")
        info = json.loads(next(c["text"] for c in t["content"] if c["type"] == "text"))
        check(any(c["type"] == "image" for c in t["content"]), "procgen_texture returns an image")
        check(info["gpu_bytes"] <= 1 << 20, f"crypt stone: {info['size']} px, {info['gpu_bytes']} GPU bytes, {info['bake_ms']} ms")
        summary["texture"] = info
        if "saved" in info:
            summary["captures"]["texture_strip"] = info["saved"]

        # 3. A lineup of the pieces.
        ops = [{"op": "spawn", "shape": "group", "path": "/procgen"},
               {"op": "spawn", "shape": "quad", "path": "/procgen/floor", "position": [0, 0, 0], "scale": [60, 1, 40], "color": "#3d434d", "roughness": 0.95}]
        lineup = [("headstone_arched", -1.6, 6.0), ("headstone_cross", 0.0, 6.0), ("headstone_obelisk", 1.6, 6.0),
                  ("grave_rock", 3.2, 6.0), ("ruined_wall", -6.0, 4.0), ("crypt", 7.5, 3.0)]
        spec = {label: (rel, params) for label, rel, params, _ in RECIPES}
        for label, x, z in lineup:
            rel, params = spec[label]
            ops.append({"op": "spawn", "shape": "model", "path": f"/procgen/{label}", "recipe": rel, "params": params, "position": [x, 0, z]})
        # The plot as one model, further back.
        ops.append({"op": "spawn", "shape": "model", "path": "/procgen/graveyard_plot", "recipe": "recipes/graveyard_plot.json", "position": [-2, 0, -6]})
        res = mcp.json("world_batch", ops=ops)
        check(res["applied"] == len(ops), f"lineup spawned ({res['applied']} ops)")

        shots = {
            "headstones": dict(eye=[0.8, 1.5, 10.8], target=[0.8, 0.55, 6.0], fov_y_deg=42),
            "crypt": dict(eye=[3.2, 2.6, 9.5], target=[7.5, 1.6, 3.0], fov_y_deg=45),
            "ruined_wall": dict(eye=[-3.5, 1.6, 8.0], target=[-6.0, 1.0, 4.0], fov_y_deg=45),
            "graveyard_plot": dict(eye=[-2, 6.5, 2.5], target=[-2, 0, -6], fov_y_deg=50),
            "lineup": dict(eye=[0, 6, 16], target=[0, 1, 2], fov_y_deg=55),
        }

        def capture(tag, **view):
            p = mcp.frame(width=1600, height=900, time_s=1.0, max_subjects=200, **view)
            summary["captures"][tag] = p["image"]["path"]
            summary["packets"][tag] = p["image"]["packet_path"]
            print(f"capture {tag}: {p['image']['path']}  models {p['models']}")
            check(p["models"]["missing"] == 0, f"{tag}: every model built ({p['models']})")
            return p

        for tag, view in shots.items():
            capture(tag, **view)

        # 4. The scattered graveyard, instanced: ground entity + collider + scattered entities.
        gx, gz = 0.0, -40.0
        mcp.json("world_spawn", shape="model", path="/procgen/ground", recipe="recipes/graveyard_ground.json", position=[gx, 0, gz])
        tr = mcp.call("procgen_terrain", recipe="recipes/graveyard_ground.json", collider={"entity": "/procgen/ground"}, preview=True)
        tinfo = json.loads(next(c["text"] for c in tr["content"] if c["type"] == "text"))
        check(tinfo.get("collider", {}).get("columns") == 65, f"heightfield collider on the ground: {tinfo.get('collider')}")
        check(0.3 < tinfo["walkable_share"] <= 1.0, f"walkable share {tinfo['walkable_share']} at {tinfo['max_slope_deg']} deg")
        summary["terrain"] = tinfo
        mcp.json("world_spawn", shape="group", path="/procgen/graves")
        sc = mcp.json("procgen_scatter", area={"rect": [21, 15]}, at=[gx, 0, gz], min_distance=1.3, count=90, seed=7,
                      slope=[0, 16], avoid=[[gx, gz, 1.6], [gx, gz + 6, 1.2]], yaw=[-10, 10], tilt=4, align=0.6, sink=0.03,
                      ground={"recipe": "recipes/graveyard_ground.json", "at": [gx, 0, gz]},
                      items=[{"model": {"recipe": "recipes/headstone_carved.json", "params": {"variant": 1, "tilt": [-1, 1]}}, "weight": 3},
                             {"model": {"recipe": "recipes/headstone_carved.json", "params": {"variant": 2, "tilt": [-1, 1]}}, "weight": 1.5},
                             {"model": {"recipe": "recipes/headstone_carved.json", "params": {"variant": 3, "tilt": [-1, 1]}}, "weight": 1}],
                      spawn={"parent": "/procgen/graves", "name": "grave"})
        check(sc.get("spawned", 0) >= 30, f"scattered {sc.get('spawned')} graves ({sc['placed']} placed, rejected {sc['rejected']})")
        # The answer lists only the first placements: read every grave back from the world.
        graves = []
        cursor = None
        while True:
            q = mcp.json("world_query", path_prefix="/procgen/graves", limit=100, **({"cursor": cursor} if cursor else {}))
            graves += [e["position"] for e in q.get("items") or [] if "/grave_" in e.get("path", "") and "position" in e]
            cursor = q.get("next_cursor")
            if not cursor:
                break
        check(len(graves) == sc.get("spawned"), f"read back {len(graves)} graves")
        rocks = mcp.json("procgen_scatter", area={"rect": [22, 16]}, at=[gx, 0, gz], min_distance=2.5, count=14, seed=8,
                         align=1, sink=0.05, scale=[0.4, 0.9], ground={"recipe": "recipes/graveyard_ground.json", "at": [gx, 0, gz]},
                         avoid=[[gx, gz, 2.0]] + [[g[0], g[2], 0.9] for g in graves],
                         items=[{"model": {"recipe": "recipes/grave_rock.json"}}], spawn={"parent": "/procgen/graves", "name": "rock"})
        check(rocks.get("spawned", 0) >= 3, f"scattered {rocks.get('spawned')} rocks")
        rock_pos = []
        cursor = None
        while True:
            q = mcp.json("world_query", path_prefix="/procgen/graves", limit=100, **({"cursor": cursor} if cursor else {}))
            rock_pos += [e["position"] for e in q.get("items") or [] if "/rock_" in e.get("path", "") and "position" in e]
            cursor = q.get("next_cursor")
            if not cursor:
                break
        gap = min((math.hypot(r[0] - g[0], r[2] - g[2]) for r in rock_pos for g in graves), default=99)
        check(len(rock_pos) == rocks.get("spawned") and gap >= 0.89, f"rocks keep clear of every grave (nearest {gap:.2f} m)")
        summary["scatter"] = {"graves": {k: sc[k] for k in ("placed", "sampled", "rejected", "spawned")}, "rocks": rocks.get("spawned"),
                              "nearest_rock_to_grave_m": round(gap, 3)}
        # The collider is where the mesh is: rays down onto placed graves' feet hit the ground.
        time.sleep(0.5)  # a physics tick after the collider arrived
        hits = 0
        for p in sc["placements"][:8]:
            x, y, z = p["position"]
            hit = mcp.json("physics_raycast", origin=[x + 0.4, y + 5, z + 0.4], direction=[0, -1, 0], max_distance=20)
            h = hit.get("hit") or (hit.get("hits") or [None])[0]
            if h and abs(h["point"][1] - (y + 0.03)) < 0.35:
                hits += 1
        check(hits >= 6, f"physics rays land on the heightfield at the graves' feet ({hits}/8)")
        capture("scattered_graveyard", eye=[gx + 9, 7, gz + 13], target=[gx, 0.3, gz], fov_y_deg=50)
        capture("scattered_graveyard_close", eye=[gx + 3.5, 1.7, gz + 5.5], target=[gx, 0.4, gz + 1.5], fov_y_deg=45)
        perf = mcp.json("perf_frame")
        off = perf.get("offscreen") or {}
        gpu = (off.get("gpu") or {}).get("total_ms")
        summary["perf"] = {"capture_gpu_ms": gpu, "draws": off.get("draws"), "models": perf.get("models")}
        print(f"perf: last capture (1600x900, the scattered graveyard close-up) GPU {gpu} ms; draws {off.get('draws')}")
        (out / "procgen.json").write_text(json.dumps(summary, indent=1), encoding="utf-8")
    errors = engine.validation_errors()
    check(errors == 0, f"validation errors at shutdown: {errors}")
    print(f"summary: {out / 'procgen.json'}")
    if failures:
        sys.exit(f"{len(failures)} check(s) failed")
    print("procgen check: all checks passed")


if __name__ == "__main__":
    main()
