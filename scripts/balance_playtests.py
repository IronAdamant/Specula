"""Balance lab, checked by playtests: a tuned result replayed in the real game, across parallel
engines, and compared with the lab's in-process numbers (docs/design/BALANCE.md).

The balance lab tunes Lex rules headless, in-process. This script checks a result where players
meet it: Sepulcra's own game, hand-written rules and all.

1. A lead engine (the game's exe, headless, --lead-port, default 7600) imports the Lex slice and
   runs `balance_sweep {runs: true}` at the point (e.g. runner_hp=61, shambler_hp=139) on the
   chosen seeds for every policy in the targets: each run comes back with its seed, policy, how
   it ended (outcome and tick, dust, tombstones) and the policy steps it applied, with their ticks.
2. Each run becomes a playtest (docs/design/PLAYTESTS.md): `play_new` with the run's seed and
   game data carrying the point's values (the slice's waves with the built-in slab, stones and
   enemies, as tests/lex_parity.rs uses), then each applied step as a `play_command` at its tick,
   until the outcome; the end asserts the outcome, its tick, dust and tombstones the lab measured.
3. scripts/playtest_swarm.py runs them: one task per playtest on the lead's task board, workers
   leasing ports from the lead's pool (--pool, default 7601-7603) and running a fresh headless
   engine per playtest, several at once.
4. The report compares: per policy, the lab's means and the game's (from the playtests' answers),
   and every run where they disagree, with what differed. Exit 1 on any disagreement.

What it can catch: the Lex port of the game drifting from the game (the slice is meant to be
tick-identical, tests/lex_parity.rs), a param that doesn't mean in the game what it means in Lex,
a policy step the game refuses, and anything in-process sweeps do differently from engines.

    cargo build --release -p sepulcra-app
    python scripts/balance_playtests.py --point runner_hp=61 --point shambler_hp=139 \\
        --targets games/sepulcra/lex/slice_styles.targets.json --seeds holdout --runs 48

Ports: --lead-port and --pool (inclusive) only. Standard library only; ASCII output.
"""
import argparse
import json
import os
import shutil
import sys
import tempfile
import time
from pathlib import Path

from specula_mcp import ROOT, Engine
from playtest_swarm import Swarm

SLICE = ROOT / "games" / "sepulcra" / "lex" / "slice.lex.json"
DATA = ROOT / "games" / "sepulcra" / "data"
SLICE_WAVES = ROOT / "games" / "sepulcra" / "lex" / "slice_waves.json"
SET = "sepulcra_slice"
DATA_DIR = "balance_data"
# Most playtests one check runs (each is a whole game in a fresh engine).
MAX_RUNS = 128


def game_data(point):
    """Sepulcra's game data with the point's Lex params applied: {file name: text}."""
    enemies = json.loads((DATA / "enemies.json").read_text())
    stones = json.loads((DATA / "stones.json").read_text())
    waves = json.loads(SLICE_WAVES.read_text())
    by_id = {e["id"]: e for e in enemies["enemies"]}
    for name, value in point.items():
        v = float(str(value).rstrip("pt%"))
        if name.endswith("_hp") and name[:-3] in by_id:
            by_id[name[:-3]]["hp"] = v
        elif name == "start_dust":
            waves["start_dust"] = int(v)
        elif name == "start_tombstones":
            waves["tombstones"] = int(v)
        elif name == "early_call_per_s":
            waves["early_call_dust_per_s"] = v
        elif name == "sell_refund":
            stones["sell_refund"] = v
        else:
            raise SystemExit(f"param '{name}' has no counterpart in Sepulcra's game data (known: <enemy>_hp, start_dust, "
                             f"start_tombstones, early_call_per_s, sell_refund)")
    return {
        "slab_01.json": (DATA / "slab_01.json").read_text(),
        "stones.json": json.dumps(stones, indent=1),
        "enemies.json": json.dumps(enemies, indent=1),
        "waves.json": json.dumps(waves, indent=1),
    }


def game_command(step):
    """A Lex policy step as the game's play_command arguments."""
    cmd, args = step["command"], step.get("args") or {}
    if cmd == "place":
        stone = args["stone"]
        # Lex names tiers (warding_1); the game places tier I of a stone (warding).
        return {"action": "place", "stone": stone[:-2] if stone.endswith("_1") else stone, "cell": args["site"]}
    if cmd in ("upgrade", "sell"):
        return {"action": cmd, "cell": args["stone"]}
    if cmd == "call_wave":
        return {"action": "call_wave"}
    raise SystemExit(f"command '{cmd}' has no counterpart in Sepulcra's play_command")


