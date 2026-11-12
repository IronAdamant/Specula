"""Live check of galleries, the pick flow, the preference store, reference boards and the style
configurator (docs/design/GALLERIES.md), dogfooded on Sepulcra in the real window.

Sepulcra runs windowed (on a scratch copy of its assets) and agents make galleries over it:

- the moonlight (the key light's colour and strength, and fog density) of the game's own view: the
  owner's pick flow is driven through the real window with the same input a human uses (mouse
  clicks posted to the window, typed characters, digit keys): select a tile, "This one, with
  edits" with a typed note and the "darker" chip, "None of these", the breadcrumb back to round 1,
  a right-click dislike, then "This one"; gallery_apply writes the accepted moon into style.toml;
- headstone proportions (the tombstone recipe on the stage, scaled), decided by an agent with role
  owner: three consistent picks make a suggestion, which goes to the review queue and, once
  approved there, becomes a prior;
- the "can't afford" colour on the HUD's panel over the game (a ui gallery), accepted at the
  window and applied to ui/theme.json; the HUD's "Not enough dust" label then uses it;
- a reference board (an engine capture and a synthetic moonlit image) measured, and the scene
  checked against it;
- the style configurator: four galleries in a row on the window (base style, key light, fog,
  interface colours), each "This one" at the window; the proposal is approved on the review queue
  and writes style.toml and the notes.

With --pacing, the window's frames are captured over a whole round's render (perf_capture back to
back) and a frame over 50 ms that went to the studio fails.

Usage: cargo build --release -p sepulcra-app && python scripts/gallery_check.py [--port 7610]
       [--adapter integrated] [--out DIR] [--exe PATH] [--pacing]
Standard library only; output is ASCII. Captures and the report go to --out (default
target/gallery-check).
"""
import argparse
import ctypes
import json
import os
import shutil
import struct
import sys
import threading
import time
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import Engine, ROOT  # noqa: E402

FAILS = []


def check(cond, what):
    print(("ok   " if cond else "FAIL ") + what)
    if not cond:
        FAILS.append(what)
    return cond


def oklab(hexc):
    def lin(c):
        c /= 255.0
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = (lin(int(hexc[i:i + 2], 16)) for i in (1, 3, 5))
    l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b) ** (1 / 3)
    m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b) ** (1 / 3)
    s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b) ** (1 / 3)
    return (0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s, 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s)


def min_gap(colours):
    """The smallest Oklab distance between any two of `colours` (hex strings)."""
    labs = [oklab(c) for c in colours if isinstance(c, str) and c.startswith("#")]
    gaps = [sum((a - b) ** 2 for a, b in zip(labs[i], labs[j])) ** 0.5 for i in range(len(labs)) for j in range(i + 1, len(labs))]
    return min(gaps) if gaps else 0.0


def body(result):
    return json.loads(next(c["text"] for c in result["content"] if c["type"] == "text"))


def call(mcp, tool, **args):
    return body(mcp.call(tool, **args))


def refused(mcp, tool, **args):
    result = mcp.request("tools/call", {"name": tool, "arguments": args})
    if not result.get("isError"):
        FAILS.append(f"{tool} should have been refused")
        print(f"FAIL {tool} should have been refused: {result['content'][0]['text'][:300]}")
        return {}
    return json.loads(result["content"][0]["text"])


def disk_text(path, want=None, timeout=8.0):
    """A project file's text once the disk holds it: project writes answer at once and reach the
    disk on the project lane a moment later. `want`: a string the text must contain."""
    def read():
        try:
            t = Path(path).read_text()
        except OSError:
            return None
        return t if want is None or want in t else None
    return wait_for(read, timeout) or (Path(path).read_text() if Path(path).exists() else "")


def wait_for(pred, timeout=20.0, step=0.15):
    end = time.time() + timeout
    while time.time() < end:
        v = pred()
        if v:
            return v
        time.sleep(step)
    return pred()


# ---- the real window (Win32, out of process): the input a human's mouse and keyboard send ----

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


def click(hwnd, x, y, right=False):
    lp = (int(y) << 16) | (int(x) & 0xFFFF)
    down, up, mk = (0x0204, 0x0205, 2) if right else (0x0201, 0x0202, 1)
    user32.PostMessageW(hwnd, 0x0200, 0, lp)
    time.sleep(0.03)
    user32.PostMessageW(hwnd, down, mk, lp)
    time.sleep(0.06)
    user32.PostMessageW(hwnd, up, 0, lp)
    time.sleep(0.12)


def click_rect(hwnd, r, right=False, fx=0.5, fy=0.5):
    click(hwnd, r[0] + r[2] * fx, r[1] + r[3] * fy, right)


def key(hwnd, vk, scan=0):
    user32.PostMessageW(hwnd, 0x0100, vk, 1 | (scan << 16))
    time.sleep(0.05)
    user32.PostMessageW(hwnd, 0x0101, vk, 1 | (scan << 16) | (0xC0 << 24))
    time.sleep(0.08)


def type_text(hwnd, text):
    for ch in text:
        user32.PostMessageW(hwnd, 0x0102, ord(ch), 1)
        time.sleep(0.02)
    time.sleep(0.1)


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
    write_png(path, w, h, bytes(raw), rgb=True)
    return ok != 0 and len({pixels.raw[i:i + 3] for i in range(0, len(pixels.raw), 4 * 97)}) > 2


def write_png(path, w, h, raw, rgb=False):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2 if rgb else 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b"")
    Path(path).write_bytes(png)


def moonlit_reference(path, w=320, h=180):
    """A synthetic reference: a deep blue night sky over a dark ground, a pale moon, soft light."""
    raw = bytearray()
    for y in range(h):
        raw.append(0)
        for x in range(w):
            t = y / h
            if t < 0.62:
                c = (int(10 + 26 * t), int(16 + 34 * t), int(34 + 60 * t))
            else:
                c = (int(28 - 10 * (t - 0.62)), int(32 - 10 * (t - 0.62)), int(44 - 14 * (t - 0.62)))
            d = ((x - w * 0.72) ** 2 + (y - h * 0.22) ** 2) ** 0.5
            if d < 13:
                c = (214, 222, 242)
            elif d < 40:
                k = (40 - d) / 27 * 0.35
                c = tuple(int(v + (200 - v) * k) for v in c)
            raw += bytes(c)
    write_png(path, w, h, bytes(raw), rgb=True)


