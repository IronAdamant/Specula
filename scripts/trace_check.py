"""Live check of built-in tracing (docs/design/TRACE.md) and the debugger role
(docs/design/DEBUGGER.md), in Sepulcra:

1. Two builder agents and a lead do mixed work in a running Sepulcra (spawns, sets, claims, captures,
   game tools); each builder reconstructs its own calls' effects with trace_call (the tool, the
   journal entries, the spans), and world_history shows the call on each entry.
2. A scripted debugger agent runs beside them. It detects an induced stall (a 2048x2048 sight_frame:
   engine.stall) and a refused write (a builder writing into reserved data: write.refused), acks
   each, narrows it with trace_call, files a critique with the evidence and the fix owner, links it,
   and resolves it after a fix lands (the lead raises the render budget; the builder redoes the write
   through the right place), checking that neither recurs.
3. A deliberate panic in a test module (games/ambulo, edited in place and always restored) disables
   it and writes a crash report; after an engine restart trace_crash reads it.
4. Determinism: the same seeded run with tracing at full verbosity and off gives the same world_hash
   (and the journal replays to it).

    cargo build -p sepulcra-app && python scripts/trace_check.py [--port 7750] [--window] [--adapter integrated] [--cost]

--cost also measures Sepulcra's window frame p95 (GPU and CPU) with tracing at info and off (needs
--window; use --adapter integrated for the reference device). Results go to <target>/trace-check/
(summary.json, captures, engine logs, the trace folder). ASCII output only.
"""
import argparse
import json
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

AMBULO = ROOT / "games" / "ambulo" / "src" / "lib.rs"
PANIC_MARK = "// trace_check: deliberate panic"
FAILS = []


