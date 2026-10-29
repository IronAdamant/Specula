"""The strict-budget harness (docs/design/WORKERS.md): every tool at the largest arguments its schema
allows, and the heavy tools at their largest *valid* arguments, against a headless engine run with
--strict-budget. Passes only when no engine-thread call or frame step went past its class budget
(trace_incidents {code: "engine.stall"} is empty) and the engine exits cleanly.

1. A world of 20,000 entities (20 batches of 1,000), a scratch lease filled with 20,000 files, a copy
   of Sepulcra's assets (a rigged creature to check and film).
2. Fuzz with schema maxima: for every tool, each top-level property at its declared maxLength or
   maxItems (and an argument name of 64 characters). The bounds refuse or bound these cheaply.
3. The heavy tools at their largest valid arguments: 2048x2048 sight_frame, the largest orbit and
   compare, world_query at 500 entities with every field, a 1,000-op batch, anim_check on every clip,
   the largest filmstrip, world_save of 20k entities, world_hash {verify} over the whole journal,
   lease_release {delete} of 20k files, project_write of a ~1 MiB file.
4. Live probes from the review of 998af3d: desync_trace of a long run; 300 scratch releases then a
   20,000-file delete (no worker job may run inline, a save right after answers promptly); twenty
   agents looping world_hash {verify} while a playtest runs twelve verify steps (the playtest
   passes, the agents are served).
5. Stalls are read from trace_incidents; each is printed with its tool, class, ms and budget, marked
   KNOWN when it is one of the follow-up lane's (2026-10-30: the capture subject list, readback and
   orbit, world_query and world_list scans, the vfx library rescan, the save plan, single anim
   steps) or NEW. --known-ok passes a run whose only stalls are known ones.

    cargo build -p specula && python scripts/budget_check.py [--port 7810] [--adapter integrated] [--skip-fuzz]

Results: <target>/budget-check/summary.json (and the engine's log). The integrator runs it at
milestone audits. ASCII output only.
"""
import argparse
import json
import threading
import os
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

FAILS = []


