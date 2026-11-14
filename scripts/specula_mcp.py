"""Shared helpers for scripts that drive Specula: a minimal MCP client and an engine launcher.

Standard library only. Keep output ASCII (Python's -I flag ignores PYTHONIOENCODING).
"""
import http.client
import json
import os
import signal
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class Connection(http.client.HTTPConnection):
    """An HTTP connection to an engine that connects first and sets its timeout after.

    Python's connect with a timeout, on Windows, often takes 10 to 26 ms where a connect takes
    0.1 ms, with nothing wrong at the other end (measured 2026-11-15, Python 3.13: on an empty
    headless engine 42 of 300 calls through urllib's own urlopen with a timeout of 180 s took
    over 10 ms, none of 300 with no timeout, none over a raw socket, none on a connection kept
    open; a connect and close alone, 93 of 300; the same against a listener in the Python process
    itself, at the system timer's default resolution and at 1 ms; Rust's `connect_timeout` does
    not show it). Why Python's does was not found. Every script made a connection a call that
    way, so about one call in seven waited, a latency a check then read as the engine's (M5's
    final audit: `entity_refs_check` timed a `note_list` at 21.9 ms that the engine's own record
    of the call put at 0.1 ms). The timeout still holds for everything after the connect: the
    send and the answer. A connect to a port of this machine with nothing listening is refused by
    the system (about 2 s on Windows)."""

    def connect(self):
        self.sock = socket.create_connection((self.host, self.port))
        self.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.sock.settimeout(self.timeout)


class _Handler(urllib.request.HTTPHandler):
    def http_open(self, req):
        return self.do_open(Connection, req)


_OPENER = urllib.request.build_opener(_Handler)


def urlopen(req, timeout):
    """`urllib.request.urlopen` for a request to an engine: the same answers and the same errors,
    over a `Connection`. Scripts call this one (`script_settings_guard.rs` holds it)."""
    return _OPENER.open(req, timeout=timeout)


def connect(port, timeout):
    """A socket connected to an engine's port, with `timeout` set after the connect."""
    s = socket.create_connection(("127.0.0.1", port))
    s.settimeout(timeout)
    return s


def poll(req):
    """One short try at a port that may have nothing listening yet, for loops that wait for an
    engine to come up. The one place a connect carries a timeout: a closed port on Windows is not
    refused for about 2 s, and how long a poll takes is not a latency anyone reads."""
    return urllib.request.urlopen(req, timeout=1)


def _physical_window_pixels():
    """This process speaks to windows in physical pixels (per-monitor DPI aware).

    Without it, on a display scaled by the OS (125%, 175%), Windows gives a script scaled-down
    window sizes and rescales the mouse messages it posts: a click posted at a widget's rect (which
    the engine reports in the window's real pixels) lands somewhere else, and only on machines
    whose display is scaled. Every script that touches a window imports this module, so it is set
    here once (a second call by a script that sets it itself fails harmlessly)."""
    if sys.platform != "win32":
        return
    try:
        import ctypes
        ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))  # PER_MONITOR_AWARE_V2
    except (AttributeError, OSError):
        pass


_physical_window_pixels()


_POINTER_NOTED = []


