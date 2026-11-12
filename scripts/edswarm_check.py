#!/usr/bin/env python3
"""The editor's swarm panels and "highlight and ask", end to end in the real window
(docs/design/EDITOR.md, "Swarm panels" and "Highlight and ask"; ROADMAP M5 swarm editor items).

Sepulcra runs in the window on the integrated GPU. The human's hands are the OS input path (keys,
characters and clicks posted to the window's own message queue, as editor_check does); agents use
the protocol. It checks, failing loudly:

- highlight and ask: the human selects a statue, opens "Ask the agents...", picks its "head" part
  (Tab), types a request and sends it: one board task posted by window:owner with the statue by
  identity and a recipe_part descriptor {name: "head"};
- the box is the human's alone: while it is open the owner's agent is refused editor_input and
  ui_act on the editor (window_held), and neither editor_state nor ui_tree {surface: "editor"}
  contains the words typed;
- an agent (sculptor) takes the task, claims the statue, edits it (world_set params) and submits it
  to the review queue with the task;
- the human opens it from the Swarm tab's Review view and approves it in the review screen: the
  submission is approved by window:owner;
- the journal timeline (driven by the owner's agent through ui_act on the editor) names the
  sculptor on the statue's entries, with the task as their badge, and highlights the statue;
- the claim map draws the sculptor's claim with its name; the Claims view lists it;
- the cost dashboard counts the sculptor's calls and commits;
- the window's frame time with the panels and the claim map on stays inside perf.example.toml's
  frame budgets; zero validation errors at shutdown.

Screenshots of the real window go to the output folder (sight-skeptic checks them).

    cargo build -p sepulcra-app -p specula --release
    python scripts/edswarm_check.py [--adapter integrated] [--port 7600] [--out DIR] [--exe PATH]

Keep output ASCII.
"""
import argparse
import json
import shutil
import sys
import time
from pathlib import Path

import editor_check as ec
from editor_check import call, raw, wait_for, find_window, park_cursor, os_key, os_click, screen_centre, Editor
from specula_mcp import ROOT, Engine
from sepulcra_play import EXE
from m2_exit import PERF_TOML, is_frame_metric, other_engines

check = ec.check

STATUE = {
    "recipe": 1, "name": "edswarm_statue", "category": "prop", "seed": 5,
    "description": "edswarm_check: a plinth, a body and a head (head_size scales the head).",
    "params": {"head_size": {"default": 1, "min": 0.5, "max": 2.5}},
    "collider": "aabb",
    "parts": [
        {"name": "plinth", "material": "graveyard_stone", "shape": {"op": "box", "size": [0.9, 0.4, 0.9], "bevel": 0.03}},
        {"name": "body", "material": "graveyard_stone", "shape": {"op": "cylinder", "radius": 0.24, "height": 1.0, "sides": 10, "bevel": 0.02},
         "ops": [{"op": "move", "by": [0, 0.4, 0]}]},
        {"name": "head", "material": "graveyard_stone", "shape": {"op": "sphere", "radius": "0.2 * head_size", "subdivisions": 1},
         "ops": [{"op": "move", "by": [0, 1.36, 0]}]},
    ],
}
WORDS = "Make the head bigger and rounder"
RECIPE = "assets/recipes/edswarm_statue.json"
LEFTOVER = ROOT / "games" / "sepulcra" / RECIPE


def os_chars(hwnd, text):
    """Characters as the keyboard sends them (WM_CHAR), one by one."""
    for ch in text:
        ec.user32.PostMessageW(hwnd, 0x0102, ord(ch), 1)
        time.sleep(0.03)
    time.sleep(0.2)


def os_key_with_shift(hwnd, vk):
    ec.user32.PostMessageW(hwnd, 0x0100, 0x10, 1)
    time.sleep(0.04)
    os_key(hwnd, vk)
    ec.user32.PostMessageW(hwnd, 0x0101, 0x10, 1 | (0xC0 << 24))
    time.sleep(0.1)


