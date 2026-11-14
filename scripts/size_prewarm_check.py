"""New-size capture check: the first capture at a size it never rendered must not stall the engine.

A capture's render graph images (depth, HDR, the bloom chain, SSAO, the ID buffers) belong to its
size. They used to be made by the first frame of a size, inside its submit: 36-67 ms on the
engine thread at 2048 x 2048 on the Intel iGPU (docs/design/WORKERS.md). The engine now makes the
target in a pass of its own, then prepares the graph's images for the view's own frame shape a
slice (a few ms) per pass, and only then renders.

What it does: a lit scene with shadows, bloom, SSAO and FXAA (the most images a frame has), one
warm-up capture, then sight_frame at a series of sizes never seen before while perf_capture
records the engine thread. It reads every frame's `gpu.prepare_graph` and `gpu.prewarm` zones and
the engine.stall incidents, and fails when:
  - a capture's own `gpu.prepare_graph` made images (over PREPARE_MAX_MS: the prewarm missed);
  - a prewarm slice went past SLICE_MAX_MS;
  - any engine.stall incident names a capture;
  - the engine logged validation messages.

Usage: cargo build -p specula && python scripts/size_prewarm_check.py [--port 7550]
       [--adapter integrated] [--exe PATH] [--headless] [--release]
Results: <target>/size-prewarm-check/<adapter>/report.json. ASCII output only.
"""
import argparse
import json
import os
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

TARGET = Path(os.environ["CARGO_TARGET_DIR"]) if os.environ.get("CARGO_TARGET_DIR") else ROOT / "target"
SIZES = [(2048, 2048), (1920, 1200), (1600, 1000), (1366, 768), (1024, 1024), (777, 555), (2000, 1100)]
# A warm graph's prepare (compile check, pool lookups) is well under a millisecond; making a
# 2048 x 2048 frame's images took 36-67 ms.
PREPARE_MAX_MS = 6.0
SLICE_MAX_MS = 15.0


