#!/usr/bin/env python3
"""Text check (M5 text lane, T2-T4): in-world text drawn from glyph outlines, typed into over MCP.

    cargo build -p specula
    python scripts/text_check.py                    # every adapter, port 7510
    python scripts/text_check.py --adapters integrated

On each adapter, in a headless engine:

1. Paragraphs in Latin, Cyrillic and Greek (bundled SIL OFL faces), a rich-text line (bold,
   colour), a CJK paragraph from the system's fallback face, a tiny label (UI size: a few pixels
   per em) and a huge inscription, on stone walls under a moon.
2. Captures from the front, close up (letters filling the frame), at a grazing angle and from far
   away; each must show the text's subjects on screen with visible pixels in sight's ID buffer,
   the frame record listing the draws, contrast inside each text box, and no missing glyphs.
3. Typing over MCP (text_edit): insert, word selection, copy and paste, an IME composition and its
   commit, backspace; the answers carry the caret, selection and clipboard, and a capture shows
   the cyan selection and the caret.
4. Budgets: the 2D pass's GPU time with every text drawn, the glyph store's GPU memory (capped),
   zero validation errors at shutdown.

Also checks the bundled fonts' SHA-256 against fonts/MANIFEST.toml. Captures, packets and logs go
to target/text-check/. Open the PNGs and look at them. Standard library only; ASCII output.
"""
import argparse
import hashlib
import json
import os
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402
from two_d_check import read_png, pixel  # noqa: E402

DEFAULT_PORT = 7510

LATIN = "Here lies Ada of the north gate, who held the wall for ten long waves and never once looked back."
CYRILLIC = "Здесь покоится Иван, сторож ворот: съешь же ещё этих мягких булок."
GREEK = "Ἐνθάδε κεῖται Νίκη. Ξεσκεπάζω τὴν ψυχοφθόρα βδελυγμία."
CJK = "静かな墓地に月が昇る。風が吹き、鐘が鳴る。"
RICH = "Press [icon=pad_a] to [b]build[/b] a [color=#3fe0d0]ward[/color]"

problems = []


def check(cond, what):
    what = what.encode("ascii", "backslashreplace").decode("ascii")
    print(("  ok    " if cond else "  FAIL  ") + what)
    if not cond:
        problems.append(what)


def fonts_match_manifest():
    """Every bundled font file's size and SHA-256 match fonts/MANIFEST.toml."""
    fonts = ROOT / "crates" / "specula-text" / "fonts"
    entries, cur = [], {}
    for line in (fonts / "MANIFEST.toml").read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line == "[[font]]":
            cur = {}
            entries.append(cur)
        elif "=" in line and not line.startswith("#"):
            k, v = line.split("=", 1)
            cur[k.strip()] = v.strip().strip('"')
    check(len(entries) >= 3, f"the manifest lists {len(entries)} fonts")
    for e in entries:
        data = (fonts / e["file"]).read_bytes()
        check(hashlib.sha256(data).hexdigest() == e["sha256"] and len(data) == int(e["bytes"]), f"{e['file']}: size and SHA-256 match the manifest")
        check((fonts / e["licence_file"]).read_text(encoding="utf-8").find("SIL Open Font License") >= 0, f"{e['file']}: its OFL text ships beside it")


def spawn_text(m, path, text, pos, rot=(0, 0, 0), color="#ece6d6", **fields):
    parent, name = path.rsplit("/", 1)
    args = {"shape": "text", "name": name, "position": list(pos), "rotation_deg": list(rot), "color": color, "text": dict(text=text, **fields)}
    if parent:
        args["parent"] = parent
    return m.json("world_spawn", **args)


