"""Live check of the launcher (docs/design/LAUNCH.md): the project list, opening and resuming a
project, and finding, calling and stopping a running engine from a terminal.

Everything a person or an agent with only a shell does, done with the real executable:

  specula projects | new | open | list | tools | call | stop | forget | help

- A machine that has never run a project lists none; a project worked on is listed, the last one
  first; `open --last` resumes it.
- `open` starts an engine on a project (or finds the one that has it open) and waits until it
  serves; `list` finds it; `tools` and `call` use it with no MCP client; `call --as NAME` keeps one
  identity across commands; `stop` shuts it down cleanly.
- An engine started by hand with `--project` is listed and found the same way; one started with
  `--settings off` (every script's engine) is neither.
- An engine `open` starts with a window opens behind the other windows and takes no keyboard.
- The start screen (Windows; `--launcher`, as the executable opened with no argument shows it):
  an empty list offers a new project, made and opened from the keyboard alone; the project last
  worked on is first and Enter resumes it; `specula open` hands a project to a start screen
  that is waiting, and its window becomes that project on the same port; Esc leaves a plain
  engine. The screen is drawn in the window only: no capture and no UI tool shows the list.
  Keys go to the engine's own window as posted messages (nothing here moves the pointer, presses
  a key for the person or takes the keyboard), and the pictures are of that window alone.

Nothing here touches the real per-user folder: every command and engine gets `--settings DIR`
inside the output folder (and the start screen's engines a home folder there, for new projects).
Engines are headless except the window checks, whose windows open behind the others.

Usage: cargo build -p specula --release && python scripts/launch_check.py [--exe PATH] [--port 7661]
"""
import argparse
import ctypes
import json
import os
import shutil
import struct
import subprocess
import sys
import threading
import time
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from specula_mcp import ROOT, Engine  # noqa: E402

FAILS = []


def check(cond, what):
    print(("ok   " if cond else "FAIL ") + what)
    if not cond:
        FAILS.append(what)
    return bool(cond)


class Cli:
    """The executable's commands, always with this run's own per-user folder."""

    def __init__(self, exe, user):
        self.exe, self.user = str(exe), str(user)

    def run(self, *words, timeout=240):
        """(exit code, the JSON answer or the raw text) of `specula WORDS --settings USER`. Its
        output is read through pipes, as an agent's shell reads it. A command that does not
        answer in `timeout` seconds is ended and reported (exit code None): `open` once left
        its engine holding these pipes, and the reader waited for the engine to end."""
        words = [str(w) for w in words]
        cut = words.index("--") if "--" in words else len(words)
        argv = [self.exe, *words[:cut], "--settings", self.user, *words[cut:]]
        p = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")  # "--settings" is in argv above
        try:
            stdout, stderr = p.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            p.kill()
            return None, {"error": "no_answer", "text": f"no answer in {timeout} s"}
        text = (stdout if p.returncode != 2 else stderr).strip() or (stderr or stdout).strip()
        try:
            return p.returncode, json.loads(text)
        except ValueError:
            return p.returncode, {"text": text}


def names(answer):
    return [p.get("name") for p in answer.get("projects", [])]


def running(pid):
    """Whether a process with this id exists (Windows: tasklist; elsewhere: a signal 0)."""
    if not pid:
        return False
    if sys.platform == "win32":
        out = subprocess.run(["tasklist", "/FI", f"PID eq {pid}", "/NH"], capture_output=True, text=True, encoding="utf-8", errors="replace").stdout
        return str(pid) in out
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


# ---- the engine's own window (Win32, out of process) ----

if sys.platform == "win32":
    from ctypes import wintypes
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    gdi32 = ctypes.WinDLL("gdi32")
    user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))

VK_RETURN, VK_ESCAPE = 0x0D, 0x1B


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
    """A key press and release posted to the engine's window, as a keyboard would send them."""
    user32.PostMessageW(hwnd, 0x0100, vk, 1 | (scan << 16))
    time.sleep(0.05)
    user32.PostMessageW(hwnd, 0x0101, vk, 1 | (scan << 16) | (0xC0 << 24))
    time.sleep(0.25)


