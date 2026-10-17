#!/usr/bin/env python3
"""Physics check: builds a physics scene over MCP, steps it, asserts the outcome with
physics_check, queries it, and captures the colliders and contacts overlays.

The scene: a floor; a tower of 10 boxes; a pyramid of boxes; a bouncing ball; a thin wall with a
fast sphere fired at it (the tunnelling guard); a trigger zone a ball falls through; a ramp and
stairs a character walks up; a heightfield mound; a hull rock. Time is paused and stepped
exactly, so the run is deterministic: two runs print the same world and physics hashes.

    cargo build -p specula
    python scripts/physics_check.py                    # port 7484, headless
    python scripts/physics_check.py --adapter integrated --windowed

Everything goes to target/physics-check/: one PNG and one JSON per capture, plus summary.json.
LOOK at the PNGs (and have the sight-skeptic check the claims) before calling it good.
Standard library only; output is ASCII.
"""
import argparse
import base64
import json
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

OUT = ROOT / "target" / "physics-check"
# Ticks stepped after the first (20 s at the engine's 30 Hz).
STEPS = 600


def save(name, result):
    png = None
    text = None
    for c in result["content"]:
        if c["type"] == "image":
            png = OUT / f"{name}.png"
            png.write_bytes(base64.b64decode(c["data"]))
        elif c["type"] == "text":
            text = c["text"]
    data = json.loads(text)
    (OUT / f"{name}.json").write_text(json.dumps(data, indent=2))
    return data, png


def check(cond, msg, failures):
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        failures.append(msg)


def scene_ops():
    ops = [
        {"op": "spawn", "shape": "cube", "name": "floor", "position": [0, -0.5, 0], "scale": [40, 1, 40], "color": "#3a3f47",
         "collider": {"shape": "box"}},
        {"op": "spawn", "shape": "group", "name": "tower"},
        {"op": "spawn", "shape": "group", "name": "pyramid"},
    ]
    for i in range(10):
        ops.append({"op": "spawn", "shape": "cube", "parent": "/tower", "name": f"b{i}", "position": [-6, 0.5 + i, -4],
                    "color": "#c8964b" if i % 2 else "#b07a3a", "collider": {"shape": "box"},
                    "body": {"kind": "dynamic", "mass": 5}})
    rows = 6
    for row in range(rows):
        for col in range(rows - row):
            x = col * 1.02 - (rows - row - 1) * 0.51
            ops.append({"op": "spawn", "shape": "cube", "parent": "/pyramid", "name": f"p{row}_{col}", "position": [x, 0.5 + row, -9],
                        "color": "#6f8fb0", "collider": {"shape": "box"}, "body": {"kind": "dynamic", "mass": 2}})
    ops += [
        # A bouncing ball.
        {"op": "spawn", "shape": "sphere", "name": "ball", "position": [3, 6, -4], "scale": [0.6, 0.6, 0.6], "color": "#d04a4a",
         "collider": {"shape": "sphere", "radius": 0.5, "restitution": 0.7}, "body": {"kind": "dynamic", "mass": 1}},
        # A thin wall and a fast slug fired at it (300 m/s: 10 m per tick at the engine's 30 Hz).
        {"op": "spawn", "shape": "cube", "name": "thin_wall", "position": [8, 1.5, 4], "scale": [0.05, 3, 3], "color": "#9aa0aa",
         "collider": {"shape": "box"}},
        {"op": "spawn", "shape": "sphere", "name": "slug", "position": [-8, 1.5, 4], "scale": [0.2, 0.2, 0.2], "color": "#f0d040",
         "collider": {"shape": "sphere", "radius": 0.5}, "body": {"kind": "dynamic", "mass": 0.05, "velocity": [300, 0, 0], "gravity_scale": 0}},
        # A trigger zone and a ball that falls through it.
        {"op": "spawn", "shape": "cube", "name": "zone", "position": [0, 3, 4], "scale": [2, 1, 2], "color": "#c040c0", "opacity": 0.25,
         "collider": {"shape": "box", "is_trigger": True, "layer": "sensors"}},
        {"op": "spawn", "shape": "sphere", "name": "drop", "position": [0, 6, 4], "scale": [0.4, 0.4, 0.4], "color": "#40c0c0",
         "collider": {"shape": "sphere", "radius": 0.5}, "body": {"kind": "dynamic", "mass": 1}},
        # Stairs and a character that walks up them (four 0.2 m steps, a 0.75 m gap, a landing).
        {"op": "spawn", "shape": "group", "name": "stairs"},
    ]
    for i in range(4):
        h = 0.2 * (i + 1)
        ops.append({"op": "spawn", "shape": "cube", "parent": "/stairs", "name": f"s{i}", "position": [6 + 0.5 * i, h / 2, -4],
                    "scale": [0.5, h, 2], "color": "#7a7f88", "collider": {"shape": "box"}})
    ops += [
        {"op": "spawn", "shape": "cube", "parent": "/stairs", "name": "landing", "position": [9.5, 0.4, -4], "scale": [2, 0.8, 2],
         "color": "#7a7f88", "collider": {"shape": "box"}},
        {"op": "spawn", "shape": "cube", "parent": "/stairs", "name": "end_wall", "position": [10.75, 1.5, -4], "scale": [0.5, 3, 2],
         "color": "#5a5f68", "collider": {"shape": "box"}},
        {"op": "spawn", "shape": "group", "name": "hero", "position": [4, 0.92, -4],
         "collider": {"shape": "capsule", "radius": 0.3, "height": 1.8, "layer": "characters"},
         "body": {"kind": "kinematic", "mass": 70, "velocity": [1.5, 0, 0], "character": True}},
        {"op": "spawn", "shape": "lathe", "parent": "/hero", "name": "look", "profile": [[0, -0.9], [0.3, -0.85], [0.3, 0.6], [0.25, 0.85], [0, 0.9]],
         "segments": 16, "color": "#e0b070"},
        # A heightfield mound and a hull rock on it.
        {"op": "spawn", "shape": "group", "name": "mound", "position": [-10, 0, 10],
         "collider": {"shape": "heightfield", "columns": 9, "rows": 9, "cell": [1, 1],
                      "heights": [round(max(0.0, 1.2 - 0.25 * ((x - 4) ** 2 + (z - 4) ** 2) ** 0.5), 3) for z in range(9) for x in range(9)]}},
        {"op": "spawn", "shape": "sphere", "name": "rock", "position": [-10, 3, 10], "scale": [0.5, 0.4, 0.5], "color": "#8c8170",
         "collider": {"shape": "hull", "points": [[-1, -1, -1], [1, -1, -1], [0, -1, 1], [0, 1, 0], [0.6, 0.2, 0.6], [-0.7, 0.1, 0.3]]},
         "body": {"kind": "dynamic", "mass": 20}},
    ]
    return ops


