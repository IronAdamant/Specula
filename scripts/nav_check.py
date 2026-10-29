#!/usr/bin/env python3
"""Navigation, AI and Locus check: builds a courtyard over MCP, places furniture by relations
(world_place), builds a navmesh, paths through it, edits it and checks only the touched tiles
rebuild, runs a guard with a mind that patrols and then chases a thief, runs a crowd, and
captures the navmesh and paths overlays. Everything is asserted with data.

The scene: a 24 m x 24 m floor, outer walls, and a divider wall at x = 0 with a 2.4 m doorway at
z = 0 (all with box colliders). The guard is a physics character (capsule, body.character) in the
west room; the thief a plain nav agent in the east room. Time is paused and stepped exactly, so
two runs give the same world, physics and nav hashes (checked).

    cargo build -p specula
    python scripts/nav_check.py                    # ports 7484-7485, headless
    python scripts/nav_check.py --adapter integrated --windowed
    python scripts/nav_check.py --big              # also a 100 m x 100 m build for timing

Everything goes to <target>/nav-check/ (CARGO_TARGET_DIR when set): one PNG and one JSON per
capture, plus summary.json (--out picks another folder). LOOK at the PNGs (and have the sight-skeptic check the claims)
before calling it good. Standard library only; output is ASCII.
"""
import argparse
import base64
import json
import math
import os
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

TARGET = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
OUT = TARGET / "nav-check"
TOP = {"ortho": {"height_m": 27}, "eye": [0, 40, 0.01], "target": [0, 0, 0], "width": 960, "height": 960}
PERSPECTIVE = {"eye": [-16, 16, 18], "target": [0, 0, 0], "width": 1280, "height": 720}
WAYPOINTS = [[-9, 0, -9], [-3, 0, -9], [-3, 0, 9], [-9, 0, 9]]
GUARD_MIND = {
    "label": "guard",
    "root": {"selector": [
        {"sequence": [{"if": "sees", "target": "/thief", "range": 9, "fov_deg": 150},
                      {"set": "alert", "value": True},
                      {"do": "chase", "target": "/thief", "within": 1.2, "give_up": 30}], "label": "pursue"},
        {"do": "patrol", "points": "/patrol", "label": "patrol"},
    ]},
}


def save(name, result):
    text = None
    for c in result["content"]:
        if c["type"] == "image":
            (OUT / f"{name}.png").write_bytes(base64.b64decode(c["data"]))
        elif c["type"] == "text":
            text = c["text"]
    data = json.loads(text)
    (OUT / f"{name}.json").write_text(json.dumps(data, indent=2))
    return data


def check(cond, msg, failures):
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        failures.append(msg)
    return cond


def tool(m, tool_name, **args):
    """(is_error, parsed or text) of a tool call, without raising on tool errors."""
    r = m.request("tools/call", {"name": tool_name, "arguments": args})
    text = next(c["text"] for c in r["content"] if c["type"] == "text")
    try:
        data = json.loads(text)
    except ValueError:
        data = text
    return bool(r.get("isError")), data


def build_all(m, **args):
    """nav_build, then fixed ticks until the navmesh is complete: the ticks build it (the world is
    paused here, so time_control step runs them). Returns nav_build's answer with the built
    navmesh's polygons and tiles, the ticks it took and their wall time."""
    b = m.json("nav_build", **args)
    path = b["path"]
    t0, steps = time.perf_counter(), 0
    while True:
        mesh = next(n for n in m.json("nav_info", mesh=path)["navmeshes"] if n["path"] == path)
        if mesh["ready"] and mesh["dirty_tiles"] == 0 and "building_new_settings" not in mesh:
            break
        m.json("time_control", action="step", ticks=5)
        steps += 5
        if steps > 5000:
            raise RuntimeError(f"{path} didn't finish building: {mesh}")
    for k in ("polygons", "tiles_built", "links", "offmesh_links_found", "hash"):
        if k in mesh:
            b[k] = mesh[k]
    b["ticks"], b["ms"] = steps, round((time.perf_counter() - t0) * 1000, 3)
    return b


def dist_xz(a, b):
    return math.hypot(a[0] - b[0], a[2] - b[2])


def pos(m, path):
    return m.json("world_query", path_prefix=path, fields=["path", "position"], limit=1)["items"][0]["position"]


