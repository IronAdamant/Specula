#!/usr/bin/env python3
"""Accessibility and Sepulcra's shell, end to end (M5 lane m5-a11y; docs/design/ACCESSIBILITY.md).

Sepulcra windowed on the chosen adapter (the iGPU by default), its settings in a scratch folder
the operator names (--settings: there agents' changes are saved too), audio on the null device
(so the play log and captions exist without a speaker):

1. **Title.** The game opens on the title screen, which holds the run (the tick stays).
2. **Saved settings.** play_settings sets a colour theme, text size, captions and subtitle size;
   the settings file appears in the named folder with them. A colour theme keeps the dead, magic
   and warnings apart for its deficiency (play_settings colours) and passes ui_check.
3. **Rebinding.** An agent's press of a rebind button is refused (the game never listens for the
   player's next key for an agent); play_settings {bind} binds call_wave to G by name.
4. **ui_check at every setting:** every colour theme x every text size, on the Deck, 1080p, 4K
   and a 175% laptop window at interface sizes 0.8, 1 and 1.5 (the player's smaller choices are
   exempt from the Deck's legibility floor), on the HUD, the accessibility screen, the title and
   the pause menu with the settings.
5. **Handheld profile.** The window's display given a 7" panel (ui_act display.diagonal_in, the
   owner's): the game sees a Steam Deck and its Auto interface size becomes 1.15. On a 175%
   laptop display the largest sizes are limited by the window and greyed out on the
   accessibility screen (ui_frame shows them disabled).
6. **Intro with subtitles and captions.** New run from the title starts the intro: its audio cue
   plays on the engine's audio service as timeline:<path> (audio_played), its subtitle lines are
   drawn (ui_tree: the subtitles screen and its text; a capture), the cue is captioned.
7. **Gamepad pause.** Start (ui_act, gamepad) opens the pause menu and pauses; Start again closes it
   and resumes.
8. **Restart.** A new engine with the same settings folder: the settings and the binding are back.
   Quit on the title, clicked with a real window message (the player's own input), stops the
   engine; zero validation errors.
9. **Headless.** Without --settings a headless engine keeps no settings.

Usage: cargo build --release -p sepulcra-app && python scripts/a11y_check.py [--adapter integrated]
       [--port 7630] [--out DIR]
Output (captures, logs, summary.json) goes to target/a11y-check/<adapter>/. Look at every PNG.
Standard library only; output ASCII. Exit 0 = everything held.
"""
import argparse
import json
import os
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
GAME = TARGET / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")

FAILURES = []
VISIONS = ["standard", "deuteranopia", "protanopia", "tritanopia", "high_contrast"]
TEXT_SIZES = [0.9, 1.0, 1.15, 1.3, 1.5]
LAPTOP = {"name": "laptop_1280x720_os175", "size": [1280, 720], "os_scale": 1.75}
TARGETS = ["deck", "1080p", "4k", LAPTOP]
SCALES = [0.8, 1.0, 1.5]
SETS = [["hud"], ["access"], ["title"], ["hud", "pause", "settings"]]


def check(cond, what):
    print(("  ok   " if cond else "  FAIL ") + what)
    if not cond:
        FAILURES.append(what)
    return cond


def wait_for(f, seconds, every=0.2):
    end = time.time() + seconds
    while time.time() < end:
        v = f()
        if v:
            return v
        time.sleep(every)
    return None


def err_of(mcp, tool, **args):
    """The structured error of a call that must fail (None if it succeeded)."""
    r = mcp.request("tools/call", {"name": tool, "arguments": args})
    if not r.get("isError"):
        return None
    t = r["content"][0]["text"]
    try:
        return json.loads(t)
    except ValueError:
        return {"error": "text", "message": t}


def act(mcp, args):
    """ui_act, sent again with its `rest` until it finishes."""
    j = {}
    for _ in range(50):
        r = mcp.request("tools/call", {"name": "ui_act", "arguments": args})
        j = json.loads(r["content"][0]["text"])
        if r.get("isError") or not j.get("rest"):
            return j
        args = dict(args, steps=j["rest"])
    return j


