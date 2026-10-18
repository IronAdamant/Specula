#!/usr/bin/env python3
"""Joints and ragdolls live check (physics P4, animation A4): builds joints over MCP, steps
them, asserts how they hold with physics_check, lists them with physics_joints, undoes one,
captures the joints overlay; then ragdolls a walking Sepulcra shambler (limp with a knock,
off again, powered with a hit) and saves a filmstrip of each, with ragdoll_info's checks.

The joint scene: a door hinged on a post, opened by its motor to its 100 degree limit; a chain
of eight links hanging from a beam; a ball on a 3 m rope swinging down; a crate sliding down a
rail to its end; two boxes welded together falling as one. Time is paused and stepped exactly,
so two runs (two engine processes) end in the same world and physics hashes.

    cargo build -p specula
    python scripts/joints_check.py                        # ports 7488-7489, headless
    python scripts/joints_check.py --adapter integrated --windowed

Everything goes to <target>/joints-check/ (CARGO_TARGET_DIR when set): a PNG and a JSON packet
per capture, ragdoll filmstrips (ragdoll_<case>_<frame>.png), and summary.json. LOOK at the
PNGs (and have the sight-skeptic check the claims) before calling it good.
Standard library only; output is ASCII.
"""
import argparse
import base64
import json
import os
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

TARGET = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
OUT = TARGET / "joints-check"
ASSETS = ROOT / "games" / "sepulcra" / "assets"
JOINT_VIEW = {"eye": [2, 5.5, 14], "target": [0, 2.5, -1], "width": 1280, "height": 720}
RAGDOLL_VIEW = {"eye": [3.4, 1.6, 9.6], "target": [0, 0.5, 5.6], "width": 960, "height": 540}


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


def tool(m, _tool, **args):
    """(is_error, parsed or text) of a tool call, without raising on tool errors."""
    r = m.request("tools/call", {"name": _tool, "arguments": args})
    text = next(c["text"] for c in r["content"] if c["type"] == "text")
    try:
        data = json.loads(text)
    except ValueError:
        data = text
    return bool(r.get("isError")), data


def entity(m, path):
    """An entity's fields (world_query on its path)."""
    q = m.json("world_query", path_prefix=path, fields=["id", "path", "position", "world_position"], limit=8)
    items = q.get("entities") or q.get("items") or []
    return next((e for e in items if e.get("path") == path), {})


def solid(name, pos, scale, color, **extra):
    return {"op": "spawn", "shape": "cube", "name": name, "position": pos, "scale": scale, "color": color, "collider": {"shape": "box"}, **extra}


def scene_ops():
    ops = [
        solid("floor", [0, -0.5, 0], [40, 1, 40], "#3a3f47"),
        # The door: a post and a slab beside it.
        solid("post", [-6, 1.25, 0], [0.2, 2.5, 0.2], "#6b6f78"),
        solid("door", [-5.45, 1.2, 0], [0.9, 2.2, 0.08], "#8a6a48",
              body={"kind": "dynamic", "mass": 15, "angular_damping": 0.5}),
        # The chain: a beam and eight links laid out level from it (they swing down).
        solid("beam", [-0.2, 6, -6], [0.4, 0.2, 0.2], "#6b6f78"),
        {"op": "spawn", "shape": "group", "name": "chain"},
    ]
    for i in range(8):
        ops.append(solid(f"l{i}", [0.2 + 0.4 * i, 6, -6], [0.36, 0.08, 0.08], "#b8b0a0", parent="/chain",
                         body={"kind": "dynamic", "mass": 1, "linear_damping": 1.0, "angular_damping": 2.0}))
    ops += [
        # The rope: a ball held out level, 3 m from its anchor.
        {"op": "spawn", "shape": "sphere", "name": "ball", "position": [7, 6, -2], "scale": [0.4, 0.4, 0.4], "color": "#c05040",
         "collider": {"shape": "sphere", "radius": 0.5}, "body": {"kind": "dynamic", "mass": 3}},
        # The slider: a crate on a 45 degree rail.
        solid("crate", [5, 3, 4], [0.4, 0.4, 0.4], "#a08050", body={"kind": "dynamic", "mass": 4}),
        # The weld: two overlapping boxes.
        solid("weld_a", [-3, 3, 5], [0.6, 0.6, 0.6], "#7090b0", body={"kind": "dynamic", "mass": 2}),
        solid("weld_b", [-2.6, 3.3, 5], [0.6, 0.6, 0.6], "#7090b0", body={"kind": "dynamic", "mass": 2}),
    ]
    return ops


