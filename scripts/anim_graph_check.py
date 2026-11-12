#!/usr/bin/env python3
"""Animation stage A3 live check: Sepulcra's creature graphs through the anim_* tools.

Starts the `sepulcra` binary (release) windowed on its own port, starts a run and lets a wave
come in, then, the way an agent would:
  - anim_get: every creature has its 'creature' graph and clips,
  - anim_check: every creature's clips, graph states and act transitions pass style.toml's
    [motion] rules (foot sliding, floating, penetration, joint limits, pops, drift, seams),
  - anim_filmstrip: a contact sheet per creature (clips, states, a transition) with the checks
    attached, and one of a live creature in the world (feet on the slab),
  - anim_state: a live creature's graph state, events, root motion and feet IK on the ground,
  - anim_set: a graph patch and a keyframed clip written live as world anim assets, journaled,
    seen by anim_get, checked, then undone with world_undo; a broken graph is refused with a
    path,
  - zero validation errors at shutdown.

    cargo build -p sepulcra-app --release
    python scripts/anim_graph_check.py [--adapter integrated] [--port 7489]

Output (sheets, packets, engine log, summary.json) goes to target/anim-graph-check/<adapter>/.
LOOK AT THE PNGs before claiming anything about them. Standard library only; ASCII output.
"""
import argparse
import json
import os
import sys
from pathlib import Path

from specula_mcp import ROOT, Engine, detailed

TARGET = Path(os.environ["CARGO_TARGET_DIR"]) if os.environ.get("CARGO_TARGET_DIR") else ROOT / "target"
EXE = TARGET / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")
CREATURES = [("recipes/shambler.json", 0.8), ("recipes/skeleton_runner.json", 2.2), ("recipes/wraith.json", 1.4), ("recipes/bone_colossus.json", 0.4)]