def oklab_l(hexc):
    """Oklab lightness of #rrggbb (for checking 'darker')."""
    def lin(v):
        v /= 255
        return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4
    r, g, b = (lin(int(hexc[i:i + 2], 16)) for i in (1, 3, 5))
    l_ = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b) ** (1 / 3)
    m_ = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b) ** (1 / 3)
    s_ = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b) ** (1 / 3)
    return 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_


# ---- the flow ----

class Window:
    """The gallery screen in the real window: its rects come from the screen's status (as data),
    the presses go to the window as a human's."""

    def __init__(self, hwnd, lead, out):
        self.hwnd, self.lead, self.out, self.shots = hwnd, lead, out, []

    def screen(self, gid):
        return call(self.lead, "gallery_get", id=gid).get("screen", {})

    def ready(self, gid, round_n):
        """Waits until round `round_n` is on the window with its sheet drawn; its screen status."""
        def ok():
            g = call(self.lead, "gallery_get", id=gid)
            s = g.get("screen", {})
            return s if (g.get("rounds") == round_n and g.get("status") == "open" and s.get("open") and s.get("sheet_shown") and s.get("round") == round_n and "tile0" in s.get("rects", {})) else None
        s = wait_for(ok, 45)
        time.sleep(0.7)  # the screen refuses presses on what has only just appeared
        return s or self.screen(gid)

    def press(self, gid, widget):
        r = self.screen(gid).get("rects", {}).get(widget)
        if not r:
            FAILS.append(f"{widget} isn't on the screen")
            print(f"FAIL {widget} isn't on the screen")
            return
        click_rect(self.hwnd, r)
        time.sleep(0.25)

    def shot(self, name):
        p = self.out / f"window_{len(self.shots):02d}_{name}.png"
        ok = screenshot(self.hwnd, p)
        self.shots.append(str(p))
        return ok


def round_info(lead, gid):
    g = call(lead, "gallery_get", id=gid)
    return g, g.get("round", {})


def moonlight(lead, win, report):
    print("\n-- moonlight: the pick flow at the window --")
    space = [
        {"name": "moon", "target": "light.key_color", "kind": "colour", "base": "#d0d9f0", "spread": {"l": 0.1, "c": 0.06, "h": 60},
         "apply": {"file": "style.toml", "toml": "stage.game.key_color"}},
        {"name": "strength", "target": "light.key_intensity", "min": 0.3, "max": 1.0, "default": 0.55, "apply": {"file": "style.toml", "toml": "stage.game.key_intensity"}},
        {"name": "fog", "target": "fog.density", "min": 0.0, "max": 0.04, "default": 0.012},
    ]
    g = call(lead, "gallery_create", id="moonlight", title="Which moonlight?", subject={"scene": {"camera": "window"}}, space=space, open=True)
    check(g.get("rounds") == 1 and len(g.get("round", {}).get("variants", [])) == 9, f"round 1 of the moonlight gallery: 9 variants ({g.get('status')}, {g.get('problem')})")
    check(Path(g["round"].get("sheet", "")).is_file(), "its sheet and packet are in the captures folder")
    lights = {v["measures"]["value_mean"] for v in g["round"]["variants"] if "measures" in v}
    check(len(lights) >= 5, f"the variants differ (value_mean of {len(lights)} distinct)")
    check(not g.get("warning"), f"no two tiles render alike ({g.get('warning')})")
    report["moonlight_round1_cost"] = g.get("render_cost")
    check("darker" in g.get("chips", []) and "foggier" in g.get("chips", []), f"chips offered: {g.get('chips')}")
    s = win.ready("moonlight", 1)
    win.shot("moonlight_round1")
    ps = call(lead, "play_state")
    check(ps.get("paused") is True, f"the game pauses while the gallery screen holds the window (paused={ps.get('paused')})")
    # Select tile 5 by a click on it, then "This one, with edits".
    click_rect(win.hwnd, s["rects"]["tile4"])
    time.sleep(0.3)
    check(win.screen("moonlight").get("selected") == 4, "a click on the 5th tile selects it")
    win.press("moonlight", "gl_edit")
    check(win.screen("moonlight").get("editing"), "\"This one, with edits\" opens the note and the chips")
    type_text(win.hwnd, "colder, less purple")
    chips = g["chips"]
    k = chips.index("darker")
    check(k < 6, "the darker chip is on the screen")
    win.press("moonlight", f"gl_k{k}")
    st = win.screen("moonlight")
    # The note the human types is the owner's alone (input sweep R3, docs/design/SECURITY.md): the
    # lead sees its length, never its words (inputguard_check reads it as the owner).
    check("note" not in st and st.get("note_redacted") is True and "note_chars" not in st and st.get("chips_on") == ["darker"],
          f"typed note (redacted for the lead, its length too) and the darker chip: {st.get('note')!r} {st.get('chips_on')}")
    win.shot("moonlight_edit")
    picked = g["round"]["variants"][4]["values"]
    win.press("moonlight", "gl_send")
    g2 = call(lead, "gallery_wait", id="moonlight", after=g["seq"], timeout_s=25)
    d1 = call(lead, "gallery_get", id="moonlight", round=1)["round"].get("decision", {})
    check(d1.get("kind") == "edit" and d1.get("note") == "colder, less purple" and d1.get("chips") == ["darker"] and d1.get("by") == "window:owner",
          f"round 1's decision: edit with the note and chip, by the window ({d1})")
    check(g2.get("rounds") == 2 and g2["round"].get("incumbent") is not None, f"round 2 is made around the pick ({g2.get('status')})")
    inc = g2["round"]["variants"][g2["round"]["incumbent"]]["values"]
    check(inc["moon"] == picked["moon"], "the pick is shown again in round 2 (the incumbent)")
    darker = all(oklab_l(v["values"]["moon"]) <= oklab_l(picked["moon"]) + 0.004 for v in g2["round"]["variants"])
    check(darker, "darker: no variant of round 2 is lighter than the pick")
    # Round 2: None of these.
    win.ready("moonlight", 2)
    win.shot("moonlight_round2")
    win.press("moonlight", "gl_none")
    g3 = call(lead, "gallery_wait", id="moonlight", after=g2["seq"], timeout_s=25)
    check(g3.get("rounds") == 3 and call(lead, "gallery_get", id="moonlight", round=2)["round"]["decision"]["kind"] == "none", "\"None of these\" makes round 3")
    # Round 3: the breadcrumb back to round 1.
    win.ready("moonlight", 3)
    win.press("moonlight", "gl_c0")
    g4 = call(lead, "gallery_wait", id="moonlight", after=g3["seq"], timeout_s=25)
    r1 = [v["values"] for v in g["round"]["variants"]]
    r4 = [v["values"] for v in g4.get("round", {}).get("variants", [])]
    check(g4.get("rounds") == 4 and r4 == r1, "the breadcrumb shows round 1's variants again, as round 4")
    # Round 4: a right-click dislike (the round stays open), the digit 3, "This one".
    s4 = win.ready("moonlight", 4)
    click_rect(win.hwnd, s4["rects"]["tile1"], right=True, fx=0.3, fy=0.6)
    time.sleep(0.4)
    g4b = call(lead, "gallery_get", id="moonlight")
    check(g4b.get("status") == "open" and g4b["round"].get("dislikes", [{}])[0].get("variant") == 1, "a right-click marks variant 2 disliked; the round stays open")
    key(win.hwnd, 0x33)  # "3"
    time.sleep(0.3)
    check(win.screen("moonlight").get("selected") == 2, "the digit 3 selects the 3rd variant")
    win.shot("moonlight_round4_selected")
    # The tallest view: editing on a later round (with the rounds row). A sight-skeptic finding:
    # it ran off an 800 px window. Measured from the screen's own rects, then left.
    win.press("moonlight", "gl_edit")
    time.sleep(0.4)
    se = win.screen("moonlight")
    lowest = max((r[1] + r[3] for r in se.get("rects", {}).values()), default=0)
    check(se.get("editing") and 0 < lowest <= 800, f"editing on round 4 fits the 1280x800 window (lowest widget ends at y {lowest})")
    win.shot("moonlight_round4_edit")
    win.press("moonlight", "gl_cancel")
    time.sleep(0.4)
    win.press("moonlight", "gl_accept")
    g5 = wait_for(lambda: (lambda j: j if j.get("status") == "accepted" else None)(call(lead, "gallery_get", id="moonlight")), 10)
    check(bool(g5) and g5.get("accepted", {}).get("moon") == r1[2]["moon"], "\"This one\" accepts it")
    win.shot("moonlight_accepted")
    a = call(lead, "gallery_apply", id="moonlight")
    toml = disk_text(report["assets"] / "style.toml", f'key_color = "{r1[2]["moon"]}"')
    check(a.get("files") and f'key_color = "{r1[2]["moon"]}"' in toml, f"gallery_apply writes the accepted moon into style.toml ({r1[2]['moon']})")
    check("[motion]" in toml and "[palette]" in toml, "the rest of style.toml is kept")
    hist = call(lead, "world_history", limit=40)
    report["moonlight"] = {"accepted": g5.get("accepted") if g5 else None, "applied": a}
    text = json.dumps(hist)
    check("assets/taste/galleries/moonlight.json" in text and "window:owner" in text, "decisions are journaled (as window:owner) on the gallery's file")