def joints(m, failures, label):
    made = []
    for args in [
        {"kind": "hinge", "a": "/post", "b": "/door", "anchor": [-5.9, 1.2, 0], "axis": [0, 1, 0], "limit": [0, 100],
         "motor": {"speed": 60, "max_force": 120}},
        {"kind": "ball", "a": "/beam", "b": "/chain/l0", "anchor": [0, 6, -6], "axis": [1, 0, 0]},
        *[{"kind": "ball", "a": f"/chain/l{i - 1}", "b": f"/chain/l{i}", "anchor": [0.4 * i, 6, -6], "axis": [1, 0, 0]} for i in range(1, 8)],
        {"kind": "distance", "a": None, "b": "/ball", "anchor": [4, 6, -2], "anchor_b": [7, 6, -2], "length": [0, 3]},
        {"kind": "prismatic", "a": None, "b": "/crate", "anchor": [5, 3, 4], "axis": [0.7071, -0.7071, 0], "limit": [-0.2, 1.5]},
        {"kind": "fixed", "a": "/weld_a", "b": "/weld_b", "anchor": [-2.8, 3.15, 5]},
    ]:
        err, r = tool(m, "physics_joint", **args)
        check(not err, f"{label}: physics_joint {args['kind']} {args['b']}" + ("" if not err else f": {r}"), failures)
        if not err:
            made.append(r)
    # Mistakes come back as data that says what to change.
    err, r = tool(m, "physics_joint", kind="hinge", b="/door", limit=[50, 10])
    check(err and "invalid_joint" in json.dumps(r) and "low <= high" in json.dumps(r), f"{label}: a bad limit is refused with why", failures)
    err, r = tool(m, "physics_joint", kind="ball", b="/floor_missing")
    check(err and "not_found" in json.dumps(r), f"{label}: an unknown body is refused", failures)
    err, r = tool(m, "physics_joint", kind="ball", a=None, b="/beam", anchor=[0, 6, -6])
    check(not err and "note" in r, f"{label}: a joint holding no dynamic body says so ({r.get('note') if isinstance(r, dict) else r})", failures)
    if not err:
        tool(m, "physics_joint", id=r["id"], remove=True)
    return made


