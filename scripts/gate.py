"""The gate: what a change must pass before it is reported or merged, as one command.

    python scripts/gate.py                  # everything
    python scripts/gate.py --only tests,repeat,slow
    python scripts/gate.py --self-test      # the log parser's own checks (no cargo)

Steps (each one's log is kept; the summary is <target>/gate/summary.json):

1. tests    cargo test --no-fail-fast --workspace, once.
2. repeat   the engine's library tests again, --repeat times (default 5) at --threads test threads
            (default 16). One run can't tell a solid test from one that passes three times in four.
3. slow     the same tests once more with SPECULA_TEST_WORKER_DELAY_MS set: every worker thread
            starts its job late. A result that depends on when a worker runs then fails every
            time, not now and then (crates/specula/src/workers.rs, `test_delay`).
4. clippy   Windows and Linux, warnings denied.
5. builds   debug and release of the engine and the game (the live scripts run both).

A test that failed in some runs and passed in others is reported as FLAKY and fails the gate: it
is a bug whose cause hasn't been found yet, never "noise" (owner, 2026-11-12: a gate must not pass
by luck). The gate reads whole logs: a pass line at the end doesn't hide a failure in the middle.

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
STEPS = ["tests", "repeat", "slow", "clippy", "builds"]
# Rule 3: an environment RUSTFLAGS replaces the config's, so the linker flag is said again.
DENY = "-Dwarnings -C link-self-contained=yes" if sys.platform == "win32" else "-Dwarnings"

TEST_LINE = re.compile(r"^test (\S+) \.\.\. (ok|FAILED|ignored)\b")
RESULT_LINE = re.compile(r"^test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored")


def parse(log):
    """What a cargo test log says: passed and failed counts, the failed tests' names, and whether
    something didn't compile or a test binary died (a run that ends without its result line)."""
    passed = failed = 0
    names = []
    broken = []
    for line in log.splitlines():
        m = TEST_LINE.match(line)
        if m and m.group(2) == "FAILED":
            names.append(m.group(1))
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
    return {"passed": passed, "failed": failed, "failed_tests": sorted(set(names)), "broken": broken[:8]}


def flaky(runs):
    """Tests that failed in some of `runs` (each a parse result) and not in others, with how often."""
    seen = {}
    for r in runs:
        for t in r["failed_tests"]:
            seen[t] = seen.get(t, 0) + 1
    return {t: n for t, n in sorted(seen.items()) if n < len(runs)}


