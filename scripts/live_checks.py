#!/usr/bin/env python3
"""Runs every live check, one after the other, and says which passed: the final audit of a
milestone (AGENTS.md, "Live checks": CI has no GPU and no desktop, so these are run by hand).

Each check is a script of this folder that starts its own engines; this one only runs them,
gives each its own ports and its own output folder, keeps its log, and reports exit codes.
A check is run once: one that fails here is a finding, never retried into passing.

    python scripts/live_checks.py [--out DIR] [--port 7720] [--only NAME,NAME] [--skip NAME,NAME] [--list] [--no-build]
    python scripts/live_checks.py --self-test    # what a check left behind, on made-up states (nothing is started)

It builds the engine and the game first, debug and release, in this checkout: an executable
keeps the folder it was built from (a game finds its assets there), so one left in a target
folder by another checkout runs with that checkout's files, or with none (M5's final audit ran
17 checks into failures that way before it built its own).

Every check is told its ports: the eight from its own first port on. A check that names its
ports another way than `--port` has its flags in RUNS; one this runner has no port flag for
stops the run before anything starts (M5's audit ran three checks on their default ports,
two of them in a range other sessions use, because it only knew `--port`).

On Windows --out has a depth limit, said before anything is started: the checks build
modules in projects of their own below it, rustc hands a module's linker its arguments in a
file deep in that build, and the toolchain's linker opens a path of at most 259 characters
(M5's last audit: project_check failed 80 minutes into a run whose --out was one character
deeper than the run before it, which passed; the engine names that failure itself now).

A check leaves the checkout as it found it: after each one this runner asks git what differs
from the last commit, and a check that left something new there (a changed file, a file git
does not ignore, a further change to a file that was already changed) fails, with the paths.
(M5's last audit passed with a module's source handed back with other line endings: the check
that edits it compared its restore as text.) Not seen: a further change to a file git does not
track; run the audit in a clean checkout.

Standard library only; ASCII output. Exit 0 = every check that ran passed.
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent

# Scripts that are not live checks. Everything else in this folder is run.
NOT_CHECKS = {
    # Helpers other scripts import, the gate, this runner, the visual history's tool, and the
    # measured comparison's harness (it runs agents; its own tests are in UNIT).
    "specula_mcp.py", "window_recorder.py", "playtest_swarm.py", "gate.py", "live_checks.py", "gallery.py", "leverage.py",
    # Measurements: they print figures to compare between two builds and pass no verdict
    # (perf/README.md says how each is read).
    "perf_baseline.py", "call_rate.py", "stall_load.py", "capture_drill.py",
    # It draws on an engine someone already has open (the interactive one's port by default).
    "lava_lamps.py",
    # It judges a file of results that the exhaustive scan of dmath writes (a test run by hand,
    # hours long): a verdict, but not of anything this runner can start.
    "dmath_oracle.py",
    # A milestone's proof of its day, kept as the record of it: M4's swarm built 33 props into
    # a map that had none, and the map has shipped them since M5 (its run is refused at the
    # first spawn; the approved world is in target/m4-exit-approved).
    "m4_exit.py",
}
# What a check needs beyond its defaults (each list is one run of the script). {port} is the
# check's first port, {port1} and {port3} the next and the third after it, {out} its output
# folder, {perf} a perf.toml this runner writes whose lease pool is the check's other ports.
RUNS = {
    "evals.py": [["--check"], ["--reference", "--workers", "3", "--lead-port", "{port}", "--pool", "{port1}-{port3}"]],
    # (Its own header: "The milestone audit runs --known-ok": the stalls a later lane owns,
    # each within its listed peak, are named and do not fail the audit.)
    "budget_check.py": [["--known-ok"]],
    "desync_check.py": [["--ports", "{port}", "{port1}"]],
    "save_check.py": [["--ports", "{port}", "{port1}"]],
    # (Three workers, not its eight: each leases two ports, one to record and one to verify, and
    # the check's own eight are its lead's and seven for the pool. Its dated results go to the
    # audit's folder, not into perf/playtests.)
    "playtest_suite.py": [["--lead-port", "{port}", "--workers", "3", "--perf", "{perf}", "--results", "{out}"]],
    "toolscope_check.py": [["--results", "{out}"]],
    "balance_playtests.py": [["--lead-port", "{port}", "--pool", "{port1}-{port3}"]],
    # (Its sheet is a tracked picture in docs/fidelity: the audit's goes to the audit's folder.)
    "fidelity_t3.py": [["--sheet", "{out}"]],
}
# Checks whose --out is one JSON file, not a folder.
OUT_IS_A_FILE = {"balance_check.py", "desync_check.py", "save_check.py"}
# The flags a check names its ports with.
PORT_FLAGS = ("--port", "--ports", "--lead-port")
# Unit tests of the scripts themselves, run first.
UNIT = ["evals_test.py", "leverage_test.py"]
# What is built before anything runs (debug and release of the first; the example is a check's
# own engine, scripts/model_stress.py).
BUILDS = [
    ["cargo", "build", "-p", "specula", "-p", "sepulcra-app"],
    ["cargo", "build", "-p", "specula", "-p", "sepulcra-app", "--release"],
    ["cargo", "build", "-p", "specula", "--example", "model_stress"],
]
LIMIT_S = 2400
# Ports a check owns, from its first.
SPAN = 8


def runs_of(script, port, out):
    """(command, needs ports) for each run of `script`."""
    source = (HERE / script).read_text(encoding="utf-8", errors="replace")
    fill = {"port": port, "port1": port + 1, "port3": port + 3, "out": str(out), "perf": str(out.parent / f"{script[:-3]}.perf.toml")}
    for extra in RUNS.get(script, [[]]):
        args = [x.format(**fill) for x in extra]
        offline = script in UNIT or extra == ["--check"]
        if not offline:
            if not any(f in args for f in PORT_FLAGS) and '"--port"' in source:
                args += ["--port", str(port)]
            if '"--out"' in source:
                args += ["--out", str(out) + (".json" if script in OUT_IS_A_FILE else "")]
        yield [sys.executable, "-u", str(HERE / script)] + args, not offline


def tree_state():
    """What git says differs from the last commit in this checkout: a line a path (every file
    of a folder git does not track, not the folder alone), and one line for the changes
    themselves, so that a second change to a file is a difference too. None when git cannot
    say (no git, or not a checkout)."""
    try:
        status = subprocess.run(["git", "status", "--porcelain", "-uall"], cwd=str(ROOT), capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=300)
        diff = subprocess.run(["git", "diff", "HEAD"], cwd=str(ROOT), capture_output=True, timeout=300)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if status.returncode != 0 or diff.returncode != 0:
        return None
    return set(x for x in status.stdout.splitlines() if x.strip()) | {CHANGES + hashlib.sha256(diff.stdout).hexdigest()[:16]}


# The line of a tree's state that stands for the content of its changes.
CHANGES = "changes to tracked files: "


def left_behind(before, after):
    """What a check left in the checkout: the lines of `after` that `before` did not have. A
    file changed for the first time is named by its own line; the line for the changes is
    kept only when no path says it (a file that was already changed, changed again)."""
    if before is None or after is None:
        return []
    new = sorted(after - before)
    paths = [x for x in new if not x.startswith(CHANGES)]
    return paths if paths else (["a file that was already changed was changed again (git diff HEAD)"] if new else [])


# The deepest file a check makes below --out: the one rustc hands a module's linker its
# arguments in, in project_check's project (the names' lengths are what matter; the layout is
# Cargo's since Rust 1.100, the deeper of the two).
DEEPEST_BELOW_OUT = r"\project_check\20261115-132118\ws\.specula-cache\modules\ambulo-8c5d3caf\debug\build\ambulo\0123456789abcdef\out\rustcXXXXXX\linker-arguments"
# What the Rust toolchain's linker opens on Windows (specula_logic::build::LINKER_PATH_MAX).
LINKER_PATH_MAX = 259


def too_deep(out, windows=sys.platform == "win32"):
    """By how many characters --out is too deep for the checks' module builds (0: it is not)."""
    return max(0, len(str(out)) + len(DEEPEST_BELOW_OUT) - LINKER_PATH_MAX) if windows else 0