def run(m, failures, label):
    m.json("time_control", action="pause")
    reply = m.json("world_batch", ops=scene_ops())
    check("error" not in reply, f"{label}: scene built", failures)
    made = joints(m, failures, label)
    m.json("time_control", action="step", ticks=1)
    save(f"{label}_joints_start", m.call("sight_frame", overlays=["joints", "colliders"], **JOINT_VIEW))
    for _ in range(10):
        m.json("time_control", action="step", ticks=30)
    listing = m.json("physics_joints", limit=64)
    (OUT / f"{label}_joints.json").write_text(json.dumps(listing, indent=2))
    by_path = {j["path"]: j for j in listing["joints"]}
    check(listing["total"] == len(made) == 12, f"{label}: physics_joints lists the {listing['total']} joints made", failures)
    door = next((j for j in listing["joints"] if j["b"]["path"] == "/door"), {})
    check(abs(door.get("angle_deg", 0) - 100) < 3 and door.get("past_limit_deg", 9) < 2,
          f"{label}: the door's motor opened it to its 100 degree limit ({door.get('angle_deg')} deg, {door.get('past_limit_deg')} past)", failures)
    rope = next((j for j in listing["joints"] if j["b"]["path"] == "/ball"), {})
    check(2.9 < rope.get("length_m", 0) < 3.03, f"{label}: the rope went taut at 3 m ({rope.get('length_m')})", failures)
    slider = next((j for j in listing["joints"] if j["b"]["path"] == "/crate"), {})
    check(abs(slider.get("offset_m", 0) - 1.5) < 0.03 and slider.get("linear_error_m", 1) < 0.01,
          f"{label}: the crate slid to the rail's end ({slider.get('offset_m')} m, {slider.get('linear_error_m')} m off it)", failures)
    weld = next((j for j in listing["joints"] if j["b"]["path"] == "/weld_b"), {})
    check(weld.get("linear_error_m", 1) < 0.01 and weld.get("angular_error_deg", 9) < 1, f"{label}: the weld holds ({weld.get('linear_error_m')} m, {weld.get('angular_error_deg')} deg)", failures)
    tp = entity(m, "/chain/l7").get("world_position")
    check(tp and abs(tp[0]) < 0.6 and tp[1] < 3.4, f"{label}: the chain hangs down from the beam (last link at {tp})", failures)
    # The weld lands hard at about tick 23 (its joint stretches a few cm for a tick): the joint
    # check covers the window after everything has landed; the transient is in the summary.
    landing = m.json("physics_check", checks=[{"check": "joint_error", "max_m": 1, "max_deg": 90}])["results"][0]
    print(f"  {label}: worst joint error over the whole run: {landing.get('worst_m')} m, {landing.get('worst_deg')} deg (the weld's landing)")
    settled = m.json("time_control", action="info")["tick"] - 200
    checks = m.json("physics_check", since_tick=settled, checks=[
        {"check": "joint_error", "max_m": 0.02, "max_deg": 3},
        {"check": "joint_limits", "max_deg": 3, "max_m": 0.02},
        {"check": "at_rest", "entities": "/chain", "max_speed": 0.1},
        {"check": "no_penetration", "max_depth": 0.03},
    ])
    (OUT / f"{label}_checks.json").write_text(json.dumps(checks, indent=2))
    for r in checks["results"]:
        check(r["pass"], f"{label}: physics_check {r['check']} " + json.dumps({k: v for k, v in r.items() if k not in ("check", "pass")})[:200], failures)
    end = save(f"{label}_joints_end", m.call("sight_frame", overlays=["joints"], **JOINT_VIEW))
    jo = end.get("overlays", {}).get("joints", {})
    check(jo.get("drawn", 0) == 12 and jo.get("holding", 0) >= 11, f"{label}: the joints overlay drew {jo.get('drawn')} joints ({json.dumps({k: v for k, v in jo.items() if k != 'legend'})})", failures)
    explain = m.json("physics_explain", path="/door")
    check(len(explain.get("joints", [])) == 1, f"{label}: physics_explain lists the door's joint", failures)
    # Undo: the slider's joint goes, and the crate falls off the rail.
    slider = next(r for r in made if r["joint"]["kind"] == "prismatic")
    err, undo = tool(m, "world_undo", ids=[slider["journal"]])
    check(not err, f"{label}: world_undo of the slider's joint" + ("" if not err else f": {undo}"), failures)
    m.json("time_control", action="step", ticks=30)
    after = m.json("physics_joints", limit=64)
    check(after["total"] == 11, f"{label}: the slider's joint is gone after the undo ({after['total']} joints)", failures)
    hashes = m.json("world_hash", recent=1, verify=True)
    check(hashes["verify"]["matches"], f"{label}: the journal replays the world exactly", failures)
    return {"world_hash": hashes["state_hash"], "physics_hash": hashes["physics"]["state_hash"]}


def frames(m, name, ticks, overlays_at=()):
    """Steps and captures the ragdoll view at each tick count of `ticks` (relative), with the
    joints and colliders overlays at the ones in `overlays_at`. Returns the packets."""
    out = []
    done = 0
    for t in ticks:
        if t > done:
            m.json("time_control", action="step", ticks=t - done)
            done = t
        ov = ["joints", "colliders"] if t in overlays_at else []
        out.append(save(f"ragdoll_{name}_{t:03d}", m.call("sight_frame", overlays=ov, **RAGDOLL_VIEW)))
    return out