def courtyard_ops():
    wall = {"collider": {"shape": "box"}, "color": "#8c8f96"}
    return [
        {"op": "spawn", "shape": "cube", "name": "floor", "position": [0, -0.5, 0], "scale": [24, 1, 24], "color": "#4a4f57", "collider": {"shape": "box"}},
        {"op": "spawn", "shape": "cube", "name": "wall_north", "position": [0, 1.25, -12.2], "scale": [24.8, 2.5, 0.4], **wall},
        {"op": "spawn", "shape": "cube", "name": "wall_south", "position": [0, 1.25, 12.2], "scale": [24.8, 2.5, 0.4], **wall},
        {"op": "spawn", "shape": "cube", "name": "wall_west", "position": [-12.2, 1.25, 0], "scale": [0.4, 2.5, 24], **wall},
        {"op": "spawn", "shape": "cube", "name": "wall_east", "position": [12.2, 1.25, 0], "scale": [0.4, 2.5, 24], **wall},
        # The divider at x = 0, with a doorway from z = -1.2 to 1.2.
        {"op": "spawn", "shape": "cube", "name": "divider_north", "position": [0, 1.25, -6.6], "scale": [0.4, 2.5, 10.8], **wall},
        {"op": "spawn", "shape": "cube", "name": "divider_south", "position": [0, 1.25, 6.6], "scale": [0.4, 2.5, 10.8], **wall},
        {"op": "spawn", "shape": "group", "name": "patrol"},
    ] + [{"op": "spawn", "shape": "group", "parent": "/patrol", "name": f"wp{i}", "position": p} for i, p in enumerate(WAYPOINTS)]


def place(m, failures, label, **args):
    err, data = tool(m, "world_place", **args)
    if err:
        check(False, f"{label}: world_place answered {json.dumps(data)[:600]}", failures)
        return None
    return data