def type_text(hwnd, text):
    """Characters posted to the engine's window (WM_CHAR), as typing sends them."""
    for c in text:
        user32.PostMessageW(hwnd, 0x0102, ord(c), 1)
        time.sleep(0.03)
    time.sleep(0.3)


def screenshot(hwnd, path):
    """The engine window's client area as it rendered it (PrintWindow) -> an RGB PNG. Returns
    (not blank, width, height, the pixels as BGRA bytes)."""
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
    Path(path).write_bytes(png)
    lively = ok != 0 and len({pixels.raw[i:i + 3] for i in range(0, len(pixels.raw), 4 * 97)}) > 2
    return lively, w, h, pixels.raw


def changed(a, b):
    """The share of sampled pixels that differ clearly between two pictures of one window."""
    if a[1:3] != b[1:3]:
        return 1.0
    pa, pb = a[3], b[3]
    step = 4 * 53
    n = d = 0
    for i in range(0, len(pa) - 4, step):
        n += 1
        if abs(pa[i] - pb[i]) + abs(pa[i + 1] - pb[i + 1]) + abs(pa[i + 2] - pb[i + 2]) > 48:
            d += 1
    return d / max(n, 1)


def decode_png(path):
    """An RGB8 or RGBA8 non-interlaced PNG -> (w, h, bytes per pixel, the pixels)."""
    data = Path(path).read_bytes()
    pos, idat = 8, b""
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if kind == b"IHDR":
            w, h, _, ctype, _, _, _ = struct.unpack(">IIBBBBB", body)
            bpp = 3 if ctype == 2 else 4
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    raw = zlib.decompress(idat)
    stride = w * bpp
    out, prev, i = bytearray(h * stride), bytearray(stride), 0
    for y in range(h):
        f, line = raw[i], bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + ((a + b) >> 1)) & 255
            elif f == 4:
                q = a + b - c
                qa, qb, qc = abs(q - a), abs(q - b), abs(q - c)
                line[x] = (line[x] + (a if qa <= qb and qa <= qc else (b if qb <= qc else c))) & 255
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return w, h, bpp, bytes(out)


def png_changed(a, b):
    """The share of pixels that differ clearly between two captures (0 when the files are the same)."""
    if Path(a).read_bytes() == Path(b).read_bytes():
        return 0.0
    (wa, ha, ba, pa), (wb, hb, bb, pb) = decode_png(a), decode_png(b)
    if (wa, ha, ba) != (wb, hb, bb):
        return 1.0
    d = sum(1 for i in range(0, len(pa) - 2, ba) if abs(pa[i] - pb[i]) + abs(pa[i + 1] - pb[i + 1]) + abs(pa[i + 2] - pb[i + 2]) > 24)
    return d / max(wa * ha, 1)


def has_text(shot):
    """Whether a picture of the engine's window has the start screen's ivory text in it (an
    empty world's window has no pixel that bright)."""
    px = shot[3]
    return sum(1 for i in range(0, len(px) - 4, 4 * 7) if px[i] > 170 and px[i + 1] > 190 and px[i + 2] > 190) >= 40


def wait_for(pred, timeout=20.0):
    end = time.time() + timeout
    while time.time() < end:
        v = pred()
        if v:
            return v
        time.sleep(0.1)
    return None


