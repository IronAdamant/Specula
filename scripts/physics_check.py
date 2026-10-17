#!/usr/bin/env python3
"""Physics check: builds a physics scene over MCP, steps it, asserts the outcome with
physics_check, queries it, and captures the colliders and contacts overlays.

The scene: a floor; a tower of 10 boxes; a pyramid of boxes; a bouncing ball; a thin wall with a
fast sphere fired at it (the tunnelling guard); a trigger zone a ball falls through; a ramp and
stairs a character walks up; a heightfield mound; a hull rock. Time is paused and stepped
exactly, so the run is deterministic: two runs print the same world and physics hashes.

    cargo build -p specula
    python scripts/physics_check.py                    # ports 7484-7485, headless
    python scripts/physics_check.py --adapter integrated --windowed

Everything goes to <target>/physics-check/ (CARGO_TARGET_DIR when set): one PNG and one JSON per capture, plus summary.json.
LOOK at the PNGs (and have the sight-skeptic check the claims) before calling it good.
Standard library only; output is ASCII.
"""
import argparse
import base64
import os
import json
import math
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

# Builds and outputs go where cargo builds (CARGO_TARGET_DIR when set).
TARGET = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
OUT = TARGET / "physics-check"
ROCK_VIEW = {"eye": [-8.6, 2.2, 11.9], "target": [-10, 1.5, 10], "width": 960, "height": 540}
ROCK_START_VIEW = {"eye": [-7.6, 2.8, 14.0], "target": [-10, 2.0, 10], "width": 960, "height": 540}
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


def rock_points():
    """16 points spread evenly over a sphere a little larger than the rock's mesh (radius 0.5
    before its scale), so the hull's faces lie at the visible surface."""
    pts = []
    for i in range(16):
        y = 1 - 2 * (i + 0.5) / 16
        r, a = (1 - y * y) ** 0.5, i * 2.39996
        pts.append([round(0.61 * r * math.cos(a), 4), round(0.61 * y, 4), round(0.61 * r * math.sin(a), 4)])
    return pts