def clear_of_pointer(hwnd, margin=8):
    """Gets the engine's window out from under the human's mouse pointer WITHOUT moving the
    pointer: the window is moved, to the corner of its own monitor farthest from the pointer.

    A pointer over the engine's window mixes into what a script injects there (a real mouse-move
    between an injected press and release drags a selection box). Scripts used to move the
    pointer away. Someone may be using this machine: their pointer is theirs, and a script never
    moves it (owner, 2026-11-13: "I can see the agent moving my cursor around while I'm reading").
    The window is the script's own, so that is what moves; it is not activated, resized or
    raised. Returns whether the pointer is clear of the window (False: no corner of the monitor
    keeps the window away from it; said once, and the run goes on)."""
    if sys.platform != "win32" or not hwnd:
        return True
    import ctypes
    from ctypes import wintypes
    u = ctypes.WinDLL("user32")
    u.GetWindowRect.argtypes = [ctypes.c_void_p, ctypes.POINTER(wintypes.RECT)]
    u.MonitorFromWindow.restype = ctypes.c_void_p
    u.MonitorFromWindow.argtypes = [ctypes.c_void_p, wintypes.DWORD]
    u.GetMonitorInfoW.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    u.SetWindowPos.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_uint]
    r, at = wintypes.RECT(), wintypes.POINT()
    if not u.GetWindowRect(hwnd, ctypes.byref(r)) or not u.GetCursorPos(ctypes.byref(at)):
        return True
    w, h = r.right - r.left, r.bottom - r.top

    def under(left, top):
        return left - margin <= at.x <= left + w + margin and top - margin <= at.y <= top + h + margin

    if not under(r.left, r.top):
        return True

    class MonitorInfo(ctypes.Structure):
        _fields_ = [("cbSize", wintypes.DWORD), ("rcMonitor", wintypes.RECT), ("rcWork", wintypes.RECT), ("dwFlags", wintypes.DWORD)]

    info = MonitorInfo()
    info.cbSize = ctypes.sizeof(info)
    monitor = u.MonitorFromWindow(hwnd, 2)  # MONITOR_DEFAULTTONEAREST: stay on its own display (and its scale)
    free = []
    if monitor and u.GetMonitorInfoW(monitor, ctypes.byref(info)):
        area = info.rcWork
        corners = [(area.left, area.top), (area.right - w, area.top), (area.left, area.bottom - h), (area.right - w, area.bottom - h)]
        free = [c for c in corners if not under(*c)]
    if not free:
        if not _POINTER_NOTED:
            _POINTER_NOTED.append(True)
            print("note: the mouse pointer is over the engine's window and no corner of its monitor keeps the window clear of it; "
                  "this script never moves the pointer, so a pointer moved over the window may disturb what it injects")
        return False
    left, top = max(free, key=lambda c: (c[0] + w / 2 - at.x) ** 2 + (c[1] + h / 2 - at.y) ** 2)
    u.SetWindowPos(hwnd, None, int(left), int(top), 0, 0, 0x0001 | 0x0004 | 0x0010)  # NOSIZE | NOZORDER | NOACTIVATE
    time.sleep(0.15)
    return True


# Tools that answer concisely by default (`response_format`, see docs/design/TOOLS.md). Scripts
# check engine data in depth, so this client asks them for detailed answers unless a call passes
# its own response_format (or the client is made with detailed=False, as an agent would see them).
# A script with its own MCP client wraps its arguments in `detailed(tool, args)`.
DETAILED_BY_DEFAULT = {"sight_frame", "perf_frame", "engine_info", "world_list", "world_query", "world_history", "play_state",
                       "logic_status", "playtest_run", "playtest_report"}


def detailed(tool, args):
    """`args` asking `tool` for its detailed answer (when it has a format and the call doesn't choose)."""
    if tool in DETAILED_BY_DEFAULT and "response_format" not in (args or {}):
        return dict(args or {}, response_format="detailed")
    return args


def reply_of(body):
    """The JSON-RPC message of a POST's answer body: plain JSON, or an event stream (the engine
    answers as one when it tells the session notifications/tools/list_changed first; the response
    is the event with an id). None for an empty body."""
    if not body:
        return None
    text = body.decode() if isinstance(body, bytes) else body
    if not text.lstrip().startswith(("event:", "data:", "id:", ":")):
        return json.loads(text)
    messages = [json.loads(line[5:].strip()) for line in text.splitlines() if line.startswith("data:") and line[5:].strip()]
    return next((m for m in reversed(messages) if "id" in m), messages[-1] if messages else None)


