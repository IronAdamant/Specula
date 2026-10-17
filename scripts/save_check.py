"""Save game check: a loaded save continues bit-identically, and the desync detector says so.

Engine A (Sepulcra, seed 7, headless by default) plays the "centre" build order with a pile of
physics crates tumbling beside the slab and the ambulo module walking its character (so the
world, the game's simulation, physics' hidden state and module state all move). At tick T it
writes save_write "proof", then runs N more ticks and exports its trace (desync_trace T+1..T+N).
Engine B starts fresh, loads ambulo, save_read "proof" (every saved hash must match on load),
runs the same N ticks and exports its trace. desync_compare of the two traces must report a match
over all N ticks, with the world hash and the subsystems sim, economy, enemies, defences,
projectiles, physics and logic all compared.

Also measured: save size and time (snapshot, encode, write), load time, and the same with a
world of --stress extra entities.

Usage: cargo build --release -p sepulcra-app && python scripts/save_check.py [--ticks 900]
       [--save-at 600] [--stress 10000] [--ports 7600 7601] [--window] [--adapter integrated]
       [--out perf/saves/NAME.json]
ASCII output. Exit code 0 = every check held.
"""
import argparse
import json
import os
import shutil
import sys
import tempfile
import time
from pathlib import Path

from specula_mcp import ROOT, Engine, Mcp, url_for


def exe_path(name):
    return Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target") / "release" / (name + (".exe" if sys.platform == "win32" else ""))


def step(m, n):
    while n > 0:
        k = min(n, 18000)
        m.json("time_control", action="step", ticks=k)
        n -= k


def build_ambulo(m):
    result = m.request("tools/call", {"name": "logic_build", "arguments": {"path": "games/ambulo"}})
    text = next(c["text"] for c in result["content"] if c["type"] == "text")
    if result.get("isError"):
        raise RuntimeError(f"logic_build failed: {text[:600]}")
    return json.loads(text)