def spec_for(run, steps, margin):
    """One lab run as a playtest spec."""
    name = f"{run['policy']}_s{run['seed']}"
    calls = [{"at": 0, "call": "play_new", "args": {"seed": str(run["seed"]), "data": DATA_DIR}, "label": "new"}]
    for tick, idx in run["applied"]:
        calls.append({"at": tick, "call": "play_command", "args": game_command(steps[idx]), "label": f"step{idx}"})
    ended = run["outcome"] is not None
    # The tick the run stopped at (Lex reports an outcome at the tick it was decided in; the
    # game and the lab both stop one step later, at run["ticks"]).
    end_tick = run["ticks"]
    res = run["resources"]
    expect ={"outcome": run["outcome"] or "running", "tick": end_tick}
    for key in ("dust", "tombstones"):
        if key in res:
            expect[key] = round(res[key])
    spec = {
        "name": name,
        "game": "sepulcra",
        "description": f"lab run: policy {run['policy']}, seed {run['seed']}",
        "steps": calls,
        "ticks": end_tick + (margin if ended else 0),
        "end": [{"call": "play_state", "args": {"response_format": "concise"}, "expect": expect, "label": "end"}],
    }
    if ended:
        spec["until"] = {"call": "play_state", "args": {"response_format": "concise"}, "expect": {"outcome": {"ne": "running"}}, "every": 30}
    return spec


class DataSwarm(Swarm):
    """Puts the point's game data in every playtest engine's working directory."""

    def __init__(self, data, *a, **k):
        super().__init__(*a, **k)
        self.data = data

    def engine(self, port, cwd, label, cache):
        d = Path(cwd) / DATA_DIR
        d.mkdir(parents=True, exist_ok=True)
        for name, text in self.data.items():
            (d / name).write_text(text)
        return super().engine(port, cwd, label, cache)


