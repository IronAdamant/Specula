"""Performance baseline: measure the engine and the agent iteration loop, save it, compare runs.

Measures, on every GPU (release build, headless engine on its own port):
  - start-up time until MCP answers, MCP round trip, spawn cost, batched spawn when available
  - sight_frame capture time (render + readback + PNG) for the lava-lamp scene at 720p and 1080p
  - GPU frame statistics from perf_frame, when the engine has it
  - engine memory (working set) and validation errors at shutdown
And the loop agents live in: clean and incremental debug builds, and the workspace test run.

Usage:
  python scripts/perf_baseline.py                      # full run, saved to perf/baselines/
  python scripts/perf_baseline.py --skip-build          # engine numbers only
  python scripts/perf_baseline.py --compare perf/baselines/<file>.json
Results are JSON (schema specula.baseline/v1); the console summary is ASCII.
"""
import argparse
import ctypes
import datetime
import json
import platform
import re
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

from specula_mcp import ROOT, Engine
import lava_lamps

PORT = 7495
ADAPTERS = {"discrete": "discrete", "integrated": "integrated"}


def run_timed(cmd, env=None, cwd=ROOT):
    start = time.perf_counter()
    proc = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, errors="replace")
    return time.perf_counter() - start, proc


def stats_ms(samples_s):
    ms = sorted(s * 1000.0 for s in samples_s)
    p95 = ms[min(len(ms) - 1, int(round(0.95 * (len(ms) - 1))))]
    return {"median": round(statistics.median(ms), 3), "p95": round(p95, 3), "n": len(ms)}


def git_state():
    commit = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    dirty = bool(subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True).stdout.strip())
    return {"commit": commit, "dirty": dirty}


def machine():
    cpu = platform.processor()
    if sys.platform == "win32":
        out = subprocess.run(["powershell", "-NoProfile", "-Command",
                              "(Get-CimInstance Win32_Processor).Name; (Get-CimInstance Win32_VideoController).Name"],
                             capture_output=True, text=True).stdout.splitlines()
        lines = [l.strip() for l in out if l.strip()]
        if lines:
            return {"cpu": lines[0], "gpus": lines[1:], "os": platform.platform()}
    return {"cpu": cpu, "gpus": [], "os": platform.platform()}


def working_set_mb(pid):
    if sys.platform != "win32":
        return None

    class Counters(ctypes.Structure):
        _fields_ = [("cb", ctypes.c_uint32), ("PageFaultCount", ctypes.c_uint32)] + [
            (n, ctypes.c_size_t) for n in ("PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                                           "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage",
                                           "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage")]
    k32 = ctypes.WinDLL("kernel32")
    handle = k32.OpenProcess(0x0400 | 0x0010, False, pid)  # QUERY_INFORMATION | VM_READ
    if not handle:
        return None
    c = Counters()
    c.cb = ctypes.sizeof(c)
    ok = k32.K32GetProcessMemoryInfo(handle, ctypes.byref(c), c.cb)
    k32.CloseHandle(handle)
    return round(c.WorkingSetSize / 2**20, 1) if ok else None


def cpu_busy_pct(seconds=1.0):
    """Whole-machine CPU use over `seconds` (Windows), so a noisy baseline is flagged, not trusted."""
    if sys.platform != "win32":
        return None
    k32 = ctypes.WinDLL("kernel32")

    def times():
        idle, kernel, user = (ctypes.c_uint64(), ctypes.c_uint64(), ctypes.c_uint64())
        k32.GetSystemTimes(ctypes.byref(idle), ctypes.byref(kernel), ctypes.byref(user))
        return idle.value, kernel.value + user.value  # kernel time includes idle time

    i0, t0 = times()
    time.sleep(seconds)
    i1, t1 = times()
    total = t1 - t0
    return round(100.0 * (1 - (i1 - i0) / total), 1) if total else None


