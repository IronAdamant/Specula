"""Live check of the review queue, critiques, merges and the director (docs/design/REVIEW.md).

A scripted lead turns a design-doc section into tasks for three builders and a reviewer
(task_plan, assign by_role). The builders stage Sepulcra props, take contact sheets
(sight_orbit) and submit them to the review queue with checks; a sight-skeptic files critiques.
Then: a rejection becomes a rework task, a critique becomes a task whose end resolves it, the
owner approves one submission at the window's review screen by keyboard (F9, Enter: posted to the
real window), a batch is approved and promoted into /world/props (one refused while its source is
claimed, then approved), stale work is refused, three-way merges are offered on a stale world_set
and a stale lex_edit and accepted, merge_suggest merges a recipe, and task_progress reports the
plan complete. Finally the world is saved and reloaded by a fresh headless engine: the queue and
the critiques come back.

Usage: cargo build -p specula && python scripts/review_check.py [--port 7484] [--adapter integrated]
       [--exe PATH] [--out DIR]
Captures (contact sheets, the review screen through sight and as the real window) go to --out
(default target/review-check). Standard library only; output is ASCII.
"""
import argparse
import ctypes
import json
import os
import shutil
import struct
import sys
import time
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import Engine, ROOT  # noqa: E402

FAILS = []
RECIPES = ["recipes/rune_pillar.json", "recipes/brazier_cairn.json", "recipes/graveyard_fence.json", "recipes/rune_column.json"]
PLAN_DOC = """# Crypt

## Crypt props

- [ ] Stage the first crypt props #builder
- [ ] Stage the second crypt props #builder
- [ ] Stage the third crypt props #builder
- [ ] Review the crypt props #reviewer (after: stage the third)
  Approve good batches, reject with a note.
"""


def check(cond, what):
    print(("ok   " if cond else "FAIL ") + what)
    if not cond:
        FAILS.append(what)
    return cond


def body(result):
    return json.loads(next(c["text"] for c in result["content"] if c["type"] == "text"))


def call(mcp, tool, **args):
    return body(mcp.call(tool, **args))


def refused(mcp, tool, **args):
    """A call expected to fail: its error JSON."""
    result = mcp.request("tools/call", {"name": tool, "arguments": args})
    if not result.get("isError"):
        FAILS.append(f"{tool} should have been refused")
        print(f"FAIL {tool} should have been refused: {result['content'][0]['text'][:300]}")
        return {}
    return json.loads(result["content"][0]["text"])


# ---- the real window (Win32, out of process) ----

if sys.platform == "win32":
    from ctypes import wintypes
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    gdi32 = ctypes.WinDLL("gdi32")
    user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))


def find_window(pid):
    found = []
    proto = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

    def visit(hwnd, _):
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and user32.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True

    user32.EnumWindows(proto(visit), 0)
    return found[0] if found else None


def key(hwnd, vk, scan):
    """A key press and release posted to the window, as a human's keyboard would send them."""
    user32.PostMessageW(hwnd, 0x0100, vk, 1 | (scan << 16))
    time.sleep(0.05)
    user32.PostMessageW(hwnd, 0x0101, vk, 1 | (scan << 16) | (0xC0 << 24))


def screenshot(hwnd, path):
    """The client area as rendered (PrintWindow) -> RGB PNG; True when not blank."""
    r = wintypes.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(r))
    w, h = r.right - r.left, r.bottom - r.top
    hdc_window = user32.GetDC(hwnd)
    hdc = gdi32.CreateCompatibleDC(hdc_window)
    bitmap = gdi32.CreateCompatibleBitmap(hdc_window, w, h)
    gdi32.SelectObject(hdc, bitmap)
    ok = user32.PrintWindow(hwnd, hdc, 3)
    header = struct.pack("<IiiHHIIiiII", 40, w, -h, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = ctypes.create_string_buffer(w * h * 4)
    gdi32.GetDIBits(hdc, bitmap, 0, h, pixels, ctypes.c_char_p(header), 0)
    gdi32.DeleteObject(bitmap)
    gdi32.DeleteDC(hdc)
    user32.ReleaseDC(hwnd, hdc_window)
    raw = bytearray()
    for y in range(h):
        row = pixels.raw[y * w * 4:(y + 1) * w * 4]
        raw.append(0)
        for x in range(0, w * 4, 4):
            raw += bytes((row[x + 2], row[x + 1], row[x]))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 6)) + chunk(b"IEND", b"")
    path.write_bytes(png)
    return ok != 0 and len({pixels.raw[i:i + 3] for i in range(0, len(pixels.raw), 4 * 97)}) > 2