def self_test():
    assert len(DEEPEST_BELOW_OUT) == 141, len(DEEPEST_BELOW_OUT)
    assert too_deep("x" * 118, windows=True) == 0, "the audits that passed"
    assert too_deep("x" * 119, windows=True) == 1, "the audit whose project_check failed at its link"
    assert too_deep("x" * 400, windows=False) == 0, "no such limit off Windows"
    a = {" M x.rs", CHANGES + "aaaa"}
    assert left_behind(a, set(a)) == [], "nothing new"
    assert left_behind(a, {" M x.rs", " M y.rs", CHANGES + "bbbb"}) == [" M y.rs"], "a file changed for the first time is named"
    assert left_behind({CHANGES + "0"}, {"?? new/file.txt", CHANGES + "0"}) == ["?? new/file.txt"], "a file git does not track"
    assert left_behind(a, {" M x.rs", CHANGES + "bbbb"}) == ["a file that was already changed was changed again (git diff HEAD)"]
    assert left_behind(None, a) == [] and left_behind(a, None) == [], "git gave no answer"
    state = tree_state()
    assert state is None or sum(x.startswith(CHANGES) for x in state) == 1, state
    print("live_checks.py self-test: ok")
    return 0


def stop_tree(proc):
    """Ends a check that ran past its limit, with the engines it started (they are its
    children: ended by the check's own process id, nothing else on the machine is touched). An
    engine a check started through `specula open` is not its child and is not reached: it is in
    `specula list --settings <the check's folder>`, which the check's log names."""
    if sys.platform == "win32":
        subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    else:
        proc.kill()
    try:
        proc.wait(timeout=30)
    except subprocess.TimeoutExpired:
        proc.kill()


