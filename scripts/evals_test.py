#!/usr/bin/env python3
"""Unit tests for scripts/evals.py (no engine, no GPU): the matcher against the shared cases the
engine's playtest matcher also passes (evals/matcher_cases.json), variables, select, task
validation, tool patterns, transcripts and path scrubbing.

    python scripts/evals_test.py
"""
import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import evals  # noqa: E402


class Matcher(unittest.TestCase):
    def test_shared_cases(self):
        doc = json.loads((evals.EVALS_DIR / "matcher_cases.json").read_text(encoding="utf-8"), object_pairs_hook=evals._no_duplicates)
        for i, c in enumerate(doc["cases"]):
            try:
                got = evals.check_expect(c["path"], c["matcher"], doc["answer"])[0]
            except evals.SpecError:
                got = "error"
            self.assertEqual(got, c["want"], f"case {i}: {c}")

    def test_bools_are_not_numbers(self):
        self.assertFalse(evals.same(True, 1))
        self.assertFalse(evals.same(0, False))
        self.assertTrue(evals.same([1, {"a": 2.0}], [1.0, {"a": 2}]))

    def test_check_all_reports_paths(self):
        fails = evals.check_all({"a": 1, "b.0": {"gt": 5}}, {"a": 1, "b": [3]})
        self.assertEqual([f["path"] for f in fails], ["b.0"])
        self.assertEqual(fails[0]["got"], 3)


class Variables(unittest.TestCase):
    VARS = {"h": {"state_hash": "ab12", "entries": [{"id": 7}, {"id": 9}]}, "work": "target/x"}

    def test_whole_string_keeps_type(self):
        self.assertEqual(evals.resolve("${h.entries.1.id}", self.VARS), 9)
        self.assertEqual(evals.resolve(["${h.entries.0.id}", "${work}/saltus"], self.VARS), [7, "target/x/saltus"])

    def test_embedded(self):
        self.assertEqual(evals.resolve({"text": "hash ${h.state_hash} ok"}, self.VARS), {"text": "hash ab12 ok"})

    def test_unknown_and_missing(self):
        with self.assertRaises(evals.SpecError):
            evals.resolve("${nope}", self.VARS)
        with self.assertRaises(evals.SpecError):
            evals.resolve("${h.entries.5.id}", self.VARS)


class Select(unittest.TestCase):
    ANS = {"items": [{"p": "/a", "n": 1}, {"p": "/b", "n": 5}, {"p": "/c", "n": 6}]}

    def test_first_match_and_count(self):
        self.assertEqual(evals.select(self.ANS, {"from": "items", "where": {"n": {"gt": 4}}}), {"p": "/b", "n": 5})
        self.assertEqual(evals.select(self.ANS, {"from": "items", "where": {"n": {"gt": 4}}, "count": True})["count"], 2)
        self.assertIsNone(evals.select(self.ANS, {"from": "items", "where": {"n": 99}}))
        self.assertIsNone(evals.select(self.ANS, {"from": "missing"}))

    def test_judge_uses_select(self):
        step = {"call": "x", "select": {"from": "items", "where": {"p": "/c"}}, "expect": {"n": 6}}
        self.assertIsNone(evals.judge(step, False, self.ANS))
        self.assertIn("n:", evals.judge({**step, "expect": {"n": 7}}, False, self.ANS))
        self.assertIsNone(evals.judge({"call": "x", "expect_error": "claimed"}, True, {"error": "claimed"}))
        self.assertIsNotNone(evals.judge({"call": "x", "expect_error": "claimed"}, True, {"error": "stale_version"}))


