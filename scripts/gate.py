"""The gate: what a change must pass before it is reported or merged, as one command.

    python scripts/gate.py                  # everything
    python scripts/gate.py --only tests,repeat,slow
    python scripts/gate.py --self-test      # the log parser's own checks (no cargo)

Steps (each one's log is kept; the summary is <target>/gate/summary.json):

0. gpu      which adapters the tests will see. GPU tests skip themselves cleanly without one (so
            CI stays green), which on a developer's machine would be a gate that tested nothing:
            no adapter fails the gate unless --no-gpu says that is expected.
1. tests    cargo test --no-fail-fast --workspace, once.
2. repeat   the engine's library tests again, --repeat times (default 5) at --threads test threads
            (default 16). One run can't tell a solid test from one that passes three times in four.
3. slow     the same tests once more with SPECULA_TEST_WORKER_DELAY_MS set: every worker thread
            starts its job late. A result that depends on when a worker runs then fails every
            time, not now and then (crates/specula/src/workers.rs, `test_delay`).
4. speed    the tests that hold a speed budget (`speed_budget(..)`), one at a time, with
            SPECULA_SPEED_TESTS set: alone, their tight bounds apply. In every other run those
            bounds are 5 s or more, because "under 20 ms" among sixteen test threads is luck
            (crates/specula/tests/clock_guard.rs holds tests to that).
5. clippy   Windows and Linux, warnings denied.
6. builds   debug and release of the engine and the game (the live scripts run both).

A test that failed in some runs and passed in others is reported as FLAKY and fails the gate: it
is a bug whose cause hasn't been found yet, never "noise" (owner, 2026-11-12: a gate must not pass
by luck). The gate reads whole logs: a pass line at the end doesn't hide a failure in the middle.
A run in which no test ran, or one that outlives its time limit, is a failure too.

Standard library only. ASCII output. Honours CARGO_TARGET_DIR.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STEPS = ["tests", "repeat", "slow", "speed", "clippy", "builds"]
TEST_STEPS = ["tests", "repeat", "slow", "speed"]
# Rule 3: an environment RUSTFLAGS replaces the config's, so the linker flag is said again.
DENY = "-Dwarnings -C link-self-contained=yes" if sys.platform == "win32" else "-Dwarnings"
TIMED_OUT = -999

TEST_LINE = re.compile(r"^test (.+?) \.\.\. (ok|FAILED|ignored)\b")
RESULT_LINE = re.compile(r"^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored")
RUNNING_LINE = re.compile(r"^test (.+?) has been running for over \d+ seconds")
FN_LINE = re.compile(r"^\s*(?:pub(?:\([a-z]+\))? )?(?:async )?fn (\w+)\s*[(<]")
GPU_LINE = re.compile(r"^gate-gpu: (\d+) adapter")


def parse(log):
    """What a cargo test log says: passed and failed counts, the failed tests' names, whether
    something didn't compile or a test binary died (a run that ends without its result line), and
    the tests that were still running when the log ends."""
    passed = failed = 0
    names = []
    broken = []
    running = []
    for line in log.splitlines():
        m = TEST_LINE.match(line)
        if m:
            if m.group(2) == "FAILED":
                names.append(m.group(1))
            if m.group(1) in running:
                running.remove(m.group(1))
        m = RUNNING_LINE.match(line)
        if m and m.group(1) not in running:
            running.append(m.group(1))
        m = RESULT_LINE.match(line)
        if m:
            passed += int(m.group(1))
            failed += int(m.group(2))
        if line.startswith("error: could not compile") or line.startswith("error[E"):
            broken.append(line.strip()[:160])
        if line.startswith("error: test failed, to rerun pass") and not names:
            broken.append(line.strip()[:160])
        if "(exit code: 0x" in line or "process didn't exit successfully" in line:
            broken.append(line.strip()[:160])
    return {"passed": passed, "failed": failed, "failed_tests": sorted(set(names)), "broken": broken[:8], "still_running": running[:8], "drivers_not_loaded": driver_notes(log)}


def driver_notes(log):
    """How many times the Vulkan loader said a GPU driver failed to load in a test process. Not a
    failure of the code under test (the engine rates it a warning), but those tests ran without
    that adapter, so the summary says so instead of losing it."""
    return sum(1 for line in log.splitlines() if "Failed loading library associated with ICD" in line)


def verdict(r, code):
    """Why a test run is not a pass (an empty list when it is one)."""
    why = []
    if code == TIMED_OUT:
        why.append(f"timed out; still running: {r['still_running'] or 'unknown (see the log)'}")
    if r["failed"] or r["failed_tests"]:
        why.append(f"{r['failed']} failed {r['failed_tests'][:12]}")
    if r["broken"]:
        why.append(f"broken: {r['broken'][:3]}")
    if code != 0 and not why:
        why.append(f"exit {code} (see the log)")
    if not why and r["passed"] == 0:
        why.append("no test ran (a filter that matches nothing, or nothing was built)")
    return why


def flaky(runs):
    """Tests that failed in some of `runs` (each a parse result) and not in others, with how often."""
    seen = {}
    for r in runs:
        for t in r["failed_tests"]:
            seen[t] = seen.get(t, 0) + 1
    return {t: n for t, n in sorted(seen.items()) if n < len(runs)}


def speed_tests_in(rel, text, found, package):
    """Adds to `found` ({package: names}) the functions in `text` that hold a speed budget."""
    name = None
    for line in text.splitlines():
        m = FN_LINE.match(line)
        if m:
            name = m.group(1)
        elif "speed_budget(" in line.split("//")[0] and name and name != "speed_budget":
            found.setdefault(package, set()).add(name)


def package_of(path):
    """The cargo package a source file belongs to (the nearest Cargo.toml above it)."""
    for folder in path.parents:
        manifest = folder / "Cargo.toml"
        if manifest.is_file():
            m = re.search(r'^name\s*=\s*"([^"]+)"', manifest.read_text(encoding="utf-8"), re.M)
            return m.group(1) if m else None
    return None


def speed_tests():
    """{package: sorted names} of every test in the tree that holds a speed budget."""
    found = {}
    for base in ("crates", "games"):
        for path in sorted((ROOT / base).rglob("*.rs")):
            rel = path.relative_to(ROOT).as_posix()
            if "/target/" in rel or rel.endswith("tests/clock_guard.rs"):
                continue
            text = path.read_text(encoding="utf-8", errors="replace")
            if "speed_budget(" in text:
                speed_tests_in(rel, text, found, package_of(path))
    return {p: sorted(names) for p, names in sorted(found.items()) if p}


def kill_tree(p):
    """Stops a step that outlived its limit, with the test binaries it started (by its own PID)."""
    if sys.platform == "win32":
        subprocess.run(["taskkill", "/T", "/F", "/PID", str(p.pid)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    else:
        p.kill()
    try:
        p.wait(timeout=30)
    except subprocess.TimeoutExpired:
        pass


def run(name, cmd, out, env=None, quiet=False, minutes=120):
    """Runs `cmd` with its whole output going to <out>/<name>.log as it is written; returns (exit
    code, text, seconds). A run past `minutes` is stopped and its code is TIMED_OUT."""
    started = time.time()
    path = out / f"{name}.log"
    with open(path, "wb") as f:
        p = subprocess.Popen(cmd, cwd=str(ROOT), env=env, stdout=f, stderr=subprocess.STDOUT)
        try:
            code = p.wait(timeout=minutes * 60)
        except subprocess.TimeoutExpired:
            kill_tree(p)
            code = TIMED_OUT
    text = path.read_text(encoding="utf-8", errors="replace")
    took = time.time() - started
    if not quiet:
        print(f"  {name}: {'TIMED OUT' if code == TIMED_OUT else f'exit {code}'} in {took:.0f} s")
    return code, text, took


def self_test():
    ok_log = "running 2 tests\ntest a::b ... ok\ntest a::c ... ok\n\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
    bad_log = ("running 3 tests\ntest a::b ... ok\ntest a::flaky ... FAILED\ntest a::c ... ok\n\nfailures:\n    a::flaky\n\n"
               "test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n\ntest result: ok. 5 passed; 0 failed; 1 ignored; 0 measured\n")
    a, b = parse(ok_log), parse(bad_log)
    assert a == {"passed": 2, "failed": 0, "failed_tests": [], "broken": [], "still_running": [], "drivers_not_loaded": 0}, a
    assert parse("t: [Warning] Loader Message loader_icd_scan: Failed loading library associated with ICD JSON x.dll. Ignoring this JSON\n")["drivers_not_loaded"] == 1
    assert b["passed"] == 7 and b["failed"] == 1 and b["failed_tests"] == ["a::flaky"], b
    # A failure in the middle counts though the last line says ok.
    assert bad_log.rstrip().endswith("0 measured") and b["failed"] == 1
    assert verdict(a, 0) == [] and verdict(b, 101), "a pass is a pass; a failure in the middle is not"
    assert flaky([a, b, a]) == {"a::flaky": 1}, flaky([a, b, a])
    assert flaky([b, b]) == {}, "failing every time isn't flaky: it is plainly failing"
    c = parse("error[E0599]: no method named `x`\nerror: could not compile `specula` (lib test) due to 1 previous error\n")
    assert len(c["broken"]) == 2 and c["passed"] == 0 and verdict(c, 101), c
    d = parse("error: test failed, to rerun pass `-p specula --lib`\n")
    assert d["broken"], "a test binary that died without naming a test is not a pass"
    # Review R2 (m5-gpuwork): three ways the gate could say PASSED with nothing proven.
    none = parse("running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out\n")
    assert verdict(none, 0) == ["no test ran (a filter that matches nothing, or nothing was built)"], verdict(none, 0)
    hung = parse("running 2 tests\ntest a::b ... ok\ntest a::slow has been running for over 60 seconds\ntest a::stuck has been running for over 60 seconds\ntest a::slow ... ok\n")
    assert hung["still_running"] == ["a::stuck"], hung
    assert "a::stuck" in verdict(hung, TIMED_OUT)[0], verdict(hung, TIMED_OUT)
    assert verdict(a, 3) == ["exit 3 (see the log)"], "a non-zero exit with a clean log is not a pass"
    assert GPU_LINE.match("gate-gpu: 2 adapter(s): A, B").group(1) == "2" and not GPU_LINE.match("gate-gpu: none (Vulkan is not available)")
    # Review R3: a doc-test's name has spaces; it is named like any other failure (so it can be
    # reported as flaky).
    doc = parse("running 1 test\ntest crates/x/src/lib.rs - speed_budget (line 12) ... FAILED\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n")
    assert doc["failed_tests"] == ["crates/x/src/lib.rs - speed_budget (line 12)"], doc
    # The speed step finds the tests that hold a budget, not the helper's own definition.
    found = {}
    text = ("fn speed_budget(tight: D) -> D {\n    tight\n}\n\n#[test]\nfn picking_is_quick() {\n    let t = now();\n"
            "    assert!(t.elapsed() < speed_budget(ms(20)));\n}\n\n#[test]\nfn other() {\n    // speed_budget(..) in a comment\n    assert!(true);\n}\n")
    speed_tests_in("x.rs", text, found, "pkg")
    assert found == {"pkg": {"picking_is_quick"}}, found
    print("gate.py self-test: ok")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--jobs", type=int, default=8, help="cargo's -j")
    ap.add_argument("--repeat", type=int, default=5, help="extra runs of the engine's library tests")
    ap.add_argument("--threads", type=int, default=16, help="test threads in the repeat and slow runs")
    ap.add_argument("--slow-workers-ms", type=int, default=30, help="how late each worker starts a job in the slow run")
    ap.add_argument("--package", default="specula", help="the package whose library tests are repeated")
    ap.add_argument("--only", default=",".join(STEPS), help=f"steps to run, of {','.join(STEPS)}")
    ap.add_argument("--no-gpu", action="store_true", help="this machine has no Vulkan adapter (CI): GPU tests skipping themselves is expected")
    ap.add_argument("--out", default=None, help="where logs and the summary go (default <target>/gate)")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    only = [s.strip() for s in a.only.split(",") if s.strip()]
    unknown = [s for s in only if s not in STEPS]
    if unknown:
        print(f"unknown step(s) {unknown}: choose from {STEPS}")
        return 2
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    out = Path(a.out) if a.out else target / "gate"
    out.mkdir(parents=True, exist_ok=True)
    j = ["-j", str(a.jobs)]
    summary = {"schema": "specula.gate/v2", "steps": {}, "failures": []}
    fail = summary["failures"].append
    lib = ["cargo", "test", *j, "-p", a.package, "--lib", "--", "--test-threads", str(a.threads)]

    if any(s in only for s in TEST_STEPS):
        print("gpu (the adapters the tests will see)")
        code, log, _ = run("gpu", ["cargo", "test", *j, "-p", "specula-gpu", "--test", "adapters", "--", "--nocapture"], out, quiet=True, minutes=30)
        lines = [line.strip() for line in log.splitlines() if line.startswith("gate-gpu:")]
        adapters = next((int(m.group(1)) for m in map(GPU_LINE.match, lines) if m), 0)
        summary["steps"]["gpu"] = {"exit": code, "adapters": adapters, "line": lines[0][:300] if lines else ""}
        print(f"  {lines[0][10:] if lines else 'no report (see gpu.log)'}")
        if code != 0 or not lines:
            fail("gpu: the adapter report didn't run (see gpu.log)")
        elif adapters == 0 and not a.no_gpu:
            fail("gpu: no Vulkan adapter, so every GPU test would skip itself and prove nothing (pass --no-gpu where that is expected)")

    if "tests" in only:
        print("tests (the whole workspace, once)")
        code, log, took = run("tests", ["cargo", "test", *j, "--no-fail-fast", "--workspace"], out, minutes=150)
        r = parse(log)
        summary["steps"]["tests"] = {**r, "exit": code, "seconds": round(took)}
        print(f"  {r['passed']} passed, {r['failed']} failed")
        if r["drivers_not_loaded"]:
            print(f"  NOTE: a GPU driver failed to load {r['drivers_not_loaded']} time(s) in this run: those tests ran without that adapter")
        for why in verdict(r, code):
            fail(f"tests: {why}")

    runs = []
    if "repeat" in only:
        print(f"repeat ({a.package}'s library tests, {a.repeat} more runs at {a.threads} test threads)")
        for i in range(a.repeat):
            code, log, took = run(f"repeat-{i + 1}", lib, out, minutes=45)
            r = parse(log)
            r["exit"] = code
            runs.append(r)
            for why in verdict(r, code):
                fail(f"repeat run {i + 1}: {why}")
        counts = sorted({r["passed"] + r["failed"] for r in runs})
        summary["steps"]["repeat"] = {"runs": len(runs), "failed_runs": sum(1 for r in runs if verdict(r, r["exit"])), "tests_each": counts}
        if len(counts) > 1:
            fail(f"repeat: the runs disagree on how many tests there are ({counts}): a test binary ended early")

    if "slow" in only:
        print(f"slow workers (every worker starts its job {a.slow_workers_ms} ms late)")
        env = dict(os.environ, SPECULA_TEST_WORKER_DELAY_MS=str(a.slow_workers_ms))
        code, log, took = run("slow-workers", lib, out, env=env, minutes=60)
        r = parse(log)
        r["exit"] = code
        summary["steps"]["slow"] = {**r, "seconds": round(took)}
        for why in verdict(r, code):
            fail(f"slow workers: {why}: these depend on when a worker runs")
        runs.append(r)

    odd = flaky(runs) if len(runs) > 1 else {}
    if odd:
        summary["flaky"] = odd
        for t, n in odd.items():
            print(f"  FLAKY {t}: failed {n} of {len(runs)} runs (a bug: find its cause)")
        fail(f"flaky tests (failed in some runs, passed in others): {odd}")

    if "speed" in only:
        budgets = speed_tests()
        total = sum(len(names) for names in budgets.values())
        print(f"speed ({total} tests that hold a speed budget, one at a time, with their tight bounds)")
        env = dict(os.environ, SPECULA_SPEED_TESTS="1")
        step = summary["steps"]["speed"] = {"tests": total, "packages": {}}
        if total == 0:
            fail("speed: found no test that holds a speed budget (has `speed_budget(` been renamed?)")
        for package, names in budgets.items():
            code, log, took = run(f"speed-{package}", ["cargo", "test", *j, "-p", package, "--", "--test-threads", "1", *names], out, env=env, minutes=45)
            r = parse(log)
            step["packages"][package] = {**r, "exit": code, "named": len(names), "seconds": round(took)}
            for why in verdict(r, code):
                fail(f"speed ({package}): {why}: over budget running alone, with nothing else of the gate's on the machine")
            if not verdict(r, code) and r["passed"] < len(names):
                fail(f"speed ({package}): {len(names)} functions hold a budget and {r['passed']} tests ran: a budget in a helper isn't run here ({names})")

    if "clippy" in only:
        print("clippy (warnings denied)")
        env = dict(os.environ, RUSTFLAGS=DENY)
        code, log, _ = run("clippy", ["cargo", "clippy", *j, "--workspace", "--all-targets", "--", "-D", "warnings"], out, env=env, minutes=90)
        summary["steps"]["clippy"] = {"exit": code}
        if code != 0:
            fail("clippy: " + "; ".join(line.strip()[:140] for line in log.splitlines() if line.startswith("error"))[:600])
        if sys.platform == "win32":
            env = dict(os.environ, RUSTFLAGS="-Dwarnings")
            code, log, _ = run("clippy-linux", ["cargo", "clippy", *j, "--workspace", "--all-targets", "--target", "x86_64-unknown-linux-gnu", "--", "-D", "warnings"], out, env=env, minutes=90)
            summary["steps"]["clippy_linux"] = {"exit": code}
            if code != 0:
                fail("clippy (Linux): " + "; ".join(line.strip()[:140] for line in log.splitlines() if line.startswith("error"))[:600])

    if "builds" in only:
        print("builds (debug and release: the live scripts run both)")
        for name, extra in (("build-debug", []), ("build-release", ["--release"])):
            code, _, _ = run(name, ["cargo", "build", *j, *extra, "-p", "specula", "-p", "sepulcra-app"], out, minutes=120)
            summary["steps"][name] = {"exit": code}
            if code != 0:
                fail(f"{name}: {'timed out' if code == TIMED_OUT else f'exit {code}'} (see its log)")

    (out / "summary.json").write_text(json.dumps(summary, indent=1) + "\n", encoding="utf-8")
    if summary["failures"]:
        print(f"GATE FAILED ({len(summary['failures'])}):")
        for f in summary["failures"]:
            print(f"  - {f[:400]}")
        print(f"logs: {out}")
        return 1
    print(f"GATE PASSED (logs: {out})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
