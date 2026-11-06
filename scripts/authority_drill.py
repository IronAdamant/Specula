"""The authority drill (docs/design/AUTHORITY.md): a second agent acts on the first agent's
objects through every tool the authority table marks `owned:*`, and must be refused (or the
table must say why it is allowed). Also: roles are never self-declared, engine writer names are
reserved, and the answers' provenance and drawn facts are truthful.

One headless engine (`--lead lee --reviewer rev`): ann makes things (a claimed box, a task, a
note, an intent, a lease, a camera, a review submission, a critique, a save, a perf capture, a
playtest); bob tries to change, cancel, replace or approve each. The engine's own access table
(engine_info detailed: access.tools) lists the owned tools; a tool with no probe here fails
the drill, so a new owned tool needs a probe.

Usage: python scripts/authority_drill.py [--exe ...] [--port 7760]
Exit 0 when every probe held. Writes target/authority-drill/report.json.
"""
import argparse
import json
import os
import shutil
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import Engine  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

# Owned tools whose cross-agent paths can't be reached by a headless drill, and the tests
# that cover them instead.
COVERED_ELSEWHERE = {
    "stream_fly": "needs a window; jobs::tests (busy refused naming the owner, own restart replaces)",
    "vfx_preview": "previews are per caller: at most a few per session, refused naming its own running previews; no tool cancels or replaces another's (vfx_tools tests)",
    "lex_session": "lex_tools tests: only the session that saved a run deletes it",
    "project_upload": "scripts/disk_check.py: another agent can't abort or continue an upload (upload_not_found)",
    "trace_incident": "trace_tools tests and scripts/trace_check.py: resolving or reopening needs an agent the incident names, reviewer authority (of_session) or the lead; an incident needs a real stall to exist",
}


def id_of(body):
    """The first `id` in an answer (top level first, then nested objects)."""
    queue = [body]
    while queue:
        v = queue.pop(0)
        if isinstance(v, dict):
            if isinstance(v.get("id"), int):
                return v["id"]
            queue.extend(v.values())
        elif isinstance(v, list):
            queue.extend(v)
    raise KeyError(f"no id in {json.dumps(body)[:300]}")


class Drill:
    def __init__(self, eng):
        self.eng = eng
        self.agents = {}
        self.results = []
        self.probed = set()

    def agent(self, name, role="builder"):
        if name not in self.agents:
            m = self.eng.mcp(f"drill-{name}")
            r = self.raw(m, "agent_hello", {"name": name, "role": role})
            if r[0]:
                raise RuntimeError(f"agent_hello {name} as {role}: {r}")
            self.agents[name] = m
        return self.agents[name]

    @staticmethod
    def raw(m, tool, args):
        result = m.request("tools/call", {"name": tool, "arguments": args})
        text = next((c["text"] for c in result.get("content", []) if c.get("type") == "text"), "")
        try:
            body = json.loads(text)
        except ValueError:
            body = {"text": text}
        code = body.get("error") if result.get("isError") and isinstance(body, dict) else None
        if result.get("isError") and code is None:
            # Batch answers (critique_update, review_decide): the first item's refusal.
            first = (body.get("results") or [{}])[0] if isinstance(body, dict) else {}
            code = first.get("error") or "error"
        return code, body

    def ok(self, who, tool, **args):
        code, body = self.raw(self.agent(who), tool, args)
        if code:
            raise RuntimeError(f"{who} {tool}: {code}: {json.dumps(body)[:400]}")
        return body

    def expect(self, label, who, tool, args, codes, m=None):
        """`who` calling `tool` must be refused with one of `codes` (None: any refusal)."""
        self.probed.add(tool)
        code, body = self.raw(m or self.agent(who), tool, args)
        held = code is not None and (codes is None or code in codes)
        self.results.append({"probe": label, "by": who, "tool": tool, "held": held, "code": code, "expected": codes,
                             "answer": json.dumps(body)[:300] if not held else None})
        print(f"  {'ok  ' if held else 'FAIL'} {label}: {tool} -> {code}")
        return body

    def check(self, label, cond, detail=""):
        self.results.append({"probe": label, "held": bool(cond), "detail": detail})
        print(f"  {'ok  ' if cond else 'FAIL'} {label}{(': ' + detail) if detail and not cond else ''}")


def wait_until(pred, timeout=30.0, step=0.2):
    end = time.time() + timeout
    while time.time() < end:
        v = pred()
        if v:
            return v
        time.sleep(step)
    return pred()


