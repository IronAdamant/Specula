"""Whole-world replacement and live Lex runs (the M3 integration audit's probes, as checks).

Headless engines on their own ports (default 7601):
  1. world_load while a playtest runs is refused (playtest_running with retry_after_ms), like
     save_read; the playtest ends passed with its world intact;
  2. world_load stops live Lex runs and says so (lex_live.stopped), and removes the /lex_live
     entities the world files kept (no run would update or remove them); a restarted run shows
     its entities again;
  3. a playtest replay hashes live Lex runs (checkpoint part lex_live): recorded with a live run
     at param rate 5, replayed with rate 50 (the world identical, the run's state not), it
     diverges and names lex_live.

    cargo build --release -p specula
    python scripts/world_load_check.py [--port 7601] [--exe PATH] [--out DIR]

Standard library only; output ASCII. Exits non-zero on the first failure.
"""
import argparse
import importlib.util
import json
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine, detailed  # noqa: E402

TARGET = Path(os.environ["CARGO_TARGET_DIR"]) if os.environ.get("CARGO_TARGET_DIR") else ROOT / "target"
EXE = TARGET / "release" / ("specula.exe" if sys.platform == "win32" else "specula")

COINS = {"lex": 1, "name": "coins", "tick_hz": 30, "items": {
    "rate": {"kind": "param", "type": "int", "value": 5, "min": 0, "max": 1000},
    "gold": {"kind": "resource", "start": "$rate"},
    "tithe": {"kind": "timer", "every": "0.1s", "do": [{"add": "gold", "amount": 3}]}}}


def check(cond, what):
    if not cond:
        print(f"FAIL: {what}")
        sys.exit(1)
    print(f"ok: {what}")


def refused(mcp, tool, **args):
    result = mcp.request("tools/call", {"name": tool, "arguments": detailed(tool, args)})
    if not result.get("isError"):
        print(f"FAIL: {tool} should have been refused: {result['content'][0]['text'][:300]}")
        sys.exit(1)
    return json.loads(result["content"][0]["text"])


