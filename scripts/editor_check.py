#!/usr/bin/env python3
"""The human editor, end to end in the real window (docs/design/EDITOR.md; ROADMAP M5 "Human editor
on the protocol").

Opens Sepulcra in the window on the integrated GPU, with an operator grant for the owner's proxy
agent, and drives the editor as the human would: F7 and a click through the window's own message
queue (the OS input path), the rest through `editor_input` (simulated window input, owner only).
It checks, failing loudly:

- click select picks the entity under the pixel (the ID buffer), and a box select both crates;
- a translate drag with snapping moves the crate to a 0.25 m grid line, and **while dragging the
  world is unchanged** (the preview writes nothing: no journal entry until the release);
- rotate and scale drags snap to 15 degrees and 0.1;
- an inspector edit (type a value, Enter) sets it;
- undo (Ctrl+Z) and redo (Ctrl+Y) go through world_undo;
- every edit is journaled as window:owner, and agents see them in world_history and world_changes;
- a claim held by an agent refuses the human's edit (drag and inspector), saying whose claim it is;
- a builder (not the owner) is refused editor_input (owner_only);
- review R2's probe 9, kept as a step: the human's own Delete while an agent's editor_input wait frames run is
  journaled as the human's alone (no via, not the agent's call);
- review R1's probes, kept as steps: an edit driven through editor_input is journaled with the agent as
  `via` and the call as `call` (probe 5); an agent's change made while the human types refuses the
  human's value (stale_version, naming who) instead of being lost (probe 6); once an agent undid the
  human's top entry, Ctrl+Z sets it aside and goes on (probe 4); editor_input is refused at once,
  structured (window_held), while the review screen holds the window; a drag that spans a world_load
  is cancelled and writes nothing (probe 1);
- the window's frame time stays inside perf.example.toml's frame budgets with the editor open;
- zero validation errors at shutdown;
- then, in a plain engine (`--big N`, default 100000 entities; 0 skips it), the outliner's search over
  N entities keeps the CPU-frame and allocation budgets (probe 2).

The physical cursor is parked outside the window first, so it never mixes with injected input.

Screenshots of the real window go to the output folder (look at them: sight-skeptic checks them).

    cargo build -p sepulcra-app -p specula --release
    python scripts/editor_check.py [--adapter integrated] [--port 7560] [--frames 300] [--out DIR] [--exe PATH]
                                   [--big N] [--big-exe PATH]

Keep output ASCII.
"""
import argparse
import ctypes
import json
import shutil
import struct
import sys
import threading
import time
import zlib
from pathlib import Path

from specula_mcp import ROOT, Engine
from sepulcra_play import EXE
from m2_exit import PERF_TOML, is_frame_metric, other_engines

FAILS = []


def check(ok, what):
    print(("ok   " if ok else "FAIL ") + what)
    if not ok:
        FAILS.append(what)
    return ok


def call(mcp, tool, **args):
    """A tool's JSON answer (raises on an error answer)."""
    return mcp.json(tool, **args)


def raw(mcp, tool, **args):
    """(error text or None, JSON answer) without raising."""
    r = mcp.request("tools/call", {"name": tool, "arguments": args})
    text = next((c["text"] for c in r.get("content", []) if c.get("type") == "text"), "")
    try:
        j = json.loads(text)
    except ValueError:
        j = {"text": text}
    return (text if r.get("isError") else None), j


def wait_for(pred, timeout=20.0, step=0.1):
    end = time.time() + timeout
    while time.time() < end:
        v = pred()
        if v:
            return v
        time.sleep(step)
    return pred()


# ---- the real window (Win32, out of process): the messages a human's keyboard and mouse send ----

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


def park_cursor(hwnd):
    """Moves the physical cursor off the window (beside it), so its moves never mix with the
    injected input (review R1: an OS click once selected 12 entities when the cursor hovered)."""
    if not hwnd:
        return
    r = wintypes.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(r))
    x = r.right + 40 if r.left < 200 else r.left - 40
    user32.SetCursorPos(int(x), int(r.top + 10))


def os_key(hwnd, vk):
    user32.PostMessageW(hwnd, 0x0100, vk, 1)
    time.sleep(0.06)
    user32.PostMessageW(hwnd, 0x0101, vk, 1 | (0xC0 << 24))
    time.sleep(0.1)


def os_click(hwnd, x, y):
    lp = (int(y) << 16) | (int(x) & 0xFFFF)
    user32.PostMessageW(hwnd, 0x0200, 0, lp)
    time.sleep(0.05)
    user32.PostMessageW(hwnd, 0x0201, 1, lp)
    time.sleep(0.08)
    user32.PostMessageW(hwnd, 0x0202, 0, lp)
    time.sleep(0.15)


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
    raw_rows = bytearray()
    for y in range(h):
        row = pixels.raw[y * w * 4:(y + 1) * w * 4]
        raw_rows.append(0)
        for x in range(0, w * 4, 4):
            raw_rows += bytes((row[x + 2], row[x + 1], row[x]))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw_rows), 6)) + chunk(b"IEND", b"")
    Path(path).write_bytes(png)
    LAST_SHOT[:] = [w, h, pixels.raw]
    return ok != 0 and len({pixels.raw[i:i + 3] for i in range(0, len(pixels.raw), 4 * 97)}) > 2


LAST_SHOT = []


def patch_colour(p, r=4):
    """Mean RGB of the last screenshot in a (2r+1)^2 patch at pixel p."""
    w, h, px = LAST_SHOT
    acc, n = [0, 0, 0], 0
    for y in range(int(p[1]) - r, int(p[1]) + r + 1):
        for x in range(int(p[0]) - r, int(p[0]) + r + 1):
            if 0 <= x < w and 0 <= y < h:
                i = (y * w + x) * 4
                acc[0] += px[i + 2]
                acc[1] += px[i + 1]
                acc[2] += px[i]
                n += 1
    return [round(c / max(n, 1)) for c in acc]