def build(m):
    m.call("world_env", sun_direction=[-0.3, 0.6, 0.75], sun_intensity=0.9, ambient_intensity=0.35, tonemap=True,
           background="#10131a", sky={"enabled": False})
    m.json("world_spawn", shape="group", name="text")
    # Stone walls the paragraphs are carved into (their fronts face +Z at z = 0).
    for name, x in (("wall_a", -3.2), ("wall_b", 0.0), ("wall_c", 3.2)):
        m.json("world_spawn", shape="cube", name=name, parent="/text", position=[x, 1.2, -0.1], scale=[3.0, 2.4, 0.2], color="#8c96a3")
    wide = dict(size=0.16, max_width=2.6, align="left", lit=True)
    spawn_text(m, "/text/latin", LATIN, (-3.2, 1.5, 0.003), color="#2b3038", **wide)
    spawn_text(m, "/text/cyrillic", CYRILLIC, (0.0, 1.5, 0.003), color="#2b3038", **wide)
    spawn_text(m, "/text/greek", GREEK, (3.2, 1.5, 0.003), color="#2b3038", font="display", **wide)
    spawn_text(m, "/text/rich", RICH, (0.0, 0.45, 0.003), color="#2b3038", rich=True, size=0.14, lit=True)
    spawn_text(m, "/text/cjk", CJK, (-3.2, 0.4, 0.003), color="#2b3038", locale="ja", size=0.14, max_width=2.6, lit=True)
    # A label at UI size seen from afar, and a huge inscription on the ground in front.
    spawn_text(m, "/text/tiny", "wave 3/10", (3.2, 0.3, 0.003), color="#ece6d6", size=0.05)
    spawn_text(m, "/text/huge", "SEPULCRA", (0.0, 0.0, 5.0), rot=(-90, 0, 0), color="#3fe0d0", font="display", size=3.0)
    # The editable field.
    spawn_text(m, "/text/field", "Here lies", (0.0, 3.0, 0.003), color="#ece6d6", size=0.2, align="left", anchor=[0, 0.5])


def subjects(f):
    return {s.get("path", s.get("name")): s for s in f.get("subjects", [])}


def save(out, adapter, name, f):
    src = Path(f["image"]["path"])
    dst = out / f"{adapter}_{name}.png"
    dst.write_bytes(src.read_bytes())
    (out / f"{adapter}_{name}.json").write_text(json.dumps(f, indent=1))
    for w in f.get("warnings", []):
        if "vulkan" in w.lower():
            problems.append(f"{name}: {w}")
    return read_png(dst)


