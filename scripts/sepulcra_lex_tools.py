#!/usr/bin/env python3
"""Rebuilds Sepulcra's rules in Lex through the lex_* tools alone, then simulates whole games to
the same hashes (the M3 proof for "Gameplay as data: Lex"; docs/design/LEX.md).

Starts the `sepulcra` binary headless on its own port (the game provides the slab's movement
hooks in-process, as lex_simulate needs), then, over MCP only:

1. reads the language (lex_schema) and imports the slab (lex/slab_01.lex.json: the level as Lex
   sites and a data item; generated from data/slab_01.json);
2. writes the rules item by item with lex_edit, each edit validated as it lands (an archetype's
   tier chain goes in one edit, since its tiers name each other);
3. validates the set (no errors, no warnings) and exports it: the export is the two documents'
   items exactly;
4. simulates the games in lex/proof.json (centre-heavy seed 7 won, no anti-air lost, a full wall
   block lost) and checks each reaches the recorded tick, outcome and hash;
5. edits a rule (warding damage 10 -> 11) and sees the hash move, undoes it (world_undo) and sees
   it come back; a notes edit leaves the rules' hash alone;
6. runs the centre-heavy game live (lex_live) on the engine's tick, giving each command when its
   tick comes (time_control pause and step), and checks it ends where the headless run did, that
   its journal replays headless to the same hash, and that its entities are world entities.

    cargo build -p sepulcra-app --release
    python scripts/sepulcra_lex_tools.py --port 7481 [--exe path/to/sepulcra.exe]

Standard library only; output ASCII.
"""
import argparse
import json
import os
import sys
import tempfile
from pathlib import Path

from specula_mcp import ROOT, Engine

LEX = ROOT / "games" / "sepulcra" / "lex"
SET = "sepulcra"


def check(cond, what):
    if not cond:
        print(f"FAIL: {what}")
        sys.exit(1)
    print(f"ok: {what}")


def canon(v):
    return json.dumps(v, sort_keys=True)