def locus(m, failures):
    """Furniture by relations; returns what it placed."""
    print("Locus")
    east = {"min": [0.2, 0, -12], "max": [12, 3, 12]}
    t = place(m, failures, "table", spawn={"shape": "cube", "name": "table", "scale": [2, 0.8, 1], "color": "#8a6a48", "collider": {"shape": "box"}},
              relations=[{"rel": "against", "target": "/wall_north", "side": "south"}, {"rel": "inside", "region": east},
                         {"rel": "near", "target": [6, 0, -11], "max": 0.5, "soft": True}])
    if t:
        p = t["placements"][0]
        check(t["satisfied"], "table: against the north wall, inside the east room", failures)
        check(abs(p["position"][2] - (-12.0 + 0.5)) < 0.02, f"table: its back on the wall's face (z {p['position'][2]}, want -11.5)", failures)
        check(abs(p["position"][1] - 0.4) < 0.01, f"table: rests on the floor (y {p['position'][1]}, want 0.4)", failures)
        check(abs(p["yaw_deg"]) < 0.5, f"table: faces away from the wall (yaw {p['yaw_deg']})", failures)
    c = place(m, failures, "crate", spawn={"shape": "cube", "name": "crate", "scale": [0.4, 0.4, 0.4], "color": "#b08040", "collider": {"shape": "box"}},
              relations=[{"rel": "on", "target": "/table"}])
    if c:
        p = c["placements"][0]
        check(c["satisfied"] and p.get("rests_on", {}).get("path") == "/table", "crate: on the table", failures)
        check(abs(p["position"][1] - 1.0) < 0.01, f"crate: on the table's top (y {p['position'][1]}, want 1.0)", failures)
    pl = place(m, failures, "pillars", spawn={"shape": "cube", "name": "pillar", "scale": [0.4, 2, 0.4], "color": "#a0a4ac", "collider": {"shape": "box"}, "tags": ["pillar"]},
               count=4, relations=[{"rel": "along", "target": "/divider_south", "offset": 1.5, "side": "east"}, {"rel": "spacing", "min": 1.5}])
    if pl:
        ps = [p["position"] for p in pl["placements"]]
        check(pl["satisfied"] and len(ps) == 4, "pillars: 4 along the divider, 1.5 m east of it, spaced", failures)
        check(all(abs(p[0] - 1.5) < 0.02 for p in ps), f"pillars: on the line x = 1.5 ({[round(p[0], 3) for p in ps]})", failures)
        zs = sorted(p[2] for p in ps)
        check(all(b - a >= 1.9 for a, b in zip(zs, zs[1:])), f"pillars: at least 1.5 m apart ({[round(z, 2) for z in zs]})", failures)
        check(all(1.2 < z < 12.0 for z in zs), "pillars: within the wall's length", failures)
    lamp = place(m, failures, "lamp", spawn={"shape": "sphere", "name": "lamp", "scale": [0.3, 0.3, 0.3], "color": "#f0d060", "emissive": 2},
                 relations=[{"rel": "between", "a": "/pillar_1", "b": "/pillar_2", "t": 0.5}])
    if lamp and pl:
        a, b = pl["placements"][0]["position"], pl["placements"][1]["position"]
        mid = [(a[0] + b[0]) / 2, 0, (a[2] + b[2]) / 2]
        p = lamp["placements"][0]["position"]
        check(dist_xz(p, mid) < 0.01 and abs(p[1] - 0.15) < 0.01, f"lamp: midway between two pillars, on the floor ({p})", failures)
    bench = place(m, failures, "bench", spawn={"shape": "cube", "name": "bench", "scale": [1.2, 0.45, 0.4], "color": "#6a5038", "collider": {"shape": "box"}},
                  relations=[{"rel": "near", "target": "/table", "min": 0.4, "max": 1.2}, {"rel": "facing", "target": "/table"},
                             {"rel": "avoid", "tag": "pillar", "distance": 1.0}])
    if bench:
        p = bench["placements"][0]
        check(bench["satisfied"], "bench: near the table, facing it, clear of the pillars", failures)
        table = t["placements"][0]["position"] if t else [6, 0, -11.5]
        want = math.degrees(math.atan2(table[0] - p["position"][0], table[2] - p["position"][2]))
        diff = abs((p["yaw_deg"] - want + 180) % 360 - 180)
        check(diff < 1.0, f"bench: its front (+Z) turned toward the table ({p['yaw_deg']} vs {want:.1f})", failures)
    # Ten barrels don't fit 1 m apart along 4 m: refused, with hints and an alternative.
    barrels = {"shape": "cube", "name": "barrel", "scale": [0.6, 0.9, 0.6], "color": "#704828", "collider": {"shape": "box"}}
    rels = [{"rel": "along", "from": [5, 0, 9], "to": [9, 0, 9]}, {"rel": "spacing", "min": 1.0}]
    before = m.json("world_hash")["journal_last_id"]
    err, refused = tool(m, "world_place", spawn=barrels, count=10, relations=rels)
    check(err and refused.get("error") == "unsatisfied", f"barrels: 10 at 1 m on 4 m refused ({refused.get('error') if isinstance(refused, dict) else refused})", failures)
    hints = refused.get("hints", []) if isinstance(refused, dict) else []
    check(any("of 10 fit" in h for h in hints), f"barrels: a hint says how many fit ({hints})", failures)
    check(m.json("world_hash")["journal_last_id"] == before, "barrels: nothing was written", failures)
    alt = place(m, failures, "barrels alternative", spawn=barrels, count=3, relations=rels)
    check(bool(alt) and alt["satisfied"], "barrels: three fit", failures)
    # A crate asked into the table's middle: the nearest free spot comes back.
    tc = t["placements"][0]["position"] if t else [6, 0.4, -11.5]
    err, blocked = tool(m, "world_place", spawn={"shape": "cube", "name": "box2", "scale": [0.5, 0.5, 0.5], "collider": {"shape": "box"}},
                        relations=[{"rel": "between", "a": [tc[0], 0, tc[2]], "b": [tc[0], 0, tc[2]], "t": 0.5}])
    bp = blocked["placements"][0] if isinstance(blocked, dict) and blocked.get("placements") else {}
    check(err and blocked.get("error") == "unsatisfied" and "blocked_by" not in bp, "box2: a spot inside the table is refused; the alternative is collision-free", failures)
    off = bp.get("relations", [{}])[0].get("off", 0)
    check(0.4 < off < 2.0, f"box2: the alternative is near the asked spot ({off} m off)", failures)
    # A dry run writes nothing; undo takes a placement back.
    before = m.json("world_hash")["journal_last_id"]
    dry = place(m, failures, "dry run", spawn={"shape": "cube", "name": "ghost"}, relations=[{"rel": "near", "target": "/bench", "max": 2}], dry_run=True)
    check(bool(dry) and dry.get("dry_run") and m.json("world_hash")["journal_last_id"] == before, "dry run: solved, nothing written", failures)
    undo = m.json("world_undo")
    err, q = tool(m, "world_query", path_prefix="/barrel_3", limit=1)
    gone = err and "not_found" in json.dumps(q)
    check(undo["changes"] >= 1 and gone, "undo: the last placement (the barrels) is taken back", failures)
    history = m.json("world_history", limit=20)
    kinds = [e.get("kind") for e in history.get("entries", [])]
    check("batch" in kinds, "placements are journaled as batches", failures)
    return {"table": t, "crate": c, "pillars": pl, "bench": bench}


