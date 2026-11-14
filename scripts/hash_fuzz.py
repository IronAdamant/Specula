"""The two-fresh-engines hash fuzz (docs/design/AUTHORITY.md, class B): world data must not
depend on wall time, cache state or thread timing.

Two fresh headless engines get the same calls in the same order. They differ only in what must
not matter:
- A starts with an empty pipeline/model cache (`--cache`, cold); B with one a warm-up engine
  filled first (warm);
- A runs physics on 1 thread, B on 4 (`--physics-threads`);
- B's calls are spaced by injected delays (0-40 ms, seeded), A's are sent back to back.

The corpus: one generated call per mutating tool family (spawns with colliders and bodies, sets,
batches, environment, impulses, a navmesh and a walking agent, model builds from Sepulcra's
recipes, deletes, undo), then the public eval tasks' setup and reference calls (tasks that need
no hosted game and no variables), each followed by clock steps. After every step the world,
physics and navigation hashes per tick (world_hash recent) must be equal; any call that one
engine refused must be refused by the other with the same code.

Usage: python scripts/hash_fuzz.py [--exe target/debug/specula.exe] [--port 7762] [--seed 7]
Exit code 0 when every hash matched. Writes target/hash-fuzz/report.json.
"""
import argparse
import glob
import json
import os
import random
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import Engine  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "games" / "sepulcra" / "assets"


def generated_corpus(rng):
    """One call (or a few) per mutating tool family, deterministic for `rng`."""
    calls = []
    for i in range(6):
        calls.append(("world_spawn", {"name": f"crate{i}", "shape": "cube", "position": [rng.uniform(-4, 4), 2 + i * 1.2, rng.uniform(-4, 4)],
                                      "collider": {"shape": "box"}, "body": {"kind": "dynamic", "mass": 1 + i}}))
    calls.append(("world_spawn", {"name": "floor", "shape": "cube", "position": [0, -0.5, 0], "scale": [40, 1, 40], "collider": {"shape": "box"}}))
    calls.append(("world_set", {"path": "/crate0", "color": "#a05030"}))
    calls.append(("world_batch", {"ops": [{"op": "spawn", "name": f"post{i}", "shape": "cube", "position": [i * 2 - 5, 0.5, 6]} for i in range(5)]}))
    calls.append(("world_env", {"physics": {"gravity": [0, -9.81, 0]}}))
    calls.append(("physics_impulse", {"path": "/crate1", "impulse": [2, 0, 1]}))
    calls.append(("world_delete", {"path": "/post4"}))
    calls.append(("world_undo", {}))
    for i, recipe in enumerate(["recipes/tombstone.json", "recipes/slab_cliff.json"]):
        calls.append(("world_spawn", {"name": f"model{i}", "shape": "model", "recipe": recipe, "position": [8 + i * 3, 0, -6]}))
    calls.append(("nav_build", {"name": "ground"}))
    calls.append(("world_spawn", {"name": "walker", "shape": "cube", "position": [-6, 0.5, -6], "scale": [0.5, 1, 0.5]}))
    calls.append(("nav_agent", {"path": "/walker", "goal": [6, 0, 6], "speed": 2}))
    return [(tool, args, "fuzz") for tool, args in calls]


def eval_corpus():
    """Setup and reference calls of the public eval tasks that need no game and no variables."""
    calls = []
    for f in sorted(glob.glob(str(ROOT / "evals" / "public" / "*.json"))):
        task = json.loads(Path(f).read_text(encoding="utf-8"))
        if task.get("game") or task.get("engine_cwd"):
            continue
        steps = [s for s in task.get("setup", []) + task.get("reference", []) if isinstance(s, dict) and "call" in s]
        if any("$" in json.dumps(s.get("args", {})) for s in steps):
            continue
        calls.append(("#", {"task": task.get("id", Path(f).stem)}, None))
        calls.extend((s["call"], s.get("args", {}), s.get("as")) for s in steps)
    return calls


class Side:
    def __init__(self, label, engine, delay_rng):
        self.label, self.engine, self.delay_rng = label, engine, delay_rng
        self.agents = {}

    def agent(self, name):
        m = self.agents.get(name)
        if m is None:
            m = self.engine.mcp(f"fuzz-{name}")
            m.call("agent_hello", name=name, role="builder")
            self.agents[name] = m
        return m

    def call(self, name, tool, args):
        if self.delay_rng is not None:
            time.sleep(self.delay_rng.uniform(0, 0.04))
        m = self.agent(name)
        result = m.request("tools/call", {"name": tool, "arguments": args})
        text = next((c["text"] for c in result.get("content", []) if c.get("type") == "text"), "")
        code = None
        if result.get("isError"):
            try:
                code = json.loads(text).get("error")
            except (ValueError, AttributeError):
                code = "error"
        return code, text


def step(side, agent, ticks):
    """Steps the clock (time_control {action: step}); fails loudly when it is refused or the
    clock didn't move (review of the authority lane: a refused step made every comparison
    vacuous)."""
    before = json.loads(side.call(agent, "time_control", {"action": "info"})[1]).get("tick")
    code, text = side.call(agent, "time_control", {"action": "step", "ticks": ticks})
    if code:
        raise RuntimeError(f"{side.label}: time_control step refused: {text[:300]}")
    after = json.loads(text).get("tick")
    if not isinstance(before, int) or after != before + ticks:
        raise RuntimeError(f"{side.label}: the clock went from {before} to {after}, not by {ticks}")
    return after