def run(name, cmd, out, env=None, quiet=False):
    """Runs `cmd`, keeps its whole output in <out>/<name>.log, returns (exit code, text, seconds)."""
    started = time.time()
    p = subprocess.run(cmd, cwd=str(ROOT), env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace")
    (out / f"{name}.log").write_text(p.stdout, encoding="utf-8")
    took = time.time() - started
    if not quiet:
        print(f"  {name}: exit {p.returncode} in {took:.0f} s")
    return p.returncode, p.stdout, took


def self_test():
    ok_log = "running 2 tests\ntest a::b ... ok\ntest a::c ... ok\n\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
    bad_log = ("running 3 tests\ntest a::b ... ok\ntest a::flaky ... FAILED\ntest a::c ... ok\n\nfailures:\n    a::flaky\n\n"
               "test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n\ntest result: ok. 5 passed; 0 failed; 1 ignored; 0 measured\n")
    a, b = parse(ok_log), parse(bad_log)
    assert a == {"passed": 2, "failed": 0, "failed_tests": [], "broken": []}, a
    assert b["passed"] == 7 and b["failed"] == 1 and b["failed_tests"] == ["a::flaky"], b
    # A failure in the middle counts though the last line says ok.
    assert bad_log.rstrip().endswith("0 measured") and b["failed"] == 1
    assert flaky([a, b, a]) == {"a::flaky": 1}, flaky([a, b, a])
    assert flaky([b, b]) == {}, "failing every time isn't flaky: it is plainly failing"
    c = parse("error[E0599]: no method named `x`\nerror: could not compile `specula` (lib test) due to 1 previous error\n")
    assert len(c["broken"]) == 2 and c["passed"] == 0, c
    d = parse("error: test failed, to rerun pass `-p specula --lib`\n")
    assert d["broken"], "a test binary that died without naming a test is not a pass"
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
    summary = {"schema": "specula.gate/v1", "steps": {}, "failures": []}
    fail = summary["failures"].append
    lib = ["cargo", "test", *j, "-p", a.package, "--lib", "--", "--test-threads", str(a.threads)]

    if "tests" in only:
        print("tests (the whole workspace, once)")
        code, log, took = run("tests", ["cargo", "test", *j, "--no-fail-fast", "--workspace"], out)
        r = parse(log)
        summary["steps"]["tests"] = {**r, "exit": code, "seconds": round(took)}
        print(f"  {r['passed']} passed, {r['failed']} failed")
        if code != 0 or r["failed"] or r["broken"]:
            fail(f"tests: {r['failed']} failed {r['failed_tests'][:12]} {r['broken'][:3]}")

    runs = []
    if "repeat" in only:
        print(f"repeat ({a.package}'s library tests, {a.repeat} more runs at {a.threads} test threads)")
        for i in range(a.repeat):
            code, log, took = run(f"repeat-{i + 1}", lib, out)
            r = parse(log)
            r["exit"] = code
            runs.append(r)
            if code != 0 or r["failed"] or r["broken"]:
                fail(f"repeat run {i + 1}: {r['failed']} failed {r['failed_tests'][:12]} {r['broken'][:3]}")
        summary["steps"]["repeat"] = {"runs": len(runs), "failed_runs": sum(1 for r in runs if r["exit"] != 0 or r["failed"]), "passed_each": [r["passed"] for r in runs]}

    if "slow" in only:
        print(f"slow workers (every worker starts its job {a.slow_workers_ms} ms late)")
        env = dict(os.environ, SPECULA_TEST_WORKER_DELAY_MS=str(a.slow_workers_ms))
        code, log, took = run("slow-workers", lib, out, env=env)
        r = parse(log)
        r["exit"] = code
        summary["steps"]["slow"] = {**r, "seconds": round(took)}
        if code != 0 or r["failed"] or r["broken"]:
            fail(f"slow workers: {r['failed']} failed {r['failed_tests'][:12]} {r['broken'][:3]}: these depend on when a worker runs")
        runs.append(r)

    odd = flaky(runs) if len(runs) > 1 else {}
    if odd:
        summary["flaky"] = odd
        for t, n in odd.items():
            print(f"  FLAKY {t}: failed {n} of {len(runs)} runs (a bug: find its cause)")
        fail(f"flaky tests (failed in some runs, passed in others): {odd}")

    if "clippy" in only:
        print("clippy (warnings denied)")
        env = dict(os.environ, RUSTFLAGS=DENY)
        code, log, _ = run("clippy", ["cargo", "clippy", *j, "--workspace", "--all-targets", "--", "-D", "warnings"], out, env=env)
        summary["steps"]["clippy"] = {"exit": code}
        if code != 0:
            fail("clippy: " + "; ".join(line.strip()[:140] for line in log.splitlines() if line.startswith("error"))[:600])
        if sys.platform == "win32":
            env = dict(os.environ, RUSTFLAGS="-Dwarnings")
            code, log, _ = run("clippy-linux", ["cargo", "clippy", *j, "--workspace", "--all-targets", "--target", "x86_64-unknown-linux-gnu", "--", "-D", "warnings"], out, env=env)
            summary["steps"]["clippy_linux"] = {"exit": code}
            if code != 0:
                fail("clippy (Linux): " + "; ".join(line.strip()[:140] for line in log.splitlines() if line.startswith("error"))[:600])

    if "builds" in only:
        print("builds (debug and release: the live scripts run both)")
        for name, extra in (("build-debug", []), ("build-release", ["--release"])):
            code, _, _ = run(name, ["cargo", "build", *j, *extra, "-p", "specula", "-p", "sepulcra-app"], out)
            summary["steps"][name] = {"exit": code}
            if code != 0:
                fail(f"{name}: exit {code} (see its log)")

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