def contrast(img, rect):
    """Max minus min luma inside a screen rect [x, y, w, h]."""
    w, h = img[0], img[1]
    x0, y0 = max(0, int(rect[0])), max(0, int(rect[1]))
    x1, y1 = min(w, int(rect[0] + rect[2])), min(h, int(rect[1] + rect[3]))
    lum = [sum(pixel(img, x, y)[:3]) // 3 for y in range(y0, y1, 2) for x in range(x0, x1, 2)]
    return (max(lum) - min(lum)) if lum else 0


def drawn_paths(f):
    rec = f.get("drawn") or f.get("frame_record") or {}
    return json.dumps(rec)


def run_adapter(exe, adapter, out, port):
    log = out / f"engine_{adapter}.log"
    with Engine(exe, port, log, "--headless", "--adapter", adapter, "--captures", str(out / "captures"), cwd=str(out)) as engine:
        with engine.mcp("text_check") as m:
            m.hello("text-check", role="builder")
            info = m.json("engine_info")
            print(f"adapter {adapter}: {info['gpu']['name']} (validation {'on' if info.get('validation') else 'OFF'})")
            build(m)
            frame_args = dict(width=1280, height=720, max_subjects=64, time_s=0.0)
            # The CJK fallback loads on its own thread: ask until it's in (its glyphs show).
            deadline = time.time() + 30
            while True:
                f = m.frame(eye=[0, 1.4, 6.5], target=[0, 1.2, 0], **frame_args)
                cjk = subjects(f).get("/text/cjk", {})
                if "missing_glyphs" not in json.dumps(cjk) or time.time() > deadline:
                    break
                time.sleep(0.5)
            img = save(out, adapter, "front", f)
            subs = subjects(f)
            for p in ("/text/latin", "/text/cyrillic", "/text/greek", "/text/rich", "/text/cjk", "/text/field"):
                s = subs.get(p)
                check(s is not None and s.get("on_screen"), f"{p} is a subject on screen")
                if not s:
                    continue
                facts = s.get("two_d", {})
                check(facts.get("lines", 0) >= 1 and "missing_glyphs" not in facts, f"{p}: {facts.get('lines')} lines, no missing glyphs ({facts.get('missing_glyphs', '')!r})")
                check((s.get("visible_px") or 0) > 200, f"{p}: {s.get('visible_px')} visible px in sight's ID buffer")
                if s.get("screen"):
                    c = contrast(img, s["screen"])
                    check(c > 40, f"{p}: carved letters show (contrast {c} in its box)")
            check(subs.get("/text/latin", {}).get("two_d", {}).get("lines", 0) >= 3, "the Latin paragraph wraps by line-break rules")
            rich = subs.get("/text/rich", {}).get("screen")
            if rich:
                x0, y0, w, h = (int(v) for v in rich)
                cyan = sum(1 for y in range(max(0, y0), min(720, y0 + h)) for x in range(max(0, x0), min(1280, x0 + w))
                           if (lambda p: p[1] > p[0] + 50 and p[2] > p[0] + 40)(pixel(img, x, y)))
                check(cyan > 30, f"rich text: 'ward' is cyan ({cyan} px), over dark carved text")
            # Close up: a few letters fill the frame (edges stay sharp, no atlas to magnify).
            f = m.frame(eye=[-3.9, 1.62, 0.35], target=[-3.9, 1.62, 0.0], **frame_args)
            img = save(out, adapter, "closeup", f)
            s = subjects(f).get("/text/latin", {})
            check((s.get("visible_px") or 0) > 50000, f"close up the letters cover {s.get('visible_px')} px")
            dark = sum(1 for y in range(0, 720, 4) for x in range(0, 1280, 4) if sum(pixel(img, x, y)[:3]) < 200)
            soft = sum(1 for y in range(0, 720, 4) for x in range(0, 1280, 4) if 200 <= sum(pixel(img, x, y)[:3]) < 330)
            check(dark > 2000 and soft < dark, f"close up: solid strokes ({dark} dark samples), thin edges ({soft} in between)")
            # Grazing: along the wall, 8 degrees off its face.
            f = m.frame(eye=[3.5, 1.5, 0.45], target=[-3.5, 1.4, 0.0], **frame_args)
            save(out, adapter, "grazing", f)
            s = subjects(f).get("/text/cyrillic", {})
            check((s.get("visible_px") or 0) > 100, f"at a grazing angle the Cyrillic paragraph still shows ({s.get('visible_px')} px)")
            # Far: the tiny label is a few pixels per em; the huge one fills the ground.
            f = m.frame(eye=[0, 6, 18], target=[0, 0.5, -4], **frame_args)
            save(out, adapter, "far", f)
            subs = subjects(f)
            check((subs.get("/text/huge", {}).get("visible_px") or 0) > 5000, f"the 3 m inscription shows ({subs.get('/text/huge', {}).get('visible_px')} px)")
            check(subs.get("/text/tiny", {}).get("on_screen"), "the tiny label is on screen")

            # Typing over MCP.
            r = m.json("text_edit", path="/text/field", ops=[{"key": "end"}, {"insert": " Ada"}, {"key": "left", "unit": "word", "select": True}, {"key": "copy"}])
            check(r["text"] == "Here lies Ada" and r["selected"] == "Ada" and r.get("copied") == "Ada", f"typed and selected a word: {r['text']!r}, selected {r['selected']!r}, copied {r.get('copied')!r}")
            r = m.json("text_edit", path="/text/field", ops=[{"key": "right"}, {"insert": ", beloved"}, {"key": "home"}, {"key": "right", "unit": "word", "select": True}])
            check(r["text"] == "Here lies Ada, beloved" and r["selected"] == "Here ", f"home, then a word selected: {r['selected']!r}")
            f = m.frame(eye=[1.0, 3.0, 2.2], target=[1.0, 3.0, 0.0], **frame_args)
            img = save(out, adapter, "typing", f)
            cyan = sum(1 for y in range(0, 720, 2) for x in range(0, 1280, 2) if (lambda p: p[1] > p[0] + 40 and p[2] > p[0] + 40)(pixel(img, x, y)))
            check(cyan > 100, f"the selection shows in cyan ({cyan} samples)")
            r = m.json("text_edit", path="/text/field", ops=[{"key": "end"}, {"compose": "やま", "caret": 6}])
            check(r.get("composing", {}).get("text") == "やま" and r["text"] == "Here lies Ada, beloved", "an IME composition shows without entering the text")
            f = m.frame(eye=[1.4, 3.0, 2.4], target=[1.4, 3.0, 0.0], **frame_args)
            save(out, adapter, "composing", f)
            r = m.json("text_edit", path="/text/field", ops=[{"commit": " 山"}, {"key": "backspace"}, {"key": "backspace"}, {"insert": "!"}])
            check(r["text"] == "Here lies Ada, beloved!" and "composing" not in r, f"commit then backspace: {r['text']!r}")
            hist = m.json("world_history", path_prefix="/text/field", limit=10)
            check(len(hist.get("entries", hist.get("history", []))) >= 4, "every text_edit call is one journaled change")
            try:
                m.json("text_edit", path="/text/wall_a", ops=[{"insert": "x"}])
                check(False, "text_edit refuses a cube")
            except RuntimeError as e:
                check("not a text entity" in str(e), "text_edit refuses a non-text entity with a reason")
            try:
                m.json("text_edit", path="/text/field", ops=[{"key": "teleport"}])
                check(False, "a bad key is refused")
            except RuntimeError as e:
                check("ops[0]" in str(e), "a bad op is refused with its index")

            # Budgets.
            perf = m.json("perf_frame")
            (out / f"{adapter}_perf.json").write_text(json.dumps(perf, indent=1))
            mem = m.json("perf_memory")
            (out / f"{adapter}_memory.json").write_text(json.dumps(mem, indent=1))
            glyph_bytes = json.dumps(mem).count('"glyphs"')
            check(glyph_bytes >= 1, "perf_memory reports the glyph store")
            # The packet monitor checks every subject's claims against the frame record: drawn
            # text must have been submitted by the colour pass.
            inc = m.json("trace_incidents")
            packet = [i for i in inc.get("incidents", []) if "packet" in json.dumps(i)]
            (out / f"{adapter}_incidents.json").write_text(json.dumps(inc, indent=1))
            check(not packet, f"the packet monitor found no subject the frame record contradicts ({len(packet)}: {json.dumps(packet)[:300]})")
    errors = engine.validation_errors()
    check(errors == 0, f"shutdown with {errors} validation errors")


def sepulcra(exe, adapter, out, port, frames):
    """Sepulcra on `adapter` (the iGPU for the budgets), windowed: the epitaphs and the title stone
    in the game view, and the HUD's text cost while the game runs."""
    log = out / f"sepulcra_{adapter}.log"
    args = ["--adapter", adapter, "--size", "1280x720", "--captures", str(out / "captures"), "--lead", "text-check", "--", "--seed", "7", "--mute"]
    with Engine(exe, port, log, *args, cwd=str(out)) as engine:
        with engine.mcp("text_check") as m:
            for _ in range(240):  # the game loads its models first: busy until then
                r = m.request("tools/call", {"name": "agent_hello", "arguments": {"name": "text-check", "role": "lead"}})
                if not r.get("isError"):
                    break
                time.sleep(0.5)
            gpu = m.json("engine_info")["gpu"]["name"]
            print(f"sepulcra on {adapter}: {gpu}")
            q = m.json("world_query", tags=["epitaph"], fields=["path", "world_position", "text"], limit=64)["items"]
            check(len(q) >= 6, f"{len(q)} headstones carry epitaphs")
            t = m.json("world_query", tags=["title_stone"], fields=["path", "world_position", "shape"], limit=8)["items"]
            check(any(i.get("shape") == "text" for i in t), "the title stone carries its words")
            if q:
                p = q[0]["world_position"]
                for name, eye in (("epitaph_close", [p[0] + 0.15, p[1] + 0.15, p[2] + 0.9]), ("epitaph_wide", [p[0] + 2.5, p[1] + 2.2, p[2] + 5.0])):
                    f = m.frame(width=1280, height=720, eye=eye, target=p, max_subjects=64, time_s=0.0)
                    save(out, adapter, f"sepulcra_{name}", f)
                    seen = [s for s in f["subjects"] if s.get("path", "").endswith("/epitaph") and (s.get("visible_px") or 0) > 0]
                    check(seen, f"{name}: {len(seen)} epitaphs show ({[s.get('visible_px') for s in seen][:4]} px)")
            for i in t:
                if i.get("shape") == "text" and i["path"].endswith("/title"):
                    p = i["world_position"]
                    f = m.frame(width=1280, height=720, eye=[p[0] + 0.8, p[1] + 1.2, p[2] + 4.5], target=p, max_subjects=64, time_s=0.0)
                    save(out, adapter, "sepulcra_title", f)
                    s = subjects(f).get(i["path"], {})
                    check((s.get("visible_px") or 0) > 500, f"the title shows ({s.get('visible_px')} px)")
            keyboard(engine, m, adapter, out)
            # The game view with its HUD (bundled faces) and the HUD's cost in real frames.
            f = m.frame(width=1280, height=720, include_ui=True, max_subjects=8)
            save(out, adapter, "sepulcra_hud", f)
            m.call("perf_zones", enabled=True)
            time.sleep(1.0)
            cap = m.json("perf_capture", frames=frames, trace=False, label=f"text_{adapter}")
            zones = {z["path"]: z for z in m.json("perf_zones", min_ms=0.0, limit=200).get("zones", []) if z.get("thread") == "main"}
            hud = next((z for p_, z in zones.items() if p_.endswith("game_overlay")), {})
            passes = cap.get("gpu_passes_ms") or {}
            (out / f"sepulcra_{adapter}_perf.json").write_text(json.dumps({"capture": cap, "zones": zones}, indent=1))
            overlay = (passes.get("overlay") or {}).get("p95")
            sprites = (passes.get("sprites") or {}).get("p95")
            print(f"  frames {cap.get('frames')}, GPU p95 {(cap.get('gpu_ms') or {}).get('p95')} ms; overlay p95 {overlay} ms, sprites p95 {sprites} ms; "
                  f"HUD CPU (game_overlay) mean {hud.get('mean_ms')} p95 {hud.get('p95_ms')} ms")
            check(hud.get("p95_ms") is not None and hud["p95_ms"] < 0.5, f"HUD text CPU p95 {hud.get('p95_ms')} ms < 0.5 ms")
            check(overlay is not None and overlay < 0.3, f"HUD GPU (overlay pass) p95 {overlay} ms < 0.3 ms")
    errors = engine.validation_errors()
    check(errors == 0, f"sepulcra shutdown with {errors} validation errors")


def keyboard(engine, m, adapter, out):
    """The human's keyboard in the real window (review of m5-text, major 2): a builder can't point
    it at a text; the lead can, and the window shows a banner naming the text until it stops."""
    import window_check as wc
    m.json("world_spawn", name="text_check_field", shape="text", position=[0, 40, 0], color="#ece6d6",
           text={"text": "Name: ", "size": 0.6, "anchor": [0.5, 0.5], "editing": {"caret": 6}})
    m.json("window_view", eye=[0, 40, 6], target=[0, 40, 0])
    time.sleep(1.0)
    with engine.mcp("text_check_builder") as b:
        b.hello("mallory", role="builder")
        r = b.request("tools/call", {"name": "text_edit", "arguments": {"path": "/text_check_field", "ops": [], "window_focus": True}})
        body = json.loads(next((c["text"] for c in r.get("content", []) if c.get("type") == "text"), "{}"))
        check(r.get("isError") and body.get("error") == "lead_only", f"a builder can't take the human's keyboard ({body.get('error')})")
    hwnd = wc.find_window(engine.proc.pid)
    before = out / f"sepulcra_{adapter}_keyboard_off.png"
    wc.screenshot(hwnd, before)
    r = m.json("text_edit", path="/text_check_field", ops=[], window_focus=True)
    check(r.get("window_focus") is True, "the lead points the keyboard at a shown text")
    time.sleep(0.6)
    after = out / f"sepulcra_{adapter}_keyboard_on.png"
    wc.screenshot(hwnd, after)
    # The banner: a dark box with a cyan edge just above the text while the keyboard is pointed
    # (the game's own interface has some cyan too: counted against the shot before).
    def cyan(path):
        img = read_png(path)
        return sum(1 for y in range(0, img[1], 2) for x in range(0, img[0], 2) if (lambda p: p[1] > p[0] + 60 and p[2] > p[0] + 50)(pixel(img, x, y)))
    added = cyan(after) - cyan(before)
    check(added > 100, f"the window shows the typing banner ({added} more cyan edge samples than before)")
    # Over the game's interface (review R3 of m5-text, lead direction 4): the field low in the
    # window, so its banner lands on Sepulcra's bottom panel. The banner is the last of the
    # frame's overlay: it draws over the panel, opaque, its cyan edge whole. Shots back to back.
    # (The operator stops the typing before moving the field: their own world_set drops it too.)
    m.json("text_edit", path="/text_check_field", ops=[], window_focus=False)
    m.json("world_set", path="/text_check_field", position=[0, 36.6, 0])
    time.sleep(0.6)
    under = out / f"sepulcra_{adapter}_keyboard_hud_off.png"
    wc.screenshot(hwnd, under)
    m.json("text_edit", path="/text_check_field", ops=[], window_focus=True)
    time.sleep(0.6)
    over = out / f"sepulcra_{adapter}_keyboard_hud_on.png"
    wc.screenshot(hwnd, over)
    typing = (m.json("engine_info", response_format="detailed").get("window") or {}).get("typing") or {}
    check((typing.get("banner") or {}).get("last") is True, f"engine_info: the banner is the last of the frame's overlay ({json.dumps(typing)[:160]})")

    def cyan_low(path):
        img = read_png(path)
        return sum(1 for y in range(img[1] * 3 // 4, img[1], 2) for x in range(0, img[0], 2) if (lambda p: p[1] > p[0] + 60 and p[2] > p[0] + 50)(pixel(img, x, y)))
    low = cyan_low(over) - cyan_low(under)
    check(low > 100, f"the banner draws over the game's bottom panel ({low} more cyan edge samples in the window's lowest quarter)")
    m.json("world_set", path="/text_check_field", visible=False)
    time.sleep(0.6)
    trace = json.dumps(m.json("trace_query", code="text.window_focus_dropped", limit=5))
    check("hidden" in trace, "hiding the text took the keyboard off it (text.window_focus_dropped: hidden)")
    m.json("world_delete", path="/text_check_field")


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--exe", type=Path, default=Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target") / "debug" / "specula.exe")
    p.add_argument("--adapters", default="discrete,integrated")
    p.add_argument("--out", type=Path, default=ROOT / "target" / "text-check")
    p.add_argument("--port", type=int, default=DEFAULT_PORT)
    p.add_argument("--sepulcra", type=Path, default=None, help="sepulcra-app.exe (release for the budgets): also check the game on --sepulcra-adapter")
    p.add_argument("--sepulcra-adapter", default="integrated")
    p.add_argument("--frames", type=int, default=300)
    p.add_argument("--skip-engine", action="store_true", help="only the Sepulcra phase")
    args = p.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    print("fonts:")
    fonts_match_manifest()
    if not args.skip_engine:
        for a in args.adapters.split(","):
            run_adapter(args.exe, a, out, args.port)
    if args.sepulcra:
        sepulcra(args.sepulcra, args.sepulcra_adapter, out, args.port + 1, args.frames)
    print()
    print("text check: " + ("clean" if not problems else f"{len(problems)} problem(s)"))
    for pr in problems:
        print("  - " + pr)
    print(f"captures in {out}")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()
