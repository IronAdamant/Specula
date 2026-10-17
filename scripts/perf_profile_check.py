"""Live check of the profiling tools: perf.toml budgets, perf_zones, perf_memory, perf_capture.

Runs the release engine (windowed by default, so window frames, vsync pacing and present are in the
numbers) with perf.example.toml, builds the lava-lamp scene, lets it render, then:
  - perf_frame: budgets source/adapter and the structured budget errors
  - perf_zones: the engine thread's zone tree (latest frame + rolling p95)
  - perf_memory: heap, allocations per frame, top allocating zones
  - perf_capture: N frames to JSON + Chrome trace; both files are re-read and checked
  - overhead: sight_frame time with the profiler on vs off (alternating rounds)

Usage:
  python scripts/perf_profile_check.py [--adapter integrated] [--port 7496] [--headless] [--frames 120] [--skip-build]
Results go to target/perf-profile-check/<timestamp>/ (summary.json, engine log, capture files).
Exit code 1 if a tool fails or a check does not hold. Output is ASCII.
"""
import argparse
import datetime
import json
import statistics
import subprocess
import sys
import time
from pathlib import Path

from specula_mcp import ROOT, Engine
import lava_lamps

DEFAULT_PORT = 7496  # never 7471, the engine you work in


def timed(m, shot):
    t = time.perf_counter()
    m.frame(max_subjects=1, **shot)
    return (time.perf_counter() - t) * 1000.0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--port", type=int, default=DEFAULT_PORT, help="the engine's MCP port for this check")
    ap.add_argument("--headless", action="store_true")
    ap.add_argument("--frames", type=int, default=120, help="frames for perf_capture")
    ap.add_argument("--rounds", type=int, default=6, help="on/off rounds for the overhead check")
    ap.add_argument("--skip-build", action="store_true")
    ap.add_argument("--perf", type=Path, default=ROOT / "perf.example.toml")
    args = ap.parse_args()

    if not args.skip_build:
        subprocess.run(["cargo", "build", "-p", "specula", "--release", "-q"], cwd=ROOT, check=True)
    exe = ROOT / "target" / "release" / ("specula.exe" if sys.platform == "win32" else "specula")
    out = ROOT / "target" / "perf-profile-check" / datetime.datetime.now().strftime("%Y-%m-%dT%H-%M-%S")
    out.mkdir(parents=True, exist_ok=True)
    engine_args = ["--adapter", args.adapter, "--perf", str(args.perf), "--captures", str(out / "captures"),
                   "--size", "1280x720"] + (["--headless"] if args.headless else [])
    failures, summary = [], {"adapter": args.adapter, "headless": args.headless}

    def check(cond, what):
        print(("  ok    " if cond else "  FAIL  ") + what)
        if not cond:
            failures.append(what)

    with Engine(exe, args.port, out / "engine.log", *engine_args) as eng:
        m = eng.mcp("perf-profile-check")
        tools = m.tools()
        for t in ("perf_frame", "perf_zones", "perf_memory", "perf_capture"):
            check(t in tools, f"tool {t} is listed")
        lava_lamps.build(m)
        shot = dict(lava_lamps.SHOTS[0][1])
        m.call("window_view", eye=shot["eye"], target=shot["target"], fov_y_deg=shot["fov_y_deg"])
        timed(m, dict(shot, width=1280, height=720))  # one offscreen frame too
        time.sleep(3.0)

        pf = m.json("perf_frame")
        summary["perf_frame"] = {k: pf.get(k) for k in ("budgets", "errors", "cpu", "heap_mb", "frame_pacing")}
        print(f"perf_frame: budgets from {pf['budgets']['source']} ({pf['budgets']['adapter']}), "
              f"{len(pf['errors'])} budget errors, heap {pf.get('heap_mb')} MB, cpu {pf.get('cpu')}")
        for e in pf["errors"]:
            print(f"    {e.get('error')}: {e.get('metric')} {e.get('value')} > {e.get('budget')} {e.get('unit')}")
        check(pf["budgets"]["source"] is not None and pf["budgets"]["source"].endswith("perf.example.toml"),
              "budgets come from perf.example.toml")
        check(all({"error", "metric", "value", "budget", "unit", "adapter"} <= set(e) for e in pf["errors"]),
              "every budget error is structured")
        check(pf.get("heap_mb") is not None, "the engine binary tracks its heap")

        zones = m.json("perf_zones", thread="main", limit=100)
        summary["perf_zones"] = zones
        paths = [z["path"] for z in zones["zones"]]
        print(f"perf_zones: {zones['total']} zones on main, history {zones['history_frames']} frames")
        for z in zones["zones"]:
            print(f"    {z['path']:28s} ms {z['ms']:>8} self {z['self_ms']:>8} p95 {z['p95_ms']:>8} "
                  f"allocs/frame {z['allocs_per_frame']}")
        # With a window open, agents' calls are served between frames (their own top-level zone).
        want = ["frame", "frame/sim"] + (["frame/requests"] if args.headless else ["requests", "frame/draw_list", "frame/present"])
        check(all(p in paths for p in want), f"zones include {want}")
        page = m.json("perf_zones", limit=2)
        check(len(page["zones"]) == 2 and page["next_cursor"] == "2", "perf_zones pages")

        mem = m.json("perf_memory", limit=5)
        summary["perf_memory"] = mem
        print(f"perf_memory: heap {mem.get('heap_mb')} MB, peak {mem.get('peak_mb')} MB, per frame "
              f"{mem.get('per_frame')}, top {[(z['path'], z['allocs_per_frame']) for z in mem.get('top_zones', [])]}")
        check(mem.get("tracking") is True, "perf_memory is tracking")

        t = time.perf_counter()
        cap = m.json("perf_capture", frames=args.frames, label="profile-check")
        cap_s = time.perf_counter() - t
        summary["perf_capture"] = cap
        print(f"perf_capture: {cap['frames']} frames in {cap_s:.2f} s, frame_ms {cap['frame_ms']}, gpu_ms "
              f"{cap['gpu_ms']}, {cap['trace_events']} trace events, {len(cap['budget_errors'])} budget errors")
        for z in cap["top_zones_by_self_ms"][:6]:
            print(f"    {z['thread']}:{z['path']:24s} self {z['self_mean_ms']} ms p95 {z['p95_ms']}")
        check(cap["frames"] == args.frames and cap["stopped_early"] is None, "the capture recorded every frame")
        data = json.loads(Path(cap["path"]).read_text())
        check(data["schema"] == "specula.perf_capture/v1" and len(data["frames"]) == cap["frames"], "capture file")
        trace = json.loads(Path(cap["trace_path"]).read_text())
        names = {e.get("name") for e in trace["traceEvents"]}
        check({"frame", "thread_name"} <= names and len(trace["traceEvents"]) == cap["trace_events"],
              "the trace file has zones and thread names")
        if not args.headless:
            check("scene" in names, "the trace has GPU passes")

        # Overhead: offscreen captures with the profiler off and on, alternating.
        small = dict(shot, width=640, height=360)
        on, off = [], []
        for _ in range(args.rounds):
            m.call("perf_zones", enabled=False)
            off += [timed(m, small) for _ in range(5)]
            m.call("perf_zones", enabled=True)
            on += [timed(m, small) for _ in range(5)]
        summary["overhead_sight_frame_ms"] = {"off_median": statistics.median(off), "on_median": statistics.median(on)}
        print(f"overhead: sight_frame median off {statistics.median(off):.2f} ms, on {statistics.median(on):.2f} ms")
        main_zones = sum(z["calls"] for z in m.json("perf_zones", thread="main", limit=500)["zones"])
        summary["zone_calls_per_frame"] = main_zones
        print(f"zone calls in the latest frame (engine thread): {main_zones}")
    summary["validation_errors"] = eng.validation_errors()
    summary["clean_exit"] = eng.exit_code == 0
    check(eng.validation_errors() == 0, "no validation errors")
    check(eng.exit_code == 0, "clean shutdown")
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"saved {out / 'summary.json'}")
    if failures:
        print(f"{len(failures)} check(s) failed")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