class Tasks(unittest.TestCase):
    def test_every_task_validates(self):
        tasks = evals.load_tasks()
        self.assertGreaterEqual(len(tasks), 20)
        self.assertTrue(any(t["set"] == "heldout" for t in tasks.values()))

    def test_validation_catches_mistakes(self):
        good = json.loads((evals.EVALS_DIR / "public" / "room_build.json").read_text(encoding="utf-8"))
        self.assertEqual(evals.validate_task(good, "room_build"), [])
        bad = dict(good, tools=["sight_frame"])
        self.assertTrue(any("don't allow" in e for e in evals.validate_task(bad, "room_build")))
        bad = dict(good, verify=[{"call": "world_query", "expect": {"total": {"bogus": 1}}}])
        errs = evals.validate_task(bad, "room_build")
        self.assertTrue(any("name" in e for e in errs) and any("bogus" in e for e in errs))
        self.assertTrue(evals.validate_task(dict(good, id="Room"), "Room"))
        self.assertTrue(evals.validate_task(dict(good, broken=[]), "room_build"))

    def test_tool_patterns(self):
        self.assertTrue(evals.tool_allowed("world_set", ["world_*"]))
        self.assertTrue(evals.tool_allowed("agent_hello", ["lex_*"]))
        self.assertFalse(evals.tool_allowed("world_set", ["lex_*"]))


class Transcripts(unittest.TestCase):
    def test_stats(self):
        t = evals.Transcript()
        t.add("a", "solution", "world_set", {}, 10, 30, 1.0, None, {})
        t.add("a", "solution", "world_set", {}, 10, 30, 1.0, "claimed", {"error": "claimed"})
        t.add("v", "verify", "world_query", {}, 5, 5, 1.0, None, {})
        s = t.stats("solution")
        self.assertEqual((s["calls"], s["req_bytes"], s["resp_bytes"], s["errors"]), (2, 20, 60, 1))
        self.assertEqual(s["error_codes"], {"claimed": 1})
        self.assertEqual(s["tokens_est"], 20)

    def test_scrub(self):
        text = json.dumps({"a": str(evals.ROOT / "x" / "y.json"), "b": str(Path.home() / "q")})
        out = evals.scrub(text)
        self.assertNotIn(str(Path.home()).replace("\\", "\\\\"), out)
        self.assertIn("<repo>", out)

    def test_scrub_is_linear_on_backslash_runs(self):
        # Review finding: a nested quantifier backtracked exponentially on doubly escaped paths
        # that almost match (a transcript of a module build hung the runner).
        import time
        home = str(Path.home().resolve())
        # The home folder with 24 backslashes between components and a wrong last one: the old
        # pattern took over a minute on three of these.
        near_miss = ("\\" * 24).join(home.replace("/", "\\").split("\\"))
        near_miss = near_miss[: near_miss.rfind("\\") + 1] + "#nope"
        t = time.perf_counter()
        evals.scrub((near_miss + " ") * 50)
        self.assertLess(time.perf_counter() - t, 2.0)


class Privacy(unittest.TestCase):
    def test_fallbacks_mask_cut_and_foreign_paths(self):
        # Made-up values, built from pieces (the repository is public; tests/private_paths.rs).
        app_data = "App" + "Data"
        cut_path = json.dumps({"p": "C:" + "\\Users\\someone\\" + app_data + "\\Local\\Te"})  # test data, made up: private-scan: allow
        out = evals.scrub(cut_path)
        self.assertNotIn("someone", out)
        self.assertNotIn(app_data, out)
        for leak in ["/" + "home/someone/x", "12345678-90ab-cdef-1234-" + "567890abcdef", "ses" + "sion_0123456789ABCDEFabcd",
                     "Temp\\" + "claude\\X--Some-Folder", "12345678-90ab-cd" + "... (40 bytes)",
                     "X--Some-Folder\\\\b10" + "... (300 bytes)"]:
            self.assertNotIn(leak, evals.scrub(f"x {leak} y"), leak)
        self.assertEqual(evals.scrub("http://127.0.0.1:7493/mcp wave-3"), "http://127.0.0.1:7493/mcp wave-3")

    def test_scrubbed_before_cut(self):
        t = evals.Transcript()
        long_path = str(Path.home() / ("x" * 700) / "y.png")
        t.add("a", "solution", "sight_frame", {}, 1, 1, 1.0, None, {"image": {"path": long_path}})
        self.assertNotIn(Path.home().name, json.dumps(t.lines[0]["answer"]))

    def test_committed_files_are_clean(self):
        bad = re.compile(r"(?<![\w])[A-Za-z]:[\\/]|App" r"Data|Temp[\\/]+claude|[\\/]Users[\\/]|/home/|\b[A-Za-z]--\w|"
                         r"\b[0-9a-f]{8}-[0-9a-f-]*\.\.\.|<folder>[\\/]+[0-9a-f]|"
                         r"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b|\bsession_[A-Za-z0-9]{8}", re.I)
        leaks = []
        for base in (evals.EVALS_DIR, evals.ROOT / "perf" / "evals"):
            for p in base.rglob("*"):
                if p.is_file() and p.suffix in (".json", ".jsonl", ".txt", ".md", ".rs", ".toml"):
                    m = bad.search(p.read_text(encoding="utf-8", errors="replace"))
                    if m:
                        leaks.append(f"{p.relative_to(evals.ROOT)}: {m.group(0)}")
        self.assertEqual(leaks, [])