def measure_build(results):
    print("build: clean debug workspace build (separate target dir) ...")
    clean_dir = ROOT / "target" / "baseline-clean"
    shutil.rmtree(clean_dir, ignore_errors=True)
    env = dict(__import__("os").environ, CARGO_TARGET_DIR=str(clean_dir))
    t, proc = run_timed(["cargo", "build", "--workspace", "--all-targets"], env=env)
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr[-2000:])
    results["clean_debug_s"] = round(t, 2)
    # Incremental: touch the engine crate's root, the file agents edit most.
    lib = ROOT / "crates" / "specula" / "src" / "lib.rs"
    lib.touch()
    t, proc = run_timed(["cargo", "build", "--workspace", "--all-targets"], env=env)
    results["incremental_debug_s"] = round(t, 2)
    shutil.rmtree(clean_dir, ignore_errors=True)

    print("build: workspace tests ...")
    run_timed(["cargo", "test", "--workspace", "--no-run"])
    t, proc = run_timed(["cargo", "test", "--workspace"])
    passed = sum(int(n) for n in re.findall(r"test result: \w+\. (\d+) passed", proc.stdout))
    failed = sum(int(n) for n in re.findall(r"(\d+) failed", proc.stdout))
    results.update(test_workspace_s=round(t, 2), tests_passed=passed, tests_failed=failed)

    print("build: release engine ...")
    t, proc = run_timed(["cargo", "build", "-p", "specula", "--release"])
    if proc.returncode != 0:
        raise RuntimeError(proc.stderr[-2000:])
    results["release_engine_build_s"] = round(t, 2)