def batches(items):
    """The rules' items in edits that each validate: tier chains (linked by next) together."""
    chains, done = [], set()
    for iid, item in items.items():
        if iid in done:
            continue
        group = [iid]
        if item.get("kind") == "archetype" and item.get("next"):
            nxt = item["next"]
            while nxt and nxt not in group:
                group.append(nxt)
                nxt = items.get(nxt, {}).get("next")
        done.update(group)
        chains.append(group)
    return chains


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7481)
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    ap.add_argument("--exe", default=str(target / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")))
    args = ap.parse_args()
    if not Path(args.exe).exists():
        sys.exit(f"{args.exe} not found: cargo build -p sepulcra-app --release (or pass --exe)")
    # Output (engine log, the capture, summary.json) goes to target/sepulcra-lex-tools/; worlds to a temp folder.
    work = ROOT / "target" / "sepulcra-lex-tools"
    work.mkdir(parents=True, exist_ok=True)
    worlds = Path(tempfile.mkdtemp(prefix="sepulcra_lex_tools_"))
    slab = json.loads((LEX / "slab_01.lex.json").read_text())
    rules = json.loads((LEX / "sepulcra.lex.json").read_text())
    proof = json.loads((LEX / "proof.json").read_text())["cases"]
    summary = {}
    extra = ["--headless", "--worlds", str(worlds), "--captures", str(work / "captures"), "--", "--mute"]
    with Engine(args.exe, args.port, work / "engine.log", *extra, cwd=work) as eng:
        mcp = eng.mcp("sepulcra-lex-tools")
        mcp.hello("lex-rebuilder")
        tools = sorted(t for t in mcp.tools() if t.startswith("lex_"))
        check("lex_live" in tools and "lex_edit" in tools, f"lex tools: {tools}")
        kinds = [k["name"] for k in mcp.json("lex_schema")["kinds"]]
        check("data" in kinds and "native" in kinds, f"lex_schema lists the item kinds ({len(kinds)}), data and native among them")
        behaviours = mcp.json("lex_schema", topic="behaviours")
        native = next(b for b in behaviours if b["name"] == "native")
        check(any(f["name"] == "on" for f in native["fields"]), "the native behaviour has its 'on' action lists")

        # 1. The slab.
        out = mcp.json("lex_import", set=SET, document=slab)
        check(out["valid"] and out["items"] == len(slab["items"]), f"imported the slab: {out['items']} items")

        # 2. The rules, item by item. Items name each other (walkers read waves.current, the
        # waves name the walkers), so the set is a draft (allow_errors) until the last lands; every
        # edit is still validated, and the errors it leaves are counted down to none.
        items = rules["items"]
        edits, left = 0, []
        for group in batches(items):
            ops = [{"op": "put", "id": i, "item": items[i], "expect_version": 0} for i in group]
            res = mcp.request("tools/call", {"name": "lex_edit", "arguments": {"set": SET, "ops": ops, "name": rules["name"], "tick_hz": rules["tick_hz"], "allow_errors": True}})
            if res.get("isError"):
                print(res["content"][0]["text"][:2000])
            check(not res.get("isError"), f"lex_edit put {', '.join(group)}")
            body = json.loads(res["content"][0]["text"])
            left.append(len(body.get("errors", [])))
            edits += 1
        print(f"   errors left after each of the {edits} edits: {left}")
        check(left[-1] == 0, "the last edit leaves no errors")
        summary["edits"], summary["errors_left"] = edits, left
        v = mcp.json("lex_validate", set=SET)
        check(v["valid"] and v["errors"] == 0 and v.get("warnings", 0) == 0, f"lex_validate: valid, {v['errors']} errors, {v.get('warnings', 0)} warnings")
        natives = v.get("natives", [])
        check(len(natives) == 4 and all(n.get("reason") for n in natives), f"4 natives, each with its reason: {[n['hook'] for n in natives]}")
        exported = json.loads(mcp.text("lex_export", set=SET))
        want = {**slab["items"], **items}
        check(canon(exported["items"]) == canon(want), f"the export is the two documents' {len(want)} items exactly")

        # 4. Whole games.
        sims = []
        for case in proof:
            r = mcp.json("lex_simulate", set=SET, seed=case["seed"], ticks=case["ticks"] + 100, commands=case["commands"])
            got = (r["tick"], (r["outcome"] or {}).get("result"), r.get("hash"), r["rules_hash"])
            check(r["complete"] and got == (case["ticks"], case["outcome"], case["hash"], case["rules_hash"]),
                  f"{case['layout']} seed {case['seed']}: {got[1]} at tick {got[0]}, hash {got[2]} ({r['wall_ms']} ms; {r['commands']['applied']} commands)")
            sims.append({"layout": case["layout"], "seed": case["seed"], "tick": r["tick"], "outcome": got[1], "hash": got[2], "wall_ms": r["wall_ms"], "native_calls": r.get("native_calls")})
        summary["simulations"] = sims

        # 5. Edits move the hash; undo brings it back; notes don't count.
        centre = proof[0]
        run = lambda: mcp.json("lex_simulate", set=SET, seed=centre["seed"], ticks=centre["ticks"] + 100, commands=centre["commands"])
        cur = mcp.json("lex_get", set=SET, ids=["warding_1"])["items"][0]
        mcp.json("lex_edit", set=SET, ops=[{"op": "patch", "id": "warding_1", "set": {"attributes.damage": "11pt"}, "expect_version": cur["version"]}])
        changed = run()
        check(changed["hash"] != centre["hash"] and changed["rules_hash"] != centre["rules_hash"], f"warding damage 11: another game (hash {changed['hash']}, {(changed['outcome'] or {}).get('result')} at {changed['tick']})")
        mcp.json("world_undo")
        back = run()
        check(back["hash"] == centre["hash"], "world_undo: the rule and the hash are back")
        cur = mcp.json("lex_get", set=SET, ids=["warding_1"])["items"][0]
        mcp.json("lex_edit", set=SET, ops=[{"op": "patch", "id": "warding_1", "set": {"notes": "Edited through lex_edit."}, "expect_version": cur["version"]}])
        check(run()["hash"] == centre["hash"], "a notes edit leaves the rules (and the game) alone")

        # 6. Live.
        mcp.call("time_control", action="pause")
        present = [
            {"tags": ["enemy", "flying"], "spawn": {"shape": "sphere", "color": [0.55, 0.85, 0.95], "scale": [0.6, 0.6, 0.6]}},
            {"tags": ["enemy"], "spawn": {"shape": "sphere", "color": [0.75, 0.8, 0.6], "scale": [0.7, 0.9, 0.7]}},
            {"tags": ["stone"], "spawn": {"shape": "cube", "color": [0.5, 0.5, 0.55], "scale": [1.2, 2.0, 1.2]}},
            {"tags": ["wall"], "spawn": {"shape": "cube", "color": [0.4, 0.38, 0.35], "scale": [1.8, 0.8, 1.8]}},
            {"tags": ["projectile"], "spawn": {"shape": "sphere", "color": [1.0, 0.7, 0.3], "scale": [0.25, 0.25, 0.25]}},
        ]
        st = mcp.json("lex_live", action="start", set=SET, seed=centre["seed"], present=present)
        check(st["started"] and st["tick"] == 0, f"lex_live started '{SET}' at Lex tick 0 ({st['tick_hz']} Hz)")
        live_tick = 0

        def step_to(t):
            nonlocal live_tick
            while live_tick < t:
                # A step runs at most 512 ticks before it answers (the rest come later), so step
                # in pieces no bigger and read where the run is after each.
                n = min(t - live_tick, 500)
                mcp.call("time_control", action="step", ticks=n)
                live_tick = mcp.json("lex_live", action="status", set=SET)["runs"][0]["tick"]

        shot = None
        for c in centre["commands"]:
            step_to(c["tick"])
            r = mcp.json("lex_live", action="command", set=SET, command=c["command"], args=c["args"])
            check(r["accepted"] and r["tick"] == c["tick"], f"live {c['command']} {c['args']} at tick {r['tick']}")
            if shot is None and c["tick"] > 9000:
                world = mcp.json("world_list", path_prefix=f"/lex_live/{SET}", limit=200)
                shot = world.get("total", len(world.get("entities", world.get("items", []))))
                # The live run as the world shows it (shapes from the present rules).
                packet = mcp.frame(width=960, height=540, eye=[30, 32, 38], target=[0, -4, 0], fov_y_deg=50, max_subjects=8)
                summary["capture"] = {k: packet.get(k) for k in ("image", "packet", "path") if k in packet}
                print(f"   capture of the live run at tick {c['tick']}: {summary['capture']}")
        step_to(centre["ticks"])
        st = mcp.json("lex_live", action="status", set=SET)["runs"][0]
        check(st["outcome"] and st["outcome"]["result"] == centre["outcome"] and st["tick"] == centre["ticks"] and st["hash"] == centre["hash"],
              f"the live run ends like the headless one: {st['outcome']} at tick {st['tick']}, hash {st['hash']}")
        check(shot is not None and shot > 5, f"mid-game the live run had {shot} world entities under /lex_live/{SET}")
        j = mcp.json("lex_live", action="journal", set=SET, limit=5000)
        cmds = [{"tick": c["tick"], "command": c["command"], "args": c["args"]} for c in j["commands"]]
        again = mcp.json("lex_simulate", set=SET, seed=int(j["seed"]), ticks=j["tick"], commands=cmds)
        check(again["hash"] == j["hash"] == centre["hash"], "the live journal replays headless (lex_simulate) to the same hash")
        # A save game takes the live run with it (its own section; save_check.py proves a load
        # continues it exactly).
        # force: the save may be a previous run's (another session's).
        res = mcp.request("tools/call", {"name": "save_write", "arguments": {"name": "lex_tools_check", "overwrite": True, "force": True}})
        body = json.loads(res["content"][0]["text"])
        check(not res.get("isError") and "lex_live" in body.get("sections", {}) and "lex_live" in body.get("subsystems", {}),
              f"save_write while the run is live saves it too ({body.get('sections', body)})")
        stop = mcp.json("lex_live", action="stop", set=SET)
        check(stop["stopped"], "stopped; its world entities are gone")
        summary["live"] = {"tick": st["tick"], "outcome": st["outcome"], "hash": st["hash"], "entities_mid_game": shot, "commands": len(cmds)}
        mcp.close()
    (work / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"sepulcra_lex_tools: all passed ({work / 'summary.json'})")


if __name__ == "__main__":
    main()
