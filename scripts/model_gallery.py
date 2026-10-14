"""Model gallery: every Sepulcra recipe (all tiers) built by the engine and spawned as `model`
entities in a moonlit scene, captured with sight_frame.

What it checks, besides drawing the gallery:
  - model_build reports for every recipe (triangles per LOD, bounds, materials, budget, notes),
  - every model entity is a sight subject, on screen, with its recipe,
  - identical specs share one build (perf_frame models.gpu.builds_uploaded),
  - a `ui`-layer marker is left out of the capture and its subjects unless include_ui,
  - a hidden model is not drawn and not a subject,
  - a broken recipe is refused with a structured error,
  - zero validation errors at shutdown.

Captures (PNG + packet JSON) land in OUT/captures/run-*/; the summary in OUT/gallery.json.
Look at the PNGs before claiming anything about them.

Usage: cargo build -p specula && python scripts/model_gallery.py [--port 7490] [--out DIR] [--adapter X]
Needs a GPU (Vulkan); runs headless.
"""
import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

ASSETS = ROOT / "games" / "sepulcra" / "assets"

# STYLE.md night: low moon from the east-south-east, blue-violet ambient, ACES, bloom, shadows.
ELEV, AZ = math.radians(30), math.radians(60)
NIGHT = {
    "sun_direction": [math.cos(ELEV) * math.sin(AZ), math.sin(ELEV), math.cos(ELEV) * math.cos(AZ)],
    "sun_color": "#c9d6ff",
    "sun_intensity": 1.3,
    "ambient_color": "#2a3448",
    "ambient_intensity": 2.2,
    "exposure": 1.1,
    "tonemap": True,
    "background": "#0b1020",
    "shadow": {"enabled": True, "distance": 160, "resolution": 4096, "softness": 1.5},
    "bloom": {"enabled": True, "threshold": 1.0, "intensity": 0.6},
    "fxaa": True,
    "sky": {"enabled": True, "ground": "#06080e"},
}


def recipes():
    """(label, recipe path, params) for every recipe, every tier of tiered ones."""
    out = []
    for path in sorted((ASSETS / "recipes").glob("*.json")):
        text = path.read_text()
        rel = f"recipes/{path.name}"
        if '"tier"' in text:
            out += [(f"{path.stem} {t}", rel, {"tier": t}) for t in (1, 2, 3)]
        else:
            out.append((path.stem, rel, {}))
    return out


def heavy_recipe():
    """A heavy-but-legal recipe (about 1.5 s to build in a dev build)."""
    ops = [{"op": "subdivide", "levels": 4}, {"op": "subdivide", "levels": 2}]
    ops += [{"op": "displace", "amount": 0.01, "scale": 0.5, "octaves": 6} for _ in range(30)]
    ops += [{"op": "ao", "amount": 0.3} for _ in range(10)]
    part = lambda i: {"name": f"p{i}", "material": "carved_stone", "shape": {"op": "box", "size": [1, 1, 1]}, "ops": ops}
    return {"recipe": 1, "name": "heavy", "lods": [0.5, 0.25], "budget": {"max_triangles": 200000},
            "parts": [part(0), part(1)]}


