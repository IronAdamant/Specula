"""The terrain renderer and stylised water, live (docs/design/TERRAIN.md).

What it does and checks:
  - a 1 km x 1 km terrain: 8 x 8 tiles of one recipe (129 samples, 128 m, 1 m cells, eroded,
    edge-locked, four splat layers, one textured with a texture graph and projected triplanar on
    slopes), each tile a model entity whose recipe says "render": "terrain"; a lake plane over
    the valleys (the `water` component);
  - captures of the whole terrain, a close view, the shoreline; sight packets list the tiles as
    `kind: terrain` and the lake as `kind: water`, with visible pixels from the ID buffer;
  - the same lake as every preset (lake, murk, frozen, lava, swamp) by world_set;
  - terrain_info: drawn by the terrain renderer, LOD levels, residency, and the centre tile's
    heightfield collider (put on by procgen_terrain) exactly the drawn surface;
  - terrain_edit: a raised mound (with the collider rewritten) shows in the capture, terrain_info
    still says the collider matches, and a physics ray lands on the new height;
  - sight packets never report water that isn't drawn (while the terrain pipelines are still
    building and the tiles draw their stand-ins, no water pixel is counted);
  - terrain_edit is one journal entry with its collider: a bad collider target refuses with a
    structured error and changes nothing (a retry doesn't append the strokes twice), a claimed
    recipe refuses, and one world_undo takes back both the recipe and the collider (terrain_info
    matches again, the ray lands on the old height);
  - a moonlit, Sepulcra-like lighting set (low cool moon, blue fill, mist) over the same views;
  - a thin post standing in the water 125 m away stays visible (pixels and packet);
  - one build drawn near and 1 km away (the near one stays drawn, the engine stays up), and 3 x 3
    tiles of one build (uploads stop);
  - 300 tiles of one landscape (300 builds): 256 records drawn as terrain, the rest by their
    stand-ins, none vanishes;
  - headless, an 8 MB memory budget: the least recently wanted sets make room as the view moves;
  - with --window: 10,000 plain cubes and no terrain cost the frame next to nothing in terrain
    and water (from their indexes), a far capture never evicts what the window draws, and
    repeated far captures don't re-make their sets;
  - reproducible captures (the same view and time: the same bytes);
  - perf_frame GPU ms per pass (scene, shadow, water) of the 1600x900 captures;
  - zero validation errors at shutdown.

Captures land in OUT/captures/run-*/; the summary in OUT/terrain.json. Look at the PNGs before
claiming anything about them.

Usage: cargo build -p specula --release && python scripts/terrain_check.py [--port 7650] [--exe PATH]
       [--out DIR] [--adapter integrated] [--window]
Needs a GPU (Vulkan); runs headless (with --window, in a 1600x900 window, recording its frames too).
"""
import argparse
import hashlib
import json
import math
import os
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

ELEV, AZ = math.radians(28), math.radians(55)
DUSK = {
    "sun_direction": [math.cos(ELEV) * math.sin(AZ), math.sin(ELEV), math.cos(ELEV) * math.cos(AZ)],
    "sun_color": "#fff1dc",
    "sun_intensity": 2.6,
    "ambient_color": "#8fa3bf",
    "ambient_intensity": 0.9,
    "exposure": 1.0,
    "tonemap": True,
    "background": "#9fb6d0",
    "shadow": {"enabled": True, "distance": 400, "resolution": 2048, "softness": 1.5},
    "bloom": {"enabled": True, "threshold": 1.2, "intensity": 0.35},
    "fog": {"enabled": True, "color": "#a9bccf", "density": 0.0012, "falloff": 0.01, "height": 0},
    "fxaa": True,
    "sky": {"enabled": True, "zenith": "#4f73a8", "horizon": "#c3d2e2", "ground": "#55606c"},
}

# Sepulcra's night (games/sepulcra-app game.rs `night`): a low cool moon, a blue fill doing most of
# the lighting, a near-black void below the horizon, blue mist.
MOON_ELEV, MOON_AZ = math.radians(30), math.radians(60)
MOON = {
    "sun_direction": [math.cos(MOON_ELEV) * math.sin(MOON_AZ), math.sin(MOON_ELEV), math.cos(MOON_ELEV) * math.cos(MOON_AZ)],
    "sun_color": "#d0d9f0",
    "sun_intensity": 0.55,
    "ambient_color": "#2f3850",
    "ambient_intensity": 1.7,
    "exposure": 1.0,
    "tonemap": True,
    "background": "#0b1020",
    "shadow": {"enabled": True, "distance": 400, "resolution": 2048, "softness": 1.5},
    "bloom": {"enabled": True, "threshold": 1.0, "intensity": 0.6},
    "fog": {"enabled": True, "color": "#2a3448", "density": 0.0016, "falloff": 0.01, "height": 0},
    "fxaa": True,
    "sky": {"enabled": True, "zenith": "#070b18", "horizon": "#2a3448", "ground": "#06080e"},
}

# A small terrain for the many-entities step (17 samples: no coarse copy; each tile its own part of
# one noise field, so the tiles join into one landscape).
SMALL_RECIPE = {"recipe": 1, "name": "small_t", "seed": 3,
                "params": {"tx": {"default": 0, "min": 0, "max": 32, "integer": True}, "tz": {"default": 0, "min": 0, "max": 32, "integer": True}},
                "parts": [{"name": "ground", "material": "slab_stone", "generate": {
    "op": "terrain", "render": "terrain", "size": [16, 16], "samples": [17, 17], "offset": ["tx * 16", "tz * 16"],
    "layers": [{"kind": "fbm", "scale": 20, "octaves": 3, "amplitude": 2}],
    "splat": [{"name": "grass", "color": "#5a7a3a"}]}}]}