def measure_engine(exe, adapter, out_dir, quick):
    res = {}
    log = out_dir / f"engine_{adapter}.log"
    with Engine(exe, PORT, log, "--headless", "--adapter", adapter, "--captures", str(out_dir / "captures")) as eng:
        res["startup_s"] = round(eng.startup_s, 3)
        m = eng.mcp("perf-baseline")
        info = m.json("engine_info")
        res["gpu"] = info["gpu"]["name"]
        res["validation"] = info["validation"]
        tools = m.tools()

        rtt = []
        for _ in range(20 if quick else 100):
            t = time.perf_counter()
            m.call("engine_info")
            rtt.append(time.perf_counter() - t)
        res["mcp_round_trip_ms"] = stats_ms(rtt)

        # Spawn cost, one call per entity (what an agent pays without batching).
        n = 100 if quick else 500
        spawn = []
        for i in range(n):
            t = time.perf_counter()
            m.call("world_spawn", shape="cube", name=f"bench_{i}", position=[i % 25, 0, i // 25], scale=0.5)
            spawn.append(time.perf_counter() - t)
        res["spawn_call_ms"] = stats_ms(spawn)
        if "world_batch" in tools:
            ops = [{"op": "spawn", "shape": "cube", "name": f"batch_{i}", "position": [i % 50, 2, i // 50]}
                   for i in range(1000)]
            t = time.perf_counter()
            m.call("world_batch", ops=ops)
            res["batch_spawn_1000_s"] = round(time.perf_counter() - t, 3)
        grid = dict(eye=[12, 18, 40], target=[12, 0, 10], fov_y_deg=50, width=1280, height=720, time_s=1)
        res["capture_grid_720p_ms"] = stats_ms([timed_frame(m, grid) for _ in range(3 if quick else 8)])

        # The lava-lamp scene (it clears the world first).
        lava_lamps.build(m)
        shot = dict(lava_lamps.SHOTS[0][1])
        for name, (w, h) in (("720p", (1280, 720)), ("1080p", (1920, 1080))):
            shot.update(width=w, height=h)
            timed_frame(m, shot)  # warm the target
            res[f"capture_lava_{name}_ms"] = stats_ms([timed_frame(m, shot) for _ in range(3 if quick else 8)])
        if "perf_frame" in tools:
            res["perf_frame_lava"] = m.json("perf_frame")
        res["working_set_mb"] = working_set_mb(eng.proc.pid)
    res["validation_errors"] = eng.validation_errors()
    res["clean_exit"] = eng.exit_code == 0
    return res


def timed_frame(m, shot):
    t = time.perf_counter()
    m.frame(max_subjects=1, **shot)
    return time.perf_counter() - t


def flatten(d, prefix=""):
    for k, v in d.items():
        key = f"{prefix}{k}"
        if isinstance(v, dict) and "median" in v:
            yield key, v["median"]
        elif isinstance(v, dict):
            yield from flatten(v, key + ".")
        elif isinstance(v, (int, float)) and not isinstance(v, bool):
            yield key, v


def compare(old, new, threshold):
    """Prints metric changes; returns the regressions (timings or sizes up by more than threshold)."""
    old_flat, regressions = dict(flatten(old)), []
    print(f"\ncompare with {old.get('label')} ({old.get('git', {}).get('commit')}, {old.get('when')}):")
    for key, value in flatten(new):
        if key not in old_flat or key.startswith(("machine", "git")) or key.endswith(("tests_passed", "_errors")):
            continue
        before = old_flat[key]
        if before == 0:
            continue
        change = (value - before) / before
        lower_is_better = key.endswith(("_s", "_ms", "_mb")) or "ms" in key or "round_trip" in key
        mark = ""
        if lower_is_better and change > threshold:
            mark, _ = "  <-- slower", regressions.append(key)
        elif lower_is_better and change < -threshold:
            mark = "  faster"
        print(f"  {key:55s} {before:>10.3f} -> {value:>10.3f}  ({change:+.0%}){mark}")
    return regressions


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--label", default="manual")
    ap.add_argument("--skip-build", action="store_true", help="skip build/test timings (engine numbers only)")
    ap.add_argument("--quick", action="store_true", help="fewer samples")
    ap.add_argument("--adapters", default="discrete,integrated")
    ap.add_argument("--compare", type=Path, help="a previous baseline JSON to compare against")
    ap.add_argument("--threshold", type=float, default=0.15, help="regression threshold (fraction)")
    ap.add_argument("--out", type=Path, default=ROOT / "perf" / "baselines")
    args = ap.parse_args()

    stamp = datetime.datetime.now().strftime("%Y-%m-%dT%H-%M-%S")
    work = ROOT / "target" / "perf-baseline" / stamp
    work.mkdir(parents=True, exist_ok=True)
    rustc = subprocess.run(["rustc", "-V"], capture_output=True, text=True).stdout.strip()
    result = {"schema": "specula.baseline/v1", "when": stamp, "label": args.label, "git": git_state(),
              "machine": machine(), "rustc": rustc, "build": {}, "engine": {}}
    busy = cpu_busy_pct()
    result["machine"]["cpu_busy_pct_before"] = busy
    if busy is not None and busy > 20:
        print(f"WARNING: the machine is busy ({busy}% CPU) - timings will be noisy; compare with care")
    if not args.skip_build:
        measure_build(result["build"])
    else:
        subprocess.run(["cargo", "build", "-p", "specula", "--release", "-q"], cwd=ROOT, check=True)
    exe = ROOT / "target" / "release" / ("specula.exe" if sys.platform == "win32" else "specula")
    for name in args.adapters.split(","):
        print(f"engine: {name} ...")
        try:
            result["engine"][name] = measure_engine(exe, ADAPTERS.get(name, name), work, args.quick)
        except Exception as e:  # one GPU failing must not lose the other's numbers
            result["engine"][name] = {"error": str(e)[-500:]}

    result["machine"]["cpu_busy_pct_after"] = cpu_busy_pct()
    args.out.mkdir(parents=True, exist_ok=True)
    path = args.out / f"{stamp}-{args.label}.json"
    path.write_text(json.dumps(result, indent=2) + "\n")
    print(f"\nsaved {path}")
    for key, value in flatten({"build": result["build"], "engine": result["engine"]}):
        print(f"  {key:55s} {value}")
    status = 0
    for name, eng in result["engine"].items():
        if "error" in eng:
            print(f"ERROR {name}: {eng['error']}")
            status = 1
        elif eng.get("validation_errors") or not eng.get("clean_exit"):
            print(f"ERROR {name}: validation_errors={eng.get('validation_errors')} clean_exit={eng.get('clean_exit')}")
            status = 1
    if args.compare:
        regressions = compare(json.loads(args.compare.read_text()), result, args.threshold)
        if regressions:
            print(f"{len(regressions)} metric(s) regressed by more than {args.threshold:.0%}")
            status = status or 2
    return status


if __name__ == "__main__":
    sys.exit(main())