def crates(m):
    ops = [{"op": "spawn", "shape": "group", "name": "crates", "position": [70, 0, 0]},
           {"op": "spawn", "shape": "cube", "parent": "/crates", "name": "floor", "position": [0, -0.5, 0], "scale": [16, 1, 16],
            "collider": {"shape": "box"}}]
    for i in range(24):
        ops.append({"op": "spawn", "shape": "sphere" if i % 4 == 0 else "cube", "parent": "/crates", "name": f"c{i}",
                    "position": [(i % 4) * 0.8 - 1.2, 0.6 + (i // 4) * 1.15, (i % 3) * 0.35 - 0.35],
                    "rotation_deg": [0, i * 13, 4],
                    "collider": {"shape": "sphere", "radius": 0.5} if i % 4 == 0 else {"shape": "box"},
                    "body": {"kind": "dynamic", "mass": 2 + i}})
    m.json("world_batch", ops=ops)


def stress(m, n):
    """n static cubes in a few groups (a big world to save)."""
    made = 0
    g = 0
    while made < n:
        ops = [{"op": "spawn", "shape": "group", "name": f"stress_{g}"}]
        for i in range(min(999, n - made)):
            k = made + i
            ops.append({"op": "spawn", "shape": "cube", "parent": f"/stress_{g}", "name": f"s{k}",
                        "position": [200 + (k % 100) * 1.5, 0, (k // 100) * 1.5], "color": "#808890"})
        m.json("world_batch", ops=ops)
        made += len(ops) - 1
        g += 1


def run(args, work):
    exe = exe_path("sepulcra")
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build --release -p sepulcra-app")
    common = ["--saves", str(work / "saves")]
    if not args.window:
        common.append("--headless")
    if args.adapter:
        common += ["--adapter", args.adapter]
    problems, report = [], {}

    def check(ok, what):
        print(f"  {'ok  ' if ok else 'FAIL'} {what}")
        if not ok:
            problems.append(what)

    pa, pb = args.ports
    with Engine(exe, pa, work / "a.log", *common, "--captures", str(work / "cap-a"), "--worlds", str(work / "worlds-a"),
                "--", "--seed", "7", cwd=str(ROOT)):
        a = Mcp(url_for(pa), "save-check-a")
        a.json("agent_hello", name="saver", role="lead")
        a.json("time_control", action="pause")
        a.call("play_new", seed=7, layout="centre")
        crates(a)
        built = build_ambulo(a)
        print(f"A: ambulo loaded ({built.get('reload', {}).get('status')})")
        a.json("logic_input", module="ambulo", actions={"move": [1, 0.3]}, ticks=args.save_at + args.ticks)
        step(a, args.save_at // 2)
        a.call("play_command", action="call_wave")
        a.json("logic_input", module="ambulo", actions={"jump": True}, ticks=2)
        step(a, args.save_at - args.save_at // 2)
        t = a.json("time_control", action="info")["tick"]
        saved = a.json("save_write", name="proof", label="save_check")
        print(f"A: saved at tick {saved['tick']}: {saved['bytes']} bytes, sections {saved['sections']}, ms {saved['timings_ms']}")
        report["save"] = saved
        check(saved["tick"] == t, "the save is at the current tick")
        for name in ("sim", "physics", "logic"):
            check(name in saved["subsystems"], f"the save covers subsystem {name}")
        step(a, args.ticks)
        trace_a = a.json("desync_trace", from_tick=t + 1, to_tick=t + args.ticks)
        print(f"A: trace {trace_a['ticks']} ticks {trace_a['from_tick']}..{trace_a['to_tick']}")
        if args.stress:
            stress(a, args.stress)
            big = a.json("save_write", name="stress", label=f"{args.stress} extra entities")
            print(f"A: stress save ({big['sections']}): {big['bytes']} bytes, ms {big['timings_ms']}")
            report["stress_save"] = big
        a.close()

    with Engine(exe, pb, work / "b.log", *common, "--captures", str(work / "cap-b"), "--worlds", str(work / "worlds-b"),
                "--", "--seed", "99", cwd=str(ROOT)):
        b = Mcp(url_for(pb), "save-check-b")
        b.json("agent_hello", name="loader", role="lead")
        b.json("time_control", action="pause")
        build_ambulo(b)
        started = time.perf_counter()
        loaded = b.json("save_read", name="proof", force=True)
        wall_ms = (time.perf_counter() - started) * 1000
        print(f"B: loaded tick {loaded['tick']} match={loaded['match']} ms {loaded['timings_ms']} (call {wall_ms:.1f} ms)")
        report["load"] = loaded
        check(loaded["match"], "every saved hash matches right after the load")
        check(not loaded["notes"], f"no notes on load ({loaded['notes']})")
        check(b.json("time_control", action="info")["tick"] == t, "the clock is at the save's tick")
        step(b, args.ticks)
        trace_b = b.json("desync_trace", from_tick=t + 1, to_tick=t + args.ticks)
        cmp = b.json("desync_compare", a=trace_a["file"], b=trace_b["file"])
        report["compare"] = cmp
        print(f"desync_compare: match={cmp.get('match')} common_ticks={cmp.get('common_ticks')} "
              f"first_divergent_tick={cmp.get('first_divergent_tick')} components={cmp.get('components')}")
        check(cmp.get("match") is True, "the loaded run matches the saving run on every tick")
        check(cmp.get("common_ticks") == args.ticks, f"over all {args.ticks} ticks")
        rows = json.loads(Path(trace_b["file"]).read_text())["ticks"]
        names = sorted(rows[-1][2].keys()) if rows else []
        report["subsystems_compared"] = names
        for name in ("sim", "economy", "enemies", "defences", "projectiles", "physics", "logic"):
            check(name in names, f"subsystem {name} was compared")
        # The player's way: the pause menu's Save game and Load game (play_ui presses the buttons
        # a click, a key or a pad's accept would).
        print("B: the player saves and loads from the pause menu")
        b.json("play_ui", press="menu_button")
        shot = b.frame(width=1280, height=720, include_ui=True)
        report["menu_capture"] = {"png": shot["image"]["path"], "packet": shot["image"]["packet_path"]}
        if args.keep:
            for k in ("path", "packet_path"):
                dst = args.keep / Path(shot["image"][k]).name
                args.keep.mkdir(parents=True, exist_ok=True)
                shutil.copy(shot["image"][k], dst)
                report["menu_capture"]["kept_" + k] = str(dst)
        print(f"  pause menu capture: {report['menu_capture']}")
        before = b.json("play_state")
        pressed = b.json("play_ui", press="save")
        check(pressed.get("action") == "save_game", f"the menu's Save game sends save_game ({pressed.get('action')})")
        listed = b.json("save_list")
        slot = next((s for s in listed["saves"] if s["name"] == "sepulcra_slot_1"), None)
        check(slot is not None and slot["created_by"] == "sepulcra" and slot["tick"] == b.json("time_control", action="info")["tick"],
              "the player's save is in the slot, written by the game at this tick")
        b.json("play_ui", press="resume")
        step(b, 300)
        moved = b.json("play_state")
        check(moved["tick"] > before["tick"], "the run went on after saving")
        b.json("play_ui", press="menu_button")
        b.json("play_ui", press="load")
        after = b.json("play_state", include=["log"])
        check(after["state_hash"] == before["state_hash"] and after["tick"] == before["tick"], "Load game brought the saved run back")
        check(after["paused"] is True, "and it waits, paused under the menu")
        said = [m["message"] for m in after.get("log", {}).get("items", after.get("log", [])) if isinstance(m, dict)]
        check(any(m.startswith("Game loaded") for m in said), f"the player is told ({said[:2]})")
        # What the loaded world looks like (the slab and the stones adopted, enemies rebuilt).
        world_shot = b.frame(width=1280, height=720, eye=[0, 38, 30], target=[0, 0, 0])
        report["world_capture"] = {"png": world_shot["image"]["path"], "packet": world_shot["image"]["packet_path"]}
        if args.keep:
            for k in ("path", "packet_path"):
                dst = args.keep / ("loaded_" + Path(world_shot["image"][k]).name)
                shutil.copy(world_shot["image"][k], dst)
                report["world_capture"]["kept_" + k] = str(dst)
        print(f"  loaded world capture: {report['world_capture']}")
        if args.stress:
            started = time.perf_counter()
            big = b.json("save_read", name="stress", force=True)
            print(f"B: stress load: match={big['match']} ms {big['timings_ms']} (call {(time.perf_counter() - started) * 1000:.1f} ms)")
            report["stress_load"] = big
            check(big["match"], "the stress save loads with matching hashes")
        b.close()
    return problems, report


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--ticks", type=int, default=900, help="ticks run after the save, compared")
    ap.add_argument("--save-at", type=int, default=600, help="ticks played before the save")
    ap.add_argument("--stress", type=int, default=10000, help="extra entities for the size/time measurement (0: skip)")
    ap.add_argument("--ports", type=int, nargs=2, default=[7600, 7601])
    ap.add_argument("--window", action="store_true", help="open the windows (default headless)")
    ap.add_argument("--adapter", default=None, help="discrete | integrated | <index>")
    ap.add_argument("--out", type=Path, help="save the report as JSON here")
    ap.add_argument("--keep", type=Path, help="copy the pause menu capture and its packet here")
    args = ap.parse_args()
    if 7471 in args.ports:
        sys.exit("never run on 7471 (the interactive engine's port)")
    work = Path(tempfile.mkdtemp(prefix="specula-saves-"))
    try:
        problems, report = run(args, work)
    finally:
        shutil.rmtree(work, ignore_errors=True)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(report, indent=2) + "\n")
    for p in problems:
        print(f"  PROBLEM: {p}")
    print("PASS" if not problems else "FAIL")
    sys.exit(0 if not problems else 1)


if __name__ == "__main__":
    main()
