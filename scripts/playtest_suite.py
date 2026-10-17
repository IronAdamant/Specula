#!/usr/bin/env python3
"""Sepulcra's headless playtest suite, run as a playtest swarm (the M3 exit proof).

26 playtests: every built-in layout over several seeds (wins and losses, each asserted to end the
way the design says: the centre layout survives all ten waves; no anti-air falls to the wraiths at
wave 6; walling every gate falls at wave 4 or 5; no defence falls at wave 2), scripted strategies
through the same tools a player's agent uses (early wave calls, selling mid-game, walls without
stones, the game's own pause and speed, HUD buttons, dry runs, rejected moves with their codes,
the game's own recordings, captures), and two engine-only playtests (a physics crate settling, a
world edit undone) to show the facility isn't Sepulcra's alone.

Each playtest runs in a fresh headless engine and records a replay; each replay is then run again
in another fresh engine and must match every recorded hash (see scripts/playtest_swarm.py); as a
negative control, one replay with its seed changed (hashes kept) must fail to verify. The
suite runs with --workers engines at once (default 8) and, with --serial, once more with one worker
to measure the wall time saved. Results: perf/playtests/<date>-sepulcra-suite.json (kept in the
repo) and target/playtest-suite/ (replays, captures, per-engine logs).

    cargo build -p sepulcra-app --release
    python scripts/playtest_suite.py                 # 8 workers, lead engine on port 7493
    python scripts/playtest_suite.py --serial        # also a one-worker run, for the speed-up
    python scripts/playtest_suite.py --only centre_s7,none_s4

Standard library only; output ASCII.
"""
import argparse
import json
import os
import sys
import time
from pathlib import Path

from specula_mcp import ROOT
from playtest_swarm import Swarm

EXE = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")) / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")
UNTIL_OVER = {"call": "play_state", "expect": {"outcome": {"ne": "running"}}, "every": 30}


def new(seed, layout=None, at=0):
    args = {"seed": str(seed)}
    if layout:
        args["layout"] = layout
    return {"at": at, "call": "play_new", "args": args, "expect": {"seed": str(seed), "outcome": "running"}}


def autoplay(name, seed, layout, end, description):
    return {
        "name": name, "game": "sepulcra", "description": description,
        "steps": [new(seed, layout)],
        "until": UNTIL_OVER, "ticks": 40000,
        "end": [{"call": "play_state", "expect": end}],
    }


def cmd(at, action, cell=None, stone=None, dry_run=False, **extra):
    """A play_command step; `extra` are step keys (expect, expect_error, label)."""
    args = {"action": action}
    if cell:
        args["cell"] = cell
    if stone:
        args["stone"] = stone
    if dry_run:
        args["dry_run"] = True
    step = {"at": at, "call": "play_command", "args": args}
    step.update(extra)
    return step