def start_screen(exe, out, adapter, port):
    """The start screen in a real window (the module's docstring says what is checked)."""
    user, home, shots = out / "user_start", out / "home", out / "start"
    home.mkdir()
    shots.mkdir()
    cli = Cli(exe, user)
    # New projects go under the home folder: this run's own, never the person's.
    env = dict(os.environ)
    env["USER" + "PROFILE"] = str(home)
    env["HOME"] = str(home)
    flags = ["--launcher", "--settings", str(user), "--adapter", adapter, "--size", "1100x760"]

    def engines():
        return cli.run("list")[1].get("engines") or []

    def became(old_pid, name):
        """The engine that took the start screen's place: another process, on its port, with the project."""
        return wait_for(lambda: next((e for e in engines() if e.get("pid") != old_pid and (e.get("project") or {}).get("name") == name), None), 120)

    def window_of(pid):
        hwnd = wait_for(lambda: find_window(pid), 30)
        time.sleep(1.2)  # its first frames
        return hwnd

    # ---- no project yet: the list is empty, and a project is made from the keyboard alone ----
    with Engine(exe, port, out / "start_a.log", *flags, cwd=str(out), env=env) as eng:
        pid = eng.proc.pid
        hwnd = window_of(pid)
        listed = engines()
        check(len(listed) == 1 and listed[0].get("launcher") is True and "project" not in listed[0], f"an engine opened with no project is found as a start screen ({str(listed)[:160]})")
        empty = screenshot(hwnd, shots / "01_empty_list.png")
        check(empty[0] and has_text(empty), "the start screen is drawn (01_empty_list.png)")
        m = eng.mcp("looker")
        frame = m.frame(width=480, height=270, include_ui=True)
        shutil.copy(frame["image"]["path"], shots / "01_capture_while_open.png")
        m.close()
        key(hwnd, VK_RETURN, 0x1C)  # the focus is on "New project"
        type_text(hwnd, "tomb")
        named = screenshot(hwnd, shots / "02_new_project_name.png")
        check(changed(empty, named) > 0.01, f"Enter on an empty list asks for the new project's name, and typing shows ({changed(empty, named):.3f} of the window changed)")
        key(hwnd, VK_RETURN, 0x1C)
        try:
            code = eng.proc.wait(timeout=120)
        except subprocess.TimeoutExpired:
            code = None
        check(code == 0, f"the project made, the start screen's engine ends cleanly (exit code {code})")
        e = became(pid, "tomb") or {}
        tomb = home / "Specula Projects" / "tomb"
        check(e.get("port") == port and not e.get("launcher") and Path((e.get("project") or {}).get("path", "")) == tomb, f"and the executable starts again in the new project, on the same port ({str(e)[:200]})")
        check((tomb / "specula.project.json").is_file() and (tomb / "assets").is_dir(), "the project is a folder in the home's Specula Projects, with its marker and an assets folder")
        if e:
            hwnd2 = window_of(e["pid"])
            opened = screenshot(hwnd2, shots / "03_project_open.png") if hwnd2 else None
            check(opened is not None and not has_text(opened) and changed(empty, opened) > 0.05, "its window shows the engine, with no start screen over it (03_project_open.png)")
            code, j = cli.run("call", "engine_info", "--port", port)
            check(code == 0 and (j.get("project") or {}).get("name") == "tomb", f"engine_info says the project ({j.get('project')})")
        code, j = cli.run("stop")
        check(code == 0 and j.get("stopped") is True, f"it stops when asked ({code} {j})")

    # ---- two projects: the last one worked on is first, and Enter resumes it ----
    code, j = cli.run("new", "crypt", "--in", out / "projects_start")
    check(code == 0 and names(cli.run("projects")[1]) == ["crypt", "tomb"], "a second project is made: it is the last one worked on")
    with Engine(exe, port, out / "start_b.log", *flags, cwd=str(out), env=env) as eng:
        pid = eng.proc.pid
        hwnd = window_of(pid)
        two = screenshot(hwnd, shots / "04_two_projects.png")
        check(two[0] and has_text(two) and changed(empty, two) > 0.005, "the list shows the projects (04_two_projects.png)")
        # No tool shows a person's project list: not a capture with the interface in it, not the
        # UI tools. ("tomb" is on the screen, in the list, and nowhere else in this engine.)
        m = eng.mcp("looker")
        frame = m.frame(width=480, height=270, include_ui=True)
        packet = Path(frame["image"]["packet_path"]).read_text(encoding="utf-8", errors="replace")
        said = json.dumps(frame) + packet
        for tool in ("ui_tree", "ui_check", "engine_info", "world_list"):
            try:
                said += json.dumps(m.request("tools/call", {"name": tool, "arguments": {}}))
            except Exception as e:  # a refusal is an answer too
                said += str(e)
        check("tomb" not in said and "crypt" not in said, "no capture packet and no tool answer names a listed project")
        shutil.copy(frame["image"]["path"], shots / "04_capture_while_open.png")
        m.close()
        key(hwnd, VK_RETURN, 0x1C)  # the focus is on the first project
        try:
            code = eng.proc.wait(timeout=120)
        except subprocess.TimeoutExpired:
            code = None
        e = became(pid, "crypt") or {}
        check(code == 0 and e.get("port") == port, f"Enter resumes the project last worked on ({code} {str(e)[:160]})")
        code, j = cli.run("stop")
        check(code == 0 and j.get("stopped") is True, "and it stops when asked")

    # ---- `specula open` hands a project to a start screen that is waiting ----
    with Engine(exe, port, out / "start_c.log", *flags, cwd=str(out), env=env) as eng:
        pid = eng.proc.pid
        # At once: the engine serves, its window may not have drawn a frame yet (the hand-over
        # waits for the start screen's first frame instead of being refused).
        code, j = cli.run("open", "tomb", timeout=200)
        check(code == 0 and j.get("from_start_screen") is True and j.get("port") == port and (j.get("project") or {}).get("name") == "tomb" and j.get("pid") != pid,
              f"specula open hands the project to the waiting start screen: its window becomes the project, on its port ({code} {str(j)[:200]})")
        time.sleep(1.5)
        owner = wintypes.DWORD(0)
        user32.GetWindowThreadProcessId(user32.GetForegroundWindow(), ctypes.byref(owner))
        check(find_window(j.get("pid") or 0) is not None and owner.value != j.get("pid"), f"and that window opened behind the others: a command never takes the keyboard (the foreground window is process {owner.value}'s)")
        try:
            code = eng.proc.wait(timeout=60)
        except subprocess.TimeoutExpired:
            code = None
        check(code == 0 and len(engines()) == 1, f"one engine serves it, and the start screen's ended cleanly ({code})")
        check(names(cli.run("projects")[1]) == ["tomb", "crypt"], "the project opened is first in the list now")
        code, j = cli.run("stop")
        check(code == 0 and j.get("stopped") is True, "and it stops when asked")

    # ---- Esc: a plain engine, with no project ----
    with Engine(exe, port, out / "start_d.log", *flags, cwd=str(out), env=env) as eng:
        pid = eng.proc.pid
        hwnd = window_of(pid)
        # Something in the world, so the captures below show a scene (a capture of an empty
        # world would prove only that the start screen is absent from it).
        m = eng.mcp("builder")
        made = m.json("world_spawn", shape="cube", path="/a", color="#d08040")
        time.sleep(0.6)
        before = screenshot(hwnd, shots / "05_before_close.png")
        shown = m.frame(width=480, height=270, include_ui=True, time_s=0)
        shutil.copy(shown["image"]["path"], shots / "05_capture_while_open.png")
        key(hwnd, VK_ESCAPE, 0x01)
        time.sleep(0.5)
        after = screenshot(hwnd, shots / "06_closed.png")
        check(has_text(before) and not has_text(after) and changed(before, after) > 0.05, f"Esc closes the start screen ({changed(before, after):.3f} of the window changed; 06_closed.png)")
        check(eng.proc.poll() is None, "and the engine runs on, with no project")
        # A capture never showed the start screen: the one taken while it was open is the same
        # picture of the scene as one taken now.
        hidden = m.frame(width=480, height=270, include_ui=True, time_s=0)
        shutil.copy(hidden["image"]["path"], shots / "06_capture_closed.png")
        differ = png_changed(shown["image"]["path"], hidden["image"]["path"])
        w, h, bpp, px = decode_png(shown["image"]["path"])
        colours = len({px[i:i + 3] for i in range(0, len(px) - 2, bpp * 7)})
        check(differ < 0.002 and colours >= 3, f"a capture taken while the start screen was open shows the scene and no start screen: the same picture as one taken with it closed ({differ:.4f} differ; {colours} colours)")
        info = m.json("engine_info")
        check(info.get("project") is None and made.get("path") == "/a", "it is an engine like any other")
        m.close()
        code, j = cli.run("stop")
        # (No wait: the command answers when the process has ended, not when it began to.)
        ended = eng.proc.poll()
        check(code == 0 and j.get("stopped") is True and ended == 0, f"specula stop ends it cleanly, and answers once it has ended ({code} {j}, exit code {ended})")
    return cli


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--exe", default=str(Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target") / "release" / ("specula.exe" if sys.platform == "win32" else "specula")))
    ap.add_argument("--port", type=int, default=7661, help="the first of four ports this check uses")
    ap.add_argument("--adapter", default="integrated")
    ap.add_argument("--out")
    a = ap.parse_args()
    exe = Path(a.exe)
    if not exe.exists():
        sys.exit(f"{exe} not found: cargo build -p specula --release")
    out = Path(a.out) if a.out else ROOT / "target" / "launch-check"
    if out.exists():
        shutil.rmtree(out, ignore_errors=True)
    (out / "projects").mkdir(parents=True)
    user = out / "user"
    cli = Cli(exe, user)
    p1, p2, p3, p4 = a.port, a.port + 1, a.port + 2, a.port + 3
    headless = ["--", "--headless", "--adapter", a.adapter]
    report = {}
    # Every per-user folder of this run (the start screen's section has its own): whatever
    # happens, no engine listed in one stays behind.
    others = []
    try:
        # ---- a machine that has never run a project ----
        code, j = cli.run("projects")
        check(code == 0 and j == {"projects": [], "count": 0}, f"no project before: an empty list ({code} {j})")
        code, j = cli.run("list")
        check(code == 0 and j == {"engines": [], "count": 0}, f"no engine running: an empty list ({code} {j})")
        code, j = cli.run("open", "--last")
        check(code == 1 and j.get("error") == "no_projects", f"nothing to resume says so, with what to do ({code} {str(j)[:160]})")
        code, j = cli.run("help")
        text = j.get("text", "")
        check(code == 0 and all(f"specula {w}" in text for w in ("projects", "open", "new", "forget", "list", "stop", "tools", "call")), "help names every command")
        code, j = cli.run("opne", "crypt")
        check(code == 1 and j.get("error") == "usage" and "is not a command" in j.get("message", ""), f"a word that is no command is said, as JSON, never run as an engine ({code} {str(j)[:120]})")
        for words in (("open",), ("call", "world_list", "[1]"), ("stop", "extra")):
            code, j = cli.run(*words)
            check(code == 1 and j.get("error") == "usage" and j.get("message"), f"'specula {' '.join(words)}' is answered as JSON too ({code} {str(j)[:100]})")

        # ---- new projects, listed most recent first ----
        code, j = cli.run("new", "crypt", "--in", out / "projects")
        crypt = Path((j.get("project") or {}).get("path", ""))
        check(code == 0 and crypt == out / "projects" / "crypt" and (crypt / "specula.project.json").is_file() and (crypt / "assets").is_dir(),
              f"new makes the project's folder, its marker and its assets folder ({code} {str(j)[:160]})")
        code, j = cli.run("new", "frost", "--in", out / "projects")
        frost = Path((j.get("project") or {}).get("path", ""))
        code, j = cli.run("projects")
        check(names(j) == ["frost", "crypt"] and j.get("last") == "frost", f"the projects are listed, the last one worked on first ({names(j)})")
        code, j = cli.run("new", "crypt", "--in", out / "projects")
        check(code == 1 and "not empty" in str(j), "a folder in use is never taken for a new project")

        # ---- open: an engine on the project, found, used and stopped from the terminal ----
        began = time.time()
        code, j = cli.run("open", "crypt", *headless, "--port", p1, timeout=200)
        pid1 = j.get("pid")
        check(code is not None, f"open answers and ends while its engine runs on: its output can be read through a pipe ({time.time() - began:.0f} s)")
        check(code == 0 and j.get("port") == p1 and j.get("started") is True and (j.get("project") or {}).get("name") == "crypt" and j.get("window") is False,
              f"open starts an engine on the project and answers once it serves ({code} {str(j)[:200]})")
        report["open"] = j
        code, j = cli.run("list")
        check(code == 0 and j.get("count") == 1 and j["engines"][0].get("port") == p1 and j["engines"][0].get("url") == f"http://127.0.0.1:{p1}/mcp", f"list finds it, with where to reach it ({str(j)[:200]})")
        code, j = cli.run("projects")
        first = (j.get("projects") or [{}])[0]
        check(names(j) == ["crypt", "frost"] and (first.get("running") or {}).get("port") == p1, f"the project opened is first now, and says its engine is running ({names(j)}, {first.get('running')})")
        code, j = cli.run("open", "crypt")
        check(code == 0 and j.get("started") is False and j.get("pid") == pid1, f"open again finds the engine that has it open, and starts none ({code} {j.get('started')} {j.get('pid')})")
        code, j = cli.run("tools", "terrain", "--project", "crypt")
        listed = [t.get("name") for t in j.get("tools", [])]
        check(code == 0 and "terrain_edit" in listed and j.get("port") == p1, f"tools lists what the engine can do, filtered ({listed[:6]})")
        code, j = cli.run("call", "engine_info", "--project", "crypt")
        proj = j.get("project") or {}
        check(code == 0 and proj.get("name") == "crypt" and Path(proj.get("path", "")) == crypt, f"call runs a tool with no MCP client; engine_info says the project ({proj})")
        code, j = cli.run("call", "world_spawn", json.dumps({"shape": "cube", "path": "/a"}), "--as", "ann", "--port", p1)
        check(code == 0 and "error" not in j, f"call --as ann spawns as ann ({code} {str(j)[:120]})")
        code, j = cli.run("call", "world_spawn", json.dumps({"shape": "cube", "path": "/b"}), "--as", "ann", "--port", p1)
        check(code == 0 and "error" not in j, f"a second command as ann is the same ann (its name is not taken) ({code} {str(j)[:160]})")
        code, j = cli.run("call", "world_history", json.dumps({"limit": 4}), "--port", p1)
        who = [e.get("by") for e in j.get("entries", [])]
        check(code == 0 and who[:2] == ["ann", "ann"], f"both edits are journaled by ann ({who})")
        for _ in range(5):
            cli.run("call", "world_list", "--as", "ann", "--port", p1)
        code, j = cli.run("call", "engine_info", json.dumps({"response_format": "detailed"}), "--port", p1)
        left = sum(1 for agent in j.get("agents", []) if agent.get("client") == "specula-cli")
        check(code == 0 and left <= 1, f"a command's session ends with the command, named or not: none piles up in the engine ({left} of the commands' sessions in engine_info.agents)")
        code, j = cli.run("call", "world_spawn", json.dumps({"shape": "cube", "path": "/c"}), "--as", "ann", "--port", p1)
        code, j = cli.run("call", "world_history", json.dumps({"limit": 1}), "--port", p1)
        check([e.get("by") for e in j.get("entries", [])] == ["ann"], "and ann is still ann after her sessions closed")
        code, j = cli.run("call", "no_such_tool", "--port", p1)
        check(code == 1, f"a tool's refusal is exit code 1 ({code} {str(j)[:100]})")
        code, j = cli.run("call", "world_list", "--port", p4)
        check(code == 2 and j.get("error") == "no_engine", f"no engine on a port is exit code 2, and says so ({code} {str(j)[:140]})")

        # ---- two engines: a command says which ----
        code, j = cli.run("open", "frost", *headless, "--port", p2)
        check(code == 0 and j.get("started") is True and j.get("port") == p2, f"a second project opens in an engine of its own ({code} {str(j)[:120]})")
        code, j = cli.run("tools")
        check(code == 1 and j.get("error") == "several_engines" and len(j.get("engines", [])) == 2, f"with two engines a command asks which ({code} {j.get('error')})")
        code, j = cli.run("projects")
        check(names(j) == ["frost", "crypt"], f"the last one opened is first ({names(j)})")
        code, j = cli.run("stop", "--project", "frost")
        check(code == 0 and j.get("stopped") is True and j.get("port") == p2, f"stop shuts the project's engine down ({code} {j})")
        check(not running(j.get("pid")), "and answers once the engine's process has ended (not only stopped serving)")
        code, j = cli.run("list")
        check(j.get("count") == 1 and j["engines"][0].get("port") == p1, "it is no longer listed, and the other one is")
        check(not list((user / "engine.instances").glob("*.stop")), "a stop request leaves nothing behind")
        code, j = cli.run("stop")
        check(code == 0 and j.get("stopped") is True and j.get("pid") == pid1, f"stop with one engine running needs no name ({code} {j})")
        code, j = cli.run("list")
        check(j == {"engines": [], "count": 0} and not list((user / "engine.instances").glob("*.json")), f"no engine is left, and no instance file ({j})")

        # ---- resume the last one ----
        code, j = cli.run("open", "--last", *headless, "--port", p1)
        check(code == 0 and (j.get("project") or {}).get("name") == "frost", f"open --last resumes the project last worked on ({(j.get('project') or {}).get('name')})")
        cli.run("stop")

        # ---- one engine a project: two commands at once start one engine ----
        crypt_folder = out / "projects" / "crypt"
        answers = []
        openers = [threading.Thread(target=lambda port=port: answers.append(cli.run("open", "crypt", *headless, "--port", port))) for port in (p1, p2)]
        for t in openers:
            t.start()
        for t in openers:
            t.join()
        started = sorted(bool(j.get("started")) for _, j in answers)
        pids = {j.get("pid") for _, j in answers}
        check(all(code == 0 for code, _ in answers) and started == [False, True] and len(pids) == 1,
              f"two 'specula open' of one project at once: one starts the engine, the other is answered with it ({[(c, j.get('started'), j.get('pid'), j.get('error')) for c, j in answers]})")
        listed = cli.run("list")[1]
        check(listed.get("count") == 1, f"and one engine has the project ({listed.get('count')})")
        # By hand: a second engine on the project is refused, and says why.
        second = subprocess.run([str(exe), "--project", str(crypt_folder), "--settings", str(user), "--headless", "--adapter", a.adapter, "--port", str(p3)], capture_output=True, text=True, timeout=180, encoding="utf-8", errors="replace")
        check(second.returncode != 0 and "is open in another engine" in second.stderr, f"an engine started by hand on a project that is open is refused ({second.returncode} {second.stderr.strip()[-160:]})")

        # ---- an engine that died: its file is not an engine, whoever has its port now ----
        dead = (listed.get("engines") or [{}])[0]
        dead_pid, dead_port = dead.get("pid"), dead.get("port")
        if sys.platform == "win32" and dead_pid:
            subprocess.run(["taskkill", "/PID", str(dead_pid), "/F"], capture_output=True)
            wait_for(lambda: not running(dead_pid), 30)
            check((user / "engine.instances" / f"{dead_pid}.json").is_file(), "an engine that is killed leaves its instance file behind")
            with Engine(exe, dead_port, out / "engine_other.log", "--headless", "--adapter", a.adapter, cwd=str(out)):
                check(cli.run("list")[1].get("count") == 0, "another program on its port is not taken for it")
                began = time.time()
                code, j = cli.run("stop", "--project", "crypt")
                check(code == 2 and j.get("error") in ("no_engine", "not_running") and time.time() - began < 30, f"and no command is sent to that program in its name ({code} {j.get('error')} in {time.time() - began:.0f} s)")
                code, j = cli.run("call", "engine_info", "--project", "crypt")
                check(code == 2 and j.get("error") in ("no_engine", "not_running"), f"a call for the project reaches nobody ({code} {str(j)[:100]})")
            check(not (user / "engine.instances" / f"{dead_pid}.json").exists(), "and its file is gone")
        else:
            cli.run("stop")

        # ---- a relative --settings is the folder the command was run in, not one in the project ----
        rel = subprocess.Popen([str(exe), "open", str(crypt_folder), "--settings", "user_rel", "--", "--headless", "--adapter", a.adapter, "--port", str(p2)], cwd=str(out), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")
        try:
            said, _ = rel.communicate(timeout=240)
        except subprocess.TimeoutExpired:
            rel.kill()
            said = "{}"
        try:
            j = json.loads(said)
        except ValueError:
            j = {"text": said}
        check(rel.returncode == 0 and j.get("port") == p2 and (out / "user_rel" / "projects.json").is_file() and not (crypt_folder / "user_rel").exists(),
              f"a relative --settings names one folder for the command and the engine it starts ({rel.returncode} {str(j)[:120]})")
        others.append(Cli(exe, out / "user_rel"))
        code, j = others[-1].run("stop")
        check(code == 0 and j.get("stopped") is True, "and that engine is found there and stops")

        # ---- --port 0: the port the engine was given is the one it says ----
        code, j = cli.run("open", "crypt", *headless, "--port", 0)
        check(code == 0 and j.get("port") not in (0, None) and cli.run("list")[1].get("count") == 1, f"an engine asked for any port is found on the one it got ({code} {j.get('port')})")
        cli.run("stop")

        # ---- an engine started by hand, and a script's engine ----
        plain = out / "projects" / "plain"
        plain.mkdir()
        with Engine(exe, p3, out / "engine_hand.log", "--headless", "--adapter", a.adapter, "--project", str(plain), "--settings", str(user), cwd=str(out)) as eng:
            m = eng.mcp("hand")
            info = m.json("engine_info")
            check((info.get("project") or {}).get("name") == "plain" and (plain / "specula.project.json").is_file(), f"--project opens a plain folder as a project and gives it its marker ({info.get('project')})")
            code, j = cli.run("list")
            check(j.get("count") == 1 and (j["engines"][0].get("project") or {}).get("name") == "plain", "an engine started by hand is found the same way")
            code, j = cli.run("projects")
            check(names(j)[:1] == ["plain"], f"and its project is first in the list ({names(j)})")
            m.close()
        code, j = cli.run("list")
        check(j.get("count") == 0, "it takes its instance file with it when it stops")
        with Engine(exe, p3, out / "engine_script.log", "--headless", "--adapter", a.adapter, cwd=str(out)) as eng:
            eng.mcp("script").close()
            code, j = cli.run("list")
            check(j.get("count") == 0, "an engine started with --settings off (every script's) is not listed")
            code, j = cli.run("stop", "--port", p3)
            check(code == 1 and j.get("error") == "not_listed", f"and stop says it is stopped where it was started ({code} {j.get('error')})")

        # ---- a project whose folder is gone ----
        shutil.rmtree(frost, ignore_errors=True)
        code, j = cli.run("projects")
        gone = next((p for p in j.get("projects", []) if p.get("name") == "frost"), {})
        check(gone.get("missing") is True, "a project whose folder is gone stays listed, as missing")
        code, j = cli.run("open", "frost")
        check(code == 1 and j.get("error") == "missing", f"opening it says so ({code} {j.get('error')})")
        code, j = cli.run("forget", "frost")
        code2, j2 = cli.run("projects")
        check(code == 0 and "frost" not in names(j2), f"forget takes it off the list ({names(j2)})")

        # ---- a window: behind the others, taking no keyboard ----
        if sys.platform == "win32":
            import ctypes
            code, j = cli.run("open", "crypt", "--", "--adapter", a.adapter, "--port", p1, "--size", "640x400")
            pid = j.get("pid")
            check(code == 0 and j.get("window") is True, f"open with no --headless starts an engine with a window ({code} {str(j)[:140]})")
            time.sleep(1.0)
            front = ctypes.windll.user32.GetForegroundWindow()
            owner = ctypes.c_ulong(0)
            ctypes.windll.user32.GetWindowThreadProcessId(ctypes.c_void_p(front), ctypes.byref(owner))
            check(owner.value != pid, f"its window opened behind the others and took no keyboard (the foreground window is another process's: {owner.value} vs {pid})")
            code, j = cli.run("stop")
            check(code == 0 and j.get("stopped") is True and not running(pid), f"and it stops when asked: its process is gone when the command answers ({code} {j}, still running: {running(pid)})")
            # ---- the start screen ----
            others.append(start_screen(exe, out, a.adapter, p1))
    finally:
        # Whatever happened above, no engine of this run stays behind: asked to stop, then (by
        # its own process id, read from its instance file) ended.
        for c in [cli, *others, Cli(exe, out / "user_start")]:
            for _ in range(4):
                engines = c.run("list")[1].get("engines") or []
                if not engines:
                    break
                c.run("stop", "--port", engines[0].get("port"), timeout=120)
            for e in c.run("list")[1].get("engines") or []:
                if sys.platform == "win32" and e.get("pid"):
                    subprocess.run(["taskkill", "/PID", str(e["pid"]), "/F"], capture_output=True)
    (out / "report.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    print(f"\nreport: {out / 'report.json'}")
    if FAILS:
        print(f"FAIL: {len(FAILS)} check(s) failed")
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
