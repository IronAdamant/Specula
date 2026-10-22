#!/usr/bin/env python3
"""M3 exit proof: "The playable build holds budget on the integrated GPU" (ROADMAP, M3).

Plays a whole game of Sepulcra windowed at 1280x720 on the Intel iGPU (the reference device, F3),
with its rules running from Lex and its shamblers dying as ragdolls, and `perf.example.toml`'s
budgets. In real time, during the busiest wave and during the boss wave, `perf_capture` records
window frames and checks the budgets on them. The criterion passes when no frame-level budget is
missed (the same set as M2's criterion 3: the frame's GPU time, frame pacing, draw calls,
triangles, GPU memory, pipelines built after warm-up and the window's draw-list CPU time). Per-pass
GPU and other CPU budgets are reported beside it as notes.

It also requires the game to play to its end, the recording to replay as verified (Lex parity
included) and zero validation messages at shutdown. Other engines and compilers running at the
same time skew timings, so their count is recorded with each capture.

    cargo build -p sepulcra-app --release
    python scripts/m3_exit.py [--adapter integrated] [--port 7488] [--frames 600] [--out DIR]

Output (summary.json, captures, engine log) goes to target/m3-exit/<adapter>/. Keep output ASCII.
"""
import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

from specula_mcp import ROOT, Engine, detailed
from sepulcra_play import EXE, Run
from m2_exit import PERF_TOML, is_frame_metric, other_engines


def compilers_running():
    """How many cargo/rustc processes are running: other sessions' builds skew the timings."""
    if sys.platform != "win32":
        return 0
    n = 0
    for image in ("cargo.exe", "rustc.exe"):
        r = subprocess.run(["tasklist", "/FI", f"IMAGENAME eq {image}", "/FO", "CSV", "/NH"],
                           capture_output=True, text=True)
        n += sum(1 for line in r.stdout.splitlines() if line.startswith('"'))
    return n


def measure(mcp, eng, label, frames, st):
    """Records `frames` window frames in real time and checks the frame-level budgets."""
    mcp.call("time_control", action="resume")
    time.sleep(1.0)
    cap = mcp.json("perf_capture", frames=frames, trace=False, label=f"m3exit_{label}")
    mcp.call("time_control", action="pause")
    errors = cap.get("budget_errors") or []
    required = [e for e in errors if is_frame_metric(e.get("metric", ""))]
    notes = [e for e in errors if not is_frame_metric(e.get("metric", ""))]
    result = {
        "label": label, "wave": st["wave"], "enemies": st["enemies"],
        "frames": cap.get("frames"), "adapter_kind": cap.get("adapter"),
        "frame_ms": cap.get("frame_ms"), "gpu_ms": cap.get("gpu_ms"),
        "gpu_passes_ms": cap.get("gpu_passes_ms"), "draw_calls": cap.get("draw_calls"),
        "triangles": cap.get("triangles"), "budgets_checked_on": cap.get("budgets_checked_on"),
        "budget_misses": required, "notes": notes,
        "other_engines_running": other_engines(eng.proc.pid), "compilers_running": compilers_running(),
    }
    gpu, fr = cap.get("gpu_ms") or {}, cap.get("frame_ms") or {}
    print(f"  {label} (wave {st['wave']}, {st['enemies']} enemies): {cap.get('frames')} frames, "
          f"frame p95 {fr.get('p95')} ms, GPU p95 {gpu.get('p95')} ms; "
          f"other engines {result['other_engines_running']}, compilers {result['compilers_running']}")
    for e in required:
        print(f"    budget miss: {e}")
    for e in notes:
        print(f"    note (per-pass or other CPU budget; doesn't decide the criterion): {e}")
    result["passed"] = bool(cap.get("frames", 0) >= frames // 2) and not required
    return result


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7488)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--seed", default="7")
    ap.add_argument("--frames", type=int, default=600, help="window frames per capture (default 600)")
    ap.add_argument("--exe", default=str(EXE))
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    if args.port == 7471:
        sys.exit("use another port than 7471 (reserved for your working engine)")
    exe = Path(args.exe)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p sepulcra-app --release")
    out = Path(args.out) if args.out else ROOT / "target" / "m3-exit" / args.adapter
    out.mkdir(parents=True, exist_ok=True)
    extra = ["--adapter", args.adapter, "--size", "1280x720", "--perf", str(PERF_TOML),
             "--captures", str(out / "captures"), "--", "--seed", str(args.seed), "--mute", "--rules", "lex"]
    summary = {"adapter": args.adapter, "seed": args.seed, "rules": "lex", "frames_per_capture": args.frames}
    measured = []
    with Engine(exe, args.port, out / "engine.log", *extra, cwd=out) as eng:
        mcp = eng.mcp("m3-exit")

        def call_raw(tool, **a):
            return mcp.request("tools/call", {"name": tool, "arguments": detailed(tool, a)})
        mcp.call_raw = call_raw
        summary["gpu"] = mcp.json("engine_info")["gpu"]["name"]
        print(f"GPU: {summary['gpu']}")
        run = Run(mcp, out)
        mcp.call("time_control", action="pause")
        mcp.json("play_new", seed=args.seed)
        busy = boss = False
        while True:
            st = run.state()
            if st["outcome"] != "running":
                break
            run.drive()
            st = run.state()
            if not busy and st["wave"] >= 6 and st["enemies"] >= 18:
                measured.append(measure(mcp, eng, "busy_wave", args.frames, st))
                run.capture("busy_wave", eye=[38, 27, 52], target=[0, -9, 1])
                busy = True
                continue
            if not boss and st["wave"] == 10 and st["enemies"] >= 1:
                measured.append(measure(mcp, eng, "boss_wave", args.frames, st))
                run.capture("boss_wave", eye=[38, 27, 52], target=[0, -9, 1])
                boss = True
                continue
            run.step(30)
        st = run.state()
        summary["end"] = {k: st[k] for k in ("outcome", "tick", "wave", "tombstones", "state_hash")}
        rec = mcp.json("play_record", path="recordings/m3-exit.json", overwrite=True)
        rep = mcp.json("play_replay", path="recordings/m3-exit.json")
        lex = st.get("rules") or {}
        summary["replay"] = {"status": rep.get("status"), "lex_parity": lex.get("parity"),
                             "lex_hash_recorded": (rec.get("lex") or {}).get("end_hash"),
                             "lex_hash_replayed": (rep.get("lex") or {}).get("end_hash")}
        summary["captures"] = run.captures
        print(f"end: {st['outcome']} at wave {st['wave']}; replay {rep.get('status')}, Lex parity {lex.get('parity')}")
    errors = eng.validation_errors()
    summary["validation_errors"] = errors
    summary["measured"] = measured
    replay_ok = (summary["replay"]["status"] == "verified" and summary["replay"]["lex_parity"] == "ok"
                 and summary["replay"]["lex_hash_recorded"] == summary["replay"]["lex_hash_replayed"])
    ok = len(measured) == 2 and all(m["passed"] for m in measured) and errors == 0 and replay_ok
    summary["passed"] = ok
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"validation errors at shutdown: {errors}; replay verified: {replay_ok}")
    print(("PASS" if ok else "FAIL") + f": summary in {out / 'summary.json'}")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
