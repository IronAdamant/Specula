"""Swarm drill: many agents build one world at once through MCP, and the safety layer must hold.

N agents (threads, each its own MCP session) work on one headless engine at the same time. Each:
  1. says hello and claims its own district (/district_NN),
  2. builds it with world_batch rounds (spawns, then moves some of what it built),
  3. deliberately writes into a neighbour's claimed district every few rounds (a spawn, a batch
     and a set): every one of those must be rejected with error 'claimed',
  4. spawns scratch entities and undoes them with world_undo (single changes and a whole batch),
  5. polls world_changes to follow everyone else's work.
Then it overloads the server: it holds the protocol's 256 connections open and checks that extra
requests get a proper HTTP 503 (with Retry-After), not a reset, and that service resumes after.
At the end the drill checks that every district holds exactly what its agent built (nothing more,
nothing missing, nothing by anyone else), that the journal replays to the live state hash
(world_hash verify), and that claims are listed and released cleanly.

Reports throughput (world ops/s), per-tool latency p50/p95, conflict and rejection counts, and saves
JSON (schema specula.drill/v1) to perf/drills/.

Usage: cargo build -p specula --release && python scripts/swarm_drill.py [--agents 16] [--rounds 12]
       [--batch 20] [--port 7488] [--debug] [--label NAME]
Needs a GPU (the engine starts its renderer even headless). Output is ASCII.
"""
import argparse
import datetime
import json
import shutil
import socket
import statistics
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path

from specula_mcp import ROOT, Engine, url_for

PORT = 7488