def check(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        FAILS.append(what)
    return cond


def call_raw(mcp, tool, args):
    """A tool call whose errors are answers: (json or None, is_error, seconds, bytes)."""
    t = time.perf_counter()
    result = mcp.request("tools/call", {"name": tool, "arguments": args})
    took = time.perf_counter() - t
    text = next((c["text"] for c in result["content"] if c["type"] == "text"), "")
    size = sum(len(c.get("text", "")) for c in result["content"])
    try:
        j = json.loads(text) if text else None
    except json.JSONDecodeError:
        j = None
    return j, bool(result.get("isError")), took, size


def schema_max(prop):
    """The largest value a property's schema allows (strings and arrays; None otherwise)."""
    t = prop.get("type")
    types = t if isinstance(t, list) else [t]
    if "string" in types and "maxLength" in prop:
        return "a" * int(prop["maxLength"])
    if "array" in types and "maxItems" in prop:
        n = int(prop["maxItems"])
        items = prop.get("items") or {}
        it = items.get("type") if isinstance(items, dict) else None
        if it == "string":
            # Distinct, in case the array is a set.
            return [f"x{i}" for i in range(n)]
        return list(range(n))
    return None


def fuzz(mcp, tools, slow):
    calls = 0
    skip = {"tools_invoke", "time_control", "world_load", "save_read", "logic_unload", "lex_live"}
    for t in tools:
        name = t["name"]
        if name in skip:
            continue
        props = (t.get("inputSchema") or {}).get("properties") or {}
        cases = [({"k" * 64: 1}, "a 64-character argument name")]
        for k, p in props.items():
            v = schema_max(p)
            if v is not None:
                cases.append(({k: v}, f"{k} at its maximum"))
        for args, what in cases:
            j, err, took, size = call_raw(mcp, name, args)
            calls += 1
            if size > 64 * 1024 or took > 5:
                slow.append({"tool": name, "case": what, "s": round(took, 3), "bytes": size})
    return calls


# Stalls the follow-up lane owns (docs/design/WORKERS.md, "What it found").
# desync_trace: its file is rendered and written on a worker; gathering the ticks from the journal
# (O(entities changed)) still runs on the engine thread.
KNOWN = {"sight_frame", "sight_compare", "sight_orbit", "sight_query", "world_query", "world_list", "vfx_get", "world_save", "anim_check", "anim_filmstrip", "engine_info", "desync_trace"}


def stalls(mcp):
    out = mcp.json("trace_incidents", code="engine.stall", limit=50)
    return out.get("incidents", [])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7810)
    ap.add_argument("--adapter", default=None)
    ap.add_argument("--exe", default=None)
    ap.add_argument("--skip-fuzz", action="store_true")
    ap.add_argument("--entities", type=int, default=20_000)
    ap.add_argument("--files", type=int, default=20_000)
    ap.add_argument("--known-ok", action="store_true", help="pass when the only stalls are the known ones")
    a = ap.parse_args()
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    exe = Path(a.exe) if a.exe else target / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")
    out = target / "budget-check"
    if out.exists():
        shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True, exist_ok=True)
    assets = out / "assets"
    shutil.copytree(ROOT / "games" / "sepulcra" / "assets", assets)
    args = ["--headless", "--strict-budget", "--captures", str(out / "captures"), "--worlds", str(out / "worlds"), "--assets", str(assets), "--saves", str(out / "saves")]
    if a.adapter:
        args += ["--adapter", a.adapter]
    summary = {"stalls": [], "slow": [], "heavy": {}}
    print(f"budget check: {exe} on port {a.port}")
    with Engine(exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as eng:
        lead = eng.mcp("budget-lead")
        lead.json("agent_hello", name="budget_lead", role="lead")
        tools = lead.request("tools/list", {})["tools"]
        print(f"  {len(tools)} tools")

        # 1. The world.
        per = 1000
        for b in range(a.entities // per):
            ops = [{"op": "spawn", "shape": "cube", "name": f"c{b}_{i}", "position": [i % 100, b, i // 100], "tags": ["bulk"]} for i in range(per)]
            j, err, took, _ = call_raw(lead, "world_batch", {"ops": ops})
            if err:
                check(False, f"world_batch {b} spawned ({j})")
                break
        info = lead.json("engine_info", response_format="detailed")
        check(info["entities"] >= a.entities, f"{info['entities']} entities in the world")
        lease = lead.json("lease_take", kind="scratch", lease_s=600)
        scratch = Path(lease["dir"])
        for i in range(a.files):
            sub = scratch / f"d{i // 1000}"
            if i % 1000 == 0:
                sub.mkdir(parents=True, exist_ok=True)
            (sub / f"f{i}.bin").write_bytes(b"x" * 64)
        print(f"  scratch lease filled: {a.files} files")
        base = stalls(lead)
        check(not base, f"no stalls while the world was built ({len(base)})")

        # 2. Fuzz with schema maxima.
        if not a.skip_fuzz:
            n = fuzz(lead, tools, summary["slow"])
            print(f"  fuzz: {n} calls")
            check(not summary["slow"], f"every fuzz answer under 64 KiB and 5 s ({summary['slow'][:5]})")

        # 3. The heavy tools at their largest valid arguments.
        heavy = [
            ("sight_frame", {"width": 2048, "height": 2048}),
            ("sight_orbit", {"point": [50, 10, 5], "views": 12, "tile_width": 1024, "tile_height": 1024}),
            ("sight_compare", {"action": "mark", "name": "before", "width": 2048, "height": 2048}),
            ("sight_compare", {"action": "diff", "name": "before"}),
            ("world_query", {"limit": 500, "fields": ["all"]}),
            ("world_batch", {"ops": [{"op": "spawn", "shape": "sphere", "name": f"s{i}", "position": [i % 50, 30, i // 50]} for i in range(1000)]}),
            ("anim_check", {"recipe": "recipes/shambler.json"}),
            ("anim_filmstrip", {"recipe": "recipes/shambler.json", "frames": 16, "tile": 256}),
            ("world_save", {"name": "budget"}),
            ("world_hash", {"verify": True}),
            ("lease_release", {"id": lease["id"], "delete": True}),
            ("project_write", {"path": "assets/ui/zz_budget_note.json", "text": json.dumps({"note": "n" * (1000 * 1000)}), "allow_errors": True}),
        ]
        # The creature's build first (anim tools answer model_building until it is built).
        call_raw(lead, "model_build", {"recipe": "recipes/shambler.json"})
        for tool, args_ in heavy:
            j, err, took, size = call_raw(lead, tool, args_)
            summary["heavy"][tool + ("" if tool != "sight_compare" else ":" + args_["action"])] = {"s": round(took, 3), "error": (j or {}).get("error") if err else None, "bytes": size}
            print(f"  {tool}: {took * 1000:.0f} ms{' ERROR ' + str((j or {}).get('error')) + ' ' + str((j or {}).get('message', ''))[:200] if err else ''}")
        # 4. Live probes (review of 998af3d).
        lead.json("time_control", action="step", ticks=600)
        j, err, took, _ = call_raw(lead, "desync_trace", {})
        check(not err and (j or {}).get("file"), f"desync_trace of 600 ticks answers from its worker ({took * 1000:.0f} ms, {(j or {}).get('bytes')} bytes)")
        for _ in range(300):
            l = lead.json("lease_take", kind="scratch", lease_s=600)
            lead.json("lease_release", id=l["id"], delete=True)
        big = lead.json("lease_take", kind="scratch", lease_s=600)
        bd = Path(big["dir"])
        for i in range(a.files):
            sub = bd / f"d{i // 1000}"
            if i % 1000 == 0:
                sub.mkdir(parents=True, exist_ok=True)
            (sub / f"f{i}.bin").write_bytes(b"x")
        j, err, took, _ = call_raw(lead, "lease_release", {"id": big["id"], "delete": True})
        check(not err and took < 1.0, f"a 20,000-file delete after 300 releases answers at once ({took * 1000:.0f} ms)")
        j, err, took, _ = call_raw(lead, "world_save", {"name": "budget"})
        check(not err and took < 5.0, f"a world_save right after answers ({took * 1000:.0f} ms)")
        pool = lead.json("engine_info", response_format="detailed")["workers"]["pool"]
        check(pool.get("inline") == 0, f"no worker job ran inline ({pool})")
        stop = time.time() + 12
        codes, lock = {}, threading.Lock()

        def flood(i):
            m = eng.mcp(f"flood{i}")
            m.json("agent_hello", name=f"flood_{i}")
            while time.time() < stop:
                r = m.request("tools/call", {"name": "world_hash", "arguments": {"verify": True}})
                code = "ok" if not r.get("isError") else json.loads(r["content"][0]["text"]).get("error")
                with lock:
                    codes[code] = codes.get(code, 0) + 1
            m.close()

        ths = [threading.Thread(target=flood, args=(i,)) for i in range(20)]
        for t in ths:
            t.start()
        time.sleep(1.0)
        spec = {"name": "verify_under_load", "ticks": 120, "steps": [{"at": t, "call": "world_hash", "args": {"verify": True}} for t in range(5, 120, 10)]}
        lead.json("playtest_run", spec=spec, wait_s=0)
        rep_ = {}
        for _ in range(120):
            rep_ = lead.json("playtest_report", wait_s=5)
            if rep_.get("status") != "running":
                break
        for t in ths:
            t.join()
        busy = codes.get("job_busy", 0)
        check(rep_.get("status") == "passed", f"a playtest's verify steps pass while 20 agents loop verify ({rep_.get('status')})")
        check(busy * 20 <= sum(codes.values()), f"the looping agents are served ({codes})")
        summary["flood"] = codes

        time.sleep(2)
        found = stalls(lead)
        summary["stalls"] = found
        known, new = [], []
        for s in found:
            (known if s.get("key") in KNOWN else new).append(s)
        for s in new:
            ev = lead.json("trace_incident", id=s["id"]).get("evidence", {})
            print(f"  STALL NEW   x{s.get('count')}: {s.get('summary')} (zone {ev.get('zone')}, args {str(ev.get('args'))[:120]})")
        for s in known:
            print(f"  STALL KNOWN x{s.get('count')}: {s.get('summary')} (follow-up lane)")
        summary["stalls_known"] = [s.get("key") for s in known]
        summary["stalls_new"] = [s.get("key") for s in new]
        check(not new and (a.known_ok or not known), f"no engine-thread stall past its budget ({len(new)} new, {len(known)} known)")
        lead.close()
    summary["exit_code"] = eng.exit_code
    # A strict run with stalls exits non-zero: with --known-ok, only when they include new ones.
    clean = eng.exit_code == 0 or (a.known_ok and not summary.get("stalls_new"))
    why = "" if eng.exit_code == 0 else ", from known stalls only" if clean else ", new stalls"
    check(clean, f"the strict run exits as expected (exit code {eng.exit_code}{why})")
    (out / "summary.json").write_text(json.dumps(summary, indent=2))
    print(f"summary: {out / 'summary.json'}")
    print("PASS" if not FAILS else f"FAIL ({len(FAILS)})")
    return 0 if not FAILS else 1


if __name__ == "__main__":
    sys.exit(main())