def wait_open(m, gid, t=60):
    return wait_for(lambda: (lambda x: x if x.get("status") == "open" and x.get("round") else None)(call(m, "gallery_get", id=gid)), t) or {}


def agents(eng, lead, owner, report):
    """Taste review 2's probes, kept: what other agents can't do to the human's picks."""
    print("\n-- other agents and the human's picks --")
    mal = eng.mcp("mallory")
    mal.hello("mallory", role="builder")
    sp = [{"name": "strength", "target": "light.key_intensity", "min": 0.3, "max": 1.0, "default": 0.55, "apply": {"file": "style.toml", "toml": "stage.game.key_intensity"}}]
    call(lead, "gallery_create", id="vic", subject={"stage": {}}, space=sp)
    wait_open(lead, "vic")
    call(owner, "gallery_decide", id="vic", round=1, decision="accept", variant=2)
    picked = call(lead, "gallery_get", id="vic").get("accepted")
    # A gallery that applies into taste state (another gallery's pick), in any spelling.
    for f in ("taste/galleries/vic.json", "Taste/galleries/vic.json", "./taste/galleries/vic.json"):
        evsp = [{"name": "n", "target": "light.key_intensity", "min": 0, "max": 8, "step": 1, "apply": {"file": f, "json": "rounds[0].decision.variant"}}]
        e = refused(mal, "gallery_create", id="evil", subject={"stage": {}}, space=evsp)
        check(e.get("error") in ("reserved", "bad_space", "invalid_argument") and "taste" in json.dumps(e), f"a gallery can't apply into taste state ({f}: {e.get('error')})")
    # Writing the file directly (round 1's forge).
    f = call(mal, "project_read", path="assets/taste/galleries/vic.json")
    forged = json.loads(f.get("text", "{}"))
    forged["rounds"][0]["decision"]["variant"] = 3
    e = refused(mal, "project_write", path="assets/taste/galleries/vic.json", text=json.dumps(forged, indent=2) + "\n", expect_version=f.get("version"))
    check(e.get("error") == "reserved", f"project_write can't touch taste state ({e.get('error')})")
    check(picked and call(lead, "gallery_get", id="vic").get("accepted") == picked, f"the owner's pick on vic stands ({picked})")
    # Another agent's gallery: its round isn't mallory's to replace, nor to delete.
    call(lead, "gallery_create", id="g3", subject={"stage": {}}, space=sp)
    wait_open(lead, "g3")
    e = refused(mal, "gallery_round", id="g3", variants=[{"strength": 0.3}, {"strength": 0.31}])
    check(e.get("error") == "not_allowed", f"a builder can't replace the lead's round ({e.get('error')})")
    call(owner, "gallery_decide", id="g3", round=1, decision="edit", variant=0, note="the owner's private words")
    h = call(mal, "world_history", path_prefix="/project/assets/taste/galleries/g3.json", limit=20)
    own = [x["id"] for x in h.get("entries", h.get("history", [])) if x.get("by") == "owner-proxy"]
    if check(bool(own), "the owner's pick is journaled"):
        e = refused(mal, "world_undo", ids=[own[-1]], force=True)
        # (A builder's force is refused as the lead's to give; even a lead's, as not_yours.)
        check(e.get("error") in ("not_yours", "force_needs_lead"), f"nobody force-undoes the owner's pick ({e.get('error')})")
    q = call(lead, "taste_query", gallery="g3")
    check(any(r.get("note") == "the owner's private words" for r in q.get("records", [])), "the pick and its note are still there")
    # Claims: none on taste state; one on what holds it doesn't lock the human out (F5).
    e = refused(mal, "claim_take", path="/project/assets/taste", lease_s=60)
    check(e.get("error") == "reserved", f"no agent claims taste state ({e.get('error')})")
    call(mal, "claim_take", path="/project/assets", lease_s=60)
    call(lead, "gallery_create", id="g4", subject={"stage": {}}, space=sp)
    wait_open(lead, "g4")
    d = call(owner, "gallery_decide", id="g4", round=1, decision="edit", variant=1, note="darker")
    check("error" not in d, f"the owner picks under a builder's claim on the assets folder ({d.get('error')})")
    call(mal, "claim_release", all=True)
    # Deleting: project_delete is refused for taste state; gallery_delete is the maker's (or the
    # owner's, an opener's), journaled and undoable by the deleter.
    e = refused(lead, "project_delete", path="assets/taste/galleries/g3.json")
    check(e.get("error") == "reserved", f"project_delete refuses taste state ({e.get('error')})")
    e = refused(mal, "gallery_delete", id="g3")
    check(e.get("error") == "not_allowed", f"a builder can't delete the lead's gallery ({e.get('error')})")
    d = call(lead, "gallery_delete", id="g3")
    check(d.get("deleted") is True, "the lead deletes it")
    e = refused(lead, "gallery_get", id="g3")
    check(e.get("error") == "gallery_not_found", "and it is gone")
    if d.get("journal"):
        call(lead, "world_undo", ids=[d["journal"]])
        g = call(lead, "gallery_get", id="g3")
        check(g.get("id") == "g3" and (g.get("rounds") or 0) >= 1, f"undoing the delete brings it back at once (the cache follows the files: rounds {g.get('rounds')})")
    for gid in ("vic", "g3", "g4"):
        call(lead, "gallery_delete", id=gid)