def check(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        FAILS.append(what)
    return cond


def call_raw(mcp, tool, **args):
    """A tool call whose errors are answers: (json, is_error)."""
    result = mcp.request("tools/call", {"name": tool, "arguments": args})
    text = next((c["text"] for c in result["content"] if c["type"] == "text"), "{}")
    try:
        return json.loads(text), result.get("isError", False)
    except json.JSONDecodeError:
        return {"raw": text}, True


def wait_for(what, fn, timeout=20.0, every=0.25):
    end = time.time() + timeout
    while time.time() < end:
        v = fn()
        if v:
            return v
        time.sleep(every)
    return None


class Debugger:
    """The scripted debugger agent (DEBUGGER.md's loop, bounded per incident)."""

    def __init__(self, mcp):
        self.mcp = mcp
        self.calls = 0
        self.handled = {}

    def tool(self, name, **args):
        self.calls += 1
        return self.mcp.json(name, **args)

    def active(self, code):
        out = self.tool("trace_incidents", code=code)
        return out.get("incidents", [])

    def triage(self, incident, critique_text, metric=None, path=None, owner=None):
        budget_start = self.calls
        iid = incident["id"]
        self.tool("trace_incident", id=iid, action="ack", note="debugger: looking")
        full = self.tool("trace_incident", id=iid, response_format="detailed")
        calls = full.get("calls") or []
        tree = self.tool("trace_call", call=calls[-1]) if calls else {}
        evidence = full.get("evidence") or {}
        owner = owner or (full.get("agents") or [None])[-1]
        item = {
            "text": f"[incident {iid} {incident['code']}] {critique_text} Evidence: {json.dumps(evidence)[:600]}."
                    f" Calls: {calls}. Fix owner: {owner or 'unknown'}.",
            "severity": "error" if incident.get("level") == "error" else "warning",
            "source": "debugger",
        }
        if metric:
            item["metric"] = metric
        if path:
            item["path"] = path
        added = self.tool("critique_add", items=[item])
        ids = added.get("added") or []
        critique = ids[0] if ids else None
        if critique is not None:
            self.tool("trace_incident", id=iid, action="link", critique=critique, note="filed")
        self.handled[iid] = {"critique": critique, "owner": owner, "calls_used": self.calls - budget_start, "tree": tree}
        return critique, full, tree

    def resolve(self, iid, critique, note):
        self.tool("trace_incident", id=iid, action="resolve", note=note)
        if critique is not None:
            self.tool("critique_update", id=critique, action="resolve", note=note)


def inject_panic():
    """Ambulo's source as it is on disk, and with a panic in its tick. Both are bytes: text
    written by Python on Windows ends its lines CRLF, so a source read and put back as text
    came back changed in a checkout whose files are LF (and the check that compared the two
    as text passed)."""
    original = AMBULO.read_bytes()
    text = original.decode("utf-8")
    anchor = "    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {\n"
    if anchor not in text:
        anchor = anchor.replace("\n", "\r\n")
    if anchor not in text:
        raise SystemExit("trace_check: ambulo's tick not found (its source changed?)")
    nl = "\r\n" if "\r\n" in anchor else "\n"
    panicking = text.replace(anchor, anchor + f"        {PANIC_MARK}{nl}        if cx.tick() > 0 {{ panic!(\"trace_check: a deliberate module panic\"); }}{nl}", 1)
    return original, panicking.encode("utf-8")


def engine_args(a, out, sub, extra=()):
    args = ["--captures", str(out / sub / "captures"), "--trace", str(out / "trace"), "--worlds", str(out / sub / "worlds"),
            "--saves", str(out / sub / "saves")]
    if not a.window:
        args.append("--headless")
    if a.adapter:
        args += ["--adapter", a.adapter]
    return args + list(extra)


def part_swarm(a, out, summary):
    print("swarm, correlation and the debugger:")
    with Engine(a.exe, a.port, out / "engine-swarm.log", *engine_args(a, out, "swarm", ["--lead", "trace-lead", "--reviewer", "debugger", "--", "--seed", "7", "--title", "off"]), cwd=str(ROOT)) as eng:
        lead, ann, bob, dbg = (eng.mcp(n) for n in ("trace-lead", "trace-ann", "trace-bob", "trace-debugger"))
        lead.hello("trace-lead", role="lead")  # granted: authority comes from the operator (docs/design/AUTHORITY.md)
        ann.hello("ann")
        bob.hello("bob")
        dbg.hello("debugger", role="reviewer")  # the debugger job reviews incidents: granted --reviewer debugger
        debugger = Debugger(dbg)
        # Mixed work.
        for group in ("/trace_check", "/trace_check/ann", "/trace_check/bob"):
            lead.json("world_spawn", shape="group", path=group)
        ann.json("world_spawn", shape="cube", path="/trace_check/ann/crate", position=[0, 0.5, 0])
        bob.json("world_spawn", shape="sphere", path="/trace_check/bob/orb", position=[2, 0.5, 0])
        bob.json("claim_take", path="/trace_check/bob")
        ann.json("world_set", path="/trace_check/ann/crate", position=[0, 1, 0])
        tree = ann.json("trace_call")
        check(tree.get("call", {}).get("tool") == "world_set" and len(tree.get("journal", [])) == 1,
              f"ann's trace_call shows her world_set and its journal entry ({tree.get('call', {}).get('tool')}, journal {tree.get('journal')})")
        hist = ann.json("world_history", path_prefix="/trace_check/ann/crate", response_format="detailed")
        entries = hist.get("entries") or hist.get("items") or []
        check(any(e.get("call") for e in entries), "world_history shows the call on the entry")
        conflict, err = call_raw(ann, "world_set", path="/trace_check/bob/orb", position=[5, 5, 5])
        check(err and conflict.get("error") == "claimed", f"ann's write to bob's claimed orb is refused ({conflict.get('error')})")
        btree = bob.json("world_spawn", shape="cube", path="/trace_check/bob/wall", position=[3, 0.5, 0])
        btree = bob.json("trace_call")
        check(btree.get("call", {}).get("agent") == "bob", "bob reconstructs his own spawn")
        state, _ = call_raw(lead, "play_state")
        check(isinstance(state, dict), "the game answers play_state (a game tool, traced)")
        ann.frame(width=320, height=240)
        frame_tree = ann.json("trace_call", response_format="detailed")
        check(frame_tree.get("call", {}).get("tool") == "sight_frame", "a capture is one traced call")
        # The refused write: ann writes into reserved data.
        bad = None
        for path in ("/nav", "/project", "/lex", "/nav/trace_probe", "/project/trace_probe", "/lex/trace_probe"):
            r, err = call_raw(ann, "world_spawn", shape="group" if path.count("/") == 1 else "cube", path=path)
            if err and r.get("error") == "reserved":
                bad = path
                break
        check(bad is not None, f"a write into reserved data is refused (reserved at {bad})")
        # The induced stall: renders get a 1 ms budget, then a 2048x2048 capture. (Captures no longer stall
        # at the normal budget since the stalls lane moved their encode and readback off the engine thread,
        # so the check lowers the budget to provoke one; the lead restores it as the "fix" below.)
        lead.json("trace_settings", budgets_ms={"render": 1})
        big = ann.frame(width=2048, height=2048, max_subjects=1)
        summary["stall_capture"] = big.get("image", {}).get("path")
        stalls = wait_for("stall incident", lambda: [i for i in debugger.active("engine.stall") if i["key"] == "sight_frame"])
        check(bool(stalls), "the debugger sees engine.stall for sight_frame")
        refusals = wait_for("refusal incident", lambda: [i for i in debugger.active("write.refused") if i["key"].startswith("reserved")])
        check(bool(refusals), "the debugger sees write.refused for the reserved path")
        perf = lead.json("perf_frame")
        recent = perf.get("trace", {}).get("recent_stalls", [])
        check(any(s.get("tool") == "sight_frame" and s.get("args", "").find("2048") >= 0 for s in recent),
              "perf_frame lists the stall with its argument digest")
        follow = dbg.json("trace_follow", level="warn")
        check(any(e.get("code") == "engine.stall" for e in follow.get("events", [])), "trace_follow shows the stall event")
        summary["incidents"] = {}
        if stalls:
            inc = stalls[0]
            critique, full, tree = debugger.triage(
                inc, "sight_frame at 2048x2048 held the engine thread past the render budget.",
                metric={"name": "ms", "value": full_ms(inc), "limit": 30}, owner=None)
            check(critique is not None, f"the stall is filed as critique {critique}")
            check(tree.get("call", {}).get("tool") == "sight_frame", "trace_call on the stall's call names the capture")
            check(debugger.handled[inc["id"]]["owner"] == "ann", "the fix owner is the agent whose call stalled (ann)")
            # The fix: the lead gives renders a bigger budget; the capture is repeated and doesn't stall.
            lead.json("trace_settings", budgets_ms={"render": 2000})
            before = dbg.json("trace_incident", id=inc["id"])["count"]
            ann.frame(width=2048, height=2048, max_subjects=1)
            after = dbg.json("trace_incident", id=inc["id"])
            check(after["count"] == before, "after the fix the capture no longer stalls")
            debugger.resolve(inc["id"], critique, "render budget raised by the lead; re-ran the 2048 capture: no stall")
            summary["incidents"]["stall"] = {"id": inc["id"], "critique": critique, "evidence": full.get("evidence")}
        if refusals:
            inc = refusals[0]
            critique, full, tree = debugger.triage(inc, "a world write into reserved data was refused.", path=None)
            ev = full.get("evidence", {})
            check(ev.get("path") == bad and ev.get("rule") == "world", f"the refusal is explained: rule {ev.get('rule')}, path {ev.get('path')}")
            check(any(n.get("code") == "write.refused" for n in walk(tree.get("tree", []))), "trace_call on the refused call shows the policy's decision")
            check(debugger.handled[inc["id"]]["owner"] == "ann", "the fix owner is ann")
            ann.json("world_spawn", shape="cube", path="/trace_check/ann/probe")
            time.sleep(1.5)
            again = dbg.json("trace_incident", id=inc["id"])
            check(again["count"] == full["count"], "the redone write is not refused")
            debugger.resolve(inc["id"], critique, "ann spawned under her own subtree; no refusal since")
            summary["incidents"]["refusal"] = {"id": inc["id"], "critique": critique, "evidence": ev}
        for name, inc in summary["incidents"].items():
            final = dbg.json("trace_incident", id=inc["id"])
            check(final["status"] == "resolved", f"the {name} incident is resolved (not recurring)")
        crit = lead.json("critique_list", source="debugger")
        summary["critiques"] = crit.get("counts") or crit.get("totals")
        summary["debugger_calls"] = {k: v["calls_used"] for k, v in debugger.handled.items()}
        check(all(v["calls_used"] <= 20 for v in debugger.handled.values()), f"each incident took the debugger at most 20 calls ({summary['debugger_calls']})")
        q = dbg.json("trace_query", agent="ann", code="write.")
        check(q.get("matched", 0) >= 1, "trace_query by agent finds ann's write decisions")
    check(eng.exit_code == 0, f"the engine shut down cleanly (exit {eng.exit_code})")


def part_security(a, out, summary):
    """Review probe (b893d0a, blocker): another agent read a resume token out of trace_call's
    argument digest and took over its owner's identity."""
    print("no agent reads another's secrets through the trace:")
    with Engine(a.exe, a.port + 6, out / "engine-security.log", *engine_args(a, out, "security", ["--", "--seed", "7", "--title", "off"]), cwd=str(ROOT)) as eng:
        ann = eng.mcp("sec-ann")
        token = ann.json("agent_hello", name="ann", role="builder").get("resume_token")
        ann2 = eng.mcp("sec-ann-2")
        ann.close()
        ann2.json("agent_hello", name="ann", resume_token=token)
        ann2.json("world_spawn", shape="cube", path="/ann_box")
        ann2.json("claim_take", path="/ann_box")
        # A slow call of ann's, so a stall carries its digest too.
        ann2.frame(width=2048, height=2048, max_subjects=1)
        mal = eng.mcp("sec-mallory")
        mal.json("agent_hello", name="mallory")
        seen, others = [], []
        for c in range(1, 80):
            t, err = call_raw(mal, "trace_call", call=c, response_format="detailed")
            if not err:
                seen.append(json.dumps(t))
                if not str(t.get("call", {}).get("agent", "")).startswith(("mallory", "sec-mallory")):
                    others.append(json.dumps(t))
        q, _ = call_raw(mal, "trace_query", limit=200, response_format="detailed")
        seen.append(json.dumps(q))
        inc, _ = call_raw(mal, "trace_incidents", status="all", response_format="detailed")
        seen.append(json.dumps(inc))
        text = "".join(seen)
        check(token and token not in text, "mallory finds ann's resume token in no trace answer")
        # Digests are public metadata: keys, types, sizes and numbers, never a string value.
        digests = [json.loads(o).get("call", {}).get("args", "") for o in others]
        texty = [d for d in digests if '"' in d]
        check(others and digests and not texty, f"the digests another agent sees hold no text ({len(digests)} probed{'; ' + texty[0][:200] if texty else ''})")
        own = ann2.json("trace_call")
        check("args" in own.get("call", {}), "ann sees her own call's digest")
        files = list((out / "trace").glob("*.jsonl")) + list((out / "trace").glob("crash-*.json"))
        check(all(token not in f.read_text(errors="replace", encoding="utf-8") for f in files), "no trace file holds the token")
        check(token not in eng.log_path.read_text(errors="replace", encoding="utf-8"), "engine.log doesn't hold the token")
    summary["security"] = {"calls_probed": 79}


def part_journal(a, out, summary):
    """Review probe (e45f66c, major 2): with 16 agents writing at once, calls commit out of id
    order, and trace_call showed an empty journal for 57 of 2,040 calls."""
    import threading
    print("every write's call names its journal entry, 16 agents at once:")
    with Engine(a.exe, a.port + 6, out / "engine-journal.log", *engine_args(a, out, "journal", ["--lead", "journal-lead"]), cwd=str(ROOT)) as eng:
        lead = eng.mcp("journal-lead")
        lead.json("agent_hello", name="journal-lead", role="lead")
        lead.json("world_spawn", path="/j", shape="group")
        lead.json("world_batch", ops=[{"op": "spawn", "path": f"/j/e{k}", "shape": "cube"} for k in range(16)])
        missing, total = [], [0]
        lock = threading.Lock()

        def agent(k):
            m = eng.mcp(f"journal-{k}")
            m.json("agent_hello", name=f"journal-{k}")
            for i in range(100):
                m.json("world_set", path=f"/j/e{k}", position=[i % 7, k, 1])
                t = m.json("trace_call")
                with lock:
                    total[0] += 1
                    if t.get("call", {}).get("tool") != "world_set" or len(t.get("journal", [])) != 1:
                        missing.append(t.get("call", {}).get("call"))

        threads = [threading.Thread(target=agent, args=(k,)) for k in range(16)]
        [t.start() for t in threads]
        [t.join() for t in threads]
        check(total[0] == 1600 and not missing, f"all {total[0]} writes show their journal entry in trace_call (missing: {len(missing)})")
    summary["journal"] = {"writes": total[0], "missing": len(missing)}


def part_replay(a, out, summary):
    """Review probe (b893d0a, major 3): the idle replay of a 12k-entity journal held the engine
    thread 148 ms; a call arriving 2 s into idle waited 99 ms."""
    print("the idle replay spot-check runs in steps:")
    with Engine(a.exe, a.port + 7, out / "engine-replay.log", *engine_args(a, out, "replay"), cwd=str(ROOT)) as eng:
        m = eng.mcp("replay-probe")
        m.json("agent_hello", name="replay-builder")
        for b in range(12):
            m.json("world_spawn", path=f"/b{b}", shape="group")
            ops = [{"op": "spawn", "path": f"/b{b}/e{i}", "shape": "cube", "position": [i % 100, b, i // 100]} for i in range(1000)]
            m.json("world_batch", ops=ops)
        for i in range(2000):
            m.json("world_set", path=f"/b0/e{i % 1000}", position=[i % 7, 1, 2])
        lat = []
        for _ in range(6):
            time.sleep(2.05)
            t = time.perf_counter()
            m.json("world_hash")
            lat.append(round((time.perf_counter() - t) * 1000, 1))
        rc = wait_for("replay done", lambda: (m.json("perf_frame").get("trace") or {}).get("replay_check", {}).get("last_total_ms") is not None, timeout=60, every=1.0)
        pf = (m.json("perf_frame").get("trace") or {}).get("replay_check", {})
        check(bool(rc), f"the replay finished ({pf})")
        check(max(lat) < 40, f"a call arriving during the replay isn't held up (latencies {lat} ms)")
        longest = pf.get("last_longest_step_ms") or 0
        check(longest < 10, f"its longest step stays near its 2 ms budget ({longest} ms of {pf.get('last_total_ms')} ms in all)")
        mism = m.json("trace_incidents", status="all", code="world.replay_mismatch")
        check(mism.get("matched") == 0, "and it matches the live world")
    summary["replay"] = {"latencies_ms": lat, "replay_check": pf}


def full_ms(inc):
    try:
        return float(inc["summary"].split(" held the engine thread ")[1].split(" ms")[0])
    except (IndexError, ValueError, KeyError):
        return 0.0


def walk(nodes):
    for n in nodes:
        yield n
        yield from walk(n.get("children", []))


def part_crash(a, out, summary):
    print("a module panic leaves a crash report readable after a restart:")
    original, panicking = inject_panic()
    try:
        AMBULO.write_bytes(panicking)
        with Engine(a.exe, a.port + 1, out / "engine-crash.log", *engine_args(a, out, "crash", ["--paused", "--lead", "crash-lead", "--", "--seed", "7", "--title", "off"]), cwd=str(ROOT)) as eng:
            m = eng.mcp("trace-crash")
            m.hello("crash-lead", role="lead")
            built, err = call_raw(m, "logic_build", path="games/ambulo")
            check(not err, f"the panicking module builds and loads ({built.get('error') or 'ok'})")
            m.json("time_control", action="step", ticks=3)
            inc = wait_for("module.disabled", lambda: m.json("trace_incidents", code="module.disabled").get("incidents"))
            check(bool(inc), "module.disabled is an incident")
    finally:
        AMBULO.write_bytes(original)
    with Engine(a.exe, a.port + 2, out / "engine-restart.log", *engine_args(a, out, "restart", ["--paused", "--", "--seed", "7", "--title", "off"]), cwd=str(ROOT)) as eng:
        m = eng.mcp("trace-restart")
        report = m.json("trace_crash")
        check(report.get("reason") == "module_panic" and "deliberate" in report.get("message", ""),
              f"after a restart trace_crash reads the module panic ({report.get('reason')}: {report.get('message', '')[:80]})")
        check(any(e.get("code") == "module.disabled" or e.get("sub", "").startswith("module:") or e.get("sub") == "logic" for e in report.get("events", [])) or report.get("events_kept", 0) > 0,
              f"the report keeps the last events ({report.get('events_kept')})")
        summary["crash_report"] = {"reason": report.get("reason"), "message": report.get("message"), "events_kept": report.get("events_kept")}
    check(AMBULO.read_bytes() == original, "ambulo's source is restored, byte for byte")


def run_seeded(a, out, sub, port, level):
    with Engine(a.exe, port, out / f"engine-{sub}.log", *engine_args(a, out, sub, ["--paused", "--lead", "det-lead", "--", "--seed", "7", "--title", "off"]), cwd=str(ROOT)) as eng:
        m = eng.mcp("trace-det")
        m.hello("det-lead", role="lead")
        m.json("trace_settings", level=level, subsystems={"mcp": level, "write_policy": level, "jobs": level})
        m.json("play_new", seed=7)
        m.json("time_control", action="step", ticks=240)
        h = m.json("world_hash", recent=4, verify=True)
        n = m.json("trace_query", limit=1).get("ring", {}).get("next_id")
    return h, n


def part_determinism(a, out, summary):
    print("determinism with tracing at full verbosity and off:")
    full, n_full = run_seeded(a, out, "det-trace", a.port + 3, "trace")
    off, n_off = run_seeded(a, out, "det-off", a.port + 4, "off")
    check(full["state_hash"] == off["state_hash"] and full["tick"] == off["tick"],
          f"same world_hash at tick {full['tick']}: {full['state_hash']} vs {off['state_hash']}")
    check(full.get("verify", {}).get("matches") and off.get("verify", {}).get("matches"), "both journals replay to their live world")
    check((n_full or 0) > (n_off or 0), f"full verbosity recorded more events ({n_full} vs {n_off})")
    summary["determinism"] = {"tick": full["tick"], "trace": full["state_hash"], "off": off["state_hash"], "events_trace": n_full, "events_off": n_off}


def compilers():
    """rustc processes running now (other lanes' builds share the machine)."""
    if sys.platform != "win32":
        return None
    import subprocess
    try:
        listing = subprocess.run(["tasklist", "/FI", "IMAGENAME eq rustc.exe", "/FO", "CSV", "/NH"], capture_output=True, text=True, timeout=10, encoding="utf-8", errors="replace").stdout
    except (OSError, subprocess.SubprocessError):
        return None
    return sum(1 for line in listing.splitlines() if line.startswith('"rustc.exe"'))


def part_cost(a, out, summary):
    print("cost: window frame p95 with tracing at info and off:")
    if not a.window:
        print("  skip (needs --window)")
        return
    res = {}
    with Engine(a.exe, a.port + 5, out / "engine-cost.log", *engine_args(a, out, "cost", ["--lead", "cost-lead", "--", "--seed", "7", "--title", "off"]), cwd=str(ROOT)) as eng:
        m = eng.mcp("trace-cost")
        m.hello("cost-lead", role="lead")
        m.json("play_new", seed=7)
        time.sleep(5)
        for level in ("info", "off", "info", "off", "info", "off"):
            m.json("trace_settings", level=level)
            time.sleep(1.0)
            cap = m.json("perf_capture", frames=240, trace=False, label=f"trace_cost_{level}")
            row = {"frame_ms": cap.get("frame_ms"), "gpu_ms": cap.get("gpu_ms"), "compilers_running": compilers()}
            res.setdefault(level, []).append(row)
            p95 = lambda k: (row.get(k) or {}).get("p95")
            print(f"  {level}: frame p95 {p95('frame_ms')} ms, GPU p95 {p95('gpu_ms')} ms, compilers running {row['compilers_running']}")
    summary["cost"] = res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7750, help="uses this port and the next seven")
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target")
    ap.add_argument("--exe", default=str(target / "debug" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")))
    ap.add_argument("--window", action="store_true")
    ap.add_argument("--adapter", default=None)
    ap.add_argument("--cost", action="store_true")
    ap.add_argument("--only", default=None, help="swarm, security, journal, replay, crash, determinism or cost")
    ap.add_argument("--out", default=str(target / "trace-check"))
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    # --only adds to an earlier run's summary.
    old = out / "summary.json"
    summary = json.loads(old.read_text(encoding="utf-8")) if a.only and old.exists() else {}
    summary.pop("failures", None)
    parts = [("swarm", part_swarm), ("security", part_security), ("journal", part_journal), ("replay", part_replay), ("crash", part_crash), ("determinism", part_determinism)] + ([("cost", part_cost)] if a.cost else [])
    for name, part in parts:
        if a.only and a.only != name:
            continue
        try:
            part(a, out, summary)
        except Exception as e:  # noqa: BLE001 (a part's failure is reported, the others still run)
            check(False, f"{name}: {type(e).__name__}: {e}")
    summary["failures"] = FAILS
    (out / "summary.json").write_text(json.dumps(summary, indent=2, default=str), encoding="utf-8")
    print(f"summary: {out / 'summary.json'}")
    if FAILS:
        raise SystemExit(f"trace_check: {len(FAILS)} check(s) failed")
    print("trace_check: all checks passed")


if __name__ == "__main__":
    main()
