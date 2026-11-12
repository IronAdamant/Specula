"""Scoped tool lists, live: what each kind of session lists, list_changed, the lead's cap, requests.

Starts the hosted game (Sepulcra, headless) and checks over real MCP sessions:
  1. a client that says nothing lists only the core tools; ?tools=all lists everything; ?role=...
     and ?tools=... pick theirs; a mistyped option refuses the initialize;
  2. tools_catalog lists every tool in one page of a few KB;
  3. tools_activate answers schemas and its HTTP answer is an event stream carrying
     notifications/tools/list_changed first (for a client that accepts text/event-stream);
     a client that accepts only JSON gets plain JSON;
  4. agent_hello {role} picks a template's list;
  5. only the lead may scope others; a capped agent is refused beyond its cap (directly, through
     tools_invoke, in tools_activate and in a playtest's calls); tools_request posts a board task;
  6. the bytes of tools/list for each role template, and how long tools/list and a tool call take.
Writes perf/tools/<date>-toolscope.json (sizes) and prints a summary.

Usage: cargo build -p sepulcra-app && python scripts/toolscope_check.py [--port 7671] [--exe PATH]
       [--release] [--out DIR]
The engine binary is found under CARGO_TARGET_DIR (or target/). Standard library only; ASCII output.
"""
import argparse
import datetime
import json
import os
import sys
import time
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine, Mcp, url_for  # noqa: E402

FAILURES = []


def check(ok, what):
    print(("  ok   " if ok else "  FAIL ") + what)
    if not ok:
        FAILURES.append(what)


def raw_post(url, body, session=None, accept="application/json, text/event-stream"):
    """One POST, returning (status, content type, body text, session header)."""
    headers = {"Content-Type": "application/json", "Accept": accept}
    if session:
        headers["Mcp-Session-Id"] = session
    req = urllib.request.Request(url, json.dumps(body).encode(), headers)
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return r.status, r.headers.get("Content-Type"), r.read().decode(), r.headers.get("Mcp-Session-Id")
    except urllib.error.HTTPError as e:
        return e.code, e.headers.get("Content-Type"), e.read().decode(), None


def sse_messages(text):
    return [json.loads(line[6:]) for line in text.splitlines() if line.startswith("data: ")]


def listed(m):
    return [t["name"] for t in m.request("tools/list", {})["tools"]]


def list_bytes(m):
    return len(json.dumps(m.request("tools/list", {}), separators=(",", ":")))