def headstones(lead, owner, reviewer, report):
    print("\n-- headstone proportions: picks by an agent speaking for the owner --")
    space = [{"name": "width", "target": "scale.x", "min": 0.75, "max": 1.4, "default": 1.0}, {"name": "height", "target": "scale.y", "min": 0.75, "max": 1.4, "default": 1.0}]
    g = call(lead, "gallery_create", id="headstones", title="Headstone proportions", subject={"recipe": "recipes/tombstone.json", "rig": "game"}, space=space, variants=6)
    check(g.get("rounds") == 1 and len(g["round"]["variants"]) == 6, f"a 6-variant recipe gallery on the stage ({g.get('problem')})")
    no = refused(lead, "gallery_decide", id="headstones", round=1, decision="accept", variant=0)
    check(no.get("error") == "owner_only", "a lead can't pick for the human (owner_only)")
    # The owner picks; after reading the note the agent proposes its own rounds close to the pick;
    # three distinct, agreeing picks make a suggestion (a prior: no apply target). One value picked
    # again and again would not (a review probe).
    seq = g["seq"]
    call(owner, "gallery_decide", id="headstones", round=1, decision="edit", variant=3, chips=["bigger"], note="taller and narrower")
    g = call(lead, "gallery_wait", id="headstones", after=seq, timeout_s=25)
    picked = call(lead, "gallery_get", id="headstones", round=1)["round"]["variants"][3]["values"]
    w, h = picked["width"], picked["height"]
    for k, (dw, dh, final) in enumerate([(0.02, 0.0, False), (-0.02, 0.02, True)]):
        near = [{"width": w, "height": h}, {"width": round(w + dw, 4), "height": round(h + dh, 4)}, {"width": round(w + 0.2, 4), "height": h}, {"width": w, "height": round(max(h - 0.25, 0.75), 4)}]
        r = call(lead, "gallery_round", id="headstones", variants=near)
        check(r.get("status") == "rendering", f"the agent's own round {k + 1} ({r})")
        seq = g["seq"]
        g = wait_for(lambda: (lambda j: j if j.get("status") == "open" and j["round"].get("made") == "agent" and j.get("rounds") == r.get("round") else None)(call(lead, "gallery_get", id="headstones")), 30) or g
        call(owner, "gallery_decide", id="headstones", round=g["rounds"], decision="accept" if final else "edit", variant=1)
        if not final:
            g = call(lead, "gallery_wait", id="headstones", after=g["seq"], timeout_s=25)
    sug = call(lead, "taste_suggest", action="list")
    ours = [s for s in sug.get("suggestions", []) if s["target"] in ("scale.x", "scale.y")]
    check(len(ours) >= 1 and ours[0]["kind"] == "prior", f"three distinct agreeing picks suggest a prior ({[s['what'] for s in ours]})")
    report["headstones_cost"] = g.get("render_cost")
    if ours:
        sub = call(lead, "taste_suggest", action="submit", id=ours[0]["id"])
        check(sub.get("state") == "submitted", "the suggestion goes to the review queue")
        no = refused(reviewer, "review_decide", ids=[sub["submission"]], verdict="approved", note="yes")
        check(no.get("results", [{}])[0].get("error") == "owner_only", f"a reviewer can't approve a taste result ({json.dumps(no)[:160]})")
        dec = call(owner, "review_decide", ids=[sub["submission"]], verdict="approved", note="yes")
        applied = dec["results"][0].get("applied", {})
        check(applied.get("state") == "applied", f"the owner approving it writes it ({applied})")
        pri = disk_text(report["assets"] / "style" / "priors.json", ours[0]["target"])
        check(ours[0]["target"] in pri, "the prior is in style/priors.json (tracked: an approved result)")
    q = call(lead, "taste_query", subject="tombstone")
    check(q.get("total") == 3 and any(r.get("note") == "taller and narrower" for r in q["records"]), f"the preference store has the three decisions and the note ({q.get('total')})")