def zones_in_capture(path, names):
    """Per zone name: (calls, worst call ms, every call's ms) over a perf_capture file."""
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    out = {n: [0, 0.0, []] for n in names}
    for f in data["frames"]:
        for rows in f["zones"].values():
            for row in rows:  # [path, calls, ms, self_ms, max_call_ms, ...]
                leaf = row[0].split("/")[-1]
                if leaf in out:
                    o = out[leaf]
                    o[0] += row[1]
                    o[1] = max(o[1], row[4])
                    o[2].append(round(row[4], 2))
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7550)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--exe", default=None)
    ap.add_argument("--release", action="store_true")
    ap.add_argument("--headless", action="store_true")
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    exe = Path(args.exe) if args.exe else TARGET / ("release" if args.release else "debug") / ("specula.exe" if sys.platform == "win32" else "specula")
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p specula")
    out = Path(args.out) if args.out else TARGET / "size-prewarm-check" / args.adapter
    out.mkdir(parents=True, exist_ok=True)
    extra = ["--adapter", args.adapter, "--captures", str(out / "captures"), "--size", "1280x720"]
    if args.headless:
        extra.append("--headless")
    report = {"adapter": args.adapter, "exe": exe.name, "sizes": [], "failures": []}
    fails = report["failures"]
    with Engine(exe, args.port, out / "engine.log", *extra, cwd=out) as eng:
        mcp = eng.mcp("size-prewarm")
        mcp.hello("size-prewarm", role="builder")
        report["gpu"] = mcp.json("engine_info")["gpu"]["name"]
        print(f"gpu: {report['gpu']}")
        mcp.call("world_env", shadow={"enabled": True, "distance": 60}, bloom={"enabled": True}, ssao={"enabled": True}, fxaa=True,
                 sun_direction=[0.4, 0.6, 0.5], ambient_intensity=1.2)
        mcp.call("world_spawn", shape="cube", name="ground", position=[0, -0.5, 0], scale=[30, 1, 30], color="#3a4150")
        for i in range(24):
            mcp.call("world_spawn", shape="cube", name=f"stone{i}", position=[-6 + (i % 6) * 2.4, 0.5 + (i // 6) * 0.2, -3 + (i // 6) * 2], color="#8c96a3")
        mcp.call("window_view", eye=[0, 6, 12], target=[0, 0.5, 0])
        view = {"eye": [0, 6, 12], "target": [0, 0.5, 0]}
        mcp.frame(width=320, height=180, **view)  # pipelines and the 320 x 180 graph are warm
        time.sleep(1.0)
        rec = {}

        def record():
            rec["out"] = eng.mcp("size-prewarm-perf").json("perf_capture", frames=600, trace=False, label="sizes")

        def heap():
            """The image heap's counters (None on builds before it existed)."""
            p = mcp.json("perf_frame")
            return (((p.get("window") or p.get("offscreen") or {}).get("memory") or {}).get("image_heap")) or None

        heap_before = heap()
        t = threading.Thread(target=record)
        t.start()
        time.sleep(0.3)
        for (w, h) in SIZES:
            t0 = time.perf_counter()
            pkt = mcp.frame(width=w, height=h, **view)
            ms = (time.perf_counter() - t0) * 1000
            gpu = (pkt.get("perf") or {}) if isinstance(pkt, dict) else {}
            report["sizes"].append({"size": [w, h], "call_ms": round(ms, 1), "prepare_ms": gpu.get("prepare_ms")})
            print(f"  {w}x{h}: call {ms:.1f} ms")
        # Let the recording cover the last capture's passes, then stop it early.
        t.join(timeout=60)
        # Every image of the new sizes came from the image heap's blocks, grown ahead on its
        # worker: none was allocated on the engine thread (M5 gpuwork).
        heap_after = heap()
        report["image_heap"] = {"before": heap_before, "after": heap_after}
        if heap_before and heap_after:
            sync = heap_after["sync_blocks"] - heap_before["sync_blocks"]
            print(f"image heap: {heap_after['blocks']} blocks, {heap_after['grown_ahead'] - heap_before['grown_ahead']} grown ahead, "
                  f"{heap_after['deferred'] - heap_before['deferred']} deferred, {heap_after['growth_waits'] - heap_before['growth_waits']} growth waits, {sync} on the engine thread")
            if sync > 0:
                fails.append(f"{sync} image heap block(s) were allocated on the engine thread during the new sizes")
        else:
            print("image heap: not reported by this build")
        cap = rec.get("out") or {}
        if not cap.get("path"):
            fails.append(f"perf_capture gave no file: {str(cap)[:200]}")
        else:
            z = zones_in_capture(cap["path"], ["gpu.prepare_graph", "gpu.prewarm"])
            prep, warm = z["gpu.prepare_graph"], z["gpu.prewarm"]
            report["prepare_graph"] = {"calls": prep[0], "worst_ms": round(prep[1], 2), "calls_ms": prep[2][:64]}
            report["prewarm"] = {"calls": warm[0], "worst_ms": round(warm[1], 2), "calls_ms": warm[2][:64]}
            print(f"gpu.prepare_graph: {prep[0]} calls, worst {prep[1]:.2f} ms; gpu.prewarm: {warm[0]} slices, worst {warm[1]:.2f} ms")
            if prep[1] > PREPARE_MAX_MS:
                fails.append(f"a capture's own graph preparation took {prep[1]:.1f} ms (at most {PREPARE_MAX_MS}): its images were not prepared ahead")
            if warm[0] == 0:
                fails.append("no gpu.prewarm slice ran: new sizes were not prepared ahead")
            if warm[1] > SLICE_MAX_MS:
                fails.append(f"a prewarm slice took {warm[1]:.1f} ms (at most {SLICE_MAX_MS})")
        inc = mcp.json("trace_incidents", code="engine.stall", limit=50).get("incidents", [])
        stalls = []
        for s in inc:
            d = mcp.json("trace_incident", id=s["id"])
            stalls.append({"summary": s.get("summary"), "peak_ms": d.get("peak_ms"), "zone": (d.get("evidence") or {}).get("zone")})
        report["stalls"] = stalls
        for s in stalls:
            print(f"  stall: {s['summary']} (peak {s['peak_ms']} ms, zone {s['zone']})")
            fails.append(f"engine stall: {s['summary']} (peak {s['peak_ms']} ms, zone {s['zone']})")
    errors = eng.validation_errors()
    report["validation_errors"] = errors
    if errors:
        fails.append(f"validation errors: {errors}")
    (out / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    print(f"report: {out / 'report.json'}")
    if fails:
        for f in fails:
            print("  FAIL " + f)
        raise SystemExit("FAILED")
    print("PASSED: new sizes are prepared ahead, no capture stalls")


if __name__ == "__main__":
    main()