def main():
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    ap = argparse.ArgumentParser(description="Every live check, one after the other.")
    ap.add_argument("--out", default=str(target / "live-checks"))
    ap.add_argument("--port", type=int, default=7720, help=f"the first port: each check gets the next {SPAN}")
    ap.add_argument("--only", default="")
    ap.add_argument("--skip", default="")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--no-build", action="store_true", help="the executables were built from this checkout a moment ago")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    only = {x.strip() for x in a.only.split(",") if x.strip()}
    skip = {x.strip() for x in a.skip.split(",") if x.strip()}
    checks = sorted(p.name for p in HERE.glob("*.py") if p.name not in NOT_CHECKS and p.name not in UNIT)
    everything = UNIT + checks
    names = [n for n in everything if (not only or n[:-3] in only or n in only) and n[:-3] not in skip and n not in skip]
    if a.list:
        print("\n".join(names))
        return 0
    out = Path(a.out).resolve()
    over = too_deep(out)
    if over:
        print(f"--out is {over} character(s) too deep ({len(str(out))}: {out}). The checks build modules in projects below it, and the "
              f"linker's argument file of the deepest would be past the {LINKER_PATH_MAX} characters the Rust toolchain's linker opens on "
              f"Windows. Name a shorter folder (one near the drive's root will do). Nothing was run.")
        return 2
    out.mkdir(parents=True, exist_ok=True)
    # Each check's ports are its place in the whole list, so a run of a few uses the same
    # ports as the full run.
    plan = []
    for name in names:
        port = a.port + SPAN * everything.index(name)
        for k, (cmd, needs_ports) in enumerate(runs_of(name, port, out / name[:-3])):
            if needs_ports and not any(f in cmd for f in PORT_FLAGS):
                print(f"{name} takes no --port and RUNS names no port flag for it: it would run on its default ports. Nothing was run.")
                return 2
            plan.append((name, port, k, cmd))
    if not a.no_build:
        for cmd in BUILDS:
            print("building: " + " ".join(cmd), flush=True)
            built = subprocess.run(cmd, cwd=str(ROOT), stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")
            if built.returncode != 0:
                print(built.stderr[-2000:])
                print("the build failed: nothing was checked")
                return 2
    results = []
    tree = tree_state()
    if tree is None:
        print("git does not answer here: what a check leaves in the checkout is not looked at", flush=True)
    for name, port, k, cmd in plan:
        if any("perf.toml" in x for x in cmd):
            (out / f"{name[:-3]}.perf.toml").write_text(f"[swarm]\nlease_ports = [{port + 1}, {port + SPAN - 1}]\n", encoding="utf-8")
        label = name[:-3] + (f" ({' '.join(cmd[3:5])})" if k or len(RUNS.get(name, [])) > 1 else "")
        log = out / f"{name[:-3]}{'-' + str(k) if k else ''}.txt"
        t0 = time.time()
        with log.open("w", encoding="utf-8") as f:
            proc = subprocess.Popen(cmd, stdout=f, stderr=subprocess.STDOUT, cwd=str(ROOT))
            try:
                code = proc.wait(timeout=LIMIT_S)
            except subprocess.TimeoutExpired:
                code = "timeout"
                stop_tree(proc)
        took = time.time() - t0
        lines = [x for x in log.read_text(encoding="utf-8", errors="replace").splitlines() if x.strip()]
        last = lines[-1][:140] if lines else ""
        now = tree_state()
        left = left_behind(tree, now)
        tree = now if now is not None else tree
        if left:
            last = "left in the checkout: " + ", ".join(left)[:110]
        print(f"{'ok  ' if code == 0 and not left else 'FAIL'} {label:44} exit {code!s:8} {took:6.0f} s  {last}", flush=True)
        results.append({"check": label, "exit": code, "seconds": round(took), "ports": [port, port + SPAN - 1], "log": str(log), "last": last, "left": left})
    failed = [r for r in results if r["exit"] != 0 or r["left"]]
    (out / "report.json").write_text(json.dumps({"results": results, "failed": [r["check"] for r in failed]}, indent=1), encoding="utf-8")
    print(f"\n{len(results) - len(failed)} of {len(results)} passed; report: {out / 'report.json'}")
    for r in failed:
        print(f"FAILED: {r['check']} (exit {r['exit']}; {r['log']})" + (f"; left in the checkout: {', '.join(r['left'])}" if r["left"] else ""))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
