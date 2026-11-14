"""Live check of the determinism guards (docs/design/DETERMINISM.md, rules 6 and 8).

Starts a headless engine, builds the example module (games/ambulo) with logic_build, and checks:
- the floating-point guard is on and had nothing to restore (engine_info determinism);
- the load is journaled (world_history kinds ["module"]) with the build hash logic_status shows;
- a playtest recorded with that build replays and verifies in a second, fresh engine (port + 1)
  that loaded the same build (both start with --paused, so they load it at the same tick);
- after an edit that adds `x.sin()`, logic_build reports a determinism::platform_math warning, the
  reload is journaled with previous_build, and the old replay is refused with
  module_build_mismatch naming ambulo (and runs, reported, with allow_module_mismatch);
- a desync trace carries the module history; world_hash verify still matches.

The module source is restored afterwards (and rebuilt). Output goes to <target>/determinism-check/
(summary.json, engine log), <target> being $CARGO_TARGET_DIR or ./target.

    cargo build -p specula && python scripts/determinism_check.py --port 7612   (uses 7612 and 7613)
"""
import argparse
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

SRC = ROOT / "games" / "ambulo" / "src" / "lib.rs"
REPLAY = "determinism_check.json"
PROBE = "\n// determinism_check.py probe (removed again by the script)\nfn probe(x: f32) -> f32 {\n    x.sin()\n}\n"
# The probe is called from start's log line, so it is in the build (dead code would be dropped).
LOG_OLD = b'started (load {loads}, reloaded: {})", env!("CARGO_PKG_VERSION"), cx.reloaded()))'
LOG_NEW = b'started (load {loads}, reloaded: {}, probe {})", env!("CARGO_PKG_VERSION"), cx.reloaded(), probe(loads as f32)))'


