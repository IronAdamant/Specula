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
    held = sum(1 for r in d.results if r["held"])
    report = {"probes": len(d.results), "held": held, "results": d.results, "owned_tools": owned, "engine_exit": eng.exit_code}
    (out / "report.json").write_text(json.dumps(report, indent=1))
    print(f"authority drill: {held}/{len(d.results)} held; engine exit {eng.exit_code}; multi_commit_calls {multi.get('multi_commit_calls')}")
    print(f"report: {out / 'report.json'}")
    return 0 if held == len(d.results) and eng.exit_code == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