def shutdown_check(a, out, check):
    """Ctrl+Break while a capture is parked on building models and a model_build is pending:
    both callers get an answer at once and the engine exits promptly."""
    import shutil
    import threading
    import time
    from specula_mcp import Mcp, url_for
    assets = out / "shutdown_assets"
    shutil.rmtree(assets, ignore_errors=True)
    (assets / "recipes").mkdir(parents=True)
    shutil.copy(ASSETS / "style.json", assets / "style.json")
    (assets / "recipes" / "heavy.json").write_text(json.dumps(heavy_recipe()))
    args = ["--headless", "--assets", str(assets), "--captures", str(out / "captures"), "--worlds", str(out / "worlds")]
    answers = {}

    def call(name, tool, **args):
        start = time.perf_counter()
        r = Mcp(url_for(a.port), name).request("tools/call", {"name": tool, "arguments": args})
        text = next(c["text"] for c in r["content"] if c["type"] == "text")
        answers[name] = (time.perf_counter() - start, r.get("isError", False), text[:120])

    with Engine(a.exe, a.port, out / "shutdown.log", *args, cwd=str(ROOT)) as engine:
        mcp = engine.mcp("shutdown-check")
        for s in range(6):
            mcp.call("world_spawn", shape="model", recipe="recipes/heavy.json", seed=s, position=[s * 2, 0, 0])
        threads = [threading.Thread(target=call, args=("capture", "sight_frame"), kwargs={"width": 64, "height": 64}),
                   threading.Thread(target=call, args=("build", "model_build"), kwargs={"recipe": "recipes/heavy.json", "seed": 99})]
        for t in threads:
            t.start()
        time.sleep(0.5)
        stopping = time.perf_counter()
        engine.stop()
        stop_s = time.perf_counter() - stopping
    for t in threads:
        t.join(timeout=60)
    check(stop_s < 1.5, f"shutdown with a parked capture and a pending model_build took {stop_s:.2f} s")
    for name in ("capture", "build"):
        took, is_error, text = answers.get(name, (None, None, "no answer"))
        ok = took is not None and took < 5 and is_error and "shutting down" in text
        check(ok, f"{name} answered at shutdown: {took and round(took, 2)} s, {text!r}")
    check(engine.validation_errors() == 0, f"validation errors at that shutdown: {engine.validation_errors()}")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--port", type=int, default=7490)
    ap.add_argument("--out", default=str(ROOT / "target" / "model-gallery"))
    ap.add_argument("--exe", default=str(ROOT / "target" / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")))
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

    with Engine(a.exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as engine:
        mcp = engine.mcp("model-gallery")
        mcp.call("agent_hello", name="gallery", role="reviewer")
        mcp.call("world_env", **NIGHT)

        # 1. Reports.
        entries = []
        print(f"{'model':<24} {'tris per LOD':<18} {'size (m)':<18} {'build ms':>8}  materials")
        for label, rel, params in recipes():
            r = mcp.json("model_build", recipe=rel, params=params)
            lo, hi = r["bounds"]["min"], r["bounds"]["max"]
            size = [hi[i] - lo[i] for i in range(3)]
            tris = "/".join(str(int(l["triangles"])) for l in r["lods"])
            print(f"{label:<24} {tris:<18} {'%.1f x %.1f x %.1f' % tuple(size):<18} {r['build_ms']:>8.1f}  "
                  f"{', '.join(r['materials'])}")
            for note in r["notes"]:
                print(f"{'':<24} note: {note}")
            entries.append({"label": label, "recipe": rel, "params": params, "report": r, "size": size, "lo": lo})

        # 2. Layout: rows of at most ~60 m, small things first so the giants sit at the back.
        entries.sort(key=lambda e: (e["size"][1] > 6, e["size"][0] * e["size"][2]))
        gap, row_width = 2.0, 60.0
        x, z, row_depth, rows = 0.0, 0.0, 0.0, [[]]
        for e in entries:
            w, d = e["size"][0], e["size"][2]
            if x > 0 and x + w > row_width:
                x, z, row_depth = 0.0, z - row_depth - gap * 2, 0.0
                rows.append([])
            # Centre the model's footprint at (x + w/2, z - d/2), its lowest point on the ground
            # (components such as rune_column hang down from y = 0).
            e["pos"] = [x + w / 2 - (e["lo"][0] + w / 2), max(0.0, -e["lo"][1]), z - d / 2 - (e["lo"][2] + d / 2)]
            rows[-1].append(e)
            x += w + gap
            row_depth = max(row_depth, d)
        depth = -(z - row_depth)
        width = max(sum(e["size"][0] + gap for e in r) for r in rows)
        ops = [{"op": "spawn", "shape": "group", "path": "/gallery"},
               {"op": "spawn", "shape": "quad", "path": "/gallery/ground", "position": [width / 2, 0, -depth / 2],
                "scale": [width + 20, 1, depth + 20], "color": "#5b6470", "roughness": 0.9}]
        for i, e in enumerate(entries):
            name = e["label"].replace(" ", "_")
            ops.append({"op": "spawn", "shape": "model", "path": f"/gallery/{name}", "recipe": e["recipe"],
                        "params": e["params"], "position": e["pos"], "tags": ["gallery"]})
        # Duplicates of one spec, to check build sharing.
        for k in range(3):
            ops.append({"op": "spawn", "shape": "model", "path": f"/gallery/twin_{k}", "recipe": "recipes/tombstone.json",
                        "position": [-4.0 - 2.0 * k, 0, 2.0]})
        # A hidden model and a ui-layer marker: neither may show up in the default capture.
        ops.append({"op": "spawn", "shape": "model", "path": "/gallery/hidden_stone",
                    "recipe": "recipes/warding_stone.json", "position": [-8, 0, -4], "visible": False})
        ops.append({"op": "spawn", "shape": "group", "path": "/ui", "layer": "ui"})
        ops.append({"op": "spawn", "shape": "cube", "path": "/ui/cursor", "position": [width / 2, 0.2, 3],
                    "scale": [3, 0.2, 3], "color": "#3fe0d0", "emissive": 2})
        res = mcp.json("world_batch", ops=ops)
        check(res["applied"] == len(ops), f"world_batch spawned {res['applied']} of {len(ops)}")

        # 3. A broken recipe is refused with a structured error.
        bad = mcp.request("tools/call", {"name": "world_spawn", "arguments": {
            "shape": "model", "recipe": "recipes/warding_stone.json", "params": {"tier": 9}}})
        err = json.loads(bad["content"][0]["text"]) if bad.get("isError") else {}
        check(err.get("error") == "recipe_invalid" and err.get("recipe_error", {}).get("code") == "OutOfRange",
              f"tier 9 refused: {err.get('error')} {err.get('recipe_error', {}).get('code')} at {err.get('recipe_error', {}).get('path')}")

        # 4. Captures: the whole gallery (LOD by size), then full detail from closer.
        centre = [width / 2, 2.0, -depth / 2]
        dist = max(width, depth) * 0.85
        eye = [centre[0] + dist * 0.35, dist * 0.55, centre[2] + dist * 0.9]
        shots = {}
        for tag, extra in [("overview", {}), ("overview_lod0", {"lod": 0}), ("overview_ui", {"include_ui": True})]:
            p = mcp.frame(width=1920, height=1080, eye=eye, target=centre, fov_y_deg=45, time_s=1.0, max_subjects=500, **extra)
            shots[tag] = p
            print(f"capture {tag}: {p['image']['path']}  models {p['models']}")
        front = entries[: len(rows[0])]
        fx = sum(e["pos"][0] for e in front) / len(front)
        close = mcp.frame(width=1920, height=1080, eye=[fx, 3.2, 9.0], target=[fx, 1.3, -2.5], fov_y_deg=50,
                          time_s=1.0, max_subjects=500)
        shots["front_row"] = close
        print(f"capture front_row: {close['image']['path']}  models {close['models']}")

        p = shots["overview"]
        subjects = {s["name"]: s for s in p["subjects"]}
        names = [e["label"].replace(" ", "_") for e in entries]
        missing = [n for n in names if n not in subjects]
        offscreen = [n for n in names if n in subjects and not subjects[n]["on_screen"]]
        check(not missing, f"every gallery model is a subject ({len(names) - len(missing)}/{len(names)}; missing {missing})")
        check(not offscreen, f"every gallery model is on screen (off: {offscreen})")
        check(all(subjects[n].get("recipe", "").startswith("recipes/") for n in names if n in subjects), "subjects carry their recipe")
        check("hidden_stone" not in subjects, "the hidden model is not a subject")
        check("cursor" not in subjects, "the ui marker is left out of the default capture")
        check("cursor" in {s["name"] for s in shots["overview_ui"]["subjects"]}, "include_ui brings the ui marker back")
        check(p["models"]["missing"] == 0, f"no model lacks a build ({p['models']['missing']})")
        check(shots["overview_lod0"]["models"]["lod_histogram"][0] == len(names) + 3,
              f"lod 0 forced: {shots['overview_lod0']['models']['lod_histogram']}")
        check(not p["warnings"], f"no capture warnings {p['warnings']}")

        perf = mcp.json("perf_frame")
        gpu = perf["models"]["gpu"]
        # The three tombstone twins share the gallery tombstone's build; the hidden stone (no
        # params, so its own spec) is one more.
        check(gpu["builds_uploaded"] == len(entries) + 1,
              f"builds uploaded {gpu['builds_uploaded']} for {len(names) + 4} model entities: twins share one")
        print(f"perf: offscreen {perf['offscreen']}")
        summary = {"entries": [{k: e[k] for k in ("label", "recipe", "params", "pos")} | {"report": e["report"]} for e in entries],
                   "captures": {k: v["image"]["path"] for k, v in shots.items()},
                   "packets": {k: v["image"]["packet_path"] for k, v in shots.items()},
                   "models": perf["models"]}
        (out / "gallery.json").write_text(json.dumps(summary, indent=1))
    errors = engine.validation_errors()
    check(errors == 0, f"validation errors at shutdown: {errors}")
    shutdown_check(a, out, check)
    print(f"summary: {out / 'gallery.json'}")
    if failures:
        sys.exit(f"{len(failures)} check(s) failed")
    print("model gallery: all checks passed")


if __name__ == "__main__":
    main()