def wait_for(pred, timeout=10.0):
    end = time.time() + timeout
    while time.time() < end:
        v = pred()
        if v:
            return v
        time.sleep(0.1)
    return pred()


# ---- the run ----

def stage(mcp, name, x0, recipes, scale=1.0):
    """A builder's staged props: a group under /staging with models in a row."""
    call(mcp, "world_spawn", shape="group", name=name, parent="/staging", position=[x0, 0, 0], tags=["staging"])
    for i, r in enumerate(recipes):
        call(mcp, "world_spawn", shape="model", recipe=r, name=f"prop_{i}", parent=f"/staging/{name}", position=[i * 3.0, 0, 0], scale=[scale] * 3)


def orbit(mcp, target, out, label):
    o = call(mcp, "sight_orbit", target=target, views=8, elevation_deg=22, tile_width=320, tile_height=240)
    src = Path(o["image"]["path"])
    shutil.copy(src, out / f"{label}.png")
    return o["image"]["path"], o["image"]["packet_path"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7484)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--exe")
    ap.add_argument("--out", default=str(ROOT / "target" / "review-check"))
    a = ap.parse_args()
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    exe = Path(a.exe) if a.exe else target_dir / "debug" / ("specula.exe" if sys.platform == "win32" else "specula")
    out = Path(a.out)
    worlds = out / "worlds"
    if worlds.exists():
        shutil.rmtree(worlds)
    out.mkdir(parents=True, exist_ok=True)
    args = ["--adapter", a.adapter, "--size", "1280x800", "--assets", str(ROOT / "games" / "sepulcra" / "assets"),
            "--captures", str(out / "captures"), "--worlds", str(worlds), "--world", "review_check"]
    report = {}
    with Engine(exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as eng:
        lead, b1, b2, b3, rev, sk = (eng.mcp(n) for n in ("lead", "builder-1", "builder-2", "builder-3", "reviewer", "skeptic"))
        lead.hello("lead", role="lead")
        for m, n in ((b1, "builder-1"), (b2, "builder-2"), (b3, "builder-3")):
            m.hello(n, role="builder")
        rev.hello("reviewer", role="reviewer")
        sk.hello("skeptic", role="sight-skeptic")
        tools = lead.tools()
        mine = ["critique_add", "critique_list", "critique_update", "merge_suggest", "review_decide", "review_list", "review_screen",
                "review_submit", "task_plan", "task_progress"]
        check(all(t in tools for t in mine), "the review, critique, merge and plan tools are listed")

        # -- the director: plan a real roadmap section (dry run), then the crypt section --
        dry = call(lead, "task_plan", file="docs/ROADMAP.md", section="M4: asset studio, style and review", dry_run=True)
        check(dry.get("dry_run") and len(dry["tasks"]) >= 10, f"task_plan dry-runs the ROADMAP's M4 section ({len(dry.get('tasks', []))} open items)")
        plan = call(lead, "task_plan", text=PLAN_DOC, section="crypt props", assign="by_role")
        pid = plan["plan"]
        who = {t["title"]: t.get("assignee") for t in plan["tasks"]}
        check(sorted(v for v in who.values() if v) == ["builder-1", "builder-2", "builder-3", "reviewer"], f"by_role assigns builders and the reviewer: {who}")
        check(plan["tasks"][3].get("after") == ["3"], "the review task waits for the third staging task")
        report["plan"] = plan

        for path in ("/world", "/staging"):
            call(lead, "world_spawn", shape="group", name=path[1:])
        call(lead, "world_spawn", shape="group", name="props", parent="/world")

        # -- builders: take their task, stage, capture, submit --
        subs = {}
        for i, (m, name) in enumerate(((b1, "b1_props"), (b2, "b2_props"), (b3, "b3_props"))):
            mine = call(m, "task_list", ready_for_me=True, tag="role:builder")["tasks"]
            check(len(mine) == 1, f"{name}: one builder task is ready for its agent")
            call(m, "task_take", id=mine[0]["id"])
            stage(m, name, i * 40.0, RECIPES[i:i + 2])
            claim = call(m, "claim_take", path=f"/staging/{name}", lease_s=300)
            png, packet = orbit(m, f"/staging/{name}", out, f"sheet_{name}")
            s = call(m, "review_submit", title=f"Crypt props {i + 1}", kind="asset", batch="crypt_props",
                     summary=f"Two Sepulcra props for the east crypt ({', '.join(RECIPES[i:i + 2])}), staged at T1.",
                     paths=[f"/staging/{name}"], promote={"to": "/world/props"},
                     captures=[{"path": png, "kind": "contact_sheet", "packet": packet}],
                     checks=[{"kind": "style", "name": "style.toml budgets", "passed": True, "detail": "under the 800-triangle prop budget"},
                             {"kind": "taste", "name": "silhouette_distinct", "passed": i != 1, "severity": "warning", "metric": "silhouette_iou",
                              "value": 0.81 if i == 1 else 0.42, "limit": 0.75}])
            subs[name] = s["id"]
            check(s["status"] == "pending" and s.get("task") == mine[0]["id"], f"{name}: submitted as #{s['id']} for task {s.get('task')}")
            if name != "b3_props":
                call(m, "claim_release", id=claim["id"])
        # builder-2's extra, oversized props: to be rejected.
        stage(b2, "b2_extra", 140.0, ["recipes/sky_obelisk.json"], scale=4.0)
        png, packet = orbit(b2, "/staging/b2_extra", out, "sheet_b2_extra")
        extra = call(b2, "review_submit", title="Crypt obelisk", kind="asset", paths=["/staging/b2_extra"], promote={"to": "/world/props"},
                     captures=[{"path": png, "kind": "contact_sheet", "packet": packet}],
                     checks=[{"kind": "style", "name": "scale_range", "passed": False, "metric": "height_m", "value": 24.0, "limit": 9.0}])
        subs["b2_extra"] = extra["id"]
        bad = refused(b1, "review_submit", title="x", paths=["/staging/b1_props"], captures=["no_such.png"])
        check(bad.get("error") == "capture_missing", "a submission whose capture isn't a PNG on disk is refused")
        lst = call(rev, "review_list", status="pending")
        check(lst["total"] == 4 and lst["pending_batches"].get("crypt_props") == 3, f"the queue holds 4 pending, 3 in batch crypt_props")
        check(refused(b1, "review_decide", ids=[subs["b2_props"]], verdict="approve").get("error") == "not_reviewer", "a builder can't decide")

        # -- critiques --
        crit = call(sk, "critique_add", items=[
            {"text": "The brazier's rim reads flat against the slab at gameplay distance", "severity": "warning", "source": "sight-skeptic",
             "capture": str(out / "sheet_b1_props.png"), "metric": {"name": "rim_contrast", "value": 0.08, "limit": 0.15},
             "path": "/staging/b1_props", "submission": subs["b1_props"]},
            {"text": "The obelisk is 24 m tall: three times the prop range", "severity": "error", "source": "sight-skeptic",
             "submission": subs["b2_extra"], "path": "/staging/b2_extra"}])
        c1, c2 = crit["added"]

        # -- reject: a rework task for the submitter --
        r = call(rev, "review_decide", ids=[subs["b2_extra"]], verdict="reject", note="Far too tall for a crypt prop; keep it under 9 m.")
        rework = r["results"][0].get("rework_task")
        check(r["decided"] == 1 and rework, f"the rejection posted rework task {rework}")
        t = call(b2, "task_get", id=rework)
        check(t.get("assignee") == "builder-2" and "rework" in t.get("tags", []) and "Open critiques" in t.get("detail", ""),
              "the rework task is assigned to the submitter with the note and the open critique")

        # -- a critique round-trips through the board --
        u = call(rev, "critique_update", id=c1, action="to_task", assignee="builder-1")
        ctask = u["results"][0]["task"]
        call(b1, "task_take", id=ctask)
        call(b1, "task_done", id=ctask, summary="Checked at 30 m: the rim reads against the slab once the fire is lit")
        cl = call(sk, "critique_list", status="resolved")
        check(any(c["id"] == c1 for c in cl["critiques"]), "the critique resolved itself when its task was done")

        # -- the owner at the window: show #b1 on the review screen, press Enter --
        hwnd = find_window(eng.proc.pid) if sys.platform == "win32" else None
        st = call(lead, "review_screen", action="show", id=subs["b1_props"])
        check(st["open"] and st["shown"] == subs["b1_props"], "the lead puts submission b1 on the review screen")
        time.sleep(1.5)  # the capture decodes on a worker and uploads
        frame = call(lead, "sight_frame", width=1280, height=800, include_ui=True)
        shutil.copy(frame["image"]["path"], out / "review_screen_sight.png")
        shutil.copy(frame["image"]["packet_path"], out / "review_screen_sight.json")
        check(frame["id_buffer"].get("ui_occluders", 0) >= 1, f"the review screen hides the scene in the sight packet ({frame['id_buffer'].get('ui_occluders')} occluders)")
        if hwnd:
            check(screenshot(hwnd, out / "review_screen_window.png"), "the real window shows the review screen (screenshot)")
            key(hwnd, 0x0D, 0x1C)  # Enter on the focused Approve
            # Generous: a loaded machine (builds alongside) can take seconds to deliver a posted key.
            got = wait_for(lambda: call(rev, "review_list", id=subs["b1_props"])["status"] != "pending", 30)
            s1 = call(rev, "review_list", id=subs["b1_props"])
            check(s1["status"] == "approved" and s1["decision"]["by"] == "window:owner", f"Enter approved b1 as the window's owner ({s1['status']}, {s1.get('decision', {}).get('by')})")
            check(s1.get("decision", {}).get("promoted", {}).get("moved"), "the window's approval promoted b1 into /world/props")
            key(hwnd, 0x0D, 0x1C)  # a second, hasty Enter: the next submission has only just appeared
            time.sleep(0.8)
            check(call(rev, "review_list", id=subs["b2_props"])["status"] == "pending", "a hasty second Enter decides nothing new")
            time.sleep(0.5)
            screenshot(hwnd, out / "review_screen_after_approve.png")
            key(hwnd, 0x78, 0x43)  # F9 closes it
            check(wait_for(lambda: not call(lead, "review_screen")["open"], 5), "F9 closes the review screen")
            check(refused(lead, "review_screen", action="open").get("error") == "owner_closed", "an agent can't reopen what the owner just closed")
            key(hwnd, 0x78, 0x43)  # and opens it again
            check(wait_for(lambda: call(lead, "review_screen")["open"], 5), "F9 opens it again")
            call(lead, "review_screen", action="close")
        else:
            call(rev, "review_decide", ids=[subs["b1_props"]], verdict="approve")
            print("note: no window (headless or not Windows): b1 approved by the reviewer instead")

        # -- batch approval; the claimed one is refused until released --
        r = call(rev, "review_decide", batch="crypt_props", verdict="approve", note="On style; welcome to the crypt")
        errs = [x for x in r["results"] if x.get("error")]
        check(r["decided"] == 1 and len(errs) == 1 and errs[0]["error"] == "claimed", f"batch: b2 promoted, b3 refused while builder-3 claims its source ({[x.get('error') for x in errs]})")
        call(b3, "claim_release", all=True)
        r = call(rev, "review_decide", ids=[subs["b3_props"]], verdict="approve")
        check(r["decided"] == 1, "b3 is approved once its claim is released")
        props = call(lead, "world_query", path_prefix="/world/props", fields=["path"], limit=100)["items"]
        moved = sorted({p["path"].split("/")[3] for p in props if p["path"].count("/") >= 3})
        check(moved == ["b1_props", "b2_props", "b3_props"], f"approved work is in the game world: {moved}")

        # -- reject with revert: the submitter's own entries are undone, nothing else --
        since = call(b3, "world_hash")["journal_last_id"]
        call(b3, "world_spawn", shape="cube", name="junk", parent="/staging")
        junk = call(b3, "review_submit", title="A junk cube", journal_since=since)
        check(junk.get("journal_entries") == 1, "journal_since covers the submitter's own new entry")
        r = call(rev, "review_decide", ids=[junk["id"]], verdict="reject", note="Not wanted in the crypt", revert=True)
        gone = refused(lead, "world_query", path_prefix="/staging/junk", fields=["path"]).get("error") == "not_found"
        check(r["results"][0].get("reverted") and gone, "a rejection with revert undoes the submitter's entries")
        bad = refused(b3, "review_submit", title="Not mine", journal=[since])
        check(bad.get("error") in ("not_your_entry", "journal_not_found"), f"a submission can't cover another agent's entries ({bad.get('error')})")
        jt = r["results"][0]["rework_task"]
        call(b3, "task_take", id=jt)
        call(b3, "task_done", id=jt, summary="reverted; nothing to redo")

        # -- rework: fix, resubmit with supersedes, approve --
        call(b2, "task_take", id=rework)
        for e in call(b2, "world_query", path_prefix="/staging/b2_extra", fields=["path", "shape"])["items"]:
            if e["shape"] == "model":
                call(b2, "world_set", path=e["path"], scale=[1, 1, 1])
        png, packet = orbit(b2, "/staging/b2_extra", out, "sheet_b2_extra_v2")
        v2 = call(b2, "review_submit", title="Crypt obelisk (rescaled)", kind="asset", paths=["/staging/b2_extra"], promote={"to": "/world/props"},
                  supersedes=subs["b2_extra"], captures=[{"path": png, "kind": "contact_sheet"}],
                  checks=[{"kind": "style", "name": "scale_range", "passed": True, "metric": "height_m", "value": 6.0, "limit": 9.0}])
        # Stale work is refused: builder-2 nudges it after submitting.
        first_model = next(e["path"] for e in call(b2, "world_query", path_prefix="/staging/b2_extra", fields=["path", "shape"])["items"] if e["shape"] == "model")
        call(b2, "world_set", path=first_model, position=[0, 0, 0.5])
        st = refused(rev, "review_decide", ids=[v2["id"]], verdict="approve")
        check(st.get("error") == "stale_submission" and st["results"][0].get("changed"), "work changed after submitting is refused as stale")
        v3 = call(b2, "review_submit", title="Crypt obelisk (rescaled, nudged)", kind="asset", paths=["/staging/b2_extra"], promote={"to": "/world/props"},
                  supersedes=v2["id"], captures=[{"path": png, "kind": "contact_sheet"}])
        check(call(rev, "review_list", id=v2["id"])["status"] == "withdrawn", "superseding withdraws the older pending submission")
        r = call(rev, "review_decide", ids=[v3["id"]], verdict="approve")
        check(r["decided"] == 1, "the reworked obelisk is approved")
        call(b2, "task_done", id=rework, summary=f"rescaled to 6 m; approved as #{v3['id']}")

        # -- three-way merges --
        ent = "/world/props/b1_props"
        base = call(b1, "world_query", path_prefix=ent, fields=["path", "version"], limit=1)["items"][0]
        call(b1, "world_set", path=ent, props={"stats": {"hp": 10, "speed": 1}}, expect_version=base["version"])
        v = call(b1, "world_query", path_prefix=ent, fields=["path", "version"], limit=1)["items"][0]["version"]
        call(b3, "world_set", path=ent, props={"stats": {"hp": 14, "speed": 1}}, expect_version=v)
        stale = refused(b1, "world_set", path=ent, props={"stats": {"hp": 10, "speed": 2}}, expect_version=v)
        m = stale.get("conflict", {}).get("merge", {})
        check(stale.get("error") == "stale_version" and m.get("clean"), "a stale world_set carries a clean merge")
        call(b1, "world_set", **m["apply"]["args"])
        now = call(b1, "world_query", path_prefix=ent, fields=["path", "props"], limit=1)["items"][0]["props"]["stats"]
        check(now == {"hp": 14, "speed": 2}, f"accepting the merge keeps both changes: {now}")
        call(lead, "lex_edit", set="crypt_rules", tick_hz=30, ops=[{"op": "put", "id": "dust", "item": {"kind": "resource", "start": 5}}])
        call(b1, "lex_edit", set="crypt_rules", ops=[{"op": "patch", "id": "dust", "set": {"start": 8}, "expect_version": 1}])
        lx = refused(b2, "lex_edit", set="crypt_rules", ops=[{"op": "patch", "id": "dust", "set": {"notes": "crypt dust"}, "expect_version": 1}])
        lm = lx.get("merge", {})
        check(lx.get("error") == "version_conflict" and lm.get("clean"), "a stale lex_edit carries a clean merge")
        call(b2, "lex_edit", **lm["apply"]["args"])
        item = call(b2, "lex_get", set="crypt_rules", ids=["dust"])["items"][0]["item"]
        check(item.get("start") == 8 and item.get("notes") == "crypt dust", f"accepting the Lex merge keeps both: {item}")
        recipe = json.loads((ROOT / "games" / "sepulcra" / "assets" / "recipes" / "rune_pillar.json").read_text())
        theirs, yours = json.loads(json.dumps(recipe)), json.loads(json.dumps(recipe))
        theirs["seed"] = recipe.get("seed", 0) + 1
        yours["description"] = "Rune pillar, crypt variant"
        ms = call(b1, "merge_suggest", kind="recipe", base=recipe, theirs=theirs, yours=yours)
        check(ms["clean"] and ms["merged"]["seed"] == theirs["seed"] and ms["merged"]["description"] == yours["description"], "merge_suggest merges a recipe")

        # -- the plan completes --
        for m in (b1, b2, b3):
            for t in call(m, "task_list", mine=True, status="taken")["tasks"]:
                call(m, "task_done", id=t["id"], summary="staged and approved")
        rt = call(rev, "task_list", ready_for_me=True, tag="role:reviewer")["tasks"]
        check(len(rt) == 1, "the review task is ready once the staging is done")
        call(rev, "task_take", id=rt[0]["id"])
        call(rev, "task_done", id=rt[0]["id"], summary="approved 4, rejected 1")
        call(rev, "critique_update", id=c2, action="resolve", note="rescaled to 6 m (#%d)" % v3["id"])
        prog = call(lead, "task_progress", plan=pid, response_format="detailed")
        report["progress"] = prog
        check(prog["complete"] and prog["review"]["approved"] >= 3 and prog["review"]["pending"] == 0, f"task_progress: complete ({prog['next']})")
        report["queue"] = call(rev, "review_list", response_format="concise")
        # M4 audit (probe_revert.py): a submission saved with the world names this run's journal
        # seqs; after a restart the journal counts from 1 again, so its revert must be refused.
        audit_seq = call(b1, "world_spawn", shape="cube", name="audit_box", position=[40, 0, 40])["journal"]
        audit_sub = call(b1, "review_submit", title="audit box", kind="changes", journal=[audit_seq])["id"]
        call(lead, "world_save")
        wdir = worlds / "review_check"
        check((wdir / "review.json").exists() and (wdir / "board.json").exists(), "review.json is saved beside board.json")
        totals = (call(rev, "review_list")["total"], call(rev, "critique_list")["total"])
        for m in (lead, b1, b2, b3, rev, sk):
            m.close()
    check(eng.exit_code == 0, f"the engine stopped cleanly (exit {eng.exit_code})")

    # -- a fresh headless engine reads the queue back --
    with Engine(exe, a.port, out / "engine_reload.log", "--headless", "--worlds", str(worlds), "--world", "review_check",
                "--captures", str(out / "captures"), cwd=str(ROOT)) as eng:
        m = eng.mcp("reader")
        back = (call(m, "review_list")["total"], call(m, "critique_list")["total"])
        check(back == totals, f"the queue and critiques come back with the world: {back} == {totals}")
        # Another agent's entries reuse the saved seqs; rejecting with revert must not undo them.
        m.hello("bob", role="builder")
        made = []
        while len(made) < 600:
            made.append(call(m, "world_spawn", shape="cube", name=f"bob_{len(made)}", position=[len(made) % 30, 0, -5 - len(made) // 30]))
            if made[-1]["journal"] >= audit_seq:
                break
        check(made[-1]["journal"] >= audit_seq, f"bob's entries reach the saved seq {audit_seq}")
        item = call(m, "review_list", id=audit_sub)
        check("earlier run" in item.get("journal_stale", ""), "review_list says the saved journal refs are from an earlier run")
        r = eng.mcp("reviewer")
        r.hello("reviewer", role="reviewer")
        e = refused(r, "review_decide", ids=[audit_sub], verdict="reject", note="audit probe", revert=True)
        codes = [x.get("error") for x in e.get("results", [])]
        check(codes == ["journal_from_earlier_run"], f"the revert of an earlier run's refs is refused: {codes}")
        bobs = call(m, "world_query", created_by="bob", fields=["path"], limit=10)["total"]
        boxes = call(m, "world_query", path_prefix="/audit_box", fields=["path"], limit=10)["total"]
        check(bobs == len(made) and boxes == 1, f"nothing was undone (bob's {bobs} of {len(made)} entities and the audit box are there)")
        ok = call(r, "review_decide", ids=[audit_sub], verdict="reject", note="audit probe: revert by hand")
        check(ok.get("decided") == 1, "rejecting without revert still works")
        r.close()
        m.close()
    (out / "report.json").write_text(json.dumps(report, indent=1))
    print(f"\ncaptures and report in {out}")
    if FAILS:
        print(f"{len(FAILS)} check(s) failed")
        sys.exit(1)
    print("review_check: all checks passed")


if __name__ == "__main__":
    main()
