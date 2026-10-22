"""Live check of the module host (M3 exit proof): one agent builds the example module
(games/ambulo) into a running engine and plays its character; another agent edits the module's
source and rebuilds it while the world keeps running; the character, its state and the module's
state store survive, and the new code takes over at a tick boundary.

Also: three agents asking for the same build at once share Cargo runs, a native hook is called
with a reason, the journal still replays to the live world's hash, and a capture of the character
is saved for review.

    cargo build -p specula && python scripts/module_reload.py [--port 7496] [--window] [--adapter integrated]

The module's source is edited in place and always restored (even if the check fails). Results
go to <target>/module-reload/ (summary.json, captures, engine log), where <target> is $CARGO_TARGET_DIR
or ./target; the engine binary is taken from there too. ASCII output only.
"""
import argparse
import json
import os
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

SRC = ROOT / "games" / "ambulo" / "src" / "lib.rs"


def check(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        raise SystemExit(f"module_reload: failed: {what}")


def player(mcp):
    q = mcp.json("world_query", path_prefix="/ambulo", tags=["player"], fields=["id", "path", "position", "props", "version"])
    items = q.get("entities") or q.get("items") or []
    check(len(items) == 1, f"one player entity (got {len(items)})")
    return items[0]


def step(mcp, n):
    mcp.json("time_control", action="step", ticks=n)


def build(mcp, **extra):
    """logic_build; the answer (errors are answers too: the tool reports failure as data)."""
    result = mcp.request("tools/call", {"name": "logic_build", "arguments": {"path": "games/ambulo", **extra}})
    text = next(c["text"] for c in result["content"] if c["type"] == "text")
    try:
        return json.loads(text), result.get("isError", False)
    except json.JSONDecodeError:
        return {"raw": text}, True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7496)
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    ap.add_argument("--exe", default=str(target / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")))
    ap.add_argument("--window", action="store_true", help="open the window (default headless)")
    ap.add_argument("--adapter", default=None, help="discrete | integrated | <index>")
    ap.add_argument("--out", default=str(target / "module-reload"))
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    args = ["--captures", str(out / "captures")]
    if not a.window:
        args.append("--headless")
    if a.adapter:
        args += ["--adapter", a.adapter]
    original = SRC.read_bytes()
    summary = {}
    engine = Engine(a.exe, a.port, out / "engine.log", *args, cwd=str(ROOT))
    try:
        with engine:
            builder, editor = engine.mcp("module-builder"), engine.mcp("module-editor")
            builder.hello("ambulo-builder")
            editor.hello("ambulo-editor")
            check("logic_build" in builder.tools(), "the engine lists logic_build")

            print("builder: logic_build games/ambulo")
            r, err = build(builder)
            check(not err and r.get("ok"), f"built and loaded ({r.get('error')})")
            check(r["reload"]["status"] == "running" and r["reload"]["event"] == "load", "module running, first load")
            summary["first_build_ms"] = r["timings_ms"]
            print(f"  timings: {r['timings_ms']}")

            builder.json("time_control", action="pause")
            p0 = player(builder)
            builder.json("logic_input", module="ambulo", actions={"move": [1, 0]}, ticks=30)
            step(builder, 30)
            p1 = player(builder)
            dx = p1["position"][0] - p0["position"][0]
            check(abs(dx - 4.0) < 0.2, f"walked 4 m/s for 1 s (dx = {dx:.3f})")
            builder.json("logic_input", module="ambulo", actions={"jump": True}, ticks=1)
            step(builder, 5)
            check(player(builder)["position"][1] > 0.3, "jumped")
            step(builder, 40)
            landed = player(builder)
            # The physics controller rests a character half its 1 cm skin above the ground.
            check(abs(landed["position"][1]) < 0.006, f"landed (y = {landed['position'][1]})")

            print("editor: WALK_SPEED 4 -> 8, logic_build while the world runs")
            builder.json("time_control", action="resume")
            t0 = time.perf_counter()
            SRC.write_bytes(original.replace(b"const WALK_SPEED: f32 = 4.0;", b"const WALK_SPEED: f32 = 8.0;", 1))
            r, err = build(editor)
            edit_to_reload_ms = (time.perf_counter() - t0) * 1000
            check(not err and r["reload"]["event"] == "reload" and r["reload"]["generation"] == 2, "reloaded as generation 2")
            builder.json("time_control", action="pause")
            p2 = player(builder)
            check(p2["id"] == landed["id"], "same player entity across the reload")
            check(p2["position"] == landed["position"], "position kept across the reload")
            status = builder.json("logic_status", module="ambulo", detailed=True)
            m = status["modules"][0]
            check(m["state"].get("jumps") == 1 and m["state"].get("loads") == 2, f"state store survived ({m['state']})")
            builder.json("logic_input", module="ambulo", actions={"move": [1, 0]}, ticks=15)
            step(builder, 15)
            dx = player(builder)["position"][0] - p2["position"][0]
            check(abs(dx - 4.0) < 0.2, f"new code: 8 m/s for 0.5 s (dx = {dx:.3f})")
            summary["edit_to_reload_ms"] = edit_to_reload_ms
            summary["rebuild_timings_ms"] = r["timings_ms"]
            summary["reload_engine_ms"] = r["reload"]["timings_ms"]
            print(f"  edit -> reload {edit_to_reload_ms:.0f} ms; timings {r['timings_ms']}; in the engine {r['reload']['timings_ms']}")

            print("three agents rebuild at once (WALK_SPEED 8 -> 6)")
            SRC.write_bytes(original.replace(b"const WALK_SPEED: f32 = 4.0;", b"const WALK_SPEED: f32 = 6.0;", 1))
            agents = [engine.mcp(f"module-swarm-{i}") for i in range(3)]
            answers = [None] * 3

            def go(i):
                answers[i] = build(agents[i])

            threads = [threading.Thread(target=go, args=(i,)) for i in range(3)]
            for t in threads:
                t.start()
            for t in threads:
                t.join()
            merged = [ans[0].get("requests_merged") for ans in answers]
            check(all(not e and ans.get("ok") for ans, e in answers), "all three builds answered ok")
            builds = builder.json("logic_status")["builds"]["recent"]
            runs = len(builds) - 2  # minus the two earlier builds
            check(runs <= 3, f"{runs} cargo runs for 3 requests (requests_merged per answer: {merged})")
            summary["swarm_rebuild"] = {"requests_merged": merged, "cargo_runs": runs}
            for ag in agents:
                ag.close()

            print("native hook with a reason")
            launched = builder.json("logic_hook", hook="ambulo/launch", args={"speed": 10}, reason="demo: a knockback launch has no Lex effect yet")
            check(launched.get("vy") == 10, "hook answered")
            step(builder, 3)
            check(player(builder)["position"][1] > 0.5, "the hook launched the player")
            step(builder, 60)
            st = builder.json("logic_status", module="ambulo")
            check(st["hook_counts"].get("ambulo/launch") == 1 and st["hook_uses"][-1]["reason"].startswith("demo"), "hook use logged with its reason")
            m = st["modules"][0]
            summary["tick_us"] = m["tick_us"]
            summary["generation"] = m["generation"]
            print(f"  module tick: {m['tick_us']} us; status {m['status']}, generation {m['generation']}")

            h = builder.json("world_hash", verify=True)
            check(h["verify"].get("matches") is True, f"the journal replays to the live hash ({h['verify']})")

            pos = player(builder)["position"]
            frame = builder.frame(width=960, height=540, eye=[pos[0] + 4.0, 3.0, pos[2] + 6.0], target=[pos[0], 0.9, pos[2]], overlays=["labels"])
            summary["capture"] = frame.get("image", {})
            print(f"  capture: {frame.get('image', {}).get('path')}")
            info = builder.json("engine_info")
            check(any(x["name"] == "ambulo" for x in info.get("modules", [])), "engine_info lists the module")
            builder.close()
            editor.close()
        check(engine.exit_code == 0 or engine.validation_errors() == 0, f"clean shutdown (exit {engine.exit_code})")
    finally:
        SRC.write_bytes(original)
    check(SRC.read_bytes() == original, "module source restored")
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"module_reload: passed; summary in {out / 'summary.json'}")


if __name__ == "__main__":
    main()