def mean(xs):
    xs = [x for x in xs if x is not None]
    return round(sum(xs) / len(xs), 4) if xs else None


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    ap.add_argument("--exe", default=str(target / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")))
    ap.add_argument("--targets", default="games/sepulcra/lex/slice_styles.targets.json")
    ap.add_argument("--point", action="append", default=[], help="param=value (the param's human unit); repeat")
    ap.add_argument("--seeds", default="holdout", help="train | holdout | comma-separated seeds")
    ap.add_argument("--runs", type=int, default=48, help=f"at most this many runs (seeds x policies), <= {MAX_RUNS}")
    ap.add_argument("--workers", type=int, default=3)
    ap.add_argument("--lead-port", type=int, default=7600)
    ap.add_argument("--pool", default="7601-7603", help="ports the workers lease, first-last")
    ap.add_argument("--out", type=Path, default=ROOT / "target" / "balance-playtests")
    ap.add_argument("--skew", action="append", default=[],
                    help="param=delta added in the game's data only: a self-test that a real difference is flagged")
    a = ap.parse_args()
    a.out = a.out.resolve()
    point = {}
    for p in a.point:
        k, _, v = p.partition("=")
        point[k.strip()] = float(v)
    game_point = dict(point)
    for p in a.skew:
        k, _, v = p.partition("=")
        game_point[k.strip()] = game_point.get(k.strip(), 0.0) + float(v)
    first, last = (int(x) for x in a.pool.split("-"))
    if a.lead_port == 7471 or 7471 in range(first, last + 1):
        sys.exit("never use 7471 (the interactive engine's port)")
    if a.lead_port in range(first, last + 1):
        sys.exit("--lead-port must be outside --pool")
    targets = json.loads((ROOT / a.targets).read_text())
    n_policies = len(targets.get("policies") or [1])
    per_seed_runs = max(1, min(a.runs, MAX_RUNS) // n_policies)
    if a.seeds in ("train", "holdout"):
        spec = targets.get("seeds", {}).get(a.seeds, 16)
        base = targets.get("seeds", {}).get("base", 1) + (10_000 if a.seeds == "holdout" else 0)
        seeds = spec if isinstance(spec, list) else [base + i for i in range(spec)]
    else:
        seeds = [int(s) for s in a.seeds.split(",")]
    seeds = seeds[:per_seed_runs]

    if a.out.exists():
        shutil.rmtree(a.out)
    a.out.mkdir(parents=True)
    perf = a.out / "perf.toml"
    perf.write_text(f"[swarm]\nlease_ports = [{first}, {last}]\n")
    lead_dir = a.out / "lead"
    report = {"point": point, "game_point": game_point, "targets": a.targets, "seeds": seeds}
    started = time.perf_counter()
    args = ["--headless", "--paused", "--captures", str(lead_dir / "captures"), "--perf", str(perf), "--", "--mute"]
    with Engine(a.exe, a.lead_port, lead_dir / "lead.log", *args, cwd=str(lead_dir.parent)) as lead:
        lab = lead.mcp("balance-lab")
        lab.hello("balance-lab", role="lead")
        lab.json("lex_import", set=SET, document=json.loads(SLICE.read_text()))
        sweep = lab.json("balance_sweep", set=SET, targets=targets, points=[point], seeds=seeds, runs=True)
        lab.close()
        p = sweep["points"][0]
        runs = p["runs_listed"]
        steps = {pol["name"]: pol["steps"] for pol in sweep["policies"]}
        print(f"lab: {len(runs)} runs at {point}: loss {p['loss']}, hit {p['hit']}, outcomes {p['outcomes']}")
        specs = {}
        for r in runs:
            s = spec_for(r, steps[r["policy"]], margin=60)
            specs[s["name"]] = s
        (a.out / "specs").mkdir()
        for name, s in specs.items():
            (a.out / "specs" / f"{name}.json").write_text(json.dumps(s, indent=1))
        swarm = DataSwarm(game_data(game_point), a.exe, a.out / "swarm", workers=a.workers, lead_port=a.lead_port, use_lead=True,
                          game_args=["--mute"], verify=False, log=lambda m: None)
        print(f"playtests: {len(specs)} on up to {a.workers} engines at once (ports {first}-{last})")
        sw = swarm.run(specs)
    by_name = {r["name"]: r for r in sw["results"]}
    disagreements = []
    per_policy = {}
    for r in runs:
        name = f"{r['policy']}_s{r['seed']}"
        got = by_name.get(name, {})
        run = got.get("run", {})
        agree = got.get("passed", False)
        game = {"outcome": r["outcome"] or "running", "tombstones": r["resources"].get("tombstones")}
        for f in run.get("failures", []):
            if f.get("path") in game:
                game[f["path"]] = f.get("actual")
        if not agree:
            disagreements.append({"run": name, "status": run.get("status"), "error": got.get("error"),
                                  "failures": run.get("failures", [])[:6]})
        pp = per_policy.setdefault(r["policy"], {"runs": 0, "agree": 0, "lab_wins": [], "game_wins": [], "lab_tombstones": [], "game_tombstones": []})
        pp["runs"] += 1
        pp["agree"] += 1 if agree else 0
        pp["lab_wins"].append(1.0 if r["outcome"] == "won" else 0.0)
        pp["game_wins"].append(1.0 if game["outcome"] == "won" else 0.0)
        pp["lab_tombstones"].append(r["resources"].get("tombstones"))
        pp["game_tombstones"].append(game["tombstones"] if isinstance(game["tombstones"], (int, float)) else None)
    summary = {}
    for pol, pp in per_policy.items():
        summary[pol] = {
            "runs": pp["runs"],
            "agree": pp["agree"],
            "win_rate": {"lab": mean(pp["lab_wins"]), "game": mean(pp["game_wins"])},
            "tombstones_left": {"lab": mean(pp["lab_tombstones"]), "game": mean(pp["game_tombstones"])},
        }
        print(f"  {pol}: {pp['agree']}/{pp['runs']} runs agree; win rate lab {summary[pol]['win_rate']['lab']} "
              f"game {summary[pol]['win_rate']['game']}; tombstones left lab {summary[pol]['tombstones_left']['lab']} "
              f"game {summary[pol]['tombstones_left']['game']}")
    report.update({
        "lab": {"loss": p["loss"], "hit": p["hit"], "outcomes": p["outcomes"], "policies": p.get("policies")},
        "playtests": len(specs),
        "passed": sw["passed"],
        "swarm_wall_s": sw["wall_s"],
        "sum_of_playtest_wall_s": sw["sum_of_playtest_wall_s"],
        "workers": sw["workers"],
        "per_policy": summary,
        "disagreements": disagreements,
        "disagree": bool(disagreements),
        "needs_human_playtest": True,
        "wall_s": round(time.perf_counter() - started, 2),
    })
    (a.out / "report.json").write_text(json.dumps(report, indent=1))
    for d in disagreements[:10]:
        print(f"  DISAGREE {d['run']}: {d['status']} {d.get('error') or ''} {json.dumps(d['failures'])[:400]}")
    print(f"{sw['passed']}/{len(specs)} playtests agree with the lab; {sw['wall_s']} s wall on {sw['workers']} workers "
          f"({sw['sum_of_playtest_wall_s']} s of playtests); report in {a.out / 'report.json'}")
    print("DISAGREE" if disagreements else "AGREE")
    sys.exit(1 if disagreements else 0)


if __name__ == "__main__":
    main()