def wait_playtest(m, r):
    while r.get("status") == "running":
        r = m.json("playtest_report", wait_s=20)
    return r


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--port", type=int, default=7601)
    ap.add_argument("--exe", default=str(EXE))
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    exe = Path(args.exe)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build --release -p specula")
    work = Path(args.out) if args.out else TARGET / "world-load-check"
    work.mkdir(parents=True, exist_ok=True)
    spec = importlib.util.spec_from_file_location("save_check", ROOT / "scripts" / "save_check.py")
    save_check = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(save_check)
    common = ["--headless", "--paused", "--lead", "other", "--lead", "ll", "--captures", str(work / "captures"), "--saves", str(work / "saves"), "--worlds", str(work / "worlds")]

    print("1. world_load during a playtest")
    with Engine(exe, args.port, work / "playtest.log", *common, cwd=work) as eng:
        a, b = eng.mcp("pt-a"), eng.mcp("pt-b")
        a.hello("tester")
        b.hello("other", role="lead")  # force and overwrite of other worlds are the lead's (docs/design/AUTHORITY.md)
        b.json("world_spawn", shape="cube", name="other_world_marker", position=[5, 0, 0])
        b.json("world_save", name="otherw", overwrite=True)
        b.json("world_delete", path="/other_world_marker")
        a.json("world_spawn", shape="cube", name="ball", position=[0, 3, 0])
        # A headless playtest of a near-empty world runs ~1M ticks a second, so give it work that
        # never settles (bodies falling with no ground) and cancel it once the refusals are checked.
        for i in range(200):
            a.json("world_spawn", shape="cube", name=f"faller_{i}", position=[(i % 20) * 2.0, 50 + i // 20 * 2.0, 0],
                   collider={"shape": "box"}, body={"kind": "dynamic", "mass": 1})
        long_spec = {"name": "long", "ticks": 1000000, "checkpoint_every": 1000,
                     "steps": [{"at": 1, "call": "world_query", "args": {"path_prefix": "/ball"}, "expect": {"total": 1}}],
                     "end": [{"call": "world_query", "args": {"path_prefix": "/ball"}, "expect": {"total": 1}}]}
        a.json("playtest_run", spec=long_spec, wait_s=0)
        time.sleep(0.3)
        check(a.json("playtest_report", wait_s=0).get("status") == "running", "the playtest is running")
        e = refused(b, "world_load", name="otherw", force=True)
        check(e.get("error") == "playtest_running" and e.get("retry_after_ms"), f"world_load is refused: {e.get('error')}, retry_after_ms {e.get('retry_after_ms')}")
        e = refused(b, "save_read", name="whatever", force=True)
        check(e.get("error") == "playtest_running", "save_read too")
        check(a.json("playtest_report", wait_s=0).get("status") == "running", "the playtest was still running for both refusals")
        r = a.json("playtest_report", cancel=True)
        r = wait_playtest(a, r)
        check(r.get("status") == "cancelled", f"its starter cancels it ({r.get('status')})")
        in_spec = {"name": "loads", "ticks": 10, "steps": [{"at": 1, "call": "world_load", "args": {"name": "otherw", "force": True}}]}
        e = refused(a, "playtest_run", spec=in_spec)
        check("world_load" in json.dumps(e), "a spec that calls world_load is refused up front")
        a.close()
        b.close()

    print("2. live Lex runs across world_load")
    with Engine(exe, args.port, work / "livelex.log", *common, cwd=work) as eng:
        m = eng.mcp("ll")
        m.hello("ll", role="lead")  # force and overwrite of other worlds are the lead's (docs/design/AUTHORITY.md)
        save_check.start_live(m)
        m.json("time_control", action="step", ticks=300)
        shown = m.json("lex_live", action="status", set="save_live")["runs"][0]["shown"]
        before = m.json("world_query", path_prefix="/lex_live", limit=500)["total"]
        check(shown > 0 and before > shown, f"the run shows {shown} entities ({before} under /lex_live with its groups)")
        m.json("world_save", name="w1", overwrite=True)
        r = m.json("world_load", name="w1", force=True)
        ll = r.get("lex_live") or {}
        check(ll.get("stopped") == ["save_live"], f"world_load lists the stopped run: {ll.get('stopped')}")
        check(ll.get("presentation_removed") == before, f"and removed the {ll.get('presentation_removed')} /lex_live entities the files kept")
        gone = refused(m, "world_query", path_prefix="/lex_live", limit=500)
        check(gone.get("error") == "not_found", "no orphan /lex_live entities remain (not even the group)")
        status = m.json("lex_live", action="status")
        check(not status.get("runs"), "no run is live after the load")
        m.json("lex_live", action="start", set="save_live", seed=3, present=[{"tags": ["enemy"], "spawn": {"shape": "sphere"}}])
        m.json("time_control", action="step", ticks=300)
        st = m.json("lex_live", action="status", set="save_live")["runs"][0]
        after = m.json("world_query", path_prefix="/lex_live", limit=500)["total"]
        check(st["shown"] > 0 and after > 0, f"a restarted run shows its entities again ({st['shown']})")
        m.close()

    print("3. playtest replays hash live Lex runs")
    pt_spec = {"name": "livelex", "ticks": 900, "checkpoint_every": 300,
               "steps": [{"at": 1, "call": "world_spawn", "args": {"shape": "cube", "name": "box"}}]}

    def run(label, rate, **pt):
        with Engine(exe, args.port, work / f"{label}.log", "--headless", "--paused", "--captures", str(work / "captures"), cwd=work) as eng:
            m = eng.mcp(label)
            m.hello("pt", role="playtester")
            m.json("lex_import", set="coins", document=COINS)
            m.json("lex_live", action="start", set="coins", seed=1, params={"rate": rate})
            r = wait_playtest(m, m.json("playtest_run", wait_s=20, **pt))
            m.close()
            return r

    rec = run("record", 5, spec=pt_spec, record="livelex.json", overwrite=True)
    check(rec.get("status") == "passed", "recorded with a live run")
    replay = json.loads((work / "playtests" / "livelex.json").read_text())
    parts = json.dumps(replay.get("checkpoints", [])[:1])
    check("lex_live" in parts, "its checkpoints hash the live run (part lex_live)")
    same = run("replay_same", 5, replay="playtests/livelex.json")
    check(same.get("status") == "passed", f"the same run replays ({same.get('status')})")
    diff = run("replay_diff", 50, replay="playtests/livelex.json")
    differing = json.dumps(diff.get("replay") or diff)
    check(diff.get("status") == "diverged" and "lex_live" in differing, f"a live run in another state diverges on lex_live ({diff.get('status')})")
    print("world_load_check: all passed")


if __name__ == "__main__":
    main()