def fail(msg):
    raise SystemExit(f"FAILED: {msg}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7489)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--exe", default=str(EXE))
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    exe = Path(args.exe)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p sepulcra-app --release")
    out = Path(args.out) if args.out else TARGET / "anim-graph-check" / args.adapter
    out.mkdir(parents=True, exist_ok=True)
    extra = ["--adapter", args.adapter, "--captures", str(out / "captures"), "--size", "1280x720", "--", "--seed", "7", "--mute", "--title", "off"]
    summary = {"adapter": args.adapter}
    with Engine(exe, args.port, out / "engine.log", *extra, cwd=out) as eng:
        mcp = eng.mcp("anim-check")
        mcp.hello("anim-check", role="animator")

        def raw(tool, **a):
            return mcp.request("tools/call", {"name": tool, "arguments": detailed(tool, a)})

        tools = mcp.tools()
        anim_tools = sorted(t for t in tools if t.startswith("anim_"))
        print(f"anim tools: {', '.join(anim_tools)}")
        if anim_tools != ["anim_check", "anim_filmstrip", "anim_get", "anim_set", "anim_state"]:
            fail(f"anim tools {anim_tools}")
        summary["gpu"] = mcp.json("engine_info")["gpu"]["name"]
        mcp.call("time_control", action="pause")
        mcp.json("play_new", seed=7)
        # Let creatures come: call waves and step until a few walk the lanes.
        for _ in range(40):
            st = mcp.json("play_state")
            if st.get("enemies", 0) >= 4:
                break
            raw("play_command", action="call_wave")
            mcp.call("time_control", action="step", ticks=150)
        st = mcp.json("play_state")
        print(f"  wave {st.get('wave')} enemies {st.get('enemies')} tick {st.get('tick')}")

        # anim_get and anim_check for every creature.
        checks = {}
        for recipe, _ in CREATURES:
            mcp.call("model_build", recipe=recipe)  # creatures not on the slab yet build now
        for recipe, speed in CREATURES:
            g = mcp.json("anim_get", recipe=recipe)
            graphs = [x["name"] for x in g["graphs"]]
            if "creature" not in graphs:
                fail(f"{recipe} has no creature graph: {graphs}")
            moving = {"act": "move", "speed": speed}
            r = mcp.json("anim_check", recipe=recipe, max_checks=5,
                         transitions=[{"from": moving, "to": {"act": "attack"}}, {"from": moving, "to": {"act": "death"}}])
            checks[recipe] = {"passed": r["passed"], "counts": r["counts"], "errors": r["errors"]}
            print(f"  anim_check {recipe}: passed {r['passed']} {r['counts']} rules from {r['rules']['source']}")
            if not r["passed"]:
                fail(f"{recipe}: {json.dumps(r['errors'])[:800]}")
        summary["checks"] = checks

        # Contact sheets: clips, states, a transition per creature.
        sheets = {}
        for recipe, speed in CREATURES:
            states = [{"state": s, "params": {"act": s, "speed": speed}} for s in ["idle", "move", "attack", "special", "death"]]
            rows = states + [{"state": "move", "params": {"act": "move", "speed": round(speed * 0.4, 3)}},
                             {"transition": {"from": {"act": "move", "speed": speed}, "to": {"act": "attack"}}}]
            res = mcp.call("anim_filmstrip", recipe=recipe, rows=rows, frames=8, tile=128)
            packet = json.loads(next(c["text"] for c in res["content"] if c["type"] == "text"))
            failed = packet["checks_failed"]
            sheets[recipe] = {"png": packet["image"]["path"], "packet": packet["image"]["packet_path"], "checks_failed": failed}
            print(f"  filmstrip {recipe}: {packet['image']['path']} ({packet['image']['width']}x{packet['image']['height']}), checks failed {failed}")
            if failed:
                fail(f"{recipe} filmstrip checks failed")
        clips = mcp.call("anim_filmstrip", recipe="recipes/shambler.json", frames=8, tile=112)
        packet = json.loads(next(c["text"] for c in clips["content"] if c["type"] == "text"))
        sheets["shambler_clips"] = {"png": packet["image"]["path"], "rows": len(packet["rows"])}
        print(f"  filmstrip shambler clips: {packet['image']['path']}")
        summary["sheets"] = sheets

        # A live creature: state, feet IK on the slab, a filmstrip in the world.
        enemies = mcp.json("world_query", tags=["enemy"], fields=["path", "anim", "position"], limit=40)["items"]
        walkers = [e for e in enemies if "graph" in (e.get("anim") or {}) and e["path"].split("/")[-1].startswith(("shambler", "runner", "colossus")) and e["position"][1] > -0.3]
        if not walkers:
            fail(f"no walking creature on the slab: {[e['path'] for e in enemies]}")
        live = walkers[0]["path"]
        state = mcp.json("anim_state", path=live, window_s=2)
        feet = next((s for s in state["ik"] if s["name"] == "feet"), None)
        print(f"  anim_state {live}: {[(l['layer'], [s['state'] for s in l['states']]) for l in state['layers']]} events {len(state['events'])} feet {feet}")
        if state["mode"] != "graph" or not feet or not feet.get("feet") or any(f["ground_m"] is None for f in feet["feet"]):
            fail(f"live creature state: {json.dumps(state)[:600]}")
        summary["live_state"] = {"entity": live, "layers": state["layers"], "ik": state["ik"], "events": state["events"][:6]}
        res = mcp.call("anim_filmstrip", path=live, frames=6, tile=160, window_s=1.5)
        packet = json.loads(next(c["text"] for c in res["content"] if c["type"] == "text"))
        summary["live_sheet"] = packet["image"]["path"]
        print(f"  filmstrip {live}: {packet['image']['path']}, checks passed {packet.get('checks', {}).get('passed')}, warnings {packet.get('warnings')}")
        if packet.get("warnings"):
            fail(f"live filmstrip: {packet['warnings']}")
        if max(packet["image"]["width"], packet["image"]["height"]) > 2048:
            fail(f"sheet {packet['image']['width']}x{packet['image']['height']}")

        # Feet on uneven ground: a shambler straddling the graveyard's 12 cm step plants one foot
        # on each level and lowers its hips (feet IK on the slab's 'ground' pieces).
        gy = mcp.json("world_query", path_prefix="/slab/graveyard/ground", fields=["path", "position", "scale"], limit=1)["items"][0]
        edge_x = gy["position"][0] + gy["scale"][0] / 2
        spawn = mcp.json("world_spawn", shape="model", name="step_test", recipe="recipes/shambler.json", position=[edge_x, 0.06, gy["position"][2]],
                         scale=[1.2, 1.2, 1.2], anim={"graph": "creature", "params": {"act": "idle"}})
        mcp.call("model_build", recipe="recipes/shambler.json")
        step = mcp.json("anim_state", id=spawn["id"])
        feet = next(s for s in step["ik"] if s["name"] == "feet")
        grounds = sorted(f["ground_m"] for f in feet["feet"])
        print(f"  step test: feet grounds {grounds}, offsets {[f['offset_m'] for f in feet['feet']]}, pelvis {feet['pelvis_m']}")
        if abs(grounds[0]) > 0.01 or abs(grounds[1] - 0.12) > 0.01 or feet["pelvis_m"] > -0.04:
            fail(f"feet on the step: {feet}")
        res = mcp.call("anim_filmstrip", id=spawn["id"], frames=4, tile=192, window_s=1.5)
        packet = json.loads(next(c["text"] for c in res["content"] if c["type"] == "text"))
        fiw = packet.get("feet_in_world") or {}
        check = fiw.get("check", {})
        print(f"  filmstrip step_test: {packet['image']['path']}, feet_on_ground {check.get('value')} cm (limit {check.get('limit')}) passed {check.get('passed')}, warnings {packet.get('warnings')}")
        if not check.get("passed"):
            fail(f"feet on the step, in the world: {json.dumps(fiw)[:600]}")
        if packet.get("warnings"):
            fail(f"step filmstrip: {packet['warnings']}")
        summary["step_test"] = {"feet": feet, "sheet": packet["image"]["path"], "feet_in_world": fiw}
        mcp.call("world_delete", id=spawn["id"])

        # Live authoring: a graph patch and a keyed clip, journaled, then undone.
        patch = mcp.json("anim_set", recipe="recipes/shambler.json", name="creature",
                         patch={"transitions": [{"from": "*", "to": "death", "when": "act == death", "blend": 0.3},
                                                {"from": "*", "to": "move", "when": "act == move", "blend": 0.5},
                                                {"from": "*", "to": "idle", "when": "act == idle", "blend": 0.5},
                                                {"from": "*", "to": "attack", "when": "act == attack", "blend": 0.4},
                                                {"from": "*", "to": "special", "when": "act == special", "blend": 0.4}]})
        print(f"  anim_set patch: {patch['path']} v{patch.get('version')} journal {patch['journal']} validated {patch['validated']}")
        g = mcp.json("anim_get", recipe="recipes/shambler.json", graph="creature")
        if g["source"] != "world" or len(g["graph"]["transitions"]) != 5:
            fail(f"the patched graph isn't the world's: {json.dumps(g)[:300]}")
        wave = mcp.json("anim_set", recipe="recipes/shambler.json", name="wave", clip={
            "kind": "keys", "duration": 1.2, "loop": "loop",
            "keys": [{"time": 0, "pose": {"upperarm_r": {"rot": [-120, 0, -20]}}},
                     {"time": 0.6, "pose": {"upperarm_r": {"rot": [-120, 0, 20]}, "forearm_r": {"rot": [-30, 0, 0]}}}]})
        print(f"  anim_set clip: {wave['path']} {wave.get('clip')}")
        r = mcp.json("anim_check", recipe="recipes/shambler.json", clips=["wave"], states=[])
        print(f"  anim_check wave: passed {r['passed']} {r['counts']}")
        bad = raw("anim_set", recipe="recipes/shambler.json", name="broken", graph={"states": {"idle": "idle"}, "transitions": [{"from": "idle", "to": "nowhere", "when": "act == x"}]})
        err = json.loads(bad["content"][0]["text"])
        print(f"  anim_set broken graph refused: {bad.get('isError')} {err.get('code')} at {err.get('path')}")
        if not bad.get("isError") or not err.get("path"):
            fail(f"a broken graph was accepted: {err}")
        undo = mcp.json("world_undo", count=2)
        g = mcp.json("anim_get", recipe="recipes/shambler.json")
        world_assets = [a["path"] for a in g["assets"]]
        print(f"  world_undo {undo.get('undone')}: assets left {world_assets}")
        if world_assets:
            fail(f"undo left anim assets: {world_assets}")
        summary["authoring"] = {"patch": patch, "clip": wave, "wave_check": r["passed"], "refused": err, "undo": undo}
    errors = eng.validation_errors()
    summary["validation_errors"] = errors
    (out / "summary.json").write_text(json.dumps(summary, indent=1))
    print(f"validation errors at shutdown: {errors}; summary in {out / 'summary.json'}")
    if errors:
        fail("validation errors")


if __name__ == "__main__":
    main()