def centre(r):
    return [r[0] + r[2] / 2, r[1] + r[3] / 2]


def ui_click(owner, widget):
    """The owner's agent presses an editor widget through ui_act (its edits would be window:owner's)."""
    err, j = raw(owner, "ui_act", surface="editor", steps=[{"click": widget}, {"wait": 3}])
    return err, j


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--exe", default=str(EXE))
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--port", type=int, default=7600)
    ap.add_argument("--frames", type=int, default=240)
    ap.add_argument("--bulk", type=int, default=100_000, help="changes journaled for the timeline's scan-budget check (0 skips it)")
    ap.add_argument("--out")
    a = ap.parse_args()
    if sys.platform != "win32":
        print("skip: the edswarm check drives the real window (Windows only for now)")
        return 0
    exe = Path(a.exe)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p sepulcra-app --release")
    out = Path(a.out) if a.out else ROOT / "target" / "edswarm-check" / a.adapter
    if out.exists():
        shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True, exist_ok=True)
    if LEFTOVER.exists():
        LEFTOVER.unlink()  # an earlier run that stopped halfway
    args = ["--adapter", a.adapter, "--size", "1280x800", "--perf", str(PERF_TOML), "--captures", str(out / "captures"),
            "--worlds", str(out / "worlds"), "--saves", str(out / "saves"), "--owner", "owner-proxy", "--reviewer", "rev-1", "--lead", "lead-1", "--", "--seed", "7", "--mute", "--intro", "off"]
    report = {"adapter": a.adapter}
    with Engine(exe, a.port, out / "engine.log", *args, cwd=str(out)) as eng:
        owner, sculptor, other, rev, lead = eng.mcp("owner"), eng.mcp("sculptor"), eng.mcp("other"), eng.mcp("rev"), eng.mcp("lead")
        for m, name, role in [(owner, "owner-proxy", "owner"), (sculptor, "sculptor", "builder"), (other, "other", "builder"), (rev, "rev-1", "reviewer"), (lead, "lead-1", "lead")]:
            for _ in range(120):
                err, _j = raw(m, "agent_hello", name=name, role=role)
                if not err:
                    break
                time.sleep(1.0)
            else:
                sys.exit(f"agent_hello {name} never succeeded: {err}")
        info = call(owner, "engine_info", response_format="detailed")
        report["gpu"] = (info.get("gpu") or {}).get("name")
        print(f"GPU: {report['gpu']}")
        hwnd = wait_for(lambda: find_window(eng.proc.pid), 30)
        if not check(bool(hwnd), "the engine's window is up"):
            return 1
        park_cursor(hwnd)
        call(owner, "time_control", action="pause")

        # -- a statue with a head, in front of the game's camera --
        call(sculptor, "project_write", path=RECIPE, text=json.dumps(STATUE, indent=1))
        cam = (info.get("window") or {}).get("camera") or {}
        target = cam.get("target", [0, 0, 0])
        call(sculptor, "world_spawn", name="edswarm", shape="group", position=[target[0], target[1] + 1.0, target[2]])
        st_ = call(sculptor, "world_spawn", name="statue", parent="/edswarm", shape="model", recipe="recipes/edswarm_statue.json", scale=[3.0, 3.0, 3.0])
        statue = st_["id"]
        if cam:
            for up in [3.0, 5.0, 8.0, 12.0, 17.0]:
                call(sculptor, "world_set", path="/edswarm", position=[target[0], target[1] + up, target[2]])
                q = call(sculptor, "sight_query", path="/edswarm/statue", eye=cam["eye"], target=cam["target"], fov_y_deg=cam.get("fov_y_deg", 50), width=1280, height=800)
                if (q.get("visible_px") or 0) >= 400:
                    report["raised_m"] = up
                    break
        # A claim another agent holds elsewhere, so the claim map shows more than one holder.
        call(other, "world_spawn", name="other_work", parent="/edswarm", shape="cube", position=[3.0, 0, 0], color="#3c78c8")
        call(other, "claim_take", path="/edswarm/other_work", lease_s=900, note="painting it")

        # -- the human opens the editor and highlights the statue (the OS path) --
        os_key(hwnd, 0x76)  # F7
        st = wait_for(lambda: (lambda s: s if s.get("open") else None)(call(owner, "editor_state")), 10)
        if not check(bool(st and st.get("open")), "F7 in the window opens the editor"):
            return 1
        ed = Editor(owner, hwnd, out)
        st = ed.settled()
        size, ecam = st["window"], st["camera"]
        cs, qs = screen_centre(sculptor, "/edswarm/statue", ecam, size)
        if not check(cs is not None, f"the statue is on screen in the editor's view ({qs.get('visible_px')} px)"):
            return 1
        park_cursor(hwnd)
        os_click(hwnd, cs[0], cs[1])
        s = wait_for(lambda: (lambda s: s if [x["id"] for x in s.get("selection", [])] == [statue] else None)(ed.settled()), 10) or {}
        if not check([x["id"] for x in s.get("selection", [])] == [statue], f"the human's click selects the statue ({s.get('selection')})"):
            return 1

        # -- Ask: only the human opens it; an agent's injected press can't --
        err, _ = raw(owner, "ui_act", surface="editor", steps=[{"click": "tb_ask"}, {"wait": 3}])
        st = ed.settled()
        check(not (st.get("ask") or {}).get("open"), f"the owner's agent pressing Ask through ui_act opens nothing ({st.get('ask')}, {err})")
        tb = (ed.state(detailed=True).get("widgets") or {}).get("tb_ask")
        if not check(bool(tb), "the toolbar shows Ask..."):
            return 1
        park_cursor(hwnd)
        os_click(hwnd, *centre(tb))
        st = wait_for(lambda: (lambda s: s if (s.get("ask") or {}).get("open") else None)(ed.state()), 10) or {}
        ask = st.get("ask") or {}
        if not check(ask.get("open") is True and "/edswarm/statue" in str(ask.get("about")), f"the human's click opens the Ask box about the statue ({ask})"):
            return 1
        # Tab to the head part (the recipe's parts come first: plinth, body, head).
        for _ in range(6):
            if "head" in str((ed.state().get("ask") or {}).get("part")):
                break
            os_key(hwnd, 0x09)
        part = (ed.state().get("ask") or {}).get("part")
        check("head" in str(part), f"Tab picks the statue's head part ({part})")
        os_chars(hwnd, WORDS)
        time.sleep(0.3)
        ed.shot("ask_typing")
        # The words are the human's alone while they type.
        est = json.dumps(call(owner, "editor_state", detailed=True))
        check(WORDS not in est and "head bigger" not in est, "editor_state (detailed) never contains the words being typed")
        err, tree = raw(owner, "ui_tree", surface="editor", limit=200)
        check(err is None and WORDS not in json.dumps(tree) and "head bigger" not in json.dumps(tree), f"ui_tree on the editor reads its panels and never contains them ({(err or 'read')[:120]}, {len(json.dumps(tree))} bytes)")
        err, _ = raw(owner, "editor_input", steps=[{"wait": 2}])
        check(err is not None and "window_held" in err, f"the owner's agent is refused editor_input while the box is open ({(err or '')[:120]})")
        err, _ = raw(owner, "ui_act", surface="editor", steps=[{"wait": 2}])
        check(err is not None and "window_held" in err, f"... and ui_act on the editor ({(err or '')[:120]})")
        err, _ = raw(rev, "review_screen", action="open")
        check(err is not None and "window_typing" in err, f"a reviewer can't put the review screen over the box while the human types ({(err or '')[:120]})")
        # Review R1, B1: the editor's screen shows what the human types, so only the owner reads
        # it; another agent's read that names no surface is the game's.
        err, _ = raw(sculptor, "ui_tree", surface="editor", limit=5)
        check(err is not None and "owner_only" in err, f"a builder can't read the editor's screen ({(err or 'it read it')[:100]})")
        err, tree = raw(sculptor, "ui_tree", limit=5)
        check(err is None and (tree or {}).get("surface") == "game", f"... and its ui_tree with no surface reads the game ({err or (tree or {}).get('surface')})")
        os_key(hwnd, 0x0D)  # Enter: send
        st = wait_for(lambda: (lambda s: s if not (s.get("ask") or {}).get("open") else None)(ed.settled()), 10) or {}
        check("Asked the agents" in (st.get("status") or {}).get("text", ""), f"Enter sends it ({(st.get('status') or {}).get('text')})")
        tasks = call(sculptor, "task_list", tag="ask").get("tasks") or []
        if not check(len(tasks) == 1, f"one 'ask' task is on the board ({len(tasks)})"):
            return 1
        task = call(sculptor, "task_get", id=tasks[0]["id"])
        t = task.get("task", task)
        report["task"] = {k: t.get(k) for k in ("id", "title", "posted_by", "entities", "parts", "tags")}
        check(t.get("posted_by") == "window:owner", f"the human posted it ({t.get('posted_by')})")
        check(WORDS in (t.get("detail") or ""), "the task carries the request")
        ents = t.get("entities") or []
        check(len(ents) == 1 and ents[0].get("id") == statue and ents[0].get("path") == "/edswarm/statue", f"the statue is its entity, by identity ({ents})")
        parts = t.get("parts") or []
        check(parts == [{"kind": "recipe_part", "entity": 0, "name": "head"}], f"the head is its part descriptor ({parts})")

        # -- the sculptor takes it, claims, edits and submits --
        tid = t["id"]
        call(sculptor, "task_take", id=tid)
        claim = call(sculptor, "claim_take", path="/edswarm/statue", lease_s=900, note=f"task {tid}: the head")
        since = call(sculptor, "world_history", limit=1).get("entries", [{}])[0].get("id", 0)
        # Close-ups before and after (the statue fills the frame), for the human to compare.
        wp = call(sculptor, "world_query", path_prefix="/edswarm/statue", fields=["world_position"])["items"][0]["world_position"]
        close = {"eye": [wp[0] + 6.0, wp[1] + 6.5, wp[2] + 10.0], "target": [wp[0], wp[1] + 3.4, wp[2]], "fov_y_deg": 40, "width": 480, "height": 360, "response_format": "detailed"}
        before = (call(sculptor, "sight_frame", **close).get("image") or {}).get("path")
        call(sculptor, "world_set", path="/edswarm/statue", params={"head_size": 1.8})
        time.sleep(2.0)  # the model rebuilds with its new parameter on a worker
        after = (call(sculptor, "sight_frame", **close).get("image") or {}).get("path")
        caps = [{"path": after, "kind": "frame", "caption": "after: head_size 1.8"}, {"path": before, "kind": "frame", "caption": "before: head_size 1.0"}]
        report["closeups"] = {"before": before, "after": after}
        sub = call(sculptor, "review_submit", title="A bigger, rounder head", kind="asset", task=tid, paths=["/edswarm/statue"], journal_since=since,
                   summary="head_size 1.0 -> 1.8, as asked", captures=[c for c in caps if c["path"]])
        report["submission"] = sub.get("id")
        # The journal links the request to its edits (checked now: the bulk check below fills the ring).
        hist = call(sculptor, "world_history", path_prefix="/edswarm/statue", limit=20).get("entries") or []
        mine = [h for h in hist if h.get("by") == "sculptor"]
        check(bool(mine), f"world_history names the sculptor's edit ({[(h.get('id'), h.get('by')) for h in hist[:4]]})")
        check(bool(sub.get("id")), f"the sculptor submits it for review ({sub.get('id')})")

        # -- review R1, B2: a lead's review screen over the Ask box takes none of the human's keys --
        # The human asks again; while they type, a granted lead opens the review screen on the
        # submission (a lead may: an alert). The human, still typing, presses Enter to send. It
        # must neither approve the submission nor post anything.
        tb = (ed.state(detailed=True).get("widgets") or {}).get("tb_ask")
        park_cursor(hwnd)
        os_click(hwnd, *centre(tb))
        st = wait_for(lambda: (lambda s: s if (s.get("ask") or {}).get("open") else None)(ed.state()), 10) or {}
        check((st.get("ask") or {}).get("open") is True, "the human opens the Ask box again")
        os_chars(hwnd, "and paint it ")
        err, _ = raw(lead, "review_screen", action="open", id=sub.get("id"))
        check(err is None, f"a granted lead opens the review screen while the human types ({(err or 'opened')[:100]})")
        os_chars(hwnd, "red")
        os_key(hwnd, 0x0D)  # Enter, meant for the box
        time.sleep(0.5)
        listed = call(sculptor, "review_list", status="pending").get("submissions") or []
        check(any(x.get("id") == sub.get("id") for x in listed), f"the human's Enter approved nothing: the submission still waits ({[(x.get('id'), x.get('status')) for x in listed]})")
        asks = call(sculptor, "task_list", tag="ask").get("tasks") or []
        check(len(asks) == 1, f"... and posted nothing ({len(asks)} ask task)")
        check(not (ed.state().get("ask") or {}).get("open"), "the box closed when the screen took the window")
        # Review R2, N1: they pause (the hold on their keys runs out after 0.6 s), then press
        # Enter again. The screen's focus was moved to Close when it took the window from under
        # their typing: this Enter closes it and decides nothing.
        time.sleep(1.2)
        os_key(hwnd, 0x0D)
        time.sleep(0.6)
        listed = call(sculptor, "review_list", status="pending").get("submissions") or []
        check(any(x.get("id") == sub.get("id") for x in listed), f"an Enter after a pause approves nothing either ({[(x.get('id'), x.get('status')) for x in listed]})")
        check(call(owner, "review_screen", action="status").get("open") is not True, "... it closes the review screen (its focus was moved to Close)")
        # A lead putting a screen on the window raises an alert: a toast over the window's lower
        # right corner (and the dock's buttons there) until the human answers it. They dismiss it.
        security = lambda: (call(owner, "engine_info").get("window") or {}).get("security")
        toast = wait_for(security, 5) or {}
        dismiss = next((b for b in toast.get("buttons", []) if b.get("label") == "Dismiss"), None)
        if check(bool(dismiss), f"the lead's open raised an alert the human can dismiss ({[b.get('label') for b in toast.get('buttons', [])]})"):
            time.sleep(0.6)  # a toast's buttons act once it has been still for 0.4 s
            park_cursor(hwnd)
            os_click(hwnd, *centre(dismiss["rect"]))
            gone = wait_for(lambda: True if not security() else None, 5)
            check(gone is True, "the human's click dismisses it")

        # -- the panels, driven by the owner's agent (ui_act on the editor) --
        for w in ("tab_swarm",):
            err, _ = ui_click(owner, w)
            check(err is None, f"ui_act presses {w} ({(err or '')[:120]})")
        err, _ = ui_click(owner, "sw_place")
        sw = ed.settled().get("swarm") or {}
        check(sw.get("shown") is True and sw.get("view") == "journal" and (sw.get("place") or {}).get("entity_path") == "/edswarm/statue",
              f"the journal timeline filters to the statue ({sw.get('view')}, {sw.get('place')})")
        rows = sw.get("rows") or []
        sculpt = [i for i, r in enumerate(rows) if "sculptor" in r.get("label", "")]
        check(bool(sculpt) and any(f"task {tid}" in rows[i].get("badge", "") for i in sculpt), f"the timeline names the sculptor on the statue, with the task ({[r.get('label') for r in rows[:6]]})")
        if sculpt:
            ui_click(owner, f"sw_row[{sculpt[0]}]")
            sw = ed.settled().get("swarm") or {}
            check(statue in (sw.get("highlight") or []) and "sculptor" in sw.get("detail", ""), f"selecting it highlights the statue and names the agent ({sw.get('highlight')}, {sw.get('detail', '')[:100]})")
        ed.shot("journal_timeline")
        # The claim map and the Claims view.
        ui_click(owner, "tb_claims")
        ui_click(owner, "sw_claims")
        sw = ed.settled().get("swarm") or {}
        crow = [r for r in sw.get("rows") or [] if r.get("label", "").startswith("sculptor:")]
        check(sw.get("claim_map") is True and bool(crow), f"the claim map is on and the Claims view lists the sculptor's claim ({[r.get('label') for r in (sw.get('rows') or [])[:5]]})")
        ed.shot("claim_map")
        # The cost dashboard.
        ui_click(owner, "sw_cost")
        sw = ed.settled().get("swarm") or {}
        cost = [r for r in sw.get("rows") or [] if r.get("label", "").startswith("sculptor:")]
        check(bool(cost) and " 0 commits" not in cost[0]["label"] and " 0 calls" not in cost[0]["label"], f"the cost dashboard counts the sculptor's calls and commits ({cost[:1]})")
        report["cost_row"] = cost[0]["label"] if cost else None
        ed.shot("cost")
        # Frame time with the panels and the claim map on, paired with the editor open on its
        # inspector (claim map off) in the same conditions: the panels' own engine-thread cost is
        # the difference. The GPU is shared with other sessions: a pair over budget is measured
        # again (every pair is reported, with how many other engines ran).
        def cpu_p95(label):
            files = sorted((out / "captures").glob(f"*/perf-capture-*-{label}.json"))
            if not files:
                return None
            cpu = []
            for x in json.loads(files[-1].read_text()).get("frames") or []:
                zones = {z[0]: z for z in (x.get("zones") or {}).get("main", [])}
                if "frame" in zones:
                    cpu.append(zones["frame"][2] - (zones["frame/present"][2] if "frame/present" in zones else 0))
            cpu.sort()
            return round(cpu[int(0.95 * (len(cpu) - 1))], 3) if cpu else None

        runs = []
        for attempt in range(3):
            pair = {}
            for mode in ("inspector", "panels"):
                ui_click(owner, "tab_inspector" if mode == "inspector" else "tab_swarm")
                if (mode == "panels") != bool((ed.settled().get("swarm") or {}).get("claim_map")):
                    ui_click(owner, "tb_claims_on" if mode == "inspector" else "tb_claims")
                label = f"edswarm_{mode}_{attempt}"
                perf = call(other, "perf_capture", frames=a.frames, trace=False, label=label)
                errs = [e for e in perf.get("budget_errors") or [] if is_frame_metric(e.get("metric", ""))]
                pair[mode] = {"frame_ms": perf.get("frame_ms"), "cpu_frame_p95_ms": cpu_p95(label), "frame_budget_errors": errs}
            pair["other_engines_running"] = other_engines(eng.proc.pid)
            runs.append(pair)
            if not pair["panels"]["frame_budget_errors"]:
                break
            time.sleep(5.0)
        report["perf"] = runs
        last = runs[-1]
        check(not last["panels"]["frame_budget_errors"], f"the window's frame budgets hold with the panels and claim map on ({last['panels']['frame_budget_errors']}; inspector alone then: {last['inspector']['frame_budget_errors']}; {len(runs)} pair(s))")
        base, pan = last["inspector"]["cpu_frame_p95_ms"], last["panels"]["cpu_frame_p95_ms"]
        check(base is not None and pan is not None and pan - base <= 1.0, f"the panels and claim map add at most 1 ms to the engine thread's frame, p95 ({base} -> {pan} ms)")
        ui_click(owner, "tab_swarm")
        # Cost at the limits: a journal of big batches and a filter that matches none of them, so
        # every refresh scans to its budget (20,000 entries or 100,000 changes).
        if a.bulk:
            call(other, "world_spawn", name="bulk", shape="group")
            for b in range(a.bulk // 1000):
                call(other, "world_batch", ops=[{"op": "spawn", "name": f"b{b}_{i:04d}", "shape": "group", "parent": "/bulk"} for i in range(1000)])
            ui_click(owner, "sw_journal")
            ui_click(owner, "sw_place")  # the statue's identity, then its path: no bulk entry matches
            sw = ed.settled().get("swarm") or {}
            scan = call(other, "perf_capture", frames=120, trace=False, label="edswarm_scan")
            report["scan"] = {"place": sw.get("place"), "note": sw.get("note"), "frame_ms": scan.get("frame_ms"), "budget_errors": scan.get("budget_errors")}
            serrs = [e for e in scan.get("budget_errors") or [] if is_frame_metric(e.get("metric", ""))]
            check(not serrs, f"the frame budgets hold while the timeline filters {a.bulk} bulk changes ({serrs}; {sw.get('note')})")

        # -- the human opens it from the Review view and approves it in the review screen --
        ui_click(owner, "sw_review")
        sw = ed.settled().get("swarm") or {}
        rrow = [i for i, r in enumerate(sw.get("rows") or []) if r.get("label", "").startswith(f"#{sub.get('id')} ")]
        if check(bool(rrow), f"the Review view lists the submission ({[r.get('label') for r in (sw.get('rows') or [])[:4]]})"):
            ui_click(owner, f"sw_row[{rrow[0]}]")
            ed.settled()
            ow = (ed.state(detailed=True).get("widgets") or {}).get("sw_open_review")
            if check(bool(ow), "the Review view offers Open in the review screen"):
                park_cursor(hwnd)
                os_click(hwnd, *centre(ow))
                rs = wait_for(lambda: (lambda r: r if r.get("open") else None)(call(owner, "review_screen", action="status")), 10) or {}
                check(rs.get("open") is True, f"the human's click opens the review screen on it ({rs.get('shown') or rs})")
                time.sleep(1.5)
                ed.shot("review_screen")
                os_key(hwnd, 0x0D)  # Enter on Approve (the screen's focus)
                dec = wait_for(lambda: (lambda s: s if s.get("status") == "approved" else None)(call(sculptor, "review_list", id=sub["id"])), 10) or call(sculptor, "review_list", id=sub["id"])
                check(dec.get("status") == "approved" and dec.get("decided_by") == "window:owner", f"the human approved it at the window ({dec.get('status')}, {dec.get('decided_by')})")
                os_key(hwnd, 0x78)  # F9 closes the review screen
                time.sleep(0.5)
                st = wait_for(lambda: (lambda s: s if (s.get("swarm") or {}).get("selected") is None else None)(ed.settled()), 5) or ed.state()
                check((st.get("swarm") or {}).get("selected") is None and "was approved" in (st.get("status") or {}).get("text", ""),
                      f"the decided submission leaves the Review view and the status says so ({(st.get('swarm') or {}).get('selected')}, {(st.get('status') or {}).get('text')})")
                ed.shot("approved")
        # The journal links the request to its edits.
        call(sculptor, "claim_release", id=claim["id"])
        call(sculptor, "task_done", id=tid, summary="head_size 1.8, approved")
        # The test recipe was written into the game's own assets folder: take it out again.
        err, _ = raw(sculptor, "project_delete", path=RECIPE)
        check(err is None, f"the test recipe is deleted from the game's assets ({(err or '')[:120]})")
        report["validation_errors"] = None
    report["validation_errors"] = eng.validation_errors()
    check(report["validation_errors"] == 0, f"validation errors at shutdown: {report['validation_errors']}")
    report["shots"] = [str(p) for p in ed.shots] if "ed" in dir() else []
    (out / "report.json").write_text(json.dumps(report, indent=1))
    print(f"\nreport: {out / 'report.json'}")
    if ec.FAILS:
        print(f"FAIL: {len(ec.FAILS)} check(s) failed")
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
