"""Call rate: how many cheap tool calls one sequential agent gets per second.

One MCP session (HTTP keep-alive, like a real agent's client) calls each tool back to back, waiting
for every reply before sending the next, for `--seconds` per tool: engine_info, world_query (a page
of 20 entities), world_set (moves one entity) and world_spawn. With a window open the engine used to
answer about one call per display refresh (60/s); it now serves calls between presents.

Usage: cargo build -p specula --release && python scripts/call_rate.py [--port 7486] [--headless]
       [--adapter integrated] [--seconds 3] [--exe PATH] [--json OUT]
Needs a GPU and (without --headless) a desktop session. Output is ASCII.
"""
import argparse
import http.client
import json
import sys
import tempfile
import time
from pathlib import Path

from specula_mcp import ROOT, Engine

PORT = 7486


class KeepAlive:
    """A minimal MCP client on one persistent HTTP connection."""

    def __init__(self, port):
        self.conn = http.client.HTTPConnection("127.0.0.1", port, timeout=60)
        self.session, self.next_id = None, 1
        self.rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                "clientInfo": {"name": "call-rate", "version": "0"}})
        self.post({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def post(self, msg):
        headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream",
                   "Connection": "keep-alive"}
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        self.conn.request("POST", "/mcp", json.dumps(msg).encode(), headers)
        r = self.conn.getresponse()
        self.session = self.session or r.getheader("Mcp-Session-Id")
        body = r.read()
        return json.loads(body) if body else None

    def rpc(self, method, params):
        reply = self.post({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        self.next_id += 1
        if "error" in reply:
            raise RuntimeError(f"{method}: {reply['error']}")
        return reply["result"]

    def call(self, tool, **args):
        result = self.rpc("tools/call", {"name": tool, "arguments": args})
        if result.get("isError"):
            raise RuntimeError(f"{tool}: {result['content'][0]['text']}")
        return result


def rate(client, seconds, make_call):
    """Sequential calls per second, and per-call latency p50/p95 in ms."""
    latencies, n = [], 0
    end = time.perf_counter() + seconds
    while True:
        t = time.perf_counter()
        if t >= end:
            break
        make_call(client, n)
        latencies.append(time.perf_counter() - t)
        n += 1
    latencies.sort()
    pick = lambda q: round(latencies[min(len(latencies) - 1, int(q * len(latencies)))] * 1000, 3)
    return {"calls": n, "calls_per_s": round(n / seconds, 1), "p50_ms": pick(0.5), "p95_ms": pick(0.95)}


def pacing(client):
    """perf_frame's window frame pacing over the last 120 frames, and its serve-window state
    (engines that have one); None headless."""
    pf = json.loads(client.call("perf_frame")["content"][0]["text"])
    p = pf.get("frame_pacing")
    if p is not None and pf.get("serve") is not None:
        p = dict(p, serve=pf["serve"])
    return p


def fmt_pacing(p):
    if not p:
        return "n/a"
    s = p.get("serve")
    serve = (f" | window {s['window_ms']} margin {s['margin_ms']} refresh {s['refresh_ms']} ms, "
             f"late {s['late_presents']} (ready {s['late_but_ready']}) of {s['windows']}") if s else ""
    return f"median {p['median_ms']} p95 {p['p95_ms']} max {p['max_ms']} ms{serve}"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--port", type=int, default=PORT)
    ap.add_argument("--headless", action="store_true")
    ap.add_argument("--adapter", default=None)
    ap.add_argument("--seconds", type=float, default=3.0)
    ap.add_argument("--exe", type=Path)
    ap.add_argument("--json", type=Path, help="also write the results here")
    args = ap.parse_args()
    if args.port == 7471:
        sys.exit("never measure on 7471 (the interactive engine's port)")
    exe = args.exe or ROOT / "target" / "release" / ("specula.exe" if sys.platform == "win32" else "specula")
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p specula --release")
    work = Path(tempfile.mkdtemp(prefix="specula-call-rate-"))
    extra = ["--headless"] if args.headless else []
    if args.adapter:
        extra += ["--adapter", args.adapter]
    results = {"schema": "specula.call_rate/v1", "exe": str(exe), "windowed": not args.headless,
               "adapter": args.adapter, "seconds": args.seconds}
    with Engine(exe, args.port, work / "engine.log", *extra, "--captures", str(work / "captures"),
                "--worlds", str(work / "worlds"), cwd=work) as eng:
        c = KeepAlive(args.port)
        for i in range(20):
            c.call("world_spawn", shape="cube", name=f"c{i}", position=[i * 1.5, 0.5, 0])
        time.sleep(2.5)  # a full pacing window (120 frames) of idle frames
        results["frame_pacing_idle"] = idle = pacing(c)
        print(f"{'idle':12s} {'':>8}          frame pacing {fmt_pacing(idle)}")
        calls = {
            "engine_info": lambda cl, n: cl.call("engine_info"),
            "world_query": lambda cl, n: cl.call("world_query", limit=20),
            "world_set": lambda cl, n: cl.call("world_set", name="c0", position=[0, 0.5 + (n % 10) * 0.1, 0]),
            "world_spawn": lambda cl, n: cl.call("world_spawn", shape="cube", position=[n % 50, 3, -(n // 50)]),
        }
        tools = {}
        for name, fn in calls.items():
            tools[name] = r = rate(c, args.seconds, fn)
            # The engine keeps the last 120 frames: the pacing while this tool was being called.
            r["frame_pacing"] = pacing(c)
            print(f"{name:12s} {r['calls_per_s']:>8} calls/s  p50 {r['p50_ms']} ms  p95 {r['p95_ms']} ms  "
                  f"frame pacing {fmt_pacing(r['frame_pacing'])}")
        results["tools"] = tools
    results["validation_errors"] = eng.validation_errors()
    results["exit_code"] = eng.exit_code
    print(f"engine exit {eng.exit_code}, validation errors {eng.validation_errors()}")
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(results, indent=1))


if __name__ == "__main__":
    main()
