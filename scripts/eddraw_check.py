#!/usr/bin/env python3
"""The editor's draw tools, end to end in the real window (docs/design/EDITOR.md, "Draw tools";
ROADMAP M5 "draw tools: paint vertex colours, sketch paths and splines, sculpt terrain").

A plain engine with a scratch assets folder: a small terrain the terrain renderer draws, and a
crate to paint. The owner's agent drives the editor's brush with injected window input
(editor_input: the same frames a mouse makes), and the human does one stroke with the real
mouse. It checks, failing loudly:

- the Draw bar opens from the toolbar and its buttons put a brush in hand (editor_state.brush);
- a sculpt drag over the selected terrain is ONE terrain_edit: one journal entry by window:owner
  (via the agent that drove it), 2..64 strokes appended to the recipe's terrain part (the ridge
  shows in the screenshot);
- Ctrl+Z takes the stroke back (the recipe's strokes as they were);
- a splat drag appends paint strokes on the layer in hand;
- a paint drag over the selected model is ONE model_paint: strokes in the recipe's "edits", in
  the brush's colour, undone with Ctrl+Z;
- a stroke by the human's own mouse is journaled with no `via`;
- with a brush in hand a press on what it can't draw on says so, sends nothing and selects
  nothing; Escape puts the brush down; a builder can't drive any of it;
- zero validation errors at shutdown.

Screenshots of the real window go to the output folder (sight-skeptic checks them).

    cargo build -p specula --release
    python scripts/eddraw_check.py [--adapter integrated] [--port 7650] [--out DIR] [--exe PATH]
"""
import argparse
import json
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import Engine  # noqa: E402
import editor_check as ec  # noqa: E402
from editor_check import call, raw, wait_for, find_window, park_cursor, check, Editor  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
EXE = ROOT / "target" / "release" / ("specula.exe" if sys.platform == "win32" else "specula")

FIELD = {"recipe": 1, "name": "draw_field", "seed": 9, "parts": [{"name": "ground", "material": "slab_stone", "generate": {
    "op": "terrain", "render": "terrain", "size": [96, 96], "samples": [97, 97],
    "layers": [{"kind": "fbm", "scale": 60, "octaves": 3, "amplitude": 3}],
    "splat": [{"name": "grass", "color": "#5a7a3a"}, {"name": "earth", "color": "#6b5138"}]}}]}
CRATE = {"recipe": 1, "name": "draw_crate", "collider": "none", "lods": [], "parts": [
    {"name": "box", "material": "slab_stone", "shape": {"op": "box", "size": [6, 6, 6], "bevel": 0.0}}]}
# A barrel: a lathe with a ring of vertices every 0.75 m up its side (paint colours vertices: the
# crate above has them only at its corners, so a small brush on its face reaches none).
BARREL = {"recipe": 1, "name": "draw_barrel", "collider": "none", "lods": [], "parts": [
    {"name": "side", "material": "slab_stone", "shape": {"op": "lathe", "segments": 32,
                                                         "profile": [[0, 0]] + [[3, 0.75 * k] for k in range(9)] + [[0, 6]]}}]}
# A slab with two sweeps over it: a rail (one scale per path point) and a wire.
FENCE = {"recipe": 1, "name": "draw_fence", "collider": "none", "lods": [], "parts": [
    {"name": "slab", "material": "slab_stone", "shape": {"op": "box", "size": [14, 0.2, 8], "bevel": 0.0}},
    {"name": "rail", "material": "slab_stone", "shape": {"op": "sweep", "profile": {"rect": [0.3, 0.3]}, "path": [[-5, 1.5, 0], [0, 1.5, 0], [5, 1.5, 0]], "scales": [1, 0.6, 1]}},
    {"name": "wire", "material": "slab_stone", "shape": {"op": "sweep", "profile": {"rect": [0.1, 0.1]}, "path": [[-5, 0.6, 2], [5, 0.6, 2]]}}]}


def edits_of(mcp, path, json_path):
    """The strokes a recipe holds at `json_path` (none yet: an empty list) and its version."""
    err, j = raw(mcp, "project_read", path=path, json_path=json_path, problems=False)
    if err:
        return [], None
    return j.get("value") or [], j.get("version")


def model_ready(mcp, path):
    """Whether the model entity at `path` is built (not being rebuilt from a changed recipe)."""
    err, j = raw(mcp, "world_query", path_prefix=path, fields=["path", "model_state"], limit=4)
    ents = [e for e in (j.get("items") or []) if e.get("path") == path]
    return bool(ents) and ents[0].get("model_state") == "ready"


