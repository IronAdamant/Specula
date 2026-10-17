#!/usr/bin/env python3
"""Drives the lex_* tools end to end through MCP, as agents would (the Lex lane's live check).

Starts a headless engine on its own port, imports Sepulcra's Lex slice, validates it, simulates
it (twice, to check determinism; with a param override; from a saved run), edits it as two
agents (a refused broken edit with its suggestion, a claim that blocks the other agent, a
versioned patch, world_undo), and round-trips it through world_save / world_load and
lex_export / lex_import. Exits non-zero on the first failure.

    cargo build -p specula
    python scripts/lex_check.py --port 7480

Standard library only; output ASCII.
"""
import argparse
import json
import os
import sys
import tempfile
from pathlib import Path

from specula_mcp import ROOT, Engine

SLICE = ROOT / "games" / "sepulcra" / "lex" / "slice.lex.json"
SET = "sepulcra_slice"
DEFENDED = [
    (0, "place", {"site": "top:10,14", "stone": "warding_1"}),
    (0, "place", {"site": "top:8,15", "stone": "rune_pillar_1"}),
    (600, "place", {"site": "top:10,15", "stone": "brazier_1"}),
    (1500, "place", {"site": "top:10,15", "stone": "brazier_1"}),
    (2100, "upgrade", {"stone": "top:10,14"}),
    (2400, "call_wave", {}),
    (2700, "upgrade", {"stone": "top:8,15"}),
    (3300, "upgrade", {"stone": "top:10,15"}),
]


def check(cond, what):
    if not cond:
        print(f"FAIL: {what}")
        sys.exit(1)
    print(f"ok: {what}")


def refused(mcp, tool, **args):
    """A tool call expected to fail: its error JSON."""
    result = mcp.request("tools/call", {"name": tool, "arguments": args})
    if not result.get("isError"):
        print(f"FAIL: {tool} should have been refused: {result['content'][0]['text'][:300]}")
        sys.exit(1)
    return json.loads(result["content"][0]["text"])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7480)
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    ap.add_argument("--exe", default=str(target / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")), help="default: $CARGO_TARGET_DIR (or target/) debug build")
    args = ap.parse_args()
    work = Path(tempfile.mkdtemp(prefix="lex_check_"))
    with Engine(args.exe, args.port, work / "engine.log", "--headless", "--worlds", str(work / "worlds"), "--captures", str(work / "captures")) as eng:
        ann, bob = eng.mcp("lex-check-ann"), eng.mcp("lex-check-bob")
        ann.hello("ann")
        bob.hello("bob")
        names = ann.tools()
        lex = sorted(n for n in names if n.startswith("lex_"))
        check(lex == ["lex_edit", "lex_export", "lex_get", "lex_import", "lex_list", "lex_schema", "lex_session", "lex_simulate", "lex_validate"], f"9 lex tools listed: {lex}")
        idx = ann.json("lex_schema")
        check("archetype" in [k["name"] for k in idx["kinds"]], "lex_schema lists the item kinds")

        doc = json.loads(SLICE.read_text())
        out = ann.json("lex_import", set=SET, document=doc)
        check(out["valid"] and out["items"] == len(doc["items"]), f"imported the slice: {out['items']} items, valid")
        v = ann.json("lex_validate", set=SET)
        check(v["valid"] and v["errors"] == 0, "lex_validate: no errors")
        check(any(p["id"] == "start_dust" for p in v["params"]), "params are listed for the balance lab")

        commands = [{"tick": t, "command": c, "args": a} for t, c, a in DEFENDED]
        sim_args = dict(set=SET, seed=7, ticks=20000, commands=commands, expect=["res.tombstones > 0", "waves.all_cleared"],
                        watch=["res.dust", "count(enemy)"], sample_every=600)
        a = ann.json("lex_simulate", **sim_args)
        b = bob.json("lex_simulate", **sim_args)
        check(a["outcome"] is not None and a["outcome"]["result"] == "won", f"simulated to an outcome: {a['outcome']} at {a['seconds']} s")
        check(a["hash"] == b["hash"], f"two agents' runs agree: hash {a['hash']}")
        check(a["expect_passed"], "expectations hold")
        check(len(a["samples"]) <= 201, f"bounded samples: {len(a['samples'])}")
        print(f"   wall time {a['wall_ms']} ms for {a['tick']} ticks; rejections {a['commands']['refused']}")
        poor = ann.json("lex_simulate", **{**sim_args, "params": {"start_dust": 0}})
        check(poor["hash"] != a["hash"] and poor["commands"]["refused"] > a["commands"]["refused"], "a param override changes the run (more refusals with no starting dust)")

        ann.json("lex_simulate", set=SET, seed=7, ticks=1500, commands=commands, save_as="mid")
        cont = ann.json("lex_simulate", **{"set": SET, "from": "mid", "ticks": 18500, "commands": [c for c in commands if c["tick"] >= 1500]})
        check(cont["hash"] == a["hash"], "a saved run continued to the end matches the run in one go")
        insp = ann.json("lex_session", action="inspect", name="mid", tag="stone")
        check(len(insp["entities"]) >= 2, f"lex_session inspects the stones: {[e['archetype'] for e in insp['entities']]}")

        # Edits: a broken one is refused with the fix.
        bad = refused(ann, "lex_edit", set=SET, ops=[{"op": "patch", "id": "warding_1", "set": {"behaviours[0].range": "7"}}])
        sugg = bad["diagnostics"][0].get("suggestion", "")
        check(bad["error"] == "invalid" and "7m" in sugg, f"a range without a unit is refused: {sugg}")
        cur = ann.json("lex_get", set=SET, ids=["warding_1"])["items"][0]
        ok = ann.json("lex_edit", set=SET, ops=[{"op": "patch", "id": "warding_1", "set": {"behaviours[0].range": "7.5m"}, "expect_version": cur["version"]}])
        check(ok["valid"] and ok["changes"][0]["version"] == cur["version"] + 1, "a versioned patch applies")
        stale = refused(bob, "lex_edit", set=SET, ops=[{"op": "patch", "id": "warding_1", "set": {"attributes.cost": 55}, "expect_version": cur["version"]}])
        check(stale["error"] == "version_conflict", "a stale version is refused")
        hist = ann.json("world_history", path_prefix=f"/lex/{SET}/warding_1")
        check(len(hist["entries"]) >= 1, "the edit is in world_history")
        ann.json("world_undo")
        after = ann.json("lex_get", set=SET, ids=["warding_1"])["items"][0]["item"]
        check(after["behaviours"][0]["range"] == "7m", "world_undo reverted the patch")

        # Claims.
        ann.json("claim_take", path=f"/lex/{SET}")
        blocked = refused(bob, "lex_edit", set=SET, ops=[{"op": "patch", "id": "dust", "set": {"start": 999}}])
        check(blocked["error"] == "claimed", "another agent's claim blocks the edit")
        ann.json("claim_release", all=True)

        # Text round trips.
        exported = json.loads(ann.text("lex_export", set=SET))
        same = ann.json("lex_import", set=SET, document=exported)
        check(same["changes"] == [], "re-importing the export changes nothing")
        ann.json("world_save", name="lexcheck")
        ann.json("world_load", name="lexcheck", force=True)
        listed = ann.json("lex_list", set=SET, kind="archetype")
        check(listed["total"] == len(doc["items"]) and len(listed["items"]) > 5, "the set survives world_save / world_load")
        again = ann.json("lex_simulate", **sim_args)
        check(again["hash"] == a["hash"], "and simulates to the same hash")
        ann.close()
        bob.close()
    print("lex_check: all passed")


if __name__ == "__main__":
    main()