def window_click(exe, port, out):
    """The human's keyboard in the window (review R3 of m5-text, lead direction): only the
    operator points it at a world text, and a click never does. Kept as rows: the reviewer's
    probe_click2 (bob's editable text behind his cube takes neither the click nor the keys), the
    three probe_cover layerings (glass, a 2D shape and a text on a higher z layer behind an
    opened field: the human's click on the field and the keys after it are the game's), a
    builder rewriting the focused field (the focus drops before the keys, which are withheld),
    the banner drawn last in every frame while the focus holds, and the probe_focuscost case at
    100,000 entities (window_focus holds the engine thread no longer than a plain text_edit).
    Needs a desktop session (Windows): skipped, with the reason, without."""
    print("the human's keyboard (windowed engine):")
    results = []

    def check(label, cond, detail=""):
        results.append({"probe": label, "held": bool(cond), "detail": detail})
        print(f"  {'ok  ' if cond else 'FAIL'} {label}{(': ' + detail) if detail and not cond else ''}")

    if sys.platform != "win32":
        check("the human's keyboard (skipped: needs a Windows desktop)", True)
        return results
    import ctypes
    import ctypes.wintypes as wt
    import window_check as wc
    u32 = ctypes.WinDLL("user32", use_last_error=True)
    u32.PostMessageW.argtypes = [wt.HWND, wt.UINT, wt.WPARAM, wt.LPARAM]
    (out / "assets-click").mkdir(parents=True, exist_ok=True)
    args = ["--size", "640x360", "--captures", str(out / "captures-click"), "--worlds", str(out / "worlds-click"), "--lead", "lee", "--trace", str(out / "trace-click"),
            "--assets", str(out / "assets-click")]
    with Engine(exe, port, out / "engine-click.log", *args, cwd=str(ROOT)) as eng:
        d = Drill(eng)
        d.agent("bob")
        d.agent("lee", "lead")
        d.agent("eve")
        hwnd = wait_until(lambda: wc.find_window(eng.proc.pid), timeout=20)
        if not hwnd:
            check("the human's keyboard (skipped: no window; no desktop session?)", True)
            return results
        d.ok("bob", "world_spawn", shape="cube", name="wall", position=[0, 0, 0], scale=[6, 4, 0.2], color="#808080")
        d.ok("bob", "world_spawn", shape="text", name="sink", position=[0, 0, -1.0], color="#ffffff",
             text={"text": "M" * 12, "size": 0.4, "max_width": 3, "editing": {"caret": 0}})
        d.ok("bob", "camera_set", name="trap", eye=[0, 0, 8], target=[0, 0, 0])
        d.ok("bob", "window_view", camera="trap")
        time.sleep(1.5)
        w, h = wc.client_size(hwnd)

        def click(x=None, y=None):
            lp = ((h // 2 if y is None else y) << 16) | (w // 2 if x is None else x)
            u32.PostMessageW(hwnd, 0x0201, 1, lp)  # WM_LBUTTONDOWN (the window's middle by default)
            u32.PostMessageW(hwnd, 0x0202, 0, lp)
            time.sleep(0.6)

        def typed(chars):
            for c in chars:
                u32.PostMessageW(hwnd, 0x0102, ord(c), 0)  # WM_CHAR
            time.sleep(1.0)

        def words(path):
            q = d.ok("eve", "world_query", path_prefix=path, fields=["text", "modified_by"], limit=1)
            item = (q.get("items") or [{}])[0]
            return (item.get("text") or {}).get("text") or "", item.get("modified_by")

        def typing():
            info = d.ok("eve", "engine_info", response_format="detailed")
            return (info.get("window") or {}).get("typing")

        # probe_click2.
        click()
        info = d.ok("eve", "engine_info", response_format="detailed")
        released = (info.get("window") or {}).get("released_by")
        check("the human's click on bob's cube reaches the game (it takes the window back from bob's camera)", released == "input", f"released_by {released!r}")
        typed("hunter2")
        text, by = words("/sink")
        check("bob's editable text behind his cube takes neither the click nor the keys (probe_click2)", "hunter2" not in text and by != "window:owner", f"{text!r} by {by}")
        # The lead's field, in front of bob's wall: the operator points the keyboard there, and the
        # banner is the last of every window frame's overlay while it holds.
        d.ok("lee", "world_spawn", shape="text", name="field", position=[0, 0, 1.0], color="#ffffff", text={"text": "name", "size": 0.5, "anchor": [0.5, 0.5], "editing": {"caret": 4}})
        time.sleep(0.5)
        code, body = d.raw(d.agent("lee"), "text_edit", {"path": "/field", "ops": [], "window_focus": True})
        check("the lead points the keyboard at the field", code is None and (body or {}).get("window_focus") is True, f"{code} {json.dumps(body)[:200]}")
        t = wait_until(lambda: ((typing() or {}).get("banner") or {}).get("frames", 0) >= 3 and typing(), timeout=10)
        banner = (t or {}).get("banner") or {}
        check("while the focus holds, the banner is the last of the frame's overlay, frame after frame", (t or {}).get("path") == "/field" and banner.get("last") is True and banner.get("quads", 0) > 1, json.dumps(t)[:200])
        typed("ok")
        text, by = words("/field")
        check("the human's keys go to the text the operator pointed them at", text == "nameok" and by == "window:owner", f"{text!r} by {by}")
        # Any click ends it, on the field itself too, and never takes it back.
        click()
        check("a click on the focused field ends the typing", typing() is None, json.dumps(typing())[:200])
        typed("xyz")
        text, _ = words("/field")
        check("... and the keys after it are the game's", "xyz" not in text, repr(text))
        click()
        typed("xyz")
        text, _ = words("/field")
        check("a click on the field never points the keyboard back at it", typing() is None and "xyz" not in text, repr(text))
        # probe_cover: bob paints over the field from behind it; the human clicks it and types.
        covers = [
            ("glass behind the field", {"shape": "cube", "name": "gb", "position": [0, 0, 0.7], "scale": [5, 3, 0.1], "material": "glass", "opacity": 1.0, "color": "#707070"}),
            ("a 2D shape on a higher z layer behind the field", {"shape": "shape2d", "name": "sb", "position": [0, 0, 0.7], "shape2d": {"items": [{"rect": [-2.5, -1.5, 5, 3], "fill": "#707070"}], "z_layer": 5}}),
            ("a text on a higher z layer behind the field", {"shape": "text", "name": "tb", "position": [0, 0, 0.7], "color": "#707070", "text": {"text": "█" * 6, "size": 1.5, "anchor": [0.5, 0.5], "z_layer": 5}}),
        ]
        for label, spec in covers:
            d.ok("bob", "world_spawn", **spec)
            time.sleep(0.5)
            click()
            typed("AAA")
            text, by = words("/field")
            check(f"probe_cover, {label}: the click and the keys are the game's", typing() is None and "AAA" not in text, f"{text!r} by {by}")
            d.ok("bob", "world_delete", path="/" + spec["name"])
        # probe_cover D: bob rewrites the focused field; the focus drops before the keys.
        d.ok("lee", "text_edit", path="/field", ops=[], window_focus=True)
        wait_until(lambda: ((typing() or {}).get("banner") or {}).get("last"), timeout=10)
        d.ok("bob", "world_set", path="/field", scale=[12, 12, 1], color="#202020")
        time.sleep(0.3)
        typed("secret")
        text, _ = words("/field")
        check("probe_cover, bob rewrites the focused field: the focus drops and no key lands", typing() is None and "secret" not in text, repr(text))
        tr = d.ok("lee", "trace_query", code="text.window_focus_dropped", limit=20)
        check("the drop is traced as foreign_write", "foreign_write" in json.dumps(tr), json.dumps(tr)[:300])
        d.ok("lee", "world_set", path="/field", scale=[1, 1, 1], color="#ffffff")
        # Review R4 (probe_screen2): bob's gallery can't take the window while the human types;
        # when the lead puts it there, the keys the human goes on typing never reach it.
        space = [{"name": "s", "target": "light.key_intensity", "min": 0.3, "max": 1.0}]
        d.ok("bob", "gallery_create", id="bobs", subject={"stage": {}}, space=space, variants=3)
        d.ok("lee", "text_edit", path="/field", ops=[], window_focus=True)
        wait_until(lambda: ((typing() or {}).get("banner") or {}).get("last"), timeout=10)
        code, body = d.raw(d.agent("bob"), "gallery_open", {"id": "bobs"})
        check("bob's gallery_open while the human types is refused (window_typing)", code == "window_typing" and typing() is not None, f"{code} {json.dumps(body)[:200]}")
        code, body = d.raw(d.agent("lee"), "gallery_open", {"id": "bobs"})
        check("the lead may put the gallery on the window", code is None, f"{code} {json.dumps(body)[:200]}")
        for vk in (0x32, 0x0D, 0x0D):
            u32.PostMessageW(hwnd, 0x0100, vk, 1)  # WM_KEYDOWN: "2", Enter, Enter
            u32.PostMessageW(hwnd, 0x0101, vk, 0xC0000001)
            time.sleep(0.12)
        time.sleep(1.0)
        tr = json.dumps(d.ok("lee", "trace_query", code="text.window_keys_withheld", limit=5))
        g = d.ok("bob", "gallery_get", id="bobs")
        check("... and the keys the human goes on typing reach neither the gallery nor the game", typing() is None and "window_keys_withheld" in tr and g.get("status") != "accepted",
              f"typing {typing()}, gallery {g.get('status')}, trace {tr[:200]}")
        d.ok("lee", "gallery_close")
        # probe_focuscost at 100,000 entities (behind the camera: drawn by nobody, walked by the
        # old cover test): window_focus holds the engine thread no longer than a plain text_edit.
        for b in range(100):
            d.ok("lee", "world_batch", ops=[{"op": "spawn", "shape": "cube", "position": [((b * 1000 + i) % 400) * 1.5, ((b * 1000 + i) // 400) * 1.5, 60]} for i in range(1000)])
        time.sleep(1.0)

        def stalled():
            tc = d.ok("lee", "trace_call")
            return any(i.get("code") == "engine.stall" for i in tc.get("incidents") or []), (tc.get("call") or {}).get("ms")
        plain, focused = [], []
        for _ in range(5):
            d.ok("lee", "text_edit", path="/field", ops=[{"key": "end"}])
            plain.append(stalled())
            d.ok("lee", "text_edit", path="/field", ops=[], window_focus=True)
            focused.append(stalled())
        print(f"    100k: plain text_edit (stalled, ms) {plain}; window_focus {focused}")
        check("window_focus at 100,000 entities stalls the engine thread no more than a plain text_edit (probe_focuscost)",
              sum(s for s, _ in focused) <= sum(s for s, _ in plain), f"stalls: plain {plain}, window_focus {focused}")
    check("the windowed engine exited cleanly", eng.exit_code == 0, f"exit {eng.exit_code}")
    return results


def one_call_one_commit(exe, port, out):
    """AGENTS rule 14 on the taste tools, against this (debug) engine, whose debug_assert panics
    on a second commit (the M4 final audit: gallery_apply and taste_suggest {apply} committed
    their results, then the gallery or the log). Also the dropped reply: gallery calls back to
    back with a project sync in flight once failed with no_answer, one in three."""
    print("one tool call, one commit (taste results), and gallery replies back to back:")
    assets = out / "assets3"
    shutil.copytree(ROOT / "games" / "sepulcra" / "assets", assets)
    args3 = ["--headless", "--assets", str(assets), "--captures", str(out / "captures3"), "--worlds", str(out / "worlds3"), "--saves", str(out / "saves3"),
             "--lead", "conductor", "--owner", "owner-proxy"]
    with Engine(exe, port, out / "engine3.log", *args3, cwd=str(ROOT)) as eng3:
        d = Drill(eng3)
        d.agent("conductor", "lead")
        d.agent("owner-proxy", "owner")

        def get(gid, **a):
            return d.raw(d.agent("conductor"), "gallery_get", dict(id=gid, **a))[1]

        def opened(gid, rounds):
            return wait_until(lambda: (lambda j: j if j.get("status") == "open" and j.get("rounds") == rounds else None)(get(gid)))

        try:
            # Back to back: 30 rounds, each answered (never no_answer).
            space = [{"name": "width", "target": "scale.x", "min": 0.75, "max": 1.4, "default": 1.0}]
            d.ok("conductor", "gallery_create", id="drop", title="Drop", subject={"recipe": "recipes/tombstone.json"}, space=space, variants=2)
            codes = {}
            for k in range(30):
                code, _ = d.raw(d.agent("conductor"), "gallery_round", {"id": "drop", "variants": [{"width": 0.8 + 0.01 * k}]})
                codes[code or "ok"] = codes.get(code or "ok", 0) + 1
            # stale_round is an answer (a render landed in between: read it again); no_answer or
            # reply_dropped is the dropped reply.
            answered = codes.get("ok", 0) + codes.get("stale_round", 0)
            d.check("30 gallery rounds back to back are all answered (no no_answer)", answered == 30 and codes.get("ok", 0) >= 20, json.dumps(codes))
            # gallery_apply: the accepted value and the gallery's record of it, one commit.
            moon = [{"name": "strength", "target": "light.key_intensity", "min": 0.3, "max": 1.0, "default": 0.55, "apply": {"file": "style.toml", "toml": "stage.game.key_intensity"}}]
            d.ok("conductor", "gallery_create", id="moon", title="Moon", subject={"recipe": "recipes/tombstone.json", "rig": "game"}, space=moon, variants=3)
            g = opened("moon", 1) or {}
            d.ok("owner-proxy", "gallery_decide", id="moon", round=1, decision="accept", variant=1)
            a = d.ok("conductor", "gallery_apply", id="moon")
            tc = d.ok("conductor", "trace_call", response_format="detailed")
            d.check("gallery_apply writes style.toml and the gallery in one commit", bool(a.get("files")) and len(tc.get("journal") or []) == 1, json.dumps({"apply": a, "journal": tc.get("journal")})[:300])
            # gallery_decide with apply_on_accept: the decision and its values, one commit.
            d.ok("conductor", "gallery_create", id="moon2", title="Moon 2", subject={"recipe": "recipes/tombstone.json", "rig": "game"}, space=moon, variants=3, apply_on_accept=True)
            opened("moon2", 1)
            dec = d.ok("owner-proxy", "gallery_decide", id="moon2", round=1, decision="accept", variant=2)
            tc = d.ok("owner-proxy", "trace_call", response_format="detailed")
            d.check("gallery_decide with apply_on_accept is one commit", bool(dec.get("applied")) and len(tc.get("journal") or []) == 1, json.dumps({"decide": dec, "journal": tc.get("journal")})[:300])
            # taste_suggest {apply}: three agreeing picks, applied by the owner (probe_suggest).
            space = [{"name": "width", "target": "scale.x", "min": 0.75, "max": 1.4, "default": 1.0}, {"name": "height", "target": "scale.y", "min": 0.75, "max": 1.4, "default": 1.0}]
            d.ok("conductor", "gallery_create", id="hs", title="HS", subject={"recipe": "recipes/tombstone.json", "rig": "game"}, space=space, variants=6)
            g = opened("hs", 1) or {}
            d.ok("owner-proxy", "gallery_decide", id="hs", round=1, decision="edit", variant=3, chips=["bigger"], note="taller")
            g = d.ok("conductor", "gallery_wait", id="hs", after=g.get("seq", 0), timeout_s=25)
            picked = get("hs", round=1)["round"]["variants"][3]["values"]
            w, h = picked["width"], picked["height"]
            for k, (dw, dh, final) in enumerate([(0.02, 0.0, False), (-0.02, 0.02, True)]):
                near = [{"width": w, "height": h}, {"width": round(w + dw, 4), "height": round(h + dh, 4)}, {"width": round(w + 0.2, 4), "height": h}, {"width": w, "height": round(max(h - 0.25, 0.75), 4)}]
                r = d.ok("conductor", "gallery_round", id="hs", variants=near)
                gg = wait_until(lambda: (lambda j: j if j.get("status") == "open" and j["round"].get("made") == "agent" and j.get("rounds") == r.get("round") else None)(get("hs"))) or {}
                d.ok("owner-proxy", "gallery_decide", id="hs", round=gg.get("rounds", 0), decision="accept" if final else "edit", variant=1)
                if not final:
                    d.ok("conductor", "gallery_wait", id="hs", after=gg.get("seq", 0), timeout_s=25)
            sug = d.ok("conductor", "taste_suggest", action="list").get("suggestions", [])
            d.check("three agreeing picks make a suggestion", bool(sug), json.dumps(sug)[:200])
            if sug:
                ap = d.ok("owner-proxy", "taste_suggest", action="apply", id=sug[0]["id"])
                tc = d.ok("owner-proxy", "trace_call", response_format="detailed")
                d.check("taste_suggest {apply} writes the prior and the log in one commit", ap.get("state") == "applied" and len(tc.get("journal") or []) == 1, json.dumps({"apply": ap, "journal": tc.get("journal")})[:300])
            # style_onboard {start}: the first step's gallery and the configurator state, one commit.
            st = d.ok("conductor", "style_onboard", action="start", open=False)
            tc = d.ok("conductor", "trace_call", response_format="detailed")
            d.check("style_onboard {start} is one commit", st.get("state") == "running" and len(tc.get("journal") or []) == 1, json.dumps({"start": st, "journal": tc.get("journal")})[:300])
            # The M4 final review: accepting a configurator step wrote the owner's decision, then
            # the next step's gallery and onboarding.json as `taste` (two entries, uncounted). Each
            # acceptance, the first and the last (which proposes the result) included, is one entry.
            entries = []
            for k in range(len(st.get("steps", []))):
                gid = d.ok("conductor", "style_onboard", action="status")["steps"][k].get("gallery")
                if not opened(gid, 1):
                    d.check(f"configurator step {k + 1}'s gallery opens", False, str(gid))
                    break
                dec = d.ok("owner-proxy", "gallery_decide", id=gid, round=1, decision="accept", variant=0)
                tc = d.ok("owner-proxy", "trace_call", response_format="detailed")
                entries.append(len(tc.get("journal") or []))
                if k == 0:
                    d.check("accepting configurator step 1 is one commit (the decision, step 2's gallery, onboarding.json)", entries[-1] == 1 and "next_step" in json.dumps(dec.get("onboarding")), json.dumps({"decide": dec, "journal": tc.get("journal")})[:300])
            last = d.ok("conductor", "style_onboard", action="status")
            d.check("accepting the last configurator step is one commit and proposes the result", len(entries) == len(st.get("steps", [])) and entries[-1:] == [1] and last.get("state") == "proposed",
                    json.dumps({"entries": entries, "state": last.get("state")}))
            d.ok("conductor", "style_onboard", action="cancel")
            swarm = d.ok("conductor", "engine_info", response_format="detailed").get("swarm", {})
            d.check("no tool call committed twice (multi_commit_calls 0)", swarm.get("multi_commit_calls") == 0, json.dumps({"multi_commit_calls": swarm.get("multi_commit_calls")}))
        except (RuntimeError, OSError, KeyError, TypeError) as e:
            d.check("the taste probes ran to the end (a debug engine panics on a second commit)", False, f"{type(e).__name__}: {str(e)[:300]}")
    d.check("the debug engine shut down cleanly after the taste probes", eng3.exit_code == 0, f"exit {eng3.exit_code}")
    return d.results


def mount_race(exe, port, out):
    """The M4 final review (round 3): four agents' project writes held while a sync wrote ran
    inside a fifth agent's project_mount (it waits for the sync), and were charged to the mount:
    `write.multi_commit` and, in a debug engine, a panic. Deferred work is charged to its own
    call: 40 rounds of four writes racing a mount make no multi-commit, and the engine lives."""
    print("held project writes pumped by a mount (charged to their own calls):")
    work = out / "mount-race"
    (work / "games").mkdir(parents=True)
    shutil.copytree(ROOT / "games" / "ambulo", work / "games" / "ambulo", ignore=shutil.ignore_patterns("target"))
    shutil.copytree(ROOT / "games" / "sepulcra" / "assets", work / "assets")
    args = ["--headless", "--assets", str(work / "assets"), "--captures", str(work / "captures"), "--worlds", str(work / "worlds")]
    with Engine(exe, port, work / "engine.log", *args, cwd=str(work)) as eng:
        d = Drill(eng)
        names = ["a", "b1", "b2", "b3", "c"]
        for n in names:
            d.agent(n)
        d.raw(d.agent("c"), "project_mount", {"module": "games/ambulo"})
        multi, rounds, errors = 0, 0, []
        for it in range(40):
            res = {}

            def go(n, tool, a):
                try:
                    res[n] = d.raw(d.agent(n), tool, a)
                except (RuntimeError, OSError) as ex:
                    res[n] = ("exception", str(ex)[:200])
            th = [threading.Thread(target=go, args=(n, "project_write", {"path": f"assets/style/race_{n}.md", "text": f"note {n} {it}"})) for n in names[:4]]
            th.append(threading.Thread(target=go, args=("c", "project_mount", {"module": "games/ambulo"})))
            for t in th:
                t.start()
            for t in th:
                t.join()
            # The scratch copy's mount is refused (not_a_module: its specula-sdk path dependency
            # isn't beside it), but only after it waited for the sync, which is the race under
            # test. Any other refusal (no_answer, reply_dropped, a dead engine) fails.
            errors += [f"{n}: {v[0]}" for n, v in res.items() if v[0] and not (n == "c" and v[0] == "not_a_module")]
            if eng.proc.poll() is not None:
                break
            rounds += 1
            multi = d.ok("a", "engine_info", response_format="detailed").get("swarm", {}).get("multi_commit_calls") or 0
            if multi:
                break
        d.check("40 rounds of four held writes racing a mount: no multi-commit, every call answered", rounds == 40 and multi == 0 and not errors, json.dumps({"rounds": rounds, "multi_commit_calls": multi, "errors": errors[:4]}))
    d.check("the engine shut down cleanly after the mount race", eng.exit_code == 0, f"exit {eng.exit_code}")
    return d.results


def recorded_results(exe, port, out):
    """M5: a review submission references results the engine recorded (style_check,
    studio_check, captures) by id and epoch; they list as engine results with their record,
    apart from what the submitter typed. Another agent's record is refused (foreign_result), as
    is one from another run or of a recipe changed since (stale_result) or none (result_not_found),
    and a forged `recorded` in typed checks or captures is refused as an unknown field."""
    print("review evidence the engine recorded:")
    work = out / "recorded"
    shutil.copytree(ROOT / "games" / "sepulcra" / "assets", work / "assets", ignore=shutil.ignore_patterns("taste"))
    args = ["--headless", "--assets", str(work / "assets"), "--captures", str(work / "captures"), "--worlds", str(work / "worlds")]
    with Engine(exe, port, work / "engine.log", *args, cwd=str(work)) as eng:
        d = Drill(eng)
        d.agent("ann")
        d.agent("bob")
        d.ok("ann", "world_spawn", name="ann_props", shape="group")
        d.ok("ann", "world_spawn", name="angel", shape="model", parent="/ann_props", recipe="recipes/statue_angel.json")
        style = d.ok("ann", "style_check", subjects=["recipes/statue_angel.json", "recipes/bone_pile.json"])
        rec = style.get("recorded") or {}
        d.check("style_check answers what it recorded {id, epoch}", isinstance(rec.get("id"), int) and isinstance(rec.get("epoch"), str), json.dumps(style)[:300])
        frame = d.ok("ann", "sight_frame", width=160, height=120, eye=[4, 3, 4], target=[0, 0, 0])
        cap = (frame.get("image") or {}).get("recorded") or {}
        d.check("a capture answers what it recorded (image.recorded)", isinstance(cap.get("id"), int) and cap.get("epoch") == rec.get("epoch"), json.dumps(frame.get("image"))[:300])
        refs = [{"id": rec.get("id"), "epoch": rec.get("epoch")}, {"id": cap.get("id"), "epoch": cap.get("epoch")}]
        sub = id_of(d.ok("ann", "review_submit", kind="asset", title="ann's angel", paths=["/ann_props"], results=refs,
                         checks=[{"kind": "style", "name": "my own look", "passed": True}]))
        full = d.ok("bob", "review_list", id=sub, response_format="detailed")
        checks = full.get("checks") or []
        recorded = [c for c in checks if c.get("recorded")]
        d.check("the referenced style_check is an engine check that names its record",
                len(recorded) == 1 and recorded[0].get("engine") is True and recorded[0]["recorded"].get("id") == rec.get("id") and recorded[0]["recorded"].get("tool") == "style_check",
                json.dumps(checks)[:400])
        typed = [c for c in checks if c.get("name") == "my own look"]
        d.check("what ann typed stays reported by ann, never the engine's", len(typed) == 1 and typed[0].get("reported_by") == "ann" and "engine" not in typed[0] and "recorded" not in typed[0], json.dumps(typed))
        caps = [c for c in (full.get("captures") or []) if c.get("recorded")]
        d.check("the referenced capture is in the evidence, marked recorded", len(caps) == 1 and caps[0]["recorded"].get("id") == cap.get("id"), json.dumps(full.get("captures"))[:400])
        listing = (d.ok("bob", "review_list").get("submissions") or [{}])[0].get("checks", {})
        d.check("review_list counts recorded results among the engine's", (listing.get("engine") or {}).get("recorded") == 1 and (listing.get("reported") or {}).get("passed") == 1, json.dumps(listing))
        # Refusals, each structured.
        d.expect("bob submits ann's recorded style_check as his", "bob", "review_submit", {"title": "bob's", "paths": ["/ann_props"], "results": refs[:1]}, ["foreign_result"])
        d.expect("bob submits ann's recorded capture as his", "bob", "review_submit", {"title": "bob's", "paths": ["/ann_props"], "results": refs[1:]}, ["foreign_result"])
        d.expect("a result from another engine run", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "results": [{"id": rec.get("id"), "epoch": "00000000deadbeef"}]}, ["stale_result"])
        d.expect("a result that doesn't exist", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "results": [{"id": 999999, "epoch": rec.get("epoch")}]}, ["result_not_found"])
        d.expect("a forged 'recorded' on a typed check", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "checks": [{"kind": "style", "name": "n", "passed": True, "recorded": {"tool": "style_check", "id": 1, "epoch": "x"}}]}, None)
        d.expect("a forged 'engine' on a typed check", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "checks": [{"kind": "style", "name": "n", "passed": True, "engine": True}]}, None)
        d.expect("a forged 'recorded' on a capture", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "captures": [{"path": frame["image"]["path"], "recorded": {"tool": "sight", "id": 1, "epoch": "x"}}]}, None)
        d.expect("the same result twice", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "results": refs[:1] + refs[:1]}, None)
        # A recipe changed after its check: the record no longer describes it.
        recipe = work / "assets" / "recipes" / "bone_pile.json"
        before = recipe.read_text(encoding="utf-8")
        time.sleep(1.1)  # the library notices a change by modification time
        recipe.write_text(before.replace('"seed": 3118', '"seed": 3119'), encoding="utf-8")
        d.expect("a style_check of a recipe changed since", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "results": refs[:1]}, ["stale_result"])
        again = d.ok("ann", "style_check", subjects=["recipes/statue_angel.json", "recipes/bone_pile.json"]).get("recorded") or {}
        fresh = d.raw(d.agent("ann"), "review_submit", {"title": "ann's angel again", "paths": ["/ann_props"], "results": [again], "supersedes": sub})
        d.check("checked again, the new record is accepted", fresh[0] is None, json.dumps(fresh[1])[:300])
        sub2 = id_of(fresh[1]) if fresh[0] is None else None

        # The lane review's probes (2026-11-06), kept as permanent probes.
        # (1) The rules judged changed after the check: style.toml loosened, checked, restored.
        toml = work / "assets" / "style.toml"
        strict = toml.read_text(encoding="utf-8")
        time.sleep(1.1)
        toml.write_text(strict + '\n[[budget]]\nname = "statue_angel"\nmax_triangles = 10\n', encoding="utf-8")
        tight = d.ok("ann", "style_check", subjects=["recipes/statue_angel.json"])
        d.check("a tightened style.toml fails the angel", tight.get("passed") is False, json.dumps(tight)[:200])
        time.sleep(1.1)
        toml.write_text(strict, encoding="utf-8")
        d.expect("a style_check made under other style.toml rules", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "results": [tight.get("recorded")]}, ["stale_result"])
        loose = d.ok("ann", "style_check", subjects=["recipes/statue_angel.json"])
        time.sleep(1.1)
        toml.write_text(strict + '\n[[budget]]\nname = "statue_angel"\nmax_triangles = 10\n', encoding="utf-8")
        d.expect("a pass recorded before style.toml was tightened", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "results": [loose.get("recorded")]}, ["stale_result"])
        time.sleep(1.1)
        toml.write_text(strict, encoding="utf-8")
        # (2) An unrelated record: a pass of the bone pile backs a submission of the angel.
        bones = d.ok("ann", "style_check", subjects=["recipes/bone_pile.json"])
        d.expect("an engine pass of another recipe backs the submission", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "results": [bones.get("recorded")]}, ["unrelated_result"])
        # ... and a check of part of the work says which models it didn't judge, by name.
        d.ok("ann", "world_spawn", name="mourner", shape="model", parent="/ann_props", recipe="recipes/statue_mourner.json")
        part = d.ok("ann", "style_check", subjects=["recipes/statue_angel.json", "recipes/bone_pile.json"])
        p = d.raw(d.agent("ann"), "review_submit", {"title": "angel and mourner", "paths": ["/ann_props"], "results": [part.get("recorded")], **({"supersedes": sub2} if sub2 else {})})
        cs = (d.ok("bob", "review_list", id=id_of(p[1]), response_format="detailed").get("checks") or []) if p[0] is None else []
        rc = [c for c in cs if (c.get("recorded") or {}).get("id") == (part.get("recorded") or {}).get("id")]
        subjects = [s.get("spec") for s in ((rc[0].get("recorded") or {}).get("subjects") or [])] if rc else []
        d.check("the recorded check names every subject it judged", subjects == ["recipes/statue_angel.json", "recipes/bone_pile.json"] and "statue_angel" in (rc[0].get("detail") or ""), json.dumps(rc)[:400])
        gap = [c for c in cs if c.get("name") == "models_engine_checked"]
        d.check("the model no recorded check judged is named, as an engine check that fails", len(gap) == 1 and gap[0].get("passed") is False and "statue_mourner" in (gap[0].get("detail") or ""), json.dumps(gap)[:300])
        # (3) The packet changed after the capture.
        f2 = d.ok("ann", "sight_frame", width=160, height=120, eye=[4, 3, 4], target=[0, 0, 0], response_format="detailed")
        pk = Path(f2["image"].get("packet_path") or Path(f2["image"]["path"]).with_suffix(".json"))
        j = json.loads(pk.read_text())
        j["FORGED"] = "edited"
        for s in j.get("subjects", []):
            s["visible_px"] = 999999
        pk.write_text(json.dumps(j))
        d.expect("a capture whose packet was edited after it was written", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "results": [f2["image"].get("recorded")]}, ["stale_result"])
        # (4) A recorded capture named only by path is evidence, never marked recorded.
        f3 = d.ok("ann", "sight_frame", width=160, height=120, eye=[4, 3, 4], target=[0, 0, 0])
        plain = d.raw(d.agent("ann"), "review_submit", {"title": "plain", "paths": ["/ann_props"], "captures": [{"path": f3["image"]["path"]}]})
        pc = d.ok("bob", "review_list", id=id_of(plain[1]), response_format="detailed").get("captures") if plain[0] is None else None
        d.check("a capture named by path isn't marked recorded", pc is not None and not any(c.get("recorded") for c in pc), json.dumps(pc)[:300])
        # (5) A forged PNG with the recorded capture's evidence name (that name is an unkeyed,
        # invertible hash; the record vouches by SHA-256).
        f4 = d.ok("ann", "sight_frame", width=160, height=120, eye=[4, 3, 4], target=[0, 0, 0])
        real = Path(f4["image"]["path"])
        rb = real.read_bytes()
        fake = forge_same_name(rb, solid_png(64, 48, (255, 0, 255)))
        d.check("the forged PNG has the real one's evidence name", evidence_name(fake) == evidence_name(rb) and fake != rb)
        real.write_bytes(fake)
        d.expect("a forged PNG with the recorded capture's name", "ann", "review_submit", {"title": "x", "paths": ["/ann_props"], "results": [f4["image"].get("recorded")]}, ["stale_result"])
    d.check("the recorded-results engine exited cleanly", eng.exit_code == 0, f"exit {eng.exit_code}")
    return d.results