def select_path(ed, path):
    """Selects `path` by its outliner row (expanding its parents), as a human would."""
    for _ in range(6):
        s = ed.settled()
        if [x.get("path") for x in s.get("selection") or []] == [path]:
            return True
        rows = (s.get("outliner") or {}).get("rows") or []
        w = ed.state(detailed=True).get("widgets") or {}
        hit = next((r for r in rows if r.get("path") == path), None)
        if hit and f"ol_row[{hit['row']}]" in w:
            r = w[f"ol_row[{hit['row']}]"]
            ed.input({"click": [r[0] + r[2] / 2, r[1] + r[3] / 2]}, {"wait": 3})
            continue
        parent = next((r for r in rows if path.startswith(r.get("path", "?") + "/") and r.get("has_children") and not r.get("expanded")), None)
        if parent and f"ol_x[{parent['row']}]" in w:
            r = w[f"ol_x[{parent['row']}]"]
            ed.input({"click": [r[0] + r[2] / 2, r[1] + r[3] / 2]}, {"wait": 3})
            continue
        time.sleep(0.3)
    return False


def press(ed, widget):
    """The owner's agent presses an editor widget (ui_act on the editor)."""
    err, _ = raw(ed.owner, "ui_act", surface="editor", steps=[{"click": widget}, {"wait": 2}])
    return err


def stroke_done(ed, words, timeout=60):
    """Waits for the stroke's answer: nothing in flight, and the status naming what was done."""
    def ok():
        s = ed.state()
        text = (s.get("status") or {}).get("text", "")
        return s if not (s.get("busy") or {}).get("edits") and (words in text or "not done" in text or "Not sent" in text) else None
    return wait_for(ok, timeout) or ed.state()


