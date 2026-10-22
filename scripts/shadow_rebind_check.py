"""Shadow-map rebind check: captures with their own shadow settings must not stall the window.

Every frame graph shares one shadow map (`specula-gpu` render graph), and descriptor binding 0
points at it. A frame whose shadow map has another shape (resolution, cascades) than the last
one rebinds it, and the rebind waits for the GPU to go idle (`gpu.shadow_rebind_wait`). The
window and `sight_frame` / `sight_orbit` use the world's shadow settings, so they share a shape;
`anim_filmstrip` renders on its own stage, which once used a fixed 2048 x 1 map whatever the
world had, so with a 4096 world every filmstrip waited for the GPU twice (once for itself, once
when the window's next frame swapped back).

What it does (windowed, default adapter integrated): a moonlit world with shadows at the given
resolutions, a recipe creature, then per resolution several anim_filmstrip and sight_frame calls
while perf_capture records the window's frames. It reports per resolution: the calls' wall
times, the window's frame ms (median, max) and how often `gpu.shadow_rebind_wait` ran and for how
long (perf_zones), and fails if a filmstrip or capture rebound the map.

Usage: cargo build --release -p specula && python scripts/shadow_rebind_check.py [--port 7600]
       [--adapter integrated] [--exe PATH] [--out DIR] [--resolutions 4096,2048]
Needs a GPU (Vulkan) and a desktop session.
"""
import argparse
import json
import os
import statistics
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

TARGET = Path(os.environ["CARGO_TARGET_DIR"]) if os.environ.get("CARGO_TARGET_DIR") else ROOT / "target"
EXE = TARGET / "release" / ("specula.exe" if sys.platform == "win32" else "specula")
ZONE = "gpu.shadow_rebind_wait"


def zone_in_capture(path):
    """(calls, total ms, worst call ms, frames with it) of ZONE over a perf_capture file."""
    data = json.loads(Path(path).read_text())
    calls, total, worst, frames = 0, 0.0, 0.0, 0
    for f in data["frames"]:
        hit = False
        for rows in f["zones"].values():
            for row in rows:  # [path, calls, ms, self_ms, max_call_ms, ...]
                if row[0].split("/")[-1] == ZONE:
                    calls, total, worst, hit = calls + row[1], total + row[2], max(worst, row[4]), True
        frames += hit
    return calls, total, worst, frames


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7600)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--exe", default=str(EXE))
    ap.add_argument("--out", default=None)
    ap.add_argument("--resolutions", default="4096,2048")
    ap.add_argument("--rounds", type=int, default=6)
    args = ap.parse_args()
    exe = Path(args.exe)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build --release -p specula")
    out = Path(args.out) if args.out else TARGET / "shadow-rebind-check" / args.adapter
    out.mkdir(parents=True, exist_ok=True)
    extra = ["--adapter", args.adapter, "--captures", str(out / "captures"), "--assets", str(ROOT / "games" / "sepulcra" / "assets"), "--size", "1280x720"]
    report = {"adapter": args.adapter, "runs": []}
    failures = []
    with Engine(exe, args.port, out / "engine.log", *extra, cwd=out) as eng:
        mcp = eng.mcp("shadow-rebind")
        mcp.hello("shadow-rebind", role="builder")
        report["gpu"] = mcp.json("engine_info")["gpu"]["name"]
        print(f"gpu: {report['gpu']}")
        mcp.call("world_spawn", shape="cube", name="ground", position=[0, -0.5, 0], scale=[30, 1, 30], color="#3a4150")
        for i in range(6):
            mcp.call("world_spawn", shape="cube", name=f"stone{i}", position=[-5 + 2 * i, 0.5, -2], color="#8c96a3")
        mcp.call("world_spawn", shape="model", name="shambler", recipe="recipes/shambler.json", position=[0, 0, 1], anim={"clip": "walk"})
        mcp.call("window_view", eye=[0, 5, 10], target=[0, 0.8, 0])
        for res in [int(r) for r in args.resolutions.split(",")]:
            mcp.call("world_env", shadow={"enabled": True, "distance": 60, "resolution": res, "softness": 1.5},
                     sun_direction=[0.4, 0.6, 0.5], ambient_intensity=1.5)
            time.sleep(1.0)  # the window settles on the new map (one rebind: a settings change)
            capture = {}

            def record():
                capture["out"] = eng.mcp("shadow-rebind-perf").json("perf_capture", frames=600, trace=False, label=f"shadow{res}")

            t = threading.Thread(target=record)
            t.start()
            film_ms, frame_ms = [], []
            time.sleep(0.3)
            for _ in range(args.rounds):
                t0 = time.perf_counter()
                mcp.call("anim_filmstrip", recipe="recipes/shambler.json", rows=[{"clip": "walk"}], frames=4, tile=96)
                film_ms.append((time.perf_counter() - t0) * 1000)
                t0 = time.perf_counter()
                mcp.frame(width=320, height=180)
                frame_ms.append((time.perf_counter() - t0) * 1000)
                time.sleep(0.15)
            t.join()
            cap = capture.get("out", {})
            frames = cap.get("frame_ms") or {}
            calls, total, worst, hit = zone_in_capture(cap["path"])
            run = {
                "resolution": res,
                "filmstrip_ms": [round(x, 1) for x in film_ms],
                "sight_frame_ms": [round(x, 1) for x in frame_ms],
                "rebind_waits": calls,
                "rebind_wait_ms": round(total, 2),
                "rebind_worst_ms": round(worst, 2),
                "frames_with_a_wait": hit,
                "window_frames": cap.get("frames"),
                "window_frame_ms": frames,
                "capture_file": Path(cap["path"]).name,
            }
            report["runs"].append(run)
            print(f"shadow {res}: filmstrip median {statistics.median(film_ms):.1f} ms, sight_frame median {statistics.median(frame_ms):.1f} ms, "
                  f"rebind waits {run['rebind_waits']} ({run['rebind_wait_ms']} ms), window frame ms {json.dumps(frames)}")
            if run["rebind_waits"] > 0:
                failures.append(f"shadow {res}: {run['rebind_waits']} shadow-map rebinds ({run['rebind_wait_ms']} ms of waiting for the GPU) during captures")
    errors = eng.validation_errors()
    report["validation_errors"] = errors
    if errors != 0:
        failures.append(f"validation errors at shutdown: {errors}")
    report["failures"] = failures
    (out / "shadow_rebind_check.json").write_text(json.dumps(report, indent=1))
    print(f"report: {out / 'shadow_rebind_check.json'}")
    if failures:
        raise SystemExit("FAILED: " + "; ".join(failures))
    print("PASSED: captures keep the window's shadow map")


if __name__ == "__main__":
    main()