# One 129-sample build drawn by several entities (the shared-build steps).
HILL_RECIPE = {"recipe": 1, "name": "hill", "seed": 4, "parts": [{"name": "ground", "material": "slab_stone", "generate": {
    "op": "terrain", "render": "terrain", "size": [128, 128], "samples": [129, 129],
    "layers": [{"kind": "fbm", "scale": 40, "octaves": 4, "amplitude": 8}],
    "splat": [{"name": "grass", "color": "#5a7a3a"}]}}]}

# 257-sample tiles of one landscape (about 1 MB a full set): the memory-budget step.
BIG_RECIPE = {"recipe": 1, "name": "big_t", "seed": 3,
              "params": {"tx": {"default": 0, "min": -16, "max": 16, "integer": True}, "tz": {"default": 0, "min": -16, "max": 16, "integer": True}},
              "parts": [{"name": "ground", "material": "slab_stone", "generate": {
    "op": "terrain", "render": "terrain", "size": [256, 256], "samples": [257, 257], "offset": ["tx * 256", "tz * 256"],
    "layers": [{"kind": "fbm", "scale": 120, "octaves": 4, "amplitude": 20}],
    "splat": [{"name": "grass", "color": "#5a7a3a", "slope": [0, 20]}, {"name": "rock", "color": "#777777", "slope": [18, 90]}]}}]}

TILES = 8  # 8 x 8 tiles of 128 m: 1 km x 1 km
TILE_M = 128

TILE_RECIPE = {
    "recipe": 1,
    "name": "terrain_tile",
    "description": "One 128 m tile of a 1 km terrain (terrain_check.py): one recipe, offset per tile, eroded with the edges locked.",
    "params": {"tx": {"default": 0, "min": -16, "max": 16, "integer": True}, "tz": {"default": 0, "min": -16, "max": 16, "integer": True}},
    "textures": {"rock": "tex_terrain_rock"},
    "seed": 11,
    "parts": [
        {"name": "ground", "material": "slab_stone", "generate": {
            "op": "terrain", "render": "terrain",
            "size": [TILE_M, TILE_M], "samples": [TILE_M + 1, TILE_M + 1],
            "offset": [f"tx * {TILE_M}", f"tz * {TILE_M}"], "edge_lock": 6,
            "layers": [{"kind": "fbm", "scale": 260, "octaves": 5, "amplitude": 30}, {"kind": "ridged", "scale": 90, "octaves": 4, "amplitude": 10}],
            "erosion": {"droplets": 6000, "strength": 0.5, "steps": 40},
            "skirt": 0.5, "grain": 0.5,
            "splat": [
                {"name": "grass", "color": "#5a7a3a", "slope": [0, 16], "soft": 5, "noise": 0.4, "noise_scale": 14, "uv_scale": 7, "roughness": 0.95},
                {"name": "earth", "color": "roots", "slope": [13, 30], "soft": 5, "uv_scale": 5},
                {"name": "rock", "color": "#ffffff", "slope": [26, 90], "soft": 5, "texture": "rock", "uv_scale": 6, "triplanar": 32, "roughness": 0.85},
                {"name": "snow", "color": "#dde5ee", "height": [30, 200], "soft": 5, "roughness": 0.6, "specular": 0.15},
            ],
        }},
    ],
}

ROCK_GRAPH = {
    "texgraph": 1,
    "name": "terrain_rock",
    "description": "Layered grey rock for terrain slopes (terrain_check.py), tiling, 256 px.",
    "size": 256,
    "seed": 5,
    "nodes": {
        "strata": {"op": "noise", "kind": "ridged", "scale": 3, "octaves": 5},
        "grain": {"op": "noise", "kind": "fbm", "scale": 12, "octaves": 4},
        "cracks": {"op": "noise", "kind": "worley_edge", "scale": 6},
        "height": {"op": "blend", "a": "strata", "b": "grain", "mode": "multiply", "amount": 0.45},
        "cut": {"op": "blend", "a": "height", "b": "cracks", "mode": "min", "amount": 0.5},
        "albedo": {"op": "gradient_map", "in": "cut", "stops": [[0, "fissure"], [0.35, "strata_dark"], [0.7, "carved_shadow"], [1, "carved_stone"]]},
    },
    "outputs": {"base_color": "albedo", "height": "cut", "normal_strength": 2.5, "roughness": 0.85, "metallic": 0},
}