def orange_pixels(p, r=10):
    """crate_a-coloured pixels of the last screenshot within r of p (the gizmo's lines cover some)."""
    w, h, px = LAST_SHOT
    n = 0
    for y in range(int(p[1]) - r, int(p[1]) + r + 1):
        for x in range(int(p[0]) - r, int(p[0]) + r + 1):
            if 0 <= x < w and 0 <= y < h:
                i = (y * w + x) * 4
                if orange((px[i + 2], px[i + 1], px[i])):
                    n += 1
    return n


def orange(c):
    """crate_a's colour (#c8783c), lit or shaded (about #824c2d and #633722 on screen): its green
    over red and blue over green ratios, which the gizmo's yellow, red and cyan don't have."""
    r, g, b = c[0], max(c[1], 1), c[2]
    return r > 60 and 0.40 <= g / r <= 0.72 and 0.35 <= b / g <= 0.80


# ---- the editor ----

class Editor:
    def __init__(self, owner, hwnd, out):
        self.owner, self.hwnd, self.out, self.shots = owner, hwnd, out, []

    def state(self, detailed=False):
        return call(self.owner, "editor_state", detailed=detailed)

    def settled(self, timeout=10.0):
        """editor_state once nothing is in flight (pick, edits, input frames)."""
        def ok():
            s = self.state()
            b = s.get("busy", {})
            return s if not b.get("pick") and not b.get("edits") and not b.get("input_frames") else None
        return wait_for(ok, timeout) or self.state()

    def input(self, *steps):
        park_cursor(self.hwnd)
        err, j = raw(self.owner, "editor_input", steps=list(steps))
        if err:
            raise RuntimeError(f"editor_input: {err}")
        return self.settled()

    def widget(self, name):
        return self.state(detailed=True).get("widgets", {}).get(name)

    def press_widget(self, name):
        r = self.widget(name)
        if not r:
            check(False, f"the editor shows widget {name}")
            return None
        return self.input({"click": [r[0] + r[2] / 2, r[1] + r[3] / 2]})

    def shot(self, name):
        p = self.out / f"window_{len(self.shots):02d}_{name}.png"
        time.sleep(0.25)
        ok = screenshot(self.hwnd, p)
        self.shots.append(str(p))
        check(ok, f"screenshot {p.name} is not blank")
        # Every coordinate here is the editor's window pixels: a display-scale change mid-run (a
        # monitor unplugged, the scale setting changed) resizes the window and invalidates them.
        win = self.state().get("window")
        if LAST_SHOT and win and [LAST_SHOT[0], LAST_SHOT[1]] != list(win):
            check(False, f"the window kept its size ({win} in editor_state, {LAST_SHOT[0]}x{LAST_SHOT[1]} on screen): the display scale changed mid-run; rerun")
        return p


def screen_centre(builder, path, cam, size):
    """Where `path` draws in the editor's view (sight_query: the engine's own projection)."""
    q = call(builder, "sight_query", path=path, eye=cam["eye"], target=cam["target"], fov_y_deg=cam.get("fov_y_deg", 50),
             width=size[0], height=size[1])
    s = q.get("screen")
    if not s:
        return None, q
    return [s[0] + s[2] / 2, s[1] + s[3] / 2], q


def history_by(mcp, path):
    """Journal entries touching `path`, newest first."""
    return call(mcp, "world_history", path_prefix=path, limit=50).get("entries") or []


