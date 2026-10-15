#!/usr/bin/env python3
"""Plays a whole game of Sepulcra through MCP, the way an agent would, and captures it.

Starts the `sepulcra` binary (release) windowed on its own port, starts a run, follows the
"centre" build order by sending each entry with `play_command` once its wave has started (and the
dust is there), and drives time with the engine's `time_control`: mostly paused + `step` (so the
run is reproducible), with stretches of real time and of `scale` fast-forward. It captures
`sight_frame` images at the build phase, during wave 3, at a busy wave, during the boss wave and
at the end, records `perf_frame` during the busy wave, then saves a recording with `play_record`
and verifies it with `play_replay`.

    cargo build -p sepulcra-app --release
    python scripts/sepulcra_play.py                          # discrete GPU, port 7486
    python scripts/sepulcra_play.py --adapter integrated --port 7487

Output (captures, packets, engine log, summary.json) goes to target/sepulcra-play/<adapter>/.
Look at every PNG. Standard library only; keep output ASCII.
"""
import argparse
import json
import sys
import time
from pathlib import Path

from specula_mcp import ROOT, Engine

LAYOUT = ROOT / "games" / "sepulcra" / "data" / "layouts" / "centre.json"
EXE = ROOT / "target" / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")


def to_args(entry):
    """A layout entry ({"wave": 2, "place": "warding", "at": "top:8,12"}) as play_command args."""
    for action in ("place", "upgrade", "sell", "wall", "unwall", "call_wave"):
        if action in entry:
            if action == "place":
                return {"action": "place", "stone": entry["place"], "cell": entry["at"]}
            if action == "call_wave":
                return {"action": "call_wave"}
            return {"action": action, "cell": entry[action]}
    raise ValueError(entry)