def cant_afford(lead, win, report):
    print("\n-- the \"can't afford\" colour on the HUD's panel --")
    # Around a dimmed, desaturated fire (the owner: fire colour is for fire only; dim or desaturated
    # shows "can't afford").
    space = [{"name": "cant_afford", "target": "ui.cant_afford", "kind": "colour", "base": "#b3a78f", "spread": {"l": 0.14, "c": 0.06, "h": 50},
              "apply": {"file": "ui/theme.json", "json": "colors.cant_afford"}}]
    subject = {"ui": {"panel": "#2b3038", "camera": "window", "lines": [
        {"text": "Rune pillar  120", "color": "#ece6d6"}, {"text": "Not enough dust", "color": "$cant_afford"}, {"text": "Dust 80", "color": "#b9b2a1"}]}}
    g = call(lead, "gallery_create", id="cant_afford", title="Which colour says \"can't afford\"?", subject=subject, space=space, open=True)
    check(g.get("rounds") == 1, f"the ui gallery's round 1 ({g.get('problem')})")
    ratios = [t.get("contrast_ratio") for v in g["round"]["variants"] for t in v.get("measures", {}).get("text", []) if t.get("text") == "Not enough dust"]
    n = len(g["round"]["variants"])
    check(len(ratios) == n, f"each tile reports the label's contrast against the panel ({ratios[:3]}...)")
    # The palette rules leave this spread a wedge of greys with room for five or six labels that
    # read apart: the round shows those rather than nine with look-alikes.
    check(n >= 5, f"the round keeps at least 5 tiles that read apart ({n})")
    if n < 9:
        check(g["round"].get("fit") == n, f"a trimmed round says how many fit the style rules (fit {g['round'].get('fit')})")
    # A sight-skeptic finding: labels 0.03 apart read the same at window size.
    labels = [v["values"]["cant_afford"] for v in g["round"]["variants"]]
    check(min_gap(labels) >= 0.05, f"the nine labels read apart (min dE {min_gap(labels):.3f}: {labels})")
    s = win.ready("cant_afford", 1)
    win.shot("cant_afford_round1")
    # Dim or desaturated, still readable (the owner: fire colour is for fire only): the lowest
    # chroma whose contrast against the panel is at least 4.5.
    def chroma(hexc):
        r, g_, b = (int(hexc[i:i + 2], 16) for i in (1, 3, 5))
        return max(r, g_, b) - min(r, g_, b)
    readable = []
    for i, v in enumerate(g["round"]["variants"]):
        texts = {x["text"]: x for x in v["measures"]["text"]}
        t, normal = texts["Not enough dust"], texts["Rune pillar  120"]
        # Dimmer than the normal text (so it reads as "not now"), still readable.
        if t["contrast_ratio"] >= 4.5 and t["delta_l"] < normal["delta_l"] - 0.02:
            readable.append((chroma(t["color"]), i))
    best = min(readable)[1] if readable else 0
    check(bool(readable), f"some variant is dimmer than the normal text and still readable ({len(readable)} of 9)")
    click_rect(win.hwnd, s["rects"][f"tile{best}"])
    time.sleep(0.3)
    win.press("cant_afford", "gl_accept")
    g2 = wait_for(lambda: (lambda j: j if j.get("status") == "accepted" else None)(call(lead, "gallery_get", id="cant_afford")), 10)
    check(bool(g2), "accepted at the window")
    a = call(lead, "gallery_apply", id="cant_afford")
    theme = disk_text(report["assets"] / "ui" / "theme.json", "cant_afford")
    chosen = g2["accepted"]["cant_afford"] if g2 else "?"
    check(f'"cant_afford": "{chosen}"' in theme, f"gallery_apply adds colors.cant_afford = {chosen} to ui/theme.json")
    # The HUD's "Not enough dust" labels use it (an agent's edit, hot-reloaded into the HUD).
    for find in ('"color": "fire", "visible": "!affordable"', '"color": "fire", "visible": "selected && !affordable"'):
        call(lead, "project_patch", path="assets/ui/hud.json", ops=[{"find": find, "replace": find.replace('"fire"', '"cant_afford"')}])
    report["cant_afford"] = {"chosen": chosen, "applied": a}


def view_of(report):
    """The window's view as the moonlight gallery resolved it (eye, target, fov)."""
    g = json.loads(disk_text(report["assets"] / "taste" / "galleries" / "moonlight.json", "eye"))
    s = g["subject"]["scene"]
    return {"eye": s["eye"], "target": s["target"], "fov_y_deg": s.get("fov_y_deg", 45)}


def board(lead, report, eng_caps):
    print("\n-- a reference board --")
    bdir = report["assets"] / "taste" / "boards" / "moonlit"
    bdir.mkdir(parents=True, exist_ok=True)
    moonlit_reference(bdir / "night_sky.png")
    f = call(lead, "sight_frame", width=480, height=270, ids=False, **view_of(report))
    shutil.copy(f["image"]["path"], bdir / "sepulcra_now.png")
    m = call(lead, "board_measure", board="moonlit")
    check(len(m.get("images", [])) == 2 and "value_p50" in m.get("targets", {}), f"board_measure measures 2 images and writes targets ({m.get('file')})")
    c = call(lead, "board_check", board="moonlit", camera="window")
    rules = {x["rule"] for x in c.get("findings", [])}
    check("board:moonlit.value_p50" in rules and "board:moonlit.palette_distance" in rules, f"board_check reports each measure with its rule ({c.get('failed')} failed)")
    bright = refused(lead, "board_check", board="nope", camera="window")
    check(bright.get("error") == "board_not_found", "an unmeasured board is refused")
    report["board"] = {"targets": m.get("targets"), "check": c}