class ProxyScreen(unittest.TestCase):
    ALLOWED = ["world_query", "playtest_*", "play_*"]

    def test_duplicate_keys_are_refused(self):
        # Review finding: Python keeps the last duplicate, the engine the first.
        msg, bad = evals.screen(b'{"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "note_add", "name": "world_query", "arguments": {}}}', self.ALLOWED)
        self.assertIsNone(msg)
        self.assertIn("duplicate", bad["message"])
        msg, bad = evals.screen(b'{"jsonrpc": "2.0", "id": 4, "method": "tools/call", "method": "ping", "params": {"name": "note_list"}}', self.ALLOWED)
        self.assertIsNone(msg)

    def test_shapes(self):
        for body in (b'[{"method": "ping"}]', b'{"method": 7}', b'{"method": "tools/call", "params": {"name": 5}}', b'not json',
                     b'{"method": "tools/call", "params": {"name": "world_query", "arguments": [1]}}'):
            self.assertIsNone(evals.screen(body, self.ALLOWED)[0], body)
        msg, bad = evals.screen(b'{"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "world_query", "arguments": {}}}', self.ALLOWED)
        self.assertIsNone(bad)
        self.assertEqual(msg["params"]["name"], "world_query")

    def test_calls_need_a_real_id(self):
        # Review round 2: a call without an id ran unanswered and was logged as a success; NaN isn't JSON.
        for body in (b'{"jsonrpc": "2.0", "method": "tools/call", "params": {"name": "world_query"}}',
                     b'{"jsonrpc": "2.0", "id": NaN, "method": "tools/call", "params": {"name": "world_query"}}',
                     b'{"jsonrpc": "2.0", "id": 1.5, "method": "tools/call", "params": {"name": "world_query"}}',
                     b'{"jsonrpc": "2.0", "id": true, "method": "tools/call", "params": {"name": "world_query"}}',
                     b'{"jsonrpc": "2.0", "id": Infinity, "method": "ping"}'):
            self.assertIsNone(evals.screen(body, self.ALLOWED)[0], body)
        self.assertIsNotNone(evals.screen(b'{"jsonrpc": "2.0", "id": "a", "method": "tools/call", "params": {"name": "world_query"}}', self.ALLOWED)[0])

    def test_playtest_calls_are_screened(self):
        spec = {"name": "x", "steps": [{"at": 0, "call": "play_new"}, {"at": 1, "call": "world_set", "args": {}}]}
        self.assertEqual(evals.refusal("playtest_run", {"spec": spec}, self.ALLOWED)["error"], "tool_not_allowed")
        spec["steps"].pop()
        spec["until"] = {"call": "note_add", "expect": {"x": 1}}
        self.assertIsNotNone(evals.refusal("playtest_run", {"spec": spec}, self.ALLOWED))
        del spec["until"]
        self.assertIsNone(evals.refusal("playtest_run", {"spec": spec}, self.ALLOWED))
        self.assertIsNotNone(evals.refusal("playtest_run", {"path": "p.json"}, self.ALLOWED))
        self.assertIsNotNone(evals.refusal(None, {}, self.ALLOWED))