def check(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        raise SystemExit(f"failed: {what}")


def build(mcp):
    result = mcp.request("tools/call", {"name": "logic_build", "arguments": {"path": "games/ambulo"}})
    text = next(c["text"] for c in result["content"] if c["type"] == "text")
    return json.loads(text)


def call(mcp, tool, **args):
    """A tool call whose error is an answer too: (json, is_error)."""
    from specula_mcp import detailed
    result = mcp.request("tools/call", {"name": tool, "arguments": detailed(tool, args)})
    text = next(c["text"] for c in result["content"] if c["type"] == "text")
    return json.loads(text), result.get("isError", False)


def module_entries(mcp):
    h = mcp.json("world_history", kinds=["module"], limit=20)
    return [e for e in h["entries"] if e.get("kind") == "module"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7612)
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    ap.add_argument("--exe", default=str(target / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")))
    ap.add_argument("--out", default=str(target / "determinism-check"))
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    original = SRC.read_bytes()
    replay_file = ROOT / "playtests" / REPLAY
    summary = {}
    # Modules are native code: the owner or a granted lead builds them (docs/design/MODULES.md).
    engine = Engine(a.exe, a.port, out / "engine.log", "--headless", "--paused", "--captures", str(out / "captures"), "--lead", "determinism-check", cwd=str(ROOT))
    try:
        with engine:
            mcp = engine.mcp("determinism-check")
            mcp.hello("determinism-check", role="lead")
            info = mcp.json("engine_info")
            fp = info["determinism"]["fp_env"]
            print(f"fp guard: {fp}")
            check(fp["checked"] and fp["restored"] == 0, f"the FP guard checks {fp['register']} and found nothing to restore")
            mcp.json("time_control", action="pause")

            print("logic_build games/ambulo")
            r = build(mcp)
            check(r.get("ok"), f"built and loaded ({r.get('error')})")
            check(r.get("determinism_warnings") == 0, "ambulo has no determinism warnings")
            status = mcp.json("logic_status", module="ambulo")
            build1 = status["modules"][0]["build_hash"]
            entries = module_entries(mcp)
            check(len(entries) == 1 and entries[0]["module"]["event"] == "load", "the load is journaled (kind module)")
            check(entries[0]["module"]["build_hash"] == build1, f"with logic_status's build hash {build1}")

            spec = {"name": "determinism_check", "steps": [
                {"at": 0, "call": "logic_input", "args": {"module": "ambulo", "actions": {"move": [1, 0]}, "ticks": 30}}],
                "ticks": 60}
            rec = mcp.json("playtest_run", spec=spec, record=REPLAY, overwrite=True, wait_s=20)
            check(rec.get("status") == "passed", f"recorded a playtest ({rec.get('status')})")
            recorded = json.loads(replay_file.read_text(encoding="utf-8"))
            check(recorded.get("modules") == [{"module": "ambulo", "build_hash": build1}], "the replay file names the module build")
            # Same build in a fresh engine, from the same start: the replay verifies.
            print("second engine: same build, replay")
            fresh = Engine(a.exe, a.port + 1, out / "engine2.log", "--headless", "--paused", "--captures", str(out / "captures2"), "--lead", "determinism-check-2", cwd=str(ROOT))
            with fresh:
                m2 = fresh.mcp("determinism-check-2")
                m2.hello("determinism-check-2", role="lead")
                r2 = build(m2)
                check(r2.get("ok"), f"the second engine built and loaded ambulo ({r2.get('error')})")
                check(m2.json("logic_status", module="ambulo")["modules"][0]["build_hash"] == build1, "with the same build hash")
                again, err = call(m2, "playtest_run", replay=f"playtests/{REPLAY}", wait_s=20)
                check(not err and again.get("replay", {}).get("verified") is True, f"the replay verifies there ({again.get('status')})")
                summary["fresh_replay"] = again.get("replay")

            print("edit: ambulo calls x.sin()")
            check(original.count(LOG_OLD) == 1, "found ambulo's start log line to call the probe from")
            SRC.write_bytes(original.replace(LOG_OLD, LOG_NEW) + PROBE.encode())
            r = build(mcp)
            check(r.get("ok"), f"rebuilt and reloaded ({r.get('error')})")
            hits = [d for d in r.get("diagnostics", []) if d.get("code") == "determinism::platform_math"]
            check(r.get("determinism_warnings") == 1 and len(hits) == 1, "logic_build reports determinism::platform_math")
            check(hits[0]["file"] == "src/lib.rs" and hits[0]["level"] == "warning", f"at src/lib.rs:{hits[0].get('line')}")
            summary["lint_hit"] = hits[0]
            build2 = mcp.json("logic_status", module="ambulo")["modules"][0]["build_hash"]
            check(build2 != build1, f"a new build ({build2})")
            entries = module_entries(mcp)
            check(entries[0]["module"]["event"] == "reload" and entries[0]["module"].get("previous_build") == build1,
                  "the reload is journaled with the build it replaced")
            refused, err = call(mcp, "playtest_run", replay=f"playtests/{REPLAY}", wait_s=20)
            check(err and refused.get("error") == "module_build_mismatch", f"the old replay is refused ({refused.get('error')})")
            check(refused["modules"][0]["module"] == "ambulo" and refused["modules"][0]["recorded"] == build1, "naming ambulo and both builds")
            summary["refusal"] = refused
            forced = mcp.json("playtest_run", replay=f"playtests/{REPLAY}", allow_module_mismatch=True, wait_s=20)
            check(len(forced.get("module_builds_differ", [])) == 1, f"with allow_module_mismatch it runs ({forced.get('status')}) and says so")

            mcp.json("time_control", action="step", ticks=5)
            trace = mcp.json("desync_trace")
            text = json.loads(Path(trace["file"]).read_text(encoding="utf-8"))
            events = text.get("modules", {}).get("events", [])
            check([e[2] for e in events] == ["load", "reload"], f"the trace carries the module history ({len(events)} events)")
            verify = mcp.json("world_hash", verify=True)
            v = verify.get("verify", verify)
            check(v.get("matches") is True, f"world_hash verify still matches ({v.get('replay_hash')})")
            summary["verify"] = verify
            print("restore the source and rebuild")
            SRC.write_bytes(original)
            r = build(mcp)
            check(r.get("ok"), f"rebuilt the original ({r.get('error')})")
            build3 = mcp.json("logic_status", module="ambulo")["modules"][0]["build_hash"]
            check(build3 == build1, f"the same source gives the same build hash ({build3}): link time stamps don't count")
            again, err = call(mcp, "playtest_run", replay=f"playtests/{REPLAY}", wait_s=20)
            check(not err and "module_builds_differ" not in again, "so the replay is accepted again")
            info = mcp.json("engine_info")
            summary["fp_env"] = info["determinism"]
            check(info["determinism"]["fp_env"]["restored"] == 0 and info["determinism"]["fp_env_modules"]["restored"] == 0,
                  "no floating-point environment changes in the whole run")
    finally:
        SRC.write_bytes(original)
        if replay_file.exists():
            replay_file.unlink()
    (out / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"all checks passed; summary in {out / 'summary.json'}")


if __name__ == "__main__":
    main()