def ragdolls(m, failures):
    m.json("time_control", action="pause")
    m.json("world_batch", ops=[solid("floor", [0, -0.5, 0], [40, 1, 40], "#3a3f47")])
    m.call("model_build", recipe="recipes/shambler.json")
    shambler = {"shape": "model", "recipe": "recipes/shambler.json", "scale": [1.2, 1.2, 1.2], "anim": {"graph": "creature", "params": {"act": "move", "speed": 0.9}}}
    m.json("world_spawn", name="shambler", position=[0, 0, 5], **shambler)
    m.json("time_control", action="step", ticks=20)
    # Parts fitted to the mesh (written as data, before it goes limp): convex hulls.
    fit = m.json("ragdoll_set", path="/shambler", fit="hulls")
    radii = fit.get("ragdoll", {}).get("radii", {})
    hulls = fit.get("ragdoll", {}).get("hulls", {})
    print(f"  ragdoll: fitted radii {json.dumps(radii)}")
    print(f"  ragdoll: fitted hulls for {sorted(hulls)} ({sum(len(p) for p in hulls.values())} points)")
    check(len(radii) >= 12, f"ragdoll: fit measured {len(radii)} radii from the mesh", failures)
    check(len(hulls) >= 12 and all(4 <= len(p) <= 16 for p in hulls.values()), f"ragdoll: fit measured {len(hulls)} hulls of 4..16 points", failures)
    # Limp, knocked backwards (it faces +Z).
    r = m.json("ragdoll_set", path="/shambler", mode="limp", hit={"bone": "chest", "impulse": [0, 12, -80]})
    check(r.get("graph_param_ragdoll") is True, f"ragdoll: ragdoll_set sets the graph's ragdoll parameter ({r.get('graph_param_ragdoll')})", failures)
    limp = frames(m, "limp", [1, 3, 6, 9, 12, 16, 22, 30, 45, 75], overlays_at=(9, 75))
    # Its joints page through physics_joints by cursor, and naming the shambler selects them.
    listed, cursor, pages = [], None, 0
    while pages < 20:
        args = {"ragdolls": True, "limit": 4}
        if cursor:
            args["cursor"] = cursor
        page = m.json("physics_joints", **args)
        listed += page["joints"]
        cursor, pages = page.get("next_cursor"), pages + 1
        if not cursor:
            break
    check(len(listed) == page["total"] and len(listed) >= 12, f"ragdoll: its {len(listed)} joints page through 4 at a time ({pages} pages)", failures)
    own = m.json("physics_joints", entity="/shambler")
    check(own["total"] == len(listed), f"ragdoll: physics_joints entity=/shambler lists its ragdoll's joints ({own['total']})", failures)
    info = m.json("ragdoll_info", path="/shambler")
    (OUT / "ragdoll_limp_info.json").write_text(json.dumps(info, indent=2))
    check(info.get("active") and len(info.get("parts", [])) >= 12, f"ragdoll: {len(info.get('parts', []))} parts, {len(info.get('joints', []))} joints", failures)
    worst = max((j.get("linear_error_m", 0) for j in info.get("joints", [])), default=1)
    check(worst < 0.03, f"ragdoll: its joints hold ({worst} m worst gap)", failures)
    past = max((j.get("past_limit_deg", 0) for j in info.get("joints", [])), default=99)
    check(past < 4, f"ragdoll: within its joint limits ({past} deg worst)", failures)
    asleep = all(p.get("sleeping") or abs(sum(v * v for v in p.get("velocity", [0, 0, 0]))) < 0.04 for p in info.get("parts", []))
    check(asleep, "ragdoll: it came to rest", failures)
    ch = info.get("checks", {})
    print(f"  ragdoll checks ({ch.get('measured_by')}): " + json.dumps(ch)[:600])
    names = {c.get("check"): c for c in ch.get("checks", [])}
    for name in ("ground_penetration", "floating", "pops"):
        c = names.get(name)
        check(c is not None and c.get("passed"), f"ragdoll: motion check {name} {c and (c.get('value'), c.get('limit'), c.get('unit'))}", failures)
    subj = next((s for s in limp[-1].get("subjects", []) if s.get("path") == "/shambler"), {})
    check(subj.get("on_screen"), "ragdoll: the shambler is on screen at the end", failures)
    # Off: it moves under its pelvis and blends back to walking.
    pos0 = entity(m, "/shambler").get("position")
    m.json("ragdoll_set", path="/shambler", mode="off")
    frames(m, "getup", [1, 4, 8, 14, 24], overlays_at=())
    pos1 = entity(m, "/shambler").get("position")
    check(pos0 != pos1, f"ragdoll: off moved it under its pelvis ({pos0} -> {pos1})", failures)
    gone = m.json("physics_joints", ragdolls=True)
    check(gone["total"] == 0, f"ragdoll: off removed its joints ({gone['total']} left)", failures)
    # Powered, with a blow to the reaching arm.
    m.json("ragdoll_set", path="/shambler", mode="powered", strength=0.6)
    m.json("time_control", action="step", ticks=15)
    m.json("ragdoll_set", path="/shambler", hit={"bone": "forearm_l", "impulse": [0, 18, -18]})
    frames(m, "powered", [1, 3, 6, 10, 20, 40], overlays_at=(3,))
    pinfo = m.json("ragdoll_info", path="/shambler")
    (OUT / "ragdoll_powered_info.json").write_text(json.dumps(pinfo, indent=2))
    check(pinfo.get("mode") == "powered" and pinfo.get("active"), "ragdoll: powered and active", failures)
    err, bad = tool(m, "ragdoll_set", path="/floor", mode="limp")
    check(err and "not_a_model" in json.dumps(bad), "ragdoll: a non-model is refused with why", failures)
    err, bad = tool(m, "ragdoll_set", path="/shambler", hit={"bone": "tail", "impulse": [0, 1, 0]})
    check(err and "unknown_bone" in json.dumps(bad), "ragdoll: an unknown bone is refused, listing the bones", failures)
    return {"limp_checks": ch, "parts": len(info.get("parts", []))}