def call(m, tool_name_, **args):
    """A tool's answer as JSON, and whether it was an error (no raise)."""
    r = m.request("tools/call", {"name": tool_name_, "arguments": args})
    return json.loads(r["content"][0]["text"]), bool(r.get("isError"))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7671)
    ap.add_argument("--exe")
    ap.add_argument("--release", action="store_true")
    ap.add_argument("--out", default=str(ROOT / "target" / "toolscope-check"))
    a = ap.parse_args()
    target = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target") / ("release" if a.release else "debug")
    exe = Path(a.exe) if a.exe else target / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    url = url_for(a.port)
    sizes = {}
    timing = {}
    with Engine(exe, a.port, out / "engine.log", "--headless", "--lead", "lead-1", "--captures", str(out / "captures"), cwd=str(ROOT)):
        print("connect-time scopes")
        bare = Mcp(url, "toolscope-bare", tools=None)
        core = listed(bare)
        check(set(core) == {"agent_hello", "tools_catalog", "tools_activate", "tools_deactivate", "tools_invoke", "tools_request"},
              f"a client that says nothing lists the core only ({len(core)} tools)")
        sizes["core"] = {"tools": len(core), "bytes": list_bytes(bare)}
        full = Mcp(url, "toolscope-all")
        everything = listed(full)
        sizes["all"] = {"tools": len(everything), "bytes": list_bytes(full)}
        check(len(everything) > 140, f"?tools=all lists every tool ({len(everything)})")
        mixed = Mcp(url + "?tools=world,sight,anim_check", "toolscope-mixed", tools=None)
        names = listed(mixed)
        check("world_spawn" in names and "camera_set" in names and "anim_check" in names and "lex_edit" not in names,
              f"?tools=world,sight,anim_check lists those ({len(names)})")
        status, _, body, sid = raw_post(url + "?tools=wrld", {"jsonrpc": "2.0", "id": 1, "method": "initialize",
                                                              "params": {"protocolVersion": "2025-11-25"}})
        check(sid is None and "did you mean 'world'" in body, "a mistyped ?tools refuses the initialize, with a suggestion")
        catalog = Mcp(url, "toolscope-roles")
        roles, _ = call(catalog, "tools_catalog", roles=True)
        for name in roles["roles"]:
            r = Mcp(url + f"?role={name}", f"toolscope-role-{name}", tools=None)
            sizes[f"role {name}"] = {"tools": len(listed(r)), "bytes": list_bytes(r)}
            r.close()

        print("kits by job")
        kits, err = call(catalog, "tools_catalog", kits=True)
        kit_bytes = len(json.dumps(kits, separators=(",", ":")))
        check(not err and len(kits.get("kits", {})) >= 20 and "families" not in kits, f"tools_catalog {{kits: true}} lists the kits and nothing else ({len(kits.get('kits', {}))} kits, {kit_bytes} bytes)")
        sizes["tools_catalog {kits}"] = {"bytes": kit_bytes}
        kit_sizes = []
        for name, k in kits["kits"].items():
            r = Mcp(url + f"?tools=kit:{name}", f"toolscope-kit-{name}", tools=None)
            names, b = listed(r), list_bytes(r)
            r.close()
            sizes[f"kit {name}"] = {"tools": len(names), "bytes": b}
            kit_sizes.append(b)
            # (The catalog's own count is of the server's bytes; this client re-encodes the JSON.)
            if not all(t in names for t in k["tools"] if t != "game") or abs(b - k["list_bytes"]) > 0.03 * b:
                check(False, f"kit {name} lists its tools at the size the catalog says ({len(names)} tools, {b} bytes; catalog {k['list_bytes']})")
        kit_sizes.sort()
        builder = sizes.get("role builder", {}).get("bytes", 0)
        check(kit_sizes[len(kit_sizes) // 2] * 4 < builder, f"the median kit lists under a quarter of the builder role ({kit_sizes[len(kit_sizes) // 2]} against {builder} bytes)")
        sizes["kits median"] = {"bytes": kit_sizes[len(kit_sizes) // 2]}
        one, err = call(catalog, "tools_catalog", kit="make_menu")
        kit = one.get("kit", {})
        check(not err and kit.get("steps") and "ui_check" in kit.get("done", "") and kit.get("tools", {}).get("ui_check", {}).get("tags") == ["reads", "checks"] and "l10n_check" in kit.get("more", {}),
              f"tools_catalog {{kit: make_menu}} gives its tools with tags, the ones it sometimes needs, the steps and the done check ({len(json.dumps(one))} bytes)")
        sizes["tools_catalog {kit}"] = {"bytes": len(json.dumps(one, separators=(",", ":")))}
        act = Mcp(url, "toolscope-kit-activate", tools=None)
        got, err = call(act, "tools_activate", tools=["kit:translate"], schemas=False)
        check(not err and {"l10n_get", "l10n_set", "l10n_check", "ui_check"} <= set(listed(act)) and "world_spawn" not in listed(act), "tools_activate {tools: [kit:translate]} lists that kit's tools")
        # What an activation answers: the definitions as the list shows them (world_set by
        # reference beside world_spawn), so a kit's answer is its list's size, not more.
        place = Mcp(url, "toolscope-kit-place", tools=None)
        got, err = call(place, "tools_activate", tools=["kit:build_place"])
        answer = len(json.dumps(got, separators=(",", ":")))
        ws = next((d for d in got.get("tools", []) if d.get("name") == "world_set"), {})
        check(not err and "as world_spawn's" in json.dumps(ws) and answer < 30_000, f"tools_activate {{kit:build_place}} answers its definitions as listed ({answer} bytes)")
        sizes["tools_activate {kit:build_place} answer"] = {"tools": len(got.get("tools", [])), "bytes": answer}
        place.close()
        bad, err = call(act, "tools_activate", tools=["kit:translat"])
        check(err and "did you mean 'kit:translate'" in json.dumps(bad), "a mistyped kit is refused with the nearest one")
        checks, _ = call(catalog, "tools_catalog", tag="checks")
        fams = checks.get("families", {})
        check("ui_check" in fams.get("ui", {}).get("tools", {}) and "ui_act" not in fams.get("ui", {}).get("tools", {}) and "world" not in fams,
              f"tools_catalog {{tag: checks}} lists what measures work, across families ({checks.get('matches', sum(len(f['tools']) for f in fams.values()))} tools)")
        act.close()

        print("one listing for arguments two tools share")
        both = Mcp(url + "?tools=world_spawn,world_set", "toolscope-shared", tools=None)
        alone = Mcp(url + "?tools=world_set", "toolscope-alone", tools=None)
        ws = next(t for t in both.request("tools/list", {})["tools"] if t["name"] == "world_set")
        wa = next(t for t in alone.request("tools/list", {})["tools"] if t["name"] == "world_set")
        slim, fullsize = len(json.dumps(ws, separators=(",", ":"))), len(json.dumps(wa, separators=(",", ":")))
        check("as world_spawn's" in json.dumps(ws) and "as world_spawn's" not in json.dumps(wa) and slim + 6000 < fullsize,
              f"beside world_spawn, world_set lists its components by reference ({slim} bytes; alone {fullsize})")
        sizes["world_spawn + world_set"] = {"tools": 2, "bytes": list_bytes(both) - sizes["core"]["bytes"]}
        sizes["world_set alone"] = {"tools": 1, "bytes": fullsize}
        # The listing is all that changed: arguments are still checked against the full schema.
        call(both, "agent_hello", name="kit-builder")
        made, err = call(both, "world_spawn", name="kit_probe", shape="cube")
        check(not err, f"world_spawn through the shared listing ({made.get('error')})")
        wrong, err = call(both, "world_set", path="/kit_probe", material=5)
        check(err and "material" in json.dumps(wrong), f"world_set still refuses a component of the wrong type ({wrong.get('error')}: {str(wrong.get('message'))[:80]})")
        right, err = call(both, "world_set", path="/kit_probe", position=[1, 2, 3])
        check(not err, f"and still takes a good change ({right.get('error')})")
        both.close()
        alone.close()

        print("the catalog")
        cat, err = call(bare, "tools_catalog")
        cat_bytes = len(json.dumps(cat, separators=(",", ":")))
        sizes["tools_catalog"] = {"bytes": cat_bytes}
        n = sum(len(f["tools"]) for f in cat["families"].values())
        check(not err and n == len(everything) and "next_cursor" not in cat, f"tools_catalog: all {n} tools in one page, {cat_bytes} bytes")
        nav, _ = call(bare, "tools_catalog", filter="navmesh path")
        check("nav_path" in nav["families"].get("nav", {}).get("tools", {}), "a filter finds nav_path for 'navmesh path'")

        print("activation and list_changed")
        init = raw_post(url, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25"}})
        s = init[3]
        raw_post(url, {"jsonrpc": "2.0", "method": "notifications/initialized"}, s)
        act = {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "tools_activate", "arguments": {"tools": ["sight"]}}}
        status, ctype, body, _ = raw_post(url, act, s)
        msgs = sse_messages(body) if ctype and ctype.startswith("text/event-stream") else []
        check(len(msgs) == 2 and msgs[0].get("method") == "notifications/tools/list_changed" and msgs[1].get("id") == 2,
              f"tools_activate answers as an event stream: list_changed, then the result ({ctype})")
        if msgs:
            res = json.loads(msgs[1]["result"]["content"][0]["text"])
            check("sight_frame" in res["added"] and any(t["name"] == "sight_frame" for t in res["tools"]),
                  f"its answer names the added tools and carries their schemas ({len(res['tools'])} schemas)")
        again = {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "tools_activate", "arguments": {"tools": ["world"]}}}
        status, ctype, body, _ = raw_post(url, again, s, accept="application/json")
        check(ctype == "application/json" and json.loads(body)["id"] == 3, "a client that takes only JSON gets plain JSON")
        status, ctype, body, _ = raw_post(url, {"jsonrpc": "2.0", "id": 4, "method": "tools/list"}, s)
        names = [t["name"] for t in json.loads(body)["result"]["tools"]]
        check("sight_frame" in names and "world_spawn" in names, f"its tools/list shows them ({len(names)})")

        print("roles at agent_hello")
        pt = Mcp(url, "toolscope-pt", tools=None)
        hi, _ = call(pt, "agent_hello", name="pt-1", role="playtester")
        names = listed(pt)
        check(hi["tool_scope"]["from"] == "role playtester" and "play_state" in names and "world_spawn" not in names,
              f"agent_hello role playtester lists the template ({len(names)} tools)")

        print("the lead's scopes and caps")
        lead = Mcp(url, "toolscope-lead", tools=None)
        call(lead, "agent_hello", name="lead-1", role="lead")
        other = Mcp(url, "toolscope-other", tools=None)
        call(other, "agent_hello", name="other-1", role="builder")
        e, err = call(other, "tools_scope", agent="worker-1", tools=["world"])
        check(err and e["error"] == "lead_only", "a builder can't scope others (lead_only)")
        call(lead, "tools_activate", tools=["tools_scope"])
        sc, err = call(lead, "tools_scope", agent="worker-1", tools=["world", "sight_frame", "playtest"], cap=True)
        check(not err and sc["capped"], f"the lead caps worker-1 before it connects ({sc.get('listed')} tools, {sc.get('list_bytes')} bytes)")
        w = Mcp(url, "toolscope-worker", tools=None)
        hi, _ = call(w, "agent_hello", name="worker-1", role="builder")
        names = listed(w)
        check(hi["tool_scope"]["capped"] and "world_spawn" in names and "claim_take" not in names,
              f"worker-1 gets the cap at agent_hello ({len(names)} tools)")
        e, err = call(w, "claim_take", path="/x")
        check(err and e["error"] == "not_in_scope", "a call beyond the cap is refused (not_in_scope)")
        e, err = call(w, "tools_invoke", tool="claim_take", arguments={"path": "/x"})
        check(err and e["error"] == "not_in_scope", "tools_invoke holds the cap too")
        e, err = call(w, "tools_activate", tools=["claim"])
        check(err and e["error"] == "not_in_scope", "activation beyond the cap is refused")
        spec = {"name": "toolscope_cap", "ticks": 2, "steps": [{"at": 0, "call": "claim_list", "args": {}}]}
        e, err = call(w, "playtest_run", spec=spec)
        check(err and e.get("error") == "not_in_scope", f"a playtest calling beyond the cap is refused ({e.get('error')})")
        q, err = call(w, "tools_invoke", tool="world_query", arguments={"limit": 1})
        check(not err and "total" in q, "tools_invoke runs a tool within the cap")
        r, err = call(w, "tools_request", need="claim a region of the slab", why="to build the east gate")
        check(not err and isinstance(r.get("task"), int) and "claim_take" in json.dumps(r), f"tools_request posts task {r.get('task')} with the matches")
        again, err = call(w, "tools_request", need="Claim  a REGION of the slab")
        check(not err and again.get("duplicate") and again.get("task") == r.get("task"), "the same request again answers the open task (duplicate)")
        big, err = call(w, "tools_activate", tools="q," * 100_000)
        check(err and len(json.dumps(big)) < 1024, f"a huge selection is refused small ({big.get('error')})")
        board, _ = call(lead, "tools_invoke", tool="task_list", arguments={"tag": "tool_request"})
        check(any("Tool request" in t.get("title", "") for t in board.get("tasks", [])), "the lead finds it on the board (tag tool_request)")

        print("a cap by kit")
        s, err = call(lead, "tools_invoke", tool="tools_scope", arguments={"agent": "kit-smith", "tools": ["kit:build_place"], "cap": True})
        check(not err and s.get("capped") is True, f"the lead caps an agent to kit:build_place ({s.get('error')})")
        smith = Mcp(url, "toolscope-kit-smith", tools=None)
        call(smith, "agent_hello", name="kit-smith")
        names = [n for n in listed(smith) if not n.startswith("tools_") and n != "agent_hello"]
        check(sorted(names) == ["claim_take", "sight_frame", "world_query", "world_set", "world_spawn"], f"it lists the kit's five tools ({sorted(names)})")
        made, err = call(smith, "world_spawn", name="smith_cube", shape="cube")
        took, err1 = call(smith, "claim_take", path="/smith_cube")
        gave, err2 = call(smith, "tools_invoke", tool="claim_release", arguments={"claim": took.get("claim", took.get("id", 0))})
        check(not err and not err1 and gave.get("error") != "not_in_scope", f"it can release the claim it took: claim_release is one the kit sometimes needs ({gave.get('error')})")
        beyond, err = call(smith, "tools_invoke", tool="lex_edit", arguments={})
        check(err and beyond.get("error") == "not_in_scope", f"and a tool beyond its kit is refused ({beyond.get('error')})")
        smith.close()

        print("timing")
        t0 = time.perf_counter()
        for _ in range(20):
            full.request("tools/list", {})
        timing["tools_list_all_ms"] = round((time.perf_counter() - t0) / 20 * 1000, 2)
        t0 = time.perf_counter()
        for _ in range(20):
            bare.request("tools/list", {})
        timing["tools_list_core_ms"] = round((time.perf_counter() - t0) / 20 * 1000, 2)
        t0 = time.perf_counter()
        for _ in range(50):
            full.call("tools_catalog", family="tools")
        timing["tools_catalog_ms"] = round((time.perf_counter() - t0) / 50 * 1000, 2)
        for m in (bare, full, mixed, catalog, pt, lead, other, w):
            m.close()

    print("sizes (compact tools/list JSON; about 4 bytes a token)")
    for k, v in sizes.items():
        print(f"  {k:24} {v.get('tools', ''):>4} tools {v['bytes']:7} bytes  ~{v['bytes'] // 4:6} tokens")
    print("timing", timing)
    stamp = datetime.datetime.now().strftime("%Y%m%d")
    report = ROOT / "perf" / "tools" / f"{stamp}-toolscope.json"
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps({"schema": "specula.toolscope/v1", "sizes": sizes, "timing": timing,
                                  "failures": FAILURES}, indent=1) + "\n")
    print(f"wrote {report.relative_to(ROOT)}")
    if FAILURES:
        print(f"{len(FAILURES)} check(s) failed")
        return 1
    print("all checks held")
    return 0


if __name__ == "__main__":
    sys.exit(main())