def onboarding(lead, owner, win, report):
    print("\n-- the style configurator at the window --")
    st = call(lead, "style_onboard", action="start", subject={"scene": {"camera": "window"}}, open=True)
    check(st.get("state") == "running" and st["steps"][0]["gallery"], f"onboarding starts on step 1 ({st.get('steps', [{}])[0]})")
    for k, step in enumerate(["base", "light", "atmosphere", "interface"]):
        st = call(lead, "style_onboard", action="status")
        gid = st["steps"][k]["gallery"]
        if not check(bool(gid), f"step {k + 1} ({step}) has a gallery"):
            return
        s = win.ready(gid, 1)
        win.shot(f"onboarding_{step}")
        if step == "interface":
            # A sight-skeptic finding: the palette guard turned five green accents into the base.
            # A variant is its accent and its "can't afford" together: no two may match in both.
            vs = [v["values"] for v in call(lead, "gallery_get", id=gid)["round"]["variants"]]
            gap = min((max(min_gap([a[k], b[k]]) for k in a if isinstance(a[k], str) and a[k].startswith("#")) for i, a in enumerate(vs) for b in vs[i + 1:]), default=1.0)
            check(gap >= 0.05, f"the interface step's variants all read apart (closest pair dE {gap:.3f}; accents {[v.get('accent') for v in vs]})")
        tile = 0 if step == "base" else 4
        ok = False
        for attempt in range(3):
            click_rect(win.hwnd, s["rects"][f"tile{tile}"])
            time.sleep(0.4)
            win.press(gid, "gl_accept")
            ok = wait_for(lambda: call(lead, "gallery_get", id=gid).get("status") == "accepted", 6)
            if ok:
                break
            sc = win.screen(gid)
            print(f"     (step {step}, try {attempt + 1}: not accepted yet; screen selected={sc.get('selected')} last_action={sc.get('last_action')!r})")
            s = win.ready(gid, 1)
        check(ok, f"step {step}: \"This one\" at the window")
        if k < 3:
            nxt = wait_for(lambda: call(lead, "style_onboard", action="status")["steps"][k + 1]["gallery"], 10)
            check(bool(nxt) and win.screen(nxt).get("open"), f"the next step ({nxt}) opens on the window by itself")
    st = wait_for(lambda: (lambda j: j if j.get("state") == "proposed" else None)(call(lead, "style_onboard", action="status")), 10)
    check(bool(st) and st.get("submission"), f"after the last step the result is on the review queue ({st})")
    before = (report["assets"] / "style.toml").read_text()
    check(not (report["assets"] / "style" / "notes.md").exists(), "nothing is written before it is approved")
    if st:
        # A review probe approved a fence model submitted under the configurator's batch: agents
        # can't use that batch, and only the owner approves it.
        spoof = refused(lead, "review_submit", title="A new iron fence", kind="changes", summary="fence", batch="taste_onboarding", journal_since=1)
        check(spoof.get("error") == "reserved", f"taste batches are the taste tools' ({spoof.get('error')})")
        dec = call(owner, "review_decide", ids=[st["submission"]], verdict="approved", note="this is the look")
        check("style_toml" in dec["results"][0].get("applied", {}), "approving it writes style.toml and the notes")
        notes = disk_text(report["assets"] / "style" / "notes.md", "## Base style")
        after = disk_text(report["assets"] / "style.toml", "[look]")
        check(after != before and "[look]" in after and "cant_afford" in after and "[motion]" in after, "style.toml has the chosen look, its other tables kept")
        check("## Base style" in notes and "accepted in round 1" in notes, "the notes say what was chosen")
        report["onboarding"] = {"style_toml_bytes": len(after), "notes": notes[:600]}


def pacing(eng, lead, report):
    print("\n-- the window's frames while a round renders --")
    space = [{"name": "moon", "target": "light.key_color", "kind": "colour", "base": "#d0d9f0"}, {"name": "fog", "target": "fog.density", "min": 0, "max": 0.04}]
    call(lead, "gallery_create", id="pacing", subject={"scene": {"camera": "window"}}, space=space)

    def round_by_agent(c):
        g = call(c, "gallery_get", id="pacing")
        call(c, "gallery_round", id="pacing", centre={"fog": 0.02}, spread=0.4)
        return call(c, "gallery_wait", id="pacing", after=g["seq"], timeout_s=25)

    report["pacing"] = measure_round(eng, round_by_agent, 9, "a 9-variant round of the Sepulcra scene")
    # Taste review 2: tiles that render alike are replaced and rendered again, inside the same
    # budget. Fog alone shows nothing from above the mist, so a planned round of it replaces
    # most of its tiles.
    print("\n-- the window's frames while a round of look-alikes is replaced --")

    def look_alikes(c):
        return call(c, "gallery_create", id="pacing_alike", subject={"scene": {"camera": "window"}}, space=[{"name": "fog", "target": "fog.density", "min": 0, "max": 0.04}])

    out = measure_round(eng, look_alikes, None, "a round whose look-alikes are replaced", pacer="pacer2")
    report["pacing_alike"] = out
    check((out.get("render_cost") or {}).get("renders", 0) > 9, f"the replacements were rendered ({out.get('render_cost')})")