def run(m, failures, label):
    start_tick = m.json("time_control", action="pause")["tick"]
    m.json("world_env", physics={"gravity": [0, -9.81, 0]})
    reply = m.json("world_batch", ops=scene_ops())
    check(reply.get("ok", True) is not False and "error" not in reply, f"{label}: scene built ({len(scene_ops())} ops)", failures)
    t = time.perf_counter()
    m.json("time_control", action="step", ticks=1)
    # A capture of the start, with colliders.
    eye = {"eye": [6, 9, 18], "target": [0, 1, -3], "width": 1280, "height": 720}
    first, _ = save(f"{label}_start", m.call("sight_frame", overlays=["colliders", "grid"], **eye))
    check("colliders" in first.get("overlays", {}).get("drawn", []), f"{label}: colliders overlay drawn at the start", failures)
    for _ in range(STEPS // 30):
        m.json("time_control", action="step", ticks=30)
    elapsed = time.perf_counter() - t
    info = m.json("physics_info")
    clock = m.json("time_control", action="info")
    ticks = clock["tick"] - start_tick
    check(ticks == STEPS + 1, f"{label}: stepped {ticks} ticks at {clock['tick_hz']} Hz", failures)
    print(f"  {label}: {ticks} ticks in {elapsed:.2f} s wall ({info['bodies']} bodies, {info['awake']} awake), "
          f"tick p95 {info['tick'] and info['tick'].get('p95_ms')} ms")
    checks = m.json("physics_check", checks=[
        {"check": "at_rest", "entities": "/tower"},
        {"check": "at_rest", "entities": "/pyramid"},
        {"check": "inside", "entity": "/tower/b9", "min": [-6.1, 9.3, -4.1], "max": [-5.9, 9.6, -3.9]},
        {"check": "inside", "entity": "/slug", "min": [-20, -1, -10], "max": [8, 10, 10]},
        {"check": "touching", "a": "/tower/b0", "b": "/floor"},
        {"check": "no_penetration", "max_depth": 0.03},
        {"check": "no_tunnelling"},
        {"check": "energy", "max_gain_j": 5},
        {"check": "inside", "entity": "/hero", "min": [9.5, 1.6, -4.5], "max": [10.4, 1.8, -3.5]},
        # Resting on the mound's 1.2 m peak, not sunk into it (its centre ~0.35-0.45 m above).
        {"check": "inside", "entity": "/rock", "min": [-10.6, 1.45, 9.4], "max": [-9.4, 1.75, 10.6]},
    ])
    (OUT / f"{label}_checks.json").write_text(json.dumps(checks, indent=2))
    for r in checks["results"]:
        check(r["pass"], f"{label}: physics_check {r['check']} " + json.dumps({k: v for k, v in r.items() if k not in ("check", "pass", "note")})[:160], failures)
    events = m.json("physics_events", limit=256)
    kinds = {}
    for e in events["events"]:
        kinds[e["kind"]] = kinds.get(e["kind"], 0) + 1
    check(any(e["kind"] == "ccd" and e["a"]["path"] == "/slug" for e in events["events"]), f"{label}: the tunnelling guard stopped the slug", failures)
    check(any(e["kind"] == "trigger_enter" and e["a"]["path"] == "/zone" and e["b"]["path"] == "/drop" for e in events["events"]),
          f"{label}: the drop entered the zone", failures)
    check(any(e["kind"] == "trigger_exit" and e["a"]["path"] == "/zone" for e in events["events"]), f"{label}: and left it", failures)
    ray = m.json("physics_raycast", origin=[-6, 20, -4], direction=[0, -1, 0], max_hits=3)
    check(ray["hits"] and ray["hits"][0]["path"] == "/tower/b9" and abs(ray["hits"][0]["distance"] - 10.0) < 0.1,
          f"{label}: a ray down the tower hits its top box ~10 m down: {ray['hits'][:1]}", failures)
    ov = m.json("physics_overlap", shape={"shape": "box", "size": [2, 2, 2]}, at=[0, 3, 4])
    check(any(e["path"] == "/zone" for e in ov["entities"]), f"{label}: overlap finds the trigger zone", failures)
    ex = m.json("physics_explain", path="/slug")
    check(ex["simulated"] and ex["tunnelling"]["guarded_against_static"], f"{label}: physics_explain on the slug", failures)
    contacts = m.json("physics_contacts", entity="/tower/b0")
    check(contacts["total"] >= 2, f"{label}: the tower's bottom box has its contacts ({contacts['total']})", failures)
    end, _ = save(f"{label}_end", m.call("sight_frame", overlays=["colliders", "contacts"], **eye))
    od = end.get("overlays", {})
    check("contacts" in od.get("drawn", []) and od.get("contacts", {}).get("points", 0) > 20,
          f"{label}: contacts overlay drew {od.get('contacts', {}).get('points')} points", failures)
    save(f"{label}_stairs", m.call("sight_frame", overlays=["colliders", "contacts"], eye=[7, 3, 2], target=[8, 1, -4], width=960, height=540))
    rock, _ = save(f"{label}_rock", m.call("sight_frame", overlays=["colliders", "contacts"], eye=[-6, 4, 15], target=[-10, 1.2, 10], width=960, height=540))
    rock_subject = next((x for x in rock.get("subjects", []) if x.get("path") == "/rock"), {})
    check(rock_subject.get("on_screen") is True, f"{label}: the rock on the mound is in its close-up", failures)
    hashes = m.json("world_hash", recent=1, verify=True)
    check(hashes["verify"]["matches"], f"{label}: the journal replays the world exactly", failures)
    check("physics" in hashes, f"{label}: world_hash reports the physics state hash", failures)
    perf = m.json("perf_frame")
    check("physics" in perf and perf["physics"]["bodies"] > 0, f"{label}: perf_frame has a physics section", failures)
    (OUT / f"{label}_info.json").write_text(json.dumps(info, indent=2))
    (OUT / f"{label}_events.json").write_text(json.dumps(events, indent=2))
    return {"world_hash": hashes["state_hash"], "physics_hash": hashes["physics"]["state_hash"], "events": kinds, "wall_s": elapsed,
            "tick_ms": info.get("tick"), "stages_ms": info.get("stages_ms")}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7484)
    ap.add_argument("--adapter", default="discrete")
    ap.add_argument("--windowed", action="store_true")
    ap.add_argument("--exe", default=str(ROOT / "target" / "debug" / "specula.exe"))
    a = ap.parse_args()
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    failures = []
    runs = []
    for i in range(2):
        args = ["--adapter", a.adapter, "--captures", str(OUT / "captures"), "--worlds", str(OUT / f"worlds{i}")]
        if not a.windowed:
            args.append("--headless")
        with Engine(a.exe, a.port, OUT / f"engine{i}.log", *args, cwd=str(OUT)) as eng:
            with eng.mcp("physics-check") as m:
                runs.append(run(m, failures, f"run{i}"))
        errs = eng.validation_errors()
        check(errs in (0, None), f"run{i}: clean shutdown ({errs} validation errors)", failures)
    check(runs[0]["world_hash"] == runs[1]["world_hash"] and runs[0]["physics_hash"] == runs[1]["physics_hash"],
          f"two runs end in the same world and physics hashes ({runs[0]['world_hash']} / {runs[0]['physics_hash']})", failures)
    (OUT / "summary.json").write_text(json.dumps({"runs": runs, "failures": failures}, indent=2))
    print("PASS" if not failures else f"FAIL ({len(failures)})")
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