# The evidence folder's content name (crate::evidence::content_name, StableHasher), ported to forge
# a PNG with another's name: the lane review's probe that the name proves nothing.
_M = (1 << 64) - 1
_K = 0x9e3779b97f4a7c15
_C = 0x5851f42d4c957f2d
_CINV = pow(_C, -1, 1 << 64)
_SEED = 0x6576_6964_656e_6365


def _rotl(x, r):
    return ((x << r) | (x >> (64 - r))) & _M


def _rotr(x, r):
    return ((x >> r) | (x << (64 - r))) & _M


def _words(b):
    import struct
    out = [len(b) ^ 0xb5_0000_0000_0000]
    for i in range(0, len(b) - len(b) % 8, 8):
        out.append(struct.unpack("<Q", b[i:i + 8])[0])
    rest = b[len(b) - len(b) % 8:]
    if rest:
        out.append(struct.unpack("<Q", rest + b"\0" * (8 - len(rest)))[0])
    return out


def _start():
    return (0x243f6a8885a308d3 ^ ((_SEED * _K) & _M)) & _M


def _mix(s, w):
    return (((_rotl(s, 23) ^ w) * _C) + _K) & _M


def _unmix(s_after, w):
    return _rotr((((s_after - _K) & _M) * _CINV & _M) ^ w, 23)