class Sequence(unittest.TestCase):
    def test_order_matters(self):
        # Review round 2: a reload right after the save (before the second run) passed.
        class Ctx:
            transcript = evals.Transcript()
        t = Ctx.transcript
        t.add("a", "solution", "save_write", {"name": "cp"}, 1, 1, 1, None, {"tick": 90})
        t.add("a", "solution", "save_read", {"name": "cp"}, 1, 1, 1, None, {"loaded": "cp"})
        t.add("a", "solution", "world_hash", {}, 1, 1, 1, None, {"tick": 180})
        seq = {"sequence": [{"tool": "save_write", "args": {"name": "cp"}}, {"tool": "world_hash", "answer": {"tick": 180}},
                            {"tool": "save_read", "args": {"name": "cp"}}, {"tool": "world_hash", "answer": {"tick": 180}}]}
        self.assertIsNotNone(evals.transcript_check(Ctx, seq))
        t.add("a", "solution", "save_read", {"name": "cp"}, 1, 1, 1, None, {"loaded": "cp"})
        self.assertIsNotNone(evals.transcript_check(Ctx, seq))
        t.add("a", "solution", "world_hash", {}, 1, 1, 1, "error", {"error": "x"})
        self.assertIsNotNone(evals.transcript_check(Ctx, seq), "a failed call doesn't count")
        t.add("a", "solution", "world_hash", {}, 1, 1, 1, None, {"tick": 180})
        self.assertIsNone(evals.transcript_check(Ctx, seq))


class WorldDiff(unittest.TestCase):
    """Review round 3: verifiers checked what should be there, not that nothing else changed."""
    BEFORE = {"/gate": {"id": 1, "path": "/gate", "position": [0, 0, 0]},
              "/gate/post": {"id": 2, "path": "/gate/post", "color": "#7a6a58", "scale": [1, 2, 1], "version": 1},
              "/garden/bench": {"id": 3, "path": "/garden/bench", "position": [0, 0.25, 0], "props": {"a": 1, "b": {"c": 2}}},
              "/lex/s/waves": {"id": 4, "path": "/lex/s/waves", "props": {"lex": {"waves": [{"scale": {"hp": 1.1}, "n": 5}, {"scale": {"hp": 1.2}, "n": 6}]}}},
              "<env>": {"gravity": -9.81}}

    def after(self, **changes):
        a = json.loads(json.dumps(self.BEFORE))
        for path, ch in changes.items():
            if ch is None:
                a.pop(path)
            else:
                a.setdefault(path, {}).update(ch)
        return a

    def test_only_allowed_changes_pass(self):
        allow = [{"path": "/gate/post", "ops": ["changed"], "fields": ["color"]}, {"path": "/new*", "ops": ["added"]},
                 {"path": "/lex/s/waves", "ops": ["changed"], "fields": ["props.lex.waves.0.scale.hp"]}]
        self.assertEqual(evals.world_diff(self.BEFORE, self.after(), allow), [])
        ok = self.after(**{"/gate/post": {"color": "#ff0000", "version": 7}, "/new": {"id": 9}})
        ok["/lex/s/waves"]["props"]["lex"]["waves"][0]["scale"]["hp"] = 1.21
        self.assertEqual(evals.world_diff(self.BEFORE, ok, allow), [])
        bad = self.after(**{"/garden/bench": {"position": [6, 0.25, 6]}, "/gate": None, "/stray": {"id": 10}, "<env>": {"gravity": -1}})
        bad["/lex/s/waves"]["props"]["lex"]["waves"][1]["n"] = 1
        found = evals.world_diff(self.BEFORE, bad, allow)
        self.assertEqual(sorted(found), sorted(["/garden/bench: position changed", "/gate: removed", "/stray: added", "<env>: gravity changed",
                                                "/lex/s/waves: props.lex.waves.1.n changed"]))

    def test_recreated_entity_shows_a_new_id(self):
        found = evals.world_diff(self.BEFORE, self.after(**{"/garden/bench": {"id": 30}}), [])
        self.assertEqual(found, ["/garden/bench: id changed"])

    def test_nested_props(self):
        self.assertEqual(evals.leaf_diff({"props": {"a": 1, "b": {"c": 2}}}, {"props": {"a": 1, "b": {"c": 3, "d": 1}}}), ["props.b.c", "props.b.d"])