def os_drag(hwnd, a, b, steps=14):
    """A left-button drag with the real mouse messages (down, moves with the button held, up)."""
    lp = lambda p: (int(p[1]) << 16) | (int(p[0]) & 0xFFFF)  # noqa: E731
    ec.user32.PostMessageW(hwnd, 0x0200, 0, lp(a))
    time.sleep(0.05)
    ec.user32.PostMessageW(hwnd, 0x0201, 1, lp(a))
    for k in range(1, steps + 1):
        time.sleep(0.03)
        p = [a[0] + (b[0] - a[0]) * k / steps, a[1] + (b[1] - a[1]) * k / steps]
        ec.user32.PostMessageW(hwnd, 0x0200, 1, lp(p))
    time.sleep(0.05)
    ec.user32.PostMessageW(hwnd, 0x0202, 0, lp(b))
    time.sleep(0.2)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--exe", default=str(EXE))
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--port", type=int, default=7650)
    ap.add_argument("--out")
    ap.add_argument("--record", action="store_true", help="also record the sculpting and the path sketch as animated PNGs of the engine's own window (scripts/window_recorder.py); the strokes are drawn slower")
    a = ap.parse_args()
    if sys.platform != "win32":
        print("skip: the draw tools check drives the real window (Windows only for now)")
        return 0
    exe = Path(a.exe)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p specula --release")
    out = Path(a.out) if a.out else ROOT / "target" / "eddraw-check" / a.adapter
    if out.exists():
        shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True, exist_ok=True)
    assets = out / "assets"
    (assets / "recipes").mkdir(parents=True)
    shutil.copy(ROOT / "games" / "sepulcra" / "assets" / "style.json", assets / "style.json")
    (assets / "recipes" / "draw_field.json").write_text(json.dumps(FIELD, indent=1))
    (assets / "recipes" / "draw_crate.json").write_text(json.dumps(CRATE, indent=1))
    (assets / "recipes" / "draw_fence.json").write_text(json.dumps(FENCE, indent=1))
    (assets / "recipes" / "draw_barrel.json").write_text(json.dumps(BARREL, indent=1))
    # A second barrel, spawned three times the size; and a crate nobody spawns (512 strokes).
    (assets / "recipes" / "draw_keg.json").write_text(json.dumps(dict(BARREL, name="draw_keg"), indent=1))
    (assets / "recipes" / "draw_dots.json").write_text(json.dumps(dict(CRATE, name="draw_dots"), indent=1))
    args = ["--adapter", a.adapter, "--size", "1280x800", "--assets", str(assets), "--captures", str(out / "captures"), "--worlds", str(out / "worlds"),
            "--owner", "owner-proxy"]
    report = {"adapter": a.adapter}
    field_file, crate_file, fence_file = "assets/recipes/draw_field.json", "assets/recipes/draw_crate.json", "assets/recipes/draw_fence.json"
    barrel_file = "assets/recipes/draw_barrel.json"
    with Engine(exe, a.port, out / "engine.log", *args, cwd=str(out)) as eng:
        owner, builder = eng.mcp("owner"), eng.mcp("builder")
        for m, name, role in [(owner, "owner-proxy", "owner"), (builder, "bob", "builder")]:
            for _ in range(120):
                err, _j = raw(m, "agent_hello", name=name, role=role)
                if not err:
                    break
                time.sleep(1.0)
            else:
                sys.exit(f"agent_hello {name} never succeeded: {err}")
        hwnd = wait_for(lambda: find_window(eng.proc.pid), 30)
        if not check(bool(hwnd), "the engine has a window"):
            return 1
        # A script's engine takes no keyboard from whoever is using this machine (--no-activate,
        # which specula_mcp.Engine adds): the window in front is still someone else's.
        front = (ec.user32.GetForegroundWindow() or 0) & 0xFFFFFFFF
        check(front != (hwnd or 0) & 0xFFFFFFFF, "the engine's window opened without taking the keyboard (it is not the foreground window)")
        call(builder, "world_spawn", shape="model", path="/field", recipe="recipes/draw_field.json")
        call(builder, "world_spawn", shape="model", path="/crate", recipe="recipes/draw_crate.json", position=[0, 12, 0])
        call(builder, "world_spawn", shape="model", path="/fence", recipe="recipes/draw_fence.json", position=[60, 20, 0])
        call(builder, "world_spawn", shape="model", path="/barrel", recipe="recipes/draw_barrel.json", position=[0, 12, 40])
        call(builder, "world_spawn", shape="model", path="/keg", recipe="recipes/draw_keg.json", position=[0, 30, -80], scale=[3, 3, 3])
        tools = builder.tools()
        check("model_paint" in tools and "terrain_edit" in tools and "model_path" in tools, "model_paint, model_path and terrain_edit are listed")
        # ---- the tools' guards (review R1, M1): a wrong-typed dry_run or expect_version is
        # refused, never ignored (dry_run: "true" committed) ----
        version = lambda f: edits_of(builder, f, "name")[1]  # noqa: E731
        stroke3 = {"at": [3, 6, 3], "radius": 0.5, "color": "#ff0000"}
        calls = [("model_paint", "assets/recipes/draw_dots.json", {"recipe": "recipes/draw_dots.json", "strokes": [stroke3]}),
                 ("model_path", fence_file, {"recipe": "recipes/draw_fence.json", "part": "wire", "points": [[0, 0], [1, 0]]}),
                 ("terrain_edit", field_file, {"recipe": "recipes/draw_field.json", "strokes": [{"op": "raise", "at": [0, 0], "radius": 2, "amount": 0.1}]})]
        slipped = []
        for tool, file, args in calls:
            for bad in ({"dry_run": "true"}, {"dry_run": 1}, {"expect_version": "1"}, {"expect_version": -1}, {"expect_version": 1.5}):
                v0 = version(file)
                err, _ = raw(builder, tool, **args, **bad)
                if not (err and "invalid_argument" in err) or version(file) != v0:
                    slipped.append((tool, bad, (err or "it ran")[:80]))
        check(not slipped, f"model_paint, model_path and terrain_edit refuse a wrong-typed dry_run or expect_version and write nothing ({slipped[:3]})")
        err, dry = raw(builder, "model_paint", recipe="recipes/draw_dots.json", strokes=[stroke3], dry_run=True)
        check(err is None and version("assets/recipes/draw_dots.json") == 1, f"a real dry run writes nothing ({err})")
        # ---- 512 strokes a recipe (review R1, m2): reachable with full-precision numbers ----
        fine = [{"at": [1.2345678901234567 + k * 1e-3, 2.3456789012345678, 3.4567890123456789], "radius": 0.123456789, "color": "#c8a060", "amount": 0.87654321} for k in range(64)]
        errs = [raw(builder, "model_paint", recipe="recipes/draw_dots.json", strokes=fine)[0] for _ in range(8)]
        over, _ = raw(builder, "model_paint", recipe="recipes/draw_dots.json", strokes=fine[:1])
        kept = edits_of(builder, "assets/recipes/draw_dots.json", "edits[0]")[0] or {}
        check(not any(errs) and over is not None and "too_many" in over and kept.get("at") == [1.2346, 2.3457, 3.4568],
              f"512 full-precision strokes fit in 8 calls (kept to a tenth of a millimetre), and the 513th is refused too_many ({[e[:60] for e in errs if e][:2]}; {(over or 'it ran')[:80]}; {kept.get('at')})")

        ed = Editor(owner, hwnd, out)
        ed.input({"key": "F7"}, {"wait": 6})
        check(ed.state().get("open") is True, "F7 opens the editor")

        # ---- sculpt: one drag, one terrain_edit ----
        if not check(select_path(ed, "/field"), "the terrain is selected from the outliner"):
            return 1
        ed.input({"key": "F"}, {"wait": 20})  # frame it
        check(press(ed, "tb_draw") is None, "the toolbar's Draw opens the draw bar")
        check(press(ed, "br_sculpt") is None, "Sculpt puts a brush in hand")
        for _ in range(7):
            press(ed, "br_r_up")
        brush = ed.settled().get("brush") or {}
        check(brush.get("kind") == "raise" and brush.get("radius", 0) > 6, f"editor_state names the brush ({brush})")
        vp = ed.state().get("viewport")
        cx, cy = vp[0] + vp[2] / 2, vp[1] + vp[3] / 2
        a0, a1 = [cx - 150, cy + 20], [cx + 150, cy + 20]
        before, v0 = edits_of(builder, field_file, "parts[0].generate.edits")
        ed.shot("field_before")
        ed.input({"move": a1}, {"wait": 3})
        ed.shot("brush_hover")
        ed.input({"drag": a0, "to": a1, "frames": 24}, {"wait": 2})
        st = stroke_done(ed, "Sculpted the terrain")
        after, v1 = edits_of(builder, field_file, "parts[0].generate.edits")
        added = len(after) - len(before)
        check(2 <= added <= 64 and all(s.get("op") == "raise" for s in after[len(before):]), f"the drag appended raise strokes to the recipe ({added}; status: {(st.get('status') or {}).get('text')})")
        hist = call(builder, "world_history", limit=4).get("entries") or []
        top = hist[0] if hist else {}
        check(top.get("by") == "window:owner" and top.get("via") == "owner-proxy", f"it is one journal entry by window:owner, via the agent that drove it ({top.get('id')}: {top.get('by')} via {top.get('via')})")
        report["sculpt"] = {"added": added, "journal": top.get("id"), "version": [v0, v1]}
        # Undo takes the whole stroke back.
        ed.input({"key": "Ctrl+Z"}, {"wait": 4})
        back = wait_for(lambda: (lambda e: e if len(e) == len(before) else None)(edits_of(builder, field_file, "parts[0].generate.edits")[0]), 20)
        check(back is not None or len(before) == 0 and len(edits_of(builder, field_file, "parts[0].generate.edits")[0]) == 0, f"Ctrl+Z takes the stroke back ({len(edits_of(builder, field_file, 'parts[0].generate.edits')[0])} strokes, {len(before)} before)")

        # A ridge worth seeing: the strongest brush, three strokes over the same line.
        for _ in range(5):
            press(ed, "br_a_up")
        landed = []
        rec = None
        if a.record:
            from window_recorder import WindowRecorder
            rec = WindowRecorder(hwnd, fps=10)
            rec.start()
        for _ in range(3):
            n = len(edits_of(builder, field_file, "parts[0].generate.edits")[0])
            ed.input({"drag": a0, "to": a1, "frames": 60 if a.record else 24}, {"wait": 2})
            st = stroke_done(ed, "Sculpted the terrain")
            landed.append((len(edits_of(builder, field_file, "parts[0].generate.edits")[0]) - n, (st.get("status") or {}).get("text", "")[:60]))
            # The terrain is rebuilt from its changed recipe: the next stroke waits for it, as a hand would.
            rebuilt = wait_for(lambda: model_ready(builder, "/field"), 20)
        check(all(n > 0 for n, _ in landed), f"three strokes over the same line each land ({landed})")
        # (What the "sculpted" capture shows: these three strokes, not the first one, undone above.)
        report["ridge"] = {"strokes": [n for n, _ in landed], "journal": ((call(builder, "world_history", limit=2).get("entries") or [{}])[0]).get("id")}
        check(rebuilt is True, "the terrain says when it is rebuilt (model_state: ready)")
        time.sleep(1.5)  # the rebuilt terrain uploads
        ed.shot("field_sculpted")

        # ---- splat: the next ground layer ----
        check(press(ed, "br_splat") is None and press(ed, "br_splat_on") is None, "Splat, pressed again, takes the next layer")
        brush = ed.settled().get("brush") or {}
        check(brush.get("kind") == "splat" and brush.get("layer") == 1, f"the splat brush holds layer 1 ({brush})")
        n0 = len(edits_of(builder, field_file, "parts[0].generate.edits")[0])
        ed.input({"drag": a0, "to": a1, "frames": 24}, {"wait": 2})
        st = stroke_done(ed, "Painted the terrain")
        now = edits_of(builder, field_file, "parts[0].generate.edits")[0]
        check(len(now) > n0 and all(s.get("op") == "paint" and s.get("layer") == 1 for s in now[n0:]), f"the drag appended paint strokes on layer 1 ({len(now) - n0}; status: {(st.get('status') or {}).get('text')})")
        time.sleep(1.5)
        ed.shot("field_splat")
        if rec:
            report["sculpt_clip"] = rec.stop(out / "sculpt_clip.png")
            print(f"recorded {out / 'sculpt_clip.png'}: {report['sculpt_clip']}")

        # ---- the human's own mouse: a stroke with no via ----
        park_cursor(hwnd)
        n1 = len(now)
        os_drag(hwnd, [cx - 120, cy - 60], [cx + 120, cy - 60])
        # (The status already reads "Painted the terrain" from the stroke before: the recipe
        # growing is what says this one landed.)
        now = wait_for(lambda: (lambda e: e if len(e) > n1 else None)(edits_of(builder, field_file, "parts[0].generate.edits")[0]), 30) or edits_of(builder, field_file, "parts[0].generate.edits")[0]
        top = (call(builder, "world_history", limit=2).get("entries") or [{}])[0]
        check(len(now) > n1 and top.get("by") == "window:owner" and not top.get("via"), f"the human's own drag is a stroke by window:owner with no via ({len(now) - n1} strokes; via {top.get('via')}; status: {(ed.state().get('status') or {}).get('text')}; brush {ed.state().get('brush')})")

        # ---- nothing to draw on, and nothing selected by the brush ----
        ed.input({"click": [vp[0] + 30, vp[1] + 12]}, {"wait": 3})  # the sky, at the top of the viewport
        st = ed.settled()
        sel = [x.get("path") for x in st.get("selection") or []]
        check(sel == ["/field"] and "Nothing to draw on" in (st.get("status") or {}).get("text", ""), f"a press on what the brush can't draw on says so and selects nothing ({sel}; {(st.get('status') or {}).get('text')})")

        # ---- paint: the crate's colours ----
        ed.input({"key": "Escape"}, {"wait": 2})
        check(ed.settled().get("brush") is None, "Escape puts the brush down")
        if not check(select_path(ed, "/crate"), "the crate is selected from the outliner"):
            return 1
        ed.input({"key": "F"}, {"wait": 20})
        check(press(ed, "br_paint") is None, "Paint puts the paint brush in hand")
        for _ in range(10):
            press(ed, "br_r_down")  # a brush for a crate, not a field
        brush = ed.settled().get("brush") or {}
        check(brush.get("kind") == "paint" and str(brush.get("color", "")).startswith("#"), f"the paint brush has a colour ({brush})")
        # Paint colours vertices. The crate is a plain box (vertices at its corners only): a
        # 2 m brush in the middle of its face reaches none, so the stroke is not sent. (The
        # capture check found this: the status said "Painted the model" and the crate looked
        # as before.)
        vp = ed.state().get("viewport")
        cx, cy = vp[0] + vp[2] / 2, vp[1] + vp[3] / 2
        p0, _ = edits_of(builder, crate_file, "edits")
        ed.input({"drag": [cx - 40, cy], "to": [cx + 40, cy], "frames": 16}, {"wait": 3})
        st = ed.settled()
        check("Nothing to paint there" in (st.get("status") or {}).get("text", "") and len(edits_of(builder, crate_file, "edits")[0]) == len(p0),
              f"a paint stroke that reaches no vertex is not sent, and says why ({(st.get('status') or {}).get('text')})")
        ed.shot("crate_no_vertex")
        # The barrel has a ring of vertices every 0.75 m: the same brush paints it.
        if not check(select_path(ed, "/barrel"), "the barrel is selected from the outliner"):
            return 1
        ed.input({"key": "F"}, {"wait": 20})
        vp = ed.state().get("viewport")
        cx, cy = vp[0] + vp[2] / 2, vp[1] + vp[3] / 2
        sky = [vp[0] + 30, vp[1] + 12]
        ed.input({"move": sky}, {"wait": 3})
        ed.shot("barrel_before")
        probe = [cx - 20, cy]
        c0 = ec.patch_colour(probe)
        p0, _ = edits_of(builder, barrel_file, "edits")
        ed.input({"drag": [cx - 40, cy], "to": [cx + 40, cy], "frames": 16}, {"wait": 2})
        st = stroke_done(ed, "Painted the model")
        p1, _ = edits_of(builder, barrel_file, "edits")
        check(len(p1) - len(p0) >= 2 and all(s.get("color") == brush.get("color") for s in p1[len(p0):]),
              f"the drag appended paint strokes in the brush's colour to the recipe's edits ({len(p1) - len(p0)}; status: {(st.get('status') or {}).get('text')})")
        top = (call(builder, "world_history", limit=2).get("entries") or [{}])[0]
        check(top.get("by") == "window:owner" and top.get("via") == "owner-proxy", f"one journal entry by window:owner via the agent ({top.get('by')} via {top.get('via')})")
        report["paint"] = {"added": len(p1) - len(p0), "color": brush.get("color"), "journal": top.get("id")}
        wait_for(lambda: model_ready(builder, "/barrel"), 20)
        ed.input({"move": sky}, {"wait": 3})  # the ring off the paint
        time.sleep(1.5)
        ed.shot("barrel_painted")
        c1 = ec.patch_colour(probe)
        moved = sum(abs(a - b) for a, b in zip(c0, c1))
        report["paint"]["pixel"] = {"before": c0, "after": c1}
        check(moved >= 30 and (c1[0] - c1[2]) > (c0[0] - c0[2]) + 10, f"the paint shows: the barrel's side under the stroke went from {c0} to {c1} (warmer, toward {brush.get('color')})")
        ed.input({"key": "Ctrl+Z"}, {"wait": 4})
        back = wait_for(lambda: (lambda e: True if len(e) == len(p0) else None)(edits_of(builder, barrel_file, "edits")[0]), 20)
        check(back is True, f"Ctrl+Z takes the paint back ({len(edits_of(builder, barrel_file, 'edits')[0])} strokes, {len(p0)} before)")

        # ---- a scaled entity (review R1, M4): the brush's size is world metres, the stroke is
        # written in the entity's own units ----
        keg_file = "assets/recipes/draw_keg.json"
        if not check(select_path(ed, "/keg"), "the keg (a barrel three times the size) is selected from the outliner"):
            return 1
        ed.input({"key": "F"}, {"wait": 20})
        vp = ed.state().get("viewport")
        cx, cy = vp[0] + vp[2] / 2, vp[1] + vp[3] / 2
        k0, _ = edits_of(builder, keg_file, "edits")
        ed.input({"drag": [cx - 40, cy], "to": [cx + 40, cy], "frames": 16}, {"wait": 2})
        st = stroke_done(ed, "Painted the model")
        k1, _ = edits_of(builder, keg_file, "edits")
        radii = sorted({s.get("radius") for s in k1[len(k0):]})
        want = brush.get("radius", 0) / 3
        check(len(k1) > len(k0) and radii and all(abs(r - want) < 2e-3 for r in radii),
              f"on an entity scaled 3x the strokes are a third of the brush's radius in its own units ({radii}, brush {brush.get('radius')}; status: {(st.get('status') or {}).get('text')})")
        wait_for(lambda: model_ready(builder, "/keg"), 20)
        # ---- ui_act names the draw tool its click sent (review R1, m3: `queued` was null) ----
        err, r = raw(owner, "ui_act", surface="editor", steps=[{"click": [cx, cy - 30]}, {"wait": 2}])
        edit = (r or {}).get("edit") or {}
        check(err is None and edit.get("queued") == ["model_paint"] and edit.get("done") is True and isinstance(edit.get("journal"), int),
              f"ui_act's click with the paint brush answers with the stroke's tool and outcome ({err or edit})")

        # ---- the path tool: an agent's model_path ----
        shape = lambda k: edits_of(builder, fence_file, f"parts[{k}].shape")[0] or {}  # noqa: E731
        err, r = raw(builder, "model_path", recipe="recipes/draw_fence.json", part="wire", points=[[-5, 2], [0, 3.5], [5, 2]], smooth=3)
        wire = shape(2)
        check(err is None and r.get("part") == "wire" and wire.get("path") == [[-5, 0.6, 2], [0, 0.6, 3.5], [5, 0.6, 2]] and wire.get("smooth") == 3,
              f"an agent's model_path redraws the wire and keeps its height ({err or r}; {wire.get('path')})")
        err, _ = raw(builder, "model_path", recipe="recipes/draw_fence.json", sweep=5, points=[[0, 0], [1, 0]])
        check(err is not None and "rail, wire" in str(err), f"a sweep the recipe doesn't have is refused, naming the ones it has ({str(err)[:140]})")
        err, _ = raw(builder, "model_path", recipe="recipes/draw_fence.json", part="slab", points=[[0, 0], [1, 0]])
        check(err is not None and "not a sweep" in str(err), f"a part that isn't a sweep is refused ({str(err)[:120]})")
        err, _ = raw(builder, "model_path", recipe="recipes/draw_crate.json", points=[[0, 0], [1, 0]])
        check(err is not None and "no sweep part" in str(err), f"a recipe with no sweep is refused ({str(err)[:120]})")
        wait_for(lambda: model_ready(builder, "/fence"), 20)

        # ---- the path tool: the editor's drag ----
        if not check(select_path(ed, "/fence"), "the fence is selected from the outliner"):
            return 1
        ed.input({"key": "F"}, {"wait": 20})
        check(press(ed, "br_path") is None, "Path puts the path tool in hand")
        st = ed.settled()
        brush = st.get("brush") or {}
        check(brush.get("kind") == "path" and brush.get("sweep") == 0 and "'rail' (sweep 1 of 2)" in (st.get("status") or {}).get("text", ""),
              f"the path tool names the sweep it will redraw ({brush}; {(st.get('status') or {}).get('text')})")
        press(ed, "br_path_on")
        second = ed.settled()
        press(ed, "br_path_on")
        third = ed.settled()
        check((second.get("brush") or {}).get("sweep") == 1 and "'wire'" in (second.get("status") or {}).get("text", "") and (third.get("brush") or {}).get("sweep") == 0,
              f"pressed again it steps through the model's sweeps only ({(second.get('brush') or {}).get('sweep')}, then {(third.get('brush') or {}).get('sweep')})")
        for _ in range(4):
            press(ed, "br_r_up")  # a point every metre or so
        brush = ed.settled().get("brush") or {}
        vp = ed.state().get("viewport")
        cx, cy = vp[0] + vp[2] / 2, vp[1] + vp[3] / 2
        ed.shot("fence_before")
        # A click is no path.
        ed.input({"click": [cx, cy + 30]}, {"wait": 3})
        st = ed.settled()
        check("A path needs a drag" in (st.get("status") or {}).get("text", "") and shape(1).get("path") == FENCE["parts"][1]["shape"]["path"],
              f"a click sends nothing and says a path needs a drag ({(st.get('status') or {}).get('text')})")
        # An arc, drawn with the button held: press, moves, release.
        arc = [[cx - 170 + 340 * k / 16, cy + 40 - 90 * (1 - (2 * k / 16 - 1) ** 2)] for k in range(17)]
        steps = [{"press": arc[0]}] + [{"move": p} for p in arc[1:]] + [{"wait": 1}]
        if a.record:
            rec = WindowRecorder(hwnd, fps=10)
            rec.start()
            time.sleep(1.0)
            steps = [{"press": arc[0]}] + [x for p in arc[1:] for x in ({"move": p}, {"wait": 5})] + [{"wait": 1}]
        ed.input(*steps)
        ed.shot("fence_sketching")
        ed.input({"release": arc[-1]}, {"wait": 2})
        st = stroke_done(ed, "Sketched the path")
        rail = shape(1)
        path = rail.get("path") or []
        bends = len({round(p[2], 2) for p in path}) >= 3
        check(len(path) >= 4 and all(p[1] == 1.5 for p in path) and bends, f"the drag redrew the rail's path, at its own height ({len(path)} points; status: {(st.get('status') or {}).get('text')})")
        check(rail.get("smooth") == round(brush.get("amount", 0) * 8) and rail.get("scales") == [1, 1],
              f"smoothing follows the Strength control, and the per-point scales are reset to their ends (smooth {rail.get('smooth')}, scales {rail.get('scales')})")
        top = (call(builder, "world_history", limit=2).get("entries") or [{}])[0]
        check(top.get("by") == "window:owner" and top.get("via") == "owner-proxy", f"one journal entry by window:owner via the agent ({top.get('by')} via {top.get('via')})")
        report["path"] = {"points": len(path), "smooth": rail.get("smooth"), "journal": top.get("id")}
        wait_for(lambda: model_ready(builder, "/fence"), 20)
        time.sleep(1.5)
        ed.shot("fence_sketched")
        if a.record:
            report["path_clip"] = rec.stop(out / "path_clip.png")
            print(f"recorded {out / 'path_clip.png'}: {report['path_clip']}")
        ed.input({"key": "Ctrl+Z"}, {"wait": 4})
        back = wait_for(lambda: True if shape(1).get("path") == FENCE["parts"][1]["shape"]["path"] else None, 20)
        check(back is True and shape(1).get("scales") == [1, 0.6, 1], f"Ctrl+Z takes the sketch back, scales and all ({shape(1).get('path')}, {shape(1).get('scales')})")
        # A path ends where the pointer was let go, not at its last sample: the same two-point
        # sketch, let go 12 pixels further on (less than the distance between samples), ends
        # somewhere else.
        ends = []
        for extra in (0, 12):
            wait_for(lambda: model_ready(builder, "/fence"), 20)
            mid = [cx + extra, cy - 50]
            steps = [{"press": arc[0]}, {"move": [cx, cy - 50]}] + ([{"move": mid}] if extra else []) + [{"release": mid}, {"wait": 2}]
            ed.input(*steps)
            stroke_done(ed, "Sketched the path")
            got = wait_for(lambda: (lambda q: q if q and q != FENCE["parts"][1]["shape"]["path"] else None)(shape(1).get("path")), 20) or []
            ends.append(got[-1] if got else None)
            ed.input({"key": "Ctrl+Z"}, {"wait": 4})
            wait_for(lambda: True if shape(1).get("path") == FENCE["parts"][1]["shape"]["path"] else None, 20)
        moved = None if None in ends else ((ends[0][0] - ends[1][0]) ** 2 + (ends[0][2] - ends[1][2]) ** 2) ** 0.5
        check(moved is not None and 0.02 < moved < brush.get("radius", 0) * 2, f"a path ends where the pointer was let go: 12 pixels on, its end is {moved} m further ({ends})")

        # ---- the window's focus: an agent's stroke holds through its loss, the human's is let go ----
        # (A run lost an agent's stroke when someone at the desk clicked into another window:
        # the window's focus loss released the button the agent's frames held.)
        original = FENCE["parts"][1]["shape"]["path"]
        mid = [cx, cy - 50]
        wait_for(lambda: model_ready(builder, "/fence"), 20)
        ed.input({"press": arc[0]}, {"move": mid}, {"wait": 1})
        ec.user32.PostMessageW(hwnd, 0x0008, 0, 0)  # WM_KILLFOCUS: the human turned to another window
        time.sleep(0.5)
        ec.user32.PostMessageW(hwnd, 0x0007, 0, 0)  # WM_SETFOCUS: and came back
        ed.input({"release": mid}, {"wait": 2})
        st = stroke_done(ed, "Sketched the path")
        got = wait_for(lambda: (lambda q: q if q and q != original else None)(shape(1).get("path")), 20)
        check(got is not None and "Sketched the path" in (st.get("status") or {}).get("text", ""),
              f"an agent's stroke holds through the window's focus loss and lands ({(st.get('status') or {}).get('text')})")
        ed.input({"key": "Ctrl+Z"}, {"wait": 4})
        wait_for(lambda: True if shape(1).get("path") == original else None, 20)
        wait_for(lambda: model_ready(builder, "/fence"), 20)
        # The human's own: real mouse messages, then the focus goes.
        park_cursor(hwnd)
        lp = lambda p: (int(p[1]) << 16) | (int(p[0]) & 0xFFFF)  # noqa: E731
        ec.user32.PostMessageW(hwnd, 0x0200, 0, lp(arc[0]))
        time.sleep(0.05)
        ec.user32.PostMessageW(hwnd, 0x0201, 1, lp(arc[0]))
        for k in range(1, 9):
            time.sleep(0.03)
            ec.user32.PostMessageW(hwnd, 0x0200, 1, lp([arc[0][0] + (mid[0] - arc[0][0]) * k / 8, arc[0][1] + (mid[1] - arc[0][1]) * k / 8]))
        time.sleep(0.2)
        ec.user32.PostMessageW(hwnd, 0x0008, 0, 0)
        time.sleep(0.5)
        st = ed.settled()
        ec.user32.PostMessageW(hwnd, 0x0007, 0, 0)
        ec.user32.PostMessageW(hwnd, 0x0202, 0, lp(mid))
        time.sleep(0.5)
        check("Stroke let go (the window lost focus)" in (st.get("status") or {}).get("text", "") and shape(1).get("path") == original,
              f"the human's own stroke is let go, unsent, when the window loses focus, and the status says why ({(st.get('status') or {}).get('text')})")
        # An agent that pressed and went quiet: the human's own press takes the viewport back
        # (they are never locked out by a hold nobody releases).
        ed.input({"press": arc[0]}, {"move": mid}, {"wait": 1})
        time.sleep(5.6)
        ec.os_click(hwnd, mid[0], mid[1])
        time.sleep(0.4)
        st = ed.settled()
        check("went quiet" in (st.get("status") or {}).get("text", "") and shape(1).get("path") == original,
              f"an agent's hold that went quiet ends when the human presses: its stroke is let go, unsent ({(st.get('status') or {}).get('text')})")
        ed.input({"release": mid}, {"wait": 2})  # the agent's late release finds nothing to send
        check(shape(1).get("path") == original, "... and its late release sends nothing")
        # The human's own hand holds the viewport: an agent's press is not taken, and its call
        # says so (review R2: it answered as fed and its drag was a silent no-op).
        park_cursor(hwnd)
        ec.user32.PostMessageW(hwnd, 0x0200, 0, lp(arc[0]))
        time.sleep(0.05)
        ec.user32.PostMessageW(hwnd, 0x0201, 1, lp(arc[0]))
        time.sleep(0.3)
        err, r = raw(owner, "editor_input", steps=[{"press": mid}, {"move": arc[-1]}, {"release": arc[-1]}, {"wait": 2}])
        refused = (r or {}).get("presses_refused") or {}
        ec.user32.PostMessageW(hwnd, 0x0202, 0, lp(arc[0]))
        time.sleep(0.5)
        check(err is None and refused.get("count", 0) >= 1 and refused.get("retry") is True and shape(1).get("path") == original,
              f"an agent's press while the human holds the viewport is not taken, and its call says so ({err or refused})")
        ed.input({"key": "Escape"}, {"wait": 2})

        # ---- ui_act on the editor answers with how its edit went ----
        # (Carried from the editor-swarm review: it answered before the edit was sent.)
        err, r = raw(owner, "ui_act", surface="editor", steps=[{"click": "tb_delete"}, {"wait": 2}])
        edit = (r or {}).get("edit") or {}
        there = lambda: [e for e in ((raw(builder, "world_query", path_prefix="/fence", fields=["path"], limit=4)[1] or {}).get("items") or []) if e.get("path") == "/fence"]  # noqa: E731
        gone = not there()
        check(err is None and edit.get("done") is True and isinstance(edit.get("journal"), int) and "world_batch" in (edit.get("queued") or []) and gone,
              f"ui_act's press on Delete answers with the edit's outcome: done, its journal entry ({err or edit})")
        err, r = raw(owner, "ui_act", surface="editor", steps=[{"click": "tb_undo"}, {"wait": 2}])
        edit = (r or {}).get("edit") or {}
        back = there()
        check(err is None and edit.get("done") is True and bool(back), f"... and its press on Undo, with the fence back ({err or edit})")

        # ---- only the owner drives it ----
        err, _ = raw(builder, "editor_input", steps=[{"drag": [cx - 40, cy], "to": [cx + 40, cy]}])
        check(err is not None, f"a builder can't drive the brush ({(err or 'it ran')[:100]})")
        report["screens"] = ed.shots
        (out / "report.json").write_text(json.dumps(report, indent=1))
    log = (out / "engine.log").read_text(errors="replace")
    check("(0 validation errors)" in log, "validation errors at shutdown: 0")
    print(f"\nreport: {out / 'report.json'}")
    if ec.FAILS:
        print(f"FAIL: {len(ec.FAILS)} check(s) failed")
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