def suite():
    specs = []
    won = {"outcome": "won", "wave": 10, "tombstones": {"gte": 1}}
    for s in (1, 2, 3, 7, 11, 42):
        specs.append(autoplay(f"centre_s{s}", s, "centre", won, "The baseline build order survives all ten waves."))
    for s in (1, 5, 9):
        specs.append(autoplay(f"no_air_s{s}", s, "no_air", {"outcome": "lost", "wave": 6, "tombstones": 0},
                              "Without sky obelisks the wraiths' dive at wave 6 ends the run."))
    for s in (2, 6, 10):
        specs.append(autoplay(f"full_block_s{s}", s, "full_block", {"outcome": "lost", "wave": {"in": [4, 5]}},
                              "Walls on every gate with almost no stones: the breakers get through by wave 4-5."))
    for s in (4, 8):
        specs.append(autoplay(f"none_s{s}", s, "none", {"outcome": "lost", "wave": 2},
                              "No defence: the graveyard falls in wave 2."))
    # Scripted strategies and rule checks, through the player's tools.
    specs.append({
        "name": "rejections_s7", "game": "sepulcra",
        "description": "Moves the rules refuse come back with their codes; accepted ones change the state.",
        "steps": [
            new(7),
            cmd(1, "place", "top:8,12", "warding", expect={"accepted": True, "dust": {"gte": 0}}),
            cmd(2, "place", "top:8,12", "warding", expect_error="occupied"),
            cmd(3, "place", "top:10,7", "warding", expect_error="not_buildable"),
            cmd(4, "wall", "top:12,8", expect_error="not_wallable"),
            cmd(5, "place", "top:99,99", "warding", expect_error="unknown_cell"),
            cmd(6, "sell", "top:12,8", expect_error=True),
            {"at": 7, "call": "play_command", "args": {"commands": [{"action": "place", "stone": "sky_obelisk", "cell": c}
                                                                    for c in ("top:12,8", "top:7,8", "top:7,12", "top:10,12", "top:12,12", "top:12,11")]},
             "expect": {"rejected": {"gte": 1}, "results.#": 6}, "label": "spend_it_all"},
            {"at": 8, "call": "play_state", "expect": {"stones": {"gte": 2}, "commands_recorded": {"gte": 2}}},
        ],
        "ticks": 600,
        "end": [{"call": "play_state", "expect": {"outcome": "running"}}],
    })
    specs.append({
        "name": "dry_run_s4", "game": "sepulcra",
        "description": "A dry run judges a move without making it.",
        "steps": [
            new(4),
            cmd(1, "place", "top:8,12", "warding", dry_run=True, expect={"accepted": True, "dry_run": True}),
            {"at": 2, "call": "play_state", "expect": {"stones": 0, "commands_recorded": 0}},
            cmd(3, "place", "top:8,12", "warding", expect={"accepted": True}),
            {"at": 4, "call": "play_state", "expect": {"stones": 1, "commands_recorded": 1}},
        ],
        "ticks": 30,
    })
    specs.append({
        "name": "pause_holds_s5", "game": "sepulcra",
        "description": "The game's own pause stops the simulation while the engine keeps ticking; resuming finishes the run.",
        "steps": [
            new(5, "centre"),
            {"at": 300, "call": "play_control", "args": {"paused": True}, "expect": {"paused": True}},
            {"at": 301, "call": "play_state", "expect": {"tick": {"gte": 299, "lte": 301}}},
            {"at": 900, "call": "play_state", "expect": {"tick": {"gte": 299, "lte": 301}, "paused": True}},
            {"at": 901, "call": "play_control", "args": {"paused": False}, "expect": {"paused": False}},
        ],
        "until": UNTIL_OVER, "ticks": 40000,
        "end": [{"call": "play_state", "expect": won}],
    })
    specs.append({
        "name": "speed4_s2", "game": "sepulcra",
        "description": "Speed 4 runs four simulation steps per tick: the run is won in about a quarter of the ticks.",
        "steps": [new(2, "centre"), {"at": 1, "call": "play_control", "args": {"speed": 4}, "expect": {"speed": 4}}],
        "until": UNTIL_OVER, "ticks": 9000,
        "end": [{"call": "play_state", "expect": dict(won, speed=4, tick={"gte": 18000})}],
    })
    specs.append({
        "name": "early_calls_s3", "game": "sepulcra",
        "description": "Calling waves early (for the dust bonus) brings them on at once.",
        "steps": [
            new(3, "centre"),
            cmd(30, "call_wave", expect={"accepted": True}),
            {"at": 31, "call": "play_state", "expect": {"wave": {"gte": 1}}},
        ],
        "until": UNTIL_OVER, "ticks": 40000,
        "end": [{"call": "play_state", "expect": {"outcome": {"in": ["won", "lost"]}, "wave": {"gte": 2}}}],
    })
    specs.append({
        "name": "sell_off_s5", "game": "sepulcra",
        "description": "Selling the two forward wardings mid-game refunds dust and the run goes on to its end.",
        "steps": [
            new(5, "centre"),
            {"at": 9000, "call": "play_command", "args": {"commands": [{"action": "sell", "cell": "top:8,12"}, {"action": "sell", "cell": "top:12,8"}]},
             "expect": {"accepted": 2}},
        ],
        "until": UNTIL_OVER, "ticks": 40000,
        "end": [{"call": "play_state", "expect": {"outcome": {"in": ["won", "lost"]}}}],
    })
    specs.append({
        "name": "walls_only_s6", "game": "sepulcra",
        "description": "Walls on every gate and no stones: walls alone only delay the loss.",
        "steps": [new(6)] + [cmd(1, "wall", c, expect={"accepted": True}) for c in ("top:10,7", "top:12,10", "top:9,12", "top:7,9")],
        "until": UNTIL_OVER, "ticks": 40000,
        "end": [{"call": "play_state", "expect": {"outcome": "lost", "walls": {"lte": 4}}}],
    })
    specs.append({
        "name": "hud_buttons_s7", "game": "sepulcra",
        "description": "The HUD's buttons, pressed as data, do what a click does.",
        "steps": [
            new(7),
            {"at": 1, "call": "play_ui", "args": {"press": "build_wall", "limit": 5}, "expect": {"pressed": "build_wall"}},
            {"at": 2, "call": "play_state", "args": {"response_format": "detailed"}, "expect": {"presentation.hud.selected": "wall"}},
            {"at": 3, "call": "play_ui", "args": {"press": "call_wave", "limit": 5}, "expect": {"pressed": "call_wave"}},
            {"at": 4, "call": "play_state", "expect": {"wave": 1}},
        ],
        "ticks": 10,
    })
    view = {"width": 640, "height": 360, "eye": [38, 27, 52], "target": [0, -9, 1], "max_subjects": 4}
    shot = {"subjects_on_screen": {"gte": 1}, "image.width": 640}
    specs.append({
        "name": "captures_s7", "game": "sepulcra",
        "description": "Captures at the build phase, mid-game and the end of a won run.",
        "steps": [
            new(7, "centre"),
            {"at": 60, "call": "sight_frame", "args": view, "expect": shot, "label": "build"},
            {"at": 9000, "call": "sight_frame", "args": view, "expect": shot, "label": "mid"},
        ],
        "until": UNTIL_OVER, "ticks": 40000,
        "end": [{"call": "sight_frame", "args": view, "expect": shot, "label": "end"}, {"call": "play_state", "expect": won}],
    })
    specs.append({
        "name": "recording_s11", "game": "sepulcra",
        "description": "The game's own recording of the run so far replays and verifies inside the playtest.",
        "steps": [
            new(11, "centre"),
            {"at": 6000, "call": "play_record", "args": {"path": "rec_s11.json"}, "expect": {"commands": {"gte": 1}}},
            {"at": 6001, "call": "play_replay", "args": {"path": "rec_s11.json"}, "expect": {"status": "verified"}},
        ],
        "ticks": 6010,
    })
    # Engine-only playtests: nothing Sepulcra-specific.
    specs.append({
        "name": "physics_crate_settles",
        "description": "A crate dropped on a floor comes to rest on it (physics checks inside a playtest).",
        "steps": [
            {"at": 0, "call": "world_spawn", "args": {"shape": "cube", "name": "pt_floor", "position": [500, -0.5, 0], "scale": [10, 1, 10], "collider": {"shape": "box"}}},
            {"at": 0, "call": "world_spawn", "args": {"shape": "cube", "name": "pt_crate", "position": [500, 3, 0], "collider": {"shape": "box"}, "body": {"kind": "dynamic", "mass": 2}}},
            {"at": 15, "call": "physics_check", "args": {"checks": [{"check": "at_rest", "entities": "/pt_crate"}]}, "expect": {"pass": False}, "label": "falling"},
            {"at": 240, "call": "physics_check", "args": {"checks": [{"check": "at_rest", "entities": "/pt_crate"}, {"check": "touching", "a": "/pt_crate", "b": "/pt_floor"},
                                                                     {"check": "inside", "entity": "/pt_crate", "min": [499, 0.3, -1], "max": [501, 0.7, 1]}]},
             "expect": {"pass": True}, "label": "settled"},
        ],
        "ticks": 240,
    })
    specs.append({
        "name": "world_edit_undo",
        "description": "A world edit undone with world_undo is gone; the journal says who did what.",
        "steps": [
            {"at": 0, "call": "world_spawn", "args": {"shape": "cube", "name": "pt_cube", "position": [600, 0, 0]}, "expect": {"path": "/pt_cube"}},
            {"at": 10, "call": "world_set", "args": {"path": "/pt_cube", "position": [601, 0, 0]}, "expect": {"position.0": 601}},
            {"at": 20, "call": "world_undo", "args": {}, "expect": {"undone.#": 1}},
            {"at": 21, "call": "world_query", "args": {"path_prefix": "/pt_cube", "fields": ["path", "position"]}, "expect": {"items.0.position.0": 600}},
        ],
        "ticks": 30,
    })
    return specs


