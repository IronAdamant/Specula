"""Window frames while agents call the heavy tools (docs/design/WORKERS.md): Sepulcra plays in its
window while one agent calls the class-C tools (2048x2048 sight_frame, sight_orbit, the largest
anim_filmstrip, anim_check on every clip, world_save, world_hash {verify}, lease_release {delete} of
a 20,000-file scratch folder, project_write) and a poller samples perf_frame's window frame pacing
(the rolling frame times: max and p95) once a second. Run it against two builds, alternating, to
compare a change.

    cargo build --release -p sepulcra-app && python scripts/stall_load.py [--exe PATH] [--port 7812] [--adapter integrated] [--seconds 60] [--label after]

Results: <target>/stall-load/<label>.json (per-second samples, the worst frame, per-tool call times)
and captures. ASCII output only.
"""
import argparse
import json
import os
import shutil
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402


def call_raw(mcp, tool, args):
    t = time.perf_counter()
    result = mcp.request("tools/call", {"name": tool, "arguments": args})
    took = time.perf_counter() - t
    text = next((c["text"] for c in result["content"] if c["type"] == "text"), "")
    try:
        j = json.loads(text) if text else None
    except json.JSONDecodeError:
        j = None
    return j, bool(result.get("isError")), took


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", default=None)
    ap.add_argument("--port", type=int, default=7812)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--seconds", type=float, default=60)
    ap.add_argument("--label", default="run")
    a = ap.parse_args()
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    exe = Path(a.exe) if a.exe else target / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")
    out = target / "stall-load"
    out.mkdir(parents=True, exist_ok=True)
    work = out / f"work-{a.label}"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    assets = work / "assets"
    shutil.copytree(ROOT / "games" / "sepulcra" / "assets", assets)
    args = ["--adapter", a.adapter, "--lead", "stall_lead", "--size", "1280x720", "--captures", str(work / "captures"), "--worlds", str(work / "worlds"), "--assets", str(assets), "--", "--seed", "7", "--intro", "off", "--title", "off"]
    samples, calls = [], []
    with Engine(exe, a.port, work / "engine.log", *args, cwd=str(ROOT)) as eng:
        lead = eng.mcp("stall-lead")
        lead.json("agent_hello", name="stall_lead", role="lead")
        poll = eng.mcp("stall-poll")
        time.sleep(3)
        lease = lead.json("lease_take", kind="scratch", lease_s=600)
        stop = threading.Event()

        def poller():
            while not stop.is_set():
                try:
                    p = poll.json("perf_frame")
                    fp = p.get("frame_pacing") or {}
                    samples.append({"t": round(time.time(), 2), "max_ms": fp.get("max_ms"), "p95_ms": fp.get("p95_ms"), "median_ms": fp.get("median_ms")})
                except Exception as e:  # noqa: BLE001
                    samples.append({"t": round(time.time(), 2), "error": str(e)[:200]})
                time.sleep(1.0)

        th = threading.Thread(target=poller, daemon=True)
        th.start()
        end = time.time() + a.seconds
        round_no = 0
        while time.time() < end:
            round_no += 1
            scratch = Path(lease["dir"])
            for i in range(20_000):
                sub = scratch / f"d{i // 1000}"
                if i % 1000 == 0:
                    sub.mkdir(parents=True, exist_ok=True)
                (sub / f"f{i}.bin").write_bytes(b"x" * 64)
            heavy = [
                ("sight_frame", {"width": 2048, "height": 2048}),
                ("sight_orbit", {"point": [0, 2, 0], "views": 12, "tile_width": 512, "tile_height": 512}),
                ("anim_filmstrip", {"recipe": "recipes/shambler.json", "frames": 16, "tile": 256}),
                ("anim_check", {"recipe": "recipes/shambler.json"}),
                ("world_save", {"name": f"stall_{a.label}"}),
                ("world_hash", {"verify": True}),
                ("lease_release", {"id": lease["id"], "delete": True}),
                ("project_write", {"path": "assets/recipes/zz_stall_note.json", "text": json.dumps({"round": round_no, "n": "n" * 100_000})}),
            ]
            for tool, args_ in heavy:
                j, err, took = call_raw(lead, tool, args_)
                calls.append({"tool": tool, "s": round(took, 3), "error": (j or {}).get("error") if err else None})
            lease = lead.json("lease_take", kind="scratch", lease_s=600)
        stop.set()
        th.join(timeout=5)
        lead.close()
        poll.close()
    good = [s for s in samples if s.get("max_ms") is not None]
    worst = max((s["max_ms"] for s in good), default=None)
    p95s = sorted(s["p95_ms"] for s in good if s.get("p95_ms") is not None)
    summary = {
        "label": a.label,
        "exe": str(exe),
        "seconds": a.seconds,
        "rounds": round_no,
        "worst_frame_ms": worst,
        "median_of_p95_ms": p95s[len(p95s) // 2] if p95s else None,
        "samples": samples,
        "calls": calls,
    }
    path = out / f"{a.label}.json"
    path.write_text(json.dumps(summary, indent=2))
    by_tool = {}
    for c in calls:
        by_tool.setdefault(c["tool"], []).append(c["s"])
    print(f"{a.label}: worst window frame {worst} ms; median p95 {summary['median_of_p95_ms']} ms over {len(good)} samples, {round_no} rounds")
    for t, v in by_tool.items():
        print(f"  {t}: {len(v)} calls, max {max(v):.2f} s")
    print(f"results: {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