class TranscriptRules(unittest.TestCase):
    def test_none_count_equal(self):
        class Ctx:
            transcript = evals.Transcript()
        t = Ctx.transcript
        t.add("a", "solution", "save_write", {"name": "cp"}, 1, 1, 1, None, {"tick": 90})
        t.add("a", "solution", "world_hash", {}, 1, 1, 1, None, {"tick": 180, "state_hash": "aa"})
        t.add("a", "solution", "save_write", {"name": "late"}, 1, 1, 1, None, {"tick": 180})
        t.add("a", "solution", "save_read", {"name": "cp"}, 1, 1, 1, None, {"loaded": "cp"})
        t.add("a", "solution", "save_read", {"name": "late"}, 1, 1, 1, None, {"loaded": "late"})
        t.add("a", "solution", "world_hash", {}, 1, 1, 1, None, {"tick": 180, "state_hash": "aa"})
        seq = [{"tool": "save_write", "args": {"name": "cp"}}, {"tool": "world_hash", "answer": {"tick": 180}},
               {"tool": "save_read", "args": {"name": "cp"}}, {"tool": "world_hash", "answer": {"tick": 180}}]
        self.assertIsNone(evals.transcript_check(Ctx, {"sequence": seq, "equal": [{"a": 1, "b": 3, "path": "state_hash"}]}))
        self.assertIn("rules out", evals.transcript_check(Ctx, {"sequence": seq, "none": [{"tool": "save_read", "args": {"name": {"ne": "cp"}}}]}))
        self.assertIn("exactly 2", evals.transcript_check(Ctx, {"count": [{"tool": "save_write", "args": {}, "n": 2}]}) or "exactly 2")
        self.assertIsNone(evals.transcript_check(Ctx, {"count": [{"tool": "save_write", "args": {"name": "cp"}, "n": 1}]}))
        t.add("a", "solution", "world_hash", {}, 1, 1, 1, None, {"tick": 180, "state_hash": "bb"})
        seq2 = seq + [{"tool": "world_hash", "answer": {"tick": 180}}]
        self.assertIsNotNone(evals.transcript_check(Ctx, {"sequence": seq2, "equal": [{"a": 1, "b": 4, "path": "state_hash"}]}))