def negative_control(exe, out, port):
    """A replay whose spec was changed (another seed) but whose hashes weren't must not verify:
    proof that 'verified' means something. Runs in a fresh engine of its own."""
    src = out / "replays" / "centre_s7.json"
    if not src.exists():
        return {"skipped": "no centre_s7 replay"}
    replay = json.loads(src.read_text())
    replay["spec"]["steps"][0]["args"]["seed"] = "8"
    work = out / "negative-control"
    work.mkdir(parents=True, exist_ok=True)
    (work / "tampered.json").write_text(json.dumps(replay))
    from specula_mcp import Engine
    from playtest_swarm import finish
    with Engine(exe, port, work / "engine.log", "--headless", "--paused", "--captures", str(work / "captures"), "--", "--mute", cwd=work) as eng:
        m = eng.mcp("negative-control")
        r = finish(m, m.json("playtest_run", replay="tampered.json"))
    d = r.get("replay", {}).get("first_divergence", {})
    return {"status": r.get("status"), "verified": r.get("replay", {}).get("verified"),
            "diverged_at_tick": d.get("got", {}).get("tick"), "parts_differing": d.get("parts_differing"),
            "passed": r.get("status") == "diverged"}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--exe", default=str(EXE), help="the sepulcra binary (default: this checkout's release build)")
    ap.add_argument("--workers", type=int, default=8)
    ap.add_argument("--lead-port", type=int, default=7493)
    ap.add_argument("--serial", action="store_true", help="also run with one worker, to measure the parallel speed-up")
    ap.add_argument("--only", default=None, help="comma-separated playtest names")
    ap.add_argument("--out", default=str(ROOT / "target" / "playtest-suite"))
    ap.add_argument("--results", default=str(ROOT / "perf" / "playtests"), help="folder for the kept results file")
    ap.add_argument("--keep-scratch", action="store_true")
    ap.add_argument("--adapter", default=None, help="engine --adapter for every engine (discrete, integrated or an index)")
    ap.add_argument("--session-format", choices=["concise", "detailed"], default=None,
                    help="each playtesting session's default response_format (agent_hello), which the playtests' calls inherit")
    a = ap.parse_args()
    exe = Path(a.exe)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p sepulcra-app --release (or pass --exe)")
    specs = {s["name"]: s for s in suite()}
    if a.only:
        names = a.only.split(",")
        specs = {n: specs[n] for n in names}
    print(f"Sepulcra playtest suite: {len(specs)} playtests, {a.workers} workers")
    runs = {}
    for label, workers in [("parallel", a.workers)] + ([("serial", 1)] if a.serial else []):
        out = Path(a.out) / label
        engine_args = ["--adapter", a.adapter] if a.adapter else []
        swarm = Swarm(exe, out, workers, a.lead_port, engine_args=engine_args, game_args=["--mute"], keep_scratch=a.keep_scratch,
                      session_format=a.session_format)
        r = swarm.run(specs)
        runs[label] = r
        print(f"{label}: {r['passed']}/{r['playtests']} passed, {r['verified']} replays verified by hash, "
              f"{r['wall_s']} s wall with {workers} worker(s)")
        for res in r["results"]:
            if not res.get("passed"):
                print(f"  FAIL {res['name']}: {json.dumps(res.get('run', res.get('error')))[:600]}")
    p = runs["parallel"]
    control = negative_control(exe, Path(a.out) / "parallel", a.lead_port)
    print(f"negative control (a replay with its seed changed): {control}")
    summary = {
        "suite": "sepulcra",
        "date": time.strftime("%Y-%m-%d %H:%M:%S"),
        "exe": exe.name,
        "adapter": a.adapter or "discrete (default)",
        "session_format": a.session_format or "engine default (concise)",
        "playtests": p["playtests"],
        "passed": p["passed"],
        "verified": p["verified"],
        "workers": p["workers"],
        "wall_s_parallel": p["wall_s"],
        "wall_s_serial": runs["serial"]["wall_s"] if "serial" in runs else None,
        "speedup": round(runs["serial"]["wall_s"] / p["wall_s"], 2) if "serial" in runs else None,
        "board": p["board"],
        "negative_control": control,
        "results": [{
            "name": r["name"],
            "passed": r.get("passed"),
            "status": r.get("run", {}).get("status"),
            "ticks": r.get("run", {}).get("ticks"),
            "failures_total": r.get("run", {}).get("failures_total"),
            "end_hash": r.get("run", {}).get("end_hash"),
            "verified": r.get("verify", {}).get("replay", {}).get("verified"),
            "checkpoints_checked": r.get("verify", {}).get("replay", {}).get("checked"),
            "playtest_ms": r.get("run", {}).get("wall_ms"),
            "engine_startup_s": r.get("startup_s"),
            "wall_s": r.get("wall_s"),
            "captures": r.get("captures"),
            "error": r.get("error"),
        } for r in p["results"]],
    }
    if not a.only:
        res_dir = Path(a.results)
        res_dir.mkdir(parents=True, exist_ok=True)
        path = res_dir / f"{time.strftime('%Y%m%d-%H%M%S')}-sepulcra-suite.json"
        path.write_text(json.dumps(summary, indent=1))
        print(f"results: {path}")
    ok = p["passed"] == p["playtests"] and p["verified"] == p["playtests"] and control.get("passed", False)
    print("SUITE PASSED" if ok else "SUITE FAILED")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
