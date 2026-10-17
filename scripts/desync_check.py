"""Desync check: run the same thing twice and let the engine say where the runs first differ.

Two headless engines (ports 7492 and 7493 by default) get the same commands in lockstep on a paused
clock. Each run exports its trace (desync_trace); the second engine compares the first's trace with
its own (desync_compare), which reports the first divergent tick and the component (an entity and
its fields, or a game subsystem) that differs.

Modes:
  (default)   plain engine: an entity moved every tick. Twice identically (must match), then with
              the second run moving it 0.5 m off at tick --at (must diverge at the next tick, on
              entity /mover, field position). Both runs are also checked against their own journal
              replay (desync_compare b: "replay").
  --game      Sepulcra (games/sepulcra-app, seed 7): identical runs must match; then the second
              run places one extra stone at --at, which must show as a divergence in the game's
              subsystems ("defences", "economy", "sim") at the next tick.
  --files A B compare two trace files with a running engine (--port), print the report.

Usage: cargo build --release -p specula -p sepulcra-app && python scripts/desync_check.py [--game]
       [--at 40] [--ticks 80] [--ports 7492 7493] [--debug]
Output is ASCII. Exit code 0 = every expectation held.
"""
import argparse
import json
import shutil
import sys
import tempfile
from pathlib import Path

from specula_mcp import ROOT, Engine, Mcp, url_for


def exe_path(name, debug):
    return ROOT / "target" / ("debug" if debug else "release") / (name + (".exe" if sys.platform == "win32" else ""))


def align(mcps):
    """Pause every clock and step each to the same tick; returns that tick."""
    ticks = [m.json("time_control", action="pause")["tick"] for m in mcps]
    target = max(ticks) + 2
    for m, t in zip(mcps, ticks):
        m.json("time_control", action="step", ticks=target - t)
    return target


def plain_run(m, ticks, at=None):
    """Spawns /mover and moves it once per tick; with `at`, 0.5 m off from that tick on."""
    start = m.json("time_control", action="info")["tick"]
    m.json("world_spawn", shape="cube", path="/mover")
    for i in range(ticks):
        tick = start + i
        x = float(i) + (0.5 if at is not None and tick >= at else 0.0)
        m.json("world_set", path="/mover", position=[x, 0.0, 0.0])
        m.json("time_control", action="step", ticks=1)
    return start


def game_run(m, ticks, at=None):
    """Steps Sepulcra; with `at`, places one stone at that tick."""
    start = m.json("time_control", action="info")["tick"]
    for i in range(ticks):
        if at is not None and start + i == at:
            m.call("play_command", action="place", stone="warding", cell="top:8,12")
        m.json("time_control", action="step", ticks=1)
    return start


def compare(a_mcp, b_mcp):
    trace = a_mcp.json("desync_trace")
    return b_mcp.json("desync_compare", a=trace["file"], b="live"), trace


def scenario(args, work, game):
    exe = exe_path("sepulcra" if game else "specula", args.debug)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build {'' if args.debug else '--release '}-p {'sepulcra-app' if game else 'specula'}")
    extra = ["--", "--seed", "7"] if game else []
    run = game_run if game else plain_run
    problems, report = [], {}
    for label, perturb in (("identical", None), ("perturbed", args.at)):
        engines = []
        try:
            for i, port in enumerate(args.ports):
                e = Engine(exe, port, work / f"{label}-{i}.log", "--headless", "--captures", str(work / f"cap-{label}-{i}"),
                           "--worlds", str(work / f"worlds-{label}-{i}"), *extra, cwd=work)
                engines.append(e.__enter__())
            a, b = (Mcp(url_for(p), f"desync-{label}-{i}") for i, p in enumerate(args.ports))
            a.json("agent_hello", name="desync_a")
            b.json("agent_hello", name="desync_b")
            start = align([a, b])
            run(a, args.ticks)
            # The perturbation is given in ticks after the aligned start.
            run(b, args.ticks, None if perturb is None else start + perturb)
            result, trace = compare(a, b)
            report[label] = result
            print(f"[{label}] trace {trace['ticks']} ticks ({trace['from_tick']}..{trace['to_tick']}), "
                  f"{trace['entity_changes']} entity changes, {trace['bytes']} bytes")
            print(f"  -> match={result.get('match')} first_divergent_tick={result.get('first_divergent_tick')} "
                  f"components={result.get('components')}")
            for side, m in (("a", a), ("b", b)):
                replay = m.json("desync_compare", a="live", b="replay")
                if not replay.get("match"):
                    problems.append(f"{label}: run {side} doesn't match its own journal replay: {replay}")
            if perturb is None:
                if not result.get("match"):
                    problems.append(f"identical runs diverged: {result}")
            else:
                expected = start + perturb + 1
                if result.get("first_divergent_tick") != expected:
                    problems.append(f"perturbed run: expected the first divergent tick {expected}, got {result.get('first_divergent_tick')}")
                comps = " ".join(result.get("components") or [])
                wanted = ["subsystem defences", "subsystem sim"] if game else ["entity /mover: position"]
                for w in wanted:
                    if w not in comps:
                        problems.append(f"perturbed run: expected component '{w}' in {result.get('components')}")
            a.close()
            b.close()
        finally:
            for e in engines:
                e.__exit__(None, None, None)
    return problems, report


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--game", action="store_true", help="run Sepulcra (sepulcra-app) instead of the plain engine")
    ap.add_argument("--at", type=int, default=40, help="ticks after the aligned start at which run b differs")
    ap.add_argument("--ticks", type=int, default=80)
    ap.add_argument("--ports", type=int, nargs=2, default=[7492, 7493])
    ap.add_argument("--debug", action="store_true", help="use debug builds")
    ap.add_argument("--files", nargs=2, metavar=("A", "B"), help="compare two trace files with the engine on --port")
    ap.add_argument("--port", type=int, default=7492)
    ap.add_argument("--out", type=Path, help="save the reports as JSON here")
    args = ap.parse_args()
    if args.files:
        m = Mcp(url_for(args.port), "desync-check")
        result = m.json("desync_compare", a=str(Path(args.files[0]).resolve()), b=str(Path(args.files[1]).resolve()))
        print(json.dumps(result, indent=2))
        sys.exit(0 if result.get("match") else 1)
    if 7471 in args.ports:
        sys.exit("never run on 7471 (the interactive engine's port)")
    work = Path(tempfile.mkdtemp(prefix="specula-desync-"))
    try:
        problems, report = scenario(args, work, args.game)
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