def top_screen(mcp):
    t = mcp.json("ui_tree", limit=1, fresh=True)
    screens = [s.get("name") for s in t.get("screens", []) if s.get("visible", True)]
    return screens[-1] if screens else None, t


def screens_of(mcp):
    t = mcp.json("ui_tree", limit=200, fresh=True)
    return t


def find_window(pid):
    if sys.platform != "win32":
        return None
    import ctypes
    from ctypes import wintypes
    user32 = ctypes.windll.user32
    found = []
    proto = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

    def each(hwnd, _):
        p = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(p))
        if p.value == pid and user32.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True
    user32.EnumWindows(proto(each), 0)
    return found[0] if found else None


def post_click(hwnd, x, y):
    """A left click at client pixel (x, y), posted to the window as the OS delivers a player's."""
    import ctypes
    user32 = ctypes.windll.user32
    lp = (int(y) << 16) | (int(x) & 0xFFFF)
    user32.PostMessageW(hwnd, 0x0200, 0, lp)  # WM_MOUSEMOVE
    time.sleep(0.15)
    user32.PostMessageW(hwnd, 0x0201, 1, lp)  # WM_LBUTTONDOWN
    time.sleep(0.15)
    user32.PostMessageW(hwnd, 0x0202, 0, lp)  # WM_LBUTTONUP


def shot(mcp, out, name):
    """A capture of what the player sees (the window's camera, the interface drawn)."""
    cam = mcp.json("engine_info")["window"]["camera"]
    args = {"width": 1280, "height": 800, "include_ui": True}
    if isinstance(cam, dict):
        args.update(eye=cam["eye"], target=cam["target"], fov_y_deg=cam.get("fov_y_deg", 45))
    f = mcp.json("sight_frame", **args)
    src = f.get("image", {}).get("path")
    if src:
        shutil.copy(src, out / f"{name}.png")
        # Beside the picture: the engine's own account of the interface in it (every widget's
        # rect, enabled and focused flags, the view's scale), so a reviewer checks the picture
        # against data and not by eye.
        tree = mcp.json("ui_tree", limit=200, fresh=True)
        packet = {k: f.get(k) for k in ("camera", "ui", "frame") if k in f}
        packet["ui_tree"] = {k: tree.get(k) for k in ("view", "display", "ui_scale", "screens", "widgets") if k in tree}
        (out / f"{name}.json").write_text(json.dumps(packet, indent=1))
    return out / f"{name}.png"


def widget(tree, wid):
    """Widget `wid` of a ui_tree answer (its widgets are one flat list, each naming its screen)."""
    for w in tree.get("widgets", []):
        if w.get("id") == wid:
            return w
    return None


def settings(mcp, **args):
    return mcp.json("play_settings", **args)