class Agent:
    """One MCP session with timing per tool."""

    def __init__(self, url, name, client="swarm-drill"):
        self.url, self.name, self.session, self.next_id = url, name, None, 1
        self.timings = {}
        self.rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                "clientInfo": {"name": client, "version": "0"}})
        self.post({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def post(self, msg):
        headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        req = urllib.request.Request(self.url, json.dumps(msg).encode(), headers)
        with urllib.request.urlopen(req, timeout=120) as r:
            self.session = self.session or r.headers.get("Mcp-Session-Id")
            body = r.read()
        return json.loads(body) if body else None

    def rpc(self, method, params):
        reply = self.post({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        self.next_id += 1
        if "error" in reply:
            raise RuntimeError(f"{method}: {reply['error']}")
        return reply["result"]

    def call(self, tool, **args):
        """(ok, payload): payload is the parsed JSON text (or the raw text)."""
        start = time.perf_counter()
        result = self.rpc("tools/call", {"name": tool, "arguments": args})
        self.timings.setdefault(tool, []).append(time.perf_counter() - start)
        text = next((c["text"] for c in result["content"] if c["type"] == "text"), "")
        try:
            payload = json.loads(text)
        except ValueError:
            payload = text
        return not result.get("isError"), payload

    def end(self):
        """Ends the session (HTTP DELETE), as a client that disconnects cleanly does."""
        req = urllib.request.Request(self.url, method="DELETE", headers={"Mcp-Session-Id": self.session})
        with urllib.request.urlopen(req, timeout=30) as r:
            r.read()

    def ok(self, tool, **args):
        ok, payload = self.call(tool, **args)
        if not ok:
            raise RuntimeError(f"{self.name} {tool}: {payload}")
        return payload


def district(i):
    return f"/district_{i:02d}"


class Worker(threading.Thread):
    def __init__(self, url, index, count, rounds, batch, feed, ready):
        super().__init__(name=f"agent-{index}")
        self.url, self.index, self.count, self.rounds, self.batch = url, index, count, rounds, batch
        self.feed = feed
        self.ready = ready  # every district is claimed and exists before anyone intrudes
        self.name_ = f"agent_{index:02d}"
        self.expected = set()      # paths this agent built and kept
        self.ops = 0               # world ops applied (each batch op counts)
        self.rejected = 0          # intrusions rejected with 'claimed'
        self.leaked = []           # intrusions that were NOT rejected (failures)
        self.other_errors = []
        self.undone = 0
        self.feed_items = 0
        self.feed_gaps = 0
        self.agent = None
        self.error = None

    def run(self):
        try:
            self.work()
        except Exception as e:  # reported in the summary
            self.error = f"{type(e).__name__}: {e}"
            self.ready.abort()

    def intrude(self, tool, **args):
        ok, payload = self.agent.call(tool, **args)
        code = payload.get("error") if isinstance(payload, dict) else None
        if ok:
            self.leaked.append({"tool": tool, "args": args})
        elif code == "claimed":
            self.rejected += 1
        else:
            self.other_errors.append({"tool": tool, "error": payload})

    def work(self):
        a = self.agent = Agent(self.url, self.name_)
        a.ok("agent_hello", name=self.name_, role="builder")
        home = district(self.index)
        claim = a.ok("claim_take", path=home, lease_s=600, note="swarm drill district")
        assert claim["owner"] == self.name_, claim
        x0 = (self.index % 8) * 40.0
        z0 = (self.index // 8) * 40.0
        a.ok("world_batch", ops=[{"op": "spawn", "shape": "group", "path": home, "position": [x0, 0, z0]}])
        self.ops += 1
        self.expected.add(home)
        self.ready.wait()
        cursor = "0"
        neighbour = district((self.index + 1) % self.count)
        for r in range(self.rounds):
            ops = []
            for k in range(self.batch):
                path = f"{home}/r{r:02d}_c{k:02d}"
                ops.append({"op": "spawn", "shape": "cube", "path": path,
                            "position": [k % 5 * 1.5, 0.5, k // 5 * 1.5 + r * 8.0],
                            "color": [0.2 + 0.6 * (self.index % 4) / 3, 0.5, 0.7], "tags": ["drill"]})
                self.expected.add(path)
            # Move a few things built in earlier rounds.
            if r > 0:
                for k in range(0, self.batch, 4):
                    ops.append({"op": "set", "path": f"{home}/r{r - 1:02d}_c{k:02d}", "position": [k % 5 * 1.5, 1.0, r * 8.0]})
            a.ok("world_batch", ops=ops)
            self.ops += len(ops)
            # Deliberate intrusions into the neighbour's claim.
            if self.count > 1 and r % 3 == 1:
                self.intrude("world_spawn", shape="cube", path=f"{neighbour}/intruder_{self.index:02d}_{r:02d}")
                self.intrude("world_batch", ops=[{"op": "spawn", "shape": "cube",
                                                   "path": f"{neighbour}/intruder_b{self.index:02d}_{r:02d}"}])
                self.intrude("world_set", path=neighbour, position=[0, 99, 0])
            # Scratch work, undone again.
            if r % 4 == 2:
                a.ok("world_spawn", shape="sphere", path=f"{home}/scratch_{r:02d}")
                undo = a.ok("world_undo")
                assert len(undo["undone"]) == 1, undo
                self.undone += 1
                self.ops += 2
            if r % 6 == 5:
                a.ok("world_batch", ops=[{"op": "spawn", "shape": "cube", "path": f"{home}/tmp_{r:02d}_{k}"} for k in range(5)])
                a.ok("world_undo")
                self.undone += 1
                self.ops += 6
            # Follow everyone's changes.
            while self.feed:
                feed = a.ok("world_changes", since=cursor, limit=500)
                self.feed_items += len(feed["changes"])
                self.feed_gaps += 1 if feed["gap"] else 0
                cursor = feed["next"]
                if not feed["more"]:
                    break
        a.ok("claim_renew", all=True, lease_s=600)


def stats_ms(samples):
    ms = sorted(s * 1000.0 for s in samples)
    if not ms:
        return None
    p95 = ms[min(len(ms) - 1, int(round(0.95 * (len(ms) - 1))))]
    return {"p50": round(statistics.median(ms), 2), "p95": round(p95, 2), "max": round(ms[-1], 2), "n": len(ms)}


def overload(port, extra=20):
    """Hold 256 idle connections; each extra request must be answered 503, then service resumes."""
    held = []
    try:
        for _ in range(256):
            held.append(socket.create_connection(("127.0.0.1", port), timeout=10))
        time.sleep(0.3)
        statuses = {}
        body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "ping"}).encode()
        for _ in range(extra):
            req = urllib.request.Request(url_for(port), body, {"Content-Type": "application/json"})
            try:
                with urllib.request.urlopen(req, timeout=10) as r:
                    key = str(r.status)
            except urllib.error.HTTPError as e:
                key = f"{e.code}" + ("+retry-after" if e.headers.get("Retry-After") else "")
            except OSError as e:
                key = f"reset:{type(e).__name__}"
            statuses[key] = statuses.get(key, 0) + 1
    finally:
        for h in held:
            h.close()
    resumed = False
    deadline = time.perf_counter() + 10
    while time.perf_counter() < deadline and not resumed:
        try:
            Agent(url_for(port), "after-overload")
            resumed = True
        except (OSError, RuntimeError):
            time.sleep(0.1)
    return {"held": len(held), "extra_requests": extra, "statuses": statuses, "service_resumed": resumed}


def cpu_busy_pct(seconds=1.0):
    """Whole-machine CPU use over `seconds` (Windows; as perf_baseline.py), so noisy runs are flagged."""
    if sys.platform != "win32":
        return None
    import ctypes
    k32 = ctypes.WinDLL("kernel32")

    def times():
        idle, kernel, user = (ctypes.c_uint64(), ctypes.c_uint64(), ctypes.c_uint64())
        k32.GetSystemTimes(ctypes.byref(idle), ctypes.byref(kernel), ctypes.byref(user))
        return idle.value, kernel.value + user.value  # kernel time includes idle time

    i0, t0 = times()
    time.sleep(seconds)
    i1, t1 = times()
    total = t1 - t0
    return round(100.0 * (1 - (i1 - i0) / total), 1) if total else None


def attacks(url):
    """Every claim bypass found in review, tried live: each must be refused with the expected code.

    Returns (rejected, leaked, wrong): leaked = accepted when it must not be; wrong = refused with
    another error than expected.
    """
    ann, bob, carl = Agent(url, "attack-ann"), Agent(url, "attack-bob"), Agent(url, "attack-carl")
    ann.ok("agent_hello", name="attack_ann")
    bob.ok("agent_hello", name="attack_bob")
    carl.ok("agent_hello", name="attack_carl")
    results = {"rejected": 0, "leaked": [], "wrong": []}

    def expect(agent, code, label, tool, **args):
        ok, payload = agent.call(tool, **args)
        got = payload.get("error") if isinstance(payload, dict) else None
        if ok:
            results["leaked"].append(label)
        elif got != code:
            results["wrong"].append({"case": label, "expected": code, "got": payload})
        else:
            results["rejected"] += 1

    spawn = lambda path, **kw: {"op": "spawn", "shape": "group", "path": path, **kw}
    # 1. Renaming / moving an ancestor carries a claimed subtree out of its claim.
    bob.ok("world_batch", ops=[spawn("/siege"), spawn("/siege/keep"), spawn("/siege/keep/wall"), spawn("/bobyard")])
    ann.ok("claim_take", path="/siege/keep", lease_s=300)
    expect(bob, "claimed", "rename ancestor out", "world_set", path="/siege", rename="siege_moved")
    expect(bob, "claimed", "reparent ancestor out", "world_set", path="/siege", parent="/bobyard")
    expect(bob, "claimed", "rename ancestor in a batch", "world_batch", ops=[{"op": "set", "path": "/siege", "rename": "x"}])
    expect(bob, "claimed", "delete ancestor", "world_delete", path="/siege")
    # 1b. ...or carries foreign entities into a claim.
    ann.ok("claim_take", path="/camp/keep", lease_s=300)
    bob.ok("world_batch", ops=[spawn("/bobs"), spawn("/bobs/keep"), spawn("/bobs/keep/x")])
    expect(bob, "claimed", "rename ancestor into a claim", "world_set", path="/bobs", rename="camp")
    # 1c. ...and through undo, which puts an old state back.
    bob.ok("world_batch", ops=[spawn("/lane"), spawn("/lane/c")])
    bob.ok("world_set", path="/lane", rename="road")
    ann.ok("claim_take", path="/lane/c", lease_s=300)
    expect(bob, "claimed", "undo a rename into a claim", "world_undo", force=True)
    # 2. Identity: names are unique; another session adopts nothing.
    expect(carl, "name_taken", "hello as a live agent's name", "agent_hello", name="attack_ann")
    claim_id = ann.ok("claim_list", mine=True)["claims"][0]["id"]
    expect(carl, "not_owner", "release someone's claim", "claim_release", id=claim_id)
    expect(carl, "not_owner", "renew someone's claim", "claim_renew", id=claim_id)
    expect(carl, "claimed", "write inside someone's claim", "world_spawn", shape="cube", path="/siege/keep/graffiti")
    dave = Agent(url, "attack-dave")
    dave.ok("agent_hello", name="attack_dave")
    dave.ok("claim_take", path="/dave_zone", lease_s=300)
    entry = dave.ok("world_spawn", shape="group", path="/dave_zone")["journal"]
    dave.end()
    heir = Agent(url, "attack-heir")
    heir.ok("agent_hello", name="attack_dave")  # the name is free again...
    expect(heir, "nothing_to_undo", "undo a former session's work", "world_undo")
    expect(heir, "not_yours", "undo a former session's entry by id", "world_undo", ids=[entry])
    anon = Agent(url, "anon-1", client="claude-code")
    other = Agent(url, "anon-2", client="claude-code")
    names = [a["name"] for a in anon.ok("engine_info")["agents"] if a["client"] == "claude-code"]
    if len(names) != len(set(names)) or len(names) < 2:
        results["wrong"].append({"case": "unnamed sessions get distinct names", "got": names})
    else:
        results["rejected"] += 1
    del other
    # 3. world_load would replace claimed work.
    carl.ok("world_save", name="drill_attack")
    expect(bob, "claimed", "world_load over others' claims", "world_load", name="drill_attack", force=True)
    # 5. A path claim and a region claim that meet later.
    ann.ok("claim_take", path="/pr", lease_s=300)
    bob.ok("claim_take", region={"min": [5000, -10, -10], "max": [5010, 10, 10]}, lease_s=300)
    ann.ok("world_spawn", shape="group", path="/pr")
    expect(ann, "claims_overlap", "path claim meets region claim", "world_spawn", shape="cube", path="/pr/x",
           position=[5005, 0, 0])
    for a in (ann, bob, carl, heir):
        a.ok("claim_release", all=True)
    return results


def verify(url, workers):
    """Every district holds exactly what its agent built; the journal replays to the live hash."""
    v = Agent(url, "verifier")
    v.ok("agent_hello", name="drill_verifier", role="reviewer")
    problems = []
    for w in workers:
        home = district(w.index)
        found, cursor = {}, None
        while True:
            args = {"path_prefix": home, "limit": 500, "fields": ["path", "created_by", "modified_by"]}
            if cursor:
                args["cursor"] = cursor
            ok, page = v.call("world_query", **args)
            if not ok:
                problems.append(f"{home}: query failed: {page}")
                break
            for item in page["items"]:
                found[item["path"]] = item
            cursor = page["next_cursor"]
            if not cursor:
                break
        missing = sorted(w.expected - set(found))
        extra = sorted(set(found) - w.expected)
        foreign = sorted(p for p, it in found.items() if it["created_by"] != w.name_ or it["modified_by"] != w.name_)
        if missing or extra or foreign:
            problems.append(f"{home}: missing {missing[:3]} ({len(missing)}), extra {extra[:3]} ({len(extra)}), "
                            f"touched by others {foreign[:3]} ({len(foreign)})")
    start = time.perf_counter()
    h = v.ok("world_hash", recent=4, verify=True)
    verify_s = time.perf_counter() - start
    if not h["verify"].get("matches"):
        problems.append(f"journal replay does not reproduce the state hash: {h['verify']}")
    claims = v.ok("claim_list", limit=200)
    if claims["total"] != len(workers):
        problems.append(f"expected {len(workers)} live claims, found {claims['total']}")
    history = v.ok("world_history", agent=workers[0].name_, limit=5)
    info = v.ok("engine_info")
    return problems, h, verify_s, history, info


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--agents", type=int, default=16)
    ap.add_argument("--rounds", type=int, default=12)
    ap.add_argument("--batch", type=int, default=20, help="spawns per batch round")
    ap.add_argument("--port", type=int, default=PORT)
    ap.add_argument("--debug", action="store_true", help="use the debug build (default: release)")
    ap.add_argument("--no-feed", action="store_true", help="agents don't poll world_changes (isolates its cost)")
    ap.add_argument("--exe", type=Path, help="engine binary (default: target/release or target/debug with --debug)")
    ap.add_argument("--label", default="")
    ap.add_argument("--out", default=str(ROOT / "perf" / "drills"))
    args = ap.parse_args()
    if args.port == 7471:
        sys.exit("never drill on 7471 (the interactive engine's port)")
    exe = args.exe or ROOT / "target" / ("debug" if args.debug else "release") / ("specula.exe" if sys.platform == "win32" else "specula")
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p specula{'' if args.debug else ' --release'}")
    work = Path(tempfile.mkdtemp(prefix="specula-drill-"))
    url = url_for(args.port)
    result = {"schema": "specula.drill/v1", "label": args.label, "agents": args.agents, "rounds": args.rounds,
              "batch": args.batch, "feed": not args.no_feed, "build": "debug" if args.debug else "release",
              "when": datetime.datetime.now().isoformat(timespec="seconds"),
              "cpu_busy_pct_before": cpu_busy_pct()}
    try:
        with Engine(exe, args.port, work / "engine.log", "--headless", "--captures", str(work / "captures"),
                    "--worlds", str(work / "worlds"), cwd=work) as engine:
            ready = threading.Barrier(args.agents)
            workers = [Worker(url, i, args.agents, args.rounds, args.batch, not args.no_feed, ready) for i in range(args.agents)]
            start = time.perf_counter()
            for w in workers:
                w.start()
            for w in workers:
                w.join()
            wall = time.perf_counter() - start
            problems, h, verify_s, history, info = verify(url, workers)
            result["attacks"] = atk = attacks(url)
            if atk["leaked"]:
                problems.append(f"claim bypasses accepted: {atk['leaked']}")
            if atk["wrong"]:
                problems.append(f"attacks refused with unexpected errors: {atk['wrong']}")
            result["overload"] = over = overload(args.port)
            if over["statuses"].get("503+retry-after") != over["extra_requests"] or not over["service_resumed"]:
                problems.append(f"overload: expected every extra request to get 503 + Retry-After: {over}")
            for w in workers:
                w.agent.ok("claim_release", all=True)
            released = Agent(url, "check").ok("claim_list")["total"]
            if released != 0:
                problems.append(f"{released} claims left after release")
        if engine.exit_code not in (0, None):
            problems.append(f"engine exit code {engine.exit_code}")
        errors = [f"{w.name_}: {w.error}" for w in workers if w.error]
        leaked = [l for w in workers for l in w.leaked]
        other = [e for w in workers for e in w.other_errors]
        timings = {}
        for w in workers:
            for tool, samples in w.agent.timings.items():
                timings.setdefault(tool, []).extend(samples)
        ops = sum(w.ops for w in workers)
        calls = sum(len(s) for s in timings.values())
        swarm = info.get("swarm", {})
        result.update({
            "wall_s": round(wall, 3),
            "world_ops": ops,
            "ops_per_s": round(ops / wall, 1),
            "tool_calls": calls,
            "calls_per_s": round(calls / wall, 1),
            "latency_ms": {tool: stats_ms(s) for tool, s in sorted(timings.items())},
            "intrusions_rejected": sum(w.rejected for w in workers),
            "intrusions_leaked": len(leaked),
            "intrusion_other_errors": other[:5],
            "undos": sum(w.undone for w in workers),
            "feed_items": sum(w.feed_items for w in workers),
            "feed_gaps": sum(w.feed_gaps for w in workers),
            "entities": h["entities"],
            "state_hash": h["state_hash"],
            "replay": h["verify"],
            "replay_check_s": round(verify_s, 3),
            "journal": swarm.get("journal"),
            "claim_rejections_engine": swarm.get("claim_rejections"),
            "history_sample": history["entries"][:2],
            "agent_errors": errors,
            "problems": problems + ([f"{len(leaked)} intrusions were accepted"] if leaked else []) +
                        ([f"{len(other)} intrusions failed with another error"] if other else []),
        })
    finally:
        shutil.rmtree(work, ignore_errors=True)
    result["cpu_busy_pct_after"] = cpu_busy_pct()
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    path = out_dir / f"{stamp}-swarm-{args.agents}{'-' + args.label if args.label else ''}.json"
    path.write_text(json.dumps(result, indent=2) + "\n")

    print(f"swarm drill: {args.agents} agents x {args.rounds} rounds x {args.batch}-spawn batches ({result['build']})")
    if "wall_s" in result:
        print(f"  wall {result['wall_s']} s, {result['world_ops']} world ops -> {result['ops_per_s']} ops/s, "
              f"{result['tool_calls']} calls -> {result['calls_per_s']} calls/s")
        print(f"  {'tool':<16}{'p50 ms':>10}{'p95 ms':>10}{'max ms':>10}{'n':>8}")
        for tool, s in result["latency_ms"].items():
            print(f"  {tool:<16}{s['p50']:>10}{s['p95']:>10}{s['max']:>10}{s['n']:>8}")
        print(f"  intrusions rejected {result['intrusions_rejected']}, leaked {result['intrusions_leaked']}; "
              f"undos {result['undos']}; feed items {result['feed_items']} (gaps {result['feed_gaps']})")
        j = result["journal"] or {}
        entries = (j.get("agent_entries") or 0) + (j.get("game_entries") or 0)
        print(f"  entities {result['entities']}, journal {entries} entries ~{(j.get('approx_bytes') or 0) // 1024} KiB, "
              f"replay {'MATCHES' if result['replay'].get('matches') else 'MISMATCH'} "
              f"({result['replay_check_s']} s), hash {result['state_hash']}")
    if "attacks" in result:
        a = result["attacks"]
        print(f"  attacks (review repros): {a['rejected']} refused as expected, {len(a['leaked'])} leaked, "
              f"{len(a['wrong'])} wrong errors")
    print(f"  machine CPU busy: {result.get('cpu_busy_pct_before')}% before, {result.get('cpu_busy_pct_after')}% after")
    if "overload" in result:
        o = result["overload"]
        print(f"  overload: {o['held']} connections held, {o['extra_requests']} extra requests -> {o['statuses']}, "
              f"service resumed: {o['service_resumed']}")
    problems = result.get("problems", []) + result.get("agent_errors", [])
    for p in problems:
        print(f"  PROBLEM: {p}")
    print(f"  saved {path.relative_to(ROOT) if path.is_relative_to(ROOT) else path}")
    print("PASS" if not problems else "FAIL")
    sys.exit(0 if not problems else 1)


if __name__ == "__main__":
    main()