def default_exe():
    target = os.environ.get("CARGO_TARGET_DIR") or str(ROOT / "target")
    return str(Path(target) / "release" / ("specula.exe" if sys.platform == "win32" else "specula"))


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()[:16]


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--port", type=int, default=7650)
    ap.add_argument("--out", default=str(ROOT / "target" / "terrain-check"))
    ap.add_argument("--exe", default=default_exe())
    ap.add_argument("--adapter", default=None)
    ap.add_argument("--window", action="store_true", help="also run the window (1600x900) and record its frames at the close and "
                    "overview views with perf_capture (frame and GPU p95, per pass)")
    a = ap.parse_args()
    if a.port == 7471:
        sys.exit("port 7471 belongs to the live engine; pick another")
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    # A scratch assets folder: Sepulcra's style, the tile recipe and its rock texture graph.
    assets = out / "assets"
    if assets.exists():
        shutil.rmtree(assets)
    (assets / "recipes").mkdir(parents=True)
    shutil.copy(ROOT / "games" / "sepulcra" / "assets" / "style.json", assets / "style.json")
    (assets / "recipes" / "terrain_tile.json").write_text(json.dumps(TILE_RECIPE, indent=1))
    (assets / "recipes" / "tex_terrain_rock.json").write_text(json.dumps(ROCK_GRAPH, indent=1))
    (assets / "recipes" / "small_t.json").write_text(json.dumps(SMALL_RECIPE, indent=1))
    (assets / "recipes" / "hill.json").write_text(json.dumps(HILL_RECIPE, indent=1))
    (assets / "recipes" / "big_t.json").write_text(json.dumps(BIG_RECIPE, indent=1))
    args = ["--assets", str(assets), "--captures", str(out / "captures"), "--worlds", str(out / "worlds")]
    args += ["--size", "1600x900"] if a.window else ["--headless"]
    if a.adapter:
        args += ["--adapter", a.adapter]
    failures = []

    def check(ok, what):
        print(("ok    " if ok else "FAIL  ") + what)
        if not ok:
            failures.append(what)

    summary = {"captures": {}, "packets": {}, "perf": {}}
    with Engine(a.exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as engine:
        mcp = engine.mcp("terrain-check")
        mcp.call("agent_hello", name="terrain", role="lead")
        mcp.call("world_env", **DUSK)
        tools = mcp.tools()
        check(all(t in tools for t in ("terrain_info", "terrain_edit")), "terrain tools are listed")

        def zone(cap, path):
            return next((z for z in cap.get("top_zones_by_self_ms") or [] if z.get("path") == path), {})

        if a.window:
            # 0. 10,000 plain cubes, no terrain or water: the window's draw list stays within
            # budget (the terrain and water scan visits nothing; review of a89b5eb: 1.3 ms mean,
            # 2 ms p95 against main's 0.63 and 1.2).
            n, side = 10000, 101
            for k0 in range(0, n, 1000):
                mcp.json("world_batch", ops=[{"op": "spawn", "shape": "cube", "path": f"/cubes_{k}", "position": [(k % side) * 1.5 - 75, 0, (k // side) * 1.5 - 75],
                                              "scale": [0.5, 0.5, 0.5]} for k in range(k0, k0 + 1000)])
            mcp.call("window_view", eye=[0, 120, 90], target=[0, 0, 0])
            time.sleep(3.0)
            cap = mcp.json("perf_capture", frames=300, trace=False, label="terrain_cubes")
            dl = zone(cap, "frame/draw_list")
            # The terrain and water work itself (zones of its own under the draw list).
            zs = mcp.json("perf_zones", thread="main", min_ms=0, limit=500).get("zones") or []
            tw = [z for z in zs if str(z.get("path", "")).rsplit("/", 1)[-1] in ("terrain", "terrain_draws")]
            tw_mean = sum(z.get("mean_ms") or 0 for z in tw)
            tz = mcp.json("perf_frame").get("terrain") or {}
            summary["cubes_10k"] = {"draw_list": dl, "terrain_zones": tw, "frame_ms": cap.get("frame_ms"), "terrain": tz}
            print(f"10k cubes: draw_list {dl}; terrain zones {tw}")
            check(len(tw) == 2 and tw_mean <= 0.02 and all((z.get("allocs_per_frame") or 0) == 0 for z in tw),
                  f"10k cubes: terrain and water cost the frame {tw_mean:.4f} ms (<= 0.02: no entity visited), no allocations: {tw}")
            check((dl.get("mean_ms") or 99) <= 1.0 and (dl.get("p95_ms") or 99) <= 1.5, f"10k cubes: the window's draw list within budget (mean {dl.get('mean_ms')} ms <= 1.0, p95 {dl.get('p95_ms')} ms <= 1.5; other processes' load shows here)")
            check(tz.get("indexed") == 0, f"no terrain indexed among 10k cubes: {tz.get('indexed')}")
            for k0 in range(0, n, 1000):
                mcp.json("world_batch", ops=[{"op": "delete", "path": f"/cubes_{k}"} for k in range(k0, k0 + 1000)])

        # 1. The tiles: 64 model entities of one recipe, each offset to its place.
        ops = [{"op": "spawn", "shape": "group", "path": "/terrain"}]
        half = TILES // 2
        for i in range(-half, half):
            for j in range(-half, half):
                ops.append({"op": "spawn", "shape": "model", "path": f"/terrain/t_{i}_{j}".replace("-", "m"), "recipe": "recipes/terrain_tile.json",
                            "params": {"tx": i, "tz": j}, "position": [i * TILE_M, 0, j * TILE_M]})
        t0 = time.time()
        res = mcp.json("world_batch", ops=ops)
        check(res["applied"] == len(ops), f"{TILES * TILES} tiles spawned ({res['applied']} ops)")
        centre = "/terrain/t_0_0"
        tr = mcp.call("procgen_terrain", recipe="recipes/terrain_tile.json", params={"tx": 0, "tz": 0}, collider={"entity": centre})
        tinfo = json.loads(next(c["text"] for c in tr["content"] if c["type"] == "text"))
        lo, hi = tinfo["height_range_m"]
        check(tinfo.get("collider", {}).get("columns") == TILE_M + 1, f"centre tile's collider: {tinfo.get('collider')} (heights {lo:.1f}..{hi:.1f} m)")
        # The lake fills the valleys: a third of the way up the centre tile's range.
        lake_y = lo + 0.3 * (hi - lo)
        mcp.json("world_spawn", shape="group", path="/lake", position=[-64, lake_y, -64], water={"preset": "lake", "size": [TILES * TILE_M, TILES * TILE_M]})

        def capture(tag, **view):
            p = mcp.frame(width=1600, height=900, time_s=2.0, max_subjects=200, **view)
            summary["captures"][tag] = p["image"]["path"]
            summary["packets"][tag] = p["image"]["packet_path"]
            perf = mcp.json("perf_frame")
            off = perf.get("offscreen") or {}
            g = off.get("gpu") or {}
            passes = g.get("passes") or {}
            summary["perf"][tag] = {"gpu_ms": g.get("total_ms"), "passes": passes, "draws": off.get("draws"), "terrain": perf.get("terrain")}
            d = off.get("draws") or {}
            print(f"capture {tag}: {p['image']['path']}  gpu {g.get('total_ms')} ms {passes}  patches {d.get('terrain_patches')} "
                  f"tris {d.get('terrain_triangles')} water {d.get('water_draws')}")
            return p

        # Wait until every tile draws as terrain (builds land on two workers). Meanwhile (the
        # fallback phase: stand-in meshes, the terrain pipelines building) a packet may count the
        # lake's pixels only in a frame that drew it.
        deadline = time.time() + 240
        drawn = None
        ghost = []
        while time.time() < deadline:
            p = capture("warmup", eye=[-700, 380, -700], target=[0, 0, 0], fov_y_deg=55)
            t = (summary["perf"]["warmup"].get("terrain") or {})
            lake_px = sum((s.get("visible_px") or 0) for s in p.get("subjects", []) if s.get("path") == "/lake")
            if lake_px > 0 and not t.get("water"):
                ghost.append(lake_px)
            drawn = t.get("terrains")
            if drawn == TILES * TILES and t.get("waiting") == 0:
                break
            time.sleep(1.0)
        summary["build_s"] = round(time.time() - t0, 1)
        check(drawn == TILES * TILES, f"all {TILES * TILES} tiles drawn as terrain after {summary['build_s']} s (last: {drawn})")
        check(not ghost, f"no packet counted lake pixels in a frame without water drawn ({ghost[:4]})")

        over = capture("overview", eye=[-700, 380, -700], target=[0, 0, 0], fov_y_deg=55)
        kinds = {}
        for s in over.get("subjects", []):
            kinds.setdefault(s.get("kind", s.get("shape")), []).append(s)
        terr = kinds.get("terrain", [])
        wat = kinds.get("water", [])
        check(len(terr) >= 20, f"sight lists the tiles as kind terrain ({len(terr)} in the packet)")
        check(any((s.get("visible_px") or 0) > 0 for s in terr), "tiles have visible pixels in the ID buffer")
        check(len(wat) == 1 and (wat[0].get("visible_px") or 0) > 0, f"the lake is kind water and visible: {[(w.get('path'), w.get('visible_px')) for w in wat]}")
        close = capture("close", eye=[-20, lake_y + 14, 70], target=[25, lake_y + 2, 0], fov_y_deg=50)
        capture("slopes", eye=[150, hi + 25, 150], target=[40, lo + 8, 40], fov_y_deg=50)
        # A tile border up close (x = 64 m between t_0_0 and t_1_0): no seam.
        capture("seam", eye=[64, hi + 12, 30], target=[64, lo + 4, -20], fov_y_deg=55)
        # Reproducible.
        again = capture("close_again", eye=[-20, lake_y + 14, 70], target=[25, lake_y + 2, 0], fov_y_deg=50)
        check(sha(close["image"]["path"]) == sha(again["image"]["path"]), "the same view at the same time: the same bytes")

        # 2. Every water preset on the same lake.
        for preset in ("murk", "frozen", "lava", "swamp", "lake"):
            mcp.json("world_set", path="/lake", water={"preset": preset, "size": [TILES * TILE_M, TILES * TILE_M]})
            capture(f"water_{preset}", eye=[-20, lake_y + 14, 70], target=[25, lake_y + 2, 0], fov_y_deg=50)

        # 2b. Moonlit (Sepulcra's night): the views the game will use, the murk pool of map 1.
        mcp.call("world_env", **MOON)
        mcp.json("world_set", path="/lake", water={"preset": "murk", "size": [TILES * TILE_M, TILES * TILE_M]})
        capture("moon_overview", eye=[-700, 380, -700], target=[0, 0, 0], fov_y_deg=55)
        capture("moon_close", eye=[-20, lake_y + 14, 70], target=[25, lake_y + 2, 0], fov_y_deg=50)
        capture("moon_slopes", eye=[150, hi + 25, 150], target=[40, lo + 8, 40], fov_y_deg=50)
        capture("moon_near_ground", eye=[30, hi + 3, 30], target=[60, lo, 60], fov_y_deg=55)
        mcp.call("world_env", **DUSK)
        mcp.json("world_set", path="/lake", water={"preset": "lake", "size": [TILES * TILE_M, TILES * TILE_M]})
        # Waves near, mid and far (no grid or parallel bands at any distance), and the ground
        # under the camera (detail, not blur).
        capture("waves_near", eye=[-260, lake_y + 3, -260], target=[-210, lake_y, -235], fov_y_deg=55)
        capture("waves_mid", eye=[-200, lake_y + 30, -200], target=[-60, lake_y, -60], fov_y_deg=55)
        capture("waves_far", eye=[-500, lake_y + 60, -500], target=[0, lake_y, 0], fov_y_deg=55)
        capture("near_ground", eye=[30, hi + 3, 30], target=[60, lo, 60], fov_y_deg=55)

        # Water never hides a real object: a thin post (0.2 m, 2 m above the surface) standing in
        # a submerged bank about 125 m away (review of 7d4df4e: the waterline's fade reached up to
        # 1.5 m over any floor far away). It must show in the pixels and in the packet.
        mid_view = {"eye": [-200, lake_y + 30, -200], "target": [-60, lake_y, -60], "fov_y_deg": 55}
        bare = mcp.frame(width=1600, height=900, time_s=2.0, max_subjects=8, **mid_view)
        mcp.json("world_spawn", shape="cube", path="/post", position=[-182.4, lake_y + 0.85, -80.6], scale=[0.2, 2.3, 0.2], color="#c8482c")
        post = mcp.frame(width=1600, height=900, time_s=2.0, max_subjects=200, **mid_view)
        mcp.call("world_delete", path="/post")
        ps = next((s_ for s_ in post.get("subjects", []) if s_.get("path") == "/post"), {})
        changed = 0
        try:
            from PIL import Image  # optional: the pixel half of the check
            a_im, b_im = Image.open(bare["image"]["path"]).convert("RGB"), Image.open(post["image"]["path"]).convert("RGB")
            bx, by, bw, bh = [int(v) for v in (ps.get("screen_box_exact") or [0, 0, 1600, 900])[:4]]
            x0, y0, x1, y1 = bx, by, bx + bw, by + bh
            for y in range(max(0, y0 - 2), min(900, y1 + 3)):
                for x in range(max(0, x0 - 2), min(1600, x1 + 3)):
                    if sum(abs(p - q) for p, q in zip(a_im.getpixel((x, y)), b_im.getpixel((x, y)))) > 30:
                        changed += 1
        except ImportError:
            changed = None
        summary["post"] = {"subject": {k: ps.get(k) for k in ("visible_px", "occluded", "screen_box_exact")}, "changed_px": changed, "image": post["image"]["path"]}
        summary["captures"]["post"] = post["image"]["path"]
        check((ps.get("visible_px") or 0) >= 8 and (changed is None or changed >= 8),
              f"a thin post in the water 125 m away stays visible: {ps.get('visible_px')} px in the packet, {changed} px changed in the image")

        # 3. terrain_info on the centre tile.
        info = mcp.json("terrain_info", path=centre)
        summary["terrain_info"] = info
        print("terrain_info:", json.dumps({k: info[k] for k in ("drawn_by", "lod", "residency", "collider") if k in info}))
        check(info.get("drawn_by") == "terrain", f"the centre tile is drawn by the terrain renderer ({info.get('drawn_by')}, {info.get('why')})")
        check(info.get("collider", {}).get("matches") is True and info["collider"].get("same_grid") is True, f"its collider is the drawn surface: {info.get('collider')}")
        check(len(info.get("layers", [])) == 4 and info["layers"][2].get("texture"), "four layers, the rock one textured")

        # 4. Sculpt: a mound at the centre tile, the collider rewritten from the edit.
        # First what must change nothing: a collider target that doesn't exist (twice: a retry
        # must not append the strokes twice), and a recipe another agent claims.
        def recipe_edits():
            v = mcp.json("project_read", path="assets/recipes/terrain_tile.json", json_path="parts")
            return v.get("version"), len(v["value"][0]["generate"].get("edits", []))

        def raw(m, tool, **args):
            r = m.request("tools/call", {"name": tool, "arguments": args})
            t = next((c["text"] for c in r["content"] if c["type"] == "text"), "")
            try:
                return r.get("isError", False), json.loads(t)
            except ValueError:
                return r.get("isError", False), {"text": t}
        v0 = recipe_edits()
        stroke = [{"op": "raise", "at": [0, 0], "radius": 8, "amount": 3}]
        bad = [raw(mcp, "terrain_edit", recipe="recipes/terrain_tile.json", strokes=stroke, collider="/no_such_entity") for _ in range(2)]
        check(all(e and j.get("error") and j.get("message") and j.get("recipe_committed") is False for e, j in bad) and recipe_edits() == v0,
              f"terrain_edit with a missing collider entity: a structured error, nothing committed ({bad[0][1]}; recipe {v0} -> {recipe_edits()})")
        other = engine.mcp("terrain-other")
        other.call("agent_hello", name="other", role="builder")
        other.json("claim_take", path="/project/assets/recipes/terrain_tile.json")
        e, j = raw(mcp, "terrain_edit", recipe="recipes/terrain_tile.json", strokes=stroke, collider=centre)
        check(e and j.get("error") in ("claimed", "claims_overlap") and recipe_edits() == v0, f"terrain_edit on a claimed recipe is refused, nothing changed: {j.get('error')}")
        other.json("claim_release", all=True)
        before_h = mcp.json("physics_raycast", origin=[10, 400, 10], direction=[0, -1, 0], max_distance=800)
        ed = mcp.json("terrain_edit", recipe="recipes/terrain_tile.json", collider=centre,
                      strokes=[{"op": "raise", "at": [10, 10], "radius": 20, "amount": 12}, {"op": "paint", "at": [10, 10], "radius": 10, "layer": 3, "amount": 1}])
        summary["terrain_edit"] = ed
        check(ed.get("added") == 2 and ed.get("collider"), f"terrain_edit: {ed}")
        # Every tile shares the recipe: all 64 rebuild. Wait until the centre tile's new build is
        # drawn (its heights then match the collider written from the edit).
        deadline = time.time() + 240
        info = {}
        while time.time() < deadline:
            capture("after_edit", eye=[-30, lake_y + 40, 80], target=[10, lake_y + 10, 10], fov_y_deg=50)
            t = summary["perf"]["after_edit"].get("terrain") or {}
            info = mcp.json("terrain_info", path=centre)
            if t.get("terrains") == TILES * TILES and t.get("waiting") == 0 and info.get("collider", {}).get("matches"):
                break
            time.sleep(1.0)
        summary["edit_rebuild_s"] = round(240 - (deadline - time.time()), 1)
        # (The capture that saw the rebuild land was taken before it: one more shows the mound.)
        capture("after_edit", eye=[-30, lake_y + 40, 80], target=[10, lake_y + 10, 10], fov_y_deg=50)
        check(info.get("collider", {}).get("matches") is True, f"after the edit the collider is still the drawn surface: {info.get('collider')}")
        time.sleep(0.3)
        after_h = mcp.json("physics_raycast", origin=[10, 400, 10], direction=[0, -1, 0], max_distance=800)

        def hit_y(h):
            x = h.get("hit") or (h.get("hits") or [None])[0]
            return x["point"][1] if x else None
        b, c = hit_y(before_h), hit_y(after_h)
        check(b is not None and c is not None and c - b > 10.0, f"a physics ray lands on the mound: {b} -> {c} m")
        summary["mound"] = {"before": b, "after": c}
        hist = mcp.json("world_history", agent="terrain", limit=1)
        entry = (hist.get("entries") or [{}])[0]
        check(entry.get("id") == ed.get("journal") and ed.get("journal") is not None, f"the edit is one journal entry ({ed.get('journal')}; newest {entry.get('id')})")

        # One undo takes back both the recipe and the collider (review of a89b5eb, probe6).
        mcp.json("world_undo")
        deadline = time.time() + 240
        while time.time() < deadline:
            capture("after_undo", eye=[-30, lake_y + 40, 80], target=[10, lake_y + 10, 10], fov_y_deg=50)
            t = summary["perf"]["after_undo"].get("terrain") or {}
            info = mcp.json("terrain_info", path=centre)
            if t.get("waiting") == 0 and info.get("collider", {}).get("matches") and info.get("height_range") and info["height_range"][1] < hi + 1.0:
                break
            time.sleep(1.0)
        capture("after_undo", eye=[-30, lake_y + 40, 80], target=[10, lake_y + 10, 10], fov_y_deg=50)
        time.sleep(0.3)
        undone_h = hit_y(mcp.json("physics_raycast", origin=[10, 400, 10], direction=[0, -1, 0], max_distance=800))
        check(info.get("collider", {}).get("matches") is True and recipe_edits()[1] == v0[1], f"after one world_undo: the recipe's strokes ({recipe_edits()[1]}) and the collider (matches: {info.get('collider', {}).get('matches')}) are both back")
        check(undone_h is not None and b is not None and abs(undone_h - b) < 0.05, f"the ray lands on the old height again: {b} -> {c} -> {undone_h} m")
        summary["mound"]["undone"] = undone_h

        perf = mcp.json("perf_frame")
        summary["perf_frame_terrain"] = perf.get("terrain")
        if a.window:
            # The window: the same views in real time (streaming budgets, the draw cache, presents).
            summary["window"] = {}
            for tag, eye, target in [("close", [-20, lake_y + 14, 70], [25, lake_y + 2, 0]), ("overview", [-700, 380, -700], [0, 0, 0]),
                                     ("seam", [64, hi + 12, 30], [64, lo + 4, -20])]:
                mcp.call("window_view", eye=eye, target=target, fov_y_deg=55 if tag != "close" else 50)
                time.sleep(2.0)
                cap = mcp.json("perf_capture", frames=240, trace=False, label=f"terrain_{tag}")
                fr, g = cap.get("frame_ms") or {}, cap.get("gpu_ms") or {}
                summary["window"][tag] = {"frame_ms": fr, "gpu_ms": g, "gpu_passes_ms": cap.get("gpu_passes_ms"), "budget_errors": cap.get("budget_errors")}
                print(f"window {tag}: frame p95 {fr.get('p95')} ms, GPU p95 {g.get('p95')} ms, passes {cap.get('gpu_passes_ms')}")
                check((g.get("p95") or 99) < 16.6, f"window {tag}: GPU p95 {g.get('p95')} ms under a 60 Hz frame")
            # A capture far away never evicts what the window draws (review of a89b5eb, probe5:
            # it freed the window's full-resolution sets, and each came back as a 15 ms hitch).
            mcp.call("window_view", eye=[-20, lake_y + 14, 70], target=[25, lake_y + 2, 0], fov_y_deg=50)
            time.sleep(3.0)
            before = mcp.json("perf_frame").get("terrain") or {}
            # Captures from the far corner, from close up there (full sets the window doesn't draw):
            # the first makes them; the window must not free them again at once (review of
            # 7d4df4e, probe_capstall: every far capture re-made them inline, 44-53 ms each).
            # (A corner no earlier view came near: the first capture there needs its sets made.)
            far_eye, far_target = [-480, hi + 20, 480], [-440, lo, 440]
            walls, made = [], []
            for k in range(3):
                t0 = time.perf_counter()
                mcp.frame(width=640, height=360, eye=far_eye, target=far_target, time_s=2.0, max_subjects=4)
                walls.append(round((time.perf_counter() - t0) * 1000, 1))
                made.append((mcp.json("perf_frame").get("terrain") or {}).get("capture_uploads"))
                time.sleep(1.5)
            t0 = time.perf_counter()
            mcp.frame(width=640, height=360, eye=[-20, lake_y + 14, 70], target=[25, lake_y + 2, 0], time_s=2.0, max_subjects=4)
            near_wall = round((time.perf_counter() - t0) * 1000, 1)
            summary["window"]["far_captures_ms"] = {"far": walls, "near": near_wall, "capture_uploads": made}
            print(f"far captures {walls} ms (sets uploaded by captures so far: {made}), a near one {near_wall} ms")
            check(made[0] is not None and made[1] == made[0] and made[2] == made[0], f"repeated far captures don't make their sets again: captures' uploads so far {made} (wall {walls} ms, near {near_wall} ms)")
            # The first of them parked while the worker made its texels: the engine thread only
            # copied them (review of 7d4df4e: captures made them inline, 44-53 ms a call).
            zs = mcp.json("perf_zones", thread="main", min_ms=0, limit=500).get("zones") or []
            req = [z for z in zs if str(z.get("path", "")).startswith(("requests", "parked")) and str(z.get("path", "")).rsplit("/", 1)[-1] in ("terrain", "terrain_wait")]
            worst = max((z.get("max_call_ms") or 0 for z in req), default=0)
            summary["window"]["capture_terrain_zones"] = req
            check(bool(req) and worst <= 10.0, f"captures' terrain work on the engine thread stays small (max {worst} ms: {[(z.get('path'), z.get('max_call_ms')) for z in req]})")
            after = mcp.json("perf_frame").get("terrain") or {}
            cap = mcp.json("perf_capture", frames=120, trace=False, label="terrain_after_far_capture")
            zs = mcp.json("perf_zones", thread="main", min_ms=0, limit=500).get("zones") or []
            tz = next((z for z in zs if z.get("path") == "frame/draw_list"), {})
            summary["window"]["after_far_capture"] = {"before": before, "after": after, "frame_ms": cap.get("frame_ms"), "draw_list": tz}
            check((after.get("gpu_bytes") or 0) >= (before.get("gpu_bytes") or 0), f"a far capture evicts nothing ({before.get('gpu_bytes')} -> {after.get('gpu_bytes')} bytes)")
            check((tz.get("max_call_ms") or 99) < 8.0, f"no re-upload hitch in the window after it (draw list max {tz.get('max_call_ms')} ms)")

        # 5. Shared builds (review of 7d4df4e). One build drawn by two entities, one near and one
        # 1 km away: the far one must not free the set the near one's record uses (a release
        # engine shut down, a debug one panicked). Then a 3 x 3 grid of that build: its
        # neighbour masks used to fight over one full set, re-uploading it every few frames.
        mcp.call("world_delete", path="/terrain")
        mcp.call("world_delete", path="/lake")
        mcp.json("world_spawn", shape="model", path="/pair_a", recipe="recipes/hill.json")
        mcp.json("world_spawn", shape="model", path="/pair_b", recipe="recipes/hill.json", position=[1000, 0, 0])
        pair_view = {"eye": [0, 70, 110], "target": [0, 0, 0]}
        if a.window:
            mcp.call("window_view", **pair_view)
        samples = []
        for k in range(60):
            if a.window:
                time.sleep(0.5)
            else:
                mcp.frame(width=480, height=270, time_s=1.0, max_subjects=4, **pair_view)
            t = mcp.json("perf_frame").get("terrain") or {}
            samples.append({x: t.get(x) for x in ("terrains", "records", "waiting", "uploads")})
            if k >= 8 and t.get("waiting") == 0 and t.get("terrains", 0) >= 1 and all(s_.get("uploads") == 0 for s_ in samples[-4:]):
                break
        ia = mcp.json("terrain_info", path="/pair_a")
        summary["shared_pair"] = {"samples": samples[-6:], "a": ia.get("drawn_by")}
        check(ia.get("drawn_by") == "terrain" and engine.proc.poll() is None, f"one build near and 1 km away: the near one drawn as terrain, the engine alive ({ia.get('drawn_by')}, last {samples[-1]})")
        check(all(s_.get("uploads") == 0 for s_ in samples[-4:]), f"and nothing re-uploads once settled: {samples[-4:]}")
        mcp.call("world_delete", path="/pair_a")
        mcp.call("world_delete", path="/pair_b")
        ops = [{"op": "spawn", "shape": "group", "path": "/grid"}]
        ops += [{"op": "spawn", "shape": "model", "path": f"/grid/h{i}_{j}", "recipe": "recipes/hill.json", "position": [i * 128, 0, j * 128]} for i in range(3) for j in range(3)]
        mcp.json("world_batch", ops=ops)
        grid_view = {"eye": [128, 90, 330], "target": [128, 0, 128]}
        if a.window:
            mcp.call("window_view", **grid_view)
        samples = []
        for k in range(60):
            if a.window:
                time.sleep(0.5)
            else:
                mcp.frame(width=480, height=270, time_s=1.0, max_subjects=4, **grid_view)
            t = mcp.json("perf_frame").get("terrain") or {}
            samples.append({x: t.get(x) for x in ("terrains", "records", "waiting", "uploads", "upload_bytes")})
            if k >= 8 and t.get("terrains") == 9 and t.get("waiting") == 0 and all(s_.get("uploads") == 0 for s_ in samples[-8:]):
                break
        summary["shared_grid"] = samples[-8:]
        check(samples[-1].get("terrains") == 9 and all(s_.get("uploads") == 0 for s_ in samples[-8:]), f"3 x 3 tiles of one build: drawn, and uploads stop ({samples[-8:]})")
        mcp.call("world_delete", path="/grid")

        # 6. Many terrain entities: 300 tiles of one landscape (300 builds) are more than a frame's
        # 256 records; those past them keep their stand-in mesh, and none vanishes.
        ops = [{"op": "spawn", "shape": "group", "path": "/g"}]
        ops += [{"op": "spawn", "shape": "model", "path": f"/g/t{i}_{j}", "recipe": "recipes/small_t.json", "params": {"tx": i, "tz": j}, "position": [i * 16 - 160, 0, j * 16 - 120]} for i in range(20) for j in range(15)]
        mcp.json("world_batch", ops=ops)
        deadline = time.time() + 180
        many = None
        while time.time() < deadline:
            many = mcp.frame(width=800, height=600, eye=[0, 300, 260], target=[0, 0, 0], fov_y_deg=60, max_subjects=400, time_s=1.0)
            t = mcp.json("perf_frame").get("terrain") or {}
            if (t.get("terrains") or 0) + (t.get("waiting") or 0) == 300 and t.get("records") == 256:
                break
            time.sleep(1.0)
        subs = [s_ for s_ in many.get("subjects", []) if str(s_.get("path", "")).startswith("/g/")]
        zero = [s_["path"] for s_ in subs if not s_.get("visible_px")]
        summary["many"] = {"terrain": t, "subjects": len(subs), "zero_px": zero[:5], "image": many["image"]["path"]}
        summary["captures"]["many"] = many["image"]["path"]
        check(t.get("records") == 256 and t.get("terrains") == 256 and t.get("waiting") == 44, f"300 tiles: 256 records drawn as terrain, 44 by their stand-ins ({t.get('terrains')} drawn, {t.get('records')} records, {t.get('waiting')} waiting)")
        check(len(subs) == 300 and not zero, f"every one is visible ({len(subs)} in the packet, {len(zero)} with no pixels: {zero[:3]})")
        mcp.call("world_delete", path="/g")

        if not a.window:
            # 7. A small memory budget (headless: no window frames to free by distance): the least
            # recently wanted sets go when a needed one doesn't fit (review of 7d4df4e,
            # probe_budget: residency filled once and never freed, and models starved).
            mcp.call("stream_settings", memory_mb=8)
            ops = [{"op": "spawn", "shape": "group", "path": "/b"}]
            ops += [{"op": "spawn", "shape": "model", "path": f"/b/t{i + 2}_{j + 2}", "recipe": "recipes/big_t.json", "params": {"tx": i, "tz": j}, "position": [i * 256, 0, j * 256]} for i in range(-2, 3) for j in range(-2, 3)]
            mcp.json("world_batch", ops=ops)
            corners = [("A", [-560, 80, -560], [-450, 0, -450]), ("B", [560, 80, 560], [450, 0, 450]), ("A2", [-560, 80, -560], [-450, 0, -450])]
            budget = {}
            for tag, eye, target in corners:
                for k in range(60):
                    mcp.frame(width=480, height=270, eye=eye, target=target, time_s=1.0, max_subjects=8)
                    t = mcp.json("perf_frame").get("terrain") or {}
                    if k > 3 and t.get("waiting") == 0:
                        break
                    time.sleep(0.5)
                budget[tag] = {x: t.get(x) for x in ("terrains", "waiting", "gpu_bytes", "blocked_bytes", "evicted_for_memory")}
            # Settled, the same view frees nothing more (no churn: a set freed only to be made again).
            for k in range(3):
                mcp.frame(width=480, height=270, eye=corners[-1][1], target=corners[-1][2], time_s=1.0, max_subjects=8)
            t = mcp.json("perf_frame").get("terrain") or {}
            budget["A2_settled"] = {x: t.get(x) for x in ("terrains", "waiting", "uploads", "evicted_for_memory")}
            summary["budget"] = budget
            print("budget:", json.dumps(budget))
            check(budget["A2_settled"]["evicted_for_memory"] == budget["A2"]["evicted_for_memory"] and budget["A2_settled"]["uploads"] == 0,
                  f"a settled view under the budget frees and makes nothing more: {budget['A2']} -> {budget['A2_settled']}")
            # (Models stream within the same budget, so how much is left for terrain varies run to
            # run: what must hold is that every view draws and that memory pressure frees old sets.)
            check(all(v.get("waiting") == 0 for v in budget.values()) and (budget["A2"].get("evicted_for_memory") or 0) > 0,
                  f"an 8 MB budget: every view drawn (full sets or coarse copies), old sets freed to make room: {budget}")
            mcp.call("stream_settings", memory_mb=512)
            mcp.call("world_delete", path="/b")
        (out / "terrain.json").write_text(json.dumps(summary, indent=1))
    errors = engine.validation_errors()
    check(errors == 0, f"validation errors at shutdown: {errors}")
    print(f"summary: {out / 'terrain.json'}")
    if failures:
        sys.exit(f"{len(failures)} check(s) failed")
    print("terrain check: all checks passed")


if __name__ == "__main__":
    main()
