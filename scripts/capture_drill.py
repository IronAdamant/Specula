"""Capture throughput drills (docs/design/WORKERS.md, "The capture flight"; from the stalls review's
thru.py and orbit_mem.py probes). Run against two builds, alternating, to compare a change.

- thru: K agents loop small sight_frames (320x180, 100 cubes) and one agent loops a 12-view
  sight_orbit for S seconds; prints sight_frame calls/s, median and p95 latency, and the orbit's
  median time. Headless by default, --window for the windowed loop.
- orbits: N concurrent 12-view sight_orbit calls (700 px tiles) from 8 agents, headless; prints how
  many succeed and the heap peak (perf_memory) while they wait.

    cargo build --release -p specula && python scripts/capture_drill.py thru [--exe PATH] [--port 7860] [--agents 4] [--seconds 10] [--window] [--adapter integrated]
    python scripts/capture_drill.py orbits [--exe PATH] [--port 7860] [--orbits 60] [--adapter integrated]

Results: <target>/capture-drill/<mode>-<label>.json. ASCII output only.
"""
import argparse
import json
import os
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine, Mcp, url_for  # noqa: E402


def thru(a, exe, out):
    args = ["--captures", str(out / "captures"), "--adapter", a.adapter]
    if not a.window:
        args.append("--headless")
    lat, orb, errs = [], [], []
    with Engine(exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as eng:
        m = eng.mcp("drill-setup")
        m.hello("setup")
        m.call("world_batch", ops=[{"op": "spawn", "shape": "cube", "name": f"c{i}", "position": [i % 10, 0, i // 10]} for i in range(100)])
        m.call("sight_frame", width=320, height=180)
        agents = [Mcp(url_for(a.port), f"drill-{k}") for k in range(a.agents)]
        for k, ag in enumerate(agents):
            ag.hello(f"agent{k}")
        end = time.time() + a.seconds

        def loop(ag):
            while time.time() < end:
                t = time.perf_counter()
                try:
                    ag.call("sight_frame", width=320, height=180, eye=[5, 6, 12], target=[5, 0, 5])
                    lat.append(time.perf_counter() - t)
                except Exception as e:  # noqa: BLE001 (counted, not fatal)
                    errs.append(str(e)[:120])

        def orbit():
            o = Mcp(url_for(a.port), "drill-orbit")
            o.hello("orbiter")
            while time.time() < end:
                t = time.perf_counter()
                try:
                    o.call("sight_orbit", point=[5, 0, 5], views=12, tile_width=256, tile_height=256)
                    orb.append(time.perf_counter() - t)
                except Exception as e:  # noqa: BLE001
                    errs.append(str(e)[:120])

        ts = [threading.Thread(target=loop, args=(ag,)) for ag in agents] + [threading.Thread(target=orbit)]
        for t in ts:
            t.start()
        for t in ts:
            t.join()
    lat.sort()
    res = {
        "frames_per_s": round(len(lat) / a.seconds, 1),
        "median_ms": round(lat[len(lat) // 2] * 1000) if lat else None,
        "p95_ms": round(lat[int(len(lat) * 0.95)] * 1000) if lat else None,
        "orbits": len(orb),
        "orbit_median_s": round(sorted(orb)[len(orb) // 2], 3) if orb else None,
        "errors": len(errs),
        "first_errors": errs[:2],
    }
    print(f"sight_frame: {res['frames_per_s']}/s, median {res['median_ms']} ms, p95 {res['p95_ms']} ms; sight_orbit(12): {res['orbits']} done, median {res['orbit_median_s']} s; errors {res['errors']}")
    return res


def orbits(a, exe, out):
    res = []
    samples = []
    with Engine(exe, a.port, out / "engine.log", "--headless", "--adapter", a.adapter, "--captures", str(out / "captures"), cwd=str(ROOT)) as eng:
        agents = []
        for k in range(8):
            ag = Mcp(url_for(a.port), f"drill-orb{k}")
            ag.hello(f"orbiter{k}")
            agents.append(ag)
        agents[0].call("world_batch", ops=[{"op": "spawn", "shape": "cube", "name": f"c{i}", "position": [i % 10, 0, i // 10]} for i in range(100)])

        def fire(i):
            ag = agents[i % 8]
            r = ag.request("tools/call", {"name": "sight_orbit", "arguments": {"point": [5, 0, 5], "views": 12, "tile_width": 700, "tile_height": 700}})
            res.append("error" if r.get("isError") else "ok")

        ts = [threading.Thread(target=fire, args=(i,)) for i in range(a.orbits)]
        for t in ts:
            t.start()
        mon = Mcp(url_for(a.port), "drill-mon")
        mon.hello("monitor")
        while any(t.is_alive() for t in ts):
            try:
                samples.append(mon.json("perf_memory"))
            except Exception:  # noqa: BLE001 (a sample missed)
                pass
            time.sleep(0.5)
        for t in ts:
            t.join()
    peak = max((s.get("peak_mb", 0) for s in samples), default=None)
    out_ = {"ok": res.count("ok"), "errors": res.count("error"), "peak_mb": peak}
    print(f"sight_orbit x{a.orbits}: {out_['ok']} ok, {out_['errors']} errors; heap peak {peak} MB")
    return out_


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["thru", "orbits"])
    ap.add_argument("--exe", default=None)
    ap.add_argument("--port", type=int, default=7860)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--agents", type=int, default=4)
    ap.add_argument("--seconds", type=float, default=10)
    ap.add_argument("--orbits", type=int, default=60)
    ap.add_argument("--window", action="store_true")
    ap.add_argument("--label", default="run")
    a = ap.parse_args()
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    exe = Path(a.exe) if a.exe else target / "release" / ("specula.exe" if sys.platform == "win32" else "specula")
    out = target / "capture-drill" / f"{a.mode}-{a.label}"
    out.mkdir(parents=True, exist_ok=True)
    res = thru(a, exe, out) if a.mode == "thru" else orbits(a, exe, out)
    (out.parent / f"{a.mode}-{a.label}.json").write_text(json.dumps(res, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