def claims_probes(eng, failures):
    """Review findings 1 and 2: a joint moves the bodies it links, so mallory can't tie alice's
    claimed boxes (path or region claim) with physics_joint, nor write props.joint with world
    tools at all."""
    alice, mal = eng.mcp("alice"), eng.mcp("mallory")
    alice.hello("alice")
    mal.hello("mallory")
    alice.json("time_control", action="pause")
    alice.json("world_spawn", shape="cube", name="cfloor", position=[0, -0.5, 30], scale=[40, 1, 40], collider={"shape": "box"})
    a = alice.json("world_spawn", shape="cube", name="abox", position=[0, 0.5, 30], collider={"shape": "box"}, body={"kind": "dynamic", "mass": 5})
    r = alice.json("world_spawn", shape="cube", name="rbox", position=[5, 0.5, 35], collider={"shape": "box"}, body={"kind": "dynamic", "mass": 5})
    aid, rid = a["id"], r["id"]
    alice.json("claim_take", path="/abox", lease_s=600)
    alice.json("claim_take", region={"min": [3, -1, 33], "max": [7, 4, 37]}, lease_s=600)
    err, out = tool(mal, "physics_joint", kind="fixed", b=aid, anchor=[0, 6, 30])
    check(err and "claimed" in json.dumps(out), "claims: physics_joint on a path-claimed body is refused", failures)
    mal.json("world_spawn", shape="group", name="mg", position=[20, 0, 20])
    err, out = tool(mal, "physics_joint", kind="distance", b=rid, anchor=[5, 8, 35], anchor_b=[5, 0.5, 35], length=[0, 0.5], parent="/mg")
    check(err and "claimed" in json.dumps(out), "claims: physics_joint on a region-claimed body is refused", failures)
    j = {"joint": {"kind": "distance", "a": None, "b": aid, "anchor_a": [0, 8, 0], "anchor_b": [0, 0, 0], "length": [0, 0.5]}}
    err, out = tool(mal, "world_spawn", shape="group", name="mj", position=[20, 0, 20], props=j)
    check(err and "reserved" in json.dumps(out), "claims: props.joint through world_spawn is reserved for physics_joint", failures)
    err, out = tool(mal, "world_set", path="/mg", props=j)
    check(err and "reserved" in json.dumps(out), "claims: props.joint through world_set is reserved", failures)
    mal.json("time_control", action="step", ticks=60)
    for path in ("/abox", "/rbox"):
        y = entity(alice, path).get("position", [0, 99, 0])[1]
        check(abs(y - 0.5) < 0.05, f"claims: alice's {path} stayed put (y {y})", failures)
    joints = mal.json("physics_joints")
    check(joints["total"] == 0, f"claims: no joint got in ({joints['total']})", failures)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7488)
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
        args = ["--adapter", a.adapter, "--captures", str(OUT / "captures"), "--worlds", str(OUT / f"worlds{i}"), "--assets", str(ASSETS), "--size", "1280x720"]
        if not a.windowed:
            args.append("--headless")
        with Engine(a.exe, a.port, OUT / f"engine{i}.log", *args, cwd=str(OUT)) as eng:
            with eng.mcp("joints-check") as m:
                runs.append(run(m, failures, f"run{i}"))
        errs = eng.validation_errors()
        check(errs in (0, None), f"run{i}: clean shutdown ({errs} validation errors)", failures)
    check(runs[0] == runs[1], f"two runs end in the same world and physics hashes ({runs[0]})", failures)
    args = ["--adapter", a.adapter, "--captures", str(OUT / "captures"), "--worlds", str(OUT / "worlds-ragdoll"), "--assets", str(ASSETS), "--size", "1280x720"]
    if not a.windowed:
        args.append("--headless")
    with Engine(a.exe, a.port + 1, OUT / "engine-ragdoll.log", *args, cwd=str(OUT)) as eng:
        with eng.mcp("ragdoll-check") as m:
            rag = ragdolls(m, failures)
        claims_probes(eng, failures)
    check(eng.validation_errors() in (0, None), "ragdoll: clean shutdown", failures)
    (OUT / "summary.json").write_text(json.dumps({"runs": runs, "ragdoll": rag, "failures": failures}, indent=2))
    print("PASS" if not failures else f"FAIL ({len(failures)})")
    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
