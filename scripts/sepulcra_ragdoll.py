#!/usr/bin/env python3
"""Films a shambler's ragdoll death in a real game of Sepulcra (animation A4 dogfood).

Starts the `sepulcra` binary (release) on its own port, plays the "centre" build order with
`play_command` (as scripts/sepulcra_play.py does), stepping time until the first ragdoll appears
(`physics_joints {ragdolls: true}`), then captures a filmstrip around the body with `sight_frame`
and reads `ragdoll_info`: its parts, joints and the motion checks on the recorded fall (ground
penetration, floating at rest, joint limits, pops where physics takes over) against the game's
`style.toml`. Fails when no ragdoll appears or a motion check fails.

    cargo build -p sepulcra-app --release
    python scripts/sepulcra_ragdoll.py --port 7486

Output (filmstrip PNGs and packets, summary.json, engine log) goes to target/sepulcra-ragdoll/.
Look at the PNGs. Standard library only; keep output ASCII.
"""
import argparse
import json
import sys
from pathlib import Path

from specula_mcp import ROOT, Engine, detailed
import sepulcra_play as sp

# Ticks after the ragdoll is found at which a frame is taken (the game runs 30 ticks a second;
# it removes the body a little after 60).
FILM = [0, 3, 6, 12, 18, 30, 45, 60]
# The checks are read here: the game removes the body a little later.
INFO_AT = 60


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7486)
    ap.add_argument("--adapter", default="discrete")
    ap.add_argument("--seed", default="7")
    ap.add_argument("--exe", default=str(sp.EXE))
    ap.add_argument("--out", default=str(ROOT / "target" / "sepulcra-ragdoll"))
    a = ap.parse_args()
    if a.port == 7471:
        sys.exit("use another port than 7471 (reserved for your working engine)")
    if not Path(a.exe).exists():
        sys.exit(f"{a.exe} not found: cargo build -p sepulcra-app --release")
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    extra = ["--adapter", a.adapter, "--captures", str(out / "captures"), "--size", "1280x720", "--", "--seed", a.seed, "--mute"]
    with Engine(a.exe, a.port, out / "engine.log", *extra, cwd=out) as eng:
        mcp = eng.mcp("ragdoll-dogfood")
        mcp.call_raw = lambda tool, **args: mcp.request("tools/call", {"name": tool, "arguments": detailed(tool, args)})
        run = sp.Run(mcp, out)
        mcp.call("time_control", action="pause")
        mcp.json("play_new", seed=a.seed)
        found = None
        while found is None:
            run.drive()
            run.step(5)
            if run.state()["outcome"] != "running":
                break
            joints = mcp.json("physics_joints", ragdolls=True, limit=1).get("joints", [])
            if joints:
                found = joints[0]["ragdoll"]
        if found is None:
            sys.exit("sepulcra_ragdoll: the run ended without a ragdoll death")
        st = run.state()
        print(f"ragdoll: {found['path']} at tick {st['tick']} (wave {st['wave']})")
        start = mcp.json("ragdoll_info", id=found["id"])
        print(f"  {len(start['parts'])} parts, {len(start['joints'])} joints, mode {start['mode']}, cosmetic {start['cosmetic']}")
        trunk = next(p for p in start["parts"] if p["bone"] in ("hips", "spine", "chest"))["position"]
        target = [trunk[0], max(trunk[1] - 0.6, 0.0), trunk[2]]
        eye = [trunk[0] + 4.5, trunk[1] + 2.5, trunk[2] + 4.5]
        frames, info, done = [], None, 0
        for at in FILM:
            run.step(at - done)
            done = at
            p = mcp.frame(width=960, height=540, eye=eye, target=target, fov_y_deg=40, max_subjects=8)
            frames.append({"after_ticks": at, "png": p["image"]["path"], "packet": p["image"]["packet_path"], "warnings": p["warnings"]})
            print(f"  frame +{at} ticks -> {p['image']['path']}")
            if at == INFO_AT:
                info = mcp.json("ragdoll_info", id=found["id"])
        checks = info["checks"]
        (out / "summary.json").write_text(json.dumps({"ragdoll": found, "tick": st["tick"], "frames": frames, "start": start, "info": info}, indent=1))
        for c in checks.get("checks", []):
            print(f"  {'ok  ' if c['passed'] else 'FAIL'} {c['check']}: {c['value']} {c['unit']} (limit {c['limit']}, {c.get('bone', '')})")
        if not checks.get("passed"):
            sys.exit("sepulcra_ragdoll: a motion check failed")
    print("sepulcra_ragdoll: PASS")


if __name__ == "__main__":
    main()