class Run:
    def __init__(self, mcp, out):
        self.mcp, self.out = mcp, out
        self.entries = json.loads(LAYOUT.read_text())["commands"]
        self.next = 0
        self.captures = []
        self.applied, self.skipped = 0, []

    def state(self, **args):
        return self.mcp.json("play_state", **args)

    def drive(self):
        """Sends every due entry in order; stops at the first one short of dust (it waits)."""
        st = self.state()
        while self.next < len(self.entries):
            e = self.entries[self.next]
            if "wave" in e and st["wave"] < e["wave"]:
                return
            if "tick" in e and st["tick"] < e["tick"]:
                return
            result = self.mcp.call_raw("play_command", **to_args(e))
            if result.get("isError"):
                err = json.loads(result["content"][0]["text"])
                if err.get("code") == "not_enough_dust":
                    return
                self.skipped.append((self.next, err.get("code")))
            else:
                self.applied += 1
            self.next += 1
            st = self.state()

    def step(self, ticks):
        left = ticks
        while left > 0:
            n = min(left, 500)
            self.mcp.call("time_control", action="step", ticks=n)
            left -= n

    def capture(self, label, eye, target, fov=45):
        packet = self.mcp.frame(width=1280, height=720, eye=eye, target=target, fov_y_deg=fov, max_subjects=12)
        st = self.state()
        self.captures.append({
            "label": label, "png": packet["image"]["path"], "packet": packet["image"]["packet_path"],
            "tick": st["tick"], "wave": st["wave"], "enemies": st["enemies"], "dust": st["dust"],
            "tombstones": st["tombstones"], "subjects_on_screen": packet["subjects_on_screen"],
            "warnings": packet["warnings"],
        })
        print(f"  capture {label}: tick {st['tick']} wave {st['wave']} enemies {st['enemies']} -> {packet['image']['path']}")
        if packet["warnings"]:
            print(f"    warnings: {packet['warnings']}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7486)
    ap.add_argument("--adapter", default="discrete")
    ap.add_argument("--seed", default="7")
    ap.add_argument("--headless", action="store_true")
    ap.add_argument("--exe", default=str(EXE), help="the sepulcra binary (default: this checkout's release build)")
    ap.add_argument("--out", default=None, help="output folder (default target/sepulcra-play/<adapter>)")
    args = ap.parse_args()
    exe = Path(args.exe)
    if args.port == 7471:
        sys.exit("use another port than 7471 (reserved for your working engine)")
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p sepulcra-app --release")
    out = Path(args.out) if args.out else ROOT / "target" / "sepulcra-play" / args.adapter
    out.mkdir(parents=True, exist_ok=True)
    extra = ["--adapter", args.adapter, "--captures", str(out / "captures"), "--size", "1280x720"]
    if args.headless:
        extra.append("--headless")
    summary = {"adapter": args.adapter, "seed": args.seed}
    with Engine(exe, args.port, out / "engine.log", *extra, cwd=out) as eng:
        mcp = eng.mcp("sepulcra-play")

        def call_raw(tool, **a):
            return mcp.request("tools/call", {"name": tool, "arguments": a})
        mcp.call_raw = call_raw
        tools = mcp.tools()
        play = sorted(t for t in tools if t.startswith("play_"))
        print(f"engine up in {eng.startup_s:.1f}s; game tools: {', '.join(play)}")
        summary["tools"] = play
        info = mcp.json("engine_info")
        summary["gpu"] = info["gpu"]["name"]
        print(f"GPU: {info['gpu']['name']}")

        run = Run(mcp, out)
        mcp.call("time_control", action="pause")
        mcp.json("play_new", seed=args.seed)
        run.drive()
        overview = dict(eye=[38, 27, 52], target=[0, -9, 1])
        run.capture("build_overview", **overview)
        run.capture("build_closeup", eye=[16, 14, 20], target=[1, 0, 1], fov=40)

        # Real time for a moment (the window animates), then deterministic steps again.
        mcp.call("time_control", action="resume")
        time.sleep(2.0)
        mcp.call("time_control", action="pause")

        busy_done = boss_done = wave3_done = False
        while True:
            st = run.state()
            if st["outcome"] != "running":
                break
            run.drive()
            st = run.state()
            if not wave3_done and st["wave"] == 3 and st["enemies"] >= 8:
                run.capture("wave3_overview", **overview)
                run.capture("wave3_east_cliff", eye=[40, 2, 6], target=[16, -6, 2], fov=50)
                # The north and west cliffs, where this wave's shamblers climb.
                run.capture("wave3_north_west", eye=[-34, 14, -34], target=[-6, -6, -6], fov=50)
                wave3_done = True
            if not busy_done and st["wave"] >= 6 and st["enemies"] >= 18:
                # A busy wave in real time: let the window run, then read perf.
                mcp.call("time_control", action="resume")
                time.sleep(4.0)
                perf = mcp.json("perf_frame")
                zones = {z["path"]: z for z in mcp.json("perf_zones", min_ms=0.0, limit=200).get("zones", [])
                         if z.get("thread") == "main"}
                draw_zone = zones.get("frame/draw_list", {})
                # While the clock runs: what the window shows (interpolated, with live flashes).
                run.capture("busy_wave_live", eye=[14, 16, 22], target=[0, 0, 2], fov=45)
                mcp.call("time_control", action="pause")
                summary["perf_busy_wave"] = {
                    "wave": st["wave"], "enemies": st["enemies"], "entities": perf.get("entities"),
                    "window": perf.get("window"), "frame_pacing": perf.get("frame_pacing"),
                    "sim_tick": perf.get("sim_tick"), "game_frame": perf.get("game_frame"),
                    "errors": perf.get("errors"),
                    "draw_list": perf.get("draw_list"),
                    "draw_list_zone_ms": {"mean": draw_zone.get("mean_ms"), "p95": draw_zone.get("p95_ms")},
                }
                print(f"  perf at wave {st['wave']} ({st['enemies']} enemies): game_frame {perf.get('game_frame')} "
                      f"sim_tick {perf.get('sim_tick')} pacing {perf.get('frame_pacing')}")
                print(f"  window draw list (frame/draw_list zone): mean {draw_zone.get('mean_ms')} ms, p95 {draw_zone.get('p95_ms')} ms; "
                      f"draws built {perf.get('draw_list')}")
                run.capture("busy_wave", **overview)
                busy_done = True
                # Fast-forward a stretch with scale (the clock runs ticks faster than real time).
                mcp.call("time_control", action="scale", scale=8)
                mcp.call("time_control", action="resume")
                time.sleep(2.0)
                mcp.call("time_control", action="pause")
                mcp.call("time_control", action="scale", scale=1)
                continue
            if not boss_done and st["wave"] == 10:
                enemies = run.state(include=["enemies"], limit=200)["enemies"]["items"]
                boss = next((e for e in enemies if e["kind"] == "colossus" and e["pos"][1] > -1), None)
                if boss:
                    x, y, z = boss["pos"]
                    run.capture("boss_wave", eye=[x + 14, 12, z + 16], target=[x, 3, z], fov=50)
                    run.capture("boss_overview", **overview)
                    boss_done = True
            run.step(30)

        st = run.state(include=["waves", "log"], limit=12)
        run.capture("end", **overview)
        summary["end"] = {k: st[k] for k in ("outcome", "tick", "wave", "tombstones", "dust", "state_hash")}
        summary["log"] = st["log"]["items"]
        summary["layout_applied"], summary["layout_skipped"] = run.applied, run.skipped
        print(f"end: {st['outcome']} at tick {st['tick']}, tombstones {st['tombstones']}, hash {st['state_hash']}")
        rec = mcp.json("play_record", path="recordings/sepulcra-play.json")
        rep = mcp.json("play_replay", path="recordings/sepulcra-play.json")
        summary["recording"], summary["replay"] = rec, rep
        print(f"replay: {rep['status']} (end tick {rep['end_tick']}, hash {rep['end_hash']})")
        summary["captures"] = run.captures
    errors = eng.validation_errors()
    summary["validation_errors"] = errors
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"validation errors at shutdown: {errors}; summary in {out / 'summary.json'}")
    ok = errors == 0 and summary["replay"]["status"] == "verified"
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