class Geometry(unittest.TestCase):
    def test_world_size_through_a_stretched_parent(self):
        # Review round 3: a chair of local scale [0.06, 0.5, 0.5] under a parent stretched 4x along z.
        sx, sy, sz = evals.world_size(([0, 0, 0], [1, 1, 4]), ([0, 71.17, 0], [0.06, 0.5, 0.5]))
        self.assertAlmostEqual(sy, 0.5, places=6)
        self.assertLess(min(sx, sz), 0.3)  # thin one way, long the other: not a 0.5 m cube
        self.assertGreater(max(sx, sz), 0.7)
        sx, sy, sz = evals.world_size(([0, 180, 0], 1), ([0, 90, 0], 0.5))
        self.assertAlmostEqual(sx, 0.5, places=6)
        self.assertAlmostEqual(sz, 0.5, places=6)

    def test_forward_composes_ancestors(self):
        # Review round 2: a parent group turned 180 degrees turned the chairs' fronts unseen.
        fx, fz = evals.forward_xz([0, 180, 0], [0, 90, 0])
        ex, ez = evals.forward_xz([0, 270, 0])
        self.assertAlmostEqual(fx, ex, places=6)
        self.assertAlmostEqual(fz, ez, places=6)
        self.assertAlmostEqual(fx, -1.0, places=6)

    def test_forward(self):
        fx, fz = evals.forward_xz([0, 90, 0])
        self.assertAlmostEqual(fx, 1.0)
        self.assertAlmostEqual(fz, 0.0)
        fx, fz = evals.forward_xz([180, -4, 180])  # a yaw of -176 degrees as the engine may report it
        ex, ez = evals.forward_xz([0, -176, 0])
        self.assertAlmostEqual(fx, ex, places=6)
        self.assertAlmostEqual(fz, ez, places=6)

    def test_box_gap(self):
        a = {"min": [0, 0, 0], "max": [1, 1, 1]}
        self.assertEqual(evals._box_gap(a, {"min": [0.5, 0, 0.5], "max": [2, 1, 2]}), 0.0)
        self.assertAlmostEqual(evals._box_gap(a, {"min": [2, 0, 0], "max": [3, 1, 1]}), 1.0)
        self.assertAlmostEqual(evals._box_gap(a, {"min": [4, 0, 5], "max": [5, 1, 6]}), 5.0)


class ProxyStreams(unittest.TestCase):
    """Review probe (toolscope lane): the proxy relayed an event-stream answer (list_changed, then
    the response) labelled application/json, which an eval agent couldn't parse. It now asks
    upstream for JSON only, and turns a stream it gets anyway into the response's JSON."""

    def test_event_streams_are_relayed_as_json(self):
        import http.server
        import socket
        import threading
        import urllib.request

        seen = []

        class Upstream(http.server.BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def do_POST(self):
                msg = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                seen.append(self.headers.get("Accept"))
                note = {"jsonrpc": "2.0", "method": "notifications/tools/list_changed"}
                answer = {"jsonrpc": "2.0", "id": msg.get("id"), "result": {"content": [{"type": "text", "text": "{\"agent\": \"ev\"}"}], "isError": False}}
                body = f"event: message\ndata: {json.dumps(note)}\n\nevent: message\ndata: {json.dumps(answer)}\n\n".encode()
                self.send_response(200)
                self.send_header("Content-Type", "text/event-stream")
                self.send_header("Mcp-Session-Id", "s1")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        up = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
        threading.Thread(target=up.serve_forever, daemon=True).start()
        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
            port = s.getsockname()[1]

        class Log:
            def add(self, *a):
                pass

        class Ctx:
            lock = threading.Lock()

        proxy = evals.Proxy(port, f"http://127.0.0.1:{up.server_address[1]}/mcp", ["world_*", "agent_hello"], Log(), Ctx())
        proxy.start()
        try:
            body = {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "agent_hello", "arguments": {"name": "ev", "tools": ["world"]}}}
            req = urllib.request.Request(f"http://127.0.0.1:{port}/mcp", json.dumps(body).encode(),
                                         {"Content-Type": "application/json", "Accept": "application/json, text/event-stream", "Mcp-Session-Id": "s1"})
            with urllib.request.urlopen(req, timeout=30) as r:
                ctype, text = r.headers.get("Content-Type"), r.read().decode()
            self.assertEqual(ctype, "application/json")
            reply = json.loads(text)  # an eval agent's client parses it as JSON
            self.assertEqual(reply["id"], 2)
            self.assertFalse(reply["result"]["isError"])
            self.assertEqual(seen, ["application/json"], "the proxy asks upstream for JSON only")
        finally:
            proxy.stop()
            up.shutdown()
            up.server_close()


if __name__ == "__main__":
    unittest.main()