def run(m, failures, label, big):
    print(f"[{label}]")
    m.hello("nav-check", role="lead")  # force and overwrite of other worlds are the lead's (docs/design/AUTHORITY.md)
    m.json("time_control", action="pause")
    m.json("world_batch", ops=courtyard_ops())
    m.json("world_env", sun_direction=[-0.4, -1, -0.3], ambient_intensity=0.6)
    placed = locus(m, failures)
    # The guard: a physics character; the navmesh matches it.
    m.json("world_spawn", shape="group", name="guard", position=[-9, 0.92, -9],
           collider={"shape": "capsule", "radius": 0.3, "height": 1.8}, body={"kind": "kinematic", "character": True})
    m.json("world_spawn", shape="cube", name="guard_body", parent="/guard", scale=[0.55, 1.8, 0.4], color="#3070d0")
    m.json("world_spawn", shape="cube", name="guard_visor", parent="/guard", position=[0, 0.6, 0.22], scale=[0.4, 0.15, 0.1], color="#ffffff", emissive=1.5)
    m.json("world_spawn", shape="cube", name="thief", position=[9, 0.5, 9], scale=[0.45, 1.0, 0.45], color="#d04040")
    print("Navmesh")
    build = build_all(m, like="/guard")
    check(build["polygons"] > 0 and build["tiles_built"] == build["tiles"][0] * build["tiles"][1],
          f"nav_build: {build['polygons']} polygons over {build['tiles_built']} tiles, built over {build['ticks']} ticks", failures)
    s = build["settings"]
    check(abs(s["agent_radius"] - 0.3) < 1e-4 and abs(s["agent_height"] - 1.8) < 1e-4 and abs(s["max_step"] - 0.3) < 1e-4,
          "nav_build like the guard: radius 0.3, height 1.8, step 0.3 (its capsule and controller)", failures)
    again = m.json("nav_build", like="/guard")
    check(again.get("unchanged") is True and again.get("journal") is None, "nav_build with the same settings again changes nothing (no journal entry, no build work)", failures)
    # Review regressions: hostile settings are refused, never a crash or a silent narrowing.
    for bad in [{"cell_size": 0}, {"tile_cells": 200}]:
        err, out = tool(m, "nav_build", name="hostile", **bad)
        check(err and isinstance(out, dict) and out.get("error") in ("invalid_value", "budget"), f"nav_build {bad} is refused ({out.get('error') if isinstance(out, dict) else out})", failures)
    err, out = tool(m, "world_place", spawn={"shape": "cube", "name": "ghost"}, count=4, dry_run=True,
                    relations=[{"rel": "inside", "region": {"min": [-10, -1, -10], "max": [10, 3, 10]}}, {"rel": "avoid", "tag": "no_such_tag", "distance": 1}])
    check(not err, "world_place avoid {tag} with an empty tag set still places", failures)
    # Timing: 50 queries (before the ones the overlay should show).
    t0 = time.perf_counter()
    qms = []
    for i in range(50):
        q = m.json("nav_path", **{"from": [-10 + 0.3 * i % 8, 0, -10 + i * 0.4], "to": [10 - i * 0.2, 0, 10 - i * 0.3]})
        qms.append(q["ms"])
    wall_per_call = (time.perf_counter() - t0) / 50 * 1000
    p = m.json("nav_path", **{"from": [-8, 0, -4], "to": [8, 0, 4]})
    check(p["complete"], f"nav_path west room -> east room: complete ({p['status']})", failures)
    crossings = [a[2] + (b[2] - a[2]) * (0 - a[0]) / (b[0] - a[0]) for a, b in zip(p["corners"], p["corners"][1:]) if (a[0] < 0) != (b[0] < 0)]
    check(len(crossings) == 1 and abs(crossings[0]) < 0.9, f"nav_path crosses the divider in the doorway (at z {crossings}; corners {p['corners']})", failures)
    straight = math.dist([-8, -4], [8, 4])
    check(straight < p["length"] < straight * 1.6, f"nav_path length {p['length']} m (straight line {straight:.2f})", failures)
    unreachable = m.json("nav_path", **{"from": [-8, 0, -4], "to": [0, 2.5, -6.6]})
    check(not unreachable["complete"] and dist_xz(unreachable["corners"][-1], [0, 0, -6.6]) < 1.0,
          f"a goal on top of the divider (too narrow to stand on): {unreachable['status']}, ends beside it", failures)
    # An edit: a crate in the doorway rebuilds only the tiles it touches, and paths change.
    info0 = m.json("nav_info")
    rebuilt0 = info0["navmeshes"][0]["rebuilt_tiles"]
    m.json("world_spawn", shape="cube", name="blocker", position=[0, 1, 0], scale=[0.6, 2, 2.6], color="#c03030", collider={"shape": "box"})
    m.json("time_control", action="step", ticks=8)  # within tiles_per_tick work: a few tiles per tick
    info1 = m.json("nav_info")
    mesh1 = info1["navmeshes"][0]
    touched = mesh1["rebuilt_tiles"] - rebuilt0
    total = mesh1["tiles"][0] * mesh1["tiles"][1]
    check(0 < touched < total and mesh1["dirty_tiles"] == 0, f"the blocker rebuilt {touched} of {total} tiles (dirty left: {mesh1['dirty_tiles']})", failures)
    blocked = m.json("nav_path", **{"from": [-8, 0, -4], "to": [8, 0, 4]})
    check(not blocked["complete"], f"with the doorway blocked the east room is unreachable ({blocked['status']})", failures)
    m.json("world_delete", path="/blocker")
    m.json("time_control", action="step", ticks=3)
    again = m.json("nav_path", **{"from": [-8, 0, -4], "to": [8, 0, 4]})
    check(again["complete"] and abs(again["length"] - p["length"]) < 1e-3, "removing it restores the same path", failures)
    # A beacon placed on the navmesh, reachable from the guard's room.
    beacon = place(m, failures, "beacon", spawn={"shape": "sphere", "name": "beacon", "scale": [0.3, 0.3, 0.3], "color": "#40e0ff", "emissive": 3},
                   relations=[{"rel": "inside", "region": {"min": [2, 0, -6], "max": [11, 2, 6]}}, {"rel": "on_navmesh", "reachable_from": [-9, 0, -9]},
                              {"rel": "avoid", "tag": "pillar", "distance": 1.5}])
    check(bool(beacon) and beacon["satisfied"], "beacon: on the navmesh, reachable from the west room", failures)
    # Review regression: an unreachable spot (the table top, a walkable island) is refused quickly
    # (one flood of the navmesh per call, not a search per candidate).
    t0 = time.perf_counter()
    err, unreach = tool(m, "world_place", spawn={"shape": "sphere", "name": "orb", "scale": [0.2, 0.2, 0.2]}, count=4,
                        relations=[{"rel": "on", "target": "/table"}, {"rel": "on_navmesh", "reachable_from": [-9, 0, -9]}])
    took = time.perf_counter() - t0
    check(err and isinstance(unreach, dict) and unreach.get("error") in ("unsatisfied", "no_free_spot") and took < 5.0,
          f"orbs on the table top, reachable from the west room: refused in {took:.2f} s ({unreach.get('error') if isinstance(unreach, dict) else unreach})", failures)
    # AI: the guard patrols, the thief walks.
    print("AI")
    err, bad = tool(m, "ai_set", **{"path": "/guard", "def": {"root": {"do": "chace", "target": "/thief"}}})
    check(err and bad.get("error") == "invalid_definition" and "chase" in json.dumps(bad), "ai_set refuses a typo with a diagnostic and a hint", failures)
    chk = m.json("ai_check", **{"def": GUARD_MIND})
    check(chk["valid"], "ai_check: the guard's mind is valid", failures)
    m.json("nav_agent", path="/guard", speed=3.0)
    m.json("ai_set", **{"path": "/guard", "def": GUARD_MIND})
    m.json("nav_agent", path="/thief", speed=1.4, goal=[9, 0, -9])
    seen = set()
    track = []
    for step in range(40):
        m.json("time_control", action="step", ticks=15)
        g = pos(m, "/guard")
        track.append(g)
        for i, w in enumerate(WAYPOINTS):
            if dist_xz(g, w) < 0.8:
                seen.add(i)
    check(len(seen) >= 3, f"the guard patrolled: reached waypoints {sorted(seen)}", failures)
    check(all(g[0] < -0.2 for g in track), "the guard stayed in its room while patrolling", failures)
    st = m.json("ai_status", path="/guard")
    running = [r["node"] for r in st["running"]]
    check("patrol" in running, f"ai_status: running {running}", failures)
    mid = save(f"{label}_patrol_top", m.call("sight_frame", overlays=["navmesh", "paths"], **TOP))
    # The thief walks into the west room: the guard sees it and chases.
    m.json("nav_agent", path="/thief", goal=[-6, 0, 2])
    caught = None
    for step in range(60):
        m.json("time_control", action="step", ticks=10)
        g, th = pos(m, "/guard"), pos(m, "/thief")
        if dist_xz(g, th) <= 1.6:
            caught = (step, dist_xz(g, th))
            break
    st = m.json("ai_status", path="/guard")
    trace = json.dumps(st.get("trace", []))
    check(caught is not None, f"the guard caught up with the thief ({caught})", failures)
    check("do chase" in trace, "ai_status's trace shows the chase starting", failures)
    check(st["blackboard"].get("alert") is True, "the mind set alert on its blackboard", failures)
    save(f"{label}_chase", m.call("sight_frame", overlays=["navmesh", "paths"], **PERSPECTIVE))
    # A crowd crossing the east room both ways: pairs walk head-on along the same lane.
    print("Crowd")
    ops = [{"op": "spawn", "shape": "group", "name": "crowd"}]
    for i in range(8):
        z = -5 + (i // 2) * 2.4
        x0, x1 = (2.5, 10.5) if i % 2 == 0 else (10.5, 2.5)
        ops.append({"op": "spawn", "shape": "cube", "parent": "/crowd", "name": f"walker{i}", "position": [x0, 0.5, z], "scale": [0.4, 1, 0.4], "color": "#e0a030",
                    "props": {"nav_agent": {"speed": 2.0, "goal": [x1, 0, z]}}})
    m.json("world_batch", ops=ops)
    closest = 99.0
    for step in range(30):
        m.json("time_control", action="step", ticks=10)
        ws = [w for w in m.json("world_query", path_prefix="/crowd", fields=["path", "position"], limit=20)["items"] if w["path"] != "/crowd"]
        pts = [w["position"] for w in ws]
        for i in range(len(pts)):
            for j in range(i + 1, len(pts)):
                closest = min(closest, dist_xz(pts[i], pts[j]))
    agents = m.json("nav_info", agents=True, limit=64)["agent_list"]
    walkers = [a for a in agents if a["path"].startswith("/crowd/")]
    arrived = sum(1 for a in walkers if a["status"] == "arrived")
    check(arrived == 8, f"crowd: {arrived} of 8 walkers arrived ({[a['status'] for a in walkers]})", failures)
    check(closest > 0.45, f"crowd: walkers kept apart (closest {closest:.2f} m; two radii 0.6)", failures)
    top = save(f"{label}_top", m.call("sight_frame", overlays=["navmesh", "paths"], **TOP))
    ov = top.get("overlays", {})
    drawn_polys = sum(n["drawn"] for n in ov.get("navmesh", {}).get("navmeshes", []))
    check(drawn_polys > 0 and ov.get("navmesh", {}).get("omitted_polygons") == 0, f"the top capture draws the navmesh ({drawn_polys} polygons)", failures)
    check(len(ov.get("paths", {}).get("agents", [])) >= 9, f"the top capture draws the agents' paths ({len(ov.get('paths', {}).get('agents', []))} agents)", failures)
    check(len(mid.get("overlays", {}).get("paths", {}).get("agents", [])) >= 2, "the patrol capture draws the guard and the thief", failures)
    # Review regression: a navmesh deleted between ticks, then queried, never takes the engine down.
    m.json("nav_build", name="temp")
    # /nav is reserved for the nav tools: world_delete is refused, nav_build remove deletes it.
    err_del, refused = tool(m, "world_delete", path="/nav/temp")
    check(err_del and isinstance(refused, dict) and refused.get("error") == "reserved", "world_delete on /nav is refused (reserved)", failures)
    m.json("nav_build", name="temp", remove=True)
    err, gone = tool(m, "nav_path", mesh="temp", **{"from": [-8, 0, -4], "to": [8, 0, 4]})
    m.json("time_control", action="step", ticks=1)
    err2, gone2 = tool(m, "nav_path", mesh="temp", **{"from": [-8, 0, -4], "to": [8, 0, 4]})
    check(not tool(m, "engine_info")[0] and err2 and "no_navmesh" in json.dumps(gone2),
          "a deleted navmesh: queries answer from the last tick, then 'no_navmesh'; the engine lives", failures)
    feats = features(m, failures)
    # A load holds the clock while its navmeshes build, and time_control step waits for it (never
    # queued behind it, never answering the old tick).
    m.json("world_save", name="navload", overwrite=True)
    t_before = m.json("time_control", action="info")["tick"]
    m.json("world_load", name="navload", force=True)
    err_step, stepped = tool(m, "time_control", action="step", ticks=3)
    stepped_ok = (not err_step and stepped.get("tick") == t_before + 3) or (err_step and isinstance(stepped, dict) and stepped.get("error") == "loading")
    check(stepped_ok, f"time_control step right after world_load: {stepped.get('tick') if not err_step else stepped.get('error')} (from tick {t_before})", failures)
    check(not m.json("nav_info").get("loading"), "the load finished", failures)
    # Determinism and journal.
    hashes = m.json("world_hash", recent=1, verify=True)
    check(hashes["verify"]["matches"], "the journal replays the world exactly (nav writes included)", failures)
    check("nav" in hashes, "world_hash reports the nav state hash", failures)
    info = m.json("nav_info")
    perf = m.json("perf_frame")
    check(perf.get("nav") is not None, "perf_frame has a nav section", failures)
    out = {
        "world_hash": hashes["state_hash"],
        "physics_hash": hashes.get("physics", {}).get("state_hash"),
        "nav_hash": hashes["nav"]["state_hash"],
        "build_ms": build["ms"],
        "polygons": build["polygons"],
        "tiles": build["tiles"],
        "path_ms_mean": sum(qms) / len(qms),
        "path_ms_max": max(qms),
        "path_call_ms": wall_per_call,
        "rebuild_tiles_for_blocker": touched,
        "nav_tick_ms": info["tick_ms"],
        "caught_at_step": caught,
        "features": feats,
    }
    if big:
        out["big"] = big_build(m, failures)
    (OUT / f"{label}_info.json").write_text(json.dumps(info, indent=2))
    return out


def features(m, failures):
    """Off-mesh links, areas and fitted model colliders, on two platforms with a 4 m gap at x 300."""
    print("Links, areas, model colliders")
    m.json("world_batch", ops=[
        {"op": "spawn", "shape": "cube", "name": "plat_w", "position": [294, -0.5, 0], "scale": [8, 1, 12], "color": "#8a8f98", "collider": {"shape": "box"}},
        {"op": "spawn", "shape": "cube", "name": "plat_e", "position": [306, -0.5, 0], "scale": [8, 1, 12], "color": "#8a8f98", "collider": {"shape": "box"}},
        # A pool on the west platform (x 290..296), marked as water by a volume.
        {"op": "spawn", "shape": "cube", "name": "pool", "position": [293, 0.01, 0], "scale": [6, 0.02, 3], "color": "#2a5a9a"},
        {"op": "spawn", "shape": "group", "name": "pool_area", "position": [293, 0, 0], "scale": [6, 1, 3], "props": {"nav_area": {"area": "water", "volume": True}}},
        {"op": "spawn", "shape": "model", "name": "tomb", "recipe": "recipes/tombstone.json", "position": [306, 0, 2]},
    ])
    links = [{"from": [297.6, 0, 0], "to": [302.4, 0, 0], "kind": "jump"}]
    areas = [{"name": "water", "cost": 5, "flags": ["wet"]}]
    b = build_all(m, name="gap", bounds={"min": [288, -1, -7], "max": [312, 4, 7]}, links=links, areas=areas)
    check(b.get("offmesh_links_found") == 1, f"nav_build with a jump link: found on the navmesh ({b.get('offmesh_links_found')} of 1)", failures)
    p = m.json("nav_path", mesh="gap", **{"from": [292, 0, -4], "to": [308, 0, 4]})
    check(p["complete"] and [o["kind"] for o in p.get("offmesh", [])] == ["jump"], f"nav_path across the gap: {p['status']}, over {p.get('offmesh')}", failures)
    dry = m.json("nav_path", mesh="gap", avoid_areas=["water"], **{"from": [293, 0, -4.5], "to": [293, 0, 4.5]})
    check(dry["complete"] and all(not (290 < c[0] < 295.9 and abs(c[2]) < 1.5) for c in dry["corners"]) and any(c[0] > 295.9 for c in dry["corners"]),
          f"avoid_areas water: the path goes round the pool ({dry['length']} m, {dry['corner_count']} corners)", failures)
    # An agent jumps the gap.
    m.json("world_spawn", shape="cube", name="jumper", position=[292, 0.5, -3], scale=[0.5, 1, 0.5], color="#e0c040",
           props={"nav_agent": {"mesh": "gap", "speed": 3, "goal": [308, 0, -3]}})
    high = 0.0
    for _ in range(20):
        m.json("time_control", action="step", ticks=15)
        high = max(high, pos(m, "/jumper")[1])
    j = pos(m, "/jumper")
    check(dist_xz(j, [308, 0, -3]) < 0.6 and high > 0.8, f"an agent jumped the gap in an arc (highest {high:.2f} m) and arrived at {j}", failures)
    # A model gets a fitted collider and carves the navmesh.
    for _ in range(100):
        e = m.json("world_query", path_prefix="/tomb", fields=["path", "model_state"], limit=1)["items"]
        if e and e[0].get("model_state") == "ready":
            break
        time.sleep(0.05)
    before = m.json("nav_point", mesh="gap", at=[306, 0, 2])
    c = m.json("model_collider", path="/tomb", kind="auto", moving=True)
    check(c["shape"] == "hull" and 4 <= c["points"] <= 16, f"model_collider: a hull of {c['points']} points for a moving body", failures)
    # F1: a mesh collider is answered with a summary, not its triangles.
    err_mesh, raw = tool(m, "model_collider", path="/tomb", kind="mesh", moving=False)
    size = len(json.dumps(raw))
    check(not err_mesh and raw.get("triangles", 0) > 0 and size < 2048, f"model_collider kind mesh answers in {size} bytes ({raw.get('triangles') if isinstance(raw, dict) else raw} triangles)", failures)
    c = m.json("model_collider", path="/tomb")
    m.json("time_control", action="step", ticks=10)
    after = m.json("nav_point", mesh="gap", at=[306, 0, 2])
    check(before["on_navmesh"] and not after["on_navmesh"], f"the fitted collider ({c['shape']}, {c.get('points')} points) carves the navmesh under the tombstone", failures)
    # A picture: the link's arc, the water area's colour, the carved tombstone, the query paths.
    m.json("nav_path", mesh="gap", **{"from": [292, 0, -4], "to": [308, 0, 4]})
    m.json("nav_path", mesh="gap", avoid_areas=["water"], **{"from": [293, 0, -4.5], "to": [293, 0, 4.5]})
    shot = save("features_top", m.call("sight_frame", overlays=["navmesh", "paths"], ortho={"height_m": 16}, eye=[300, 30, 0.01], target=[300, 0, 0], width=1200, height=800))
    # Oblique views: the link's arc rising over the gap (straight down it lies under the query), and the tombstone's hole.
    save("features_link", m.call("sight_frame", overlays=["navmesh"], eye=[297, 5, -8], target=[300, 0.6, 0], width=1200, height=800))
    save("features_tomb", m.call("sight_frame", overlays=["navmesh", "paths"], eye=[303, 5, 7], target=[306, 0, 2], width=1200, height=800))
    nm = next((n for n in shot.get("overlays", {}).get("navmesh", {}).get("navmeshes", []) if n["path"] == "/nav/gap"), {})
    check(len(nm.get("offmesh_links", [])) == 1 and nm["offmesh_links"][0]["found"] and [a["name"] for a in nm.get("areas", [])] == ["water"],
          "the features capture lists the link (found) and the water area", failures)
    return {"link_path_m": p["length"], "dry_path_m": dry["length"], "jump_high_m": round(high, 3), "tomb_hull_points": c.get("points")}


def big_build(m, failures):
    """A 100 m x 100 m field of 400 boxes, for build timing."""
    ops = [{"op": "spawn", "shape": "cube", "name": "big_floor", "position": [200, -0.5, 0], "scale": [100, 1, 100], "collider": {"shape": "box"}}]
    for i in range(400):
        x, z = 155 + (i % 20) * 4.5, -45 + (i // 20) * 4.5
        ops.append({"op": "spawn", "shape": "cube", "name": f"big_box{i}", "position": [x, 0.75, z], "scale": [1.5, 1.5, 1.5], "collider": {"shape": "box"}})
    m.json("world_batch", ops=ops)
    b = build_all(m, name="big", bounds={"min": [150, -1, -50], "max": [250, 4, 50]})
    ms = []
    for i in range(20):
        q = m.json("nav_path", mesh="big", **{"from": [152, 0, -48 + i], "to": [248, 0, 48 - i]})
        ms.append(q["ms"])
        if i == 0:
            check(q["complete"], f"big: a path across 100 m through 400 boxes ({q['corner_count']} corners)", failures)
    print(f"  big: {b['polygons']} polygons, {b['tiles_built']} tiles over {b['ticks']} ticks ({b['ms']} ms wall, paused and stepped); paths {sum(ms) / len(ms):.3f} ms mean")
    return {"build_ms": b["ms"], "polygons": b["polygons"], "tiles": b["tiles_built"], "path_ms_mean": sum(ms) / len(ms), "path_ms_max": max(ms)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7484)
    ap.add_argument("--adapter", default="discrete")
    ap.add_argument("--windowed", action="store_true")
    ap.add_argument("--big", action="store_true")
    ap.add_argument("--exe", default=str(TARGET / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")))
    ap.add_argument("--out", default=None, help="output folder (default <target>/nav-check)")
    a = ap.parse_args()
    global OUT
    if a.out:
        OUT = Path(a.out)
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    failures = []
    runs = []
    for i in range(2):
        args = ["--adapter", a.adapter, "--captures", str(OUT / "captures"), "--worlds", str(OUT / f"worlds{i}"), "--assets", str(ROOT / "games" / "sepulcra" / "assets")]
        if not a.windowed:
            args.append("--headless")
        with Engine(a.exe, a.port + i, OUT / f"engine{i}.log", *args, cwd=str(OUT)) as eng:
            with eng.mcp("nav-check") as m:
                runs.append(run(m, failures, f"run{i}", a.big and i == 0))
        errs = eng.validation_errors()
        check(errs in (0, None), f"run{i}: clean shutdown ({errs} validation errors)", failures)
    same = all(runs[0][k] == runs[1][k] for k in ("world_hash", "physics_hash", "nav_hash"))
    check(same, f"two runs end in the same world, physics and nav hashes ({runs[0]['world_hash']} / {runs[0]['physics_hash']} / {runs[0]['nav_hash']})", failures)
    (OUT / "summary.json").write_text(json.dumps({"runs": runs, "failures": failures}, indent=2))
    print(json.dumps({k: v for k, v in runs[0].items() if k not in ("world_hash", "physics_hash", "nav_hash")}, indent=1))
    print("PASS" if not failures else f"FAIL ({len(failures)})")
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