def _state(b):
    s = _start()
    for w in _words(b):
        s = _mix(s, w)
    return s, len(_words(b))


def evidence_name(b):
    s, n = _state(b)
    z = s ^ ((n * _K) & _M)
    z = ((z ^ (z >> 30)) * 0xbf58476d1ce4e5b9) & _M
    z = ((z ^ (z >> 27)) * 0x94d049bb133111eb) & _M
    return f"{z ^ (z >> 31):016x}-{len(b)}.png"


def solid_png(w, h, rgb):
    import struct
    import zlib
    raw = b"".join(b"\0" + bytes(rgb) * w for _ in range(h))

    def chunk(k, data):
        return struct.pack(">I", len(data)) + k + data + struct.pack(">I", zlib.crc32(k + data) & 0xffffffff)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def forge_same_name(target, fake):
    """`fake`, padded after IEND with one solved word and zeros, to `target`'s length and name."""
    import struct
    length = len(target)
    target_state, _ = _state(target)
    body = fake + b"\0" * ((len(fake) + 7) // 8 * 8 - len(fake))
    tail = b"\0" * (length - len(body) - 8)
    ws = _words(body + b"\0" * 8 + tail)
    nb = len(body) // 8
    s = _start()
    for w in ws[:1 + nb]:
        s = _mix(s, w)
    t = target_state
    for w in reversed(ws[1 + nb + 1:]):
        t = _unmix(t, w)
    free = (((t - _K) & _M) * _CINV & _M) ^ _rotl(s, 23)
    return body + struct.pack("<Q", free) + tail


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe")
    ap.add_argument("--port", type=int, default=7760)
    ap.add_argument("--out", default=str(ROOT / "target" / "authority-drill"))
    a = ap.parse_args()
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    exe = Path(a.exe) if a.exe else target / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")
    out = Path(a.out)
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    args = ["--headless", "--captures", str(out / "captures"), "--worlds", str(out / "worlds"), "--saves", str(out / "saves"),
            "--lead", "lee", "--reviewer", "rev", "--world", "ownerworld"]
    with Engine(exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as eng:
        d = Drill(eng)
        print("roles and names:")
        carl = eng.mcp("drill-carl")
        d.expect("carl says role owner", "carl", "agent_hello", {"name": "carl", "role": "owner"}, ["role_not_granted"], m=carl)
        d.expect("dave says role lead (the operator named lee)", "dave", "agent_hello", {"name": "dave", "role": "lead"}, ["role_not_granted"], m=eng.mcp("drill-dave"))
        d.expect("bob2 says role reviewer", "bob2", "agent_hello", {"name": "bob2", "role": "reviewer"}, ["role_not_granted"], m=eng.mcp("drill-bob2"))
        for n in ["physics", "nav", "disk", "engine:x", "lex:waves", "module:x", "window:owner",
                  # Look-alikes (review of the authority lane): case, Cyrillic letters, full width.
                  "Physics", "DISK", "Window:owner", "phys\u0456cs", "\u0440hysics", "\uff50hysics",
                  # Role words, ungranted.
                  "owner", "Lead", "reviewer", "director"]:
            d.expect(f"an agent takes the name {n!a}", "x", "agent_hello", {"name": n, "role": "builder"}, ["name_reserved", "name_invalid"], m=eng.mcp("drill-x"))
        for n in ["physics\u200b", "disk\u2060", "x\u202ey", "bo\ufeffb", "module\u2236x", "engine\uff1ax", "na\u0301v", "\u217eisk"]:
            d.expect(f"an agent takes the name {n!a} (outside ASCII letters, digits, space _ . -)", "x", "agent_hello", {"name": n, "role": "builder"}, ["name_invalid"], m=eng.mcp("drill-x"))
        d.expect("casey says role 'Lead'", "casey", "agent_hello", {"name": "casey", "role": "Lead"}, ["role_not_granted"], m=eng.mcp("drill-casey"))
        lee = d.agent("lee", "lead")
        rev = d.agent("rev", "reviewer")
        d.expect("another session takes rev's granted name while rev is live", "rev", "agent_hello", {"name": "rev", "role": "builder"}, ["name_taken"], m=eng.mcp("drill-rev2"))
        # rev's session takes another name: "rev" is free of live sessions, but bound to rev's
        # identity for the run.
        d.ok("rev", "agent_hello", name="rev-elsewhere", role="builder")
        d.expect("another session takes rev's granted name after rev moved", "rev", "agent_hello", {"name": "rev", "role": "reviewer"}, ["grant_bound"], m=eng.mcp("drill-rev3"))
        d.expect("... or a look-alike of it", "rev", "agent_hello", {"name": "Rev", "role": "builder"}, ["grant_bound"], m=eng.mcp("drill-rev4"))
        d.ok("rev", "agent_hello", name="rev", role="reviewer")
        info = d.ok("lee", "engine_info", response_format="detailed")
        table = info.get("access", {}).get("tools", {})
        d.check("engine_info lists the access table", len(table) > 100, f"{len(table)} tools")
        d.check("the grants are listed", info.get("swarm", {}).get("grants", {}).get("operator", {}).get("lee") is not None)
        _ = (lee, rev)

        print("ann makes things:")
        d.agent("ann")
        d.agent("bob")
        box_seq = d.ok("ann", "world_spawn", name="ann_box", shape="cube", position=[0, 0.5, 0])["journal"]
        claim = id_of(d.ok("ann", "claim_take", path="/ann_box"))
        task = id_of(d.ok("ann", "task_post", title="ann's task", assign="ann"))
        note = id_of(d.ok("ann", "note_add", text="ann's note", path="/ann_box"))
        intent = id_of(d.ok("ann", "intent_declare", paths=["/ann_box"], text="painting it"))
        lease = id_of(d.ok("ann", "lease_take", kind="scratch"))
        d.ok("ann", "camera_set", name="anncam", eye=[4, 3, 4], target=[0, 0, 0])
        frame = d.ok("ann", "sight_frame", width=160, height=120, eye=[4, 3, 4], target=[0, 0, 0])
        capture = frame.get("image", {}).get("path")
        sub = id_of(d.ok("ann", "review_submit", kind="asset", title="ann's box", paths=["/ann_box"], captures=[{"path": capture, "kind": "frame"}],
                   checks=[{"kind": "style", "name": "palette", "passed": True}]))
        crit = d.ok("ann", "critique_add", items=[{"text": "the box is plain", "severity": "note", "submission": sub}])["added"][0]
        d.ok("ann", "save_write", name="ann_save")

        # The window's editor acts as the human (window:owner): only the owner drives it
        # (docs/design/EDITOR.md); a lead or reviewer is refused like any agent.
        for who in ("bob", "lee", "rev"):
            d.expect(f"{who} drives the window's editor", who, "editor_input", {"steps": [{"key": "F7"}]}, ["owner_only"])

        print("bob acts on ann's things:")
        # World tools on ann's claimed box.
        d.expect("bob sets ann's claimed box", "bob", "world_set", {"path": "/ann_box", "color": "#ff0000"}, ["claimed"])
        d.expect("bob deletes ann's claimed box", "bob", "world_delete", {"path": "/ann_box"}, ["claimed"])
        # Claims.
        d.expect("bob takes ann's claim's path", "bob", "claim_take", {"path": "/ann_box"}, None)
        d.expect("bob renews ann's claim", "bob", "claim_renew", {"id": claim}, None)
        d.expect("bob shares ann's claim", "bob", "claim_share", {"id": claim, "with": "bob"}, None)
        d.expect("bob releases ann's claim", "bob", "claim_release", {"id": claim}, None)
        # The board.
        d.expect("bob edits ann's task", "bob", "task_update", {"id": task, "priority": 1}, None)
        d.expect("bob finishes ann's task", "bob", "task_done", {"id": task, "summary": "done by bob"}, None)
        d.expect("bob takes ann's assigned task", "bob", "task_take", {"id": task}, None)
        # Notes, intents, leases.
        d.expect("bob deletes ann's note", "bob", "note_delete", {"id": note}, ["not_yours"])
        d.expect("bob forces the deletion of ann's note", "bob", "note_delete", {"id": note, "force": True}, ["force_needs_lead"])
        d.expect("bob clears ann's intent", "bob", "intent_clear", {"id": intent}, ["not_yours"])
        d.expect("bob renews ann's lease", "bob", "lease_renew", {"id": lease}, None)
        d.expect("bob releases ann's lease", "bob", "lease_release", {"id": lease}, None)
        # Cameras are per agent: bob's names never reach ann's.
        d.expect("bob deletes ann's camera by name", "bob", "camera_delete", {"name": "anncam"}, None)
        cams = d.ok("bob", "camera_list", of="ann")
        d.check("ann's camera is still there", "anncam" in json.dumps(cams))
        # The review queue and critiques.
        d.expect("bob approves ann's submission", "bob", "review_decide", {"ids": [sub], "verdict": "approve"}, ["not_reviewer"])
        d.expect("bob withdraws ann's submission", "bob", "review_decide", {"ids": [sub], "verdict": "withdraw"}, ["not_yours", "not_reviewer"])
        d.expect("ann approves her own submission", "ann", "review_decide", {"ids": [sub], "verdict": "approve"}, ["not_reviewer", "self_review"])
        d.expect("bob puts ann's work on the owner's screen", "bob", "review_screen", {"action": "open", "id": sub}, ["not_reviewer"])
        d.expect("bob dismisses ann's critique", "bob", "critique_update", {"id": crit, "action": "dismiss", "note": "no"}, ["not_yours"])
        for src in ["owner", "Owner", "window:owner", "LEAD"]:
            d.expect(f"bob adds a critique claiming source {src!a}", "bob", "critique_add", {"items": [{"text": "x", "severity": "note", "source": src}]}, ["source_reserved"])
        # Saves.
        d.expect("bob replaces ann's save", "bob", "save_write", {"name": "ann_save", "overwrite": True}, None)
        d.expect("bob deletes ann's save", "bob", "save_delete", {"name": "ann_save"}, None)
        d.expect("bob forces the deletion of ann's save", "bob", "save_delete", {"name": "ann_save", "force": True}, ["force_needs_lead"])
        d.expect("bob forces a save over ann's", "bob", "save_write", {"name": "ann_save", "overwrite": True, "force": True}, ["force_needs_lead"])
        d.expect("bob forces an undo of ann's entries", "bob", "world_undo", {"ids": [box_seq], "force": True}, ["force_needs_lead"])
        # The world: a checkpoint under its name is anyone's; renaming it is the lead's.
        d.expect("bob renames the owner's world (world_save under another name)", "bob", "world_save", {"name": "bobs_world"}, ["lead_only"])
        d.expect("bob saves over another world", "bob", "world_save", {"name": "other", "overwrite": True}, ["lead_only"])
        d.check("bob saves the world under its own name", d.raw(d.agent("bob"), "world_save", {})[0] is None)
        name_now = d.ok("lee", "engine_info", response_format="detailed").get("world", {}).get("name")
        d.check("the world keeps its name", name_now == "ownerworld", str(name_now))
        # Settings.
        d.expect("bob sets the frame floor", "bob", "swarm_settings", {"frame_floor_fps": 30}, ["lead_only"])
        d.expect("bob changes streaming budgets", "bob", "stream_settings", {"upload_ms": 2}, ["lead_only"])
        d.expect("bob sets ann's tools", "bob", "tools_scope", {"agent": "ann", "tools": ["world"]}, ["lead_only"])
        d.expect("bob prunes past the disk budget", "bob", "disk_prune", {"free_mb": 1}, ["lead_only"])
        lead_ok = d.raw(d.agent("lee"), "swarm_settings", {"frame_floor_fps": 30})[0] is None
        d.check("the granted lead sets the frame floor", lead_ok)
        # The human's keyboard and clipboard (review of m5-text, major 2): a builder can't point
        # them at a text (here one hidden under the world, which another agent could read).
        d.ok("bob", "world_spawn", name="bob_field", shape="text", position=[0, -500, 0], visible=False, text={"text": "x"})
        d.expect("bob points the human's keyboard at his hidden text", "bob", "text_edit", {"path": "/bob_field", "ops": [], "window_focus": True}, ["lead_only"])
        d.expect("bob stops the human's typing", "bob", "text_edit", {"path": "/bob_field", "ops": [], "window_focus": False}, ["lead_only"])
        d.expect("the lead points it at a hidden text", "lee", "text_edit", {"path": "/bob_field", "ops": [], "window_focus": True}, ["not_visible"])
        d.ok("bob", "world_set", path="/bob_field", visible=True)
        lead_focus = d.raw(d.agent("lee"), "text_edit", {"path": "/bob_field", "ops": [], "window_focus": True})
        d.check("the granted lead points the keyboard at a shown text", lead_focus[0] is None and (lead_focus[1] or {}).get("window_focus") is True, json.dumps(lead_focus[1])[:200])
        d.raw(d.agent("lee"), "text_edit", {"path": "/bob_field", "ops": [], "window_focus": False})
        # Jobs: ann's perf capture records; bob can't replace it or switch the profiler under it.
        done = {}
        t = threading.Thread(target=lambda: done.setdefault("ann", d.raw(d.agent("ann"), "perf_capture", {"frames": 240})))
        t.start()
        time.sleep(0.5)
        d.expect("bob starts a perf capture during ann's", "bob", "perf_capture", {"frames": 10}, ["job_busy"])
        d.expect("bob switches the profiler off during ann's capture", "bob", "perf_zones", {"enabled": False}, ["capture_in_progress"])
        t.join(timeout=60)
        d.check("ann's capture answered (not replaced)", done.get("ann", ("x",))[0] is None, json.dumps(done.get("ann"))[:200])
        # Playtests: only the starter cancels.
        pt = d.raw(d.agent("ann"), "playtest_run", {"spec": {"name": "slow", "steps": [{"at": 0, "call": "world_spawn", "args": {"name": "pt_box", "shape": "cube"}}], "ticks": 200000}, "wait_s": 0})
        pt_id = (pt[1] or {}).get("id")
        if pt_id is not None:
            d.expect("bob cancels ann's playtest", "bob", "playtest_report", {"id": pt_id, "cancel": True}, None)
            d.expect("bob forces the cancel of ann's playtest", "bob", "playtest_report", {"id": pt_id, "cancel": True, "force": True}, ["force_needs_lead"])
            d.raw(d.agent("ann"), "playtest_report", {"id": pt_id, "cancel": True})
        else:
            d.check("ann started a playtest", False, json.dumps(pt)[:300])
        d.probed.add("playtest_run")
        # Galleries (docs/design/GALLERIES.md): the maker opens, closes, replaces a round of and
        # deletes its own; the owner, a lead or a reviewer any; bob none of ann's.
        space = [{"name": "s", "target": "light.key_intensity", "min": 0.3, "max": 1.0}]
        d.ok("ann", "gallery_create", id="anns", subject={"stage": {}}, space=space, wait=False)
        d.expect("bob puts ann's gallery on the owner's screen", "bob", "gallery_open", {"id": "anns"}, ["not_allowed"])
        d.check("ann puts her own gallery on the screen", d.raw(d.agent("ann"), "gallery_open", {"id": "anns"})[0] is None)
        d.expect("bob closes the screen showing ann's gallery", "bob", "gallery_close", {}, ["not_allowed"])
        rev_close = d.raw(d.agent("rev"), "gallery_close", {})
        d.check("the granted reviewer closes it", rev_close[0] is None, json.dumps(rev_close[1])[:200])
        d.expect("bob replaces ann's gallery round", "bob", "gallery_round", {"id": "anns", "variants": [{"s": 0.3}, {"s": 0.9}]}, ["not_allowed"])
        d.expect("bob deletes ann's gallery", "bob", "gallery_delete", {"id": "anns"}, ["not_allowed"])
        ann_del = d.raw(d.agent("ann"), "gallery_delete", {"id": "anns"})
        d.check("ann deletes her own gallery", ann_del[0] is None and (ann_del[1] or {}).get("deleted") is True, json.dumps(ann_del[1])[:200])

        lead_note = d.raw(d.agent("lee"), "note_delete", {"id": note, "force": True})
        d.check("the lead may force the deletion of ann's note", lead_note[0] is None, json.dumps(lead_note[1])[:200])
        print("truthful answers:")
        d.ok("ann", "world_spawn", name="behind", shape="cube", position=[0, 0.5, 30])
        q = d.ok("ann", "sight_query", path="/behind", eye=[0, 1, 10], target=[0, 0, 0])
        d.check("an entity behind the camera is not drawn", q.get("drawn") is False, json.dumps(q)[:300])
        cl = d.ok("bob", "critique_list")
        item = (cl.get("critiques") or [{}])[0]
        d.check("the concise critique listing names its author", item.get("by") == "ann", json.dumps(item)[:200])
        rl = d.ok("bob", "review_list")
        checks = (rl.get("submissions") or [{}])[0].get("checks", {})
        d.check("review_list counts reported checks apart from the engine's", checks.get("reported", {}).get("passed") == 1 and checks.get("reported", {}).get("by") == "ann", json.dumps(checks))
        tc = d.ok("bob", "time_control", action="pause")
        d.check("time_control says who moved the clock", tc.get("set_by", {}).get("agent") == "bob", json.dumps(tc))

        # Timelines: a playback is its starter's; another agent neither pauses nor stops it, not
        # even with force (that needs the lead).
        d.ok("ann", "world_spawn", name="tl_door", shape="cube", position=[40, 0.5, 0])
        tl = d.ok("ann", "timeline_set", path="drill", timeline={"schema": "specula.timeline.v1", "duration": 30, "tracks": [
            {"kind": "entity", "name": "door", "target": "/tl_door", "position": [{"t": 0, "value": [40, 0.5, 0]}, {"t": 30, "value": [40, 3, 0]}]}]})
        d.ok("ann", "timeline_play", path="drill")
        d.expect("bob pauses ann's playback", "bob", "timeline_pause", {"path": "drill"}, ["job_busy"])
        d.expect("bob stops ann's playback", "bob", "timeline_stop", {"path": "drill"}, ["job_busy"])
        d.expect("bob stops ann's playback with force", "bob", "timeline_stop", {"path": "drill", "force": True}, None)
        d.ok("ann", "timeline_stop", path="drill")
        # This engine's project is the repository (no --assets): the timeline goes again.
        d.ok("ann", "project_delete", path=tl["path"])

        # Every owned tool has a probe, or a reason it is covered elsewhere.
        owned = sorted(n for n, v in table.items() if isinstance(v, str) and v.startswith("owned:"))
        creates = {"task_post", "task_plan", "note_add", "intent_declare", "lease_take", "camera_set", "review_submit", "critique_add", "tools_request"}
        for n in owned:
            if n in d.probed:
                continue
            if n in COVERED_ELSEWHERE:
                d.results.append({"probe": f"{n}: covered elsewhere", "held": True, "detail": COVERED_ELSEWHERE[n]})
            elif n in creates:
                d.results.append({"probe": f"{n}: creates the caller's own item", "held": True})
            else:
                d.check(f"a probe for owned tool {n}", False, "add one to scripts/authority_drill.py")
        multi = d.ok("lee", "engine_info", response_format="detailed").get("swarm", {})
        # One tool call, one commit (AGENTS rule 14): a second commit is counted (and panics a
        # debug engine).
        d.check("no tool call committed twice (multi_commit_calls 0)", multi.get("multi_commit_calls") == 0, json.dumps({"multi_commit_calls": multi.get("multi_commit_calls")}))
    # An unnamed engine over the same worlds folder: overwriting a world an earlier run saved
    # ("ownerworld") is the lead's call (review round 2); a new folder is anyone's.
    print("an unnamed engine over earlier runs' worlds:")
    args2 = ["--headless", "--captures", str(out / "captures2"), "--worlds", str(out / "worlds"), "--saves", str(out / "saves2"), "--lead", "lee"]
    with Engine(exe, a.port + 1, out / "engine2.log", *args2, cwd=str(ROOT)) as eng2:
        d2 = Drill(eng2)
        d2.agent("bob")
        d2.agent("lee", "lead")
        d2.ok("bob", "world_spawn", name="bobs_box", shape="cube")
        d2.expect("bob overwrites the earlier run's ownerworld", "bob", "world_save", {"name": "ownerworld", "overwrite": True}, ["lead_only"])
        # Force discarding only the caller's own (and the simulation's) unsaved changes is anyone's.
        d2.check("bob loads over his own unsaved changes with force", d2.raw(d2.agent("bob"), "world_load", {"name": "ownerworld", "force": True})[0] is None)
        d2.agent("ann")
        d2.ok("ann", "world_spawn", name="anns_box", shape="cube")
        d2.expect("bob loads over ann's unsaved changes with force", "bob", "world_load", {"name": "ownerworld", "force": True}, ["force_needs_lead"])
        lead_save = d2.raw(d2.agent("lee"), "world_save", {"name": "ownerworld", "overwrite": True})
        d2.check("the lead may overwrite an earlier run's world", lead_save[0] is None, json.dumps(lead_save[1])[:200])
        d.results.extend(d2.results)
    d.results.extend(one_call_one_commit(exe, a.port + 2, out))
    d.results.extend(mount_race(exe, a.port + 3, out))
    d.results.extend(recorded_results(exe, a.port + 4, out))
    d.results.extend(window_click(exe, a.port + 5, out))
    held = sum(1 for r in d.results if r["held"])
    report = {"probes": len(d.results), "held": held, "results": d.results, "owned_tools": owned, "engine_exit": eng.exit_code}
    (out / "report.json").write_text(json.dumps(report, indent=1))
    print(f"authority drill: {held}/{len(d.results)} held; engine exit {eng.exit_code}; multi_commit_calls {multi.get('multi_commit_calls')}")
    print(f"report: {out / 'report.json'}")
    return 0 if held == len(d.results) and eng.exit_code == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