def hashes(side):
    code, text = side.call("fuzz", "world_hash", {"recent": 64})
    if code:
        raise RuntimeError(f"{side.label}: world_hash refused: {text[:300]}")
    j = json.loads(text)
    return {
        "world": [tuple(x) if isinstance(x, list) else (x.get("tick"), x.get("hash")) for x in j.get("recent", [])],
        "physics": j.get("physics", {}).get("recent"),
        "nav": j.get("nav", {}).get("recent"),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe")
    ap.add_argument("--port", type=int, default=7762)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--steps", type=int, default=30, help="ticks stepped after each call")
    ap.add_argument("--out", default=str(ROOT / "target" / "hash-fuzz"))
    ap.add_argument("--no-evals", action="store_true")
    a = ap.parse_args()
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    exe = Path(a.exe) if a.exe else target / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")
    out = Path(a.out)
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    common = ["--headless", "--paused", "--assets", str(ASSETS)]
    cold, warm = out / "cache-cold", out / "cache-warm"
    # Warm B's cache: one engine builds the corpus' models and pipelines, then stops.
    with Engine(exe, a.port + 2, out / "warmup.log", *common, "--cache", str(warm), "--captures", str(out / "cap-warm"), "--worlds", str(out / "worlds-warm"), cwd=str(ROOT)) as w:
        side = Side("warmup", w, None)
        for tool, args, agent in generated_corpus(random.Random(a.seed)):
            side.call(agent, tool, args)
        step(side, "fuzz", 30)
    corpus = generated_corpus(random.Random(a.seed)) + ([] if a.no_evals else eval_corpus())
    report = {"seed": a.seed, "calls": 0, "compared": 0, "mismatches": [], "refusal_mismatches": [], "ticks": 0}
    eng_a = Engine(exe, a.port, out / "a.log", *common, "--cache", str(cold), "--physics-threads", "1", "--captures", str(out / "cap-a"), "--worlds", str(out / "worlds-a"), cwd=str(ROOT))
    eng_b = Engine(exe, a.port + 1, out / "b.log", *common, "--cache", str(warm), "--physics-threads", "4", "--captures", str(out / "cap-b"), "--worlds", str(out / "worlds-b"), cwd=str(ROOT))
    with eng_a, eng_b:
        A, B = Side("a", eng_a, None), Side("b", eng_b, random.Random(a.seed * 31 + 1))
        task = "generated"
        for tool, args, agent in corpus:
            if tool == "#":
                task = args["task"]
                # Each eval task starts from an empty world (its setup names its own entities).
                for side in (A, B):
                    for _ in range(20):
                        code, text = side.call("fuzz", "world_query", {"fields": ["path"], "limit": 100})
                        if code:
                            raise RuntimeError(f"world_query: {text[:300]}")
                        tops = sorted({e["path"].split("/")[1] for e in json.loads(text).get("items", []) if e.get("path", "").count("/") >= 1})
                        if not tops:
                            break
                        for t in tops:
                            side.call("fuzz", "world_delete", {"path": f"/{t}"})
                continue
            agent = agent or "fuzz"
            ca, ta = A.call(agent, tool, args)
            cb, tb = B.call(agent, tool, args)
            report["calls"] += 1
            if ca:
                report.setdefault("refused", []).append({"task": task, "tool": tool, "code": ca, "text": ta[:200]})
            if ca != cb:
                report["refusal_mismatches"].append({"task": task, "tool": tool, "a": ca, "b": cb, "a_text": ta[:300], "b_text": tb[:300]})
            if tool in ("world_hash", "time_control"):
                continue
            ta_, tb_ = step(A, agent, a.steps), step(B, agent, a.steps)
            if ta_ != tb_:
                report["mismatches"].append({"task": task, "after": tool, "kind": "tick", "a": ta_, "b": tb_})
            report["ticks"] = ta_
            ha, hb = hashes(A), hashes(B)
            if not ha["world"]:
                raise RuntimeError("world_hash has no per-tick hashes after a step: nothing would be compared")
            report["compared"] += 1
            report["last_world_hashes"] = ha["world"][-1:] if ha["world"] else None
            report["physics_ticks"] = len(ha["physics"] or [])
            for k in ("world", "physics", "nav"):
                if ha[k] != hb[k]:
                    report["mismatches"].append({"task": task, "after": tool, "kind": k, "a": ha[k][-3:] if ha[k] else ha[k], "b": hb[k][-3:] if hb[k] else hb[k]})
    report["ok"] = not report["mismatches"] and not report["refusal_mismatches"]
    (out / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    print(f"hash fuzz: {report['calls']} calls, {report['compared']} hash comparisons, "
          f"{len(report['mismatches'])} hash mismatches, {len(report['refusal_mismatches'])} refusal mismatches, "
          f"{len(report.get('refused', []))} calls refused on both (same code); clock at tick {report['ticks']}")
    for m in report["mismatches"][:10]:
        print(f"  MISMATCH {m['kind']} after {m['tool'] if 'tool' in m else m['after']} ({m['task']})")
    for m in report["refusal_mismatches"][:10]:
        print(f"  REFUSAL {m['tool']} ({m['task']}): a={m['a']} b={m['b']}")
    print(f"report: {out / 'report.json'}")
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
