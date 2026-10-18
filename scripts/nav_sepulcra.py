#!/usr/bin/env python3
"""Navigation dogfood on Sepulcra's slab: builds a navmesh from the slab the game builds (its
drawn rock, no colliders needed), finds paths from the three gorge spawns to the graveyard and
checks they stay in the fissures (the lanes enemies walk; the map from play_map is the
reference), sends a nav agent down one, and captures the navmesh and paths overlays.

Sepulcra paths its enemies with flow fields over its own cell graph; this shows the engine's
navmesh agrees with that graph on the same slab, built only from what the world draws.

    cargo build -p sepulcra-app
    python scripts/nav_sepulcra.py                     # port 7486, headless
    python scripts/nav_sepulcra.py --adapter integrated --port 7487

Output goes to <target>/nav-sepulcra/ (CARGO_TARGET_DIR when set). Look at the PNG. Standard
library only; output is ASCII.
"""
import argparse
import base64
import json
import math
import os
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

TARGET = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
OUT = TARGET / "nav-sepulcra"


def check(cond, msg, failures):
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        failures.append(msg)
    return cond


def top_face(map_text):
    """The top face's rows (20 strings of 20 cells) from play_map's plain text."""
    rows = []
    for line in map_text.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[0].isdigit() and len(rows) < 20:
            cells = next((p for p in parts[1:] if len(p) == 20 and not set(p) <= {"-", "|", "#", "B"}), None)
            if cells:
                rows.append(cells)
    return rows


def cells_on(a, b, half, cell):
    """Cells the segment a-b passes over (sampled every 0.2 m)."""
    n = max(1, int(math.dist([a[0], a[2]], [b[0], b[2]]) / 0.2))
    out = []
    for i in range(n + 1):
        t = i / n
        x, z = a[0] + (b[0] - a[0]) * t, a[2] + (b[2] - a[2]) * t
        out.append((int((x + half) // cell), int((z + half) // cell)))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7486)
    ap.add_argument("--adapter", default="discrete")
    ap.add_argument("--exe", default=str(TARGET / "debug" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")))
    a = ap.parse_args()
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    failures = []
    args = ["--adapter", a.adapter, "--headless", "--captures", str(OUT / "captures"), "--worlds", str(OUT / "worlds"), "--", "--seed", "7", "--mute"]
    with Engine(a.exe, a.port, OUT / "engine.log", *args, cwd=str(ROOT)) as eng:
        with eng.mcp("nav-sepulcra") as m:
            m.json("time_control", action="pause")
            rows = top_face(m.text("play_map", plain=True))
            check(len(rows) == 20, f"read the 20 x 20 top face from play_map ({len(rows)} rows)", failures)
            slab = m.json("world_query", path_prefix="/slab", fields=["path", "props"], limit=1)["items"][0]
            cell = slab["props"]["cell_m"]
            half = cell * 10
            build = m.json("nav_build", name="slab", sources={"under": "/slab"})
            # The game's ticks build it (tiles_per_tick a tick): wait until it is complete.
            t0 = time.time()
            while True:
                mesh = next(n for n in m.json("nav_info", mesh="slab")["navmeshes"] if n["path"] == "/nav/slab")
                if mesh["ready"] and mesh["dirty_tiles"] == 0:
                    break
                if time.time() - t0 > 60:
                    raise RuntimeError(f"the slab navmesh didn't finish: {mesh}")
                time.sleep(0.1)
            build.update({k: mesh[k] for k in ("polygons", "tiles_built")})
            build["ms"] = round((time.time() - t0) * 1000, 1)
            check(build["polygons"] > 0, f"nav_build from the slab the game draws: {build['polygons']} polygons, {build['tiles_built']} tiles, {build['ms']} ms", failures)
            centre = lambda c, r, y=0.0: [-half + (c + 0.5) * cell, y, -half + (r + 0.5) * cell]  # noqa: E731
            graveyard = centre(9.5, 9.5, 0.12)
            spawns = [(c, r) for r, row in enumerate(rows) for c, ch in enumerate(row) if ch == "S"]
            check(len(spawns) == 3, f"three gorge spawns ({spawns})", failures)
            lanes = set(".jGS")
            paths = []
            for (c, r) in spawns:
                p = m.json("nav_path", mesh="slab", **{"from": centre(c, r), "to": graveyard})
                paths.append(p)
                over = set()
                for x, y in zip(p["corners"], p["corners"][1:]):
                    for (cc, cr) in cells_on(x, y, half, cell):
                        if 0 <= cc < 20 and 0 <= cr < 20 and rows[cr][cc] not in lanes:
                            over.add((cc, cr, rows[cr][cc]))
                check(p["complete"], f"S at top:{c},{r} -> graveyard: {p['status']}, {p['length']} m, {p['corner_count']} corners, {p['ms']} ms", failures)
                check(not over, f"  ... and it stays in the fissures (cells off the lanes: {sorted(over)[:6]})", failures)
                check(all(abs(k[1]) < 0.2 for k in p["corners"]), "  ... at lane height (no fragment tops)", failures)
            # A pilgrim walks the first one.
            (c, r) = spawns[0]
            m.json("world_spawn", shape="cube", name="pilgrim", position=centre(c, r, 0.4), scale=[0.5, 0.8, 0.5], color="#40e0ff", emissive=2,
                   props={"nav_agent": {"mesh": "slab", "speed": 2.5, "goal": graveyard}})
            for _ in range(40):
                m.json("time_control", action="step", ticks=15)
                info = m.json("nav_info", agents=True)["agent_list"]
                me = next(x for x in info if x["path"] == "/pilgrim")
                if me["status"] == "arrived":
                    break
            check(me["status"] == "arrived", f"a nav agent walked from the gorge to the graveyard ({me['status']}, at {me['position']})", failures)
            shot = m.call("sight_frame", overlays=["navmesh", "paths"], ortho={"height_m": 46}, eye=[0, 60, 0.01], target=[0, 0, 0], width=960, height=960)
            text = None
            for part in shot["content"]:
                if part["type"] == "image":
                    (OUT / "slab_top.png").write_bytes(base64.b64decode(part["data"]))
                elif part["type"] == "text":
                    text = part["text"]
            (OUT / "slab_top.json").write_text(json.dumps(json.loads(text), indent=2))
            persp = m.call("sight_frame", overlays=["navmesh", "paths"], eye=[26, 24, 34], target=[0, -2, 2], width=1280, height=720)
            for part in persp["content"]:
                if part["type"] == "image":
                    (OUT / "slab_perspective.png").write_bytes(base64.b64decode(part["data"]))
                elif part["type"] == "text":
                    (OUT / "slab_perspective.json").write_text(json.dumps(json.loads(part["text"]), indent=2))
            state = m.json("play_state")
            check(state["outcome"] == "running", "the game kept running beside it", failures)
            summary = {"build": {k: build[k] for k in ("polygons", "tiles", "tiles_built", "ms")}, "paths": [{k: p[k] for k in ("status", "length", "corner_count", "ms")} for p in paths]}
    check(eng.validation_errors() in (0, None), "clean shutdown", failures)
    summary["failures"] = failures
    (OUT / "summary.json").write_text(json.dumps(summary, indent=2))
    print("PASS" if not failures else f"FAIL ({len(failures)})")
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