def big_world_check(a, out):
    """Probe 2: the outliner's search over a huge world, measured live (release, this GPU). The
    plain engine (no game): N entities as 10 groups of N/10, the editor open, a search for
    something absent typed again and again while perf_capture records the frames."""
    import threading
    exe = Path(a.big_exe) if a.big_exe else Path(a.exe).with_name("specula.exe" if sys.platform == "win32" else "specula")
    if not exe.exists():
        check(False, f"{exe} exists for the big-world check (cargo build -p specula --release)")
        return {}
    bout = out / "big"
    bout.mkdir(parents=True, exist_ok=True)
    args = ["--adapter", a.adapter, "--size", "1280x800", "--perf", str(PERF_TOML), "--captures", str(bout / "captures"),
            "--worlds", str(bout / "worlds"), "--owner", "owner-proxy"]
    res = {"entities": a.big}
    with Engine(exe, a.port + 1, bout / "engine.log", *args, cwd=str(bout)) as eng:
        own, other = eng.mcp("owner"), eng.mcp("measure")
        call(own, "agent_hello", name="owner-proxy", role="owner")
        call(other, "agent_hello", name="measure", role="builder")
        hwnd = wait_for(lambda: find_window(eng.proc.pid), 30)
        if hwnd:
            park_cursor(hwnd)
        groups = max(a.big // 10_000, 1)
        per = a.big // groups
        t0 = time.time()
        for g in range(groups):
            call(own, "world_spawn", name=f"g{g}", shape="group")
            for b in range(0, per, 1000):
                ops = [{"op": "spawn", "name": f"e{g}_{b + i:05d}", "shape": "group", "parent": f"/g{g}"} for i in range(min(1000, per - b))]
                call(own, "world_batch", ops=ops)
        res["spawn_s"] = round(time.time() - t0, 1)
        print(f"  big world: {groups * per} entities spawned in {res['spawn_s']} s")
        call(own, "time_control", action="pause")
        ed = Editor(own, hwnd, bout)
        ed.input({"key": "F7"}, {"wait": 10})
        time.sleep(1.0)
        idle = call(other, "perf_capture", frames=90, trace=False, label="big_open_idle")
        box = ed.widget("ol_search")
        if not check(bool(box), "the outliner's search box shows"):
            return res
        ed.input({"click": [box[0] + box[2] / 2, box[1] + box[3] / 2]})
        got = {}
        th = threading.Thread(target=lambda: got.update(c=call(other, "perf_capture", frames=120, trace=False, label="big_searching")))
        th.start()
        rounds = 0
        while th.is_alive() and rounds < 20:
            raw(own, "editor_input", steps=[{"text": "zq"}, {"wait": 12}, {"key": "Backspace"}, {"key": "Backspace"}])
            rounds += 1
        th.join()
        cap = got.get("c") or {}
        st = ed.state()
        scanned = (st.get("outliner") or {}).get("entities")

        def measured(label):
            # cpu_frame (the engine thread's frame minus present) and allocations per frame, p95.
            files = sorted((bout / "captures").glob(f"*/perf-capture-*-{label}.json"))
            if not files:
                return {}
            fr = json.loads(files[-1].read_text()).get("frames") or []

            def p95(xs):
                xs = sorted(xs)
                return round(xs[int(0.95 * (len(xs) - 1))], 3) if xs else None
            cpu, allocs = [], []
            for x in fr:
                zones = {z[0]: z for z in (x.get("zones") or {}).get("main", [])}
                if "frame" in zones:
                    cpu.append(zones["frame"][2] - (zones["frame/present"][2] if "frame/present" in zones else 0))
                allocs.append((x.get("memory") or {}).get("allocations", 0))
            return {"cpu_frame_p95_ms": p95(cpu), "cpu_frame_max_ms": round(max(cpu), 3) if cpu else None, "allocations_p95": p95(allocs)}

        def metric(c, name):
            return [e for e in (c.get("budget_errors") or []) if e.get("metric") == name]
        for label, key, c in (("open, idle", "big_open_idle", idle), ("searching", "big_searching", cap)):
            m = measured(key)
            res[key] = {"frames": c.get("frames"), "frame_ms": c.get("frame_ms"), "measured": m, "budget_errors": c.get("budget_errors")}
            print(f"  big world, editor {label}: {c.get('frames')} frames, {m}, budget errors {[e.get('metric') for e in c.get('budget_errors') or []]}")
        check(cap.get("frames", 0) >= 60 and (scanned or 0) >= groups * per, f"a search ran over all {scanned} entities while {cap.get('frames')} frames were measured")
        check(not metric(cap, "cpu_frame_p95_ms"), f"searching {a.big} entities keeps the CPU-frame budget ({metric(cap, 'cpu_frame_p95_ms')})")
        check(not metric(cap, "allocations_per_frame_p95"), f"... and the allocation budget ({metric(cap, 'allocations_per_frame_p95')})")
        res["validation_errors"] = None
    res["validation_errors"] = eng.validation_errors()
    check(res["validation_errors"] == 0, f"big world: validation errors at shutdown: {res['validation_errors']}")
    return res


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--exe", default=str(EXE))
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--port", type=int, default=7560)
    ap.add_argument("--frames", type=int, default=300)
    ap.add_argument("--out")
    ap.add_argument("--big", type=int, default=100_000, help="entities for the outliner search budget check (0 skips it)")
    ap.add_argument("--big-exe", help="the plain engine for it (default: specula beside --exe)")
    a = ap.parse_args()
    if sys.platform != "win32":
        print("skip: the editor check drives the real window (Windows only for now)")
        return 0
    exe = Path(a.exe)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p sepulcra-app --release")
    out = Path(a.out) if a.out else ROOT / "target" / "editor-check" / a.adapter
    if out.exists():
        shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True, exist_ok=True)
    args = ["--adapter", a.adapter, "--size", "1280x800", "--perf", str(PERF_TOML), "--captures", str(out / "captures"),
            "--worlds", str(out / "worlds"), "--saves", str(out / "saves"), "--owner", "owner-proxy", "--", "--seed", "7", "--mute",
            # The game's own view, not the intro flyover's (its first pose is far out), and no
            # title screen holding the run (it hides the HUD).
            "--intro", "off", "--title", "off"]
    report = {"adapter": a.adapter}
    with Engine(exe, a.port, out / "engine.log", *args, cwd=str(out)) as eng:
        owner, builder, claimer = eng.mcp("owner"), eng.mcp("builder"), eng.mcp("claimer")
        for m, name, role in [(owner, "owner-proxy", "owner"), (builder, "builder-1", "builder"), (claimer, "claimer", "builder")]:
            # The game loads its models first: the engine may answer busy for a while.
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

        # -- two crates in front of the game's camera, one claimed by another agent --
        cam = (info.get("window") or {}).get("camera")
        target = cam.get("target", [0, 0, 0]) if isinstance(cam, dict) else [0, 0, 0]
        call(builder, "world_spawn", name="editor_test", shape="group", position=[target[0], target[1] + 1.2, target[2]])
        a_ = call(builder, "world_spawn", name="crate_a", parent="/editor_test", shape="cube", position=[-2.0, 0, 0], color="#c8783c")
        b_ = call(builder, "world_spawn", name="crate_b", parent="/editor_test", shape="cube", position=[2.0, 0, 0], color="#3c78c8")
        ida, idb = a_["id"], b_["id"]
        # Raised until both crates show (the slab's blocks hide anything placed low).
        if isinstance(cam, dict):
            for up in [4.0, 6.0, 9.0, 13.0, 18.0]:
                call(builder, "world_set", path="/editor_test", position=[target[0], target[1] + up, target[2]])
                seen = []
                for p in ("/editor_test/crate_a", "/editor_test/crate_b"):
                    q = call(builder, "sight_query", path=p, eye=cam["eye"], target=cam["target"], fov_y_deg=cam.get("fov_y_deg", 50), width=1280, height=800)
                    seen.append(q.get("visible_px") or 0)
                if min(seen) >= 80:
                    report["crates_raised_m"] = up
                    break
            check(min(seen) >= 80, f"both crates are visible in the game's view ({seen} px)")
        call(claimer, "claim_take", path="/editor_test/crate_b", lease_s=600, note="painting it")

        # -- open the editor with the human's key (the OS path) --
        os_key(hwnd, 0x76)  # F7
        st = wait_for(lambda: (lambda s: s if s.get("open") else None)(call(owner, "editor_state")), 10)
        if not check(bool(st and st.get("open")), "F7 in the window opens the editor"):
            return 1
        ed = Editor(owner, hwnd, out)
        ps = call(owner, "play_state")
        check(ps.get("paused") is True, f"the game pauses while the editor holds the window (paused={ps.get('paused')})")
        st = ed.settled()
        size = st["window"]
        cam = st["camera"]
        # Frame the test group: select it in the outliner? Pick by the camera instead: point it at them.
        ca, qa = screen_centre(builder, "/editor_test/crate_a", cam, size)
        cb, qb = screen_centre(builder, "/editor_test/crate_b", cam, size)
        if not check(ca is not None and cb is not None, f"both crates are on screen in the editor's view (centres {ca} and {cb}; {qa.get('visible_px')} and {qb.get('visible_px')} px visible through its camera)"):
            return 1
        vp = st["viewport"]
        inside = lambda p: vp[0] <= p[0] < vp[0] + vp[2] and vp[1] <= p[1] < vp[1] + vp[3]
        check(inside(ca) and inside(cb), f"the crates are in the viewport between the panels ({ca}, {cb}, viewport {vp})")
        ed.shot("editor_open")
        before_px = patch_colour(ca)
        check(orange(before_px), f"crate_a draws at its spot before anything is selected (mean colour {before_px})")

        # -- click select: the OS path, then editor_input --
        park_cursor(hwnd)
        os_click(hwnd, cb[0], cb[1])
        s = wait_for(lambda: (lambda s: s if s.get("selection") else None)(ed.settled()), 10) or {}
        sel = [x["id"] for x in s.get("selection", [])]
        check(sel == [idb], f"a click in the window selects crate_b from the ID buffer (selection {sel})")
        s = ed.input({"click": ca})
        sel = [x["id"] for x in s.get("selection", [])]
        check(sel == [ida], f"editor_input's click selects crate_a (selection {sel})")
        insp = s.get("inspector") or {}
        check(insp.get("id") == ida and insp.get("created_by") == "builder-1", f"the inspector shows crate_a, made by builder-1 ({insp.get('path')}, {insp.get('created_by')})")
        rows = s.get("outliner", {}).get("rows", [])
        check(any(r.get("id") == ida for r in rows), "the outliner reveals the selected crate")
        claimed_rows = [r for r in rows if r.get("id") == idb]
        check(bool(claimed_rows) and claimed_rows[0].get("claimed_by") == "claimer", f"the outliner shows crate_b claimed by claimer ({claimed_rows})")
        ed.shot("selected_crate_a")

        # -- translate drag with snapping; the world is unchanged until the release --
        before = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["position", "version"])["items"][0]
        last_seq = call(builder, "world_history", limit=1).get("entries", [{}])
        g = s.get("gizmo") or {}
        hx, o = (g.get("handles") or {}).get("axis_x"), g.get("origin")
        if not check(bool(hx and o), f"the translate gizmo shows its X handle ({g})"):
            return 1
        d = [hx[0] - o[0], hx[1] - o[1]]
        n = max((d[0] ** 2 + d[1] ** 2) ** 0.5, 1)
        to = [hx[0] + d[0] / n * 140, hx[1] + d[1] / n * 140]
        err, _ = raw(owner, "editor_input", steps=[{"press": hx}, {"move": [(hx[0] + to[0]) / 2, (hx[1] + to[1]) / 2]}, {"move": to}, {"wait": 3}])
        check(err is None, f"a drag starts on the X handle ({err})")
        mid = ed.state()
        check(mid.get("dragging") is True, "the drag is in progress")
        during = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["position", "version"])["items"][0]
        check(during == before, f"while dragging, the world is unchanged (preview only): {before} -> {during}")
        now_seq = call(builder, "world_history", limit=1).get("entries", [{}])
        check(now_seq == last_seq, "while dragging, nothing was journaled")
        ed.shot("dragging_preview")
        during_px = patch_colour(ca)
        check(not orange(during_px), f"while dragging, the window draws crate_a moved away from its spot: the preview patches the draws (mean colour there {during_px})")
        moved_to = (mid.get("gizmo") or {}).get("origin")
        seen = orange_pixels(moved_to) if moved_to else 0
        check(seen >= 60, f"... and draws it where the drag has it, under the gizmo ({seen} crate-coloured pixels at {moved_to})")
        s = ed.input({"release": to})
        after = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["position", "version"])["items"][0]
        world_x = None
        moved = after["position"][0] - before["position"][0]
        grp = call(builder, "world_query", path_prefix="/editor_test", fields=["position"])["items"][0]["position"]
        world_x = grp[0] + after["position"][0]
        check(moved > 0.2, f"the drag moved crate_a along +X ({before['position']} -> {after['position']})")
        check(abs(world_x / 0.25 - round(world_x / 0.25)) < 1e-3, f"snapping put it on a 0.25 m grid line (world x {world_x:.4f})")
        check(after["position"][1:] == before["position"][1:], "only X changed")
        hist = history_by(builder, "/editor_test/crate_a")
        last = hist[0] if hist else {}
        check(last.get("by") == "window:owner" and last.get("kind") in ("batch", "set"), f"the move is one journal entry by window:owner ({json.dumps(last)[:200]})")
        report["move"] = {"before": before, "after": after, "journal": last.get("id") or last.get("seq")}
        ed.shot("moved")

        # -- rotate and scale, snapped --
        ed.input({"key": "E"}, {"wait": 2})
        g = ed.state().get("gizmo") or {}
        hy, o = (g.get("handles") or {}).get("ring_y"), g.get("origin")
        if check(bool(hy and o), "the rotate gizmo shows its Y ring"):
            ed.input({"drag": hy, "to": [hy[0] + 70, hy[1] + 25], "frames": 8}, {"wait": 2})
            r = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["rotation_deg"])["items"][0]["rotation_deg"]
            check(any(abs(v) > 1 for v in r) and all(abs(v / 15 - round(v / 15)) < 1e-3 for v in r), f"a ring drag turns it by 15 degree steps ({r})")
            ed.shot("rotated")
        ed.input({"key": "R"}, {"wait": 2})
        g = ed.state().get("gizmo") or {}
        hx, o = (g.get("handles") or {}).get("axis_x"), g.get("origin")
        if check(bool(hx and o), "the scale gizmo shows its X handle"):
            d = [hx[0] - o[0], hx[1] - o[1]]
            ed.input({"drag": hx, "to": [hx[0] + d[0] * 0.8, hx[1] + d[1] * 0.8], "frames": 8}, {"wait": 2})
            sc = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["scale"])["items"][0]["scale"]
            check(sc[0] > 1.2 and abs(sc[0] / 0.1 - round(sc[0] / 0.1)) < 1e-3 and sc[1] == 1 and sc[2] == 1, f"an axis drag scales X by 0.1 steps ({sc})")
        ed.input({"key": "W"}, {"wait": 2})

        # -- the inspector: type a value --
        s = ed.press_widget("f_py")
        check((s.get("typing") or {}).get("field") == "py", f"clicking the Y field starts typing it ({s.get('typing')})")
        # Input sweep R3: what the human types is the owner's alone (docs/design/SECURITY.md).
        s = ed.input({"text": "7"}, {"wait": 1})
        own_t, b_t = s.get("typing") or {}, call(builder, "editor_state").get("typing") or {}
        check(own_t.get("text") is not None and "text" not in b_t and b_t.get("redacted") is True and "chars" not in b_t,
              f"the owner's editor_state shows the field being typed; another agent's has only its length ({own_t} / {b_t})")
        ed.input({"key": "Backspace"}, {"wait": 1})
        s = ed.input({"text": "1.5"}, {"key": "Enter"}, {"wait": 2})
        y = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["position"])["items"][0]["position"][1]
        check(abs(y - 1.5) < 1e-6, f"typing 1.5 and Enter sets position Y ({y})")
        hist = history_by(builder, "/editor_test/crate_a")
        check(bool(hist) and hist[0].get("by") == "window:owner", "the inspector edit is journaled as window:owner")
        ed.shot("inspector_edit")

        # -- undo and redo --
        s = ed.input({"key": "Ctrl+Z"}, {"wait": 2})
        y2 = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["position"])["items"][0]["position"][1]
        check(abs(y2 - before["position"][1]) < 1e-6, f"Ctrl+Z undoes it (Y {y} -> {y2})")
        hist = history_by(builder, "/editor_test/crate_a")
        check(bool(hist) and hist[0].get("by") == "window:owner" and hist[0].get("kind") == "undo", f"the undo is a world_undo entry by window:owner ({json.dumps(hist[0] if hist else {})[:160]})")
        s = ed.input({"key": "Ctrl+Y"}, {"wait": 2})
        y3 = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["position"])["items"][0]["position"][1]
        check(abs(y3 - 1.5) < 1e-6, f"Ctrl+Y redoes it ({y3})")

        # -- one editor_input call, one commit (AGENTS rule 14) --
        n_before = len(history_by(builder, "/editor_test/crate_a"))
        s = ed.input({"key": "Ctrl+Z"}, {"key": "Ctrl+Y"}, {"wait": 2})
        n_after = len(history_by(builder, "/editor_test/crate_a"))
        st_text = s.get("status", {}).get("text", "")
        check(n_after == n_before + 1 and "one edit" in st_text, f"two edits in one editor_input call: the first is made, the second refused ({n_after - n_before} new entries; {st_text})")
        s = ed.input({"key": "Ctrl+Y"}, {"wait": 2})
        y4 = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["position"])["items"][0]["position"][1]
        check(abs(y4 - 1.5) < 1e-6, f"... and the redo goes through in a call of its own ({y4})")

        # -- probe 5: an edit an agent drove is journaled with the agent (via) and its call --
        hist = call(builder, "world_history", path_prefix="/editor_test/crate_a", limit=1, response_format="detailed").get("entries") or [{}]
        e5 = hist[0]
        check(e5.get("by") == "window:owner" and e5.get("via") == "owner-proxy", f"an editor_input edit is the human's, via the agent that drove it ({json.dumps(e5)[:200]})")
        tc = call(owner, "trace_call", call=e5.get("call", 0)) if e5.get("call") else {}
        tool = (tc.get("call") or {}).get("tool")
        check(tool == "editor_input" and e5.get("id") in (tc.get("journal") or []), f"... and its call is the editor_input call, which lists the entry (call {e5.get('call')}: {tool}, journal {tc.get('journal')})")

        # -- probe 6: an agent's change while the human types refuses the human's value --
        s = ed.press_widget("f_px")
        check((s.get("typing") or {}).get("field") == "px", f"clicking the X field starts typing it ({s.get('typing')})")
        ed.input({"text": "3"})
        cur = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["position"])["items"][0]["position"]
        theirs = [round(cur[0] + 0.5, 3), cur[1], cur[2]]
        call(builder, "world_set", path="/editor_test/crate_a", position=theirs)
        s = ed.input({"key": "Enter"}, {"wait": 3})
        last = s.get("last_edit") or {}
        st_text = s.get("status", {}).get("text", "")
        check((last.get("answer") or {}).get("error") == "stale_version" and "builder-1" in st_text, f"Enter after an agent's change is refused stale_version, naming who changed it ({st_text[:200]})")
        now = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["position"])["items"][0]["position"]
        check(abs(now[0] - theirs[0]) < 1e-6, f"... and the agent's value stays ({now[0]}, theirs {theirs[0]})")

        # -- review R3 of m5-text: a click in the world never points the window's keyboard at a
        # text; the human types a text's words in the inspector's Text field instead --
        call(builder, "world_spawn", name="epitaph", parent="/editor_test", shape="text", position=[3, 0, -3], visible=False, text={"text": "Here lies", "size": 0.3})
        def e_row():
            rows = (ed.state().get("outliner") or {}).get("rows") or []
            return next((r["row"] for r in rows if r.get("path") == "/editor_test/epitaph"), None)
        row = wait_for(lambda: e_row(), 5)
        box = ed.widget(f"ol_row[{row}]") if row is not None else None
        if check(bool(box), f"the epitaph shows in the outliner (row {row})"):
            ed.input({"click": [box[0] + box[2] / 2, box[1] + box[3] / 2]})
            s = ed.press_widget("in_text")
            typing = s.get("typing") or {}
            check(typing.get("field") == "text", f"clicking the inspector's Text field starts typing the text's words ({typing})")
            ed.input(*([{"key": "Backspace"}] * 4), {"text": "rests"}, {"key": "Enter"}, {"wait": 2})
            words = (call(builder, "world_query", path_prefix="/editor_test/epitaph", fields=["text"])["items"][0].get("text") or {}).get("text")
            check(words == "Here rests", f"Backspace, typing and Enter set the text's words ({words!r})")
            hist = history_by(builder, "/editor_test/epitaph")
            check(bool(hist) and hist[0].get("by") == "window:owner", "the text edit is journaled as window:owner")
        # Review R4 of m5-text (probe_inspector): a text longer than 512 characters is never cut,
        # and Enter with nothing changed writes nothing.
        long = "epitaph " * 187 + "abcd"
        call(builder, "world_spawn", name="long_epitaph", parent="/editor_test", shape="text", position=[3, 0, -4], visible=False, text={"text": long, "size": 0.3})
        def l_row():
            rows = (ed.state().get("outliner") or {}).get("rows") or []
            return next((r["row"] for r in rows if r.get("path") == "/editor_test/long_epitaph"), None)
        row = wait_for(lambda: l_row(), 5)
        box = ed.widget(f"ol_row[{row}]") if row is not None else None
        if check(bool(box), f"the 1,500-character epitaph shows in the outliner (row {row})"):
            ed.input({"click": [box[0] + box[2] / 2, box[1] + box[3] / 2]})
            n0 = len(history_by(builder, "/editor_test/long_epitaph"))
            s = ed.press_widget("in_text")
            check(len((s.get("typing") or {}).get("text") or "") == 1500, f"the Text field starts with all 1,500 characters ({len((s.get('typing') or {}).get('text') or '')})")
            ed.input({"key": "Enter"}, {"wait": 2})
            words = (call(builder, "world_query", path_prefix="/editor_test/long_epitaph", fields=["text"])["items"][0].get("text") or {}).get("text")
            n1 = len(history_by(builder, "/editor_test/long_epitaph"))
            check(words == long and n1 == n0, f"Enter with nothing changed writes nothing ({len(words or '')} characters, {n1 - n0} new entries)")
            ed.press_widget("in_text")
            ed.input({"text": "!"}, {"key": "Enter"}, {"wait": 2})
            words = (call(builder, "world_query", path_prefix="/editor_test/long_epitaph", fields=["text"])["items"][0].get("text") or {}).get("text")
            check(words == long + "!", f"a key typed at its end keeps the whole text ({len(words or '')} characters)")

        # -- probe 4: an agent undoes the human's top entry; Ctrl+Z sets it aside and goes on --
        # Edit A on crate_c (hidden, so no capture shows it), edit B on crate_a, B undone by the
        # owner's agent; then Ctrl+Z twice.
        call(builder, "world_spawn", name="crate_c", parent="/editor_test", shape="cube", position=[0, 0, -3], visible=False)
        def c_row():
            rows = (ed.state().get("outliner") or {}).get("rows") or []
            return next((r["row"] for r in rows if r.get("path") == "/editor_test/crate_c"), None)
        row = wait_for(lambda: c_row(), 5)
        box = ed.widget(f"ol_row[{row}]") if row is not None else None
        if check(bool(box), f"crate_c shows in the outliner (row {row})"):
            ed.input({"click": [box[0] + box[2] / 2, box[1] + box[3] / 2]})
        s = ed.press_widget("f_pz")
        z0 = call(builder, "world_query", path_prefix="/editor_test/crate_c", fields=["position"])["items"][0]["position"][2]
        s = ed.input({"text": str(round(z0 + 0.5, 3))}, {"key": "Enter"}, {"wait": 2})
        z1 = call(builder, "world_query", path_prefix="/editor_test/crate_c", fields=["position"])["items"][0]["position"][2]
        check(abs(z1 - (z0 + 0.5)) < 1e-6, f"the human moves crate_c in the inspector (z {z0} -> {z1})")
        s = ed.input({"key": "Escape"}, {"wait": 1})
        st = ed.settled()
        ca2, _ = screen_centre(builder, "/editor_test/crate_a", st["camera"], st["window"])
        s = ed.input({"click": ca2})
        s = ed.press_widget("f_py")
        s = ed.input({"text": "2"}, {"key": "Enter"}, {"wait": 2})
        undo = s.get("undo") or []
        top = undo[0] if undo else None
        err, _ = raw(owner, "world_undo", ids=[top], force=True)
        check(top is not None and err is None, f"the owner's agent undoes the human's top entry {top} ({(err or '')[:120]})")
        s = ed.input({"key": "Ctrl+Z"}, {"wait": 2})
        st_text = s.get("status", {}).get("text", "")
        check("set aside" in st_text and top not in (s.get("undo") or []), f"Ctrl+Z on it is refused for good and set aside ({st_text[:160]})")
        s = ed.input({"key": "Ctrl+Z"}, {"wait": 2})
        z2 = call(builder, "world_query", path_prefix="/editor_test/crate_c", fields=["position"])["items"][0]["position"][2]
        check(abs(z2 - z0) < 1e-6 and "Undid" in s.get("status", {}).get("text", ""), f"... and the next Ctrl+Z undoes the human's earlier edit on crate_c (z {z0} restored: {z2}; {s.get('status', {}).get('text', '')[:120]})")

        # -- simulated input is the editor's alone: it never reaches the review screen --
        ed.input({"key": "F9"}, {"wait": 3})
        rv = call(owner, "review_screen", action="status")
        check(rv.get("open") is not True, f"editor_input's F9 doesn't open the review screen ({rv.get('open')})")

        # -- editor_input under the review screen: refused at once, structured --
        rv_err, rv = raw(owner, "review_screen", action="open")
        if check(rv_err is None and wait_for(lambda: call(owner, "review_screen", action="status").get("open") is True, 10), f"the review screen opens over the editor ({(rv_err or '')[:120]})"):
            t0 = time.time()
            err, j = raw(owner, "editor_input", steps=[{"key": "W"}])
            took = time.time() - t0
            check(err is not None and j.get("error") == "window_held" and "review" in (j.get("held_by") or "") and j.get("retry") is True and took < 5,
                  f"editor_input while the review screen holds the window: window_held at once ({took:.1f} s; {json.dumps(j)[:200]})")
            call(owner, "review_screen", action="close")
            wait_for(lambda: call(owner, "review_screen", action="status").get("open") is not True, 10)

        # -- an agent's window_view doesn't take the window from the open editor --
        before_cam = ed.state().get("camera")
        err, _ = raw(builder, "window_view", eye=[90, 90, 90], target=[0, 0, 0])
        check(err is not None and "editor_open" in err, f"window_view says the editor holds the window ({(err or '')[:100]})")
        time.sleep(0.5)
        win_cam = (call(owner, "engine_info", response_format="detailed").get("window") or {}).get("camera")
        check(isinstance(win_cam, dict) and before_cam and win_cam.get("eye") == before_cam.get("eye"), f"the window keeps the editor's camera when an agent calls window_view ({win_cam}, editor {before_cam})")

        # -- agents see the human's edits --
        ch = call(builder, "world_changes", since=0, limit=200, path_prefix="/editor_test")
        by = {c.get("by") for c in (ch.get("changes") or [])}
        check("window:owner" in by, f"world_changes shows window:owner's edits to the agents ({sorted(x for x in by if x)})")

        # -- a claim refuses the human --
        s = ed.input({"click": cb}, {"wait": 2})
        s = ed.settled()
        sel = [x["id"] for x in s.get("selection", [])]
        check(sel == [idb], f"crate_b selected ({sel})")
        insp = s.get("inspector") or {}
        check("claimer" in (insp.get("claim") or ""), f"the inspector says crate_b is claimed by claimer ({insp.get('claim')})")
        b_before = call(builder, "world_query", path_prefix="/editor_test/crate_b", fields=["position", "version"])["items"][0]
        g = s.get("gizmo") or {}
        hx = (g.get("handles") or {}).get("axis_x")
        if hx:
            s = ed.input({"drag": hx, "to": [hx[0] + 120, hx[1]], "frames": 6}, {"wait": 2})
            st_text = s.get("status", {}).get("text", "")
            check(not s.get("status", {}).get("good") and "claimer" in st_text, f"a drag on claimed crate_b is refused, naming the claim's holder ({st_text})")
        ed.shot("claim_refused_drag")
        s = ed.press_widget("f_px")
        s = ed.input({"text": "9"}, {"key": "Enter"}, {"wait": 3})
        st_text = s.get("status", {}).get("text", "")
        check(not s.get("status", {}).get("good") and "claimer" in st_text and "claim" in st_text, f"an inspector edit of crate_b is refused by the write policy, naming claimer ({st_text})")
        last = s.get("last_edit") or {}
        check(last.get("ok") is False and (last.get("answer") or {}).get("error") == "claimed", f"the refusal is the protocol's own 'claimed' ({json.dumps(last)[:200]})")
        b_after = call(builder, "world_query", path_prefix="/editor_test/crate_b", fields=["position", "version"])["items"][0]
        check(b_after == b_before, "crate_b is unchanged")
        ed.shot("claim_refused_inspector")

        # -- box select --
        ed.input({"key": "Escape"}, {"wait": 2})
        st = ed.settled()
        cam = st["camera"]
        ca, _ = screen_centre(builder, "/editor_test/crate_a", cam, size)
        cb, _ = screen_centre(builder, "/editor_test/crate_b", cam, size)
        if ca and cb:
            # A box just around both crates (about 25 px each): the ground under them shows
            # mostly outside it.
            x0, x1 = min(ca[0], cb[0]) - 22, max(ca[0], cb[0]) + 22
            y0, y1 = min(ca[1], cb[1]) - 22, max(ca[1], cb[1]) + 22
            s = ed.input({"drag": [x0, y0], "to": [x1, y1], "frames": 6}, {"wait": 4})
            s = ed.settled()
            sel = {x["id"] for x in s.get("selection", [])}
            paths = sorted(x["path"] for x in s.get("selection", []))
            check({ida, idb} <= sel, f"a box over both crates selects both ({paths})")
            # Only what is mostly inside the box: never the slab or the ground it merely overlaps.
            big = [p for p in paths if p in ("/slab", "/slab/graveyard/ground") or p.count("/") == 1 and p != "/editor_test"]
            # The small dressing between the crates (grass tufts, fence segments, tombstones) is
            # mostly inside the box, so the rule takes it too: bounded, never the big pieces.
            check(not big and s.get("selected", 0) <= 24, f"... and not the slab or the graveyard ground it overlaps; at most 24 entities, each mostly inside the box by the engine's rule ({s.get('selected')} selected: {paths})")
            ed.shot("box_select")

        # -- authority: only the owner drives the editor --
        err, _ = raw(builder, "editor_input", steps=[{"key": "Escape"}])
        check(err is not None and "owner_only" in err, f"a builder's editor_input is refused as owner_only ({(err or '')[:120]})")
        # The try is a security incident (docs/design/SECURITY.md): the builder is frozen until the
        # owner clears it.
        ferr, _ = raw(builder, "world_query", limit=1)
        check(ferr is not None and "session_frozen" in ferr, f"... a security incident: the builder's session is frozen ({(ferr or '')[:80]})")
        call(owner, "security_clear", all=True, note="editor_check's builder probe")
        ferr, _ = raw(builder, "world_query", limit=1)
        check(ferr is None, f"... and works again once the owner clears it ({(ferr or '')[:80]})")

        # -- frame time with the editor open, on this GPU --
        call(owner, "time_control", action="resume")
        time.sleep(1.0)
        cap = call(owner, "perf_capture", frames=a.frames, trace=False, label="editor_open")
        call(owner, "time_control", action="pause")
        errors = cap.get("budget_errors") or []
        frame_misses = [e for e in errors if is_frame_metric(e.get("metric", ""))]
        gpu, fr = cap.get("gpu_ms") or {}, cap.get("frame_ms") or {}
        report["perf"] = {"frames": cap.get("frames"), "frame_ms": fr, "gpu_ms": gpu, "budget_misses": frame_misses, "notes": [e for e in errors if not is_frame_metric(e.get("metric", ""))],
                          "other_engines_running": other_engines(eng.proc.pid)}
        print(f"  editor open: {cap.get('frames')} frames, frame p95 {fr.get('p95')} ms, GPU mean {gpu.get('mean')} p95 {gpu.get('p95')} ms")
        check(cap.get("frames", 0) >= a.frames // 2 and not frame_misses, f"frame time stays in budget with the editor open ({frame_misses})")

        # -- close: the game gets the window back --
        os_key(hwnd, 0x76)
        closed = wait_for(lambda: (lambda s: s if not s.get("open") else None)(call(owner, "editor_state")), 10)
        check(bool(closed), "F7 closes the editor")
        # The same view with the editor closed, back to back: what the editor itself costs (other
        # engines on the GPU slow both alike).
        call(owner, "time_control", action="resume")
        time.sleep(1.0)
        cap2 = call(owner, "perf_capture", frames=a.frames, trace=False, label="editor_closed")
        call(owner, "time_control", action="pause")
        gpu2, fr2 = cap2.get("gpu_ms") or {}, cap2.get("frame_ms") or {}
        report["perf_closed"] = {"frames": cap2.get("frames"), "frame_ms": fr2, "gpu_ms": gpu2, "other_engines_running": other_engines(eng.proc.pid)}
        print(f"  editor closed: {cap2.get('frames')} frames, frame p95 {fr2.get('p95')} ms, GPU mean {gpu2.get('mean')} p95 {gpu2.get('p95')} ms; "
              f"other engines running: {report['perf']['other_engines_running']} / {report['perf_closed']['other_engines_running']}")
        # -- probe 1: a drag that spans an agent's world_load is cancelled and writes nothing --
        ed.input({"key": "F7"}, {"wait": 5})
        ed.input({"key": "Escape"}, {"wait": 2})
        raw(claimer, "claim_release", all=True)
        call(owner, "world_save", name="editor_check")
        st = ed.settled()
        cam = st["camera"]
        ca, _ = screen_centre(builder, "/editor_test/crate_a", cam, st["window"])
        if check(ca is not None, f"crate_a is on screen for the load probe ({ca})"):
            s = ed.input({"click": ca})
            hx = ((s.get("gizmo") or {}).get("handles") or {}).get("axis_x")
            if check(bool(hx), "crate_a's gizmo shows"):
                before_load = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["position", "version"])["items"][0]
                err, _ = raw(owner, "editor_input", steps=[{"press": hx}, {"move": [hx[0] + 40, hx[1]]}, {"move": [hx[0] + 120, hx[1]]}, {"wait": 3}])
                check(err is None and ed.state().get("dragging") is True, f"a drag is in progress ({err})")
                seq0 = (call(builder, "world_history", limit=1).get("entries") or [{}])[0].get("id")
                # A sight_compare mark taken before the load: its ids are the old world's.
                mark_err, _ = raw(builder, "sight_compare", action="mark", name="before_load", width=320, height=200)
                err, _ = raw(owner, "world_load", name="editor_check", force=True)
                check(err is None, f"an agent loads a world mid-drag ({(err or '')[:160]})")
                s = ed.state()
                check(s.get("dragging") is False and s.get("selected") == 0 and not s.get("undo"), f"the load cancels the drag and clears the selection and undo history (dragging {s.get('dragging')}, selected {s.get('selected')}, undo {s.get('undo')})")
                ed.input({"release": [hx[0] + 120, hx[1]]}, {"wait": 3})
                after_load = call(builder, "world_query", path_prefix="/editor_test/crate_a", fields=["position", "version"])["items"][0]
                hist = call(builder, "world_history", limit=5).get("entries") or []
                by_human = [h for h in hist if h.get("by") == "window:owner" and (seq0 is None or h.get("id", 0) > seq0)]
                d_err, diff = raw(builder, "sight_compare", action="diff", name="before_load")
                warn = json.dumps(diff.get("warnings") or [])
                check(mark_err is None and d_err is None and "world was replaced" in warn, f"a sight_compare diff across the load matches no subjects by id, and says so ({(mark_err or d_err or warn)[:200]})")
                check(not by_human and after_load["position"] == before_load["position"], f"the release writes nothing (window:owner entries since: {by_human}; crate_a {before_load['position']} -> {after_load['position']})")
        # -- probe 9 (review R2): the human's own key, pressed while an agent's editor_input
        # frames run, is the human's: no via, not the agent's call, none of its one commit. Last:
        # it deletes crate_a --
        ed.input({"key": "Escape"}, {"wait": 2})
        st = ed.settled()
        ca, _ = screen_centre(builder, "/editor_test/crate_a", st["camera"], size)
        if check(bool(ca), "crate_a is on screen for probe 9"):
            s = ed.input({"click": ca}, {"wait": 3})
            sel = [x["path"] for x in s.get("selection", [])]
            if check(sel == ["/editor_test/crate_a"], f"crate_a selected for probe 9 ({sel})"):
                res = {}
                waiter = threading.Thread(target=lambda: res.setdefault("r", raw(owner, "editor_input", steps=[{"wait": 90}, {"wait": 90}])))
                waiter.start()
                time.sleep(1.0)
                os_key(hwnd, 0x2E)  # Delete, pressed in the real window
                waiter.join()
                err9 = res["r"][0]
                check(err9 is None, f"the agent's wait frames ran ({(err9 or '')[:120]})")
                ed.settled()
                q_err, q = raw(builder, "world_query", path_prefix="/editor_test/crate_a")
                gone = q_err is not None and q.get("error") == "not_found"
                hist = call(builder, "world_history", path_prefix="/editor_test/crate_a", limit=1, response_format="detailed").get("entries") or [{}]
                e9 = hist[0]
                tc = call(owner, "trace_call", call=e9.get("call", 0)) if e9.get("call") else {}
                tool = (tc.get("call") or {}).get("tool")
                check(gone and e9.get("by") == "window:owner" and e9.get("kind") in ("batch", "delete") and not e9.get("via") and tool != "editor_input",
                      f"the human's Delete during an agent's wait frames is the human's alone: no via, not the agent's call ({json.dumps(e9)[:200]}; call tool {tool})")
        report["shots"] = ed.shots
    errors = eng.validation_errors()
    report["validation_errors"] = errors
    check(errors == 0, f"validation errors at shutdown: {errors}")
    if a.big > 0:
        report["big"] = big_world_check(a, out)
    report["fails"] = FAILS
    (out / "summary.json").write_text(json.dumps(report, indent=2))
    print(f"\n{'PASS' if not FAILS else 'FAIL'}: {len(FAILS)} failure(s); screenshots and summary in {out}")
    return 1 if FAILS else 0


if __name__ == "__main__":
    sys.exit(main())