def ui_check_everything(mcp, summary):
    print("== ui_check at every setting ==")
    errors, runs = [], 0
    for vision in VISIONS:
        for ts in TEXT_SIZES:
            settings(mcp, set={"vision": vision, "text_size": ts})
            for screens in SETS:
                j = mcp.json("ui_check", screens=screens, targets=TARGETS, scales=SCALES, max_findings=50)
                runs += 1
                for f in j.get("findings", []):
                    if f.get("severity") != "error":
                        continue
                    smaller = "@0.8x" in f.get("target", "") or ts < 1.0
                    if f.get("rule") == "legibility" and smaller:
                        continue
                    errors.append(f"{vision} text {ts} {'+'.join(screens)}: [{f['rule']}] {f['target']} {f['screen']}:{f['widget']} {f['message']}")
    summary["ui_check"] = {"runs": runs, "errors": len(errors), "first": errors[:20]}
    check(not errors, f"ui_check holds at every colour theme x text size x display x interface size ({runs} runs, {len(errors)} errors)")
    for e in errors[:10]:
        print("     " + e)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--port", type=int, default=7630)
    ap.add_argument("--out")
    a = ap.parse_args()
    out = Path(a.out) if a.out else ROOT / "target" / "a11y-check" / a.adapter
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True, exist_ok=True)
    if not GAME.exists():
        print(f"no {GAME}: cargo build --release -p sepulcra-app")
        return 2
    settings_dir = out / "settings"
    summary = {"adapter": a.adapter}

    def flags(n):
        return ["--adapter", a.adapter, "--size", "1280x800", "--owner", "a11y", "--audio", "null", "--settings", str(settings_dir),
                "--captures", str(out / f"captures{n}"), "--worlds", str(out / f"worlds{n}"), "--saves", str(out / "saves"), "--", "--seed", "7"]

    # ---- Engine A ----
    with Engine(GAME, a.port, out / "engine_a.log", *flags(0), cwd=str(out)) as eng:
        m = eng.mcp("a11y")
        m.hello("a11y", role="owner")
        print("== 1. the title ==")
        st = wait_for(lambda: (lambda s: s if (s.get("shell") or {}).get("title") else None)(m.json("play_state")), 30)
        check(st is not None, "the game opens on the title screen")
        tick0 = m.json("play_state")["tick"]
        time.sleep(1.5)
        check(m.json("play_state")["tick"] == tick0, "the title holds the run (the tick stays)")
        # The pictures don't depend on the desk this runs at (its display scale changes when a
        # monitor is switched off): a plain 100% display first, and its scale kept to compare.
        act(m, {"display": {"diagonal_in": None, "touch": False, "os_scale": 1.0}, "steps": []})
        view_scale = lambda: (screens_of(m).get("view") or {}).get("scale") or 0
        plain_scale = wait_for(lambda: (lambda v: v if abs(v - 1.0) < 1e-3 else None)(view_scale()), 5) or view_scale()
        summary["plain_display_scale"] = plain_scale
        check(abs(plain_scale - 1.0) < 1e-3, f"a plain 100% display lays the interface out at x1 ({plain_scale})")
        top, _ = top_screen(m)
        check(top == "title", f"the title is the top screen ({top})")
        shot(m, out, "title")

        print("== 2. saved settings and a colour theme ==")
        s = settings(m, set={"vision": "deuteranopia", "text_size": 1.15, "captions": True, "subtitle_size": "large"})
        check((s.get("saving") or {}).get("agents_save") is True and s["saving"].get("store") == "folder", f"the named folder saves agents' changes too: {s.get('saving')}")
        check((s.get("colours") or {}).get("pairs_apart") is True, f"the deuteranopia theme keeps the dead, magic and warnings apart: {s.get('colours')}")
        f = settings_dir / "settings.json"
        saved = wait_for(lambda: f.exists() and "deuteranopia" in f.read_text(encoding="utf-8") and json.loads(f.read_text(encoding="utf-8")), 10)
        check(bool(saved) and saved.get("text_size") == 1.15 and saved.get("captions") is True, "the settings file holds them")
        for screens in (["hud"], ["access"]):
            j = m.json("ui_check", screens=screens, targets=["deck", "1080p", "4k"], scales=[1.0, 1.5], max_findings=20)
            check(j.get("errors") == 0, f"the deuteranopia theme passes ui_check on {'+'.join(screens)} ({j.get('errors')} errors)")

        print("== 3. rebinding ==")
        m.json("play_ui", press="title_controls")
        top, _ = top_screen(m)
        check(top == "controls", f"the title opens the controls ({top})")
        m.json("play_ui", press="bind_kbm[0]")
        s = settings(m)
        check(s.get("rebinding") is False and s.get("refused_agent_presses", 0) >= 1, "an agent's press can't make the game listen for the player's next key")
        s = settings(m, bind=[{"action": "call_wave", "device": "kbm", "input": "Key.G"}])
        check("Key.G" in json.dumps((s.get("live") or {}).get("bindings")), "play_settings binds call_wave to G by name")
        e = err_of(m, "play_settings", bind=[{"action": "orbit", "device": "kbm", "input": "Key.H"}])
        check(e is not None and e.get("error") == "not_rebindable", f"an axis isn't rebound by name: {e}")
        m.json("play_ui", press="controls_back")
        check(top_screen(m)[0] == "title", "back to the title")

        ui_check_everything(m, summary)
        settings(m, set={"vision": "deuteranopia", "text_size": 1.15})

        print("== 5. handheld profile and limited sizes ==")
        act(m, {"display": {"diagonal_in": 7.0, "touch": True, "os_scale": 1.0}, "steps": []})  # a Deck reports 100%: don't inherit this desk's display scale
        # (The game takes the new display at its next frame.)
        s = wait_for(lambda: (lambda x: x if x.get("handheld") and (x.get("ui_scale") or {}).get("user", 1) > 1.01 else None)(settings(m)), 5) or settings(m)
        check(s.get("handheld") == "Steam Deck", f"a 7-inch 1280x800 panel is a Steam Deck ({s.get('handheld')})")
        check(abs((s.get("ui_scale") or {}).get("user", 0) - 1.15) < 1e-3 and s["ui_scale"].get("auto"), f"its Auto size is 1.15: {s.get('ui_scale', {}).get('user')}")
        deck_scale = view_scale()
        summary["handheld_scale"] = deck_scale
        check(deck_scale >= plain_scale * 1.14, f"the interface is at least 14% larger than on a plain display of the same size ({plain_scale} -> {deck_scale})")
        shot(m, out, "handheld_deck_title")
        act(m, {"display": {"diagonal_in": None, "touch": False, "os_scale": 1.75}, "steps": [{"activate": "title_settings"}, {"activate": "accessibility"}]})
        # (The game takes the new display at its next window frame: on a busy machine that can
        # be after this call's answer.)
        limited_of = lambda x: [o for o in (x.get("ui_scale") or {}).get("options", []) if o.get("limited")]
        s = wait_for(lambda: (lambda x: x if limited_of(x) else None)(settings(m)), 5) or settings(m)
        limited = limited_of(s)
        check(bool(limited) and all(o["limited"] == "limited_by_window" for o in limited), f"on a 175% display the largest sizes are limited by the window: {limited}")
        tree = screens_of(m)
        greyed = [w for w in (widget(tree, f"ui_opt_{i}") for i in range(6)) if w and w.get("enabled") is False]
        check(len(greyed) == len(limited) and len(limited) > 0, f"and greyed out on the accessibility screen ({len(greyed)} disabled)")
        uf = m.json("ui_frame")
        if uf.get("image", {}).get("path"):
            shutil.copy(uf["image"]["path"], out / "access_limited_ui_frame.png")
        shot(m, out, "access_limited")
        summary["limited"] = limited
        act(m, {"display": {"os_scale": None}, "steps": [{"press": "back"}, {"press": "back"}]})
        check(top_screen(m)[0] == "title", "back to the title")

        print("== 6. intro, subtitles and captions ==")
        m.json("play_ui", press="start")
        playing = wait_for(lambda: (m.json("play_state").get("cutscene") or {}).get("playing"), 10)
        check(playing == "assets/timelines/intro.json", f"New run plays the intro ({playing})")
        cap = wait_for(lambda: settings(m).get("captioned", 0) >= 1, 6, every=0.1)
        check(bool(cap), "the intro's cue is captioned (from the audio log)")
        played = m.json("audio_played", seconds=30, limit=50)
        by_timeline = [p for p in played.get("plays", []) if str(p.get("by", "")).startswith("timeline:")]
        check(any(p.get("name") == "brazier_whoosh" for p in by_timeline), f"the cue played on the engine's audio as timeline:<path> ({[(p.get('name'), p.get('by')) for p in by_timeline]})")

        def subtitle_now():
            t = screens_of(m)
            texts = [w.get("text") for w in t.get("widgets", []) if w.get("screen") == "subtitles" and w.get("text")]
            return texts if any("dead rise" in (x or "") for x in texts) else None
        texts = wait_for(subtitle_now, 12, every=0.25)
        check(bool(texts), f"the Warden's line is drawn by the subtitles screen ({texts})")
        if texts:
            shot(m, out, "intro_subtitles")
            perf = m.json("perf_capture", frames=90, trace=False, label="a11y_intro_subtitles")
            summary["perf_intro_subtitles"] = {k: perf.get(k) for k in ("frames", "adapter", "frame_ms", "gpu_ms")}
        ended = wait_for(lambda: (m.json("play_state").get("cutscene") or {}).get("last_ended"), 20)
        check(bool(ended), f"the intro ends ({ended})")

        info = m.json("engine_info", response_format="detailed")
        check((info.get("settings") or {}).get("store") == "folder" and (info["settings"].get("written") or 0) >= 1 and "failed" not in info["settings"],
              f"engine_info shows the settings store and its writes ({json.dumps(info.get('settings'))[:120]})")
        check(((info.get("timeline_audio") or {}).get("played") or 0) >= 1, f"and the timeline's cue sounds ({json.dumps(info.get('timeline_audio'))})")
        print("== 7. gamepad pause ==")
        j = act(m, {"device": "gamepad", "steps": [{"press": "pause"}]})
        summary["gamepad_pause_answer"] = j.get("screens")
        check((j.get("screens") or [None])[0] == "pause" and m.json("play_state").get("paused") is True, f"Start opens the pause menu and pauses ({j.get('screens')}; steps {json.dumps(j.get('steps'))[:300]})")
        shot(m, out, "pause_deuteranopia")
        j = act(m, {"device": "gamepad", "steps": [{"press": "pause"}]})
        check((j.get("screens") or [None])[0] == "hud" and m.json("play_state").get("paused") is False, f"Start again closes it and resumes ({j.get('screens')})")
        shot(m, out, "hud_deuteranopia")
    check(eng.validation_errors() == 0, f"engine A shut down with {eng.validation_errors()} validation errors")

    # ---- Engine B: the same settings folder ----
    print("== 8. restart, saved settings, Quit ==")
    with Engine(GAME, a.port + 1, out / "engine_b.log", *flags(1), cwd=str(out)) as eng:
        m = eng.mcp("a11y")
        m.hello("a11y", role="owner")
        s = settings(m)
        live = s.get("live") or {}
        check(live.get("vision") == "deuteranopia" and live.get("text_size") == 1.15 and live.get("captions") is True, f"the settings came back after a restart: {live.get('vision')}, {live.get('text_size')}")
        check("Key.G" in json.dumps(live.get("bindings")), "and the binding")
        wait_for(lambda: (m.json("play_state").get("shell") or {}).get("title"), 20)
        tree = screens_of(m)
        q = widget(tree, "quit_game")
        hwnd = find_window(eng.proc.pid)
        if check(q is not None and hwnd is not None, "the title's Quit and the game's window"):
            # Two changes back to back, then Quit at once: the second waits behind the first's
            # write, and must still reach the disk (review R1: it was lost).
            settings(m, set={"volume": 0.25})
            settings(m, set={"volume": 0.75})
            x, y = q["rect"][0] + q["rect"][2] / 2, q["rect"][1] + q["rect"][3] / 2
            post_click(hwnd, x, y)
            try:
                code = eng.proc.wait(timeout=20)
            except Exception:  # noqa: BLE001
                code = None
            check(code is not None, f"Quit, clicked with the window's own input, stops the engine (exit {code})")
    check(eng.validation_errors() == 0, f"engine B shut down with {eng.validation_errors()} validation errors")
    last = json.loads((out / "settings" / "settings.json").read_text(encoding="utf-8"))
    check(last.get("volume") == 0.75, f"the change made just before Quit is in the file (volume {last.get('volume')})")

    # ---- Engine C: headless, no settings ----
    print("== 9. headless keeps no settings ==")
    with Engine(GAME, a.port + 2, out / "engine_c.log", "--headless", "--captures", str(out / "captures2"), "--", "--seed", "7", "--mute", cwd=str(out)) as eng:
        m = eng.mcp("a11y")
        m.hello("a11y")
        s = settings(m)
        check((s.get("saving") or {}).get("store") == "off", f"a headless engine reads and writes no settings: {s.get('saving')}")
        check((m.json("play_state").get("shell") or {}).get("title") is False, "and shows no title")

    summary["failures"] = FAILURES
    (out / "summary.json").write_text(json.dumps(summary, indent=1))
    print(f"\n{len(FAILURES)} failures; captures and summary in {out}")
    return 1 if FAILURES else 0


if __name__ == "__main__":
    sys.exit(main())
