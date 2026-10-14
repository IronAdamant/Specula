"""Model stress (foundation F3): runs the `model_stress` example game - 500 model entities plus
500 transient model enemies it moves every tick and eases every frame - and reports what
perf_frame measures: sim_tick and game_frame cost, draw-list build time, frame pacing, GPU time
per pass, draws and LOD histogram. Also checks the transient enemies leave the world unsaved-free
(no unsaved changes, state hash steady) and the journal quiet.

Usage: cargo build -p specula --example model_stress && python scripts/model_stress.py
       [--port 7492] [--count 500] [--headless] [--seconds 6] [--adapter integrated]
Needs a GPU (Vulkan); opens a window unless --headless (game_frame and window numbers need one).
"""
import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--port", type=int, default=7492)
    ap.add_argument("--count", type=int, default=500)
    ap.add_argument("--seconds", type=float, default=6.0)
    ap.add_argument("--headless", action="store_true")
    ap.add_argument("--adapter", default=None)
    ap.add_argument("--out", default=str(ROOT / "target" / "model-stress"))
    exe = ROOT / "target" / "debug" / "examples" / ("model_stress.exe" if sys.platform == "win32" else "model_stress")
    ap.add_argument("--exe", default=str(exe))
    a = ap.parse_args()
    if a.port == 7471:
        sys.exit("port 7471 belongs to the live engine; pick another")
    out = Path(a.out)
    args = ["--assets", str(ROOT / "games" / "sepulcra" / "assets"), "--captures", str(out / "captures"), "--size", "1600x900"]
    if a.headless:
        args.append("--headless")
    if a.adapter:
        args += ["--adapter", a.adapter]
    args += ["--", "--count", str(a.count)]
    failures = []

    def check(ok, what):
        print(("ok    " if ok else "FAIL  ") + what)
        if not ok:
            failures.append(what)

    with Engine(a.exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as engine:
        mcp = engine.mcp("model-stress")
        info = mcp.json("engine_info")
        status = info["game"]["status"]
        print(f"gpu: {info['gpu']['name']}; spawned {status['stones']} stones + {status['enemies']} transient enemies "
              f"in {status['spawn_ms']:.1f} ms (engine start to MCP: {engine.startup_s:.1f} s)")
        check(status["failures"] == 0 and status["stones"] == a.count and status["enemies"] == a.count, "every spawn worked")
        h0 = mcp.json("world_hash")
        time.sleep(a.seconds)
        perf = mcp.json("perf_frame")
        h1 = mcp.json("world_hash")
        info = mcp.json("engine_info")
        cap = mcp.frame(width=1280, height=720, max_subjects=5)
        print(f"capture: {cap['image']['path']}  models {cap['models']}")

        def series(name):
            s = perf.get(name)
            return "n/a" if not s else f"mean {s['mean_ms']:.3f} ms, p95 {s['p95_ms']:.3f} ms, max {s['max_ms']:.3f} ms ({s['samples']} samples)"

        print(f"sim_tick   ({a.count} transient moves): {series('sim_tick')}")
        print(f"game_frame ({a.count} transient eases): {series('game_frame')}")
        print(f"draw_list  ({2 * a.count} models): {series('draw_list')}")
        print(f"frame pacing: {series('frame_pacing')}")
        for path in ("window", "offscreen"):
            s = perf.get(path)
            if s:
                print(f"{path}: gpu {s['gpu']['total_ms']} ms {s['gpu']['passes']}, cpu {s['cpu']}, draws {s['draws']}")
        print(f"models: {json.dumps(perf['models']['window'])}  gpu {json.dumps(perf['models']['gpu'])}")
        print(f"errors: {perf['errors']}")
        tick = perf.get("sim_tick")
        check(tick is not None and tick["samples"] > 0, "ticks ran")
        check(h0["state_hash"] == h1["state_hash"] and h1["tick"] > h0["tick"],
              f"state hash steady over {h1['tick'] - h0['tick']} ticks of moving enemies")
        check(not info["world"]["unsaved_changes"] or info["world"]["name"] is None, "no unsaved changes from presentation")
        check(info["world"]["transient_entities"] == a.count, f"{info['world']['transient_entities']} transient entities")
        journal = info["swarm"]["journal"]
        check(journal["game_entries"] <= 2, f"journal quiet: {journal['game_entries']} game entries (start only)")
        (out / "perf.json").write_text(json.dumps(perf, indent=1))
    errors = engine.validation_errors()
    check(errors == 0, f"validation errors at shutdown: {errors}")
    if failures:
        sys.exit(f"{len(failures)} check(s) failed")
    print("model stress: all checks passed")


if __name__ == "__main__":
    main()