def mound_height(r):
    """The mound: flat at 1.2 m out to 2.6 m, then down to the floor at 4.8 m."""
    return round(1.2 if r <= 2.6 else max(0.0, 1.2 - 1.2 / 2.2 * (r - 2.6)), 4)


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
        # A heightfield mound (a flat-topped cone sampled every 1.25 m) with a visible one (a
        # lathe) that matches it at the samples, and a rock on its top: a sphere mesh (radius 0.5 before its
        # scale) with a hull inscribed in that sphere, so what you see is what collides.
        {"op": "spawn", "shape": "group", "name": "mound", "position": [-10, 0, 10],
         "collider": {"shape": "heightfield", "columns": 9, "rows": 9, "cell": [1.25, 1.25],
                      "heights": [mound_height(1.25 * ((x - 4) ** 2 + (z - 4) ** 2) ** 0.5) for z in range(9) for x in range(9)]}},
        {"op": "spawn", "shape": "lathe", "parent": "/mound", "name": "look", "profile": [[0, 1.2], [2.6, 1.2], [4.8, 0]], "segments": 48,
         "color": "#6b5d4a"},
        {"op": "spawn", "shape": "sphere", "name": "rock", "position": [-10, 3, 10], "scale": [0.5, 0.4, 0.5], "color": "#8c8170",
         "collider": {"shape": "hull", "points": rock_points()},
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
    rock_start, _ = save(f"{label}_rock_start", m.call("sight_frame", overlays=["colliders", "contacts"], **ROCK_START_VIEW))
    check(any(x.get("path") == "/rock" and x.get("on_screen") for x in rock_start.get("subjects", [])), f"{label}: the rock is in the air in its start close-up", failures)
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
        # Resting on the mound's 1.2 m peak: its centre 0.2 m above (the scaled radius), so a
        # rock sunk into the mound (below 1.3) or floating over it (above 1.46) fails.
        {"check": "inside", "entity": "/rock", "min": [-10.6, 1.3, 9.4], "max": [-9.4, 1.46, 10.6]},
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
    rock, _ = save(f"{label}_rock", m.call("sight_frame", overlays=["colliders", "contacts"], **ROCK_VIEW))
    rock_subject = next((x for x in rock.get("subjects", []) if x.get("path") == "/rock"), {})
    check(rock_subject.get("on_screen") is True, f"{label}: the rock on the mound is in its close-up", failures)
    hashes = m.json("world_hash", recent=1, verify=True)
    check(hashes["verify"]["matches"], f"{label}: the journal replays the world exactly", failures)
    check("physics" in hashes, f"{label}: world_hash reports the physics state hash", failures)
    trace = m.json("desync_trace")
    trace_text = Path(trace["file"]).read_text() if trace.get("file") else ""
    check('"physics"' in trace_text, f"{label}: the desync trace carries a physics subsystem hash", failures)
    perf = m.json("perf_frame")
    check("physics" in perf and perf["physics"]["bodies"] > 0, f"{label}: perf_frame has a physics section", failures)
    (OUT / f"{label}_info.json").write_text(json.dumps(info, indent=2))
    (OUT / f"{label}_events.json").write_text(json.dumps(events, indent=2))
    return {"world_hash": hashes["state_hash"], "physics_hash": hashes["physics"]["state_hash"], "events": kinds, "wall_s": elapsed,
            "tick_ms": info.get("tick"), "stages_ms": info.get("stages_ms")}


def tool(m, name, **args):
    """(is_error, parsed or text) of a tool call, without raising on tool errors."""
    r = m.request("tools/call", {"name": name, "arguments": args})
    text = next(c["text"] for c in r["content"] if c["type"] == "text")
    try:
        data = json.loads(text)
    except ValueError:
        data = text
    return bool(r.get("isError")), data


def regressions(exe, port, adapter, failures):
    """Review round 1's live repros: impulses that add up and never crash (findings 1, 3),
    thin plates and huge thin query boxes (2), and a world with a plate that reloads (2)."""
    worlds = OUT / "worlds-regress"
    args = ["--adapter", adapter, "--headless", "--captures", str(OUT / "captures"), "--worlds", str(worlds), "--world", "plate"]
    with Engine(exe, port, OUT / "engine-regress.log", *args, cwd=str(OUT)) as eng:
        with eng.mcp("physics-regress") as m:
            m.json("time_control", action="pause")
            m.json("world_spawn", shape="cube", name="floor", position=[0, -0.5, 0], scale=[20, 1, 20], collider={"shape": "box"})
            m.json("world_spawn", shape="cube", name="crate", position=[0, 0.5, 0], collider={"shape": "box"}, body={"kind": "dynamic", "mass": 2})
            m.json("time_control", action="step", ticks=2)
            _, a = tool(m, "physics_impulse", path="/crate", impulse=[2, 0, 0])
            _, b = tool(m, "physics_impulse", path="/crate", impulse=[2, 0, 0])
            check(abs(a["velocity"][0] - 1) < 1e-4 and abs(b["velocity"][0] - 2) < 1e-4, f"regress: two impulses in one tick add up ({a['velocity'][0]}, {b['velocity'][0]})", failures)
            m.json("world_spawn", shape="cube", name="k", position=[5, 0.5, 0], collider={"shape": "box"}, body="kinematic")
            m.json("time_control", action="step", ticks=1)
            m.json("world_set", path="/k", body="dynamic")
            err, c = tool(m, "physics_impulse", path="/k", impulse=[1, 0, 0])
            check(not err and abs(c["velocity"][0] - 1) < 1e-4, f"regress: an impulse right after a kind change works ({c})", failures)
            err, d = tool(m, "physics_impulse", path="/floor", impulse=[1, 0, 0])
            check(err and "not_dynamic" in json.dumps(d), "regress: an impulse on a static body is a structured error", failures)
            m.json("world_spawn", shape="cube", name="plate", position=[30, 0, 0], scale=[1000, 0.01, 1000], collider={"shape": "box"})
            m.json("world_spawn", shape="cube", name="sliver", position=[0, 20, 0], scale=[0.001, 0.001, 0.001],
                   collider={"shape": "box", "size": [0.0001, 0.0001, 0.0001]})
            m.json("time_control", action="step", ticks=1)
            err, ov = tool(m, "physics_overlap", shape={"shape": "box", "size": [10000, 0.0001, 10000]}, at=[0, 0, 0])
            check(not err and any(e["path"] == "/plate" for e in ov["entities"]), "regress: a huge thin query box works", failures)
            check(tool(m, "engine_info")[0] is False, "regress: the engine is alive", failures)
            m.json("world_save")
    check(eng.exit_code in (0, None) and eng.validation_errors() in (0, None), "regress: clean shutdown", failures)
    with Engine(exe, port, OUT / "engine-regress-reload.log", *args, cwd=str(OUT)) as eng:
        with eng.mcp("physics-regress") as m:
            m.json("time_control", action="step", ticks=5)
            info = m.json("physics_info")
            check(info["bodies"] >= 5 and info["not_simulated_total"] == 0, f"regress: the plate world reloads and simulates ({info['bodies']} bodies)", failures)
    check(eng.exit_code in (0, None), "regress: clean shutdown after the reload", failures)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7484)
    ap.add_argument("--adapter", default="discrete")
    ap.add_argument("--windowed", action="store_true")
    ap.add_argument("--exe", default=str(TARGET / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")))
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
    regressions(a.exe, a.port + 1, a.adapter, failures)
    (OUT / "summary.json").write_text(json.dumps({"runs": runs, "failures": failures}, indent=2))
    print("PASS" if not failures else f"FAIL ({len(failures)})")
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
