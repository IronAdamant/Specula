"""Balance lab check: targets as data, sweeps independent of the thread count, a tune job across
two calls that hits its targets on held-out seeds, all through the engine's tools.

A headless engine (port 7602 by default) gets the Sepulcra slice in Lex (lex_import) and the
targets in games/sepulcra/lex/slice.targets.json. Then:
  1. balance_targets binds the targets to the set (and catches a typo with a suggestion);
  2. balance_sweep scores a grid of runner_hp values, once on 1 thread and once on many: the
     numbers must be identical; reports runs per second;
  3. balance_tune tunes runner_hp as a kept job over two calls (7 evaluations each), then checks
     the best value on the held-out seeds: every target must hit there;
  4. while a sweep runs, another agent's calls still answer quickly (the engine thread isn't
     doing the work).

Usage: cargo build --release -p specula && python scripts/balance_check.py [--port 7602] [--exe PATH] [--out FILE]
ASCII output. Exit code 0 = every check held.
"""
import argparse
import json
import os
import sys
import tempfile
import threading
import time
from pathlib import Path

from specula_mcp import ROOT, Engine

SLICE = ROOT / "games" / "sepulcra" / "lex" / "slice.lex.json"
TARGETS = "games/sepulcra/lex/slice.targets.json"
SET = "sepulcra_slice"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7602)
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    ap.add_argument("--exe", default=str(target / "release" / ("specula.exe" if sys.platform == "win32" else "specula")))
    ap.add_argument("--out", type=Path, help="save the report as JSON here")
    ap.add_argument("--window", action="store_true", help="open the window and measure its frame times during a sweep")
    ap.add_argument("--adapter", default=None, help="discrete | integrated | <index>")
    args = ap.parse_args()
    if args.port == 7471:
        sys.exit("never run on 7471 (the interactive engine's port)")
    problems, report = [], {}

    def check(ok, what):
        print(f"  {'ok  ' if ok else 'FAIL'} {what}")
        if not ok:
            problems.append(what)

    work = Path(tempfile.mkdtemp(prefix="balance_check_"))
    flags = ([] if args.window else ["--headless"]) + (["--adapter", args.adapter] if args.adapter else [])
    with Engine(args.exe, args.port, work / "engine.log", *flags, "--worlds", str(work / "worlds"), "--captures", str(work / "captures"), cwd=str(ROOT)) as eng:
        lab, other = eng.mcp("balance-lab"), eng.mcp("balance-other")
        lab.hello("tuner")
        other.hello("bystander")
        tools = lab.tools()
        check(all(t in tools for t in ("balance_targets", "balance_sweep", "balance_tune")), "the three balance tools are listed")
        out = lab.json("lex_import", set=SET, document=json.loads(SLICE.read_text()))
        check(out["valid"], "the slice is imported")

        bound = lab.json("balance_targets", targets_file=TARGETS, set=SET)
        check(bound["valid"] and any(p["id"] == "runner_hp" for p in bound["params"]), "targets bind to the set; runner_hp is tunable")
        doc = json.loads((ROOT / TARGETS).read_text())
        doc["targets"][2]["archetype"] = "runer"
        res = lab.request("tools/call", {"name": "balance_targets", "arguments": {"targets": doc, "set": SET}})
        err = json.loads(res["content"][0]["text"])
        d = err.get("diagnostics", [{}])[0]
        check(res.get("isError") and d.get("code") == "unknown_ref" and "runner" in (d.get("suggestion") or ""), f"a typo is caught with a suggestion ({d.get('suggestion')})")

        grid = {"runner_hp": [26, 60, 100, 118, 140]}
        one = lab.json("balance_sweep", set=SET, targets_file=TARGETS, grid=grid, threads=1)

        def probe():
            t0 = time.perf_counter()
            other.json("world_list", limit=5)
            return (time.perf_counter() - t0) * 1000

        idle = sorted(probe() for _ in range(20))
        # A bystander's calls while a many-thread sweep runs.
        latencies = []
        done = threading.Event()

        def poke():
            while not done.is_set():
                latencies.append(probe())
                time.sleep(0.01)

        th = threading.Thread(target=poke)
        th.start()
        many = lab.json("balance_sweep", set=SET, targets_file=TARGETS, grid=grid)
        done.set()
        th.join()
        strip = lambda r: [{k: v for k, v in p.items()} for p in r["points"]]
        check(strip(one) == strip(many), f"the same numbers on 1 thread and on {many['threads']}")
        check(one["needs_human_playtest"] is True, "results are flagged needs_human_playtest")
        print(f"  sweep: {many['runs']} runs, 1 thread {one['runs_per_s']} runs/s, {many['threads']} threads {many['runs_per_s']} runs/s")
        lat = sorted(latencies)
        med = lambda v: v[len(v) // 2]
        print(f"  bystander world_list: idle median {med(idle):.1f} ms; during the sweep {len(lat)} calls, median {med(lat):.1f} ms, max {lat[-1]:.1f} ms")
        check(med(lat) < med(idle) * 5 + 25, "other agents' calls still answer during a sweep (the engine thread isn't doing the work)")
        report["sweep"] = {"runs": many["runs"], "runs_per_s_1": one["runs_per_s"], "runs_per_s_many": many["runs_per_s"], "threads": many["threads"],
                           "bystander_ms": {"idle_median": med(idle), "median": med(lat), "max": lat[-1]}}
        for p in many["points"]:
            tomb = next(t for t in p["targets"] if t["id"] == "tombstones_left")
            print(f"    runner_hp {p['params']['runner_hp']}: loss {p['loss']}, tombstones {tomb['mean']} +- {tomb['stdev']}, hit {p['hit']}")

        if args.window:
            # The window while a big sweep runs: frame times before and during (perf_capture).
            capture = lambda: lab.json("perf_capture", frames=120, trace=False)["frame_ms"]
            idle_frames = capture()
            during = {}

            def frames():
                during.update(capture())

            th = threading.Thread(target=frames)
            th.start()
            big = other.json("balance_sweep", set=SET, targets_file=TARGETS, grid={"runner_hp": {"from": 30, "to": 150, "steps": 24}})
            th.join()
            print(f"  window frame ms idle {idle_frames}; during a {big['runs']}-run sweep on {big['threads']} threads {during}")
            report["window"] = {"idle_frame_ms": idle_frames, "sweep_frame_ms": during, "sweep_runs": big["runs"], "sweep_runs_per_s": big["runs_per_s"]}

        tune = [{"param": "runner_hp", "min": 40, "max": 160, "step": 1}]
        a = lab.json("balance_tune", set=SET, targets_file=TARGETS, tune=tune, budget=14, seed=7, job="proof", evals=7)
        check(a["evaluations"] == 7 and not a["done"], "the first call ran 7 evaluations and kept the job")
        b = lab.json("balance_tune", set=SET, targets_file=TARGETS, job="proof")
        check(b["done"] and b["evaluations"] == 14, "the second call finished the job")
        best = b["best"]
        held = b["held_out"]
        print(f"  tuned: {best['params']} train loss {best['loss']} hit {best['hit']}; held-out loss {held and held['loss']} hit {b['held_out_hit']}")
        check(best["hit"], "the best point hits every target on the training seeds")
        check(b["held_out_hit"] is True, "and on the held-out seeds")
        res = other.request("tools/call", {"name": "balance_tune", "arguments": {"set": SET, "targets_file": TARGETS, "job": "proof"}})
        check(res.get("isError") and "job_owned" in res["content"][0]["text"], "another agent can't continue the job")
        report["tune"] = {"best": best, "held_out": held, "seconds": [a["seconds"], b["seconds"]]}
        lab.close()
        other.close()
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(report, indent=2) + "\n")
    for p in problems:
        print(f"  PROBLEM: {p}")
    print("PASS" if not problems else "FAIL")
    sys.exit(0 if not problems else 1)


if __name__ == "__main__":
    main()