def measure_round(eng, job, renders, what, pacer="pacer"):
    """The window's frames (perf_capture) while `job` makes a round on another connection."""
    done, res = threading.Event(), {}

    def run():
        c = eng.mcp("pacer", detailed=False)
        c.hello(pacer, role="reviewer")
        t0 = time.perf_counter()
        g2 = job(c)
        res["ms"] = round((time.perf_counter() - t0) * 1000)
        res["cost"] = g2.get("render_cost")
        c.close()
        done.set()

    cap = eng.mcp("capture", detailed=True)
    paths = []
    th = threading.Thread(target=run)
    th.start()
    while not done.is_set():
        paths.append(cap.json("perf_capture", frames=240, trace=False)["path"])
    paths.append(cap.json("perf_capture", frames=60, trace=False)["path"])
    cap.close()
    th.join()
    frames = []
    for p in paths:
        j = json.loads(Path(p).read_text())
        si = j["zone_columns"].index("self_ms")
        for fr in j["frames"]:
            zones = fr.get("zones", {}).get("main", [])
            studio = sum(z[si] for z in zones if "studio" in z[0])
            frames.append((fr["dt_ms"], studio))
    dts = sorted(f[0] for f in frames)
    slow = [f for f in frames if f[0] > 50.0 and f[1] >= 0.5 * f[0]]
    out = {"round_ms": res.get("ms"), "render_cost": res.get("cost"), "frames": len(dts), "p50_ms": round(dts[len(dts) // 2], 2), "p95_ms": round(dts[int(len(dts) * 0.95)], 2),
           "max_ms": round(dts[-1], 2), "studio_over_50ms": len(slow), "max_studio_ms_in_a_frame": round(max((f[1] for f in frames), default=0.0), 2)}
    print("     " + json.dumps(out))
    check(len(slow) == 0, f"no window frame over 50 ms went to the gallery's renders ({what})")
    cost = res.get("cost") or {}
    n = cost.get("renders") or 0
    check((res.get("ms") or 1e9) < 1500 * max(n, 9) / 9, f"{what} in {res.get('ms')} ms ({n} renders; budget 1500 ms per 9)")
    check((renders is None or n == renders) and (cost.get("max_engine_ms") or 1e9) < 30, f"render cost {cost}")
    return out


def back_to_back(lead, report):
    """The M4 final audit's dropped reply: a gallery call right after another, with the project
    sync of the first still writing, failed with no_answer one time in three. 30 rounds back to
    back must all be answered."""
    print("\n-- gallery rounds back to back --")
    space = [{"name": "width", "target": "scale.x", "min": 0.75, "max": 1.4, "default": 1.0}]
    call(lead, "gallery_create", id="drop", title="Back to back", subject={"recipe": "recipes/tombstone.json"}, space=space, variants=2)
    codes = {}
    for k in range(30):
        r = lead.request("tools/call", {"name": "gallery_round", "arguments": {"id": "drop", "variants": [{"width": round(0.8 + 0.01 * k, 3)}]}})
        code = json.loads(r["content"][0]["text"]).get("error") if r.get("isError") else "ok"
        codes[code] = codes.get(code, 0) + 1
    report["back_to_back"] = codes
    # stale_round is an answer (the previous round's render landed in between: read it again);
    # no_answer or reply_dropped is the dropped reply.
    answered = codes.get("ok", 0) + codes.get("stale_round", 0)
    check(answered == 30 and codes.get("ok", 0) >= 20, f"30 gallery rounds back to back are all answered, none no_answer ({codes})")
    call(lead, "gallery_delete", id="drop")


def restart(exe, a, out, assets, report):
    """A review probe: after a restart on a fresh captures folder the open round's sheet was gone
    and the window could never decide it. The engine renders it again."""
    print("\n-- after a restart on a fresh captures folder --")
    args = ["--adapter", a.adapter, "--size", "1280x800", "--assets", str(assets), "--captures", str(out / "captures-restart"), "--worlds", str(out / "worlds"), "--saves", str(out / "saves"),
            "--lead", "conductor", "--owner", "owner-proxy", "--reviewer", "critic", "--reviewer", "pacer", "--reviewer", "pacer2", "--", "--seed", "7", "--mute"]
    with Engine(exe, a.port, out / "engine-restart.log", *args, cwd=str(ROOT)) as eng:
        lead = eng.mcp("lead")
        lead.hello("conductor", role="lead")
        hwnd = wait_for(lambda: find_window(eng.proc.pid), 15)
        win = Window(hwnd, lead, out)
        call(lead, "gallery_open", id="restart")
        s = win.ready("restart", 1)
        check(bool(s.get("sheet_shown")), "the open round's sheet is made again and shown")
        # A sight-skeptic finding: the first tiles after a restart were drawn before the world's
        # models were built (no headstones). The sheet made again measures as the one before.
        before = sorted((out / "captures").glob("run-*/gallery-restart-r1.json"), key=lambda p: p.stat().st_mtime)
        after = sorted((out / "captures-restart").glob("run-*/gallery-restart-r1.json"), key=lambda p: p.stat().st_mtime)
        if check(bool(before and after), "both sheets' packets are there"):
            m0 = [t["measures"] for t in json.loads(before[-1].read_text())["tiles"]]
            m1 = [t["measures"] for t in json.loads(after[-1].read_text())["tiles"]]
            gaps = [round(abs(a["edge_density"] - b["edge_density"]) + abs(a["contrast"] - b["contrast"]), 4) for a, b in zip(m0, m1)]
            check(len(m0) == len(m1) and max(gaps, default=1) <= 0.004, f"the sheet made again shows the same world (measure gaps {gaps})")
        win.shot("restart_round1")
        click_rect(hwnd, s["rects"]["tile1"])
        time.sleep(0.4)
        win.press("restart", "gl_accept")
        ok = wait_for(lambda: call(lead, "gallery_get", id="restart").get("status") == "accepted", 10)
        check(ok, "and decided at the window")
    check(eng.validation_errors() == 0, f"no validation errors after the restart ({eng.validation_errors()})")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=7610)
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--exe")
    ap.add_argument("--out", default=str(ROOT / "target" / "gallery-check"))
    ap.add_argument("--pacing", action="store_true")
    a = ap.parse_args()
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    exe = Path(a.exe) if a.exe else target_dir / "release" / ("sepulcra.exe" if sys.platform == "win32" else "sepulcra")
    out = Path(a.out)
    for d in ("assets", "worlds", "saves"):
        if (out / d).exists():
            shutil.rmtree(out / d)
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("window_*.png"):
        old.unlink()
    assets = out / "assets"
    shutil.copytree(ROOT / "games" / "sepulcra" / "assets", assets)
    report = {"assets": assets}
    args = ["--adapter", a.adapter, "--size", "1280x800", "--assets", str(assets), "--captures", str(out / "captures"), "--worlds", str(out / "worlds"), "--saves", str(out / "saves"),
            "--lead", "conductor", "--owner", "owner-proxy", "--reviewer", "critic", "--reviewer", "pacer", "--reviewer", "pacer2", "--", "--seed", "7", "--mute"]
    with Engine(exe, a.port, out / "engine.log", *args, cwd=str(ROOT)) as eng:
        lead, owner, reviewer = eng.mcp("lead"), eng.mcp("owner"), eng.mcp("reviewer")
        lead.hello("conductor", role="lead")
        owner.hello("owner-proxy", role="owner")
        reviewer.hello("critic", role="reviewer")
        tools = lead.tools()
        mine = ["gallery_create", "gallery_get", "gallery_delete", "gallery_list", "gallery_open", "gallery_close", "gallery_wait", "gallery_decide", "gallery_round", "gallery_apply",
                "taste_query", "taste_suggest", "board_measure", "board_check", "style_onboard"]
        check(all(t in tools for t in mine), "the taste tools are listed")
        call(lead, "play_new", seed=7)
        time.sleep(2.0)
        hwnd = wait_for(lambda: find_window(eng.proc.pid), 15)
        if not check(bool(hwnd), "the engine's window is found"):
            return 1
        win = Window(hwnd, lead, out)
        if a.pacing:
            pacing(eng, lead, report)
        else:
            moonlight(lead, win, report)
            agents(eng, lead, owner, report)
            headstones(lead, owner, reviewer, report)
            cant_afford(lead, win, report)
            board(lead, report, out / "captures")
            onboarding(lead, owner, win, report)
            # The HUD with the new "can't afford" colour (a sight capture with the interface).
            call(lead, "gallery_close")
            time.sleep(0.5)
            win.shot("hud_after")
            ps = call(lead, "play_state")
            check(ps.get("paused") is False, f"the game resumes when the screen closes (paused={ps.get('paused')})")
            # An open gallery left for the restart below.
            # Fog thin enough to vanish above Sepulcra's mist renders four identical tiles (a
            # sight-skeptic finding): the round says so instead of asking the owner to choose.
            # The agent's own round (never replaced), with densities a step apart: whether the
            # fog shows at all from up there, these four render alike.
            fog = call(lead, "gallery_create", id="fog_only", title="Fog from above", subject={"scene": {"camera": "window"}}, space=[{"name": "fog", "target": "fog.density", "min": 0, "max": 0.04}], variants=4)
            r = call(lead, "gallery_round", id="fog_only", variants=[{"fog": 0.010}, {"fog": 0.011}, {"fog": 0.012}, {"fog": 0.013}])
            fog = wait_for(lambda: (lambda j: j if j.get("rounds") == r.get("round") and j.get("status") == "open" else None)(call(lead, "gallery_get", id="fog_only")), 30) or {}
            alike = (fog.get("warning") or {}).get("rendered_alike", [])
            check(len(alike) >= 1, f"a parameter that shows nothing is reported (round {fog.get('rounds')}, rendered_alike {alike})")
            # Taste review 2: the screen closing resumed a run an agent had paused itself.
            call(lead, "gallery_open", id="fog_only")
            wait_for(lambda: call(lead, "play_state").get("paused") is True, 5)
            call(lead, "play_control", paused=True)
            call(lead, "gallery_close")
            time.sleep(0.6)
            check(call(lead, "play_state").get("paused") is True, "an agent's own pause outlasts the gallery screen")
            call(lead, "play_control", paused=False)
            check(wait_for(lambda: call(lead, "play_state").get("paused") is False, 5), "and it resumes when the agent says")
            g = call(lead, "gallery_create", id="restart", title="After a restart", subject={"scene": {"camera": "window"}},
                     space=[{"name": "moon", "target": "light.key_color", "kind": "colour", "base": "#d0d9f0", "spread": {"l": 0.1, "c": 0.06, "h": 60}}], variants=4)
            check(not g.get("warning"), f"a visible parameter's round has no look-alike tiles ({g.get('warning')})")
            check(g.get("rounds") == 1, "a gallery left open for the restart")
            f = call(lead, "sight_frame", width=1280, height=800, include_ui=True, ids=False, **view_of(report))
            shutil.copy(f["image"]["path"], out / "hud_after.png")
            q = call(lead, "taste_query", limit=64)
            report["store"] = {"records": q.get("total"), "galleries": q.get("galleries"), "priors": q.get("priors")}
            check(q.get("total", 0) >= 13, f"the preference store holds every decision ({q.get('total')} records in {q.get('galleries')} galleries)")
            report["window_shots"] = win.shots
            back_to_back(lead, report)
        # One tool call, one commit (AGENTS rule 14; the M4 final audit: gallery_apply and an
        # applied suggestion committed twice).
        multi = call(lead, "engine_info", response_format="detailed").get("swarm", {}).get("multi_commit_calls")
        report["multi_commit_calls"] = multi
        check(multi == 0, f"no tool call committed twice (multi_commit_calls {multi})")
    check(eng.validation_errors() == 0, f"no validation errors ({eng.validation_errors()})")
    if not a.pacing:
        restart(exe, a, out, assets, report)
    report["assets"] = "<out>/assets"
    sheets = sorted(str(p.name) for p in (out / "captures").rglob("gallery-*.png"))
    report["sheets"] = sheets
    (out / "report.json").write_text(json.dumps(report, indent=2, default=str))
    print(f"\n{len(FAILS)} failures" if FAILS else "\nall checks passed")
    for f in FAILS:
        print("  - " + f)
    return 1 if FAILS else 0


if __name__ == "__main__":
    sys.exit(main())