class Mcp:
    """Minimal MCP client over Streamable HTTP (JSON responses)."""

    def __init__(self, url, client_name, detailed=True, tools="all"):
        # Scripts list every tool (`?tools=all`): the engine's default for a client that says
        # nothing is the core tools only (docs/design/TOOLS.md, "Scoped tool lists"). Pass
        # tools=None to connect as an unconfigured agent would, or a URL with its own query.
        if tools and "?" not in url:
            url = f"{url}?tools={tools}"
        self.url, self.session, self.next_id = url, None, 1
        self.detailed = detailed
        self.request("initialize", {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": {"name": client_name, "version": "0"},
        })
        self.post({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def post(self, msg):
        headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        req = urllib.request.Request(self.url, json.dumps(msg).encode(), headers)
        # (Longer than the engine may hold a call: up to 120 s for its start, startgate.rs, and
        # then the call's own 30 s. A shorter wait here read as a socket timeout what the
        # engine was about to answer.)
        with urlopen(req, timeout=180) as r:
            self.session = self.session or r.headers.get("Mcp-Session-Id")
            body = r.read()
        return reply_of(body)

    def request(self, method, params):
        reply = self.post({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        self.next_id += 1
        if "error" in reply:
            raise RuntimeError(f"{method}: {reply['error']}")
        return reply["result"]

    def call(self, tool, **args):
        if self.detailed:
            args = detailed(tool, args)
        result = self.request("tools/call", {"name": tool, "arguments": args})
        if result.get("isError"):
            raise RuntimeError(f"{tool}: {result['content'][0]['text']}")
        return result

    def text(self, tool, **args):
        """The first text content of a tool call."""
        result = self.call(tool, **args)
        return next(c["text"] for c in result["content"] if c["type"] == "text")

    def json(self, tool, **args):
        return json.loads(self.text(tool, **args))

    def frame(self, **args):
        return self.json("sight_frame", **args)

    def hello(self, name, role="builder", token_file=None):
        """agent_hello that survives script restarts: with `token_file`, the resume token from the
        first run is stored there and later runs resume the same identity (claims and undo come
        along) instead of being refused as name_taken."""
        token = None
        if token_file and os.path.exists(token_file):
            token = open(token_file, encoding="utf-8").read().strip() or None
        # (No retry on engine_busy: a call sent while the engine is still starting waits for the
        # start, crates/specula/src/startgate.rs. A busy answer here is a finding.)
        try:
            reply = self.json("agent_hello", name=name, role=role, **({"resume_token": token} if token else {}))
        except RuntimeError as e:
            if not (token and "bad_resume_token" in str(e)):
                raise
            # A new engine run: start a new identity.
            reply = self.json("agent_hello", name=name, role=role)
        if token_file and reply.get("resume_token"):
            with open(token_file, "w", encoding="utf-8") as f:
                f.write(reply["resume_token"])
        return reply

    def close(self):
        """Ends the MCP session (DELETE), so its name is free at once. Claims stay with the
        identity while another session carries it (see hello's token_file)."""
        if not self.session:
            return
        req = urllib.request.Request(self.url, method="DELETE", headers={"Mcp-Session-Id": self.session})
        try:
            urlopen(req, timeout=10).read()
        except OSError:
            pass
        self.session = None

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
        return False

    def tools(self):
        return [t["name"] for t in self.request("tools/list", {})["tools"]]


def share_file(mcp, path):
    """Copies a file (another engine's desync trace, say) into the captures folder of the engine
    `mcp` talks to, and returns the name to pass it: engines read files agents name only under
    their own folders (captures, worlds, assets), relative to them."""
    import shutil
    captures = Path(mcp.json("engine_info", response_format="detailed")["captures"])
    captures.mkdir(parents=True, exist_ok=True)
    src = Path(path)
    if src.resolve().parent == captures.resolve():
        return src.name
    name = f"shared_{src.name}"
    shutil.copyfile(src, captures / name)
    return name


def url_for(port):
    return f"http://127.0.0.1:{port}/mcp"


def server_up(port):
    try:
        poll(urllib.request.Request(url_for(port), b"{}", {"Content-Type": "application/json"}))
    except urllib.error.HTTPError:
        return True  # it answered (with an error for the empty request): it's up
    except OSError:
        return False
    return True


class Engine:
    """Starts `specula` on its own port and stops it cleanly (Ctrl+Break, like a user's Ctrl+C).

    Use as a context manager. `startup_s` is the time from launch until MCP answered;
    `shutdown_log` holds the engine's stderr after it stopped.
    """

    def __init__(self, exe, port, log_path, *args, cwd=None, env=None):
        self.exe, self.port, self.log_path, self.args, self.cwd = Path(exe), port, Path(log_path), list(args), cwd
        # A hosted game keeps no settings unless the script names a folder (--settings DIR). With
        # a window and no flag, the game would read the real per-user settings of whoever runs
        # the script (their saved reduced motion would skip an intro a check waits for), and the
        # keys and clicks a script posts to the window count as the player's and would be
        # written back into that file. (SPECULA_NO_SETTINGS_FLAG=1: for an older engine build
        # that doesn't know the flag, in an A/B run.)
        engine_args = self.args[: self.args.index("--")] if "--" in self.args else self.args
        if "--settings" not in engine_args and not os.environ.get("SPECULA_NO_SETTINGS_FLAG"):
            self.args[0:0] = ["--settings", "off"]
        # A script's engine opens its window without taking the keyboard from whoever is using
        # this machine (owner, 2026-11-13). Headless engines have no window. SPECULA_ACTIVATE=1
        # lets a window take the keyboard as a person's own launch does (and is for an older
        # engine build that doesn't know the flag).
        if "--no-activate" not in engine_args and "--headless" not in engine_args and not os.environ.get("SPECULA_ACTIVATE"):
            self.args[0:0] = ["--no-activate"]
        # The engine's environment (default: this process's).
        self.env = env
        self.proc, self.startup_s, self.shutdown_log, self.exit_code = None, None, "", None

    def __enter__(self):
        if server_up(self.port):
            raise RuntimeError(f"port {self.port} is already in use; pick another with --port")
        self.log_path.parent.mkdir(parents=True, exist_ok=True)
        self.log = open(self.log_path, "w", encoding="utf-8")
        flags = subprocess.CREATE_NEW_PROCESS_GROUP if sys.platform == "win32" else 0
        start = time.perf_counter()
        self.proc = subprocess.Popen([str(self.exe), "--port", str(self.port), *self.args], stderr=self.log,
                                     stdout=subprocess.DEVNULL, creationflags=flags, cwd=self.cwd, env=self.env)
        # The process exists: a subclass may note it (before it answers, so a kill during start-up
        # can't hide it).
        self.started_process()
        while not server_up(self.port):
            if self.proc.poll() is not None:
                self.log.close()
                raise RuntimeError(f"engine exited early:\n{self.log_path.read_text(encoding='utf-8')}")
            if time.perf_counter() - start > 60:
                # Don't leave it running: __exit__ won't run for a failed __enter__.
                self.proc.kill()
                self.proc.wait()
                self.log.close()
                raise RuntimeError("engine's MCP server never came up")
            time.sleep(0.05)
        self.startup_s = time.perf_counter() - start
        return self

    def started_process(self):
        """Called right after the engine's process starts (self.proc), before it answers."""

    def mcp(self, client_name, detailed=True):
        return Mcp(url_for(self.port), client_name, detailed)

    def stop(self):
        if self.proc is None or self.proc.poll() is not None:
            return
        if sys.platform == "win32":
            os.kill(self.proc.pid, signal.CTRL_BREAK_EVENT)
        else:
            self.proc.send_signal(signal.SIGINT)
        try:
            self.exit_code = self.proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.exit_code = self.proc.wait()

    def __exit__(self, *exc):
        self.stop()
        self.log.close()
        self.shutdown_log = self.log_path.read_text(errors="replace", encoding="utf-8")
        return False

    def validation_errors(self):
        """From the engine's last line: `shut down (N validation errors)`; None if missing."""
        for line in reversed(self.shutdown_log.splitlines()):
            if "shut down (" in line:
                return int(line.split("shut down (")[1].split()[0])
        return None
